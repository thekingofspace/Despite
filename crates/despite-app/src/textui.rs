use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use despite_core::text::{TextAlign, TextData, TextStyle};
use despite_core::{Color, ImageId, LayerId, Raster};
use egui::{RichText, Ui};

use crate::app::DespiteApp;
use crate::panels::color_button;

pub struct TextSession {
    pub Image: ImageId,
    pub Layer: Option<LayerId>,
    pub X: f32,
    pub Y: f32,
    pub Data: TextData,
    pub Focus: bool,
    pub Anchor: egui::Pos2,
}

fn padding(Style: &TextStyle) -> f32 {
    Style.OutlineWidth.min(64) as f32 + 2.0
}

pub fn begin(App: &mut DespiteApp, Position: (f32, f32), Screen: egui::Pos2) {
    finish_session(App);
    let Some(Image) = App.active_image_id() else { return };
    let Existing = App.Project.image(Image).and_then(|Found| {
        let Entry = Found.active_layer()?;
        let Data = Entry.Text.clone()?;
        let Bounds = Entry.image_bounds();
        Bounds.contains(Position.0.floor() as i32, Position.1.floor() as i32).then(|| (Entry.Id, Data, Entry.Transform))
    });
    App.record("Text");
    App.TextSession = Some(match Existing {
        Some((LayerRef, Data, Transform)) => {
            let Pad = padding(&Data.Style);
            TextSession { Image, Layer: Some(LayerRef), X: Transform.OffsetX + Pad, Y: Transform.OffsetY + Pad, Data, Focus: true, Anchor: Screen }
        }
        None => {
            let mut Style = App.ToolOptions.Text.clone();
            Style.Paint = App.Primary;
            TextSession {
                Image,
                Layer: None,
                X: Position.0.floor(),
                Y: Position.1.floor(),
                Data: TextData { Content: String::new(), Style },
                Focus: true,
                Anchor: Screen,
            }
        }
    });
}

fn refresh(App: &mut DespiteApp) {
    let Some(Session) = &App.TextSession else { return };
    let (Image, X, Y, Data, Existing) = (Session.Image, Session.X, Session.Y, Session.Data.clone(), Session.Layer);
    let Pad = padding(&Data.Style);
    let Pixels = despite_core::text::render(&Data).unwrap_or_else(|| Raster::new(1, 1));
    let Name: String = {
        let Line = Data.Content.lines().next().unwrap_or("").trim();
        if Line.is_empty() { "Text".to_string() } else { format!("Text: {}", Line.chars().take(24).collect::<String>()) }
    };
    let LayerRef = match Existing {
        Some(Found) => Found,
        None => {
            if Data.Content.trim().is_empty() {
                return;
            }
            match App.Project.add_layer_from_raster(Image, &Name, Pixels.clone()) {
                Some(Created) => Created,
                None => return,
            }
        }
    };
    if let Some(Entry) = App.Project.image_mut(Image).and_then(|Found| Found.layer_mut(LayerRef)) {
        Entry.Pixels = Arc::new(Pixels);
        Entry.Transform.OffsetX = X - Pad;
        Entry.Transform.OffsetY = Y - Pad;
        Entry.Name = Name;
        Entry.Text = Some(Data);
    }
    if let Some(Session) = &mut App.TextSession {
        Session.Layer = Some(LayerRef);
    }
    App.Generation += 1;
    App.touch();
}

pub fn finish_session(App: &mut DespiteApp) {
    let Some(Session) = App.TextSession.take() else { return };
    let Empty = Session.Data.Content.trim().is_empty();
    if Session.Layer.is_none() && Empty && App.History.undo_label() == Some("Text") {
        App.History.Undo.pop();
        return;
    }
    if Empty {
        if let Some(LayerRef) = Session.Layer {
            if let Some(Found) = App.Project.image_mut(Session.Image) {
                if let Some(Index) = Found.layer_index(LayerRef) {
                    if Found.Layers.len() > 1 {
                        Found.Layers.remove(Index);
                        Found.clamp_active();
                    }
                }
            }
            App.touch();
        }
        return;
    }
    let mut Style = Session.Data.Style.clone();
    Style.Paint = App.ToolOptions.Text.Paint;
    App.ToolOptions.Text = Style;
    App.Generation += 1;
}

