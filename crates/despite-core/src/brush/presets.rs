use super::{AdvancedBrush, Brush, BrushMode, DitherPattern, TipAnimation, TipImage, TipShape};
use crate::color::Color;

fn pixel(Name: &str, Size: f32, Tip: TipShape) -> Brush {
    Brush {
        Name: Name.into(),
        Tip,
        Size,
        Hardness: 1.0,
        Spacing: 0.1,
        Antialias: false,
        PixelPerfect: Size <= 1.0,
        PressureSize: false,
        ..Default::default()
    }
}

pub fn default_presets() -> Vec<Brush> {
    vec![
        pixel("Pixel Pencil", 1.0, TipShape::Square),
        pixel("Pixel Square 2", 2.0, TipShape::Square),
        pixel("Pixel Square 3", 3.0, TipShape::Square),
        pixel("Pixel Circle 4", 4.0, TipShape::Round),
        pixel("Pixel Circle 6", 6.0, TipShape::Round),
        pixel("Pixel Plus 3", 3.0, TipShape::Plus),
        pixel("Pixel Diamond 5", 5.0, TipShape::Diamond),
        Brush { Name: "Pixel Eraser".into(), Mode: BrushMode::Erase, ..pixel("", 1.0, TipShape::Square) },
        Brush {
            Name: "Pixel Dither".into(),
            Dither: DitherPattern::Bayer4,
            Flow: 0.5,
            ..pixel("", 4.0, TipShape::Square)
        },
        Brush { Name: "Pixel Shade Lighter".into(), Mode: BrushMode::ShadeLighter, ..pixel("", 1.0, TipShape::Square) },
        Brush { Name: "Pixel Shade Darker".into(), Mode: BrushMode::ShadeDarker, ..pixel("", 1.0, TipShape::Square) },
        Brush { Name: "Color Replace".into(), Mode: BrushMode::ColorReplace, PixelPerfect: false, ..pixel("", 3.0, TipShape::Square) },
        Brush { Name: "Round".into(), ..Default::default() },
        Brush { Name: "Ink".into(), Size: 6.0, Hardness: 0.95, MinSizeRatio: 0.05, Smoothing: 0.4, ..Default::default() },
        Brush { Name: "Marker".into(), Size: 18.0, Hardness: 0.7, Opacity: 0.6, Roundness: 0.5, Angle: 35.0, PressureSize: false, ..Default::default() },
        Brush { Name: "Airbrush".into(), Size: 60.0, Hardness: 0.0, Flow: 0.08, Spacing: 0.05, PressureFlow: true, PressureSize: false, ..Default::default() },
        Brush {
            Name: "Spray".into(),
            Size: 3.0,
            Hardness: 1.0,
            Scatter: 6.0,
            DabCount: 8,
            Spacing: 0.5,
            SizeJitter: 0.5,
            PressureSize: false,
            ..Default::default()
        },
        Brush {
            Name: "Chalk".into(),
            Size: 22.0,
            Hardness: 0.6,
            Flow: 0.5,
            Roundness: 0.6,
            AngleJitter: 1.0,
            SizeJitter: 0.3,
            OpacityJitter: 0.6,
            Spacing: 0.2,
            ..Default::default()
        },
        Brush { Name: "Eraser".into(), Mode: BrushMode::Erase, Size: 24.0, Hardness: 0.6, PressureSize: false, ..Default::default() },
        Brush { Name: "Soft Eraser".into(), Mode: BrushMode::Erase, Size: 48.0, Hardness: 0.0, Flow: 0.3, PressureSize: false, ..Default::default() },
        Brush { Name: "Blur".into(), Mode: BrushMode::Blur, Size: 30.0, Hardness: 0.3, Strength: 0.6, PressureSize: false, Spacing: 0.25, ..Default::default() },
        Brush { Name: "Smudge".into(), Mode: BrushMode::Smudge, Size: 24.0, Hardness: 0.4, Strength: 0.7, Spacing: 0.08, PressureSize: false, ..Default::default() },
        Brush { Name: "Sharpen".into(), Mode: BrushMode::Sharpen, Size: 24.0, Hardness: 0.5, Strength: 0.3, Spacing: 0.3, PressureSize: false, ..Default::default() },
        Brush { Name: "Grayscale".into(), Mode: BrushMode::Grayscale, Size: 30.0, Hardness: 0.5, PressureSize: false, ..Default::default() },
        Brush { Name: "Desaturate".into(), Mode: BrushMode::Desaturate, Size: 30.0, Hardness: 0.5, Strength: 0.5, PressureSize: false, ..Default::default() },
        Brush { Name: "Saturate".into(), Mode: BrushMode::Saturate, Size: 30.0, Hardness: 0.5, Strength: 0.4, PressureSize: false, ..Default::default() },
        Brush { Name: "Dodge".into(), Mode: BrushMode::Lighten, Size: 30.0, Hardness: 0.2, Strength: 0.25, PressureSize: false, ..Default::default() },
        Brush { Name: "Burn".into(), Mode: BrushMode::Darken, Size: 30.0, Hardness: 0.2, Strength: 0.25, PressureSize: false, ..Default::default() },
        Brush { Name: "Hue Shift".into(), Mode: BrushMode::HueShift, Size: 30.0, Hardness: 0.5, Strength: 0.25, PressureSize: false, ..Default::default() },
        Brush { Name: "Invert".into(), Mode: BrushMode::Invert, Size: 30.0, Hardness: 0.8, PressureSize: false, ..Default::default() },
        Brush { Name: "Pixelate".into(), Mode: BrushMode::Pixelate, Size: 40.0, Hardness: 1.0, Strength: 0.4, PressureSize: false, ..Default::default() },
    ]
}

