use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::blend::BlendMode;
use crate::geom::IRect;
use crate::layer::{Layer, LayerId, composite_layers};
use crate::overlay::Overlay;
use crate::raster::Raster;
use crate::transform::LayerTransform;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ImageId(pub u64);

#[derive(Clone, Debug)]
pub struct Image {
    pub Id: ImageId,
    pub Name: String,
    pub Width: u32,
    pub Height: u32,
    pub Layers: Vec<Layer>,
    pub ActiveLayer: usize,
    pub FrameDelayMs: u32,
    pub PixelArt: bool,
    pub Overlays: Vec<Overlay>,
}

impl Image {
    pub fn new(Id: ImageId, Name: impl Into<String>, Width: u32, Height: u32, FirstLayer: LayerId) -> Self {
        Self {
            Id,
            Name: Name.into(),
            Width,
            Height,
            Layers: vec![Layer::new(FirstLayer, "Layer 1", Width, Height)],
            ActiveLayer: 0,
            FrameDelayMs: 100,
            PixelArt: false,
            Overlays: Vec::new(),
        }
    }

    pub fn bounds(&self) -> IRect {
        IRect::sized(self.Width, self.Height)
    }

    pub fn layer_index(&self, Id: LayerId) -> Option<usize> {
        self.Layers.iter().position(|Candidate| Candidate.Id == Id)
    }

    pub fn layer(&self, Id: LayerId) -> Option<&Layer> {
        self.Layers.iter().find(|Candidate| Candidate.Id == Id)
    }

    pub fn layer_mut(&mut self, Id: LayerId) -> Option<&mut Layer> {
        self.Layers.iter_mut().find(|Candidate| Candidate.Id == Id)
    }

    pub fn active_layer(&self) -> Option<&Layer> {
        self.Layers.get(self.ActiveLayer)
    }

    pub fn active_layer_mut(&mut self) -> Option<&mut Layer> {
        self.Layers.get_mut(self.ActiveLayer)
    }

    pub fn clamp_active(&mut self) {
        if self.Layers.is_empty() {
            self.ActiveLayer = 0;
        } else {
            self.ActiveLayer = self.ActiveLayer.min(self.Layers.len() - 1);
        }
    }

    pub fn smooth_sampling(&self) -> bool {
        !self.PixelArt
    }

    pub fn boil_group_sizes(&self) -> HashMap<u64, u32> {
        let mut Sizes: HashMap<u64, u32> = HashMap::new();
        for Entry in &self.Layers {
            if let Some(Tag) = Entry.Boil {
                *Sizes.entry(Tag.Group).or_default() += 1;
            }
        }
        Sizes
    }

    pub fn boil_frame_count(&self) -> u32 {
        self.boil_group_sizes().values().copied().max().unwrap_or(1).max(1)
    }

    pub fn boil_members(&self, Group: u64) -> Vec<usize> {
        let mut Members: Vec<(u32, usize)> = self
            .Layers
            .iter()
            .enumerate()
            .filter_map(|(Index, Entry)| Entry.Boil.filter(|Tag| Tag.Group == Group).map(|Tag| (Tag.Index, Index)))
            .collect();
        Members.sort();
        Members.into_iter().map(|(_, Index)| Index).collect()
    }

    pub fn layers_at_frame(&self, Frame: u32) -> Vec<&Layer> {
        let Sizes = self.boil_group_sizes();
        self.Layers
            .iter()
            .filter(|Entry| match Entry.Boil {
                None => true,
                Some(Tag) => {
                    let Count = Sizes.get(&Tag.Group).copied().unwrap_or(1).max(1);
                    Tag.Index % Count == Frame % Count
                }
            })
            .collect()
    }

    pub fn composite(&self) -> Raster {
        self.composite_frame(0)
    }

    pub fn composite_frame(&self, Frame: u32) -> Raster {
        let mut Out = Raster::new(self.Width, self.Height);
        self.composite_into_frame(&mut Out, self.bounds(), Frame);
        Out
    }

    pub fn composite_into(&self, Out: &mut Raster, Region: IRect) {
        self.composite_into_frame(Out, Region, 0);
    }

    pub fn composite_into_frame(&self, Out: &mut Raster, Region: IRect, Frame: u32) {
        let Chosen = self.layers_at_frame(Frame);
        composite_layers(&Chosen, self.smooth_sampling(), Out, Region);
    }

