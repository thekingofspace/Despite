use std::sync::Arc;

use despite_core::{IRect, Mask, Raster};
use egui::{Pos2, Rect, TextureHandle, TextureOptions, Vec2};

pub struct DocumentView {
    pub Zoom: f32,
    pub Pan: Vec2,
    pub NeedsFit: bool,
    pub AutoFit: bool,
    pub LastViewport: Rect,
    pub Selection: Option<Arc<Mask>>,
    pub SelectionOutline: Vec<[(i32, i32); 2]>,
    pub Frame: u32,
    pub Cache: Option<Raster>,
    pub Texture: Option<TextureHandle>,
    pub TextureSmooth: bool,
    pub Dirty: IRect,
    pub FullDirty: bool,
    pub SeenGeneration: u64,
    pub SeenFrame: u32,
}

impl Default for DocumentView {
    fn default() -> Self {
        Self {
            Zoom: 1.0,
            Pan: Vec2::ZERO,
            NeedsFit: true,
            AutoFit: true,
            LastViewport: Rect::NOTHING,
            Selection: None,
            SelectionOutline: Vec::new(),
            Frame: 0,
            Cache: None,
            Texture: None,
            TextureSmooth: false,
            Dirty: IRect::Empty,
            FullDirty: true,
            SeenGeneration: u64::MAX,
            SeenFrame: u32::MAX,
        }
    }
}

impl DocumentView {
    pub fn set_selection(&mut self, Selection: Option<Mask>) {
        self.SelectionOutline = Selection.as_ref().map(|Found| Found.outline_segments()).unwrap_or_default();
        self.Selection = Selection.map(Arc::new);
    }

    pub fn mark_dirty(&mut self, Region: IRect) {
        self.Dirty = self.Dirty.union(&Region);
    }

    pub fn canvas_rect(&self, Viewport: Rect, Width: u32, Height: u32) -> Rect {
        let Size = Vec2::new(Width as f32, Height as f32) * self.Zoom;
        let TopLeft = Viewport.center() + self.Pan - Size * 0.5;
        let TopLeft = Pos2::new(TopLeft.x.round(), TopLeft.y.round());
        Rect::from_min_size(TopLeft, Size)
    }

    pub fn screen_to_image(&self, Viewport: Rect, Width: u32, Height: u32, Point: Pos2) -> (f32, f32) {
        let Canvas = self.canvas_rect(Viewport, Width, Height);
        ((Point.x - Canvas.min.x) / self.Zoom, (Point.y - Canvas.min.y) / self.Zoom)
    }

    pub fn image_to_screen(&self, Viewport: Rect, Width: u32, Height: u32, X: f32, Y: f32) -> Pos2 {
        let Canvas = self.canvas_rect(Viewport, Width, Height);
        Pos2::new(Canvas.min.x + X * self.Zoom, Canvas.min.y + Y * self.Zoom)
    }

    pub fn fit(&mut self, Viewport: Rect, Width: u32, Height: u32) {
        let Margin = 32.0;
        let Available = (Viewport.size() - Vec2::splat(Margin * 2.0)).max(Vec2::splat(16.0));
        let Scale = (Available.x / Width.max(1) as f32).min(Available.y / Height.max(1) as f32);
        self.Zoom = if Scale >= 1.0 { Scale.floor().max(1.0) } else { Scale };
        self.Pan = Vec2::ZERO;
        self.NeedsFit = false;
    }

    pub fn request_fit(&mut self) {
        self.NeedsFit = true;
        self.AutoFit = true;
    }

    pub fn zoom_around(&mut self, Viewport: Rect, Width: u32, Height: u32, Anchor: Pos2, NewZoom: f32) {
        self.AutoFit = false;
        let NewZoom = NewZoom.clamp(0.02, 256.0);
        let (ImageX, ImageY) = self.screen_to_image(Viewport, Width, Height, Anchor);
        self.Zoom = NewZoom;
        let After = self.image_to_screen(Viewport, Width, Height, ImageX, ImageY);
        self.Pan += Anchor - After;
    }

    pub fn upload(&mut self, Context: &egui::Context, Name: &str, Smooth: bool) {
        let Some(Pixels) = &self.Cache else { return };
        let Options = TextureOptions {
            magnification: egui::TextureFilter::Nearest,
            minification: if Smooth { egui::TextureFilter::Linear } else { egui::TextureFilter::Nearest },
            ..TextureOptions::NEAREST
        };
        let Size = [Pixels.width() as usize, Pixels.height() as usize];
        let SizeChanged = self.Texture.as_ref().is_none_or(|Texture| Texture.size() != Size);
        if self.FullDirty || SizeChanged || self.TextureSmooth != Smooth {
            let Image = egui::ColorImage::from_rgba_unmultiplied(Size, Pixels.data());
            match &mut self.Texture {
                Some(Texture) if !SizeChanged => Texture.set(Image, Options),
                _ => self.Texture = Some(Context.load_texture(Name, Image, Options)),
            }
            self.TextureSmooth = Smooth;
        } else if !self.Dirty.is_empty() {
            let Region = self.Dirty.clamp_to(Pixels.width(), Pixels.height());
            if !Region.is_empty() {
                let Patch = Pixels.crop(Region);
                let Image = egui::ColorImage::from_rgba_unmultiplied([Patch.width() as usize, Patch.height() as usize], Patch.data());
                if let Some(Texture) = &mut self.Texture {
                    Texture.set_partial([Region.X as usize, Region.Y as usize], Image, Options);
                }
            }
        }
        self.FullDirty = false;
        self.Dirty = IRect::Empty;
    }
}

pub fn to_color32(Color: despite_core::Color) -> egui::Color32 {
    let [Red, Green, Blue, Alpha] = Color.to_rgba8();
    egui::Color32::from_rgba_unmultiplied(Red, Green, Blue, Alpha)
}

pub fn from_color32(Color: egui::Color32) -> despite_core::Color {
    let [Red, Green, Blue, Alpha] = Color.to_srgba_unmultiplied();
    despite_core::Color::from_rgba8([Red, Green, Blue, Alpha])
}

pub fn raster_texture(Context: &egui::Context, Name: &str, Pixels: &Raster, Smooth: bool) -> TextureHandle {
    let Image = egui::ColorImage::from_rgba_unmultiplied([Pixels.width() as usize, Pixels.height() as usize], Pixels.data());
    let Options = if Smooth {
        TextureOptions { magnification: egui::TextureFilter::Nearest, ..TextureOptions::LINEAR }
    } else {
        TextureOptions::NEAREST
    };
    Context.load_texture(Name, Image, Options)
}
