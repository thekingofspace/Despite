use std::collections::HashMap;

use despite_core::brush::{BrushMode, DitherPattern, SymmetryMode, TipAnimation, TipImage, TipShape};
use despite_core::fill::GradientKind;
use despite_core::params::{ParamDecl, ParamKind, ParamValue};
use despite_core::{BlendMode, Brush, Color, DocumentRef, LayerTransform, Overlay, OverlaySource, Playback, Project, SceneItem};
use egui::{Color32, RichText, Ui};

use crate::app::{DespiteApp, human_bytes};
use crate::dialogs::{self, Dialog};
use crate::tools::{SelectionMode, Tool};
use crate::view::{from_color32, to_color32};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RightTab {
    Layers,
    Scene,
    Brush,
    Color,
    Guides,
    History,
}

impl RightTab {
    const All: [RightTab; 6] = [RightTab::Layers, RightTab::Scene, RightTab::Brush, RightTab::Color, RightTab::Guides, RightTab::History];

    fn name(self) -> &'static str {
        match self {
            RightTab::Layers => "Layers",
            RightTab::Scene => "Scene",
            RightTab::Brush => "Brush",
            RightTab::Color => "Color",
            RightTab::Guides => "Guides",
            RightTab::History => "History",
        }
    }
}

pub fn blend_combo(Ui: &mut Ui, Id: &str, Value: &mut BlendMode) -> bool {
    let mut Changed = false;
    egui::ComboBox::from_id_salt(Id).selected_text(Value.name()).show_ui(Ui, |Ui| {
        for Mode in BlendMode::All {
            Changed |= Ui.selectable_value(Value, *Mode, Mode.name()).changed();
        }
    });
    Changed
}

pub fn color_button(Ui: &mut Ui, Value: &mut Color) -> bool {
    let mut Shown = to_color32(*Value);
    let Changed = egui::color_picker::color_edit_button_srgba(Ui, &mut Shown, egui::color_picker::Alpha::OnlyBlend).changed();
    if Changed {
        *Value = from_color32(Shown);
    }
    Changed
}

fn layer_thumbnail(App: &mut DespiteApp, Context: &egui::Context, Image: despite_core::ImageId, Index: usize, Budget: &mut u32) -> Option<egui::TextureId> {
    let Found = App.Project.image(Image)?;
    let Entry = Found.Layers.get(Index)?;
    let Key = Entry.Id;
    let Pointer = std::sync::Arc::as_ptr(&Entry.Pixels) as usize;
    let Generation = App.Generation;
    let Stale = App.LayerThumbs.get(&Key).is_none_or(|(_, SeenPointer, SeenGeneration)| *SeenPointer != Pointer || *SeenGeneration != Generation);
    if Stale && *Budget > 0 {
        *Budget -= 1;
        let Pixels = &Entry.Pixels;
        let Longest = Pixels.width().max(Pixels.height()).max(1);
        let Scale = 40.0 / Longest as f32;
        let Small = if Scale >= 1.0 {
            (**Pixels).clone()
        } else {
            let Width = ((Pixels.width() as f32 * Scale).round() as u32).max(1);
            let Height = ((Pixels.height() as f32 * Scale).round() as u32).max(1);
            if Found.PixelArt { Pixels.scaled_nearest(Width, Height) } else { Pixels.scaled_nearest(Width * 2, Height * 2).scaled_smooth(Width, Height) }
        };
        let Texture = crate::view::raster_texture(Context, &format!("layer-{}", Key.0), &Small, false);
        App.LayerThumbs.insert(Key, (Texture, Pointer, Generation));
    }
    App.LayerThumbs.get(&Key).map(|(Texture, _, _)| Texture.id())
}

fn paint_thumbnail(Ui: &Ui, Rect: egui::Rect, Texture: Option<egui::TextureId>, Dimmed: bool) {
    let Painter = Ui.painter();
    let Cell = 6.0;
    for Row in 0..(Rect.height() / Cell).ceil() as i32 {
        for Column in 0..(Rect.width() / Cell).ceil() as i32 {
            let Tile = egui::Rect::from_min_size(Rect.min + egui::vec2(Column as f32 * Cell, Row as f32 * Cell), egui::vec2(Cell, Cell)).intersect(Rect);
            Painter.rect_filled(Tile, 0.0, if (Row + Column) % 2 == 0 { Color32::from_gray(170) } else { Color32::from_gray(130) });
        }
    }
    if let Some(Id) = Texture {
        let Size = Ui.ctx().tex_manager().read().meta(Id).map(|Meta| Meta.size).unwrap_or([1, 1]);
        let Aspect = Size[0] as f32 / Size[1].max(1) as f32;
        let Fitted = if Aspect >= 1.0 { egui::vec2(Rect.width(), Rect.width() / Aspect) } else { egui::vec2(Rect.height() * Aspect, Rect.height()) };
        let Tint = if Dimmed { Color32::from_white_alpha(90) } else { Color32::WHITE };
        Painter.image(Id, egui::Rect::from_center_size(Rect.center(), Fitted), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Tint);
    }
    Painter.rect_stroke(Rect, 0.0, egui::Stroke::new(1.0, Color32::from_black_alpha(140)), egui::StrokeKind::Outside);
}

pub fn tool_palette(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.heading("Tools");
    egui::Grid::new("ToolGrid").spacing([4.0, 4.0]).show(Ui, |Ui| {
        for (Index, Candidate) in Tool::All.iter().enumerate() {
            let Tooltip = format!("{} ({})\n{}", Candidate.name(), Candidate.shortcut_label(), Candidate.hint());
            if crate::icons::large_button(Ui, Candidate.icon_name(), App.Tool == *Candidate, &Tooltip, 36.0).clicked() {
                App.set_tool(*Candidate);
            }
            if Index % 4 == 3 {
                Ui.end_row();
            }
        }
    });
}

pub fn color_swatches(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.horizontal(|Ui| {
        let PrimaryResponse = color_button(Ui, &mut App.Primary);
        let _ = PrimaryResponse;
        if crate::icons::button(Ui, "SwapColors", false, "Swap colors (X)").clicked() {
            App.swap_colors();
        }
        color_button(Ui, &mut App.Secondary);
        if crate::icons::button(Ui, "ResetColors", false, "Reset to black and white (D)").clicked() {
            App.Primary = Color::Black;
            App.Secondary = Color::White;
        }
    });
}

#[derive(Clone, Default)]
struct ThumbnailCache {
    Entries: HashMap<DocumentRef, (egui::TextureHandle, u64, f64)>,
}

fn thumbnail(App: &DespiteApp, Ui: &Ui, Document: DocumentRef, Budget: &mut u32) -> Option<egui::TextureId> {
    let Id = egui::Id::unique("DespiteThumbnails");
    let mut Cache: ThumbnailCache = Ui.data(|Data| Data.get_temp(Id).unwrap_or_default());
    let Existing = Cache.Entries.get(&Document).cloned();
    let Stale = match &Existing {
        None => true,
        Some((_, Generation, Time)) => *Generation != App.Generation && App.Now - Time > if App.Project.Active == Some(Document) { 1.0 } else { 3.0 },
    };
    if Stale && *Budget > 0 {
        *Budget -= 1;
        if let Some(Pixels) = App.Project.render_document(Document, 0) {
            let Longest = Pixels.width().max(Pixels.height()).max(1);
            let Scale = 48.0 / Longest as f32;
            let Small = if Scale < 1.0 {
                let Width = ((Pixels.width() as f32 * Scale).round() as u32).max(1);
                let Height = ((Pixels.height() as f32 * Scale).round() as u32).max(1);
                Pixels.scaled_smooth(Width, Height)
            } else {
                Pixels
            };
            let Texture = crate::view::raster_texture(Ui.ctx(), &format!("thumb-{Document:?}"), &Small, false);
            Cache.Entries.insert(Document, (Texture, App.Generation, App.Now));
            Ui.data_mut(|Data| Data.insert_temp(Id, Cache.clone()));
        }
    }
    Cache.Entries.get(&Document).map(|(Texture, _, _)| Texture.id())
}

