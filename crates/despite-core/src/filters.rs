use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::blend;
use crate::brush::DitherPattern;
use crate::color::{self, Color};
use crate::fill::color_distance;
use crate::geom::IRect;
use crate::raster::Raster;
use crate::rng;
use crate::selection::MaskView;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Filter {
    Grayscale,
    Invert,
    Sepia,
    BrightnessContrast { Brightness: f32, Contrast: f32 },
    HueSaturation { Hue: f32, Saturation: f32, Lightness: f32 },
    GaussianBlur { Radius: f32 },
    BoxBlur { Radius: u32 },
    MotionBlur { Angle: f32, Distance: u32 },
    Sharpen { Amount: f32, Radius: f32 },
    Posterize { Levels: u32 },
    Threshold { Level: f32 },
    Pixelate { Size: u32 },
    Noise { Amount: f32, Monochrome: bool, Seed: u64 },
    Outline { Paint: Color, Thickness: u32, Outside: bool, Diagonal: bool },
    PaletteQuantize { Dither: DitherPattern, ErrorDiffusion: bool },
    ReplaceColor { From: Color, To: Color, Tolerance: f32 },
    RemoveColor { Target: Color, Tolerance: f32 },
    AlphaThreshold { Level: f32 },
    DropShadow { OffsetX: i32, OffsetY: i32, Blur: f32, Paint: Color },
    Vignette { Strength: f32, Radius: f32 },
}

impl Filter {
    pub fn catalogue() -> Vec<Filter> {
        vec![
            Filter::Grayscale,
            Filter::Invert,
            Filter::Sepia,
            Filter::BrightnessContrast { Brightness: 0.0, Contrast: 0.0 },
            Filter::HueSaturation { Hue: 0.0, Saturation: 0.0, Lightness: 0.0 },
            Filter::GaussianBlur { Radius: 4.0 },
            Filter::BoxBlur { Radius: 2 },
            Filter::MotionBlur { Angle: 0.0, Distance: 8 },
            Filter::Sharpen { Amount: 0.6, Radius: 1.5 },
            Filter::Posterize { Levels: 4 },
            Filter::Threshold { Level: 0.5 },
            Filter::Pixelate { Size: 4 },
            Filter::Noise { Amount: 0.15, Monochrome: true, Seed: 7 },
            Filter::Outline { Paint: Color::Black, Thickness: 1, Outside: true, Diagonal: false },
            Filter::PaletteQuantize { Dither: DitherPattern::None, ErrorDiffusion: false },
            Filter::ReplaceColor { From: Color::White, To: Color::Black, Tolerance: 0.05 },
            Filter::RemoveColor { Target: Color::White, Tolerance: 0.05 },
            Filter::AlphaThreshold { Level: 0.5 },
            Filter::DropShadow { OffsetX: 2, OffsetY: 2, Blur: 2.0, Paint: Color::rgba(0.0, 0.0, 0.0, 0.6) },
            Filter::Vignette { Strength: 0.5, Radius: 0.75 },
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            Filter::Grayscale => "Grayscale",
            Filter::Invert => "Invert",
            Filter::Sepia => "Sepia",
            Filter::BrightnessContrast { .. } => "Brightness / Contrast",
            Filter::HueSaturation { .. } => "Hue / Saturation / Lightness",
            Filter::GaussianBlur { .. } => "Gaussian Blur",
            Filter::BoxBlur { .. } => "Box Blur",
            Filter::MotionBlur { .. } => "Motion Blur",
            Filter::Sharpen { .. } => "Sharpen (Unsharp Mask)",
            Filter::Posterize { .. } => "Posterize",
            Filter::Threshold { .. } => "Threshold",
            Filter::Pixelate { .. } => "Pixelate",
            Filter::Noise { .. } => "Add Noise",
            Filter::Outline { .. } => "Outline (Pixel Art)",
            Filter::PaletteQuantize { .. } => "Reduce To Palette",
            Filter::ReplaceColor { .. } => "Replace Color",
            Filter::RemoveColor { .. } => "Remove Color (Make Transparent)",
            Filter::AlphaThreshold { .. } => "Hard Alpha (Remove Semi-Transparency)",
            Filter::DropShadow { .. } => "Drop Shadow",
            Filter::Vignette { .. } => "Vignette",
        }
    }