fn cancel_session(App: &mut DespiteApp) {
    if App.TextSession.take().is_some() && App.History.undo_label() == Some("Text") {
        App.History.undo(&mut App.Project);
        App.History.Redo.pop();
        App.Generation += 1;
        App.touch();
    }
}

#[derive(Default)]
struct FontPreviewCache {
    Textures: HashMap<(String, String, bool, bool, u32), Option<egui::TextureHandle>>,
    Pass: u64,
    Budget: u32,
}

thread_local! {
    static FontPreviews: RefCell<FontPreviewCache> = RefCell::new(FontPreviewCache::default());
}

const PreviewRow: f32 = 30.0;
const PreviewBudget: u32 = 10;
const PangramSample: &str = "The quick brown fox jumps over the lazy dog\nABCDEFGHIJKLMNOPQRSTUVWXYZ\nabcdefghijklmnopqrstuvwxyz 0123456789 !?&";

fn font_preview(Context: &egui::Context, Family: &str, Sample: &str, Style: &TextStyle, Points: f32) -> Option<egui::TextureHandle> {
    let PixelSize = (Points * Context.pixels_per_point()).round().max(4.0);
    let Key = (Family.to_string(), Sample.to_string(), Style.Bold, Style.Italic, PixelSize as u32);
    FontPreviews.with(|Cell| {
        let mut Cache = Cell.borrow_mut();
        if let Some(Found) = Cache.Textures.get(&Key) {
            return Found.clone();
        }
        let Pass = Context.cumulative_pass_nr();
        if Cache.Pass != Pass {
            Cache.Pass = Pass;
            Cache.Budget = PreviewBudget;
        }
        if Cache.Budget == 0 {
            Context.request_repaint();
            return None;
        }
        Cache.Budget -= 1;
        if Cache.Textures.len() > 3000 {
            Cache.Textures.clear();
        }
        let Data = TextData {
            Content: Sample.to_string(),
            Style: TextStyle {
                Family: Family.to_string(),
                Size: PixelSize,
                Bold: Style.Bold,
                Italic: Style.Italic,
                Paint: Color::White,
                Antialias: true,
                OutlineWidth: 0,
                WrapWidth: 0.0,
                Align: TextAlign::Left,
                ..TextStyle::default()
            },
        };
        let Texture = despite_core::text::render(&Data).map(|Pixels| {
            let Image = egui::ColorImage::from_rgba_unmultiplied([Pixels.width() as usize, Pixels.height() as usize], Pixels.data());
            Context.load_texture(format!("FontPreview{}", Cache.Textures.len()), Image, egui::TextureOptions::LINEAR)
        });
        Cache.Textures.insert(Key, Texture.clone());
        Texture
    })
}

fn paint_preview(Ui: &Ui, Texture: &egui::TextureHandle, Area: egui::Rect, Tint: egui::Color32) {
    let Scale = Ui.ctx().pixels_per_point();
    let mut Size = Texture.size_vec2() / Scale;
    let Shrink = (Area.height() / Size.y).min(1.0);
    Size *= Shrink;
    let Placed = egui::Rect::from_min_size(egui::pos2(Area.left(), Area.center().y - Size.y / 2.0), Size);
    Ui.painter_at(Area).image(Texture.id(), Placed, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Tint);
}

fn big_preview(Ui: &mut Ui, Family: &str, Style: &TextStyle, Sample: &str) {
    let Title = if Family.is_empty() { "Default font" } else { Family };
    Ui.label(RichText::new(Title).strong());
    let Text = if Sample.trim().is_empty() { PangramSample.to_string() } else { format!("{}\n{PangramSample}", Sample.trim()) };
    match font_preview(Ui.ctx(), Family, &Text, Style, 24.0) {
        Some(Texture) => {
            let Scale = Ui.ctx().pixels_per_point();
            let mut Size = Texture.size_vec2() / Scale;
            let MaxWidth = (Ui.ctx().content_rect().width() - 40.0).clamp(200.0, 560.0);
            if Size.x > MaxWidth {
                Size *= MaxWidth / Size.x;
            }
            Ui.add(egui::Image::new(egui::load::SizedTexture::new(Texture.id(), Size)).tint(Ui.visuals().text_color()));
        }
        None => {
            Ui.spinner();
        }
    }
}

