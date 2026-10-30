use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Add,
    Subtract,
}

impl BlendMode {
    pub const All: &'static [BlendMode] = &[
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::HardLight,
        BlendMode::SoftLight,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Add,
        BlendMode::Subtract,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Multiply => "Multiply",
            BlendMode::Screen => "Screen",
            BlendMode::Overlay => "Overlay",
            BlendMode::Darken => "Darken",
            BlendMode::Lighten => "Lighten",
            BlendMode::ColorDodge => "Color Dodge",
            BlendMode::ColorBurn => "Color Burn",
            BlendMode::HardLight => "Hard Light",
            BlendMode::SoftLight => "Soft Light",
            BlendMode::Difference => "Difference",
            BlendMode::Exclusion => "Exclusion",
            BlendMode::Add => "Add",
            BlendMode::Subtract => "Subtract",
        }
    }

    pub fn from_name(Name: &str) -> Option<BlendMode> {
        let Wanted: String = Name.chars().filter(|Character| !Character.is_whitespace()).collect();
        BlendMode::All.iter().copied().find(|Mode| {
            let Candidate: String = Mode.name().chars().filter(|Character| !Character.is_whitespace()).collect();
            Candidate.eq_ignore_ascii_case(&Wanted)
        })
    }
}

#[inline]
fn blend_channel(Mode: BlendMode, Backdrop: f32, Source: f32) -> f32 {
    match Mode {
        BlendMode::Normal => Source,
        BlendMode::Multiply => Backdrop * Source,
        BlendMode::Screen => Backdrop + Source - Backdrop * Source,
        BlendMode::Overlay => hard_light(Source, Backdrop),
        BlendMode::Darken => Backdrop.min(Source),
        BlendMode::Lighten => Backdrop.max(Source),
        BlendMode::ColorDodge => {
            if Backdrop <= 0.0 {
                0.0
            } else if Source >= 1.0 {
                1.0
            } else {
                (Backdrop / (1.0 - Source)).min(1.0)
            }
        }
        BlendMode::ColorBurn => {
            if Backdrop >= 1.0 {
                1.0
            } else if Source <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - Backdrop) / Source).min(1.0)
            }
        }
        BlendMode::HardLight => hard_light(Backdrop, Source),
        BlendMode::SoftLight => {
            if Source <= 0.5 {
                Backdrop - (1.0 - 2.0 * Source) * Backdrop * (1.0 - Backdrop)
            } else {
                let Curve = if Backdrop <= 0.25 {
                    ((16.0 * Backdrop - 12.0) * Backdrop + 4.0) * Backdrop
                } else {
                    Backdrop.sqrt()
                };
                Backdrop + (2.0 * Source - 1.0) * (Curve - Backdrop)
            }
        }
        BlendMode::Difference => (Backdrop - Source).abs(),
        BlendMode::Exclusion => Backdrop + Source - 2.0 * Backdrop * Source,
        BlendMode::Add => (Backdrop + Source).min(1.0),
        BlendMode::Subtract => (Backdrop - Source).max(0.0),
    }
}

#[inline]
fn hard_light(Backdrop: f32, Source: f32) -> f32 {
    if Source <= 0.5 {
        Backdrop * 2.0 * Source
    } else {
        let Scaled = 2.0 * Source - 1.0;
        Backdrop + Scaled - Backdrop * Scaled
    }
}

#[inline]
pub fn composite_f32(Destination: [f32; 4], Source: [f32; 4], Opacity: f32, Mode: BlendMode) -> [f32; 4] {
    let SourceAlpha = Source[3] * Opacity;
    if SourceAlpha <= 0.0 {
        return Destination;
    }
    let DestinationAlpha = Destination[3];
    let OutAlpha = SourceAlpha + DestinationAlpha * (1.0 - SourceAlpha);
    if OutAlpha <= 0.0 {
        return [0.0; 4];
    }
    let mut Out = [0.0, 0.0, 0.0, OutAlpha];
    for Channel in 0..3 {
        let SourceValue = Source[Channel];
        let Backdrop = Destination[Channel];
        let Blended = if Mode == BlendMode::Normal { SourceValue } else { blend_channel(Mode, Backdrop, SourceValue) };
        let Mixed = (1.0 - DestinationAlpha) * SourceValue + DestinationAlpha * Blended;
        Out[Channel] = (SourceAlpha * Mixed + DestinationAlpha * (1.0 - SourceAlpha) * Backdrop) / OutAlpha;
    }
    Out
}