    pub fn make_boil_group(&mut self, Index: usize, Count: u32, Group: u64, NewIds: &[LayerId]) -> Vec<usize> {
        let Count = Count.max(2);
        let Some(Base) = self.Layers.get_mut(Index) else { return Vec::new() };
        if let Some(Tag) = Base.Boil {
            return self.boil_members(Tag.Group);
        }
        Base.Boil = Some(crate::layer::BoilTag { Group, Index: 0 });
        let Template = Base.clone();
        for Step in 1..Count {
            let Some(Id) = NewIds.get(Step as usize - 1) else { break };
            let mut Copy = Template.clone();
            Copy.Id = *Id;
            Copy.Name = format!("{} frame {}", Template.Name, Step + 1);
            Copy.Boil = Some(crate::layer::BoilTag { Group, Index: Step });
            self.Layers.insert(Index + Step as usize, Copy);
        }
        self.boil_members(Group)
    }

    pub fn unboil(&mut self, Group: u64, Keep: usize) {
        let Members = self.boil_members(Group);
        let KeepId = self.Layers.get(Keep).map(|Entry| Entry.Id);
        let ActiveId = self.Layers.get(self.ActiveLayer).map(|Entry| Entry.Id);
        let Removed: Vec<LayerId> = Members.iter().filter_map(|Index| self.Layers.get(*Index)).map(|Entry| Entry.Id).filter(|Id| Some(*Id) != KeepId).collect();
        self.Layers.retain(|Entry| !Removed.contains(&Entry.Id));
        if let Some(Kept) = KeepId.and_then(|Id| self.layer_mut(Id)) {
            Kept.Boil = None;
        }
        let Target = if ActiveId.is_some_and(|Id| Removed.contains(&Id)) { KeepId } else { ActiveId };
        self.ActiveLayer = Target.and_then(|Id| self.layer_index(Id)).unwrap_or(0);
        self.clamp_active();
    }

    pub fn composite_where(&self, Include: impl Fn(&Layer) -> bool) -> Raster {
        let mut Out = Raster::new(self.Width, self.Height);
        let Chosen: Vec<&Layer> = self.Layers.iter().filter(|Candidate| Include(Candidate)).collect();
        composite_layers(&Chosen, self.smooth_sampling(), &mut Out, self.bounds());
        Out
    }

    pub fn insert_layer_above_active(&mut self, New: Layer) -> usize {
        let Index = if self.Layers.is_empty() { 0 } else { (self.ActiveLayer + 1).min(self.Layers.len()) };
        self.Layers.insert(Index, New);
        self.ActiveLayer = Index;
        Index
    }

    pub fn remove_layer(&mut self, Index: usize) -> Option<Layer> {
        if Index >= self.Layers.len() || self.Layers.len() <= 1 {
            return None;
        }
        let Removed = self.Layers.remove(Index);
        if self.ActiveLayer >= Index && self.ActiveLayer > 0 {
            self.ActiveLayer -= 1;
        }
        self.clamp_active();
        Some(Removed)
    }

    pub fn move_layer(&mut self, From: usize, To: usize) {
        if From >= self.Layers.len() || To >= self.Layers.len() || From == To {
            return;
        }
        let Moved = self.Layers.remove(From);
        self.Layers.insert(To, Moved);
        if self.ActiveLayer == From {
            self.ActiveLayer = To;
        } else if From < self.ActiveLayer && To >= self.ActiveLayer {
            self.ActiveLayer -= 1;
        } else if From > self.ActiveLayer && To <= self.ActiveLayer {
            self.ActiveLayer += 1;
        }
    }

    pub fn merge_down(&mut self, Index: usize) -> bool {
        if Index == 0 || Index >= self.Layers.len() {
            return false;
        }
        let Smooth = self.smooth_sampling();
        let (Width, Height) = (self.Width, self.Height);
        let Upper = self.Layers.remove(Index);
        let Lower = &mut self.Layers[Index - 1];
        Lower.bake_transform(Width, Height, Smooth);
        if Upper.Visible {
            let Isolated = Layer { Blend: BlendMode::Normal, Opacity: 1.0, ..Upper.clone() };
            let mut UpperFlat = Raster::new(Width, Height);
            composite_layers(&[&Isolated], Smooth, &mut UpperFlat, IRect::sized(Width, Height));
            Lower.pixels_mut().draw_over(&UpperFlat, 0, 0, Upper.Opacity, Upper.Blend);
        }
        if self.ActiveLayer >= Index {
            self.ActiveLayer -= 1;
        }
        true
    }

    pub fn flatten(&mut self, NewLayer: LayerId) {
        let Flat = self.composite();
        self.Layers = vec![Layer::from_raster(NewLayer, "Flattened", Flat)];
        self.ActiveLayer = 0;
    }

    pub fn resize_canvas(&mut self, Width: u32, Height: u32, AnchorX: f32, AnchorY: f32) {
        let ShiftX = ((Width as f32 - self.Width as f32) * AnchorX).round() as i32;
        let ShiftY = ((Height as f32 - self.Height as f32) * AnchorY).round() as i32;
        for Entry in &mut self.Layers {
            match Entry.Transform.integer_offset() {
                Some((Dx, Dy)) => {
                    let Resized = Entry.Pixels.resized_canvas(Width, Height, Dx + ShiftX, Dy + ShiftY);
                    Entry.Pixels = Arc::new(Resized);
                    Entry.Transform = LayerTransform::default();
                }
                None => {
                    Entry.Transform.OffsetX += ShiftX as f32;
                    Entry.Transform.OffsetY += ShiftY as f32;
                }
            }
        }
        self.Width = Width;
        self.Height = Height;
    }

