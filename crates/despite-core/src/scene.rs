use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::blend::BlendMode;
use crate::color::Color;
use crate::geom::IRect;
use crate::image::ImageId;
use crate::layer::{Layer, LayerId, composite_layers};
use crate::overlay::Overlay;
use crate::project::Project;
use crate::raster::Raster;
use crate::transform::LayerTransform;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SceneId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Playback {
    #[default]
    Composite,
    LayersAsFrames,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneItem {
    pub Id: u64,
    pub Name: String,
    pub Source: Option<ImageId>,
    pub Transform: LayerTransform,
    pub Opacity: f32,
    pub Blend: BlendMode,
    pub Visible: bool,
    pub Locked: bool,
    pub Playback: Playback,
    pub FrameOffset: u32,
    pub HoldFrames: u32,
    pub StartFrame: u32,
    pub EndFrame: Option<u32>,
}

impl Default for SceneItem {
    fn default() -> Self {
        Self {
            Id: 0,
            Name: String::new(),
            Source: None,
            Transform: LayerTransform::default(),
            Opacity: 1.0,
            Blend: BlendMode::Normal,
            Visible: true,
            Locked: false,
            Playback: Playback::Composite,
            FrameOffset: 0,
            HoldFrames: 1,
            StartFrame: 0,
            EndFrame: None,
        }
    }
}

impl SceneItem {
    pub fn active_at(&self, Frame: u32) -> bool {
        self.Visible && Frame >= self.StartFrame && self.EndFrame.is_none_or(|End| Frame <= End)
    }

    pub fn source_frame_index(&self, Frame: u32, SourceFrames: usize) -> usize {
        if SourceFrames == 0 {
            return 0;
        }
        let Local = Frame.saturating_sub(self.StartFrame) / self.HoldFrames.max(1);
        (Local as usize + self.FrameOffset as usize) % SourceFrames
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scene {
    pub Id: SceneId,
    pub Name: String,
    pub Width: u32,
    pub Height: u32,
    pub Background: Option<Color>,
    pub Fps: f32,
    pub FrameCount: u32,
    pub PixelArt: bool,
    pub Items: Vec<SceneItem>,
    pub ActiveItem: Option<usize>,
    pub Overlays: Vec<Overlay>,
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            Id: SceneId(0),
            Name: "Scene".into(),
            Width: 256,
            Height: 256,
            Background: None,
            Fps: 12.0,
            FrameCount: 1,
            PixelArt: false,
            Items: Vec::new(),
            ActiveItem: None,
            Overlays: Vec::new(),
        }
    }
}

impl Scene {
    pub fn bounds(&self) -> IRect {
        IRect::sized(self.Width, self.Height)
    }

    pub fn auto_frame_count(&self, Owner: &Project) -> u32 {
        let mut Longest = 1u32;
        for Item in &self.Items {
            let Some(Source) = Item.Source.and_then(|Id| Owner.image(Id)) else { continue };
            let Frames = match Item.Playback {
                Playback::LayersAsFrames => Source.Layers.iter().filter(|Candidate| Candidate.Visible).count() as u32,
                Playback::Composite => Source.boil_frame_count(),
            };
            if Frames <= 1 {
                continue;
            }
            Longest = Longest.max(Item.StartFrame + Frames * Item.HoldFrames.max(1));
        }
        Longest
    }

    pub fn item_index_by_id(&self, Id: u64) -> Option<usize> {
        self.Items.iter().position(|Item| Item.Id == Id)
    }

    pub fn item_at(&self, Owner: &Project, X: f32, Y: f32, Frame: u32) -> Option<usize> {
        for (Index, Item) in self.Items.iter().enumerate().rev() {
            if !Item.active_at(Frame) {
                continue;
            }
            let Some(Source) = Item.Source.and_then(|Id| Owner.image(Id)) else { continue };
            let Matrix = Item.Transform.matrix(Source.Width, Source.Height);
            let Some(Inverse) = Matrix.inverse() else { continue };
            let (LocalX, LocalY) = Inverse.apply(X, Y);
            if LocalX >= 0.0 && LocalY >= 0.0 && LocalX < Source.Width as f32 && LocalY < Source.Height as f32 {
                return Some(Index);
            }
        }
        None
    }
}

pub fn item_raster(Owner: &Project, Item: &SceneItem, Frame: u32) -> Option<Raster> {
    let Source = Owner.image(Item.Source?)?;
    match Item.Playback {
        Playback::Composite => {
            let Local = Frame.saturating_sub(Item.StartFrame) / Item.HoldFrames.max(1) + Item.FrameOffset;
            Some(Source.composite_frame(Local))
        }
        Playback::LayersAsFrames => {
            let Frames: Vec<&Layer> = Source.Layers.iter().filter(|Candidate| Candidate.Visible).collect();
            if Frames.is_empty() {
                return None;
            }
            let Chosen = Frames[Item.source_frame_index(Frame, Frames.len())];
            let mut Out = Raster::new(Source.Width, Source.Height);
            let Single = Layer { Opacity: Chosen.Opacity, Blend: BlendMode::Normal, ..Chosen.clone() };
            composite_layers(&[&Single], Source.smooth_sampling(), &mut Out, Source.bounds());
            Some(Out)
        }
    }
}

pub fn render_scene(Owner: &Project, Target: &Scene, Frame: u32) -> Raster {
    let mut Stack: Vec<Layer> = Vec::with_capacity(Target.Items.len() + 1);
    if let Some(Fill) = Target.Background {
        Stack.push(Layer::from_raster(LayerId(0), "Background", Raster::filled(Target.Width, Target.Height, Fill.to_rgba8())));
    }
    for Item in &Target.Items {
        if !Item.active_at(Frame) {
            continue;
        }
        let Some(Pixels) = item_raster(Owner, Item, Frame) else { continue };
        Stack.push(Layer {
            Id: LayerId(Item.Id),
            Name: Item.Name.clone(),
            Visible: true,
            Locked: false,
            AlphaLocked: false,
            Opacity: Item.Opacity,
            Blend: Item.Blend,
            Transform: Item.Transform,
            Pixels: Arc::new(Pixels),
            Boil: None,
            Text: None,
        });
    }
    let Refs: Vec<&Layer> = Stack.iter().collect();
    let mut Out = Raster::new(Target.Width, Target.Height);
    composite_layers(&Refs, !Target.PixelArt, &mut Out, Target.bounds());
    Out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_places_images_and_animates_layers() {
        let mut Owner = Project::new("Test");
        let Hero = Owner.add_image("Hero", 2, 2);
        {
            let Target = Owner.image_mut(Hero).unwrap();
            Target.Layers[0].pixels_mut().fill([255, 0, 0, 255]);
            let SecondId = LayerId(999);
            let mut Second = Layer::new(SecondId, "Frame 2", 2, 2);
            Second.pixels_mut().fill([0, 255, 0, 255]);
            Target.insert_layer_above_active(Second);
        }
        let SceneRef = Owner.add_scene("Stage", 4, 4);
        let Stage = Owner.scene_mut(SceneRef).unwrap();
        Stage.Background = Some(Color::rgb(0.0, 0.0, 1.0));
        Stage.Items.push(SceneItem {
            Id: 50,
            Source: Some(Hero),
            Transform: LayerTransform { OffsetX: 2.0, OffsetY: 2.0, ..Default::default() },
            Playback: Playback::LayersAsFrames,
            ..Default::default()
        });
        let Stage = Owner.scene(SceneRef).unwrap().clone();
        let First = render_scene(&Owner, &Stage, 0);
        assert_eq!(First.get(0, 0), [0, 0, 255, 255]);
        assert_eq!(First.get(3, 3), [255, 0, 0, 255]);
        let Second = render_scene(&Owner, &Stage, 1);
        assert_eq!(Second.get(3, 3), [0, 255, 0, 255]);
        assert_eq!(Stage.auto_frame_count(&Owner), 2);
        assert_eq!(Stage.item_at(&Owner, 2.5, 2.5, 0), Some(0));
        assert_eq!(Stage.item_at(&Owner, 0.5, 0.5, 0), None);
    }
}
