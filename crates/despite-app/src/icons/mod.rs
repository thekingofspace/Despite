mod actions;
mod rig;
mod tools;

use std::collections::HashMap;

use egui::{Color32, Response, Ui, Vec2};

pub const IconSize: f32 = 16.0;

fn mask(Name: &str) -> Option<&'static [&'static str; 16]> {
    tools::Names.iter().chain(actions::Names.iter()).chain(rig::Names.iter()).find(|(Candidate, _)| *Candidate == Name).map(|(_, Rows)| *Rows)
}


fn rasterize(Rows: &[&str; 16]) -> egui::ColorImage {
    let mut Pixels = Vec::with_capacity(256);
    for Row in Rows {
        let mut Count = 0;
        for Character in Row.chars().take(16) {
            let Alpha = match Character {
                '#' => 255,
                '+' => 153,
                '-' => 77,
                _ => 0,
            };
            Pixels.push(Color32::from_rgba_unmultiplied(255, 255, 255, Alpha));
            Count += 1;
        }
        for _ in Count..16 {
            Pixels.push(Color32::TRANSPARENT);
        }
    }
    egui::ColorImage::new([16, 16], Pixels)
}

#[derive(Clone, Default)]
struct IconCache {
    Textures: HashMap<&'static str, egui::TextureHandle>,
}

pub fn texture(Context: &egui::Context, Name: &'static str) -> Option<egui::TextureId> {
    let Id = egui::Id::unique("DespiteIconCache");
    if let Some(Found) = Context.data(|Data| Data.get_temp::<IconCache>(Id)).and_then(|Cache| Cache.Textures.get(Name).map(|Texture| Texture.id())) {
        return Some(Found);
    }
    let Rows = mask(Name)?;
    let Texture = Context.load_texture(format!("icon-{Name}"), rasterize(Rows), egui::TextureOptions::NEAREST);
    let TextureId = Texture.id();
    Context.data_mut(|Data| {
        let Cache = Data.get_temp_mut_or_default::<IconCache>(Id);
        Cache.Textures.insert(Name, Texture);
    });
    Some(TextureId)
}

pub fn scale_for(Ui: &Ui) -> f32 {
    let PixelsPerPoint = Ui.ctx().pixels_per_point();
    let Physical = (IconSize * PixelsPerPoint).round();
    let Multiple = (Physical / IconSize).round().max(1.0);
    Multiple * IconSize / PixelsPerPoint / IconSize
}

