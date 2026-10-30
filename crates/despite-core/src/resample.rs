use serde::{Deserialize, Serialize};

use crate::blend;
use crate::raster::Raster;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResampleMethod {
    #[default]
    Nearest,
    Bilinear,
    Bicubic,
    Lanczos3,
    Gaussian,
    Box,
    Scale2x,
    Scale3x,
}

impl ResampleMethod {
    pub const All: &'static [ResampleMethod] = &[
        ResampleMethod::Nearest,
        ResampleMethod::Bilinear,
        ResampleMethod::Bicubic,
        ResampleMethod::Lanczos3,
        ResampleMethod::Gaussian,
        ResampleMethod::Box,
        ResampleMethod::Scale2x,
        ResampleMethod::Scale3x,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ResampleMethod::Nearest => "Nearest Neighbour (pixel art)",
            ResampleMethod::Bilinear => "Bilinear",
            ResampleMethod::Bicubic => "Bicubic (Catmull-Rom)",
            ResampleMethod::Lanczos3 => "Lanczos 3 (sharp)",
            ResampleMethod::Gaussian => "Gaussian (soft)",
            ResampleMethod::Box => "Box / Area average",
            ResampleMethod::Scale2x => "Scale2x (pixel art smoothing)",
            ResampleMethod::Scale3x => "Scale3x (pixel art smoothing)",
        }
    }

    pub fn is_pixel_art(self) -> bool {
        matches!(self, ResampleMethod::Nearest | ResampleMethod::Scale2x | ResampleMethod::Scale3x)
    }
}

pub fn resample(Source: &Raster, Width: u32, Height: u32, Method: ResampleMethod) -> Raster {
    let Width = Width.max(1);
    let Height = Height.max(1);
    if Source.size() == (Width, Height) {
        return Source.clone();
    }
    if Source.width() == 0 || Source.height() == 0 {
        return Raster::new(Width, Height);
    }
    match Method {
        ResampleMethod::Nearest => Source.scaled_nearest(Width, Height),
        ResampleMethod::Box => Source.scaled_smooth(Width, Height),
        ResampleMethod::Bilinear => filtered(Source, Width, Height, image::imageops::FilterType::Triangle),
        ResampleMethod::Bicubic => filtered(Source, Width, Height, image::imageops::FilterType::CatmullRom),
        ResampleMethod::Lanczos3 => filtered(Source, Width, Height, image::imageops::FilterType::Lanczos3),
        ResampleMethod::Gaussian => filtered(Source, Width, Height, image::imageops::FilterType::Gaussian),
        ResampleMethod::Scale2x | ResampleMethod::Scale3x => {
            let mut Current = Source.clone();
            while Current.width() < Width || Current.height() < Height {
                Current = if Method == ResampleMethod::Scale2x { scale2x(&Current) } else { scale3x(&Current) };
                if Current.width() > 16_384 || Current.height() > 16_384 {
                    break;
                }
            }
            if Current.size() == (Width, Height) {
                Current
            } else if Current.width() >= Width {
                Current.scaled_smooth(Width, Height)
            } else {
                Current.scaled_nearest(Width, Height)
            }
        }
    }
}

pub fn scale_by(Source: &Raster, Factor: f32, Method: ResampleMethod) -> Raster {
    let Width = ((Source.width() as f32 * Factor).round() as u32).max(1);
    let Height = ((Source.height() as f32 * Factor).round() as u32).max(1);
    resample(Source, Width, Height, Method)
}

fn filtered(Source: &Raster, Width: u32, Height: u32, Filter: image::imageops::FilterType) -> Raster {
    let mut Premultiplied: image::ImageBuffer<image::Rgba<f32>, Vec<f32>> = image::ImageBuffer::new(Source.width(), Source.height());
    for (X, Y, Pixel) in Premultiplied.enumerate_pixels_mut() {
        let Value = blend::premultiply(blend::to_f32(Source.get(X, Y)));
        *Pixel = image::Rgba(Value);
    }
    let Scaled = image::imageops::resize(&Premultiplied, Width, Height, Filter);
    let mut Out = Raster::new(Width, Height);
    for (X, Y, Pixel) in Scaled.enumerate_pixels() {
        let Clamped = [
            Pixel.0[0].clamp(0.0, 1.0),
            Pixel.0[1].clamp(0.0, 1.0),
            Pixel.0[2].clamp(0.0, 1.0),
            Pixel.0[3].clamp(0.0, 1.0),
        ];
        Out.put(X, Y, blend::to_u8(blend::unpremul(Clamped)));
    }
    Out
}

