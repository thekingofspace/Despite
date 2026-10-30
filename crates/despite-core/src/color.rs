use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub R: f32,
    pub G: f32,
    pub B: f32,
    pub A: f32,
}

impl Default for Color {
    fn default() -> Self {
        Self::Black
    }
}

impl Color {
    pub const Black: Color = Color::rgb(0.0, 0.0, 0.0);
    pub const White: Color = Color::rgb(1.0, 1.0, 1.0);
    pub const Transparent: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);

    pub const fn rgb(R: f32, G: f32, B: f32) -> Self {
        Self { R, G, B, A: 1.0 }
    }

    pub const fn rgba(R: f32, G: f32, B: f32, A: f32) -> Self {
        Self { R, G, B, A }
    }

    pub fn from_rgba8(Pixel: [u8; 4]) -> Self {
        Self::rgba(
            Pixel[0] as f32 / 255.0,
            Pixel[1] as f32 / 255.0,
            Pixel[2] as f32 / 255.0,
            Pixel[3] as f32 / 255.0,
        )
    }

    pub fn to_rgba8(self) -> [u8; 4] {
        [to_u8(self.R), to_u8(self.G), to_u8(self.B), to_u8(self.A)]
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.R, self.G, self.B, self.A]
    }

    pub fn from_array(Values: [f32; 4]) -> Self {
        Self::rgba(Values[0], Values[1], Values[2], Values[3])
    }

    pub fn with_alpha(self, A: f32) -> Self {
        Self { A, ..self }
    }

    pub fn from_hex(Text: &str) -> Option<Self> {
        let Hex = Text.trim().trim_start_matches('#');
        let Parse = |Index: usize| u8::from_str_radix(Hex.get(Index..Index + 2)?, 16).ok();
        match Hex.len() {
            3 => {
                let mut Bytes = [0u8; 4];
                for (Index, Character) in Hex.chars().enumerate() {
                    Bytes[Index] = Character.to_digit(16)? as u8 * 17;
                }
                Bytes[3] = 255;
                Some(Self::from_rgba8(Bytes))
            }
            6 => Some(Self::from_rgba8([Parse(0)?, Parse(2)?, Parse(4)?, 255])),
            8 => Some(Self::from_rgba8([Parse(0)?, Parse(2)?, Parse(4)?, Parse(6)?])),
            _ => None,
        }
    }

    pub fn to_hex(self) -> String {
        let [Red, Green, Blue, Alpha] = self.to_rgba8();
        if Alpha == 255 {
            format!("#{Red:02x}{Green:02x}{Blue:02x}")
        } else {
            format!("#{Red:02x}{Green:02x}{Blue:02x}{Alpha:02x}")
        }
    }

    pub fn to_hsv(self) -> (f32, f32, f32) {
        rgb_to_hsv(self.R, self.G, self.B)
    }

    pub fn from_hsva(Hue: f32, Saturation: f32, Value: f32, Alpha: f32) -> Self {
        let (Red, Green, Blue) = hsv_to_rgb(Hue, Saturation, Value);
        Self::rgba(Red, Green, Blue, Alpha)
    }

    pub fn lerp(self, Other: Color, T: f32) -> Color {
        Color::rgba(
            self.R + (Other.R - self.R) * T,
            self.G + (Other.G - self.G) * T,
            self.B + (Other.B - self.B) * T,
            self.A + (Other.A - self.A) * T,
        )
    }

    pub fn luminance(self) -> f32 {
        luminance(self.R, self.G, self.B)
    }
}

