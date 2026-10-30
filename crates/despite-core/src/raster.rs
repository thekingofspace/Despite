use std::fmt;
use std::io::Cursor;

use crate::blend::{self, BlendMode};
use crate::error::{Error, Result};
use crate::geom::IRect;

pub const MaxDimension: u32 = 16_384;

#[derive(Clone, PartialEq, Eq, Default)]
pub struct Raster {
    Width: u32,
    Height: u32,
    Data: Vec<u8>,
}

impl fmt::Debug for Raster {
    fn fmt(&self, Formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(Formatter, "Raster({}x{})", self.Width, self.Height)
    }
}

impl Raster {
    pub fn new(Width: u32, Height: u32) -> Self {
        Self { Width, Height, Data: vec![0; Width as usize * Height as usize * 4] }
    }

    pub fn filled(Width: u32, Height: u32, Pixel: [u8; 4]) -> Self {
        let mut Out = Self::new(Width, Height);
        Out.fill(Pixel);
        Out
    }

    pub fn from_rgba(Width: u32, Height: u32, Data: Vec<u8>) -> Result<Self> {
        let Expected = Width as usize * Height as usize * 4;
        if Data.len() != Expected {
            return Err(Error::InvalidData(format!(
                "pixel buffer is {} bytes, expected {Expected} for {Width}x{Height}",
                Data.len()
            )));
        }
        Ok(Self { Width, Height, Data })
    }

    pub fn width(&self) -> u32 {
        self.Width
    }

    pub fn height(&self) -> u32 {
        self.Height
    }

    pub fn size(&self) -> (u32, u32) {
        (self.Width, self.Height)
    }

    pub fn bounds(&self) -> IRect {
        IRect::sized(self.Width, self.Height)
    }

    pub fn data(&self) -> &[u8] {
        &self.Data
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.Data
    }

    pub fn into_data(self) -> Vec<u8> {
        self.Data
    }

    pub fn byte_size(&self) -> usize {
        self.Data.len()
    }

    pub fn pixels(&self) -> &[[u8; 4]] {
        bytemuck::cast_slice(&self.Data)
    }

    pub fn pixels_mut(&mut self) -> &mut [[u8; 4]] {
        bytemuck::cast_slice_mut(&mut self.Data)
    }

    #[inline]
    fn index(&self, X: u32, Y: u32) -> usize {
        (Y as usize * self.Width as usize + X as usize) * 4
    }

    #[inline]
    pub fn get(&self, X: u32, Y: u32) -> [u8; 4] {
        let Index = self.index(X, Y);
        [self.Data[Index], self.Data[Index + 1], self.Data[Index + 2], self.Data[Index + 3]]
    }

    #[inline]
    pub fn contains(&self, X: i32, Y: i32) -> bool {
        X >= 0 && Y >= 0 && (X as u32) < self.Width && (Y as u32) < self.Height
    }

    #[inline]
    pub fn get_checked(&self, X: i32, Y: i32) -> Option<[u8; 4]> {
        self.contains(X, Y).then(|| self.get(X as u32, Y as u32))
    }

    #[inline]
    pub fn get_clamped(&self, X: i32, Y: i32) -> [u8; 4] {
        if self.Width == 0 || self.Height == 0 {
            return [0; 4];
        }
        self.get(X.clamp(0, self.Width as i32 - 1) as u32, Y.clamp(0, self.Height as i32 - 1) as u32)
    }

    #[inline]
    pub fn put(&mut self, X: u32, Y: u32, Pixel: [u8; 4]) {
        let Index = self.index(X, Y);
        self.Data[Index..Index + 4].copy_from_slice(&Pixel);
    }

    #[inline]
    pub fn put_checked(&mut self, X: i32, Y: i32, Pixel: [u8; 4]) {
        if self.contains(X, Y) {
            self.put(X as u32, Y as u32, Pixel);
        }
    }

    pub fn clear(&mut self) {
        self.Data.fill(0);
    }

    pub fn fill(&mut self, Pixel: [u8; 4]) {
        for Target in self.pixels_mut() {
            *Target = Pixel;
        }
    }

    pub fn fill_rect(&mut self, Rect: IRect, Pixel: [u8; 4]) {
        let Clipped = Rect.clamp_to(self.Width, self.Height);
        for Y in Clipped.Y..Clipped.bottom() {
            for X in Clipped.X..Clipped.right() {
                self.put(X as u32, Y as u32, Pixel);
            }
        }
    }

    pub fn crop(&self, Rect: IRect) -> Raster {
        let mut Out = Raster::new(Rect.W.max(0) as u32, Rect.H.max(0) as u32);
        Out.blit(self, -Rect.X, -Rect.Y);
        Out
    }