pub fn image(Ui: &Ui, Name: &'static str, Tint: Color32) -> Option<egui::Image<'static>> {
    let Id = texture(Ui.ctx(), Name)?;
    let Size = Vec2::splat(IconSize * scale_for(Ui));
    Some(egui::Image::from_texture((Id, Size)).fit_to_exact_size(Size).tint(Tint))
}

pub fn show(Ui: &mut Ui, Name: &'static str) -> Response {
    let Tint = Ui.visuals().text_color();
    match image(Ui, Name, Tint) {
        Some(Icon) => Ui.add(Icon),
        None => Ui.label(Name),
    }
}

pub fn button(Ui: &mut Ui, Name: &'static str, Selected: bool, Tooltip: &str) -> Response {
    let Tint = if Selected { Ui.visuals().strong_text_color() } else { Ui.visuals().text_color() };
    let Response = match image(Ui, Name, Tint) {
        Some(Icon) => Ui.add(egui::Button::image(Icon).selected(Selected)),
        None => Ui.add(egui::Button::new(Name).selected(Selected)),
    };
    if Tooltip.is_empty() { Response } else { Response.on_hover_text(Tooltip) }
}

pub fn large_button(Ui: &mut Ui, Name: &'static str, Selected: bool, Tooltip: &str, Side: f32) -> Response {
    let Tint = if Selected { Ui.visuals().strong_text_color() } else { Ui.visuals().text_color() };
    let Response = match texture(Ui.ctx(), Name) {
        Some(Id) => {
            let Scale = ((Side - 10.0) / IconSize).floor().max(1.0);
            let Size = Vec2::splat(IconSize * Scale);
            Ui.add(egui::Button::image(egui::Image::from_texture((Id, Size)).fit_to_exact_size(Size).tint(Tint)).selected(Selected).min_size(Vec2::splat(Side)))
        }
        None => Ui.add(egui::Button::new(Name).selected(Selected).min_size(Vec2::splat(Side))),
    };
    if Tooltip.is_empty() { Response } else { Response.on_hover_text(Tooltip) }
}

pub fn text_button(Ui: &mut Ui, Name: &'static str, Text: impl Into<egui::WidgetText>) -> Response {
    let Tint = Ui.visuals().text_color();
    match image(Ui, Name, Tint) {
        Some(Icon) => Ui.add(egui::Button::image_and_text(Icon, Text)),
        None => Ui.add(egui::Button::new(Text)),
    }
}

pub fn toggle(Ui: &mut Ui, Value: &mut bool, On: &'static str, Off: &'static str, Tooltip: &str) -> Response {
    let mut Response = button(Ui, if *Value { On } else { Off }, *Value, Tooltip);
    if Response.clicked() {
        *Value = !*Value;
        Response.mark_changed();
    }
    Response
}

pub fn row_toggle(Ui: &mut Ui, Value: &mut bool, On: &'static str, Off: &'static str, Tooltip: &str) -> Response {
    let Tint = if *Value { Ui.visuals().strong_text_color() } else { Ui.visuals().weak_text_color().gamma_multiply(0.7) };
    let mut Response = match image(Ui, if *Value { On } else { Off }, Tint) {
        Some(Icon) => Ui.add(egui::Button::image(Icon).frame(false)),
        None => Ui.add(egui::Button::new(if *Value { On } else { Off }).frame(false)),
    };
    if Response.clicked() {
        *Value = !*Value;
        Response.mark_changed();
    }
    Response.on_hover_text(Tooltip)
}

pub fn selectable<V: PartialEq>(Ui: &mut Ui, Current: &mut V, Target: V, Name: &'static str, Text: impl Into<egui::WidgetText>) -> Response {
    let Selected = *Current == Target;
    let Tint = if Selected { Ui.visuals().strong_text_color() } else { Ui.visuals().text_color() };
    let Response = match image(Ui, Name, Tint) {
        Some(Icon) => Ui.add(egui::Button::image_and_text(Icon, Text).selected(Selected)),
        None => Ui.add(egui::Button::new(Text).selected(Selected)),
    };
    if Response.clicked() {
        *Current = Target;
    }
    Response
}

pub fn toggle_text(Ui: &mut Ui, Value: &mut bool, Name: &'static str, Text: impl Into<egui::WidgetText>) -> Response {
    let Tint = if *Value { Ui.visuals().strong_text_color() } else { Ui.visuals().text_color() };
    let mut Response = match image(Ui, Name, Tint) {
        Some(Icon) => Ui.add(egui::Button::image_and_text(Icon, Text).selected(*Value)),
        None => Ui.add(egui::Button::new(Text).selected(*Value)),
    };
    if Response.clicked() {
        *Value = !*Value;
        Response.mark_changed();
    }
    Response
}

pub fn enabled_button(Ui: &mut Ui, Enabled: bool, Name: &'static str, Tooltip: &str) -> Response {
    Ui.add_enabled_ui(Enabled, |Ui| button(Ui, Name, false, Tooltip)).inner
}

pub fn paint(Painter: &egui::Painter, Context: &egui::Context, Name: &'static str, Rect: egui::Rect, Tint: Color32) {
    if let Some(Id) = texture(Context, Name) {
        Painter.image(Id, Rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Tint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_are_well_formed() {
        let mut Seen = std::collections::HashSet::new();
        for (Name, Rows) in tools::Names.iter().chain(actions::Names.iter()) {
            assert!(Seen.insert(*Name), "duplicate icon {Name}");
            for Row in Rows.iter() {
                assert_eq!(Row.chars().count(), 16, "{Name} row width");
                assert!(Row.chars().all(|Character| matches!(Character, '#' | '+' | '-' | '.')), "{Name} has invalid characters");
            }
        }
        assert!(Seen.len() >= 70);
    }
}