fn document_row(App: &mut DespiteApp, Ui: &mut Ui, Document: DocumentRef, Label: String, Budget: &mut u32) {
    let Selected = App.Project.Active == Some(Document);
    Ui.horizontal(|Ui| {
        let Texture = thumbnail(App, Ui, Document, Budget);
        let (Rect, _) = Ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
        Ui.painter().rect_filled(Rect, 2.0, Color32::from_gray(60));
        if let Some(TextureId) = Texture {
            let Size = Ui.ctx().tex_manager().read().meta(TextureId).map(|Meta| Meta.size).unwrap_or([36, 36]);
            let Aspect = Size[0] as f32 / Size[1].max(1) as f32;
            let Fitted = if Aspect >= 1.0 { egui::vec2(36.0, 36.0 / Aspect) } else { egui::vec2(36.0 * Aspect, 36.0) };
            Ui.painter().image(TextureId, egui::Rect::from_center_size(Rect.center(), Fitted), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        let Response = Ui.selectable_label(Selected, Label);
        if Response.clicked() {
            App.activate(Document);
        }
        Response.context_menu(|Ui| {
            if Ui.button("Rename…").clicked() {
                let Current = App.Project.document_name(Document).unwrap_or_default().to_string();
                App.Dialog = Some(Dialog::Rename(dialogs::RenameState { Target: dialogs::RenameTarget::Document(Document), Text: Current }));
                Ui.close();
            }
            if Ui.button("Clone").clicked() {
                App.clone_document(Document);
                Ui.close();
            }
            if Ui.button("Copy").clicked() {
                let Context = Ui.ctx().clone();
                App.copy_document(&Context, Document);
                Ui.close();
            }
            if App.Clipboard.is_some() && Ui.button("Paste").clicked() {
                App.paste_smart();
                Ui.close();
            }
            if Ui.button("Export…").clicked() {
                App.activate(Document);
                App.Dialog = Some(Dialog::Export(dialogs::ExportState::new(App)));
                Ui.close();
            }
            if Ui.button("Delete").clicked() {
                App.delete_document(Document);
                Ui.close();
            }
        });
    });
}

pub fn project_browser(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.horizontal(|Ui| {
        Ui.heading("Project");
        Ui.label(RichText::new(App.Project.Kind.name()).weak());
    });
    if Ui.add(egui::Label::new(RichText::new(&App.Project.Name).strong()).sense(egui::Sense::click())).on_hover_text("Click to rename the project").clicked() {
        App.Dialog = Some(Dialog::Rename(dialogs::RenameState { Target: dialogs::RenameTarget::Project, Text: App.Project.Name.clone() }));
    }
    let mut Budget = 2u32;
    Ui.horizontal(|Ui| {
        Ui.label(RichText::new(format!("Images ({})", App.Project.Images.len())).strong());
        if crate::icons::button(Ui, "Add", false, "New image").clicked() {
            App.Dialog = Some(Dialog::NewImage(dialogs::NewImageState::new(&App.Settings)));
        }
    });
    let Images: Vec<(DocumentRef, String)> = App
        .Project
        .Images
        .iter()
        .map(|Found| (DocumentRef::Image(Found.Id), format!("{}  {}×{}", Found.Name, Found.Width, Found.Height)))
        .collect();
    for (Index, (Document, Label)) in Images.into_iter().enumerate() {
        document_row(App, Ui, Document, Label, &mut Budget);
        let _ = Index;
    }
    Ui.add_space(6.0);
    Ui.horizontal(|Ui| {
        Ui.label(RichText::new(format!("Scenes ({})", App.Project.Scenes.len())).strong());
        if crate::icons::button(Ui, "Add", false, "New scene: arrange many images together").clicked() {
            App.Dialog = Some(Dialog::NewScene(Default::default()));
        }
    });
    let Scenes: Vec<(DocumentRef, String)> = App
        .Project
        .Scenes
        .iter()
        .map(|Found| (DocumentRef::Scene(Found.Id), format!("{}  {}×{}  {}f", Found.Name, Found.Width, Found.Height, Found.FrameCount)))
        .collect();
    for (Document, Label) in Scenes {
        document_row(App, Ui, Document, Label, &mut Budget);
    }
    if Budget == 0 {
        Ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
    }
    Ui.add_space(6.0);
    if let Some(Document) = App.Project.Active {
        Ui.horizontal_wrapped(|Ui| {
            if Ui.button("Clone").on_hover_text("Duplicate the selected image or scene").clicked() {
                App.clone_document(Document);
            }
            if Ui.button("Delete").clicked() {
                App.delete_document(Document);
            }
            if let DocumentRef::Image(Id) = Document
                && let Some(Index) = App.Project.image_index(Id) {
                    if crate::icons::button(Ui, "ArrowUp", false, "Move up").clicked() && Index > 0 {
                        App.record("Reorder images");
                        App.Project.move_image(Index, Index - 1);
                        App.touch();
                    }
                    if crate::icons::button(Ui, "ArrowDown", false, "Move down").clicked() && Index + 1 < App.Project.Images.len() {
                        App.record("Reorder images");
                        App.Project.move_image(Index, Index + 1);
                        App.touch();
                    }
                }
        });
    }
}

pub fn right_panel(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.horizontal_wrapped(|Ui| {
        for Tab in RightTab::All {
            if Tab == RightTab::Scene && App.active_scene_id().is_none() {
                continue;
            }
            if Tab == RightTab::Layers && App.active_image_id().is_none() {
                continue;
            }
            Ui.selectable_value(&mut App.RightTab, Tab, Tab.name());
        }
    });
    if App.RightTab == RightTab::Layers && App.active_image_id().is_none() {
        App.RightTab = RightTab::Scene;
    }
    if App.RightTab == RightTab::Scene && App.active_scene_id().is_none() {
        App.RightTab = RightTab::Layers;
    }
    Ui.separator();
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(Ui, |Ui| match App.RightTab {
        RightTab::Layers => layers_panel(App, Ui),
        RightTab::Scene => scene_panel(App, Ui),
        RightTab::Brush => brush_panel(App, Ui),
        RightTab::Color => color_panel(App, Ui),
        RightTab::Guides => guides_panel(App, Ui),
        RightTab::History => history_panel(App, Ui),
    });
}

fn transform_editor(Ui: &mut Ui, Transform: &mut LayerTransform, Id: &str) -> bool {
    let mut Changed = false;
    egui::Grid::new(Id).num_columns(2).show(Ui, |Ui| {
        Ui.label("Position");
        Ui.horizontal(|Ui| {
            Changed |= Ui.add(egui::DragValue::new(&mut Transform.OffsetX).speed(1.0).prefix("x ")).changed();
            Changed |= Ui.add(egui::DragValue::new(&mut Transform.OffsetY).speed(1.0).prefix("y ")).changed();
        });
        Ui.end_row();
        Ui.label("Scale");
        Ui.horizontal(|Ui| {
            Changed |= Ui.add(egui::DragValue::new(&mut Transform.ScaleX).speed(0.01).range(0.01..=64.0).prefix("x ")).changed();
            Changed |= Ui.add(egui::DragValue::new(&mut Transform.ScaleY).speed(0.01).range(0.01..=64.0).prefix("y ")).changed();
        });
        Ui.end_row();
        Ui.label("Rotation");
        Changed |= Ui.add(egui::DragValue::new(&mut Transform.Rotation).speed(0.5).suffix("°")).changed();
        Ui.end_row();
        Ui.label("Flip");
        Ui.horizontal(|Ui| {
            Changed |= Ui.checkbox(&mut Transform.FlipX, "Horizontal").changed();
            Changed |= Ui.checkbox(&mut Transform.FlipY, "Vertical").changed();
        });
        Ui.end_row();
    });
    Changed
}

fn layers_panel(App: &mut DespiteApp, Ui: &mut Ui) {
    let Some(Id) = App.active_image_id() else { return };
    let Some(Found) = App.Project.image(Id) else { return };
    let (Name, Width, Height, mut PixelArt, mut Delay) = (Found.Name.clone(), Found.Width, Found.Height, Found.PixelArt, Found.FrameDelayMs);
    Ui.horizontal(|Ui| {
        Ui.label(RichText::new(Name).strong());
        Ui.label(format!("{Width}×{Height}"));
    });
    Ui.horizontal(|Ui| {
        if Ui.checkbox(&mut PixelArt, "Pixel art").on_hover_text("Nearest-neighbour sampling for transforms and scaling").changed() {
            App.with_active_layer("Pixel art mode", |Target| Target.PixelArt = PixelArt);
        }
        Ui.label("Frame ms");
        if Ui.add(egui::DragValue::new(&mut Delay).range(10..=10_000)).on_hover_text("Delay when this image is a GIF/video frame").changed() {
            App.record_coalesced("delay", "Frame delay");
            if let Some(Target) = App.Project.image_mut(Id) {
                Target.FrameDelayMs = Delay;
            }
            App.Modified = true;
        }
    });
    Ui.horizontal_wrapped(|Ui| {
        if crate::icons::text_button(Ui, "Add", "Layer").clicked() {
            App.new_layer();
        }
        if crate::icons::button(Ui, "Duplicate", false, "Duplicate layer").clicked() {
            App.duplicate_layer();
        }
        if crate::icons::button(Ui, "Delete", false, "Delete layer").clicked() {
            App.delete_layer();
        }
        if crate::icons::button(Ui, "MergeDown", false, "Merge down").clicked() {
            App.merge_down();
        }
        if crate::icons::button(Ui, "ArrowUp", false, "Move layer up").clicked() {
            App.move_layer(true);
        }
        if crate::icons::button(Ui, "ArrowDown", false, "Move layer down").clicked() {
            App.move_layer(false);
        }
    });
    Ui.separator();
    let Count = App.Project.image(Id).map(|Found| Found.Layers.len()).unwrap_or(0);
    let mut MoveRequest: Option<(usize, usize)> = None;
    let mut Budget = 4u32;
    for Index in (0..Count).rev() {
        let Some(Found) = App.Project.image(Id) else { return };
        let Entry = &Found.Layers[Index];
        let (mut Visible, mut Locked, mut AlphaLocked, LayerName, Opacity, Blend, LayerRef, BoilTag) =
            (Entry.Visible, Entry.Locked, Entry.AlphaLocked, Entry.Name.clone(), Entry.Opacity, Entry.Blend, Entry.Id, Entry.Boil);
        let BoilCount = BoilTag.and_then(|Tag| Found.boil_group_sizes().get(&Tag.Group).copied()).unwrap_or(0);
        let Active = Found.ActiveLayer == Index;
        let Thumb = layer_thumbnail(App, Ui.ctx(), Id, Index, &mut Budget);
        let (RowRect, RowResponse) = Ui.allocate_exact_size(egui::vec2(Ui.available_width(), 52.0), egui::Sense::click());
        let Visuals = Ui.visuals().clone();
        if Active {
            Ui.painter().rect_filled(RowRect, 4.0, Visuals.selection.bg_fill.gamma_multiply(0.45));
            Ui.painter().rect_stroke(RowRect, 4.0, egui::Stroke::new(1.0, Visuals.selection.bg_fill), egui::StrokeKind::Inside);
        } else if RowResponse.hovered() {
            Ui.painter().rect_filled(RowRect, 4.0, Visuals.widgets.hovered.weak_bg_fill);
        }
        if let Some(Dragged) = RowResponse.dnd_hover_payload::<usize>() {
            if *Dragged != Index {
                let Y = if *Dragged < Index { RowRect.top() } else { RowRect.bottom() };
                Ui.painter().hline(RowRect.x_range(), Y, egui::Stroke::new(2.0, Visuals.selection.bg_fill));
            }
        }
        if let Some(Dragged) = RowResponse.dnd_release_payload::<usize>() {
            MoveRequest = Some((*Dragged, Index));
        }
        let mut Child = Ui.new_child(egui::UiBuilder::new().max_rect(RowRect.shrink2(egui::vec2(4.0, 3.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        Child.dnd_drag_source(egui::Id::unique(("LayerDrag", LayerRef.0)), Index, |Ui| {
            let Weak = Ui.visuals().weak_text_color();
            if let Some(Icon) = crate::icons::image(Ui, "DragHandle", Weak) {
                Ui.add(Icon);
            }
        })
        .response
        .on_hover_text("Drag to reorder");
        let (ThumbRect, _) = Child.allocate_exact_size(egui::vec2(42.0, 42.0), egui::Sense::hover());
        paint_thumbnail(&Child, ThumbRect, Thumb, !Visible);
        if crate::icons::row_toggle(&mut Child, &mut Visible, "Visible", "Hidden", "Show / hide").changed() {
            App.record("Toggle visibility");
            if let Some(Target) = App.Project.image_mut(Id) {
                Target.Layers[Index].Visible = Visible;
            }
            App.touch();
        }
        if crate::icons::row_toggle(&mut Child, &mut Locked, "Locked", "Unlocked", "Lock layer").changed() {
            if let Some(Target) = App.Project.image_mut(Id) {
                Target.Layers[Index].Locked = Locked;
            }
            App.Modified = true;
        }
        if crate::icons::row_toggle(&mut Child, &mut AlphaLocked, "AlphaLock", "AlphaLock", "Lock transparency: paint only where pixels already exist").changed() {
            if let Some(Target) = App.Project.image_mut(Id) {
                Target.Layers[Index].AlphaLocked = AlphaLocked;
            }
            App.Modified = true;
        }
        let TextWidth = Child.available_width().max(20.0);
        Child.vertical(|Ui| {
            Ui.set_max_width(TextWidth);
            Ui.spacing_mut().item_spacing.y = 1.0;
            let Title = RichText::new(&LayerName);
            Ui.add(egui::Label::new(if Active { Title.strong() } else { Title }).truncate()).on_hover_text(&LayerName);
            let Detail = match BoilTag {
                Some(Tag) => format!("{:.0}% · {} · boil {}/{}", Opacity * 100.0, Blend.name(), Tag.Index + 1, BoilCount),
                None => format!("{:.0}% · {}", Opacity * 100.0, Blend.name()),
            };
            Ui.add(egui::Label::new(RichText::new(Detail).small().weak()).truncate());
        });
        if RowResponse.clicked()
            && let Some(Target) = App.Project.image_mut(Id)
        {
            Target.ActiveLayer = Index;
        }
        if RowResponse.double_clicked() {
            App.Dialog = Some(Dialog::Rename(dialogs::RenameState { Target: dialogs::RenameTarget::Layer(Id, Index), Text: LayerName.clone() }));
        }
        RowResponse.context_menu(|Ui| {
            if let Some(Target) = App.Project.image_mut(Id) {
                Target.ActiveLayer = Index;
            }
            if Ui.button("Rename…").clicked() {
                App.Dialog = Some(Dialog::Rename(dialogs::RenameState { Target: dialogs::RenameTarget::Layer(Id, Index), Text: LayerName.clone() }));
                Ui.close();
            }
            if Ui.button("Duplicate").clicked() {
                App.duplicate_layer();
                Ui.close();
            }
            if Ui.button("Copy layer").clicked() {
                let Context = Ui.ctx().clone();
                App.copy_layer(&Context);
                Ui.close();
            }
            if App.Clipboard.is_some() && Ui.button("Paste").clicked() {
                App.paste_smart();
                Ui.close();
            }
            if Ui.button("Merge down").clicked() {
                App.merge_down();
                Ui.close();
            }
            if let Some(Tag) = BoilTag
                && Ui.button("Remove boil (keep this frame)").clicked()
            {
                App.with_active_layer("Remove boil", |Target| Target.unboil(Tag.Group, Index));
                Ui.close();
            }
            if Ui.button("Delete").clicked() {
                App.delete_layer();
                Ui.close();
            }
        });
    }
    if Budget == 0 {
        Ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
    }
    if let Some((From, To)) = MoveRequest
        && From != To
    {
        App.record("Reorder layer");
        if let Some(Target) = App.Project.image_mut(Id) {
            Target.move_layer(From, To);
        }
        App.touch();
    }
    Ui.separator();
    let Some(Found) = App.Project.image(Id) else { return };
    let Some(Entry) = Found.active_layer() else { return };
    let Index = Found.ActiveLayer;
    let (mut Opacity, mut Blend, mut Transform) = (Entry.Opacity, Entry.Blend, Entry.Transform);
    Ui.label(RichText::new("Layer properties").strong());
    egui::Grid::new("LayerProps").num_columns(2).show(Ui, |Ui| {
        Ui.label("Opacity");
        if Ui.add(egui::Slider::new(&mut Opacity, 0.0..=1.0).custom_formatter(|Value, _| format!("{:.0}%", Value * 100.0))).changed() {
            App.record_coalesced(&format!("opacity{Index}"), "Layer opacity");
            if let Some(Target) = App.Project.image_mut(Id) {
                Target.Layers[Index].Opacity = Opacity;
            }
            App.touch();
        }
        Ui.end_row();
        Ui.label("Blend");
        if blend_combo(Ui, "LayerBlend", &mut Blend) {
            App.record("Blend mode");
            if let Some(Target) = App.Project.image_mut(Id) {
                Target.Layers[Index].Blend = Blend;
            }
            App.touch();
        }
        Ui.end_row();
    });
    egui::CollapsingHeader::new("Transform").default_open(!Transform.is_identity()).show(Ui, |Ui| {
        if transform_editor(Ui, &mut Transform, "LayerTransform") {
            App.record_coalesced(&format!("transform{Index}"), "Layer transform");
            if let Some(Target) = App.Project.image_mut(Id) {
                Target.Layers[Index].Transform = Transform;
            }
            App.touch();
        }
        Ui.horizontal(|Ui| {
            if Ui.button("Apply (bake)").clicked() {
                App.bake_layer_transform();
            }
            if Ui.button("Reset").clicked() {
                App.with_active_layer("Reset transform", |Target| {
                    if let Some(Entry) = Target.active_layer_mut() {
                        Entry.Transform = LayerTransform::default();
                    }
                });
            }
        });
    });
    Ui.horizontal(|Ui| {
        if Ui.button("Export layer…").clicked() {
            let mut State = dialogs::ExportState::new(App);
            State.What = dialogs::ExportWhat::ActiveLayer;
            App.Dialog = Some(Dialog::Export(State));
        }
        if Ui.button("Use as brush tip").on_hover_text("Turns this layer into an image tip for the Brush tool").clicked()
            && let Some(Entry) = App.Project.image(Id).and_then(|Found| Found.active_layer()) {
                let Bounds = Entry.Pixels.opaque_bounds().unwrap_or(Entry.Pixels.bounds());
                let Tip = TipImage::from_raster(&Entry.Pixels.crop(Bounds), false);
                App.Brushes.BrushTool.Tip = TipShape::Image;
                App.Brushes.BrushTool.TipImage = Some(Tip);
                App.Brushes.BrushTool.Size = Bounds.W.max(Bounds.H) as f32;
                App.set_tool(Tool::Brush);
                App.set_status("Layer captured as brush tip");
            }
    });
}

fn scene_panel(App: &mut DespiteApp, Ui: &mut Ui) {
    let Some(Id) = App.active_scene_id() else { return };
    let Some(Found) = App.Project.scene(Id).cloned() else { return };
    let mut Edited = Found.clone();
    let Document = DocumentRef::Scene(Id);
    Ui.label(RichText::new(&Found.Name).strong());
    egui::Grid::new("SceneProps").num_columns(2).show(Ui, |Ui| {
        Ui.label("Size");
        Ui.horizontal(|Ui| {
            Ui.add(egui::DragValue::new(&mut Edited.Width).range(1..=16384));
            Ui.label("×");
            Ui.add(egui::DragValue::new(&mut Edited.Height).range(1..=16384));
        });
        Ui.end_row();
        Ui.label("Background");
        Ui.horizontal(|Ui| {
            let mut HasBackground = Edited.Background.is_some();
            if Ui.checkbox(&mut HasBackground, "").changed() {
                Edited.Background = if HasBackground { Some(Color::White) } else { None };
            }
            if let Some(Fill) = &mut Edited.Background {
                color_button(Ui, Fill);
            }
        });
        Ui.end_row();
        Ui.label("FPS");
        Ui.add(egui::DragValue::new(&mut Edited.Fps).range(0.1..=120.0).speed(0.2));
        Ui.end_row();
        Ui.label("Frames");
        Ui.horizontal(|Ui| {
            Ui.add(egui::DragValue::new(&mut Edited.FrameCount).range(1..=10_000));
            if Ui.small_button("Auto").on_hover_text("Fit to the longest animated item").clicked() {
                Edited.FrameCount = Found.auto_frame_count(&App.Project);
            }
        });
        Ui.end_row();
        Ui.label("Pixel art");
        Ui.checkbox(&mut Edited.PixelArt, "Nearest sampling");
        Ui.end_row();
    });
    let Frame = App.Views.entry(Document).or_default().Frame;
    Ui.horizontal(|Ui| {
        if crate::icons::button(Ui, if App.Playing { "Pause" } else { "Play" }, false, "Play / pause (Space)").clicked() {
            App.Playing = !App.Playing;
            App.PlayClock = App.Now;
        }
        if crate::icons::button(Ui, "FirstFrame", false, "First frame").clicked() {
            App.Views.entry(Document).or_default().Frame = 0;
        }
        let mut Current = Frame;
        if Ui.add(egui::Slider::new(&mut Current, 0..=Edited.FrameCount.saturating_sub(1)).text("frame")).changed() {
            App.Views.entry(Document).or_default().Frame = Current;
        }
    });
    Ui.separator();
    Ui.horizontal(|Ui| {
        Ui.label(RichText::new("Items").strong());
        egui::ComboBox::from_id_salt("AddSceneItem").selected_text("+ Add image").show_ui(Ui, |Ui| {
            for Source in &App.Project.Images {
                if Ui.selectable_label(false, &Source.Name).clicked() {
                    let ItemId = App.Project.NextId;
                    App.Project.NextId += 1;
                    let Multi = Source.Layers.iter().filter(|Entry| Entry.Visible).count() > 1;
                    Edited.Items.push(SceneItem {
                        Id: ItemId,
                        Name: Source.Name.clone(),
                        Source: Some(Source.Id),
                        Transform: LayerTransform {
                            OffsetX: ((Edited.Width as f32 - Source.Width as f32) * 0.5).round(),
                            OffsetY: ((Edited.Height as f32 - Source.Height as f32) * 0.5).round(),
                            ..Default::default()
                        },
                        Playback: Playback::Composite,
                        ..Default::default()
                    });
                    Edited.ActiveItem = Some(Edited.Items.len() - 1);
                    let _ = Multi;
                }
            }
        });
    });
    let mut Remove = None;
    let mut MoveUp = None;
    for Index in (0..Edited.Items.len()).rev() {
        let Item = &mut Edited.Items[Index];
        Ui.horizontal(|Ui| {
            crate::icons::toggle(Ui, &mut Item.Visible, "Visible", "Hidden", "Show / hide");
            crate::icons::toggle(Ui, &mut Item.Locked, "Locked", "Unlocked", "Lock item");
            if Ui.selectable_label(Edited.ActiveItem == Some(Index), &Item.Name).clicked() {
                Edited.ActiveItem = Some(Index);
            }
            if crate::icons::button(Ui, "ArrowUp", false, "Move up").clicked() {
                MoveUp = Some(Index);
            }
            if crate::icons::button(Ui, "Delete", false, "Remove item").clicked() {
                Remove = Some(Index);
            }
        });
    }
    if let Some(Index) = MoveUp
        && Index + 1 < Edited.Items.len() {
            Edited.Items.swap(Index, Index + 1);
            Edited.ActiveItem = Some(Index + 1);
        }
    if let Some(Index) = Remove {
        Edited.Items.remove(Index);
        Edited.ActiveItem = None;
    }
    if let Some(Index) = Edited.ActiveItem.filter(|Index| *Index < Edited.Items.len()) {
        Ui.separator();
        let Images: Vec<(despite_core::ImageId, String)> = App.Project.Images.iter().map(|Found| (Found.Id, Found.Name.clone())).collect();
        let Item = &mut Edited.Items[Index];
        Ui.label(RichText::new("Item").strong());
        egui::Grid::new("SceneItemProps").num_columns(2).show(Ui, |Ui| {
            Ui.label("Name");
            Ui.text_edit_singleline(&mut Item.Name);
            Ui.end_row();
            Ui.label("Image");
            let Current = Item.Source.and_then(|SourceId| Images.iter().find(|(Candidate, _)| *Candidate == SourceId)).map(|(_, Name)| Name.clone()).unwrap_or_else(|| "(none)".into());
            egui::ComboBox::from_id_salt("ItemSource").selected_text(Current).show_ui(Ui, |Ui| {
                for (SourceId, Name) in &Images {
                    Ui.selectable_value(&mut Item.Source, Some(*SourceId), Name);
                }
            });
            Ui.end_row();
            Ui.label("Opacity");
            Ui.add(egui::Slider::new(&mut Item.Opacity, 0.0..=1.0));
            Ui.end_row();
            Ui.label("Blend");
            blend_combo(Ui, "ItemBlend", &mut Item.Blend);
            Ui.end_row();
            Ui.label("Playback");
            egui::ComboBox::from_id_salt("ItemPlayback")
                .selected_text(match Item.Playback {
                    Playback::Composite => "Still image",
                    Playback::LayersAsFrames => "Layers as frames",
                })
                .show_ui(Ui, |Ui| {
                    Ui.selectable_value(&mut Item.Playback, Playback::Composite, "Still image");
                    Ui.selectable_value(&mut Item.Playback, Playback::LayersAsFrames, "Layers as frames");
                });
            Ui.end_row();
            if Item.Playback == Playback::LayersAsFrames {
                Ui.label("Hold frames");
                Ui.add(egui::DragValue::new(&mut Item.HoldFrames).range(1..=240));
                Ui.end_row();
                Ui.label("Frame offset");
                Ui.add(egui::DragValue::new(&mut Item.FrameOffset).range(0..=10_000));
                Ui.end_row();
            }
            Ui.label("Visible from");
            Ui.horizontal(|Ui| {
                Ui.add(egui::DragValue::new(&mut Item.StartFrame).range(0..=100_000));
                let mut HasEnd = Item.EndFrame.is_some();
                if Ui.checkbox(&mut HasEnd, "until").changed() {
                    Item.EndFrame = if HasEnd { Some(Item.StartFrame) } else { None };
                }
                if let Some(End) = &mut Item.EndFrame {
                    Ui.add(egui::DragValue::new(End).range(0..=100_000));
                }
            });
            Ui.end_row();
        });
        transform_editor(Ui, &mut Item.Transform, "ItemTransform");
        if let Some(SourceId) = Item.Source
            && Ui.button("Edit source image").clicked() {
                App.activate(DocumentRef::Image(SourceId));
            }
    }
    if Edited != Found {
        let Structural = Edited.Items.len() != Found.Items.len() || Edited.Width != Found.Width || Edited.Height != Found.Height;
        if Structural {
            App.record("Edit scene");
        } else {
            App.record_coalesced(&format!("scene{}", Id.0), "Edit scene");
        }
        if let Some(Target) = App.Project.scene_mut(Id) {
            *Target = Edited;
        }
        App.touch();
    }
}

fn slider(Ui: &mut Ui, Label: &str, Value: &mut f32, Range: std::ops::RangeInclusive<f32>) -> bool {
    Ui.label(Label);
    let Changed = Ui.add(egui::Slider::new(Value, Range)).changed();
    Ui.end_row();
    Changed
}

fn preset_menu(Ui: &mut Ui, Title: &str, Presets: &[Brush], Target: &mut Brush) -> bool {
    let mut Changed = false;
    if Presets.is_empty() {
        return false;
    }
    Ui.menu_button(Title, |Ui| {
        for Preset in Presets {
            if Ui.button(&Preset.Name).clicked() {
                *Target = Preset.clone();
                Changed = true;
                Ui.close();
            }
        }
    });
    Changed
}

fn brush_panel(App: &mut DespiteApp, Ui: &mut Ui) {
    let mut Editing = if App.Tool.is_painting() { App.Tool } else { Tool::Brush };
    Ui.horizontal(|Ui| {
        for Candidate in [Tool::Brush, Tool::Pencil, Tool::Eraser] {
            if Ui.selectable_value(&mut Editing, Candidate, Candidate.name()).clicked() {
                App.set_tool(Candidate);
            }
        }
    });
    let (Builtin, Advanced, User, Plugin) = (App.Brushes.Builtin.clone(), App.Brushes.Advanced.clone(), App.Brushes.User.clone(), App.Brushes.Plugin.clone());
    let Installed: Vec<(String, Vec<Brush>)> = App.Brushes.Installed.iter().map(|Pack| (Pack.Name.clone(), Pack.Brushes.clone())).collect();
    let Pixel: Vec<Brush> = Builtin.iter().filter(|Entry| !Entry.Antialias).cloned().collect();
    let Painting: Vec<Brush> = Builtin.iter().filter(|Entry| Entry.Antialias && matches!(Entry.Mode, BrushMode::Paint | BrushMode::Erase)).cloned().collect();
    let Effects: Vec<Brush> = Builtin.iter().filter(|Entry| Entry.Antialias && !matches!(Entry.Mode, BrushMode::Paint | BrushMode::Erase)).cloned().collect();
    let mut SaveRequested = false;
    {
        let Target = App.Brushes.for_tool_mut(Editing);
        Ui.horizontal_wrapped(|Ui| {
            preset_menu(Ui, "Pixel", &Pixel, Target);
            preset_menu(Ui, "Paint", &Painting, Target);
            preset_menu(Ui, "Effects", &Effects, Target);
            preset_menu(Ui, "Fancy", &Advanced, Target);
            preset_menu(Ui, "Plugins", &Plugin, Target);
            if !Installed.is_empty() {
                Ui.menu_button("Installed", |Ui| {
                    for (PackName, Brushes) in &Installed {
                        if preset_menu(Ui, PackName, Brushes, Target) {
                            Ui.close();
                        }
                    }
                });
            }
            preset_menu(Ui, "Mine", &User, Target);
            if Ui.button("Save preset").clicked() {
                SaveRequested = true;
            }
        });
    }
    if SaveRequested {
        let Current = App.Brushes.for_tool(Editing).clone();
        App.Dialog = Some(Dialog::Rename(dialogs::RenameState { Target: dialogs::RenameTarget::BrushPreset(Box::new(Current.clone())), Text: Current.Name }));
    }
    Ui.horizontal_wrapped(|Ui| {
        if crate::icons::text_button(Ui, "Export", "Share brush…").on_hover_text("Package this brush into a .despitebrush file to send to another device").clicked() {
            let Current = App.Brushes.for_tool(Editing).clone();
            App.Dialog = Some(Dialog::ShareBrush(dialogs::ShareBrushState::new(&Current)));
        }
        if crate::icons::text_button(Ui, "Import", "Install brushes…").clicked() {
            App.Dialog = Some(Dialog::InstallBrush);
        }
        if crate::icons::text_button(Ui, "Settings", "Manage…").on_hover_text("Uninstall brush packs and delete saved presets").clicked() {
            App.Dialog = Some(Dialog::ManageBrushes);
        }
    });
    let ImagesForTips: Vec<(despite_core::ImageId, String)> = App.Project.Images.iter().map(|Found| (Found.Id, Found.Name.clone())).collect();
    let mut AnimatedFrom: Option<despite_core::ImageId> = None;
    let Target = App.Brushes.for_tool_mut(Editing);
    Ui.separator();
    Ui.label(RichText::new(&Target.Name).strong());
    Ui.horizontal_wrapped(|Ui| {
        Ui.label("Size:");
        for Size in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 12.0, 16.0, 24.0, 32.0, 64.0] {
            if Ui.selectable_label((Target.Size - Size).abs() < 0.01, format!("{Size}")).clicked() {
                Target.Size = Size;
            }
        }
    });
    egui::Grid::new("BrushBasics").num_columns(2).show(Ui, |Ui| {
        Ui.label("Mode");
        egui::ComboBox::from_id_salt("BrushMode").selected_text(Target.Mode.name()).show_ui(Ui, |Ui| {
            for Mode in BrushMode::All {
                Ui.selectable_value(&mut Target.Mode, *Mode, Mode.name());
            }
        });
        Ui.end_row();
        Ui.label("Tip");
        egui::ComboBox::from_id_salt("BrushTip").selected_text(Target.Tip.name()).show_ui(Ui, |Ui| {
            for Shape in TipShape::All {
                Ui.selectable_value(&mut Target.Tip, *Shape, Shape.name());
            }
        });
        Ui.end_row();
        slider(Ui, "Size", &mut Target.Size, 1.0..=1000.0);
        slider(Ui, "Opacity", &mut Target.Opacity, 0.0..=1.0);
        slider(Ui, "Flow", &mut Target.Flow, 0.01..=1.0);
        slider(Ui, "Hardness", &mut Target.Hardness, 0.0..=1.0);
        slider(Ui, "Spacing", &mut Target.Spacing, 0.01..=3.0);
        slider(Ui, "Angle", &mut Target.Angle, -180.0..=180.0);
        slider(Ui, "Roundness", &mut Target.Roundness, 0.05..=1.0);
        slider(Ui, "Stabilizer", &mut Target.Smoothing, 0.0..=0.95);
        if Target.Mode.uses_strength() {
            slider(Ui, "Strength", &mut Target.Strength, 0.0..=1.0);
        }
        if Target.Mode == BrushMode::ColorReplace {
            slider(Ui, "Replace tolerance", &mut Target.ReplaceTolerance, 0.0..=1.0);
        }
        Ui.label("Blend");
        blend_combo(Ui, "BrushBlend", &mut Target.Blend);
        Ui.end_row();
        Ui.label("Dither");
        egui::ComboBox::from_id_salt("BrushDither").selected_text(Target.Dither.name()).show_ui(Ui, |Ui| {
            for Pattern in DitherPattern::All {
                Ui.selectable_value(&mut Target.Dither, *Pattern, Pattern.name());
            }
        });
        Ui.end_row();
    });
    Ui.horizontal_wrapped(|Ui| {
        Ui.checkbox(&mut Target.Antialias, "Anti-alias").on_hover_text("Turn off for crisp pixel art");
        Ui.checkbox(&mut Target.PixelPerfect, "Pixel perfect").on_hover_text("Removes doubled corners on 1px lines");
        Ui.checkbox(&mut Target.FollowDirection, "Follow stroke");
    });
    egui::CollapsingHeader::new("Pressure").show(Ui, |Ui| {
        Ui.checkbox(&mut Target.PressureSize, "Pressure → size");
        Ui.checkbox(&mut Target.PressureOpacity, "Pressure → opacity");
        Ui.checkbox(&mut Target.PressureFlow, "Pressure → flow");
        Ui.checkbox(&mut Target.PressureHardness, "Pressure → hardness");
        Ui.checkbox(&mut Target.TiltAngle, "Pen tilt → angle");
        Ui.add(egui::Slider::new(&mut Target.TiltFlatten, 0.0..=1.0).text("pen tilt → flat tip"));
        Ui.label(RichText::new("Works with drawing tablets, Apple Pencil and Android styluses. Effect brushes use opacity/flow pressure as strength.").weak().small());
        egui::Grid::new("BrushPressure").num_columns(2).show(Ui, |Ui| {
            slider(Ui, "Minimum size", &mut Target.MinSizeRatio, 0.0..=1.0);
            slider(Ui, "Curve", &mut Target.PressureCurve, 0.2..=5.0);
            slider(Ui, "Taper in (px)", &mut Target.TaperIn, 0.0..=400.0);
        });
    });
    egui::CollapsingHeader::new("Jitter & scatter").show(Ui, |Ui| {
        egui::Grid::new("BrushJitter").num_columns(2).show(Ui, |Ui| {
            slider(Ui, "Size jitter", &mut Target.SizeJitter, 0.0..=1.0);
            slider(Ui, "Angle jitter", &mut Target.AngleJitter, 0.0..=1.0);
            slider(Ui, "Roundness jitter", &mut Target.RoundnessJitter, 0.0..=1.0);
            slider(Ui, "Opacity jitter", &mut Target.OpacityJitter, 0.0..=1.0);
            slider(Ui, "Scatter", &mut Target.Scatter, 0.0..=10.0);
            Ui.label("Dabs per step");
            Ui.add(egui::DragValue::new(&mut Target.DabCount).range(1..=64));
            Ui.end_row();
        });
    });
    egui::CollapsingHeader::new("Color dynamics").show(Ui, |Ui| {
        egui::Grid::new("BrushColor").num_columns(2).show(Ui, |Ui| {
            slider(Ui, "Hue jitter", &mut Target.HueJitter, 0.0..=1.0);
            slider(Ui, "Saturation jitter", &mut Target.SaturationJitter, 0.0..=1.0);
            slider(Ui, "Value jitter", &mut Target.ValueJitter, 0.0..=1.0);
        });
    });
    if let Some(Tip) = &Target.TipImage {
        Ui.label(format!("Image tip: {}×{}", Tip.Width, Tip.Height));
    }
    {
        egui::CollapsingHeader::new("Fancy effects (boil, animated tips, color cycle…)").show(Ui, |Ui| {
            let Extra = &mut Target.Advanced;
            egui::Grid::new("BrushAdvanced").num_columns(2).show(Ui, |Ui| {
                slider(Ui, "Color cycle (°/100px)", &mut Extra.ColorCycle, -720.0..=720.0);
                Ui.label("Gradient to");
                Ui.horizontal(|Ui| {
                    let mut HasEnd = Extra.GradientEnd.is_some();
                    if Ui.checkbox(&mut HasEnd, "").changed() {
                        Extra.GradientEnd = if HasEnd { Some(Color::White) } else { None };
                    }
                    if let Some(End) = &mut Extra.GradientEnd {
                        color_button(Ui, End);
                    }
                });
                Ui.end_row();
                slider(Ui, "Gradient length", &mut Extra.GradientLength, 10.0..=2000.0);
                slider(Ui, "Wiggle amplitude", &mut Extra.WiggleAmplitude, 0.0..=64.0);
                slider(Ui, "Wiggle wavelength", &mut Extra.WiggleWavelength, 2.0..=400.0);
                slider(Ui, "Fade out length", &mut Extra.FadeLength, 0.0..=4000.0);
                slider(Ui, "Rotation spin", &mut Extra.RotationSpin, -90.0..=90.0);
                Ui.label("Echo copies");
                Ui.add(egui::DragValue::new(&mut Extra.EchoCount).range(0..=16));
                Ui.end_row();
                slider(Ui, "Echo offset X", &mut Extra.EchoOffsetX, -64.0..=64.0);
                slider(Ui, "Echo offset Y", &mut Extra.EchoOffsetY, -64.0..=64.0);
                slider(Ui, "Echo fade", &mut Extra.EchoFade, 0.0..=1.0);
                slider(Ui, "Sparkle chance", &mut Extra.SparkleChance, 0.0..=1.0);
                slider(Ui, "Sparkle scale", &mut Extra.SparkleScale, 0.1..=8.0);
                Ui.label("Boil frames");
                Ui.add(egui::DragValue::new(&mut Extra.BoilFrames).range(0..=12)).on_hover_text("2 or more paints every stroke into that many frame layers that animate (line boil). 0 = off");
                Ui.end_row();
                slider(Ui, "Boil wobble (px)", &mut Extra.BoilJitter, 0.0..=8.0);
                slider(Ui, "Boil wavelength", &mut Extra.BoilWavelength, 2.0..=120.0);
                slider(Ui, "Pencil grain", &mut Extra.Grain, 0.0..=1.0);
                slider(Ui, "Grain scale", &mut Extra.GrainScale, 0.25..=8.0);
            });
            Ui.label(RichText::new("Boil brushes create frame layers that play on the canvas, in scenes and in GIF/video exports. Set the speed with the image's Frame ms.").weak().small());
            Ui.separator();
            Ui.label(format!("Animated tip frames: {}", Extra.AnimatedTips.len()));
            egui::ComboBox::from_id_salt("TipAnimation").selected_text(Extra.Animation.name()).show_ui(Ui, |Ui| {
                for Mode in TipAnimation::All {
                    Ui.selectable_value(&mut Extra.Animation, *Mode, Mode.name());
                }
            });
            Ui.add(egui::Slider::new(&mut Extra.AnimationStep, 0.1..=100.0).text("step"));
            Ui.horizontal_wrapped(|Ui| {
                egui::ComboBox::from_id_salt("TipFromImage").selected_text("Frames from image layers…").show_ui(Ui, |Ui| {
                    for (Id, Name) in &ImagesForTips {
                        if Ui.selectable_label(false, Name).clicked() {
                            AnimatedFrom = Some(*Id);
                        }
                    }
                });
                if Ui.button("Clear frames").clicked() {
                    Extra.AnimatedTips.clear();
                }
            });
        });
    }
    if let Some(Source) = AnimatedFrom
        && let Some(Found) = App.Project.image(Source) {
            let Frames: Vec<TipImage> = Found
                .Layers
                .iter()
                .filter(|Entry| Entry.Visible)
                .map(|Entry| TipImage::from_raster(&Entry.rasterize_in_image_space(Found.Width, Found.Height, false), false))
                .collect();
            let Count = Frames.len();
            let Target = App.Brushes.for_tool_mut(Editing);
            Target.Advanced.AnimatedTips = Frames;
            Target.Tip = TipShape::Image;
            App.set_status(format!("Loaded {Count} animated tip frames"));
        }
}

fn color_panel(App: &mut DespiteApp, Ui: &mut Ui) {
    let mut Shown = to_color32(App.Primary);
    if egui::color_picker::color_picker_color32(Ui, &mut Shown, egui::color_picker::Alpha::OnlyBlend) {
        App.Primary = from_color32(Shown);
    }
    Ui.horizontal(|Ui| {
        Ui.label("Secondary");
        color_button(Ui, &mut App.Secondary);
        if crate::icons::text_button(Ui, "SwapColors", "Swap").clicked() {
            App.swap_colors();
        }
    });
    let HexId = egui::Id::unique("PrimaryHex");
    let mut Hex: String = Ui.data(|Data| Data.get_temp(HexId)).unwrap_or_else(|| App.Primary.to_hex());
    Ui.horizontal(|Ui| {
        Ui.label("Hex");
        let Response = Ui.text_edit_singleline(&mut Hex);
        if Response.changed()
            && let Some(Parsed) = Color::from_hex(&Hex) {
                App.Primary = Parsed;
            }
        if !Response.has_focus() {
            Hex = App.Primary.to_hex();
        }
    });
    Ui.data_mut(|Data| Data.insert_temp(HexId, Hex));
    Ui.separator();
    Ui.horizontal(|Ui| {
        Ui.label(RichText::new(format!("Palette ({})", App.Project.Palette.len())).strong());
        if crate::icons::button(Ui, "Add", false, "Add primary color").clicked() {
            App.record("Edit palette");
            let Primary = App.Primary;
            App.Project.Palette.push(Primary);
        }
    });
    Ui.label(RichText::new("Click: primary · Right-click: secondary · Ctrl-click: remove").weak().small());
    let mut Remove = None;
    Ui.horizontal_wrapped(|Ui| {
        Ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
        for (Index, Swatch) in App.Project.Palette.iter().enumerate() {
            let (Rect, Response) = Ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
            Ui.painter().rect_filled(Rect, 2.0, to_color32(*Swatch));
            if *Swatch == App.Primary {
                Ui.painter().rect_stroke(Rect, 2.0, egui::Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Inside);
            }
            if Response.clicked() {
                if Ui.input(|Input| Input.modifiers.command) {
                    Remove = Some(Index);
                } else {
                    App.Primary = *Swatch;
                }
            }
            if Response.secondary_clicked() {
                App.Secondary = *Swatch;
            }
            Response.on_hover_text(Swatch.to_hex());
        }
    });
    if let Some(Index) = Remove {
        App.record("Edit palette");
        App.Project.Palette.remove(Index);
    }
    Ui.horizontal_wrapped(|Ui| {
        if Ui.button("From image").on_hover_text("Replace the palette with the colors used in this image (max 256)").clicked()
            && let Some(Found) = App.Project.active_image() {
                let Pixels = Found.composite();
                let mut Unique: Vec<[u8; 4]> = Vec::new();
                for Pixel in Pixels.pixels() {
                    if Pixel[3] > 0 && !Unique.contains(&[Pixel[0], Pixel[1], Pixel[2], 255]) {
                        Unique.push([Pixel[0], Pixel[1], Pixel[2], 255]);
                        if Unique.len() > 256 {
                            break;
                        }
                    }
                }
                if Unique.len() <= 256 {
                    App.record("Palette from image");
                    App.Project.Palette = Unique.into_iter().map(Color::from_rgba8).collect();
                } else {
                    App.set_status("This image has more than 256 colors");
                }
            }
        if Ui.button("Default").clicked() {
            App.record("Reset palette");
            App.Project.Palette = despite_core::project::default_palette();
        }
        if Ui.button("Clear").clicked() {
            App.record("Clear palette");
            App.Project.Palette.clear();
        }
        #[cfg(feature = "desktop")]
        {
            if Ui.button("Load…").clicked()
                && let Some(Path) = crate::platform::dialogs::open_file("Palette", &["gpl", "hex", "txt"])
                    && let Ok(Text) = std::fs::read_to_string(Path) {
                        let Colors = despite_plugin::parse_palette_text(&Text);
                        if !Colors.is_empty() {
                            App.record("Load palette");
                            App.Project.Palette = Colors;
                        }
                    }
            if Ui.button("Save…").clicked()
                && let Some(Path) = crate::platform::dialogs::save_file("GIMP palette", "gpl", &App.Project.Name) {
                    let _ = std::fs::write(Path, despite_plugin::palette_to_gpl(&App.Project.Name, &App.Project.Palette));
                }
            let Palettes: Vec<(String, Vec<Color>)> = App.Desktop.Plugins.palettes().map(|(_, Entry)| (Entry.Name.clone(), Entry.Colors.clone())).collect();
            if !Palettes.is_empty() {
                Ui.menu_button("Plugin palettes", |Ui| {
                    for (Name, Colors) in Palettes {
                        if Ui.button(Name).clicked() {
                            App.record("Load palette");
                            App.Project.Palette = Colors;
                            Ui.close();
                        }
                    }
                });
            }
        }
    });
}

fn guides_panel(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.label(RichText::new("Symmetry ruler").strong());
    egui::Grid::new("SymmetryGrid").num_columns(2).show(Ui, |Ui| {
        Ui.label("Mode");
        egui::ComboBox::from_id_salt("SymmetryMode").selected_text(App.Symmetry.Mode.name()).show_ui(Ui, |Ui| {
            for Mode in SymmetryMode::All {
                Ui.selectable_value(&mut App.Symmetry.Mode, *Mode, Mode.name());
            }
        });
        Ui.end_row();
        if matches!(App.Symmetry.Mode, SymmetryMode::Radial | SymmetryMode::Kaleidoscope) {
            Ui.label("Segments");
            Ui.add(egui::DragValue::new(&mut App.Symmetry.Segments).range(2..=64));
            Ui.end_row();
        }
        Ui.label("Center");
        Ui.horizontal(|Ui| {
            Ui.add(egui::DragValue::new(&mut App.Symmetry.CenterX).speed(0.5).prefix("x "));
            Ui.add(egui::DragValue::new(&mut App.Symmetry.CenterY).speed(0.5).prefix("y "));
        });
        Ui.end_row();
    });
    Ui.horizontal(|Ui| {
        if Ui.button("Center on canvas").clicked() {
            App.center_symmetry();
        }
        Ui.checkbox(&mut App.Symmetry.ShowGuide, "Show guide");
        if Ui.button("Place with mouse").clicked() {
            App.set_tool(Tool::Symmetry);
        }
    });
    Ui.separator();
    Ui.label(RichText::new("Tiling").strong());
    Ui.checkbox(&mut App.Wrap, "Wrap-around painting (seamless tiles)");
    Ui.checkbox(&mut App.Settings.TilePreview, "Tile preview around canvas");
    Ui.horizontal(|Ui| {
        Ui.checkbox(&mut App.Settings.TileGrid, "Tile grid");
        Ui.add(egui::DragValue::new(&mut App.Settings.TileGridWidth).range(1..=1024).prefix("w "));
        Ui.add(egui::DragValue::new(&mut App.Settings.TileGridHeight).range(1..=1024).prefix("h "));
    });
    Ui.checkbox(&mut App.Settings.PixelGrid, "Pixel grid when zoomed in");
    let Some(Document) = App.Project.Active else { return };
    if let DocumentRef::Image(_) = Document {
        Ui.separator();
        Ui.label(RichText::new("Onion skin (neighbouring images)").strong());
        let View = &mut App.Settings.View;
        Ui.label(RichText::new("Applies to every image").weak().small());
        Ui.checkbox(&mut View.OnionSkin.Enabled, "Enabled");
        Ui.horizontal(|Ui| {
            Ui.add(egui::DragValue::new(&mut View.OnionSkin.Before).range(0..=8).prefix("before "));
            Ui.add(egui::DragValue::new(&mut View.OnionSkin.After).range(0..=8).prefix("after "));
        });
        Ui.add(egui::Slider::new(&mut View.OnionSkin.Opacity, 0.05..=1.0).text("opacity"));
        Ui.checkbox(&mut View.OnionSkin.Tint, "Tint red / blue");
    }
    Ui.separator();
    Ui.label(RichText::new("Reference overlays (alignment)").strong());
    Ui.label(RichText::new("Show other images or scenes on top of / below this one while you work. They are never exported.").weak().small());
    let mut Overlays = match Document {
        DocumentRef::Image(Id) => App.Project.image(Id).map(|Found| Found.Overlays.clone()).unwrap_or_default(),
        DocumentRef::Scene(Id) => App.Project.scene(Id).map(|Found| Found.Overlays.clone()).unwrap_or_default(),
    };
    let Original = Overlays.clone();
    let mut Sources: Vec<(OverlaySource, String)> = App.Project.Images.iter().map(|Found| (OverlaySource::Image(Found.Id), format!("Image: {}", Found.Name))).collect();
    Sources.extend(App.Project.Scenes.iter().map(|Found| (OverlaySource::Scene(Found.Id), format!("Scene: {}", Found.Name))));
    Sources.retain(|(Source, _)| match (Source, Document) {
        (OverlaySource::Image(Id), DocumentRef::Image(Current)) => *Id != Current,
        (OverlaySource::Scene(Id), DocumentRef::Scene(Current)) => *Id != Current,
        _ => true,
    });
    let mut Remove = None;
    for (Index, Entry) in Overlays.iter_mut().enumerate() {
        egui::Frame::group(Ui.style()).show(Ui, |Ui| {
            let Current = Entry.Source.and_then(|Source| Sources.iter().find(|(Candidate, _)| *Candidate == Source)).map(|(_, Name)| Name.clone()).unwrap_or_else(|| "(choose)".into());
            Ui.horizontal(|Ui| {
                Ui.checkbox(&mut Entry.Visible, "");
                egui::ComboBox::from_id_salt(("OverlaySource", Index)).selected_text(Current).show_ui(Ui, |Ui| {
                    for (Source, Name) in &Sources {
                        Ui.selectable_value(&mut Entry.Source, Some(*Source), Name);
                    }
                });
                if crate::icons::button(Ui, "Delete", false, "Remove overlay").clicked() {
                    Remove = Some(Index);
                }
            });
            Ui.add(egui::Slider::new(&mut Entry.Opacity, 0.0..=1.0).text("opacity"));
            Ui.horizontal(|Ui| {
                Ui.add(egui::DragValue::new(&mut Entry.OffsetX).prefix("x "));
                Ui.add(egui::DragValue::new(&mut Entry.OffsetY).prefix("y "));
                Ui.checkbox(&mut Entry.Above, "Above");
            });
            if matches!(Entry.Source, Some(OverlaySource::Scene(_))) {
                Ui.add(egui::DragValue::new(&mut Entry.Frame).prefix("frame "));
            }
        });
    }
    if let Some(Index) = Remove {
        Overlays.remove(Index);
    }
    if Ui.button("+ Add overlay").clicked() {
        Overlays.push(Overlay { Source: Sources.first().map(|(Source, _)| *Source), ..Default::default() });
    }
    if Overlays != Original {
        App.record_coalesced("overlays", "Edit overlays");
        match Document {
            DocumentRef::Image(Id) => {
                if let Some(Found) = App.Project.image_mut(Id) {
                    Found.Overlays = Overlays;
                }
            }
            DocumentRef::Scene(Id) => {
                if let Some(Found) = App.Project.scene_mut(Id) {
                    Found.Overlays = Overlays;
                }
            }
        }
        App.Modified = true;
    }
}

fn history_panel(App: &mut DespiteApp, Ui: &mut Ui) {
    let Extra = App.History.extra_memory_bytes(&App.Project);
    Ui.label(format!("{} undo · {} redo · {} extra memory", App.History.Undo.len(), App.History.Redo.len(), human_bytes(Extra as u64)));
    Ui.horizontal(|Ui| {
        if crate::icons::enabled_button(Ui, App.History.can_undo(), "Undo", "Undo").clicked() {
            App.undo();
        }
        if crate::icons::enabled_button(Ui, App.History.can_redo(), "Redo", "Redo").clicked() {
            App.redo();
        }
        if Ui.button("Clear").on_hover_text("Forget all undo steps to free memory").clicked() {
            App.History.clear();
        }
    });
    Ui.label(RichText::new(if App.Settings.SaveHistoryInProject { "History is saved inside project files (Settings)." } else { "History is not saved in project files (enable in Settings)." }).weak().small());
    Ui.separator();
    let UndoLabels: Vec<String> = App.History.Undo.iter().map(|Entry| Entry.Label.clone()).collect();
    let RedoLabels: Vec<String> = App.History.Redo.iter().rev().map(|Entry| Entry.Label.clone()).collect();
    let mut UndoTo = None;
    let mut RedoTo = None;
    for (Index, Label) in UndoLabels.iter().enumerate() {
        if Ui.selectable_label(false, format!("{}. {Label}", Index + 1)).clicked() {
            UndoTo = Some(UndoLabels.len() - Index);
        }
    }
    Ui.label(RichText::new("● Current state").strong());
    for (Index, Label) in RedoLabels.iter().enumerate() {
        if Ui.selectable_label(false, RichText::new(format!("redo: {Label}")).weak()).clicked() {
            RedoTo = Some(Index + 1);
        }
    }
    if let Some(Steps) = UndoTo {
        for _ in 0..Steps.saturating_sub(1) {
            App.undo();
        }
    }
    if let Some(Steps) = RedoTo {
        for _ in 0..Steps {
            App.redo();
        }
    }
}

pub fn tool_options_bar(App: &mut DespiteApp, Ui: &mut Ui) {
    let Layout = egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(!App.Compact);
    Ui.with_layout(Layout, |Ui| {
        crate::icons::show(Ui, App.Tool.icon_name());
        Ui.label(RichText::new(App.Tool.name()).strong());
        Ui.separator();
        let Options = &mut App.ToolOptions;
        match App.Tool {
            Tool::Brush | Tool::Pencil | Tool::Eraser => {
                let Active = App.Tool;
                let Target = App.Brushes.for_tool_mut(Active);
                Ui.label("Size");
                Ui.add(egui::DragValue::new(&mut Target.Size).range(1.0..=1000.0).speed(0.25));
                Ui.label("Opacity");
                Ui.add(egui::Slider::new(&mut Target.Opacity, 0.0..=1.0).show_value(false));
                if Target.Mode.uses_strength() {
                    Ui.label("Strength");
                    Ui.add(egui::Slider::new(&mut Target.Strength, 0.0..=1.0).show_value(false));
                }
                Ui.checkbox(&mut Target.Antialias, "AA");
                Ui.checkbox(&mut Target.PixelPerfect, "Pixel perfect");
                egui::ComboBox::from_id_salt("QuickTip").selected_text(Target.Tip.name()).width(80.0).show_ui(Ui, |Ui| {
                    for Shape in TipShape::All {
                        Ui.selectable_value(&mut Target.Tip, *Shape, Shape.name());
                    }
                });
                Ui.separator();
                symmetry_quick(App, Ui);
            }
            Tool::Fill => {
                Ui.label("Tolerance");
                Ui.add(egui::Slider::new(&mut Options.FillTolerance, 0.0..=1.0));
                Ui.checkbox(&mut Options.FillContiguous, "Contiguous");
                Ui.checkbox(&mut Options.FillSampleMerged, "Sample all layers");
                Ui.label("Grow");
                Ui.add(egui::DragValue::new(&mut Options.FillExpand).range(0..=16).suffix(" px")).on_hover_text("Expands the fill under anti-aliased line art");
                Ui.label("Opacity");
                Ui.add(egui::Slider::new(&mut Options.FillOpacity, 0.0..=1.0).show_value(false));
                blend_combo(Ui, "FillBlend", &mut Options.FillBlend);
            }
            Tool::Gradient => {
                egui::ComboBox::from_id_salt("GradientKind").selected_text(Options.GradientKind.name()).show_ui(Ui, |Ui| {
                    for Kind in GradientKind::All {
                        Ui.selectable_value(&mut Options.GradientKind, *Kind, Kind.name());
                    }
                });
                Ui.label("Steps");
                Ui.add(egui::DragValue::new(&mut Options.GradientSteps).range(0..=64)).on_hover_text("0 = smooth, 2+ = banded (pixel art)");
                Ui.label("Dither");
                egui::ComboBox::from_id_salt("GradientDither").selected_text(Options.GradientDither.name()).show_ui(Ui, |Ui| {
                    for Pattern in DitherPattern::All {
                        Ui.selectable_value(&mut Options.GradientDither, *Pattern, Pattern.name());
                    }
                });
                Ui.label("Opacity");
                Ui.add(egui::Slider::new(&mut Options.GradientOpacity, 0.0..=1.0).show_value(false));
                blend_combo(Ui, "GradientBlend", &mut Options.GradientBlend);
            }
            Tool::Line | Tool::Rectangle | Tool::Ellipse => {
                if App.Tool != Tool::Line {
                    Ui.checkbox(&mut Options.ShapeFilled, "Filled");
                }
                Ui.checkbox(&mut Options.ShapePixelExact, "Pixel exact").on_hover_text("Uses Bresenham lines and pixel ellipses when the brush has anti-aliasing off");
                Ui.label("Uses the Brush tool's settings · Shift to constrain");
                Ui.separator();
                let Target = &mut App.Brushes.BrushTool;
                Ui.label("Width");
                Ui.add(egui::DragValue::new(&mut Target.Size).range(1.0..=200.0));
                Ui.checkbox(&mut Target.Antialias, "AA");
            }
            Tool::RectSelect | Tool::EllipseSelect | Tool::Lasso | Tool::MagicWand => {
                for Mode in SelectionMode::All {
                    Ui.selectable_value(&mut Options.Selection, *Mode, Mode.name());
                }
                Ui.checkbox(&mut Options.SelectionAntialias, "Anti-alias");
                if App.Tool == Tool::MagicWand {
                    Ui.label("Tolerance");
                    Ui.add(egui::Slider::new(&mut Options.WandTolerance, 0.0..=1.0));
                    Ui.checkbox(&mut Options.WandContiguous, "Contiguous");
                    Ui.checkbox(&mut Options.WandSampleMerged, "Sample all layers");
                }
                Ui.separator();
                if Ui.button("All").clicked() {
                    App.select_all();
                }
                if Ui.button("None").clicked() {
                    App.deselect();
                }
                if Ui.button("Invert").clicked() {
                    App.invert_selection();
                }
            }
            Tool::Picker => {
                Ui.checkbox(&mut Options.PickerMerged, "Sample all layers");
            }
            Tool::Text => {
                crate::textui::style_controls(Ui, &mut Options.Text, "TextBar", false, "");
                Ui.label(RichText::new("Click the canvas to type").weak());
            }
            Tool::Move => {
                Ui.checkbox(&mut Options.MoveSelectsItems, "Click selects scene items");
                Ui.label("Arrow keys nudge · Shift = 10px");
                if App.active_image_id().is_some() && Ui.button("Apply transform").clicked() {
                    App.bake_layer_transform();
                }
            }
            Tool::Symmetry => symmetry_quick(App, Ui),
            Tool::Pan => {
                Ui.label("Drag to pan · scroll / pinch to zoom");
            }
        }
    });
}

fn symmetry_quick(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.label("Symmetry");
    egui::ComboBox::from_id_salt("QuickSymmetry").selected_text(App.Symmetry.Mode.name()).width(110.0).show_ui(Ui, |Ui| {
        for Mode in SymmetryMode::All {
            Ui.selectable_value(&mut App.Symmetry.Mode, *Mode, Mode.name());
        }
    });
    if matches!(App.Symmetry.Mode, SymmetryMode::Radial | SymmetryMode::Kaleidoscope) {
        Ui.add(egui::DragValue::new(&mut App.Symmetry.Segments).range(2..=64));
    }
    Ui.checkbox(&mut App.Wrap, "Wrap");
}

pub fn status_bar(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.horizontal(|Ui| {
        if let Some(Document) = App.Project.Active {
            if let Some((Width, Height)) = App.Project.document_size(Document) {
                Ui.label(format!("{Width}×{Height}"));
            }
            let Zoom = App.Views.get(&Document).map(|View| View.Zoom).unwrap_or(1.0);
            Ui.label(format!("{:.0}%", Zoom * 100.0));
            if let Some((X, Y)) = App.PointerImagePosition {
                Ui.label(format!("x {} y {}", X.floor() as i32, Y.floor() as i32));
                let Under = App.Views.get(&Document).and_then(|View| View.Cache.as_ref()).and_then(|Cache| Cache.get_checked(X.floor() as i32, Y.floor() as i32));
                if let Some(Pixel) = Under {
                    let (Rect, _) = Ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    Ui.painter().rect_filled(Rect, 2.0, Color32::from_rgba_unmultiplied(Pixel[0], Pixel[1], Pixel[2], Pixel[3]));
                    Ui.label(Color::from_rgba8(Pixel).to_hex());
                }
            }
            if let Some(Bounds) = App.Views.get(&Document).and_then(|View| View.Selection.as_ref()).and_then(|Found| Found.bounds()) {
                Ui.label(format!("selection {}×{}", Bounds.W, Bounds.H));
            }
        }
        Ui.separator();
        if let Some((Message, Time)) = &App.Status
            && App.Now - Time < 6.0 {
                Ui.label(Message);
            }
        Ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |Ui| {
            crate::icons::toggle_text(Ui, &mut App.Settings.TileGrid, "TileGrid", "Tiles").on_hover_text("Toggle tile grid");
            if App.Project.active_image().is_some_and(|Found| Found.boil_frame_count() > 1) {
                crate::icons::toggle_text(Ui, &mut App.Settings.View.BoilPreview, "Play", "Boil").on_hover_text("Play the line boil animation on the canvas");
            }
            crate::icons::toggle_text(Ui, &mut App.Settings.PixelGrid, "Grid", "Grid").on_hover_text("Toggle pixel grid (shows when zoomed in)");
            Ui.separator();
            if App.Modified {
                Ui.label(RichText::new("• unsaved").color(Color32::from_rgb(230, 180, 80)));
            }
            Ui.label(format!("{} undo", App.History.Undo.len()));
        });
    });
}

#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
pub fn params_editor(Ui: &mut Ui, Params: &mut Vec<ParamDecl>, Owner: &Project, AllowStructure: bool, Salt: &str) -> bool {
    let mut Changed = false;
    let mut Remove = None;
    for (Index, Decl) in Params.iter_mut().enumerate() {
        Ui.horizontal(|Ui| {
            Ui.label(RichText::new(Decl.display_label()).strong()).on_hover_text(format!("Name in code: {}", Decl.Name));
            match (&Decl.Kind, &mut Decl.Value) {
                (ParamKind::Float { Min, Max }, ParamValue::Float(Value)) => {
                    Changed |= Ui.add(egui::Slider::new(Value, *Min..=*Max)).changed();
                }
                (ParamKind::Int { Min, Max }, ParamValue::Int(Value)) => {
                    Changed |= Ui.add(egui::Slider::new(Value, *Min..=*Max)).changed();
                }
                (ParamKind::Bool, ParamValue::Bool(Value)) => Changed |= Ui.checkbox(Value, "").changed(),
                (ParamKind::Color, ParamValue::Color(Value)) => Changed |= color_button(Ui, Value),
                (ParamKind::Vec2, ParamValue::Vec2(Value)) => {
                    Changed |= Ui.add(egui::DragValue::new(&mut Value[0]).speed(0.01)).changed();
                    Changed |= Ui.add(egui::DragValue::new(&mut Value[1]).speed(0.01)).changed();
                }
                (ParamKind::Text, ParamValue::Text(Value)) => Changed |= Ui.text_edit_singleline(Value).changed(),
                (ParamKind::Image, ParamValue::Image(Value)) => {
                    let Current = Value.and_then(|Id| Owner.image(Id)).map(|Found| Found.Name.clone()).unwrap_or_else(|| "(none)".into());
                    egui::ComboBox::from_id_salt((Salt, "ParamImage", Index)).selected_text(Current).show_ui(Ui, |Ui| {
                        Changed |= Ui.selectable_value(Value, None, "(none)").changed();
                        for Found in &Owner.Images {
                            Changed |= Ui.selectable_value(Value, Some(Found.Id), &Found.Name).changed();
                        }
                    });
                }
                _ => {
                    Decl.sanitize();
                    Changed = true;
                }
            }
            if AllowStructure && crate::icons::button(Ui, "Delete", false, "Remove parameter").clicked() {
                Remove = Some(Index);
            }
        });
        if AllowStructure {
            if let ParamKind::Float { Min, Max } = &mut Decl.Kind {
                Ui.horizontal(|Ui| {
                    Ui.label(RichText::new("range").weak());
                    Changed |= Ui.add(egui::DragValue::new(Min).speed(0.1)).changed();
                    Changed |= Ui.add(egui::DragValue::new(Max).speed(0.1)).changed();
                });
            }
            if let ParamKind::Int { Min, Max } = &mut Decl.Kind {
                Ui.horizontal(|Ui| {
                    Ui.label(RichText::new("range").weak());
                    Changed |= Ui.add(egui::DragValue::new(Min)).changed();
                    Changed |= Ui.add(egui::DragValue::new(Max)).changed();
                });
            }
            Decl.sanitize();
        }
    }
    if let Some(Index) = Remove {
        Params.remove(Index);
        Changed = true;
    }
    if AllowStructure {
        let NameId = egui::Id::unique((Salt, "NewParamName"));
        let KindId = egui::Id::unique((Salt, "NewParamKind"));
        let mut Name: String = Ui.data(|Data| Data.get_temp(NameId)).unwrap_or_default();
        let mut KindName: String = Ui.data(|Data| Data.get_temp(KindId)).unwrap_or_else(|| "Float".to_string());
        Ui.horizontal(|Ui| {
            Ui.add(egui::TextEdit::singleline(&mut Name).hint_text("NewParam").desired_width(110.0));
            egui::ComboBox::from_id_salt((Salt, "NewParamKindCombo")).selected_text(KindName.clone()).width(70.0).show_ui(Ui, |Ui| {
                for Candidate in ParamKind::Names {
                    Ui.selectable_value(&mut KindName, Candidate.to_string(), *Candidate);
                }
            });
            let Valid = despite_core::params::is_valid_identifier(&Name) && !Params.iter().any(|Existing| Existing.Name == Name);
            if Ui.add_enabled(Valid, egui::Button::new("+ Param")).clicked()
                && let Some(Kind) = ParamKind::from_name(&KindName) {
                    Params.push(ParamDecl::new(Name.clone(), Kind));
                    Name.clear();
                    Changed = true;
                }
        });
        Ui.data_mut(|Data| {
            Data.insert_temp(NameId, Name);
            Data.insert_temp(KindId, KindName);
        });
    }
    Changed
}