pub fn font_list(Ui: &mut Ui, Style: &mut TextStyle, SearchId: egui::Id, SampleId: egui::Id, Sample: &str) -> bool {
    let mut Changed = false;
    let Some(Engine) = despite_core::text::engine_if_ready() else {
        Ui.horizontal(|Ui| {
            Ui.spinner();
            Ui.label("Loading fonts…");
        });
        Ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        return false;
    };
    let Screen = Ui.ctx().content_rect();
    Ui.set_min_width((Screen.width() - 40.0).clamp(200.0, 320.0));
    let Families = Engine.lock().map(|Locked| Locked.families()).unwrap_or_default();
    let mut Search: String = Ui.data(|Data| Data.get_temp(SearchId)).unwrap_or_default();
    let mut Custom: String = Ui.data(|Data| Data.get_temp(SampleId)).unwrap_or_default();
    Ui.add(egui::TextEdit::singleline(&mut Search).hint_text("Search fonts").desired_width(f32::INFINITY));
    Ui.add(egui::TextEdit::singleline(&mut Custom).hint_text("Preview text (optional)").desired_width(f32::INFINITY));
    let Needle = Search.to_lowercase();
    let Matches: Vec<Option<&String>> = std::iter::once(None)
        .filter(|_| Needle.is_empty())
        .chain(Families.iter().filter(|Name| Needle.is_empty() || Name.to_lowercase().contains(&Needle)).map(Some))
        .collect();
    Ui.label(RichText::new(format!("{} of {} fonts · hover one for a bigger preview", Matches.len().saturating_sub(Needle.is_empty() as usize), Families.len())).weak().small());
    let Typed: String = {
        let Source = if Custom.trim().is_empty() { Sample } else { Custom.as_str() };
        Source.lines().find(|Line| !Line.trim().is_empty()).unwrap_or("").trim().chars().take(40).collect()
    };
    egui::ScrollArea::vertical().max_height(320.0).auto_shrink([false, true]).show_rows(Ui, PreviewRow, Matches.len(), |Ui, Range| {
        for Entry in &Matches[Range] {
            let Name = Entry.map(|Found| Found.as_str()).unwrap_or("");
            let Shown = if Name.is_empty() { "Default font" } else { Name };
            let RowText = if Typed.is_empty() { Shown.to_string() } else { Typed.clone() };
            let Selected = Style.Family == Name;
            let (Rect, Response) = Ui.allocate_exact_size(egui::vec2(Ui.available_width(), PreviewRow), egui::Sense::click());
            if Ui.is_rect_visible(Rect) {
                let Visuals = Ui.style().interact_selectable(&Response, Selected);
                if Selected || Response.hovered() {
                    Ui.painter().rect_filled(Rect, 3.0, Visuals.weak_bg_fill);
                }
                let mut Area = Rect.shrink2(egui::vec2(6.0, 2.0));
                if !Typed.is_empty() {
                    let Galley = Ui.painter().layout_no_wrap(Shown.to_string(), egui::FontId::proportional(10.0), Ui.visuals().weak_text_color());
                    let Width = Galley.size().x.min(Area.width() * 0.45);
                    Ui.painter_at(Rect).galley(egui::pos2(Area.right() - Width, Area.top()), Galley, Ui.visuals().weak_text_color());
                    Area.max.x -= Width + 6.0;
                }
                match font_preview(Ui.ctx(), Name, &RowText, Style, 18.0) {
                    Some(Texture) => paint_preview(Ui, &Texture, Area, Visuals.text_color()),
                    None => {
                        Ui.painter_at(Area).text(Area.left_center(), egui::Align2::LEFT_CENTER, Shown, egui::FontId::proportional(14.0), Visuals.text_color());
                    }
                }
            }
            let Response = Response.on_hover_ui(|Ui| big_preview(Ui, Name, Style, &Typed));
            if Response.clicked() {
                Style.Family = Name.to_string();
                Changed = true;
            }
        }
    });
    Ui.data_mut(|Data| {
        Data.insert_temp(SearchId, Search);
        Data.insert_temp(SampleId, Custom);
    });
    Changed
}

