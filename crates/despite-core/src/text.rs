use std::sync::{Arc, Mutex, OnceLock};

use cosmic_text::{Align, Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, Weight, Wrap};
use serde::{Deserialize, Serialize};

use crate::blend::{self, BlendMode};
use crate::color::Color;
use crate::raster::Raster;
use crate::selection::Mask;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    pub const All: &'static [TextAlign] = &[TextAlign::Left, TextAlign::Center, TextAlign::Right];

    pub fn name(self) -> &'static str {
        match self {
            TextAlign::Left => "Left",
            TextAlign::Center => "Center",
            TextAlign::Right => "Right",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextStyle {
    pub Family: String,
    pub Size: f32,
    pub LineHeight: f32,
    pub Bold: bool,
    pub Italic: bool,
    pub Paint: Color,
    pub Align: TextAlign,
    pub Antialias: bool,
    pub OutlineWidth: u32,
    pub OutlineColor: Color,
    pub WrapWidth: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            Family: String::new(),
            Size: 24.0,
            LineHeight: 1.2,
            Bold: false,
            Italic: false,
            Paint: Color::Black,
            Align: TextAlign::Left,
            Antialias: true,
            OutlineWidth: 0,
            OutlineColor: Color::White,
            WrapWidth: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextData {
    pub Content: String,
    pub Style: TextStyle,
}

impl Default for TextData {
    fn default() -> Self {
        Self { Content: String::new(), Style: TextStyle::default() }
    }
}

pub struct TextEngine {
    Fonts: FontSystem,
    Cache: SwashCache,
    Families: Arc<Vec<String>>,
}

static Engine: OnceLock<Mutex<TextEngine>> = OnceLock::new();

pub fn engine() -> &'static Mutex<TextEngine> {
    Engine.get_or_init(|| Mutex::new(TextEngine::new()))
}

pub fn engine_if_ready() -> Option<&'static Mutex<TextEngine>> {
    Engine.get()
}

