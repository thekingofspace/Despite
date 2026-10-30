use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use egui::{Color32, RichText, Ui};

use crate::app::{DespiteApp, Screen, human_bytes};
use crate::dialogs::{Dialog, PendingAction};
use crate::platform;

#[derive(Clone, Debug)]
pub struct ProjectEntry {
    pub Path: PathBuf,
    pub Name: String,
    pub Modified: Option<SystemTime>,
    pub Size: u64,
    pub Autosave: bool,
}

pub enum Thumbnail {
    Ready(egui::TextureHandle),
    Missing,
}

#[derive(Default)]
pub struct HomeState {
    pub Entries: Vec<ProjectEntry>,
    pub LastScan: f64,
    pub Search: String,
    pub Thumbnails: HashMap<PathBuf, (Thumbnail, Option<SystemTime>)>,
    pub ConfirmDelete: Option<PathBuf>,
    pub NeedsScan: bool,
}

fn entry_for(Path: &Path, Autosave: bool) -> Option<ProjectEntry> {
    let Meta = std::fs::metadata(Path).ok()?;
    if !Meta.is_file() {
        return None;
    }
    Some(ProjectEntry {
        Path: Path.to_path_buf(),
        Name: Path.file_stem().and_then(|Stem| Stem.to_str()).unwrap_or("Project").to_string(),
        Modified: Meta.modified().ok(),
        Size: Meta.len(),
        Autosave,
    })
}

pub fn scan(App: &mut DespiteApp) {
    let mut Seen: Vec<PathBuf> = Vec::new();
    let mut Entries = Vec::new();
    let mut Add = |Path: &Path, Autosave: bool, Entries: &mut Vec<ProjectEntry>| {
        let Key = Path.canonicalize().unwrap_or_else(|_| Path.to_path_buf());
        if Seen.contains(&Key) {
            return;
        }
        if let Some(Found) = entry_for(Path, Autosave) {
            Seen.push(Key);
            Entries.push(Found);
        }
    };
    for Path in &App.Settings.RecentFiles {
        Add(Path, false, &mut Entries);
    }
    let Documents = platform::documents_dir();
    platform::ensure_dir(&Documents);
    for Path in platform::list_files(&Documents, &["despite"]) {
        Add(&Path, false, &mut Entries);
    }
    if App.Settings.HomeShowAutosaves {
        for Path in platform::list_files(&platform::autosave_dir(), &["despite"]) {
            Add(&Path, true, &mut Entries);
        }
    }
    App.Home.Entries = Entries;
    App.Home.LastScan = App.Now;
    App.Home.NeedsScan = false;
}

fn relative_time(Time: Option<SystemTime>) -> String {
    let Some(Time) = Time else { return String::new() };
    let Seconds = SystemTime::now().duration_since(Time).map(|Elapsed| Elapsed.as_secs()).unwrap_or(0);
    match Seconds {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", Seconds / 60),
        3600..=86_399 => format!("{} h ago", Seconds / 3600),
        86_400..=604_799 => format!("{} days ago", Seconds / 86_400),
        _ => {
            let Days = Time.duration_since(SystemTime::UNIX_EPOCH).map(|Since| Since.as_secs() / 86_400).unwrap_or(0) as i64;
            let (Year, Month, Day) = civil_from_days(Days);
            format!("{Year:04}-{Month:02}-{Day:02}")
        }
    }
}

fn civil_from_days(Days: i64) -> (i64, u32, u32) {
    let Shifted = Days + 719_468;
    let Era = Shifted.div_euclid(146_097);
    let DayOfEra = Shifted.rem_euclid(146_097);
    let YearOfEra = (DayOfEra - DayOfEra / 1460 + DayOfEra / 36_524 - DayOfEra / 146_096) / 365;
    let DayOfYear = DayOfEra - (365 * YearOfEra + YearOfEra / 4 - YearOfEra / 100);
    let MonthIndex = (5 * DayOfYear + 2) / 153;
    let Day = (DayOfYear - (153 * MonthIndex + 2) / 5 + 1) as u32;
    let Month = if MonthIndex < 10 { MonthIndex + 3 } else { MonthIndex - 9 } as u32;
    let Year = YearOfEra + Era * 400 + if Month <= 2 { 1 } else { 0 };
    (Year, Month, Day)
}