    pub fn blit(&mut self, Source: &Raster, X: i32, Y: i32) {
        let Target = IRect::new(X, Y, Source.Width as i32, Source.Height as i32).clamp_to(self.Width, self.Height);
        if Target.is_empty() {
            return;
        }
        let RowBytes = Target.W as usize * 4;
        for DestinationY in Target.Y..Target.bottom() {
            let SourceIndex = Source.index((Target.X - X) as u32, (DestinationY - Y) as u32);
            let DestinationIndex = self.index(Target.X as u32, DestinationY as u32);
            self.Data[DestinationIndex..DestinationIndex + RowBytes]
                .copy_from_slice(&Source.Data[SourceIndex..SourceIndex + RowBytes]);
        }
    }

    pub fn draw_over(&mut self, Source: &Raster, X: i32, Y: i32, Opacity: f32, Mode: BlendMode) {
        let Target = IRect::new(X, Y, Source.Width as i32, Source.Height as i32).clamp_to(self.Width, self.Height);
        for DestinationY in Target.Y..Target.bottom() {
            for DestinationX in Target.X..Target.right() {
                let SourcePixel = Source.get((DestinationX - X) as u32, (DestinationY - Y) as u32);
                let Backdrop = self.get(DestinationX as u32, DestinationY as u32);
                self.put(
                    DestinationX as u32,
                    DestinationY as u32,
                    blend::composite_pixel(Backdrop, SourcePixel, Opacity, Mode),
                );
            }
        }
    }

    pub fn resized_canvas(&self, Width: u32, Height: u32, OffsetX: i32, OffsetY: i32) -> Raster {
        let mut Out = Raster::new(Width, Height);
        Out.blit(self, OffsetX, OffsetY);
        Out
    }

    pub fn scaled_nearest(&self, Width: u32, Height: u32) -> Raster {
        let mut Out = Raster::new(Width, Height);
        if self.Width == 0 || self.Height == 0 {
            return Out;
        }
        for Y in 0..Height {
            let SourceY = ((Y as u64 * self.Height as u64) / Height.max(1) as u64) as u32;
            for X in 0..Width {
                let SourceX = ((X as u64 * self.Width as u64) / Width.max(1) as u64) as u32;
                Out.put(X, Y, self.get(SourceX, SourceY));
            }
        }
        Out
    }

    pub fn scaled_smooth(&self, Width: u32, Height: u32) -> Raster {
        let mut Out = Raster::new(Width, Height);
        if self.Width == 0 || self.Height == 0 || Width == 0 || Height == 0 {
            return Out;
        }
        let StepX = self.Width as f32 / Width as f32;
        let StepY = self.Height as f32 / Height as f32;
        if StepX <= 1.0 && StepY <= 1.0 {
            for Y in 0..Height {
                for X in 0..Width {
                    let Sample = self.sample_bilinear((X as f32 + 0.5) * StepX, (Y as f32 + 0.5) * StepY);
                    Out.put(X, Y, blend::to_u8(Sample));
                }
            }
            return Out;
        }
        for Y in 0..Height {
            let Y0 = (Y as f32 * StepY) as u32;
            let Y1 = (((Y + 1) as f32 * StepY).ceil() as u32).clamp(Y0 + 1, self.Height);
            for X in 0..Width {
                let X0 = (X as f32 * StepX) as u32;
                let X1 = (((X + 1) as f32 * StepX).ceil() as u32).clamp(X0 + 1, self.Width);
                let mut Sum = [0f32; 4];
                let mut Count = 0f32;
                for SourceY in Y0..Y1 {
                    for SourceX in X0..X1 {
                        let Premultiplied = blend::premultiply(blend::to_f32(self.get(SourceX, SourceY)));
                        for Channel in 0..4 {
                            Sum[Channel] += Premultiplied[Channel];
                        }
                        Count += 1.0;
                    }
                }
                for Channel in &mut Sum {
                    *Channel /= Count;
                }
                Out.put(X, Y, blend::to_u8(blend::unpremul(Sum)));
            }
        }
        Out
    }

    pub fn flipped_h(&self) -> Raster {
        let mut Out = self.clone();
        for Y in 0..self.Height {
            for X in 0..self.Width {
                Out.put(self.Width - 1 - X, Y, self.get(X, Y));
            }
        }
        Out
    }

    pub fn flipped_v(&self) -> Raster {
        let mut Out = self.clone();
        for Y in 0..self.Height {
            for X in 0..self.Width {
                Out.put(X, self.Height - 1 - Y, self.get(X, Y));
            }
        }
        Out
    }

    #[inline]
    pub fn sample_nearest(&self, X: f32, Y: f32) -> [u8; 4] {
        self.get_checked(X.floor() as i32, Y.floor() as i32).unwrap_or([0; 4])
    }