    pub fn has_settings(&self) -> bool {
        !matches!(self, Filter::Grayscale | Filter::Invert | Filter::Sepia)
    }
}

pub fn apply_filter(Target: &mut Raster, Chosen: &Filter, Clip: Option<MaskView>, Palette: &[Color]) -> IRect {
    let Original = Target.clone();
    let Processed = run_filter(&Original, Chosen, Palette);
    let mut Dirty = IRect::Empty;
    for Y in 0..Target.height() as i32 {
        for X in 0..Target.width() as i32 {
            let Amount = Clip.as_ref().map_or(1.0, |View| View.coverage(X, Y));
            if Amount <= 0.0 {
                continue;
            }
            let Before = Original.get(X as u32, Y as u32);
            let After = Processed.get(X as u32, Y as u32);
            let Mixed = if Amount >= 1.0 {
                After
            } else {
                blend::to_u8(blend::lerp_premul(blend::to_f32(Before), blend::to_f32(After), Amount))
            };
            if Mixed != Before {
                Target.put(X as u32, Y as u32, Mixed);
                Dirty = Dirty.union(&IRect::new(X, Y, 1, 1));
            }
        }
    }
    Dirty
}

fn map_pixels(Source: &Raster, Function: impl Fn([f32; 4], u32, u32) -> [f32; 4] + Sync) -> Raster {
    let mut Out = Source.clone();
    let Width = Source.width();
    Out.data_mut().par_chunks_mut(4).enumerate().for_each(|(Index, Pixel)| {
        let X = Index as u32 % Width.max(1);
        let Y = Index as u32 / Width.max(1);
        let Input = blend::to_f32([Pixel[0], Pixel[1], Pixel[2], Pixel[3]]);
        let Output = blend::to_u8(Function(Input, X, Y));
        Pixel.copy_from_slice(&Output);
    });
    Out
}

