use rayon::prelude::*;

use super::model::{EffectKind, NodeId};
use crate::blend::{self, BlendMode};
use crate::color::{self, Color};
use crate::filters::gaussian_blur;
use crate::raster::Raster;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timing {
    pub Frame: f32,
    pub Time: f32,
    pub Fps: f32,
    pub Length: u32,
}

pub trait EffectHook {
    fn shader(&mut self, Shader: u64, Node: NodeId, Params: &[f32], Input: &Raster, Clock: Timing) -> Option<Raster>;
}

fn param(Params: &[f32], Index: usize, Kind: EffectKind) -> f32 {
    Params.get(Index).copied().unwrap_or_else(|| Kind.params().get(Index).map(|Entry| Entry.Default).unwrap_or(0.0))
}

fn tinted_alpha(Source: &Raster, Paint: Color, Opacity: f32) -> Raster {
    let [Red, Green, Blue, Alpha] = Paint.to_rgba8();
    let mut Out = Raster::new(Source.width(), Source.height());
    for (Target, Pixel) in Out.pixels_mut().iter_mut().zip(Source.pixels()) {
        let Value = (Pixel[3] as f32 * Alpha as f32 / 255.0 * Opacity.clamp(0.0, 1.0)).round() as u8;
        *Target = [Red, Green, Blue, Value];
    }
    Out
}

fn under(Base: &Raster, Top: &Raster) -> Raster {
    let mut Out = Base.clone();
    Out.draw_over(Top, 0, 0, 1.0, BlendMode::Normal);
    Out
}

fn shifted(Source: &Raster, Dx: i32, Dy: i32) -> Raster {
    let mut Out = Raster::new(Source.width(), Source.height());
    Out.blit(Source, Dx, Dy);
    Out
}

fn color_adjust(Target: &mut Raster, Brightness: f32, Contrast: f32, Saturation: f32, Hue: f32) {
    let ContrastFactor = (1.0 + Contrast).max(0.0);
    let SaturationFactor = (1.0 + Saturation).max(0.0);
    Target.pixels_mut().par_iter_mut().for_each(|Pixel| {
        if Pixel[3] == 0 {
            return;
        }
        let mut Red = Pixel[0] as f32 / 255.0;
        let mut Green = Pixel[1] as f32 / 255.0;
        let mut Blue = Pixel[2] as f32 / 255.0;
        if Hue.abs() > 1e-3 || (SaturationFactor - 1.0).abs() > 1e-3 {
            let (H, S, V) = color::rgb_to_hsv(Red, Green, Blue);
            (Red, Green, Blue) = color::hsv_to_rgb(H + Hue, (S * SaturationFactor).clamp(0.0, 1.0), V);
        }
        let Adjust = |Value: f32| ((Value - 0.5) * ContrastFactor + 0.5 + Brightness).clamp(0.0, 1.0);
        Pixel[0] = color::to_u8(Adjust(Red));
        Pixel[1] = color::to_u8(Adjust(Green));
        Pixel[2] = color::to_u8(Adjust(Blue));
    });
}

fn pixelate(Source: &Raster, Block: u32) -> Raster {
    let Block = Block.max(1);
    if Block == 1 {
        return Source.clone();
    }
    let (Width, Height) = Source.size();
    let mut Out = Raster::new(Width, Height);
    let mut BlockY = 0;
    while BlockY < Height {
        let mut BlockX = 0;
        while BlockX < Width {
            let mut Sum = [0f32; 4];
            let mut Count = 0f32;
            for Y in BlockY..(BlockY + Block).min(Height) {
                for X in BlockX..(BlockX + Block).min(Width) {
                    let Premultiplied = blend::premultiply(blend::to_f32(Source.get(X, Y)));
                    for Channel in 0..4 {
                        Sum[Channel] += Premultiplied[Channel];
                    }
                    Count += 1.0;
                }
            }
            let Average = blend::to_u8(blend::unpremul(Sum.map(|Value| Value / Count.max(1.0))));
            for Y in BlockY..(BlockY + Block).min(Height) {
                for X in BlockX..(BlockX + Block).min(Width) {
                    Out.put(X, Y, Average);
                }
            }
            BlockX += Block;
        }
        BlockY += Block;
    }
    Out
}

fn chromatic(Source: &Raster, Offset: f32) -> Raster {
    let (Width, Height) = Source.size();
    let mut Out = Raster::new(Width, Height);
    let Shift = Offset.round() as i32;
    for Y in 0..Height {
        for X in 0..Width {
            let Red = Source.get_checked(X as i32 - Shift, Y as i32).unwrap_or([0; 4]);
            let Middle = Source.get(X, Y);
            let Blue = Source.get_checked(X as i32 + Shift, Y as i32).unwrap_or([0; 4]);
            let Alpha = Red[3].max(Middle[3]).max(Blue[3]);
            if Alpha == 0 {
                continue;
            }
            let Scale = |Pixel: [u8; 4], Index: usize| (Pixel[Index] as u32 * Pixel[3] as u32 / Alpha as u32).min(255) as u8;
            Out.put(X, Y, [Scale(Red, 0), Scale(Middle, 1), Scale(Blue, 2), Alpha]);
        }
    }
    Out
}

