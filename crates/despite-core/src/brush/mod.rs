pub mod pack;
mod presets;
pub mod shapes;
mod stroke;

use serde::{Deserialize, Serialize};

use crate::blend::BlendMode;
use crate::color::Color;
use crate::raster::Raster;

pub use pack::{BrushPack, PackExtension};
pub use presets::{advanced_presets, default_presets};
pub use stroke::{Stroke, StrokeContext, StrokePoint, box_blur_region, shade_with_palette};

pub fn box_blur_for_filter(Source: &Raster, Radius: i32) -> Vec<[f32; 4]> {
    if Radius <= 0 {
        return Source.pixels().iter().map(|Pixel| crate::blend::premultiply(crate::blend::to_f32(*Pixel))).collect();
    }
    box_blur_region(Source, Source.bounds(), Radius)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BrushMode {
    #[default]
    Paint,
    Erase,
    Blur,
    Smudge,
    Sharpen,
    Grayscale,
    Desaturate,
    Saturate,
    Lighten,
    Darken,
    Invert,
    HueShift,
    Pixelate,
    ColorReplace,
    ShadeLighter,
    ShadeDarker,
}

impl BrushMode {
    pub const All: &'static [BrushMode] = &[
        BrushMode::Paint,
        BrushMode::Erase,
        BrushMode::Blur,
        BrushMode::Smudge,
        BrushMode::Sharpen,
        BrushMode::Grayscale,
        BrushMode::Desaturate,
        BrushMode::Saturate,
        BrushMode::Lighten,
        BrushMode::Darken,
        BrushMode::Invert,
        BrushMode::HueShift,
        BrushMode::Pixelate,
        BrushMode::ColorReplace,
        BrushMode::ShadeLighter,
        BrushMode::ShadeDarker,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BrushMode::Paint => "Paint",
            BrushMode::Erase => "Erase",
            BrushMode::Blur => "Blur",
            BrushMode::Smudge => "Smudge",
            BrushMode::Sharpen => "Sharpen",
            BrushMode::Grayscale => "Grayscale",
            BrushMode::Desaturate => "Desaturate",
            BrushMode::Saturate => "Saturate",
            BrushMode::Lighten => "Lighten (Dodge)",
            BrushMode::Darken => "Darken (Burn)",
            BrushMode::Invert => "Invert",
            BrushMode::HueShift => "Hue Shift",
            BrushMode::Pixelate => "Pixelate",
            BrushMode::ColorReplace => "Color Replace",
            BrushMode::ShadeLighter => "Shade Lighter (Palette)",
            BrushMode::ShadeDarker => "Shade Darker (Palette)",
        }
    }

    pub fn is_live(self) -> bool {
        matches!(self, BrushMode::Blur | BrushMode::Smudge | BrushMode::Sharpen)
    }

    pub fn uses_strength(self) -> bool {
        !matches!(self, BrushMode::Paint | BrushMode::Erase | BrushMode::Grayscale | BrushMode::Invert)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipShape {
    #[default]
    Round,
    Square,
    Plus,
    Diamond,
    Image,
}

impl TipShape {
    pub const All: &'static [TipShape] = &[TipShape::Round, TipShape::Square, TipShape::Plus, TipShape::Diamond, TipShape::Image];

    pub fn name(self) -> &'static str {
        match self {
            TipShape::Round => "Round",
            TipShape::Square => "Square",
            TipShape::Plus => "Plus",
            TipShape::Diamond => "Diamond",
            TipShape::Image => "Image",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DitherPattern {
    #[default]
    None,
    Checker,
    Bayer2,
    Bayer4,
    Bayer8,
    Lines,
}

impl DitherPattern {
    pub const All: &'static [DitherPattern] =
        &[DitherPattern::None, DitherPattern::Checker, DitherPattern::Bayer2, DitherPattern::Bayer4, DitherPattern::Bayer8, DitherPattern::Lines];

    pub fn name(self) -> &'static str {
        match self {
            DitherPattern::None => "None",
            DitherPattern::Checker => "Checker",
            DitherPattern::Bayer2 => "Bayer 2x2",
            DitherPattern::Bayer4 => "Bayer 4x4",
            DitherPattern::Bayer8 => "Bayer 8x8",
            DitherPattern::Lines => "Lines",
        }
    }

    pub fn threshold(self, X: i32, Y: i32) -> f32 {
        const Matrix2: [[u8; 2]; 2] = [[0, 2], [3, 1]];
        const Matrix4: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
        let (Px, Py) = (X.rem_euclid(8) as usize, Y.rem_euclid(8) as usize);
        match self {
            DitherPattern::None => 0.0,
            DitherPattern::Checker => {
                if (X + Y).rem_euclid(2) == 0 { 0.25 } else { 0.75 }
            }
            DitherPattern::Bayer2 => (Matrix2[Py % 2][Px % 2] as f32 + 0.5) / 4.0,
            DitherPattern::Bayer4 => (Matrix4[Py % 4][Px % 4] as f32 + 0.5) / 16.0,
            DitherPattern::Bayer8 => {
                let Coarse = Matrix4[(Py / 2) % 4][(Px / 2) % 4] as u32;
                let Fine = Matrix2[Py % 2][Px % 2] as u32;
                ((Fine * 16 + Coarse) as f32 + 0.5) / 64.0
            }
            DitherPattern::Lines => {
                if Y.rem_euclid(2) == 0 { 0.25 } else { 0.75 }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymmetryMode {
    #[default]
    None,
    Vertical,
    Horizontal,
    Both,
    Radial,
    Kaleidoscope,
}

impl SymmetryMode {
    pub const All: &'static [SymmetryMode] = &[
        SymmetryMode::None,
        SymmetryMode::Vertical,
        SymmetryMode::Horizontal,
        SymmetryMode::Both,
        SymmetryMode::Radial,
        SymmetryMode::Kaleidoscope,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SymmetryMode::None => "Off",
            SymmetryMode::Vertical => "Vertical Axis",
            SymmetryMode::Horizontal => "Horizontal Axis",
            SymmetryMode::Both => "Both Axes",
            SymmetryMode::Radial => "Radial",
            SymmetryMode::Kaleidoscope => "Kaleidoscope",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Symmetry {
    pub Mode: SymmetryMode,
    pub CenterX: f32,
    pub CenterY: f32,
    pub Segments: u32,
    pub ShowGuide: bool,
}

impl Default for Symmetry {
    fn default() -> Self {
        Self { Mode: SymmetryMode::None, CenterX: 0.0, CenterY: 0.0, Segments: 6, ShowGuide: true }
    }
}

impl Symmetry {
    pub fn mirror_points(&self, X: f32, Y: f32) -> Vec<(f32, f32, f32, bool)> {
        let (Cx, Cy) = (self.CenterX, self.CenterY);
        let mut Out = vec![(X, Y, 0.0, false)];
        match self.Mode {
            SymmetryMode::None => {}
            SymmetryMode::Vertical => Out.push((2.0 * Cx - X, Y, 0.0, true)),
            SymmetryMode::Horizontal => Out.push((X, 2.0 * Cy - Y, 180.0, true)),
            SymmetryMode::Both => {
                Out.push((2.0 * Cx - X, Y, 0.0, true));
                Out.push((X, 2.0 * Cy - Y, 180.0, true));
                Out.push((2.0 * Cx - X, 2.0 * Cy - Y, 180.0, false));
            }
            SymmetryMode::Radial | SymmetryMode::Kaleidoscope => {
                let Count = self.Segments.clamp(2, 64);
                let (Dx, Dy) = (X - Cx, Y - Cy);
                for Step in 1..Count {
                    let Degrees = 360.0 * Step as f32 / Count as f32;
                    let (Sin, Cos) = Degrees.to_radians().sin_cos();
                    Out.push((Cx + Dx * Cos - Dy * Sin, Cy + Dx * Sin + Dy * Cos, Degrees, false));
                }
                if self.Mode == SymmetryMode::Kaleidoscope {
                    let Mirrored: Vec<_> = Out.iter().map(|(PointX, PointY, Rotation, _)| (2.0 * Cx - PointX, *PointY, -Rotation, true)).collect();
                    Out.extend(Mirrored);
                }
            }
        }
        Out
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TipImage {
    pub Width: u32,
    pub Height: u32,
    #[serde(with = "base64_bytes")]
    pub Alpha: Vec<u8>,
}

impl TipImage {
    pub fn from_raster(Source: &Raster, UseLuminance: bool) -> TipImage {
        let mut Alpha = Vec::with_capacity(Source.width() as usize * Source.height() as usize);
        for Pixel in Source.pixels() {
            let Value = if UseLuminance {
                let Luma = crate::color::luminance(Pixel[0] as f32, Pixel[1] as f32, Pixel[2] as f32) / 255.0;
                ((1.0 - Luma) * Pixel[3] as f32) as u8
            } else {
                Pixel[3]
            };
            Alpha.push(Value);
        }
        TipImage { Width: Source.width(), Height: Source.height(), Alpha }
    }

    #[inline]
    pub fn sample(&self, U: f32, V: f32, Smooth: bool) -> f32 {
        if self.Width == 0 || self.Height == 0 {
            return 0.0;
        }
        let X = U * self.Width as f32;
        let Y = V * self.Height as f32;
        let Read = |Px: i32, Py: i32| -> f32 {
            if Px < 0 || Py < 0 || Px >= self.Width as i32 || Py >= self.Height as i32 {
                0.0
            } else {
                self.Alpha[Py as usize * self.Width as usize + Px as usize] as f32 / 255.0
            }
        };
        if !Smooth {
            return Read(X.floor() as i32, Y.floor() as i32);
        }
        let (Fx, Fy) = (X - 0.5, Y - 0.5);
        let (X0, Y0) = (Fx.floor(), Fy.floor());
        let (Tx, Ty) = (Fx - X0, Fy - Y0);
        let (X0, Y0) = (X0 as i32, Y0 as i32);
        let Top = Read(X0, Y0) * (1.0 - Tx) + Read(X0 + 1, Y0) * Tx;
        let Bottom = Read(X0, Y0 + 1) * (1.0 - Tx) + Read(X0 + 1, Y0 + 1) * Tx;
        Top * (1.0 - Ty) + Bottom * Ty
    }
}

mod base64_bytes {
    use base64::Engine;
    use serde::Deserialize;

    pub fn serialize<S: serde::Serializer>(Bytes: &[u8], Output: S) -> Result<S::Ok, S::Error> {
        Output.serialize_str(&base64::engine::general_purpose::STANDARD.encode(Bytes))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(Input: D) -> Result<Vec<u8>, D::Error> {
        let Text = String::deserialize(Input)?;
        base64::engine::general_purpose::STANDARD.decode(Text).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipAnimation {
    #[default]
    Sequential,
    Random,
    ByDistance,
    ByPressure,
}

impl TipAnimation {
    pub const All: &'static [TipAnimation] =
        &[TipAnimation::Sequential, TipAnimation::Random, TipAnimation::ByDistance, TipAnimation::ByPressure];

    pub fn name(self) -> &'static str {
        match self {
            TipAnimation::Sequential => "Sequential",
            TipAnimation::Random => "Random",
            TipAnimation::ByDistance => "By Distance",
            TipAnimation::ByPressure => "By Pressure",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdvancedBrush {
    pub AnimatedTips: Vec<TipImage>,
    pub Animation: TipAnimation,
    pub AnimationStep: f32,
    pub ColorCycle: f32,
    pub GradientEnd: Option<Color>,
    pub GradientLength: f32,
    pub WiggleAmplitude: f32,
    pub WiggleWavelength: f32,
    pub FadeLength: f32,
    pub RotationSpin: f32,
    pub EchoCount: u32,
    pub EchoOffsetX: f32,
    pub EchoOffsetY: f32,
    pub EchoFade: f32,
    pub SparkleChance: f32,
    pub SparkleScale: f32,
    pub BoilFrames: u32,
    pub BoilJitter: f32,
    pub BoilWavelength: f32,
    pub Grain: f32,
    pub GrainScale: f32,
}

impl Default for AdvancedBrush {
    fn default() -> Self {
        Self {
            AnimatedTips: Vec::new(),
            Animation: TipAnimation::Sequential,
            AnimationStep: 1.0,
            ColorCycle: 0.0,
            GradientEnd: None,
            GradientLength: 200.0,
            WiggleAmplitude: 0.0,
            WiggleWavelength: 40.0,
            FadeLength: 0.0,
            RotationSpin: 0.0,
            EchoCount: 0,
            EchoOffsetX: 6.0,
            EchoOffsetY: 6.0,
            EchoFade: 0.6,
            SparkleChance: 0.0,
            SparkleScale: 2.5,
            BoilFrames: 0,
            BoilJitter: 1.5,
            BoilWavelength: 16.0,
            Grain: 0.0,
            GrainScale: 1.0,
        }
    }
}

impl AdvancedBrush {
    pub fn is_active(&self) -> bool {
        *self != AdvancedBrush::default()
    }

    pub fn boil_frames(&self) -> u32 {
        if cfg!(feature = "advanced-brushes") && self.BoilFrames >= 2 { self.BoilFrames.min(12) } else { 1 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Brush {
    pub Name: String,
    pub Mode: BrushMode,
    pub Tip: TipShape,
    pub TipImage: Option<TipImage>,
    pub Size: f32,
    pub Opacity: f32,
    pub Flow: f32,
    pub Hardness: f32,
    pub Spacing: f32,
    pub Angle: f32,
    pub Roundness: f32,
    pub Antialias: bool,
    pub PixelPerfect: bool,
    pub FollowDirection: bool,
    pub Smoothing: f32,
    pub Strength: f32,
    pub Blend: BlendMode,
    pub Dither: DitherPattern,
    pub PressureSize: bool,
    pub PressureOpacity: bool,
    pub PressureFlow: bool,
    pub PressureHardness: bool,
    pub TiltAngle: bool,
    pub TiltFlatten: f32,
    pub MinSizeRatio: f32,
    pub PressureCurve: f32,
    pub SizeJitter: f32,
    pub AngleJitter: f32,
    pub RoundnessJitter: f32,
    pub OpacityJitter: f32,
    pub Scatter: f32,
    pub DabCount: u32,
    pub HueJitter: f32,
    pub SaturationJitter: f32,
    pub ValueJitter: f32,
    pub TaperIn: f32,
    pub ReplaceTolerance: f32,
    pub Advanced: AdvancedBrush,
}

impl Default for Brush {
    fn default() -> Self {
        Self {
            Name: "Round".into(),
            Mode: BrushMode::Paint,
            Tip: TipShape::Round,
            TipImage: None,
            Size: 12.0,
            Opacity: 1.0,
            Flow: 1.0,
            Hardness: 0.8,
            Spacing: 0.12,
            Angle: 0.0,
            Roundness: 1.0,
            Antialias: true,
            PixelPerfect: false,
            FollowDirection: false,
            Smoothing: 0.0,
            Strength: 0.5,
            Blend: BlendMode::Normal,
            Dither: DitherPattern::None,
            PressureSize: true,
            PressureOpacity: false,
            PressureFlow: false,
            PressureHardness: false,
            TiltAngle: false,
            TiltFlatten: 0.0,
            MinSizeRatio: 0.2,
            PressureCurve: 1.0,
            SizeJitter: 0.0,
            AngleJitter: 0.0,
            RoundnessJitter: 0.0,
            OpacityJitter: 0.0,
            Scatter: 0.0,
            DabCount: 1,
            HueJitter: 0.0,
            SaturationJitter: 0.0,
            ValueJitter: 0.0,
            TaperIn: 0.0,
            ReplaceTolerance: 0.1,
            Advanced: AdvancedBrush::default(),
        }
    }
}

impl Brush {
    pub fn is_pixel_brush(&self) -> bool {
        !self.Antialias
    }

    pub fn uses_pixel_perfect_path(&self) -> bool {
        self.PixelPerfect && !self.Antialias && self.Size <= 1.5 && self.Scatter <= 0.0
    }

    pub fn to_json(&self) -> crate::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(Text: &str) -> crate::Result<Brush> {
        Ok(serde_json::from_str(Text)?)
    }
}
