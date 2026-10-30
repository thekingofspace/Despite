use std::collections::HashMap;
use std::sync::Arc;

use rayon::prelude::*;

use super::effects::{self, EffectHook};
use super::model::{NodeKind, Rig, SpriteFrames};
use super::pose::{Pose, Solver};
use crate::blend::{self, BlendMode};
use crate::color::Color;
use crate::export::Frame;
use crate::image::ImageId;
use crate::project::Project;
use crate::raster::Raster;
use crate::transform::Affine;

#[derive(Clone, Default)]
pub struct SpriteCache {
    Map: HashMap<(ImageId, i32), Arc<Raster>>,
    LayerCounts: HashMap<ImageId, usize>,
}

impl SpriteCache {
    pub fn build(Owner: &Project) -> SpriteCache {
        let mut Cache = SpriteCache::default();
        for Entry in &Owner.Rig.Nodes {
            let NodeKind::Sprite { Source: Some(Id), Frames, .. } = &Entry.Kind else { continue };
            let Some(Found) = Owner.image(*Id) else { continue };
            Cache.Map.entry((*Id, -1)).or_insert_with(|| Arc::new(Found.composite()));
            if *Frames == SpriteFrames::Layers && !Cache.LayerCounts.contains_key(Id) {
                for (Index, Layer) in Found.Layers.iter().enumerate() {
                    Cache.Map.insert((*Id, Index as i32), Arc::new(Layer.rasterize_in_image_space(Found.Width, Found.Height, Found.smooth_sampling())));
                }
                Cache.LayerCounts.insert(*Id, Found.Layers.len());
            }
        }
        Cache
    }