pub fn run_filter(Source: &Raster, Chosen: &Filter, Palette: &[Color]) -> Raster {
    match Chosen {
        Filter::Grayscale => map_pixels(Source, |[R, G, B, A], _, _| {
            let Luma = color::luminance(R, G, B);
            [Luma, Luma, Luma, A]
        }),
        Filter::Invert => map_pixels(Source, |[R, G, B, A], _, _| [1.0 - R, 1.0 - G, 1.0 - B, A]),
        Filter::Sepia => map_pixels(Source, |[R, G, B, A], _, _| {
            [
                (0.393 * R + 0.769 * G + 0.189 * B).min(1.0),
                (0.349 * R + 0.686 * G + 0.168 * B).min(1.0),
                (0.272 * R + 0.534 * G + 0.131 * B).min(1.0),
                A,
            ]
        }),
        Filter::BrightnessContrast { Brightness, Contrast } => {
            let Factor = (1.0 + Contrast.clamp(-1.0, 1.0)).max(0.0).powi(2);
            let Shift = Brightness.clamp(-1.0, 1.0);
            map_pixels(Source, move |[R, G, B, A], _, _| {
                let Adjust = |Value: f32| ((Value - 0.5) * Factor + 0.5 + Shift).clamp(0.0, 1.0);
                [Adjust(R), Adjust(G), Adjust(B), A]
            })
        }
        Filter::HueSaturation { Hue, Saturation, Lightness } => {
            let (HueShift, SaturationShift, LightShift) = (*Hue, *Saturation, *Lightness);
            map_pixels(Source, move |[R, G, B, A], _, _| {
                let (H, S, L) = color::rgb_to_hsl(R, G, B);
                let NewSaturation = if SaturationShift >= 0.0 { S + (1.0 - S) * SaturationShift } else { S * (1.0 + SaturationShift) };
                let NewLightness = if LightShift >= 0.0 { L + (1.0 - L) * LightShift } else { L * (1.0 + LightShift) };
                let (NewR, NewG, NewB) = color::hsl_to_rgb(H + HueShift, NewSaturation, NewLightness);
                [NewR, NewG, NewB, A]
            })
        }
        Filter::GaussianBlur { Radius } => gaussian_blur(Source, *Radius),
        Filter::BoxBlur { Radius } => {
            let Blurred = crate::brush::box_blur_for_filter(Source, *Radius as i32);
            from_premultiplied(Source.width(), Source.height(), &Blurred)
        }
        Filter::MotionBlur { Angle, Distance } => motion_blur(Source, *Angle, *Distance),
        Filter::Sharpen { Amount, Radius } => {
            let Soft = gaussian_blur(Source, *Radius);
            let Strength = *Amount;
            map_pixels(Source, move |[R, G, B, A], X, Y| {
                let Blurred = blend::to_f32(Soft.get(X, Y));
                [
                    (R + (R - Blurred[0]) * Strength).clamp(0.0, 1.0),
                    (G + (G - Blurred[1]) * Strength).clamp(0.0, 1.0),
                    (B + (B - Blurred[2]) * Strength).clamp(0.0, 1.0),
                    A,
                ]
            })
        }
        Filter::Posterize { Levels } => {
            let Steps = (*Levels).max(2) as f32 - 1.0;
            map_pixels(Source, move |[R, G, B, A], _, _| {
                [(R * Steps).round() / Steps, (G * Steps).round() / Steps, (B * Steps).round() / Steps, A]
            })
        }
        Filter::Threshold { Level } => {
            let Cut = *Level;
            map_pixels(Source, move |[R, G, B, A], _, _| {
                let Value = if color::luminance(R, G, B) >= Cut { 1.0 } else { 0.0 };
                [Value, Value, Value, A]
            })
        }
        Filter::Pixelate { Size } => pixelate(Source, (*Size).max(1)),
        Filter::Noise { Amount, Monochrome, Seed } => {
            let (Strength, Mono, SeedValue) = (*Amount, *Monochrome, *Seed);
            map_pixels(Source, move |[R, G, B, A], X, Y| {
                let Base = rng::hash_noise(X as i32, Y as i32, SeedValue) * 2.0 - 1.0;
                let Offsets = if Mono {
                    [Base; 3]
                } else {
                    [
                        Base,
                        rng::hash_noise(X as i32, Y as i32, SeedValue ^ 0xA5) * 2.0 - 1.0,
                        rng::hash_noise(X as i32, Y as i32, SeedValue ^ 0x5A) * 2.0 - 1.0,
                    ]
                };
                [
                    (R + Offsets[0] * Strength).clamp(0.0, 1.0),
                    (G + Offsets[1] * Strength).clamp(0.0, 1.0),
                    (B + Offsets[2] * Strength).clamp(0.0, 1.0),
                    A,
                ]
            })
        }
        Filter::Outline { Paint, Thickness, Outside, Diagonal } => outline(Source, *Paint, *Thickness, *Outside, *Diagonal),
        Filter::PaletteQuantize { Dither, ErrorDiffusion } => quantize(Source, Palette, *Dither, *ErrorDiffusion),
        Filter::ReplaceColor { From, To, Tolerance } => {
            let (Wanted, Replacement, Limit) = (From.to_rgba8(), To.to_rgba8(), *Tolerance);
            let mut Out = Source.clone();
            for Pixel in Out.pixels_mut() {
                if Pixel[3] > 0 && color_distance([Pixel[0], Pixel[1], Pixel[2], 255], [Wanted[0], Wanted[1], Wanted[2], 255]) <= Limit {
                    *Pixel = [Replacement[0], Replacement[1], Replacement[2], ((Pixel[3] as u32 * Replacement[3] as u32) / 255) as u8];
                }
            }
            Out
        }
        Filter::RemoveColor { Target, Tolerance } => {
            let (Wanted, Limit) = (Target.to_rgba8(), *Tolerance);
            let mut Out = Source.clone();
            for Pixel in Out.pixels_mut() {
                if color_distance([Pixel[0], Pixel[1], Pixel[2], 255], [Wanted[0], Wanted[1], Wanted[2], 255]) <= Limit {
                    *Pixel = [0, 0, 0, 0];
                }
            }
            Out
        }
        Filter::AlphaThreshold { Level } => {
            let Cut = (*Level * 255.0) as u8;
            let mut Out = Source.clone();
            for Pixel in Out.pixels_mut() {
                Pixel[3] = if Pixel[3] >= Cut.max(1) { 255 } else { 0 };
                if Pixel[3] == 0 {
                    *Pixel = [0, 0, 0, 0];
                }
            }
            Out
        }
        Filter::DropShadow { OffsetX, OffsetY, Blur, Paint } => {
            let mut Shadow = Raster::new(Source.width(), Source.height());
            let ShadowColor = Paint.to_rgba8();
            for Y in 0..Source.height() as i32 {
                for X in 0..Source.width() as i32 {
                    let Alpha = Source.get_checked(X - OffsetX, Y - OffsetY).map_or(0, |Pixel| Pixel[3]);
                    if Alpha > 0 {
                        let Combined = (Alpha as u32 * ShadowColor[3] as u32 / 255) as u8;
                        Shadow.put(X as u32, Y as u32, [ShadowColor[0], ShadowColor[1], ShadowColor[2], Combined]);
                    }
                }
            }
            let mut Out = if *Blur > 0.0 { gaussian_blur(&Shadow, *Blur) } else { Shadow };
            Out.draw_over(Source, 0, 0, 1.0, blend::BlendMode::Normal);
            Out
        }
        Filter::Vignette { Strength, Radius } => {
            let (Width, Height) = (Source.width() as f32, Source.height() as f32);
            let (Amount, Reach) = (*Strength, Radius.max(0.05));
            map_pixels(Source, move |[R, G, B, A], X, Y| {
                let Dx = (X as f32 + 0.5) / Width - 0.5;
                let Dy = (Y as f32 + 0.5) / Height - 0.5;
                let Distance = (Dx * Dx + Dy * Dy).sqrt() / std::f32::consts::FRAC_1_SQRT_2;
                let T = ((Distance - Reach) / (1.0 - Reach).max(0.01)).clamp(0.0, 1.0);
                let Factor = 1.0 - Amount * T * T;
                [R * Factor, G * Factor, B * Factor, A]
            })
        }
    }
}