#[inline]
pub fn to_f32(Pixel: [u8; 4]) -> [f32; 4] {
    const Scale: f32 = 1.0 / 255.0;
    [Pixel[0] as f32 * Scale, Pixel[1] as f32 * Scale, Pixel[2] as f32 * Scale, Pixel[3] as f32 * Scale]
}

#[inline]
pub fn to_u8(Pixel: [f32; 4]) -> [u8; 4] {
    [
        crate::color::to_u8(Pixel[0]),
        crate::color::to_u8(Pixel[1]),
        crate::color::to_u8(Pixel[2]),
        crate::color::to_u8(Pixel[3]),
    ]
}

#[inline]
pub fn composite_pixel(Destination: [u8; 4], Source: [u8; 4], Opacity: f32, Mode: BlendMode) -> [u8; 4] {
    if Source[3] == 0 || Opacity <= 0.0 {
        return Destination;
    }
    if Mode == BlendMode::Normal && Source[3] == 255 && Opacity >= 1.0 {
        return Source;
    }
    to_u8(composite_f32(to_f32(Destination), to_f32(Source), Opacity, Mode))
}

#[inline]
pub fn premultiply(Pixel: [f32; 4]) -> [f32; 4] {
    [Pixel[0] * Pixel[3], Pixel[1] * Pixel[3], Pixel[2] * Pixel[3], Pixel[3]]
}

#[inline]
pub fn unpremul(Pixel: [f32; 4]) -> [f32; 4] {
    if Pixel[3] <= 1e-6 {
        [0.0; 4]
    } else {
        [Pixel[0] / Pixel[3], Pixel[1] / Pixel[3], Pixel[2] / Pixel[3], Pixel[3]]
    }
}

#[inline]
pub fn lerp_premul(From: [f32; 4], To: [f32; 4], T: f32) -> [f32; 4] {
    let Start = premultiply(From);
    let End = premultiply(To);
    let mut Out = [0.0; 4];
    for Channel in 0..4 {
        Out[Channel] = Start[Channel] + (End[Channel] - Start[Channel]) * T;
    }
    unpremul(Out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_over_opaque() {
        let Out = composite_pixel([0, 0, 255, 255], [255, 0, 0, 128], 1.0, BlendMode::Normal);
        assert!(Out[0] > 120 && Out[0] < 135 && Out[2] > 120 && Out[3] == 255, "{Out:?}");
    }

    #[test]
    fn over_transparent_keeps_color() {
        let Out = composite_pixel([0, 0, 0, 0], [255, 10, 20, 100], 1.0, BlendMode::Multiply);
        assert_eq!(Out, [255, 10, 20, 100]);
    }

    #[test]
    fn multiply_and_screen() {
        let Multiplied = composite_pixel([128, 128, 128, 255], [128, 255, 0, 255], 1.0, BlendMode::Multiply);
        assert_eq!(Multiplied, [64, 128, 0, 255]);
        let Screened = composite_pixel([0, 0, 0, 255], [10, 20, 30, 255], 1.0, BlendMode::Screen);
        assert_eq!(Screened, [10, 20, 30, 255]);
    }

    #[test]
    fn names_roundtrip() {
        for Mode in BlendMode::All {
            assert_eq!(BlendMode::from_name(Mode.name()), Some(*Mode));
        }
        assert_eq!(BlendMode::from_name("colordodge"), Some(BlendMode::ColorDodge));
    }
}
