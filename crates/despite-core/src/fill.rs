use crate::blend::{self, BlendMode};
use crate::color::Color;
use crate::geom::IRect;
use crate::raster::Raster;
use crate::selection::{Mask, MaskView};

#[inline]
pub fn color_distance(First: [u8; 4], Second: [u8; 4]) -> f32 {
    if First[3] == 0 && Second[3] == 0 {
        return 0.0;
    }
    let mut Largest = 0u8;
    for Channel in 0..4 {
        Largest = Largest.max(First[Channel].abs_diff(Second[Channel]));
    }
    Largest as f32 / 255.0
}

pub fn flood_region(Reference: &Raster, X: i32, Y: i32, Tolerance: f32, Contiguous: bool) -> Mask {
    let (Width, Height) = Reference.size();
    let mut Out = Mask::new(Width, Height);
    let Some(Seed) = Reference.get_checked(X, Y) else { return Out };
    let Matches = |PixelX: u32, PixelY: u32| color_distance(Reference.get(PixelX, PixelY), Seed) <= Tolerance + 1e-6;
    if !Contiguous {
        for PixelY in 0..Height {
            for PixelX in 0..Width {
                if Matches(PixelX, PixelY) {
                    Out.set(PixelX, PixelY, 255);
                }
            }
        }
        return Out;
    }
    let mut Stack = vec![(X as u32, Y as u32)];
    while let Some((StartX, RowY)) = Stack.pop() {
        if Out.get(StartX, RowY) != 0 || !Matches(StartX, RowY) {
            continue;
        }
        let mut Left = StartX;
        while Left > 0 && Out.get(Left - 1, RowY) == 0 && Matches(Left - 1, RowY) {
            Left -= 1;
        }
        let mut Right = StartX;
        while Right + 1 < Width && Out.get(Right + 1, RowY) == 0 && Matches(Right + 1, RowY) {
            Right += 1;
        }
        for FillX in Left..=Right {
            Out.set(FillX, RowY, 255);
        }
        for NeighbourY in [RowY.wrapping_sub(1), RowY + 1] {
            if NeighbourY >= Height {
                continue;
            }
            let mut InRun = false;
            for ScanX in Left..=Right {
                let Open = Out.get(ScanX, NeighbourY) == 0 && Matches(ScanX, NeighbourY);
                if Open && !InRun {
                    Stack.push((ScanX, NeighbourY));
                }
                InRun = Open;
            }
        }
    }
    Out
}

#[derive(Clone, Copy, Debug)]
pub struct FillStyle {
    pub Paint: Color,
    pub Opacity: f32,
    pub Blend: BlendMode,
    pub AlphaLocked: bool,
}