fn star_frame(Points: u32, Rotation: f32, Side: u32) -> TipImage {
    let mut Alpha = vec![0u8; (Side * Side) as usize];
    let Center = Side as f32 * 0.5;
    for Y in 0..Side {
        for X in 0..Side {
            let Dx = X as f32 + 0.5 - Center;
            let Dy = Y as f32 + 0.5 - Center;
            let Radius = (Dx * Dx + Dy * Dy).sqrt() / Center;
            let Angle = Dy.atan2(Dx) + Rotation;
            let Spike = 0.45 + 0.55 * (Angle * Points as f32).cos().abs().powf(6.0);
            let Value = ((Spike - Radius) * 6.0).clamp(0.0, 1.0);
            Alpha[(Y * Side + X) as usize] = (Value * 255.0) as u8;
        }
    }
    TipImage { Width: Side, Height: Side, Alpha }
}

fn ring_frame(Thickness: f32, Side: u32) -> TipImage {
    let mut Alpha = vec![0u8; (Side * Side) as usize];
    let Center = Side as f32 * 0.5;
    for Y in 0..Side {
        for X in 0..Side {
            let Dx = X as f32 + 0.5 - Center;
            let Dy = Y as f32 + 0.5 - Center;
            let Radius = (Dx * Dx + Dy * Dy).sqrt() / Center;
            let Value = (1.0 - ((Radius - 0.7).abs() / Thickness)).clamp(0.0, 1.0);
            Alpha[(Y * Side + X) as usize] = (Value * 255.0) as u8;
        }
    }
    TipImage { Width: Side, Height: Side, Alpha }
}