pub fn font_picker(Ui: &mut Ui, Style: &mut TextStyle, Salt: &str, Sample: &str) -> bool {
    let mut Changed = false;
    let Label = if Style.Family.trim().is_empty() { "Default font".to_string() } else { Style.Family.clone() };
    let SearchId = egui::Id::unique((Salt, "FontSearch"));
    let SampleId = egui::Id::unique((Salt, "FontSample"));
    let Combo = egui::ComboBox::from_id_salt((Salt, "FontPicker")).selected_text(Label).width(190.0).height(420.0).show_ui(Ui, |Ui| {
        Changed |= font_list(Ui, Style, SearchId, SampleId, Sample);
    });
    Combo.response.on_hover_ui(|Ui| big_preview(Ui, &Style.Family.clone(), Style, Sample));
    #[cfg(feature = "desktop")]
    if Ui.small_button("Load font…").on_hover_text("Use a .ttf / .otf file that isn't installed").clicked() {
        if let Some(Path) = crate::platform::dialogs::open_file("Fonts", &["ttf", "otf", "ttc"]) {
            if let Ok(Bytes) = std::fs::read(&Path) {
                if let Ok(mut Engine) = despite_core::text::engine().lock() {
                    if let Some(First) = Engine.load_font_file(Bytes).into_iter().next() {
                        Style.Family = First;
                        Changed = true;
                    }
                }
            }
        }
    }
    Changed
}

pub fn style_controls(Ui: &mut Ui, Style: &mut TextStyle, Salt: &str, ShowColor: bool, Sample: &str) -> bool {
    let mut Changed = font_picker(Ui, Style, Salt, Sample);
    Ui.label("Size");
    Changed |= Ui.add(egui::DragValue::new(&mut Style.Size).range(1.0..=2000.0).speed(0.5)).changed();
    Changed |= Ui.toggle_value(&mut Style.Bold, RichText::new("B").strong()).on_hover_text("Bold").changed();
    Changed |= Ui.toggle_value(&mut Style.Italic, RichText::new("I").italics()).on_hover_text("Italic").changed();
    egui::ComboBox::from_id_salt((Salt, "TextAlign")).selected_text(Style.Align.name()).width(70.0).show_ui(Ui, |Ui| {
        for Align in TextAlign::All {
            Changed |= Ui.selectable_value(&mut Style.Align, *Align, Align.name()).changed();
        }
    });
    if ShowColor {
        Changed |= color_button(Ui, &mut Style.Paint);
    }
    Changed |= Ui.checkbox(&mut Style.Antialias, "Smooth").on_hover_text("Turn off for crisp pixel-art text").changed();
    Ui.label("Outline");
    Changed |= Ui.add(egui::DragValue::new(&mut Style.OutlineWidth).range(0..=64)).changed();
    if Style.OutlineWidth > 0 {
        Changed |= color_button(Ui, &mut Style.OutlineColor);
    }
    Changed
}

