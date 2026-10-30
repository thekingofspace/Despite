use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::blend::{self, BlendMode};
use crate::geom::IRect;
use crate::raster::Raster;
use crate::transform::{Affine, LayerTransform};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LayerId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BoilTag {
    pub Group: u64,
    pub Index: u32,
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub Id: LayerId,
    pub Name: String,
    pub Visible: bool,
    pub Locked: bool,
    pub AlphaLocked: bool,
    pub Opacity: f32,
    pub Blend: BlendMode,
    pub Transform: LayerTransform,
    pub Pixels: Arc<Raster>,
    pub Boil: Option<BoilTag>,
    pub Text: Option<crate::text::TextData>,
}

impl Layer {
    pub fn new(Id: LayerId, Name: impl Into<String>, Width: u32, Height: u32) -> Self {
        Self::from_raster(Id, Name, Raster::new(Width, Height))
    }

    pub fn from_raster(Id: LayerId, Name: impl Into<String>, Pixels: Raster) -> Self {
        Self {
            Id,
            Name: Name.into(),
            Visible: true,
            Locked: false,
            AlphaLocked: false,
            Opacity: 1.0,
            Blend: BlendMode::Normal,
            Transform: LayerTransform::default(),
            Pixels: Arc::new(Pixels),
            Boil: None,
            Text: None,
        }
    }

    pub fn pixels_mut(&mut self) -> &mut Raster {
        Arc::make_mut(&mut self.Pixels)
    }

    pub fn matrix(&self) -> Affine {
        self.Transform.matrix(self.Pixels.width(), self.Pixels.height())
    }

    pub fn image_bounds(&self) -> IRect {
        match self.Transform.integer_offset() {
            Some((Dx, Dy)) => self.Pixels.bounds().translate(Dx, Dy),
            None => self.matrix().bounds_of(self.Pixels.width() as f32, self.Pixels.height() as f32),
        }
    }

    pub fn image_to_layer(&self, X: f32, Y: f32) -> Option<(f32, f32)> {
        self.matrix().inverse().map(|Inverse| Inverse.apply(X, Y))
    }

    pub fn rasterize_in_image_space(&self, ImageWidth: u32, ImageHeight: u32, Smooth: bool) -> Raster {
        let mut Out = Raster::new(ImageWidth, ImageHeight);
        let Flat = Layer {
            Opacity: 1.0,
            Blend: BlendMode::Normal,
            Visible: true,
            ..self.clone()
        };
        composite_layers(&[&Flat], Smooth, &mut Out, IRect::sized(ImageWidth, ImageHeight));
        Out
    }

    pub fn bake_transform(&mut self, ImageWidth: u32, ImageHeight: u32, Smooth: bool) {
        if self.Transform.integer_offset() == Some((0, 0))
            && self.Pixels.width() == ImageWidth
            && self.Pixels.height() == ImageHeight
        {
            return;
        }
        let Baked = self.rasterize_in_image_space(ImageWidth, ImageHeight, Smooth);
        self.Pixels = Arc::new(Baked);
        self.Transform = LayerTransform::default();
    }
}

struct PreparedLayer<'a> {
    Source: &'a Layer,
    Offset: Option<(i32, i32)>,
    Inverse: Affine,
    Bounds: IRect,
}

pub fn composite_layers(Layers: &[&Layer], Smooth: bool, Out: &mut Raster, Region: IRect) {
    use rayon::prelude::*;

    let Region = Region.clamp_to(Out.width(), Out.height());
    if Region.is_empty() {
        return;
    }
    let Prepared: Vec<PreparedLayer> = Layers
        .iter()
        .filter(|Candidate| Candidate.Visible && Candidate.Opacity > 0.0)
        .filter_map(|Candidate| {
            let Bounds = Candidate.image_bounds().intersect(&Region);
            if Bounds.is_empty() {
                return None;
            }
            let Offset = Candidate.Transform.integer_offset();
            let Inverse = Candidate.matrix().inverse()?;
            Some(PreparedLayer { Source: Candidate, Offset, Inverse, Bounds })
        })
        .collect();

    let Width = Out.width() as usize;
    let RowStart = Region.Y as usize;
    let RowEnd = Region.bottom() as usize;
    Out.data_mut()[RowStart * Width * 4..RowEnd * Width * 4]
        .par_chunks_mut(Width * 4)
        .enumerate()
        .for_each(|(RowIndex, Row)| {
            let Y = (RowStart + RowIndex) as i32;
            for X in Region.X..Region.right() {
                let mut Accumulated = [0f32; 4];
                for Entry in &Prepared {
                    if !Entry.Bounds.contains(X, Y) {
                        continue;
                    }
                    let Sample = match Entry.Offset {
                        Some((Dx, Dy)) => match Entry.Source.Pixels.get_checked(X - Dx, Y - Dy) {
                            Some(Pixel) => blend::to_f32(Pixel),
                            None => continue,
                        },
                        None => {
                            let (LayerX, LayerY) = Entry.Inverse.apply(X as f32 + 0.5, Y as f32 + 0.5);
                            if Smooth {
                                Entry.Source.Pixels.sample_bilinear(LayerX, LayerY)
                            } else {
                                blend::to_f32(Entry.Source.Pixels.sample_nearest(LayerX, LayerY))
                            }
                        }
                    };
                    if Sample[3] <= 0.0 {
                        continue;
                    }
                    Accumulated = blend::composite_f32(Accumulated, Sample, Entry.Source.Opacity, Entry.Source.Blend);
                }
                let Index = X as usize * 4;
                Row[Index..Index + 4].copy_from_slice(&blend::to_u8(Accumulated));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_respects_offset_and_opacity() {
        let mut Bottom = Layer::from_raster(LayerId(1), "Bottom", Raster::filled(4, 4, [0, 0, 255, 255]));
        Bottom.Opacity = 1.0;
        let mut Top = Layer::from_raster(LayerId(2), "Top", Raster::filled(2, 2, [255, 0, 0, 255]));
        Top.Transform.OffsetX = 2.0;
        Top.Transform.OffsetY = 2.0;
        Top.Opacity = 0.5;
        let mut Out = Raster::new(4, 4);
        composite_layers(&[&Bottom, &Top], false, &mut Out, IRect::sized(4, 4));
        assert_eq!(Out.get(0, 0), [0, 0, 255, 255]);
        let Mixed = Out.get(3, 3);
        assert!(Mixed[0] > 120 && Mixed[0] < 135 && Mixed[2] > 120 && Mixed[2] < 135, "{Mixed:?}");
    }

    #[test]
    fn bake_flip() {
        let mut Source = Raster::new(2, 1);
        Source.put(0, 0, [255, 0, 0, 255]);
        let mut Flipped = Layer::from_raster(LayerId(1), "L", Source);
        Flipped.Transform.FlipX = true;
        Flipped.bake_transform(2, 1, false);
        assert_eq!(Flipped.Pixels.get(1, 0), [255, 0, 0, 255]);
        assert_eq!(Flipped.Pixels.get(0, 0), [0, 0, 0, 0]);
    }
}