fn wave(Source: &Raster, Amplitude: f32, Wavelength: f32, Speed: f32, Time: f32) -> Raster {
    let (Width, Height) = Source.size();
    let mut Out = Raster::new(Width, Height);
    let Wavelength = Wavelength.max(1.0);
    Out.data_mut().par_chunks_mut(Width as usize * 4).enumerate().for_each(|(Y, Row)| {
        let Offset = Amplitude * (std::f32::consts::TAU * (Y as f32 / Wavelength + Speed * Time)).sin();
        for X in 0..Width as usize {
            let Pixel = blend::to_u8(Source.sample_bilinear(X as f32 + 0.5 - Offset, Y as f32 + 0.5));
            Row[X * 4..X * 4 + 4].copy_from_slice(&Pixel);
        }
    });
    Out
}

fn posterize(Target: &mut Raster, Levels: f32) {
    let Steps = Levels.round().clamp(2.0, 255.0) - 1.0;
    Target.pixels_mut().par_iter_mut().for_each(|Pixel| {
        for Channel in 0..3 {
            let Value = Pixel[Channel] as f32 / 255.0;
            Pixel[Channel] = color::to_u8((Value * Steps).round() / Steps);
        }
    });
}

fn outline(Source: &Raster, Width: f32, Paint: Color) -> Raster {
    let Radius = Width.round().clamp(0.0, 64.0) as i32;
    if Radius == 0 {
        return Source.clone();
    }
    let (ImageWidth, ImageHeight) = Source.size();
    let Alpha: Vec<u8> = Source.pixels().iter().map(|Pixel| Pixel[3]).collect();
    let mut Horizontal = vec![0u8; Alpha.len()];
    Horizontal.par_chunks_mut(ImageWidth as usize).enumerate().for_each(|(Y, Row)| {
        for X in 0..ImageWidth as i32 {
            let mut Highest = 0;
            for Dx in -Radius..=Radius {
                let Sample = X + Dx;
                if Sample >= 0 && Sample < ImageWidth as i32 {
                    Highest = Highest.max(Alpha[Y * ImageWidth as usize + Sample as usize]);
                }
            }
            Row[X as usize] = Highest;
        }
    });
    let mut Grown = Raster::new(ImageWidth, ImageHeight);
    let [Red, Green, Blue, PaintAlpha] = Paint.to_rgba8();
    Grown.data_mut().par_chunks_mut(ImageWidth as usize * 4).enumerate().for_each(|(Y, Row)| {
        for X in 0..ImageWidth as usize {
            let mut Highest = 0;
            for Dy in -Radius..=Radius {
                let Sample = Y as i32 + Dy;
                if Sample >= 0 && Sample < ImageHeight as i32 {
                    Highest = Highest.max(Horizontal[Sample as usize * ImageWidth as usize + X]);
                }
            }
            let Value = (Highest as u32 * PaintAlpha as u32 / 255) as u8;
            Row[X * 4..X * 4 + 4].copy_from_slice(&[Red, Green, Blue, Value]);
        }
    });
    under(&Grown, Source)
}

fn glow(Source: &Raster, Radius: f32, Strength: f32, Threshold: f32, Paint: Color) -> Raster {
    let mut Bright = Raster::new(Source.width(), Source.height());
    for (Target, Pixel) in Bright.pixels_mut().iter_mut().zip(Source.pixels()) {
        let Light = color::luminance(Pixel[0] as f32 / 255.0, Pixel[1] as f32 / 255.0, Pixel[2] as f32 / 255.0);
        if Light >= Threshold && Pixel[3] > 0 {
            *Target = [
                color::to_u8(Pixel[0] as f32 / 255.0 * Paint.R),
                color::to_u8(Pixel[1] as f32 / 255.0 * Paint.G),
                color::to_u8(Pixel[2] as f32 / 255.0 * Paint.B),
                Pixel[3],
            ];
        }
    }
    let Blurred = gaussian_blur(&Bright, Radius);
    let mut Out = Source.clone();
    let Amount = (Strength * Paint.A).max(0.0);
    let mut Remaining = Amount;
    while Remaining > 0.0 {
        Out.draw_over(&Blurred, 0, 0, Remaining.min(1.0), BlendMode::Add);
        Remaining -= 1.0;
    }
    Out
}

pub fn apply(Kind: EffectKind, Node: NodeId, Params: &[f32], Paint: Color, Clock: Timing, Target: &mut Raster, Hook: Option<&mut dyn EffectHook>) {
    let Time = Clock.Time;
    let Value = |Index: usize| param(Params, Index, Kind);
    match Kind {
        EffectKind::Blur => *Target = gaussian_blur(Target, Value(0).max(0.0)),
        EffectKind::Glow => *Target = glow(Target, Value(0).max(0.0), Value(1), Value(2), Paint),
        EffectKind::DropShadow => {
            let Shadow = tinted_alpha(Target, Paint, Value(3));
            let Moved = shifted(&Shadow, Value(0).round() as i32, Value(1).round() as i32);
            let Soft = gaussian_blur(&Moved, Value(2).max(0.0));
            *Target = under(&Soft, Target);
        }
        EffectKind::Outline => *Target = outline(Target, Value(0), Paint),
        EffectKind::ColorAdjust => color_adjust(Target, Value(0), Value(1), Value(2), Value(3)),
        EffectKind::Pixelate => *Target = pixelate(Target, Value(0).round().max(1.0) as u32),
        EffectKind::ChromaticAberration => *Target = chromatic(Target, Value(0)),
        EffectKind::Wave => *Target = wave(Target, Value(0), Value(1), Value(2), Time),
        EffectKind::Posterize => posterize(Target, Value(0)),
        EffectKind::Shader { Shader } => {
            if let Some(Runner) = Hook
                && let Some(Result) = Runner.shader(Shader, Node, Params, Target, Clock)
            {
                *Target = if Result.size() == Target.size() { Result } else { Result.scaled_smooth(Target.width(), Target.height()) };
            }
        }
    }
}