    pub fn scale_image(&mut self, Width: u32, Height: u32) {
        let FactorX = Width as f32 / self.Width.max(1) as f32;
        let FactorY = Height as f32 / self.Height.max(1) as f32;
        let PixelArt = self.PixelArt;
        for Entry in &mut self.Layers {
            let NewWidth = ((Entry.Pixels.width() as f32 * FactorX).round() as u32).max(1);
            let NewHeight = ((Entry.Pixels.height() as f32 * FactorY).round() as u32).max(1);
            let Scaled = if PixelArt {
                Entry.Pixels.scaled_nearest(NewWidth, NewHeight)
            } else {
                Entry.Pixels.scaled_smooth(NewWidth, NewHeight)
            };
            Entry.Pixels = Arc::new(Scaled);
            Entry.Transform.OffsetX *= FactorX;
            Entry.Transform.OffsetY *= FactorY;
        }
        self.Width = Width;
        self.Height = Height;
    }

    pub fn crop(&mut self, Rect: IRect) {
        let Rect = Rect.clamp_to(self.Width, self.Height);
        if Rect.is_empty() {
            return;
        }
        for Entry in &mut self.Layers {
            Entry.Transform.OffsetX -= Rect.X as f32;
            Entry.Transform.OffsetY -= Rect.Y as f32;
            if let Some((Dx, Dy)) = Entry.Transform.integer_offset() {
                let Resized = Entry.Pixels.resized_canvas(Rect.W as u32, Rect.H as u32, Dx, Dy);
                Entry.Pixels = Arc::new(Resized);
                Entry.Transform = LayerTransform::default();
            }
        }
        self.Width = Rect.W as u32;
        self.Height = Rect.H as u32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_image() -> Image {
        let mut Result = Image::new(ImageId(1), "Test", 4, 4, LayerId(2));
        Result.Layers[0].pixels_mut().fill([0, 0, 255, 255]);
        let mut Top = Layer::new(LayerId(3), "Top", 4, 4);
        Top.pixels_mut().put(1, 1, [255, 0, 0, 255]);
        Result.insert_layer_above_active(Top);
        Result
    }

    #[test]
    fn merge_down_combines() {
        let mut Subject = sample_image();
        assert!(Subject.merge_down(1));
        assert_eq!(Subject.Layers.len(), 1);
        assert_eq!(Subject.Layers[0].Pixels.get(1, 1), [255, 0, 0, 255]);
        assert_eq!(Subject.Layers[0].Pixels.get(0, 0), [0, 0, 255, 255]);
    }

    #[test]
    fn resize_canvas_centered() {
        let mut Subject = sample_image();
        Subject.resize_canvas(6, 6, 0.5, 0.5);
        assert_eq!(Subject.Layers[1].Pixels.get(2, 2), [255, 0, 0, 255]);
        assert_eq!(Subject.composite().get(0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn boil_groups_cycle_frames() {
        let mut Subject = Image::new(ImageId(1), "Boil", 2, 1, LayerId(2));
        Subject.Layers[0].pixels_mut().put(0, 0, [255, 0, 0, 255]);
        let Members = Subject.make_boil_group(0, 3, 77, &[LayerId(3), LayerId(4)]);
        assert_eq!(Members, vec![0, 1, 2]);
        Subject.Layers[1].pixels_mut().put(0, 0, [0, 255, 0, 255]);
        Subject.Layers[2].pixels_mut().put(0, 0, [0, 0, 255, 255]);
        assert_eq!(Subject.boil_frame_count(), 3);
        assert_eq!(Subject.composite_frame(0).get(0, 0), [255, 0, 0, 255]);
        assert_eq!(Subject.composite_frame(1).get(0, 0), [0, 255, 0, 255]);
        assert_eq!(Subject.composite_frame(5).get(0, 0), [0, 0, 255, 255]);
        Subject.unboil(77, 1);
        assert_eq!(Subject.Layers.len(), 1);
        assert_eq!(Subject.composite().get(0, 0), [0, 255, 0, 255]);
    }

    #[test]
    fn move_layer_tracks_active() {
        let mut Subject = sample_image();
        assert_eq!(Subject.ActiveLayer, 1);
        Subject.move_layer(1, 0);
        assert_eq!(Subject.ActiveLayer, 0);
        assert_eq!(Subject.Layers[0].Name, "Top");
    }
}