pub fn fill_region(Target: &mut Raster, Region: MaskView, Clip: Option<MaskView>, Style: FillStyle) -> IRect {
    let Source = Style.Paint.to_rgba8();
    let mut Dirty = IRect::Empty;
    for Y in 0..Target.height() as i32 {
        for X in 0..Target.width() as i32 {
            let mut Amount = Region.coverage(X, Y);
            if let Some(View) = &Clip {
                Amount *= View.coverage(X, Y);
            }
            if Amount <= 0.0 {
                continue;
            }
            let Before = Target.get(X as u32, Y as u32);
            let mut After = blend::composite_pixel(Before, Source, Style.Opacity * Amount, Style.Blend);
            if Style.AlphaLocked {
                After[3] = Before[3];
            }
            if After != Before {
                Target.put(X as u32, Y as u32, After);
                Dirty = Dirty.union(&IRect::new(X, Y, 1, 1));
            }
        }
    }
    Dirty
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flood_stops_at_walls() {
        let mut Canvas = Raster::filled(5, 5, [255, 255, 255, 255]);
        for Y in 0..5 {
            Canvas.put(2, Y, [0, 0, 0, 255]);
        }
        let Region = flood_region(&Canvas, 0, 0, 0.0, true);
        assert_eq!(Region.get(1, 4), 255);
        assert_eq!(Region.get(2, 0), 0);
        assert_eq!(Region.get(3, 0), 0);
        let Global = flood_region(&Canvas, 0, 0, 0.0, false);
        assert_eq!(Global.get(3, 0), 255);
    }

    #[test]
    fn fill_paints_region() {
        let mut Canvas = Raster::new(3, 3);
        let Region = flood_region(&Canvas, 1, 1, 0.0, true);
        let Dirty = fill_region(
            &mut Canvas,
            MaskView::identity(&Region),
            None,
            FillStyle { Paint: Color::rgb(1.0, 0.0, 0.0), Opacity: 1.0, Blend: BlendMode::Normal, AlphaLocked: false },
        );
        assert_eq!(Dirty, IRect::new(0, 0, 3, 3));
        assert_eq!(Canvas.get(2, 2), [255, 0, 0, 255]);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum GradientKind {
    #[default]
    Linear,
    Radial,
    Reflected,
    Conical,
}

impl GradientKind {
    pub const All: &'static [GradientKind] = &[GradientKind::Linear, GradientKind::Radial, GradientKind::Reflected, GradientKind::Conical];

    pub fn name(self) -> &'static str {
        match self {
            GradientKind::Linear => "Linear",
            GradientKind::Radial => "Radial",
            GradientKind::Reflected => "Reflected",
            GradientKind::Conical => "Conical",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GradientStyle {
    pub Kind: GradientKind,
    pub From: Color,
    pub To: Color,
    pub Steps: u32,
    pub Dither: crate::brush::DitherPattern,
    pub Opacity: f32,
    pub Blend: BlendMode,
    pub AlphaLocked: bool,
}

pub fn gradient_amount(Kind: GradientKind, Start: (f32, f32), End: (f32, f32), X: f32, Y: f32) -> f32 {
    let (Dx, Dy) = (End.0 - Start.0, End.1 - Start.1);
    let LengthSquared = (Dx * Dx + Dy * Dy).max(1e-6);
    let (Px, Py) = (X - Start.0, Y - Start.1);
    match Kind {
        GradientKind::Linear => ((Px * Dx + Py * Dy) / LengthSquared).clamp(0.0, 1.0),
        GradientKind::Reflected => ((Px * Dx + Py * Dy) / LengthSquared).abs().clamp(0.0, 1.0),
        GradientKind::Radial => ((Px * Px + Py * Py).sqrt() / LengthSquared.sqrt()).clamp(0.0, 1.0),
        GradientKind::Conical => {
            let Base = Dy.atan2(Dx);
            let Angle = Py.atan2(Px) - Base;
            (Angle / std::f32::consts::TAU).rem_euclid(1.0)
        }
    }
}

pub fn gradient_fill(
    Target: &mut Raster,
    LayerToImage: crate::transform::Affine,
    Start: (f32, f32),
    End: (f32, f32),
    Clip: Option<MaskView>,
    Style: GradientStyle,
) -> IRect {
    use crate::brush::DitherPattern;
    let mut Dirty = IRect::Empty;
    let From = Style.From.to_array();
    let To = Style.To.to_array();
    for Y in 0..Target.height() as i32 {
        for X in 0..Target.width() as i32 {
            let Coverage = Clip.as_ref().map_or(1.0, |View| View.coverage(X, Y));
            if Coverage <= 0.0 {
                continue;
            }
            let (ImageX, ImageY) = LayerToImage.apply(X as f32 + 0.5, Y as f32 + 0.5);
            let mut T = gradient_amount(Style.Kind, Start, End, ImageX, ImageY);
            if Style.Steps >= 2 {
                let Bands = (Style.Steps - 1) as f32;
                let Scaled = T * Bands;
                let Base = Scaled.floor();
                let Fraction = Scaled - Base;
                let Step = if Style.Dither != DitherPattern::None {
                    if Fraction > Style.Dither.threshold(ImageX.floor() as i32, ImageY.floor() as i32) { 1.0 } else { 0.0 }
                } else if Fraction >= 0.5 {
                    1.0
                } else {
                    0.0
                };
                T = ((Base + Step) / Bands).clamp(0.0, 1.0);
            } else if Style.Dither != DitherPattern::None {
                let Bias = Style.Dither.threshold(ImageX.floor() as i32, ImageY.floor() as i32) - 0.5;
                T = (T + Bias / 32.0).clamp(0.0, 1.0);
            }
            let mut Source = [0f32; 4];
            for Channel in 0..4 {
                Source[Channel] = From[Channel] + (To[Channel] - From[Channel]) * T;
            }
            let Before = Target.get(X as u32, Y as u32);
            let mut After = blend::to_u8(blend::composite_f32(blend::to_f32(Before), Source, Style.Opacity * Coverage, Style.Blend));
            if Style.AlphaLocked {
                After[3] = Before[3];
            }
            if After != Before {
                Target.put(X as u32, Y as u32, After);
                Dirty = Dirty.union(&IRect::new(X, Y, 1, 1));
            }
        }
    }
    Dirty
}

#[cfg(test)]
mod gradient_tests {
    use super::*;

    #[test]
    fn linear_gradient_endpoints_and_bands() {
        let mut Canvas = Raster::new(10, 1);
        let Style = GradientStyle {
            Kind: GradientKind::Linear,
            From: Color::Black,
            To: Color::White,
            Steps: 0,
            Dither: crate::brush::DitherPattern::None,
            Opacity: 1.0,
            Blend: BlendMode::Normal,
            AlphaLocked: false,
        };
        gradient_fill(&mut Canvas, crate::transform::Affine::Identity, (0.0, 0.5), (10.0, 0.5), None, Style);
        assert!(Canvas.get(0, 0)[0] < 20 && Canvas.get(9, 0)[0] > 235);
        let mut Banded = Raster::new(10, 1);
        gradient_fill(&mut Banded, crate::transform::Affine::Identity, (0.0, 0.5), (10.0, 0.5), None, GradientStyle { Steps: 2, ..Style });
        assert!(Banded.pixels().iter().all(|Pixel| Pixel[0] == 0 || Pixel[0] == 255));
    }
}