pub fn draw_highlight(App: &DespiteApp, Painter: &egui::Painter, Canvas: egui::Rect, Zoom: f32, Image: ImageId) {
    let Some(Session) = &App.TextSession else { return };
    if Session.Image != Image {
        return;
    }
    let ToScreen = |X: f32, Y: f32| Canvas.min + egui::vec2(X, Y) * Zoom;
    let Accent = Painter.ctx().global_style().visuals.selection.bg_fill;
    let Fallback = Session.Data.Style.Size * Session.Data.Style.LineHeight;
    let (CaretX, CaretY, CaretHeight) = despite_core::text::caret(&Session.Data).unwrap_or((0.0, 0.0, Fallback));
    let CaretTop = ToScreen(Session.X + CaretX, Session.Y + CaretY);
    let CaretBottom = CaretTop + egui::vec2(0.0, (CaretHeight * Zoom).max(8.0));
    let Caret = egui::Rect::from_min_max(CaretTop, CaretBottom);
    let Bounds = Session
        .Layer
        .filter(|_| !Session.Data.Content.trim().is_empty())
        .and_then(|LayerRef| App.Project.image(Image)?.layer(LayerRef))
        .map(|Entry| {
            let Area = Entry.image_bounds();
            egui::Rect::from_min_max(ToScreen(Area.X as f32, Area.Y as f32), ToScreen((Area.X + Area.W) as f32, (Area.Y + Area.H) as f32))
        });
    let Highlight = Bounds.map_or(Caret, |Found| Found.union(Caret)).expand(4.0);
    Painter.rect_filled(Highlight, 3.0, Accent.gamma_multiply(0.16));
    let Corners = [Highlight.left_top(), Highlight.right_top(), Highlight.right_bottom(), Highlight.left_bottom(), Highlight.left_top()];
    Painter.add(egui::Shape::line(Corners.to_vec(), egui::Stroke::new(2.0, egui::Color32::from_black_alpha(120))));
    Painter.extend(egui::Shape::dashed_line(&Corners, egui::Stroke::new(1.0, Accent), 5.0, 3.0));
    let Time = Painter.ctx().input(|Input| Input.time);
    if (Time * 1.8).floor() as i64 % 2 == 0 {
        let Width = (Zoom * 0.15).clamp(1.5, 3.0);
        Painter.line_segment([CaretTop, CaretBottom], egui::Stroke::new(Width + 2.0, egui::Color32::from_black_alpha(140)));
        Painter.line_segment([CaretTop, CaretBottom], egui::Stroke::new(Width, egui::Color32::WHITE));
    }
    Painter.ctx().request_repaint_after(std::time::Duration::from_millis(280));
}

pub fn text_window(App: &mut DespiteApp, Context: &egui::Context) {
    let Valid = App.TextSession.as_ref().is_some_and(|Session| App.Project.image(Session.Image).is_some());
    if !Valid {
        App.TextSession = None;
        return;
    }
    let Screen = Context.content_rect();
    let Compact = App.Compact;
    let mut Changed = false;
    let mut Apply = false;
    let mut Cancel = false;
    let mut Open = true;
    let Session = App.TextSession.as_mut().expect("validated above");
    let Width = if Compact { (Screen.width() - 24.0).max(200.0) } else { 360.0 };
    let mut Window = egui::Window::new("Text").open(&mut Open).collapsible(false).resizable(true).default_width(Width);
    Window = if Compact {
        Window.anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
    } else {
        Window.default_pos(Session.Anchor + egui::vec2(16.0, 16.0))
    };
    Window.show(Context, |Ui| {
        let Edit = Ui.add(
            egui::TextEdit::multiline(&mut Session.Data.Content)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text("Type your text…"),
        );
        if Session.Focus {
            Edit.request_focus();
            Session.Focus = false;
        }
        Changed |= Edit.changed();
        Ui.horizontal_wrapped(|Ui| {
            let Data = &mut Session.Data;
            Changed |= style_controls(Ui, &mut Data.Style, "TextWindow", true, &Data.Content);
        });
        Ui.horizontal_wrapped(|Ui| {
            Ui.label("Line height");
            Changed |= Ui.add(egui::DragValue::new(&mut Session.Data.Style.LineHeight).range(0.5..=5.0).speed(0.01)).changed();
            Ui.label("Wrap width");
            Changed |= Ui.add(egui::DragValue::new(&mut Session.Data.Style.WrapWidth).range(0.0..=8000.0).speed(1.0)).on_hover_text("0 = no wrapping").changed();
        });
        Ui.horizontal(|Ui| {
            Apply = crate::icons::text_button(Ui, "Check", RichText::new("Apply").strong()).clicked();
            Cancel = crate::icons::text_button(Ui, "Close", "Cancel").clicked();
        });
        Ui.label(RichText::new("Ctrl+Enter applies · Esc cancels · move, scale or rotate it later with the Move tool").weak().small());
    });
    let (CommandEnter, Escape) = Context.input(|Input| (Input.modifiers.command && Input.key_pressed(egui::Key::Enter), Input.key_pressed(egui::Key::Escape)));
    if Changed {
        refresh(App);
    }
    if Apply || CommandEnter || !Open {
        finish_session(App);
    } else if Cancel || Escape {
        cancel_session(App);
    }
}