fn from_premultiplied(Width: u32, Height: u32, Values: &[[f32; 4]]) -> Raster {
    let mut Out = Raster::new(Width, Height);
    for (Pixel, Value) in Out.pixels_mut().iter_mut().zip(Values) {
        *Pixel = blend::to_u8(blend::unpremul(*Value));
    }
    Out
}

pub fn gaussian_blur(Source: &Raster, Radius: f32) -> Raster {
    if Radius <= 0.0 {
        return Source.clone();
    }
    let Sigma = (Radius / 2.0).max(0.3);
    let Reach = (Sigma * 3.0).ceil() as i32;
    let mut Kernel: Vec<f32> = (-Reach..=Reach).map(|Offset| (-(Offset * Offset) as f32 / (2.0 * Sigma * Sigma)).exp()).collect();
    let Total: f32 = Kernel.iter().sum();
    for Weight in &mut Kernel {
        *Weight /= Total;
    }
    let (Width, Height) = (Source.width() as usize, Source.height() as usize);
    let Premultiplied: Vec<[f32; 4]> = Source.pixels().iter().map(|Pixel| blend::premultiply(blend::to_f32(*Pixel))).collect();
    let mut Horizontal = vec![[0f32; 4]; Width * Height];
    Horizontal.par_chunks_mut(Width.max(1)).enumerate().for_each(|(Y, Row)| {
        for X in 0..Width {
            let mut Sum = [0f32; 4];
            for (Tap, Weight) in Kernel.iter().enumerate() {
                let SampleX = (X as i32 + Tap as i32 - Reach).clamp(0, Width as i32 - 1) as usize;
                let Pixel = Premultiplied[Y * Width + SampleX];
                for Channel in 0..4 {
                    Sum[Channel] += Pixel[Channel] * Weight;
                }
            }
            Row[X] = Sum;
        }
    });
    let mut Vertical = vec![[0f32; 4]; Width * Height];
    Vertical.par_chunks_mut(Width.max(1)).enumerate().for_each(|(Y, Row)| {
        for X in 0..Width {
            let mut Sum = [0f32; 4];
            for (Tap, Weight) in Kernel.iter().enumerate() {
                let SampleY = (Y as i32 + Tap as i32 - Reach).clamp(0, Height as i32 - 1) as usize;
                let Pixel = Horizontal[SampleY * Width + X];
                for Channel in 0..4 {
                    Sum[Channel] += Pixel[Channel] * Weight;
                }
            }
            Row[X] = Sum;
        }
    });
    from_premultiplied(Source.width(), Source.height(), &Vertical)
}