    #[inline]
    pub fn sample_bilinear(&self, X: f32, Y: f32) -> [f32; 4] {
        let ShiftedX = X - 0.5;
        let ShiftedY = Y - 0.5;
        let FloorX = ShiftedX.floor();
        let FloorY = ShiftedY.floor();
        let Tx = ShiftedX - FloorX;
        let Ty = ShiftedY - FloorY;
        let (BaseX, BaseY) = (FloorX as i32, FloorY as i32);
        let mut Sum = [0f32; 4];
        for (Dx, Dy, Weight) in [
            (0, 0, (1.0 - Tx) * (1.0 - Ty)),
            (1, 0, Tx * (1.0 - Ty)),
            (0, 1, (1.0 - Tx) * Ty),
            (1, 1, Tx * Ty),
        ] {
            if Weight <= 0.0 {
                continue;
            }
            if let Some(Pixel) = self.get_checked(BaseX + Dx, BaseY + Dy) {
                let Premultiplied = blend::premultiply(blend::to_f32(Pixel));
                for Channel in 0..4 {
                    Sum[Channel] += Premultiplied[Channel] * Weight;
                }
            }
        }
        blend::unpremul(Sum)
    }

    pub fn is_fully_transparent(&self) -> bool {
        self.pixels().iter().all(|Pixel| Pixel[3] == 0)
    }

    pub fn opaque_bounds(&self) -> Option<IRect> {
        let (mut X0, mut Y0, mut X1, mut Y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for Y in 0..self.Height {
            for X in 0..self.Width {
                if self.get(X, Y)[3] != 0 {
                    X0 = X0.min(X as i32);
                    Y0 = Y0.min(Y as i32);
                    X1 = X1.max(X as i32 + 1);
                    Y1 = Y1.max(Y as i32 + 1);
                }
            }
        }
        (X0 != i32::MAX).then(|| IRect::from_min_max(X0, Y0, X1, Y1))
    }

    pub fn to_image(&self) -> image::RgbaImage {
        image::RgbaImage::from_raw(self.Width, self.Height, self.Data.clone())
            .expect("raster buffer size always matches its dimensions")
    }

    pub fn from_image(Source: image::RgbaImage) -> Raster {
        let (Width, Height) = Source.dimensions();
        Raster { Width, Height, Data: Source.into_raw() }
    }

    pub fn encode_png(&self) -> Result<Vec<u8>> {
        let mut Out = Vec::new();
        image::write_buffer_with_format(
            &mut Cursor::new(&mut Out),
            &self.Data,
            self.Width,
            self.Height,
            image::ExtendedColorType::Rgba8,
            image::ImageFormat::Png,
        )?;
        Ok(Out)
    }

    pub fn decode(Bytes: &[u8]) -> Result<Raster> {
        let Reader = image::ImageReader::new(Cursor::new(Bytes)).with_guessed_format()?;
        let (Width, Height) = Reader.into_dimensions()?;
        if Width > MaxDimension || Height > MaxDimension {
            return Err(Error::TooLarge { Width, Height });
        }
        let Decoded = image::load_from_memory(Bytes)?;
        Ok(Raster::from_image(Decoded.to_rgba8()))
    }

    pub fn load(Path: &std::path::Path) -> Result<Raster> {
        Raster::decode(&std::fs::read(Path)?)
    }

    pub fn save_png(&self, Path: &std::path::Path) -> Result<()> {
        std::fs::write(Path, self.encode_png()?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blit_and_crop() {
        let mut Canvas = Raster::new(4, 4);
        let Patch = Raster::filled(2, 2, [255, 0, 0, 255]);
        Canvas.blit(&Patch, 3, 3);
        assert_eq!(Canvas.get(3, 3), [255, 0, 0, 255]);
        assert_eq!(Canvas.get(2, 2), [0, 0, 0, 0]);
        let Cropped = Canvas.crop(IRect::new(2, 2, 4, 4));
        assert_eq!(Cropped.size(), (4, 4));
        assert_eq!(Cropped.get(1, 1), [255, 0, 0, 255]);
        assert_eq!(Cropped.get(2, 2), [0, 0, 0, 0]);
        assert_eq!(Canvas.opaque_bounds(), Some(IRect::new(3, 3, 1, 1)));
    }

    #[test]
    fn png_roundtrip() {
        let mut Source = Raster::new(3, 2);
        Source.put(1, 1, [1, 2, 3, 4]);
        let Bytes = Source.encode_png().unwrap();
        assert_eq!(Raster::decode(&Bytes).unwrap(), Source);
    }

    #[test]
    fn scaling() {
        let Source = Raster::filled(4, 4, [10, 20, 30, 255]);
        assert_eq!(Source.scaled_nearest(8, 2).get(7, 1), [10, 20, 30, 255]);
        assert_eq!(Source.scaled_smooth(2, 2).get(1, 1), [10, 20, 30, 255]);
    }
}