pub fn scale2x(Source: &Raster) -> Raster {
    let (Width, Height) = Source.size();
    let mut Out = Raster::new(Width * 2, Height * 2);
    for Y in 0..Height as i32 {
        for X in 0..Width as i32 {
            let Center = Source.get_clamped(X, Y);
            let Up = Source.get_clamped(X, Y - 1);
            let Right = Source.get_clamped(X + 1, Y);
            let Left = Source.get_clamped(X - 1, Y);
            let Down = Source.get_clamped(X, Y + 1);
            let TopLeft = if Left == Up && Left != Down && Up != Right { Up } else { Center };
            let TopRight = if Up == Right && Up != Left && Right != Down { Right } else { Center };
            let BottomLeft = if Down == Left && Down != Right && Left != Up { Left } else { Center };
            let BottomRight = if Right == Down && Right != Up && Down != Left { Down } else { Center };
            let (OutX, OutY) = (X as u32 * 2, Y as u32 * 2);
            Out.put(OutX, OutY, TopLeft);
            Out.put(OutX + 1, OutY, TopRight);
            Out.put(OutX, OutY + 1, BottomLeft);
            Out.put(OutX + 1, OutY + 1, BottomRight);
        }
    }
    Out
}

pub fn scale3x(Source: &Raster) -> Raster {
    let (Width, Height) = Source.size();
    let mut Out = Raster::new(Width * 3, Height * 3);
    for Y in 0..Height as i32 {
        for X in 0..Width as i32 {
            let A = Source.get_clamped(X - 1, Y - 1);
            let B = Source.get_clamped(X, Y - 1);
            let C = Source.get_clamped(X + 1, Y - 1);
            let D = Source.get_clamped(X - 1, Y);
            let E = Source.get_clamped(X, Y);
            let F = Source.get_clamped(X + 1, Y);
            let G = Source.get_clamped(X - 1, Y + 1);
            let H = Source.get_clamped(X, Y + 1);
            let I = Source.get_clamped(X + 1, Y + 1);
            let Cells = if B != H && D != F {
                [
                    if D == B { D } else { E },
                    if (D == B && E != C) || (B == F && E != A) { B } else { E },
                    if B == F { F } else { E },
                    if (D == B && E != G) || (D == H && E != A) { D } else { E },
                    E,
                    if (B == F && E != I) || (H == F && E != C) { F } else { E },
                    if D == H { D } else { E },
                    if (D == H && E != I) || (H == F && E != G) { H } else { E },
                    if H == F { F } else { E },
                ]
            } else {
                [E; 9]
            };
            for (Index, Pixel) in Cells.iter().enumerate() {
                Out.put(X as u32 * 3 + (Index % 3) as u32, Y as u32 * 3 + (Index / 3) as u32, *Pixel);
            }
        }
    }
    Out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_integer_upscale_is_exact() {
        let mut Source = Raster::new(2, 2);
        Source.put(1, 0, [255, 0, 0, 255]);
        let Scaled = resample(&Source, 8, 8, ResampleMethod::Nearest);
        assert_eq!(Scaled.get(4, 0), [255, 0, 0, 255]);
        assert_eq!(Scaled.get(7, 3), [255, 0, 0, 255]);
        assert_eq!(Scaled.get(3, 3), [0, 0, 0, 0]);
    }

    #[test]
    fn scalers_smooth_diagonals() {
        let mut Source = Raster::filled(3, 3, [255, 255, 255, 255]);
        Source.put(0, 0, [0, 0, 0, 255]);
        Source.put(1, 1, [0, 0, 0, 255]);
        Source.put(2, 2, [0, 0, 0, 255]);
        let Doubled = scale2x(&Source);
        assert_eq!(Doubled.size(), (6, 6));
        let Tripled = scale3x(&Source);
        assert_eq!(Tripled.size(), (9, 9));
        assert_eq!(resample(&Source, 12, 12, ResampleMethod::Scale2x).size(), (12, 12));
    }

    #[test]
    fn smooth_methods_keep_flat_color() {
        let Source = Raster::filled(5, 5, [40, 80, 120, 255]);
        for Method in [ResampleMethod::Bilinear, ResampleMethod::Bicubic, ResampleMethod::Lanczos3, ResampleMethod::Box] {
            let Scaled = resample(&Source, 13, 9, Method);
            let Pixel = Scaled.get(6, 4);
            assert!(Pixel[0].abs_diff(40) <= 1 && Pixel[3] == 255, "{Method:?} {Pixel:?}");
        }
    }
}