pub fn advanced_presets() -> Vec<Brush> {
    let StarFrames: Vec<TipImage> = (0..6).map(|Index| star_frame(5, Index as f32 * 0.2, 32)).collect();
    let RingFrames: Vec<TipImage> = (0..5).map(|Index| ring_frame(0.08 + Index as f32 * 0.06, 32)).collect();
    vec![
        Brush {
            Name: "Rainbow Ribbon".into(),
            Size: 14.0,
            Hardness: 0.9,
            Spacing: 0.05,
            Advanced: AdvancedBrush { ColorCycle: 90.0, ..Default::default() },
            ..Default::default()
        },
        Brush {
            Name: "Wiggle Line".into(),
            Size: 5.0,
            Hardness: 1.0,
            Spacing: 0.05,
            PressureSize: false,
            Advanced: AdvancedBrush { WiggleAmplitude: 8.0, WiggleWavelength: 30.0, ..Default::default() },
            ..Default::default()
        },
        Brush {
            Name: "Twinkle Stars".into(),
            Tip: TipShape::Image,
            Size: 26.0,
            Spacing: 1.2,
            Scatter: 0.8,
            SizeJitter: 0.5,
            PressureSize: false,
            Advanced: AdvancedBrush {
                AnimatedTips: StarFrames,
                Animation: TipAnimation::Random,
                RotationSpin: 17.0,
                SparkleChance: 0.08,
                ..Default::default()
            },
            ..Default::default()
        },
        Brush {
            Name: "Pulse Rings".into(),
            Tip: TipShape::Image,
            Size: 30.0,
            Spacing: 0.6,
            PressureSize: false,
            Advanced: AdvancedBrush { AnimatedTips: RingFrames, Animation: TipAnimation::ByDistance, AnimationStep: 12.0, ..Default::default() },
            ..Default::default()
        },
        Brush {
            Name: "Echo Trail".into(),
            Size: 10.0,
            Hardness: 0.8,
            Spacing: 0.1,
            Advanced: AdvancedBrush { EchoCount: 4, EchoOffsetX: 8.0, EchoOffsetY: 5.0, EchoFade: 0.55, ..Default::default() },
            ..Default::default()
        },
        Brush {
            Name: "Fading Comet".into(),
            Size: 20.0,
            Hardness: 0.3,
            Spacing: 0.05,
            PressureSize: false,
            Advanced: AdvancedBrush { FadeLength: 400.0, GradientEnd: Some(Color::rgb(1.0, 0.85, 0.2)), GradientLength: 300.0, ..Default::default() },
            ..Default::default()
        },
        Brush {
            Name: "Shaky Pencil (boil)".into(),
            Size: 3.0,
            Hardness: 0.85,
            Opacity: 1.0,
            Flow: 0.95,
            Spacing: 0.12,
            MinSizeRatio: 0.45,
            PressureSize: true,
            Smoothing: 0.25,
            Advanced: AdvancedBrush { BoilFrames: 3, BoilJitter: 1.1, BoilWavelength: 14.0, Grain: 0.3, GrainScale: 1.0, ..Default::default() },
            ..Default::default()
        },
        Brush {
            Name: "Wobbly Ink (boil)".into(),
            Size: 4.0,
            Hardness: 0.95,
            Spacing: 0.08,
            MinSizeRatio: 0.3,
            Smoothing: 0.35,
            Advanced: AdvancedBrush { BoilFrames: 3, BoilJitter: 1.6, BoilWavelength: 24.0, ..Default::default() },
            ..Default::default()
        },
        Brush {
            Name: "Crayon Scribble (boil)".into(),
            Size: 9.0,
            Hardness: 0.6,
            Flow: 0.9,
            Spacing: 0.1,
            AngleJitter: 0.3,
            Advanced: AdvancedBrush { BoilFrames: 4, BoilJitter: 1.4, BoilWavelength: 18.0, Grain: 0.6, GrainScale: 1.5, ..Default::default() },
            ..Default::default()
        },
        Brush {
            Name: "Confetti".into(),
            Tip: TipShape::Square,
            Size: 6.0,
            Scatter: 4.0,
            DabCount: 3,
            Spacing: 0.8,
            AngleJitter: 1.0,
            Roundness: 0.5,
            HueJitter: 1.0,
            SaturationJitter: 0.2,
            PressureSize: false,
            Advanced: AdvancedBrush { SparkleChance: 0.05, SparkleScale: 1.8, ..Default::default() },
            ..Default::default()
        },
    ]
}