fn thumbnail(App: &mut DespiteApp, Context: &egui::Context, Entry: &ProjectEntry, Budget: &mut u32) -> Option<egui::TextureId> {
    let Fresh = App.Home.Thumbnails.get(&Entry.Path).is_some_and(|(_, Stamp)| *Stamp == Entry.Modified);
    if !Fresh && *Budget > 0 {
        *Budget -= 1;
        let Loaded = match despite_core::io::read_thumbnail(&Entry.Path) {
            Ok(Some(Pixels)) => Thumbnail::Ready(crate::view::raster_texture(Context, &format!("home-{}", Entry.Path.display()), &Pixels, false)),
            _ => Thumbnail::Missing,
        };
        App.Home.Thumbnails.insert(Entry.Path.clone(), (Loaded, Entry.Modified));
    }
    match App.Home.Thumbnails.get(&Entry.Path) {
        Some((Thumbnail::Ready(Texture), _)) => Some(Texture.id()),
        _ => None,
    }
}

fn paint_preview(Ui: &Ui, Rect: egui::Rect, Texture: Option<egui::TextureId>, Context: &egui::Context) {
    let Painter = Ui.painter();
    Painter.rect_filled(Rect, 6.0, Color32::from_gray(38));
    let Cell = 8.0;
    let Inner = Rect.shrink(6.0);
    match Texture {
        Some(Id) => {
            let Size = Context.tex_manager().read().meta(Id).map(|Meta| Meta.size).unwrap_or([1, 1]);
            let Aspect = Size[0] as f32 / Size[1].max(1) as f32;
            let Fitted = if Aspect >= Inner.width() / Inner.height() {
                egui::vec2(Inner.width(), Inner.width() / Aspect)
            } else {
                egui::vec2(Inner.height() * Aspect, Inner.height())
            };
            let Target = egui::Rect::from_center_size(Inner.center(), Fitted);
            let Columns = (Target.width() / Cell).ceil() as i32;
            let Rows = (Target.height() / Cell).ceil() as i32;
            for Row in 0..Rows {
                for Column in 0..Columns {
                    let Tile = egui::Rect::from_min_size(Target.min + egui::vec2(Column as f32 * Cell, Row as f32 * Cell), egui::vec2(Cell, Cell)).intersect(Target);
                    Painter.rect_filled(Tile, 0.0, if (Row + Column) % 2 == 0 { Color32::from_gray(150) } else { Color32::from_gray(115) });
                }
            }
            Painter.image(Id, Target, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            let Side = (Inner.height() * 0.3 / 16.0).floor().max(1.0) * 16.0;
            crate::icons::paint(Painter, Context, "Image", egui::Rect::from_center_size(Inner.center(), egui::vec2(Side, Side)), Color32::from_gray(90));
        }
    }
}

fn open_entry(App: &mut DespiteApp, Path: &Path) {
    if App.Project.Images.is_empty() && App.Project.Scenes.is_empty() || !App.Modified || App.ProjectPath.as_deref() == Some(Path) {
        if App.ProjectPath.as_deref() == Some(Path) && App.HasProject {
            App.Screen = Screen::Editor;
            return;
        }
        App.open_path(Path);
    } else {
        App.guard_unsaved(PendingAction::Open(Some(Path.to_path_buf())));
    }
}

fn card_menu(App: &mut DespiteApp, Response: &egui::Response, Entry: &ProjectEntry) {
    Response.context_menu(|Ui| {
        if Ui.button("Open").clicked() {
            open_entry(App, &Entry.Path);
            Ui.close();
        }
        if !platform::IsMobile && Ui.button("Show in folder").clicked() {
            if let Some(Parent) = Entry.Path.parent() {
                platform::open_in_file_manager(Parent);
            }
            Ui.close();
        }
        if App.Settings.RecentFiles.contains(&Entry.Path) && Ui.button("Remove from recent").clicked() {
            App.Settings.RecentFiles.retain(|Existing| *Existing != Entry.Path);
            App.Home.NeedsScan = true;
            Ui.close();
        }
        if Ui.button("Delete…").clicked() {
            App.Home.ConfirmDelete = Some(Entry.Path.clone());
            Ui.close();
        }
    });
}

pub fn ui(App: &mut DespiteApp, Ui: &mut Ui) {
    let Context = Ui.ctx().clone();
    if App.Home.NeedsScan || App.Now - App.Home.LastScan > 5.0 || App.Home.LastScan == 0.0 {
        scan(App);
    }
    egui::Panel::top("HomeHeader").show(Ui, |Ui| {
        Ui.add_space(8.0);
        Ui.horizontal_wrapped(|Ui| {
            Ui.label(RichText::new("Despite").size(30.0).strong());
            Ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).weak());
            Ui.add_space(16.0);
            if crate::icons::text_button(Ui, "Add", RichText::new("New Project").size(16.0)).clicked() {
                App.Dialog = Some(Dialog::NewProject(Default::default()));
            }
            if crate::icons::text_button(Ui, "Open", RichText::new("Open…").size(16.0)).clicked() {
                App.guard_unsaved(PendingAction::Open(None));
            }
            if crate::icons::text_button(Ui, "Image", RichText::new("New from Image…").size(16.0)).clicked() {
                App.project_from_image_dialog();
            }
            if App.HasProject {
                let Label = format!("↩ Continue “{}”{}", App.Project.Name, if App.Modified { " •" } else { "" });
                if Ui.button(RichText::new(Label).size(16.0).strong()).clicked() {
                    App.Screen = Screen::Editor;
                }
                if Ui.button(RichText::new("Close").size(16.0)).on_hover_text("Close the open project").clicked() {
                    App.guard_unsaved(PendingAction::Close);
                }
            }
            Ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |Ui| {
                if crate::icons::text_button(Ui, "Settings", "Settings").clicked() {
                    App.ShowSettings = true;
                }
            });
        });
        Ui.add_space(6.0);
        Ui.horizontal_wrapped(|Ui| {
            Ui.add(egui::TextEdit::singleline(&mut App.Home.Search).hint_text("🔍 Search projects").desired_width(220.0));
            Ui.separator();
            Ui.label("View");
            crate::icons::selectable(Ui, &mut App.Settings.HomeGridView, true, "GridView", "Grid");
            crate::icons::selectable(Ui, &mut App.Settings.HomeGridView, false, "ListView", "List");
            Ui.separator();
            Ui.label("Sort");
            Ui.selectable_value(&mut App.Settings.HomeSortByName, false, "Recent");
            Ui.selectable_value(&mut App.Settings.HomeSortByName, true, "Name");
            if App.Settings.HomeGridView {
                Ui.separator();
                Ui.add(egui::Slider::new(&mut App.Settings.HomeCardSize, 110.0..=320.0).show_value(false).text("size"));
            }
            Ui.separator();
            if Ui.checkbox(&mut App.Settings.HomeShowAutosaves, "Autosaves").changed() {
                App.Home.NeedsScan = true;
            }
            if crate::icons::button(Ui, "Refresh", false, "Refresh").clicked() {
                App.Home.Thumbnails.clear();
                App.Home.NeedsScan = true;
            }
        });
        Ui.add_space(6.0);
    });
    egui::Panel::bottom("HomeFooter").show(Ui, |Ui| {
        Ui.horizontal(|Ui| {
            let Folder = platform::documents_dir();
            Ui.label(RichText::new(format!("Projects folder: {}", Folder.display())).weak());
            if !platform::IsMobile && Ui.small_button("Open folder").clicked() {
                platform::open_in_file_manager(&Folder);
            }
            if let Some((Message, Time)) = &App.Status {
                if App.Now - Time < 6.0 {
                    Ui.separator();
                    Ui.label(Message);
                }
            }
        });
    });
    let Search = App.Home.Search.to_lowercase();
    let mut Entries: Vec<ProjectEntry> = App.Home.Entries.iter().filter(|Entry| Search.is_empty() || Entry.Name.to_lowercase().contains(&Search)).cloned().collect();
    if App.Settings.HomeSortByName {
        Entries.sort_by_key(|Entry| Entry.Name.to_lowercase());
    } else {
        Entries.sort_by_key(|Entry| std::cmp::Reverse(Entry.Modified));
    }
    egui::CentralPanel::default().show(Ui, |Ui| {
        if Entries.is_empty() {
            Ui.vertical_centered(|Ui| {
                Ui.add_space(80.0);
                Ui.label(RichText::new(if Search.is_empty() { "No projects yet" } else { "No projects match your search" }).size(22.0));
                Ui.add_space(8.0);
                Ui.label("Create a new project, open one from anywhere, or start from an image.");
                Ui.add_space(12.0);
                if crate::icons::text_button(Ui, "Add", RichText::new("New Project").size(18.0)).clicked() {
                    App.Dialog = Some(Dialog::NewProject(Default::default()));
                }
            });
            return;
        }
        let mut Budget = 3u32;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(Ui, |Ui| {
            if App.Settings.HomeGridView {
                let Available = Ui.available_width();
                let Size = if App.Compact { ((Available - 14.0) / 2.0).clamp(110.0, App.Settings.HomeCardSize.max(150.0)) } else { App.Settings.HomeCardSize };
                Ui.horizontal_wrapped(|Ui| {
                    Ui.spacing_mut().item_spacing = egui::vec2(14.0, 14.0);
                    for Entry in &Entries {
                        let Texture = thumbnail(App, &Context, Entry, &mut Budget);
                        let (Rect, Response) = Ui.allocate_exact_size(egui::vec2(Size, Size + 44.0), egui::Sense::click());
                        let Hovered = Response.hovered();
                        let Preview = egui::Rect::from_min_size(Rect.min, egui::vec2(Size, Size));
                        paint_preview(Ui, Preview, Texture, &Context);
                        if Hovered {
                            Ui.painter().rect_stroke(Preview, 6.0, egui::Stroke::new(2.0, Ui.visuals().selection.bg_fill), egui::StrokeKind::Inside);
                        }
                        let Painter = Ui.painter();
                        let Title = if Entry.Autosave { format!("⟲ {}", Entry.Name) } else { Entry.Name.clone() };
                        let TextColor = Ui.visuals().text_color();
                        Painter.text(Rect.min + egui::vec2(4.0, Size + 6.0), egui::Align2::LEFT_TOP, Title, egui::FontId::proportional(14.0), TextColor);
                        Painter.text(
                            Rect.min + egui::vec2(4.0, Size + 25.0),
                            egui::Align2::LEFT_TOP,
                            format!("{} · {}", relative_time(Entry.Modified), human_bytes(Entry.Size)),
                            egui::FontId::proportional(11.5),
                            Ui.visuals().weak_text_color(),
                        );
                        if Response.clicked() {
                            open_entry(App, &Entry.Path);
                        }
                        card_menu(App, &Response.on_hover_text(Entry.Path.display().to_string()), Entry);
                    }
                });
            } else {
                for Entry in &Entries {
                    let Texture = thumbnail(App, &Context, Entry, &mut Budget);
                    let (Rect, Response) = Ui.allocate_exact_size(egui::vec2(Ui.available_width(), 58.0), egui::Sense::click());
                    if Response.hovered() {
                        Ui.painter().rect_filled(Rect, 4.0, Ui.visuals().widgets.hovered.weak_bg_fill);
                    }
                    paint_preview(Ui, egui::Rect::from_min_size(Rect.min + egui::vec2(4.0, 4.0), egui::vec2(50.0, 50.0)), Texture, &Context);
                    let Painter = Ui.painter();
                    let Title = if Entry.Autosave { format!("⟲ {}", Entry.Name) } else { Entry.Name.clone() };
                    Painter.text(Rect.min + egui::vec2(66.0, 10.0), egui::Align2::LEFT_TOP, Title, egui::FontId::proportional(15.0), Ui.visuals().text_color());
                    Painter.text(
                        Rect.min + egui::vec2(66.0, 32.0),
                        egui::Align2::LEFT_TOP,
                        Entry.Path.display().to_string(),
                        egui::FontId::proportional(11.5),
                        Ui.visuals().weak_text_color(),
                    );
                    Painter.text(
                        Rect.right_center() - egui::vec2(12.0, 0.0),
                        egui::Align2::RIGHT_CENTER,
                        format!("{}   {}", relative_time(Entry.Modified), human_bytes(Entry.Size)),
                        egui::FontId::proportional(12.0),
                        Ui.visuals().weak_text_color(),
                    );
                    if Response.clicked() {
                        open_entry(App, &Entry.Path);
                    }
                    card_menu(App, &Response, Entry);
                }
            }
        });
        if Budget == 0 {
            Context.request_repaint();
        }
    });
    if let Some(Path) = App.Home.ConfirmDelete.clone() {
        let mut Keep = true;
        egui::Window::new("Delete project?").collapsible(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(&Context, |Ui| {
            Ui.label(format!("Permanently delete {}?", Path.display()));
            Ui.horizontal(|Ui| {
                if Ui.button("Delete").clicked() {
                    match std::fs::remove_file(&Path) {
                        Ok(()) => App.set_status(format!("Deleted {}", Path.display())),
                        Err(Problem) => App.set_status(format!("Could not delete: {Problem}")),
                    }
                    App.Settings.RecentFiles.retain(|Existing| *Existing != Path);
                    App.Home.NeedsScan = true;
                    Keep = false;
                }
                if Ui.button("Cancel").clicked() {
                    Keep = false;
                }
            });
        });
        if !Keep {
            App.Home.ConfirmDelete = None;
        }
    }
}