    pub fn get(&self, Image: ImageId, Frames: SpriteFrames, Frame: i32) -> Option<&Arc<Raster>> {
        match (Frames, self.LayerCounts.get(&Image)) {
            (SpriteFrames::Layers, Some(Count)) if *Count > 0 => self.Map.get(&(Image, Frame.rem_euclid(*Count as i32))),
            _ => self.Map.get(&(Image, -1)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RenderSettings {
    pub Smooth: bool,
    pub Background: Option<Color>,
}

impl RenderSettings {
    pub fn for_rig(Rig: &Rig) -> RenderSettings {
        RenderSettings { Smooth: !Rig.PixelArt, Background: Rig.Background }
    }
}

pub fn reborrow<'s>(Hook: &'s mut Option<&mut dyn EffectHook>) -> Option<&'s mut dyn EffectHook> {
    match Hook {
        Some(Found) => Some(&mut **Found),
        None => None,
    }
}

pub fn paint_sprite(Target: &mut Raster, Source: &Raster, Matrix: &Affine, Opacity: f32, Tint: Color, Mode: BlendMode, Smooth: bool) {
    if Opacity <= 0.0 || Tint.A <= 0.0 {
        return;
    }
    let Some(Inverse) = Matrix.inverse() else { return };
    let Bounds = Matrix.bounds_of(Source.width() as f32, Source.height() as f32).expand(1).intersect(&Target.bounds());
    if Bounds.is_empty() {
        return;
    }
    let (SourceWidth, SourceHeight) = (Source.width() as f32, Source.height() as f32);
    let Width = Target.width() as usize;
    let Plain = Tint == Color::White;
    Target.data_mut().par_chunks_mut(Width * 4).enumerate().skip(Bounds.Y as usize).take(Bounds.H as usize).for_each(|(Y, Row)| {
        for X in Bounds.X..Bounds.right() {
            let (U, V) = Inverse.apply(X as f32 + 0.5, Y as f32 + 0.5);
            if U < -0.5 || V < -0.5 || U > SourceWidth + 0.5 || V > SourceHeight + 0.5 {
                continue;
            }
            let mut Sample = if Smooth {
                blend::to_u8(Source.sample_bilinear(U, V))
            } else {
                if U < 0.0 || V < 0.0 || U >= SourceWidth || V >= SourceHeight {
                    continue;
                }
                Source.sample_nearest(U, V)
            };
            if Sample[3] == 0 {
                continue;
            }
            if !Plain {
                Sample = [
                    (Sample[0] as f32 * Tint.R).round().clamp(0.0, 255.0) as u8,
                    (Sample[1] as f32 * Tint.G).round().clamp(0.0, 255.0) as u8,
                    (Sample[2] as f32 * Tint.B).round().clamp(0.0, 255.0) as u8,
                    (Sample[3] as f32 * Tint.A).round().clamp(0.0, 255.0) as u8,
                ];
            }
            let Offset = X as usize * 4;
            let Existing = [Row[Offset], Row[Offset + 1], Row[Offset + 2], Row[Offset + 3]];
            Row[Offset..Offset + 4].copy_from_slice(&blend::composite_pixel(Existing, Sample, Opacity, Mode));
        }
    });
}

fn apply_mask(Target: &mut Raster, Coverage: &Raster, Invert: bool) {
    Target.pixels_mut().par_iter_mut().zip(Coverage.pixels().par_iter()).for_each(|(Pixel, Mask)| {
        let Amount = if Invert { 255 - Mask[3] } else { Mask[3] } as u32;
        Pixel[3] = (Pixel[3] as u32 * Amount / 255) as u8;
    });
}

fn mix_rasters(Original: &Raster, Processed: &mut Raster, Amount: f32) {
    Processed.pixels_mut().par_iter_mut().zip(Original.pixels().par_iter()).for_each(|(Pixel, Before)| {
        let Mixed = blend::lerp_premul(blend::premultiply(blend::to_f32(*Before)), blend::premultiply(blend::to_f32(*Pixel)), Amount);
        *Pixel = blend::to_u8(blend::unpremul(Mixed));
    });
}

struct Renderer<'a, 'h> {
    Rig: &'a Rig,
    Pose: &'a Pose,
    Sprites: &'a SpriteCache,
    Solver: Solver<'a>,
    Ordered: Vec<Vec<usize>>,
    Smooth: bool,
    Hook: Option<&'h mut dyn EffectHook>,
}

impl<'a, 'h> Renderer<'a, 'h> {
    fn blank(&self) -> Raster {
        Raster::new(self.Rig.Width.max(1), self.Rig.Height.max(1))
    }

    fn sprite_of(&self, Index: usize) -> Option<(&'a Raster, Affine)> {
        let NodeKind::Sprite { Source: Some(Id), PivotX, PivotY, Frames } = &self.Rig.Nodes[Index].Kind else { return None };
        let Pixels: &'a Arc<Raster> = self.Sprites.get(*Id, *Frames, self.Pose.States[Index].Frame)?;
        Some((Pixels.as_ref(), self.Pose.World[Index].mul(&Affine::translate(-PivotX, -PivotY))))
    }

    fn paint_own(&self, Index: usize, Target: &mut Raster) {
        if let Some((Pixels, Matrix)) = self.sprite_of(Index) {
            paint_sprite(Target, Pixels, &Matrix, self.Pose.Alpha[Index], self.Pose.Colors[Index], self.Rig.Nodes[Index].Blend, self.Smooth);
        }
    }

    fn coverage(&self, Index: usize, Subtree: bool) -> Raster {
        let mut Out = self.blank();
        let mut Stack = vec![Index];
        while let Some(Next) = Stack.pop() {
            if let Some((Pixels, Matrix)) = self.sprite_of(Next) {
                paint_sprite(&mut Out, Pixels, &Matrix, 1.0, Color::White, BlendMode::Normal, self.Smooth);
            }
            if Subtree {
                Stack.extend(self.Solver.Children[Next].iter().copied());
            }
        }
        Out
    }

    fn render_list(&mut self, Nodes: &[usize], Target: &mut Raster) {
        for Index in Nodes {
            self.node(*Index, Target);
        }
    }

    fn node(&mut self, Index: usize, Target: &mut Raster) {
        if !self.Pose.Shown[Index] {
            return;
        }
        let Source = &self.Rig.Nodes[Index];
        if let NodeKind::Effect { Effect } = Source.Kind {
            let State = &self.Pose.States[Index];
            let (Params, Paint, Time, Amount) = (State.Params.clone(), State.Tint, self.Pose.timing(), self.Pose.Alpha[Index]);
            let Children = self.Ordered[Index].clone();
            if Children.is_empty() {
                let Original = (Amount < 0.999).then(|| Target.clone());
                effects::apply(Effect, Source.Id, &Params, Paint, Time, Target, reborrow(&mut self.Hook));
                if let Some(Before) = Original {
                    mix_rasters(&Before, Target, Amount);
                }
            } else {
                let mut Layer = self.blank();
                self.render_list(&Children, &mut Layer);
                effects::apply(Effect, Source.Id, &Params, Paint, Time, &mut Layer, reborrow(&mut self.Hook));
                Target.draw_over(&Layer, 0, 0, 1.0, Source.Blend);
            }
            return;
        }
        if let Some(MaskIndex) = self.Solver.index(Source.MaskBy).filter(|Found| *Found != Index) {
            let mut Layer = self.blank();
            self.body(Index, &mut Layer);
            let Coverage = self.coverage(MaskIndex, true);
            apply_mask(&mut Layer, &Coverage, Source.InvertMask);
            Target.draw_over(&Layer, 0, 0, 1.0, BlendMode::Normal);
            return;
        }
        self.body(Index, Target);
    }

    fn body(&mut self, Index: usize, Target: &mut Raster) {
        let Children = self.Ordered[Index].clone();
        let Split = Children.iter().position(|Child| self.Pose.States[*Child].DrawOrder >= 0).unwrap_or(Children.len());
        let (Behind, Front) = Children.split_at(Split);
        let Clip = self.Rig.Nodes[Index].ClipChildren && self.sprite_of(Index).is_some();
        if !Clip {
            self.render_list(Behind, Target);
            self.paint_own(Index, Target);
            self.render_list(Front, Target);
            return;
        }
        let Coverage = self.coverage(Index, false);
        for (Group, Own) in [(Behind, false), (Front, true)] {
            if Own {
                self.paint_own(Index, Target);
            }
            if Group.is_empty() {
                continue;
            }
            let mut Layer = self.blank();
            self.render_list(Group, &mut Layer);
            apply_mask(&mut Layer, &Coverage, false);
            Target.draw_over(&Layer, 0, 0, 1.0, BlendMode::Normal);
        }
    }
}

pub fn draw_order(Pose: &Pose, Nodes: &[usize]) -> Vec<usize> {
    let mut Sorted: Vec<(i32, usize, usize)> = Nodes.iter().enumerate().map(|(Position, Index)| (Pose.States[*Index].DrawOrder, Position, *Index)).collect();
    Sorted.sort();
    Sorted.into_iter().map(|(_, _, Index)| Index).collect()
}

pub fn render_pose(Rig: &Rig, Pose: &Pose, Sprites: &SpriteCache, Settings: &RenderSettings, Hook: Option<&mut dyn EffectHook>) -> Raster {
    let Solver = Solver::new(Rig);
    let Ordered: Vec<Vec<usize>> = Solver.Children.iter().map(|Children| draw_order(Pose, Children)).collect();
    let Roots: Vec<usize> = (0..Rig.Nodes.len()).filter(|Index| Solver.Parents[*Index].is_none()).collect();
    let Roots = draw_order(Pose, &Roots);
    let mut Engine = Renderer { Rig, Pose, Sprites, Solver, Ordered, Smooth: Settings.Smooth, Hook };
    let mut Out = Engine.blank();
    Engine.render_list(&Roots, &mut Out);
    if let Some(Background) = Settings.Background {
        let mut Filled = Raster::filled(Out.width(), Out.height(), Background.to_rgba8());
        Filled.draw_over(&Out, 0, 0, 1.0, BlendMode::Normal);
        return Filled;
    }
    Out
}

pub fn render_frame(Owner: &Project, ClipIndex: Option<usize>, Frame: f32, Sprites: &SpriteCache, Hook: Option<&mut dyn EffectHook>) -> Raster {
    let Rig = &Owner.Rig;
    let Clip = ClipIndex.and_then(|Index| Rig.Clips.get(Index));
    let Pose = Solver::new(Rig).evaluate(Clip, Frame);
    render_pose(Rig, &Pose, Sprites, &RenderSettings::for_rig(Rig), Hook)
}

pub fn clip_frames(Owner: &Project, ClipIndex: usize, Hook: Option<&mut dyn EffectHook>) -> Vec<Frame> {
    let Rig = &Owner.Rig;
    let Some(Clip) = Rig.Clips.get(ClipIndex) else { return Vec::new() };
    let Sprites = SpriteCache::build(Owner);
    let Delay = (1000.0 / Clip.Fps.max(0.1)).round().max(1.0) as u32;
    let Solver = Solver::new(Rig);
    let Settings = RenderSettings::for_rig(Rig);
    let mut Hook = Hook;
    let Physics = Solver.has_physics();
    let mut Simulation = super::pose::Physics::default();
    if Physics && Clip.Loop {
        for Step in 0..Clip.Length.max(1) {
            Solver.step(Some(Clip), Step as f32, Some(&mut Simulation));
        }
    }
    (0..Clip.Length.max(1))
        .map(|Index| {
            let Pose = if Physics { Solver.step(Some(Clip), Index as f32, Some(&mut Simulation)) } else { Solver.step(Some(Clip), Index as f32, None) };
            let Pixels = render_pose(Rig, &Pose, &Sprites, &Settings, reborrow(&mut Hook));
            Frame { Name: format!("{} {}", Clip.Name, Index + 1), Pixels, DelayMs: Delay }
        })
        .collect()
}