fn motion_blur(Source: &Raster, Angle: f32, Distance: u32) -> Raster {
    let Steps = Distance.max(1) as i32;
    let (Sin, Cos) = Angle.to_radians().sin_cos();
    let mut Out = Raster::new(Source.width(), Source.height());
    let Width = Source.width();
    Out.data_mut().par_chunks_mut(4).enumerate().for_each(|(Index, Pixel)| {
        let X = (Index as u32 % Width.max(1)) as f32;
        let Y = (Index as u32 / Width.max(1)) as f32;
        let mut Sum = [0f32; 4];
        for Step in 0..=Steps {
            let Offset = Step as f32 - Steps as f32 * 0.5;
            let Sample = Source.get_clamped((X + Cos * Offset).round() as i32, (Y + Sin * Offset).round() as i32);
            let Value = blend::premultiply(blend::to_f32(Sample));
            for Channel in 0..4 {
                Sum[Channel] += Value[Channel];
            }
        }
        for Channel in &mut Sum {
            *Channel /= (Steps + 1) as f32;
        }
        Pixel.copy_from_slice(&blend::to_u8(blend::unpremul(Sum)));
    });
    Out
}

fn pixelate(Source: &Raster, Size: u32) -> Raster {
    let mut Out = Source.clone();
    let (Width, Height) = Source.size();
    for BlockY in (0..Height).step_by(Size as usize) {
        for BlockX in (0..Width).step_by(Size as usize) {
            let mut Sum = [0f32; 4];
            let mut Count = 0f32;
            for Y in BlockY..(BlockY + Size).min(Height) {
                for X in BlockX..(BlockX + Size).min(Width) {
                    let Value = blend::premultiply(blend::to_f32(Source.get(X, Y)));
                    for Channel in 0..4 {
                        Sum[Channel] += Value[Channel];
                    }
                    Count += 1.0;
                }
            }
            for Channel in &mut Sum {
                *Channel /= Count;
            }
            let Average = blend::to_u8(blend::unpremul(Sum));
            Out.fill_rect(IRect::new(BlockX as i32, BlockY as i32, Size as i32, Size as i32), Average);
        }
    }
    Out
}

fn outline(Source: &Raster, Paint: Color, Thickness: u32, Outside: bool, Diagonal: bool) -> Raster {
    let Fill = Paint.to_rgba8();
    let mut Current = Source.clone();
    for _ in 0..Thickness.max(1) {
        let Snapshot = Current.clone();
        let Solid = |X: i32, Y: i32| Snapshot.get_checked(X, Y).is_some_and(|Pixel| Pixel[3] >= 128);
        for Y in 0..Source.height() as i32 {
            for X in 0..Source.width() as i32 {
                let Cardinal = [(X - 1, Y), (X + 1, Y), (X, Y - 1), (X, Y + 1)];
                let Diag = [(X - 1, Y - 1), (X + 1, Y - 1), (X - 1, Y + 1), (X + 1, Y + 1)];
                let Neighbours = Cardinal.iter().chain(if Diagonal { Diag.iter() } else { [].iter() });
                if Outside {
                    if !Solid(X, Y) && Neighbours.clone().any(|(Nx, Ny)| Solid(*Nx, *Ny)) {
                        Current.put(X as u32, Y as u32, Fill);
                    }
                } else if Solid(X, Y) && Neighbours.clone().any(|(Nx, Ny)| !Solid(*Nx, *Ny)) {
                    Current.put(X as u32, Y as u32, Fill);
                }
            }
        }
    }
    Current
}

pub fn nearest_palette_color(Palette: &[Color], Pixel: [f32; 3]) -> Option<Color> {
    Palette
        .iter()
        .min_by(|Left, Right| {
            let Score = |Candidate: &Color| {
                let Dr = Candidate.R - Pixel[0];
                let Dg = Candidate.G - Pixel[1];
                let Db = Candidate.B - Pixel[2];
                0.299 * Dr * Dr + 0.587 * Dg * Dg + 0.114 * Db * Db
            };
            Score(Left).total_cmp(&Score(Right))
        })
        .copied()
}