pub fn start_loading() {
    if Engine.get().is_none() {
        let _ = std::thread::Builder::new().name("despite-fonts".into()).spawn(|| {
            let _ = engine();
        });
    }
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEngine {
    pub fn new() -> TextEngine {
        let mut Fonts = FontSystem::new();
        {
            let Database = Fonts.db_mut();
            for Folder in ["/system/fonts", "/system/font", "/data/fonts", "/System/Library/Fonts", "/System/Library/Fonts/Core", "/System/Library/Fonts/CoreUI"] {
                let Path = std::path::Path::new(Folder);
                if Path.is_dir() {
                    Database.load_fonts_dir(Path);
                }
            }
            Database.load_font_data(epaint_default_fonts::UBUNTU_LIGHT.to_vec());
            Database.load_font_data(epaint_default_fonts::HACK_REGULAR.to_vec());
            Database.load_font_data(epaint_default_fonts::NOTO_EMOJI_REGULAR.to_vec());
        }
        let mut Names: Vec<String> = Fonts.db().faces().flat_map(|Face| Face.families.iter().map(|(Name, _)| Name.clone())).collect();
        Names.sort_by_key(|Name| Name.to_lowercase());
        Names.dedup();
        TextEngine { Fonts, Cache: SwashCache::new(), Families: Arc::new(Names) }
    }

    pub fn families(&self) -> Arc<Vec<String>> {
        self.Families.clone()
    }

    pub fn load_font_file(&mut self, Bytes: Vec<u8>) -> Vec<String> {
        let Before = self.Fonts.db().len();
        self.Fonts.db_mut().load_font_data(Bytes);
        let Added: Vec<String> = self.Fonts.db().faces().skip(Before).flat_map(|Face| Face.families.iter().map(|(Name, _)| Name.clone())).collect();
        let mut Names = (*self.Families).clone();
        Names.extend(Added.iter().cloned());
        Names.sort_by_key(|Name| Name.to_lowercase());
        Names.dedup();
        self.Families = Arc::new(Names);
        Added
    }

    fn layout(&mut self, Data: &TextData) -> (Buffer, f32, f32) {
        let Style = &Data.Style;
        let Size = Style.Size.clamp(1.0, 2000.0);
        let Metrics = Metrics::new(Size, Size * Style.LineHeight.clamp(0.5, 5.0));
        let mut Layout = Buffer::new(&mut self.Fonts, Metrics);
        let Wrapping = Style.WrapWidth > 0.0;
        Layout.set_wrap(if Wrapping { Wrap::WordOrGlyph } else { Wrap::None });
        Layout.set_size(Wrapping.then_some(Style.WrapWidth), None);
        let mut Attributes = Attrs::new()
            .weight(if Style.Bold { Weight::BOLD } else { Weight::NORMAL })
            .style(if Style.Italic { Style::Italic } else { Style::Normal });
        if !Style.Family.trim().is_empty() {
            Attributes = Attributes.family(Family::Name(&Style.Family));
        }
        let Alignment = match Style.Align {
            TextAlign::Left => Align::Left,
            TextAlign::Center => Align::Center,
            TextAlign::Right => Align::Right,
        };
        Layout.set_text(&Data.Content, &Attributes, Shaping::Advanced, Some(Alignment));
        Layout.shape_until_scroll(&mut self.Fonts, false);
        let mut Width = 0f32;
        let mut Height = 0f32;
        for Run in Layout.layout_runs() {
            Width = Width.max(Run.line_w);
            Height = Height.max(Run.line_top + Run.line_height);
        }
        if Wrapping {
            Width = Style.WrapWidth;
        } else if Style.Align != TextAlign::Left {
            Layout.set_size(Some(Width.ceil().max(1.0)), None);
            Layout.shape_until_scroll(&mut self.Fonts, false);
        }
        (Layout, Width, Height)
    }

    pub fn caret(&mut self, Data: &TextData) -> (f32, f32, f32) {
        let Style = &Data.Style;
        let LineHeight = Style.Size.clamp(1.0, 2000.0) * Style.LineHeight.clamp(0.5, 5.0);
        if Data.Content.is_empty() {
            return (0.0, 0.0, LineHeight);
        }
        let (Layout, _, _) = self.layout(Data);
        let mut Caret = (0.0, 0.0, LineHeight);
        let mut LastHadGlyphs = false;
        let mut Lines = 0;
        for Run in Layout.layout_runs() {
            let End = Run.glyphs.last().map(|Glyph| Glyph.x + Glyph.w).unwrap_or(0.0);
            Caret = (End, Run.line_top, Run.line_height);
            LastHadGlyphs = !Run.glyphs.is_empty();
            Lines += 1;
        }
        let Expected = Data.Content.split('\n').count();
        if Data.Content.ends_with('\n') && (LastHadGlyphs || Lines < Expected) {
            Caret = (0.0, Caret.1 + Caret.2, Caret.2);
        }
        Caret
    }

    pub fn render(&mut self, Data: &TextData) -> Option<Raster> {
        if Data.Content.trim().is_empty() {
            return None;
        }
        let Style = &Data.Style;
        let (Layout, Width, Height) = self.layout(Data);
        if Width <= 0.0 || Height <= 0.0 {
            return None;
        }
        let Pad = Style.OutlineWidth.min(64) as i32 + 2;
        let (CanvasWidth, CanvasHeight) = (Width.ceil() as i32 + Pad * 2, Height.ceil() as i32 + Pad * 2);
        if CanvasWidth as u32 > crate::raster::MaxDimension || CanvasHeight as u32 > crate::raster::MaxDimension {
            return None;
        }
        let mut Glyphs = Raster::new(CanvasWidth as u32, CanvasHeight as u32);
        let [Red, Green, Blue, Alpha] = Style.Paint.to_rgba8();
        let Paint = cosmic_text::Color::rgba(Red, Green, Blue, Alpha);
        let Antialias = Style.Antialias;
        let mut Layout = Layout;
        Layout.draw(&mut self.Fonts, &mut self.Cache, Paint, |X, Y, RectWidth, RectHeight, Shade| {
            let mut Source = [Shade.r(), Shade.g(), Shade.b(), Shade.a()];
            if !Antialias {
                Source[3] = if Source[3] >= 128 { Alpha } else { 0 };
            }
            if Source[3] == 0 {
                return;
            }
            for DrawY in Y..Y + RectHeight as i32 {
                for DrawX in X..X + RectWidth as i32 {
                    let (TargetX, TargetY) = (DrawX + Pad, DrawY + Pad);
                    if let Some(Existing) = Glyphs.get_checked(TargetX, TargetY) {
                        Glyphs.put(TargetX as u32, TargetY as u32, blend::composite_pixel(Existing, Source, 1.0, BlendMode::Normal));
                    }
                }
            }
        });
        if Style.OutlineWidth == 0 {
            return Some(Glyphs);
        }
        let mut Coverage = Mask::new(Glyphs.width(), Glyphs.height());
        for Y in 0..Glyphs.height() {
            for X in 0..Glyphs.width() {
                let Value = Glyphs.get(X, Y)[3];
                if Value >= if Antialias { 1 } else { 128 } {
                    Coverage.set(X, Y, if Antialias { Value } else { 255 });
                }
            }
        }
        Coverage.dilate(Style.OutlineWidth.min(64));
        let Edge = Style.OutlineColor.to_rgba8();
        let mut Out = Raster::new(Glyphs.width(), Glyphs.height());
        for Y in 0..Out.height() {
            for X in 0..Out.width() {
                let Amount = Coverage.get(X, Y);
                if Amount > 0 {
                    Out.put(X, Y, [Edge[0], Edge[1], Edge[2], ((Edge[3] as u32 * Amount as u32) / 255) as u8]);
                }
            }
        }
        Out.draw_over(&Glyphs, 0, 0, 1.0, BlendMode::Normal);
        Some(Out)
    }
}

pub fn render(Data: &TextData) -> Option<Raster> {
    engine().lock().ok()?.render(Data)
}

pub fn caret(Data: &TextData) -> Option<(f32, f32, f32)> {
    Some(engine().lock().ok()?.caret(Data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_moves_along_text() {
        let Style = TextStyle { Size: 20.0, ..Default::default() };
        let Short = caret(&TextData { Content: "Hi".into(), Style: Style.clone() }).expect("caret");
        let Long = caret(&TextData { Content: "Hi there".into(), Style: Style.clone() }).expect("caret");
        assert!(Long.0 > Short.0 + 10.0, "{Short:?} {Long:?}");
        assert_eq!(Short.1, 0.0);
        let Broken = caret(&TextData { Content: "Hi\n".into(), Style: Style.clone() }).expect("caret");
        assert_eq!(Broken.0, 0.0);
        assert!(Broken.1 > 10.0, "{Broken:?}");
        let Second = caret(&TextData { Content: "Hi\nthere".into(), Style }).expect("caret");
        assert!(Second.1 > 10.0 && Second.0 > 10.0, "{Second:?}");
    }

    #[test]
    fn renders_text_with_fallback_fonts() {
        let Data = TextData { Content: "Hi there\nDespite".into(), Style: TextStyle { Size: 20.0, ..Default::default() } };
        let Pixels = render(&Data).expect("text renders");
        assert!(Pixels.width() > 40 && Pixels.height() > 30, "{:?}", Pixels.size());
        assert!(Pixels.pixels().iter().any(|Pixel| Pixel[3] > 200));
        let Families = engine().lock().unwrap().families();
        assert!(!Families.is_empty());
    }

    #[test]
    fn pixel_text_is_binary_and_outline_grows() {
        let Plain = TextData { Content: "Pixel".into(), Style: TextStyle { Size: 12.0, Antialias: false, ..Default::default() } };
        let Pixels = render(&Plain).unwrap();
        assert!(Pixels.pixels().iter().all(|Pixel| Pixel[3] == 0 || Pixel[3] == 255));
        let Outlined = TextData { Style: TextStyle { OutlineWidth: 2, ..Plain.Style.clone() }, ..Plain.clone() };
        let WithEdge = render(&Outlined).unwrap();
        let Count = |Source: &Raster| Source.pixels().iter().filter(|Pixel| Pixel[3] > 0).count();
        assert!(Count(&WithEdge) > Count(&Pixels));
        assert!(render(&TextData { Content: "   ".into(), ..Default::default() }).is_none());
    }
}