#[inline]
pub fn to_u8(Value: f32) -> u8 {
    (Value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

#[inline]
pub fn luminance(R: f32, G: f32, B: f32) -> f32 {
    0.299 * R + 0.587 * G + 0.114 * B
}

pub fn rgb_to_hsv(R: f32, G: f32, B: f32) -> (f32, f32, f32) {
    let Max = R.max(G).max(B);
    let Min = R.min(G).min(B);
    let Delta = Max - Min;
    let Hue = if Delta <= f32::EPSILON {
        0.0
    } else if Max == R {
        60.0 * ((G - B) / Delta).rem_euclid(6.0)
    } else if Max == G {
        60.0 * ((B - R) / Delta + 2.0)
    } else {
        60.0 * ((R - G) / Delta + 4.0)
    };
    let Saturation = if Max <= f32::EPSILON { 0.0 } else { Delta / Max };
    (Hue, Saturation, Max)
}

fn hue_sector(Hue: f32, Chroma: f32) -> (f32, f32, f32) {
    let Sector = Hue.rem_euclid(360.0) / 60.0;
    let Second = Chroma * (1.0 - ((Sector % 2.0) - 1.0).abs());
    match Sector as u32 {
        0 => (Chroma, Second, 0.0),
        1 => (Second, Chroma, 0.0),
        2 => (0.0, Chroma, Second),
        3 => (0.0, Second, Chroma),
        4 => (Second, 0.0, Chroma),
        _ => (Chroma, 0.0, Second),
    }
}

pub fn hsv_to_rgb(Hue: f32, Saturation: f32, Value: f32) -> (f32, f32, f32) {
    let Value = Value.clamp(0.0, 1.0);
    let Chroma = Value * Saturation.clamp(0.0, 1.0);
    let (R, G, B) = hue_sector(Hue, Chroma);
    let Offset = Value - Chroma;
    (R + Offset, G + Offset, B + Offset)
}

pub fn rgb_to_hsl(R: f32, G: f32, B: f32) -> (f32, f32, f32) {
    let Max = R.max(G).max(B);
    let Min = R.min(G).min(B);
    let Lightness = (Max + Min) / 2.0;
    let Delta = Max - Min;
    if Delta <= f32::EPSILON {
        return (0.0, 0.0, Lightness);
    }
    let Saturation = if Lightness > 0.5 { Delta / (2.0 - Max - Min) } else { Delta / (Max + Min) };
    let (Hue, _, _) = rgb_to_hsv(R, G, B);
    (Hue, Saturation, Lightness)
}

pub fn hsl_to_rgb(Hue: f32, Saturation: f32, Lightness: f32) -> (f32, f32, f32) {
    let Lightness = Lightness.clamp(0.0, 1.0);
    let Chroma = (1.0 - (2.0 * Lightness - 1.0).abs()) * Saturation.clamp(0.0, 1.0);
    let (R, G, B) = hue_sector(Hue, Chroma);
    let Offset = Lightness - Chroma / 2.0;
    (R + Offset, G + Offset, B + Offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let Orange = Color::from_hex("#ff8800").unwrap();
        assert_eq!(Orange.to_rgba8(), [255, 136, 0, 255]);
        assert_eq!(Orange.to_hex(), "#ff8800");
        assert_eq!(Color::from_hex("#fff").unwrap().to_rgba8(), [255, 255, 255, 255]);
        assert_eq!(Color::from_hex("11223344").unwrap().to_rgba8(), [0x11, 0x22, 0x33, 0x44]);
        assert!(Color::from_hex("zz").is_none());
    }

    #[test]
    fn hsv_and_hsl_roundtrip() {
        for Sample in [[1.0, 0.0, 0.0], [0.2, 0.7, 0.4], [0.5, 0.5, 0.5], [0.1, 0.2, 0.9]] {
            let (Hue, Saturation, Value) = rgb_to_hsv(Sample[0], Sample[1], Sample[2]);
            let (R, G, B) = hsv_to_rgb(Hue, Saturation, Value);
            assert!((R - Sample[0]).abs() < 1e-4 && (G - Sample[1]).abs() < 1e-4 && (B - Sample[2]).abs() < 1e-4);
            let (Hue, Saturation, Lightness) = rgb_to_hsl(Sample[0], Sample[1], Sample[2]);
            let (R, G, B) = hsl_to_rgb(Hue, Saturation, Lightness);
            assert!((R - Sample[0]).abs() < 1e-4 && (G - Sample[1]).abs() < 1e-4 && (B - Sample[2]).abs() < 1e-4);
        }
    }
}