fn quantize(Source: &Raster, Palette: &[Color], Dither: DitherPattern, ErrorDiffusion: bool) -> Raster {
    if Palette.is_empty() {
        return Source.clone();
    }
    let (Width, Height) = (Source.width() as usize, Source.height() as usize);
    let mut Working: Vec<[f32; 4]> = Source.pixels().iter().map(|Pixel| blend::to_f32(*Pixel)).collect();
    let mut Out = Source.clone();
    for Y in 0..Height {
        for X in 0..Width {
            let Index = Y * Width + X;
            let Pixel = Working[Index];
            if Pixel[3] <= 0.0 {
                continue;
            }
            let mut Wanted = [Pixel[0], Pixel[1], Pixel[2]];
            if Dither != DitherPattern::None {
                let Bias = Dither.threshold(X as i32, Y as i32) - 0.5;
                for Channel in &mut Wanted {
                    *Channel = (*Channel + Bias * 0.25).clamp(0.0, 1.0);
                }
            }
            let Chosen = nearest_palette_color(Palette, Wanted).unwrap_or(Color::Black);
            Out.put(X as u32, Y as u32, Color::rgba(Chosen.R, Chosen.G, Chosen.B, Pixel[3]).to_rgba8());
            if ErrorDiffusion {
                let Error = [Pixel[0] - Chosen.R, Pixel[1] - Chosen.G, Pixel[2] - Chosen.B];
                for (Dx, Dy, Weight) in [(1i32, 0i32, 7.0 / 16.0), (-1, 1, 3.0 / 16.0), (0, 1, 5.0 / 16.0), (1, 1, 1.0 / 16.0)] {
                    let (Nx, Ny) = (X as i32 + Dx, Y as i32 + Dy);
                    if Nx >= 0 && (Nx as usize) < Width && (Ny as usize) < Height {
                        let Target = &mut Working[Ny as usize * Width + Nx as usize];
                        for Channel in 0..3 {
                            Target[Channel] += Error[Channel] * Weight;
                        }
                    }
                }
            }
        }
    }
    Out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_filter_runs() {
        let mut Source = Raster::new(12, 10);
        Source.fill_rect(IRect::new(3, 3, 5, 4), [200, 100, 50, 255]);
        for Chosen in Filter::catalogue() {
            let Result = run_filter(&Source, &Chosen, &crate::project::default_palette());
            assert_eq!(Result.size(), Source.size(), "{}", Chosen.name());
        }
    }

    #[test]
    fn grayscale_with_selection_clip() {
        let mut Source = Raster::filled(4, 1, [255, 0, 0, 255]);
        let Selected = crate::selection::Mask::from_rect(4, 1, IRect::new(0, 0, 2, 1));
        apply_filter(&mut Source, &Filter::Grayscale, Some(MaskView::identity(&Selected)), &[]);
        assert_eq!(Source.get(0, 0)[0], Source.get(0, 0)[1]);
        assert_eq!(Source.get(3, 0), [255, 0, 0, 255]);
    }

    #[test]
    fn outline_adds_border() {
        let mut Source = Raster::new(5, 5);
        Source.put(2, 2, [255, 255, 255, 255]);
        let Result = run_filter(&Source, &Filter::Outline { Paint: Color::Black, Thickness: 1, Outside: true, Diagonal: false }, &[]);
        assert_eq!(Result.get(1, 2), [0, 0, 0, 255]);
        assert_eq!(Result.get(1, 1), [0, 0, 0, 0]);
        assert_eq!(Result.get(2, 2), [255, 255, 255, 255]);
    }

    #[test]
    fn quantize_snaps_to_palette() {
        let Source = Raster::filled(2, 2, [250, 10, 10, 255]);
        let Result = run_filter(&Source, &Filter::PaletteQuantize { Dither: DitherPattern::None, ErrorDiffusion: false }, &[Color::rgb(1.0, 0.0, 0.0), Color::Black]);
        assert_eq!(Result.get(0, 0), [255, 0, 0, 255]);
    }
}
