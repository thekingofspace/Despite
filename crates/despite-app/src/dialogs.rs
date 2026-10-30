use std::path::PathBuf;
use std::sync::Arc;

use despite_core::brush::DitherPattern;
use despite_core::export::{self, FrameSource, GifOptions, LayerSpace, ScaleOptions, SheetLayout, SheetOptions};
use despite_core::filters::Filter;
use despite_core::{Brush, Color, DocumentRef, ImageId, LayerId, ProjectKind, Raster, ResampleMethod};
use egui::{Context, RichText, Ui};

use crate::app::DespiteApp;
use crate::panels::color_button;
use crate::platform;
use crate::settings::{LayoutMode, ScrollAction, Settings, ThemeChoice};

#[derive(Clone, Debug)]
pub enum PendingAction {
    NewProject,
    Open(Option<PathBuf>),
    Close,
    Quit,
    OpenSampleRig,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackgroundChoice {
    #[default]
    Transparent,
    White,
    Black,
    Custom,
}

impl BackgroundChoice {
    fn resolve(self, Custom: Color) -> Option<Color> {
        match self {
            BackgroundChoice::Transparent => None,
            BackgroundChoice::White => Some(Color::White),
            BackgroundChoice::Black => Some(Color::Black),
            BackgroundChoice::Custom => Some(Custom),
        }
    }
}

fn background_picker(Ui: &mut Ui, Choice: &mut BackgroundChoice, Custom: &mut Color) {
    Ui.horizontal(|Ui| {
        Ui.selectable_value(Choice, BackgroundChoice::Transparent, "Transparent");
        Ui.selectable_value(Choice, BackgroundChoice::White, "White");
        Ui.selectable_value(Choice, BackgroundChoice::Black, "Black");
        Ui.selectable_value(Choice, BackgroundChoice::Custom, "Color");
        if *Choice == BackgroundChoice::Custom {
            color_button(Ui, Custom);
        }
    });
}

pub fn number_box(Ui: &mut Ui, Value: &mut u32, Minimum: u32, Maximum: u32, Salt: &str) -> bool {
    let Id = Ui.scope_id().with(("NumberBox", Salt));
    let mut Text: String = Ui.data(|Data| Data.get_temp(Id)).unwrap_or_else(|| Value.to_string());
    let Response = Ui.add(egui::TextEdit::singleline(&mut Text).desired_width(72.0).char_limit(6));
    let mut Changed = false;
    if Response.changed() {
        Text.retain(|Character| Character.is_ascii_digit());
        if let Ok(Parsed) = Text.parse::<u32>() {
            let Clamped = Parsed.clamp(Minimum, Maximum);
            if Clamped != *Value {
                *Value = Clamped;
                Changed = true;
            }
        }
    }
    if !Response.has_focus() {
        Text = Value.to_string();
    }
    Ui.data_mut(|Data| Data.insert_temp(Id, Text));
    Changed
}

fn size_fields(Ui: &mut Ui, Width: &mut u32, Height: &mut u32, PixelArt: &mut bool) {
    let Maximum = despite_core::raster::MaxDimension;
    Ui.horizontal(|Ui| {
        Ui.label("Width");
        number_box(Ui, Width, 1, Maximum, "Width");
        Ui.label("Height");
        number_box(Ui, Height, 1, Maximum, "Height");
        Ui.label("px");
    });
    Ui.checkbox(PixelArt, "Pixel art (crisp nearest-neighbour, pixel pencil)");
}

#[derive(Clone, Debug)]
pub struct NewProjectState {
    pub Name: String,
    pub Kind: ProjectKind,
    pub Width: u32,
    pub Height: u32,
    pub PixelArt: bool,
    pub Background: BackgroundChoice,
    pub Custom: Color,
}

impl Default for NewProjectState {
    fn default() -> Self {
        Self { Name: "Untitled".into(), Kind: ProjectKind::Art, Width: 64, Height: 64, PixelArt: true, Background: BackgroundChoice::Transparent, Custom: Color::White }
    }
}

#[derive(Clone, Debug)]
pub struct NewImageState {
    pub Name: String,
    pub Width: u32,
    pub Height: u32,
    pub PixelArt: bool,
    pub Background: BackgroundChoice,
    pub Custom: Color,
}

impl NewImageState {
    pub fn new(Settings: &Settings) -> Self {
        Self {
            Name: "Image".into(),
            Width: Settings.DefaultWidth,
            Height: Settings.DefaultHeight,
            PixelArt: Settings.DefaultPixelArt,
            Background: if Settings.DefaultBackground.is_some() { BackgroundChoice::Custom } else { BackgroundChoice::Transparent },
            Custom: Settings.DefaultBackground.unwrap_or(Color::White),
        }
    }
}

#[derive(Clone, Debug)]
pub struct NewSceneState {
    pub Name: String,
    pub Width: u32,
    pub Height: u32,
    pub Fps: f32,
    pub Background: BackgroundChoice,
    pub Custom: Color,
}

impl Default for NewSceneState {
    fn default() -> Self {
        Self { Name: "Scene".into(), Width: 320, Height: 180, Fps: 12.0, Background: BackgroundChoice::Transparent, Custom: Color::White }
    }
}

#[derive(Clone, Debug)]
pub struct ResizeState {
    pub Width: u32,
    pub Height: u32,
    pub AnchorX: f32,
    pub AnchorY: f32,
}

impl ResizeState {
    pub fn new(App: &DespiteApp) -> Self {
        let (Width, Height) = App.Project.active_image().map(|Found| (Found.Width, Found.Height)).unwrap_or((64, 64));
        Self { Width, Height, AnchorX: 0.5, AnchorY: 0.5 }
    }
}

#[derive(Clone, Debug)]
pub struct ScaleState {
    pub Width: u32,
    pub Height: u32,
    pub Percent: f32,
    pub KeepAspect: bool,
    pub Method: ResampleMethod,
    pub OriginalWidth: u32,
    pub OriginalHeight: u32,
}

impl ScaleState {
    pub fn new(App: &DespiteApp) -> Self {
        let Found = App.Project.active_image();
        let (Width, Height) = Found.map(|Found| (Found.Width, Found.Height)).unwrap_or((64, 64));
        let PixelArt = Found.is_some_and(|Found| Found.PixelArt);
        Self {
            Width,
            Height,
            Percent: 100.0,
            KeepAspect: true,
            Method: if PixelArt { ResampleMethod::Nearest } else { ResampleMethod::Lanczos3 },
            OriginalWidth: Width,
            OriginalHeight: Height,
        }
    }
}

pub struct FilterState {
    pub Chosen: Filter,
    pub Preview: bool,
    pub Original: Option<(ImageId, LayerId, Arc<Raster>)>,
    pub Dirty: bool,
}

impl FilterState {
    pub fn new(Chosen: Filter) -> Self {
        Self { Chosen, Preview: true, Original: None, Dirty: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportWhat {
    CurrentDocument,
    CurrentFrame,
    ActiveLayer,
    EachLayer,
    AllImages,
}

impl ExportWhat {
    fn label(self, Animation: bool) -> &'static str {
        if !Animation {
            return self.name();
        }
        match self {
            ExportWhat::CurrentFrame => "Current frame only",
            ExportWhat::AllImages => "All animations, one after another",
            _ => "Current animation",
        }
    }

    fn name(self) -> &'static str {
        match self {
            ExportWhat::CurrentDocument => "Current image / whole scene",
            ExportWhat::CurrentFrame => "Current scene frame",
            ExportWhat::ActiveLayer => "Active layer",
            ExportWhat::EachLayer => "Each visible layer (as frames)",
            ExportWhat::AllImages => "All images in project (as frames)",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    SpriteSheet,
    Gif,
    PngSequence,
    Video,
}

impl ExportFormat {
    fn name(self) -> &'static str {
        match self {
            ExportFormat::Png => "PNG image",
            ExportFormat::SpriteSheet => "Sprite sheet (PNG + JSON)",
            ExportFormat::Gif => "Animated GIF",
            ExportFormat::PngSequence => "PNG sequence (folder)",
            ExportFormat::Video => "Video (MP4 / WebM / MOV / WebP)",
        }
    }
}

pub struct ExportState {
    pub What: ExportWhat,
    pub Format: ExportFormat,
    pub Scale: ScaleOptions,
    pub Space: LayerSpace,
    pub Sheet: SheetOptions,
    pub WriteJson: bool,
    pub Gif: GifOptions,
    pub Flatten: bool,
    pub FlattenColor: Color,
    pub FileName: String,
    pub Result: Option<String>,
    #[cfg(feature = "desktop")]
    pub Video: despite_video::VideoOptions,
    #[cfg(feature = "desktop")]
    pub Code: crate::desktop::export::CodeSource,
}

impl ExportState {
    pub fn new(App: &DespiteApp) -> Self {
        let Animation = crate::animator::is_animation(App);
        let Name = if Animation {
            App.Project.Rig.Clips.get(crate::animator::clip_index(App)).map(|Found| Found.Name.clone()).unwrap_or_else(|| "animation".into())
        } else {
            App.Project.Active.and_then(|Document| App.Project.document_name(Document)).unwrap_or("export").to_string()
        };
        let IsScene = App.active_scene_id().is_some() || Animation;
        Self {
            What: ExportWhat::CurrentDocument,
            Format: if IsScene { ExportFormat::Gif } else { ExportFormat::Png },
            Scale: ScaleOptions { Factor: App.Settings.ExportScale, Method: App.Settings.ExportMethod, ..Default::default() },
            Space: LayerSpace::ImageCanvas,
            Sheet: App.Settings.Sheet.clone(),
            WriteJson: App.Settings.WriteSheetJson,
            Gif: App.Settings.Gif,
            Flatten: false,
            FlattenColor: Color::White,
            FileName: export::sanitize_file_name(&Name),
            Result: None,
            #[cfg(feature = "desktop")]
            Video: despite_video::VideoOptions { Quality: App.Settings.VideoQuality, Ffmpeg: App.Settings.FfmpegPath.clone(), ..Default::default() },
            #[cfg(feature = "desktop")]
            Code: crate::desktop::export::CodeSource::Document,
        }
    }

    fn extension(&self) -> &'static str {
        match self.Format {
            ExportFormat::Png | ExportFormat::SpriteSheet | ExportFormat::PngSequence => "png",
            ExportFormat::Gif => "gif",
            #[cfg(feature = "desktop")]
            ExportFormat::Video => self.Video.Format.extension(),
            #[cfg(not(feature = "desktop"))]
            ExportFormat::Video => "mp4",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserMode {
    Open,
    Save,
    Import { AsLayers: bool },
}

pub struct FileBrowserState {
    pub Mode: BrowserMode,
    pub FileName: String,
    pub ShowAutosaves: bool,
}

impl FileBrowserState {
    pub fn open() -> Self {
        Self { Mode: BrowserMode::Open, FileName: String::new(), ShowAutosaves: false }
    }

    pub fn save(Name: &str) -> Self {
        Self { Mode: BrowserMode::Save, FileName: export::sanitize_file_name(Name), ShowAutosaves: false }
    }

    pub fn import(AsLayers: bool) -> Self {
        Self { Mode: BrowserMode::Import { AsLayers }, FileName: String::new(), ShowAutosaves: false }
    }
}

#[derive(Clone, Debug)]
pub enum RenameTarget {
    Document(DocumentRef),
    Layer(ImageId, usize),
    Project,
    BrushPreset(Box<Brush>),
}

#[derive(Clone, Debug)]
pub struct RenameState {
    pub Target: RenameTarget,
    pub Text: String,
}

pub enum Dialog {
    NewProject(NewProjectState),
    NewImage(NewImageState),
    NewScene(NewSceneState),
    ResizeCanvas(ResizeState),
    ScaleImage(ScaleState),
    Filter(FilterState),
    Export(ExportState),
    FileBrowser(FileBrowserState),
    ConfirmDiscard(PendingAction),
    Rename(RenameState),
    About,
    Message(String, String),
    PasteTooLarge(Raster),
    ShareBrush(ShareBrushState),
    InstallBrush,
    ManageBrushes,
    Shortcuts,
}

#[derive(Clone, Debug)]
pub struct ShareBrushState {
    pub Brush: Brush,
    pub PackName: String,
    pub Author: String,
    pub Description: String,
    pub IncludeMine: bool,
    pub Result: Option<String>,
}

impl ShareBrushState {
    pub fn new(Current: &Brush) -> Self {
        Self { PackName: Current.Name.clone(), Brush: Current.clone(), Author: String::new(), Description: String::new(), IncludeMine: false, Result: None }
    }
}

pub fn shortcut_text() -> String {
    let mut Lines = vec![
        "Ctrl+N new project · Ctrl+O open · Ctrl+S save · Ctrl+Shift+S save as · Ctrl+E export".to_string(),
        "Ctrl+Z undo · Ctrl+Shift+Z / Ctrl+Y redo".to_string(),
        "Ctrl+A select all · Ctrl+D deselect · Ctrl+Shift+I invert · Delete clears selection".to_string(),
        "Ctrl+C copy · Ctrl+Shift+C copy merged · Ctrl+X cut · Ctrl+V paste as layer · Ctrl+J selection to layer".to_string(),
        "Ctrl+Shift+N new layer · [ / ] brush size · X swap colors · D default colors".to_string(),
        "Space+drag or middle mouse pans · scroll / pinch zooms · 0 fit · 1 actual size · + / - zoom".to_string(),
        "Alt+click picks a color · Shift+click draws a straight line · Tab hides panels".to_string(),
        "Two-finger tap undo · three-finger tap redo (touch)".to_string(),
        "Space plays a scene · arrow keys nudge with the Move tool".to_string(),
        "Enter applies a floating selection or deselects · Escape cancels".to_string(),
        "Move tool: drag corners to scale (Shift keeps proportions), drag just outside a corner to rotate (Shift snaps 15°)".to_string(),
    ];
    Lines.retain(|Line| !Line.is_empty());
    Lines.join("\n")
}

pub fn shortcuts_ui(Ui: &mut Ui) {
    Ui.label(shortcut_text());
    Ui.add_space(8.0);
    Ui.label(RichText::new("Tools").strong());
    egui::Grid::new("ToolShortcuts").num_columns(3).spacing([12.0, 4.0]).show(Ui, |Ui| {
        for Candidate in crate::tools::Tool::All {
            crate::icons::show(Ui, Candidate.icon_name());
            Ui.label(RichText::new(Candidate.shortcut_label()).monospace());
            Ui.label(Candidate.name());
            Ui.end_row();
        }
    });
}

pub struct ModalWindow<'a> {
    Window: egui::Window<'a>,
    MaxHeight: f32,
}

impl<'a> ModalWindow<'a> {
    pub fn default_width(mut self, Width: f32) -> Self {
        self.Window = self.Window.default_width(Width);
        self
    }

    pub fn show<R>(self, Context: &Context, Content: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        let MaxHeight = self.MaxHeight;
        self.Window
            .show(Context, |Ui| egui::ScrollArea::vertical().auto_shrink([false, true]).max_height(MaxHeight).show(Ui, Content).inner)
            .and_then(|Shown| Shown.inner)
    }
}

pub fn modal_window<'a>(Title: impl Into<egui::WidgetText>, Context: &Context, Open: &'a mut bool, Compact: bool) -> ModalWindow<'a> {
    let Screen = Context.content_rect();
    let Width = (Screen.width() - 24.0).max(200.0);
    let Window = egui::Window::new(Title)
        .open(Open)
        .collapsible(false)
        .resizable([true, false])
        .default_width(Width.min(440.0))
        .max_width(Width);
    let Window = if Compact {
        Window.anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
    } else {
        Window.pivot(egui::Align2::CENTER_CENTER).default_pos(Screen.center()).constrain(true)
    };
    ModalWindow { Window, MaxHeight: (Screen.height() - 90.0).max(160.0) }
}

pub fn show(App: &mut DespiteApp, Context: &Context) {
    let Some(Current) = App.Dialog.take() else { return };
    let mut Keep = true;
    let mut Open = true;
    let Compact = App.Compact;
    let mut Next: Option<Dialog> = None;
    match Current {
        Dialog::NewProject(mut State) => {
            modal_window("New Project", Context, &mut Open, Compact).show(Context, |Ui| {
                egui::Grid::new("NewProjectGrid").num_columns(2).show(Ui, |Ui| {
                    Ui.label("Name");
                    Ui.text_edit_singleline(&mut State.Name);
                    Ui.end_row();
                    Ui.label("Project type");
                    Ui.vertical(|Ui| {
                        for Kind in ProjectKind::All {
                            Ui.radio_value(&mut State.Kind, *Kind, Kind.name()).on_hover_text(Kind.description());
                        }
                    });
                    Ui.end_row();
                });
                Ui.label(RichText::new(State.Kind.description()).weak());
                Ui.separator();
                Ui.label(RichText::new(if State.Kind == ProjectKind::Animation { "Stage (the size of the exported animation)" } else { "First image" }).strong());
                size_fields(Ui, &mut State.Width, &mut State.Height, &mut State.PixelArt);
                background_picker(Ui, &mut State.Background, &mut State.Custom);
                Ui.separator();
                Ui.horizontal(|Ui| {
                    if Ui.button(RichText::new("Create").strong()).clicked() {
                        let Background = State.Background.resolve(State.Custom);
                        App.create_project(State.Kind, &State.Name, State.Width, State.Height, State.PixelArt, Background);
                        Keep = false;
                    }
                    if Ui.button("Open existing…").clicked() {
                        Keep = false;
                        App.open_dialog();
                    }
                    if State.Kind == ProjectKind::Animation && Ui.button("Start from the sample rig").on_hover_text("A finished character with bones, IK, look-at, logic and three animations to explore").clicked() {
                        Keep = false;
                        crate::animator::open_sample(App);
                    }
                    if Ui.button("Cancel").clicked() {
                        Keep = false;
                    }
                });
            });
            if Keep {
                Next = Some(Dialog::NewProject(State));
            }
        }
        Dialog::NewImage(mut State) => {
            modal_window("New Image", Context, &mut Open, Compact).show(Context, |Ui| {
                Ui.horizontal(|Ui| {
                    Ui.label("Name");
                    Ui.text_edit_singleline(&mut State.Name);
                });
                size_fields(Ui, &mut State.Width, &mut State.Height, &mut State.PixelArt);
                background_picker(Ui, &mut State.Background, &mut State.Custom);
                if let Some(Found) = App.Project.active_image()
                    && Ui.button(format!("Same size as {} ({}×{})", Found.Name, Found.Width, Found.Height)).clicked() {
                        State.Width = Found.Width;
                        State.Height = Found.Height;
                        State.PixelArt = Found.PixelArt;
                    }
                Ui.horizontal(|Ui| {
                    if Ui.button(RichText::new("Create").strong()).clicked() {
                        let Background = State.Background.resolve(State.Custom);
                        App.add_image(&State.Name, State.Width, State.Height, State.PixelArt, Background);
                        Keep = false;
                    }
                    if Ui.button("Cancel").clicked() {
                        Keep = false;
                    }
                });
            });
            if Keep {
                Next = Some(Dialog::NewImage(State));
            }
        }
        Dialog::NewScene(mut State) => {
            modal_window("New Scene", Context, &mut Open, Compact).show(Context, |Ui| {
                Ui.label("A scene arranges many project images together (with positions, scale, rotation, opacity and blend), and can play animated images.");
                egui::Grid::new("NewSceneGrid").num_columns(2).show(Ui, |Ui| {
                    Ui.label("Name");
                    Ui.text_edit_singleline(&mut State.Name);
                    Ui.end_row();
                    Ui.label("Size");
                    Ui.horizontal(|Ui| {
                        number_box(Ui, &mut State.Width, 1, 16384, "SceneWidth");
                        Ui.label("×");
                        number_box(Ui, &mut State.Height, 1, 16384, "SceneHeight");
                    });
                    Ui.end_row();
                    Ui.label("FPS");
                    Ui.add(egui::DragValue::new(&mut State.Fps).range(0.1..=120.0));
                    Ui.end_row();
                });
                background_picker(Ui, &mut State.Background, &mut State.Custom);
                Ui.horizontal(|Ui| {
                    if Ui.button(RichText::new("Create").strong()).clicked() {
                        let Background = State.Background.resolve(State.Custom);
                        App.add_scene(&State.Name, State.Width, State.Height, State.Fps, Background);
                        Keep = false;
                    }
                    if Ui.button("Cancel").clicked() {
                        Keep = false;
                    }
                });
            });
            if Keep {
                Next = Some(Dialog::NewScene(State));
            }
        }
        Dialog::ResizeCanvas(mut State) => {
            modal_window("Resize Canvas", Context, &mut Open, Compact).show(Context, |Ui| {
                Ui.horizontal(|Ui| {
                    Ui.label("Width");
                    number_box(Ui, &mut State.Width, 1, 16384, "ResizeWidth");
                    Ui.label("Height");
                    number_box(Ui, &mut State.Height, 1, 16384, "ResizeHeight");
                });
                Ui.label("Anchor");
                egui::Grid::new("AnchorGrid").show(Ui, |Ui| {
                    for Row in 0..3 {
                        for Column in 0..3 {
                            let (X, Y) = (Column as f32 * 0.5, Row as f32 * 0.5);
                            let Selected = (State.AnchorX - X).abs() < 0.01 && (State.AnchorY - Y).abs() < 0.01;
                            if Ui.selectable_label(Selected, if Selected { "●" } else { "○" }).clicked() {
                                State.AnchorX = X;
                                State.AnchorY = Y;
                            }
                        }
                        Ui.end_row();
                    }
                });
                Ui.horizontal(|Ui| {
                    if Ui.button("Resize").clicked() {
                        let (Width, Height, AnchorX, AnchorY) = (State.Width, State.Height, State.AnchorX, State.AnchorY);
                        App.with_active_layer("Resize canvas", |Target| Target.resize_canvas(Width, Height, AnchorX, AnchorY));
                        App.deselect();
                        App.center_symmetry();
                        Keep = false;
                    }
                    if Ui.button("Cancel").clicked() {
                        Keep = false;
                    }
                });
            });
            if Keep {
                Next = Some(Dialog::ResizeCanvas(State));
            }
        }
        Dialog::ScaleImage(mut State) => {
            modal_window("Scale Image", Context, &mut Open, Compact).show(Context, |Ui| {
                Ui.horizontal(|Ui| {
                    Ui.label("Percent");
                    if Ui.add(egui::DragValue::new(&mut State.Percent).range(1.0..=6400.0).suffix("%")).changed() {
                        State.Width = ((State.OriginalWidth as f32 * State.Percent / 100.0).round() as u32).max(1);
                        State.Height = ((State.OriginalHeight as f32 * State.Percent / 100.0).round() as u32).max(1);
                    }
                    for Factor in [50.0, 200.0, 300.0, 400.0, 800.0] {
                        if Ui.small_button(format!("{}×", Factor / 100.0)).clicked() {
                            State.Percent = Factor;
                            State.Width = ((State.OriginalWidth as f32 * Factor / 100.0).round() as u32).max(1);
                            State.Height = ((State.OriginalHeight as f32 * Factor / 100.0).round() as u32).max(1);
                        }
                    }
                });
                Ui.horizontal(|Ui| {
                    Ui.label("Width");
                    if number_box(Ui, &mut State.Width, 1, 16384, "ScaleWidth") && State.KeepAspect {
                        State.Height = ((State.Width as u64 * State.OriginalHeight as u64) / State.OriginalWidth.max(1) as u64).max(1) as u32;
                    }
                    Ui.label("Height");
                    if number_box(Ui, &mut State.Height, 1, 16384, "ScaleHeight") && State.KeepAspect {
                        State.Width = ((State.Height as u64 * State.OriginalWidth as u64) / State.OriginalHeight.max(1) as u64).max(1) as u32;
                    }
                    Ui.checkbox(&mut State.KeepAspect, "Keep aspect");
                });
                resample_combo(Ui, "ScaleMethod", &mut State.Method);
                Ui.horizontal(|Ui| {
                    if Ui.button("Scale").clicked() {
                        let (Width, Height, Method) = (State.Width, State.Height, State.Method);
                        App.with_active_layer("Scale image", |Target| {
                            let (FactorX, FactorY) = (Width as f32 / Target.Width.max(1) as f32, Height as f32 / Target.Height.max(1) as f32);
                            for Entry in &mut Target.Layers {
                                let NewWidth = ((Entry.Pixels.width() as f32 * FactorX).round() as u32).max(1);
                                let NewHeight = ((Entry.Pixels.height() as f32 * FactorY).round() as u32).max(1);
                                Entry.Pixels = Arc::new(despite_core::resample::resample(&Entry.Pixels, NewWidth, NewHeight, Method));
                                Entry.Transform.OffsetX *= FactorX;
                                Entry.Transform.OffsetY *= FactorY;
                            }
                            Target.Width = Width;
                            Target.Height = Height;
                        });
                        App.deselect();
                        App.center_symmetry();
                        Keep = false;
                    }
                    if Ui.button("Cancel").clicked() {
                        Keep = false;
                    }
                });
            });
            if Keep {
                Next = Some(Dialog::ScaleImage(State));
            }
        }
        Dialog::Filter(mut State) => {
            let mut Apply = false;
            let mut Cancel = false;
            modal_window(State.Chosen.name(), Context, &mut Open, Compact).show(Context, |Ui| {
                if filter_settings(Ui, &mut State.Chosen) {
                    State.Dirty = true;
                }
                if Ui.checkbox(&mut State.Preview, "Live preview").changed() {
                    State.Dirty = true;
                }
                Ui.horizontal(|Ui| {
                    Apply = Ui.button(RichText::new("Apply").strong()).clicked();
                    Cancel = Ui.button("Cancel").clicked();
                });
            });
            Cancel |= !Open;
            if State.Dirty {
                State.Dirty = false;
                filter_preview(App, &mut State);
            }
            if Apply || Cancel {
                if let Some((Image, LayerRef, Original)) = State.Original.take() {
                    if let Some(Entry) = App.Project.image_mut(Image).and_then(|Found| Found.layer_mut(LayerRef)) {
                        Entry.Pixels = Original;
                    }
                    App.touch();
                }
                if Apply {
                    App.apply_filter(&State.Chosen);
                }
                Keep = false;
            }
            if Keep {
                Next = Some(Dialog::Filter(State));
            }
        }
        Dialog::Export(mut State) => {
            modal_window("Export", Context, &mut Open, Compact).default_width(520.0).show(Context, |Ui| {
                export_ui(App, Ui, &mut State, &mut Keep);
            });
            if Keep {
                Next = Some(Dialog::Export(State));
            }
        }
        Dialog::FileBrowser(mut State) => {
            modal_window(match State.Mode {
                BrowserMode::Open => "Open Project",
                BrowserMode::Save => "Save Project",
                BrowserMode::Import { .. } => "Import Image",
            }, Context, &mut Open, Compact)
            .show(Context, |Ui| {
                file_browser_ui(App, Ui, &mut State, &mut Keep);
            });
            if Keep {
                Next = Some(Dialog::FileBrowser(State));
            }
        }
        Dialog::ConfirmDiscard(Action) => {
            modal_window("Unsaved changes", Context, &mut Open, Compact).show(Context, |Ui| {
                Ui.label(format!("\"{}\" has unsaved changes.", App.Project.Name));
                Ui.horizontal(|Ui| {
                    if Ui.button("Save").clicked() {
                        App.save();
                        Keep = false;
                        if !App.Modified {
                            App.perform_pending(Action.clone());
                        }
                    }
                    if Ui.button("Discard").clicked() {
                        Keep = false;
                        App.Modified = false;
                        App.perform_pending(Action.clone());
                    }
                    if Ui.button("Cancel").clicked() {
                        Keep = false;
                    }
                });
            });
            if Keep {
                Next = Some(Dialog::ConfirmDiscard(Action));
            }
        }
        Dialog::Rename(mut State) => {
            modal_window("Name", Context, &mut Open, Compact).show(Context, |Ui| {
                let Response = Ui.text_edit_singleline(&mut State.Text);
                Response.request_focus();
                let Enter = Ui.input(|Input| Input.key_pressed(egui::Key::Enter));
                Ui.horizontal(|Ui| {
                    if Ui.button("OK").clicked() || Enter {
                        apply_rename(App, &State);
                        Keep = false;
                    }
                    if Ui.button("Cancel").clicked() {
                        Keep = false;
                    }
                });
            });
            if Keep {
                Next = Some(Dialog::Rename(State));
            }
        }
        Dialog::About => {
            modal_window("About Despite", Context, &mut Open, Compact).show(Context, |Ui| {
                Ui.heading(format!("Despite {}", env!("CARGO_PKG_VERSION")));
                Ui.label("An open source art program for pixel art, painting, sprite sheets, scenes, shaders and generative project code.");
                Ui.label(format!("Licensed {}", env!("CARGO_PKG_LICENSE")));
                Ui.label(format!("Data folder: {}", platform::data_dir().display()));
                Ui.label(format!("Documents folder: {}", platform::documents_dir().display()));
                if Ui.button("Close").clicked() {
                    Keep = false;
                }
            });
            if Keep {
                Next = Some(Dialog::About);
            }
        }
        Dialog::PasteTooLarge(Pixels) => {
            let Canvas = App.Project.active_image().map(|Found| (Found.Width, Found.Height)).unwrap_or((0, 0));
            let mut Choice = None;
            modal_window("Pasted image is larger than the canvas", Context, &mut Open, Compact).show(Context, |Ui| {
                Ui.label(crate::clipboard::oversize_message(&Pixels, Canvas));
                Ui.label("What would you like to do?");
                Ui.add_space(6.0);
                if Ui.button("Expand the canvas to fit").clicked() {
                    Choice = Some(crate::clipboard::OversizeChoice::ExpandCanvas);
                }
                if Ui.button("Scale the image down to fit").clicked() {
                    Choice = Some(crate::clipboard::OversizeChoice::ScaleToFit);
                }
                if Ui.button("Paste anyway (parts outside the canvas stay hidden)").clicked() {
                    Choice = Some(crate::clipboard::OversizeChoice::PasteAnyway);
                }
                if Ui.button("Paste as a new image").clicked() {
                    Choice = Some(crate::clipboard::OversizeChoice::NewImage);
                }
                if Ui.button("Cancel").clicked() {
                    Keep = false;
                }
            });
            match Choice {
                Some(Picked) => App.resolve_oversize(Pixels, Picked),
                None if Keep => Next = Some(Dialog::PasteTooLarge(Pixels)),
                None => {}
            }
        }
        Dialog::ShareBrush(mut State) => {
            modal_window("Share brush", Context, &mut Open, Compact).show(Context, |Ui| {
                Ui.label("Packages brushes into one .despitebrush file. Send it to another device (chat, email, AirDrop, USB) and install it there from Brush → Install brushes.");
                egui::Grid::new("ShareBrushGrid").num_columns(2).show(Ui, |Ui| {
                    Ui.label("Pack name");
                    Ui.text_edit_singleline(&mut State.PackName);
                    Ui.end_row();
                    Ui.label("Author");
                    Ui.text_edit_singleline(&mut State.Author);
                    Ui.end_row();
                    Ui.label("Description");
                    Ui.text_edit_singleline(&mut State.Description);
                    Ui.end_row();
                });
                Ui.checkbox(&mut State.IncludeMine, format!("Also include my {} saved presets", App.Brushes.User.len()));
                if let Some(Message) = &State.Result {
                    Ui.label(Message);
                }
                Ui.horizontal(|Ui| {
                    if Ui.button(RichText::new("Create file").strong()).clicked() {
                        let mut Brushes = vec![State.Brush.clone()];
                        if State.IncludeMine {
                            Brushes.extend(App.Brushes.User.iter().filter(|Entry| Entry.Name != State.Brush.Name).cloned());
                        }
                        let mut Pack = despite_core::brush::BrushPack::new(State.PackName.clone(), Brushes);
                        Pack.Author = State.Author.clone();
                        Pack.Description = State.Description.clone();
                        State.Result = Some(match write_brush_pack(App, &Pack) {
                            Ok(Some(Path)) => format!("Saved {}", Path.display()),
                            Ok(None) => "Cancelled".into(),
                            Err(Problem) => format!("Could not save: {Problem}"),
                        });
                    }
                    if Ui.button("Close").clicked() {
                        Keep = false;
                    }
                });
            });
            if Keep {
                Next = Some(Dialog::ShareBrush(State));
            }
        }
        Dialog::Shortcuts => {
            modal_window("Keyboard shortcuts", Context, &mut Open, Compact).show(Context, |Ui| {
                shortcuts_ui(Ui);
                if Ui.button("Close").clicked() {
                    Keep = false;
                }
            });
            if Keep {
                Next = Some(Dialog::Shortcuts);
            }
        }
        Dialog::InstallBrush => {
            modal_window("Install brushes", Context, &mut Open, Compact).show(Context, |Ui| {
                Ui.label("Brush files (.despitebrush) found on this device:");
                let mut Found: Vec<PathBuf> = Vec::new();
                for Folder in platform::brush_search_dirs() {
                    Found.extend(platform::list_files(&Folder, &[despite_core::brush::PackExtension]));
                }
                Found.dedup();
                if Found.is_empty() {
                    Ui.label(RichText::new("No brush files found yet.").strong());
                }
                for Path in Found {
                    let Name = Path.file_name().and_then(|Name| Name.to_str()).unwrap_or("?").to_string();
                    Ui.horizontal(|Ui| {
                        if crate::icons::text_button(Ui, "Import", RichText::new(format!("Install {Name}")).size(15.0)).clicked() {
                            App.install_brush_file(&Path);
                            Keep = false;
                        }
                    });
                    Ui.label(RichText::new(Path.parent().map(|Parent| Parent.display().to_string()).unwrap_or_default()).weak().small());
                }
                Ui.separator();
                Ui.label("Put received brush files in one of these folders:");
                for Folder in platform::brush_search_dirs().into_iter().take(3) {
                    Ui.label(RichText::new(Folder.display().to_string()).monospace().small());
                }
                if cfg!(target_os = "ios") {
                    Ui.label(RichText::new("On iPhone/iPad: share the file to Despite, or save it into Files → On My iPhone → Despite.").weak());
                }
                Ui.horizontal(|Ui| {
                    #[cfg(feature = "desktop")]
                    if Ui.button("Choose file…").clicked() {
                        if let Some(Path) = platform::dialogs::open_file("Despite brushes", &[despite_core::brush::PackExtension, "json"]) {
                            App.install_brush_file(&Path);
                            Keep = false;
                        }
                    }
                    if Ui.button("Close").clicked() {
                        Keep = false;
                    }
                });
            });
            if Keep {
                Next = Some(Dialog::InstallBrush);
            }
        }
        Dialog::ManageBrushes => {
            modal_window("Manage brushes", Context, &mut Open, Compact).show(Context, |Ui| {
                Ui.label(RichText::new("Installed brush packs").strong());
                if App.Brushes.Installed.is_empty() {
                    Ui.label(RichText::new("None installed.").weak());
                }
                let Packs: Vec<(String, String, Vec<String>)> = App
                    .Brushes
                    .Installed
                    .iter()
                    .map(|Pack| (Pack.Name.clone(), Pack.Author.clone(), Pack.Brushes.iter().map(|Entry| Entry.Name.clone()).collect()))
                    .collect();
                let mut Uninstall = None;
                for (Index, (Name, Author, Brushes)) in Packs.iter().enumerate() {
                    egui::Frame::group(Ui.style()).show(Ui, |Ui| {
                        Ui.horizontal(|Ui| {
                            Ui.vertical(|Ui| {
                                Ui.label(RichText::new(Name).strong());
                                if !Author.is_empty() {
                                    Ui.label(RichText::new(format!("by {Author}")).weak().small());
                                }
                                Ui.label(RichText::new(Brushes.join(", ")).small());
                            });
                            Ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |Ui| {
                                if crate::icons::text_button(Ui, "Delete", "Uninstall").clicked() {
                                    Uninstall = Some(Index);
                                }
                            });
                        });
                    });
                }
                if let Some(Index) = Uninstall {
                    if let Some(Name) = App.Brushes.uninstall(Index) {
                        App.set_status(format!("Uninstalled {Name}"));
                    }
                }
                Ui.separator();
                Ui.label(RichText::new("My saved presets").strong());
                if App.Brushes.User.is_empty() {
                    Ui.label(RichText::new("None saved. Use Save preset in the Brush panel.").weak());
                }
                let mut DeleteUser = None;
                for (Index, Entry) in App.Brushes.User.iter().enumerate() {
                    Ui.horizontal(|Ui| {
                        Ui.label(&Entry.Name);
                        Ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |Ui| {
                            if crate::icons::text_button(Ui, "Delete", "Delete").clicked() {
                                DeleteUser = Some(Index);
                            }
                        });
                    });
                }
                if let Some(Index) = DeleteUser {
                    App.Brushes.User.remove(Index);
                    App.Brushes.save_user();
                }
                Ui.separator();
                Ui.horizontal(|Ui| {
                    if crate::icons::text_button(Ui, "Import", "Install brushes…").clicked() {
                        Next = Some(Dialog::InstallBrush);
                        Keep = false;
                    }
                    if Ui.button("Close").clicked() {
                        Keep = false;
                    }
                });
            });
            if Keep {
                Next = Some(Dialog::ManageBrushes);
            }
        }
        Dialog::Message(Title, Body) => {
            modal_window(&Title, Context, &mut Open, Compact).show(Context, |Ui| {
                egui::ScrollArea::vertical().max_height(420.0).show(Ui, |Ui| {
                    Ui.label(&Body);
                });
                if Ui.button("OK").clicked() {
                    Keep = false;
                }
            });
            if Keep {
                Next = Some(Dialog::Message(Title, Body));
            }
        }
    }
    if !Open {
        Next = None;
    }
    if App.Dialog.is_none() {
        App.Dialog = Next;
    }
}

fn write_brush_pack(App: &DespiteApp, Pack: &despite_core::brush::BrushPack) -> Result<Option<PathBuf>, String> {
    let Bytes = Pack.to_bytes().map_err(|Problem| Problem.to_string())?;
    #[cfg(feature = "desktop")]
    {
        let _ = App;
        let Some(Path) = platform::dialogs::save_file("Despite brushes", despite_core::brush::PackExtension, &Pack.file_stem()) else { return Ok(None) };
        std::fs::write(&Path, Bytes).map_err(|Problem| Problem.to_string())?;
        return Ok(Some(Path));
    }
    #[allow(unreachable_code)]
    {
        let Folder = App.Settings.export_folder();
        platform::ensure_dir(&Folder);
        let Path = Folder.join(format!("{}.{}", Pack.file_stem(), despite_core::brush::PackExtension));
        std::fs::write(&Path, Bytes).map_err(|Problem| Problem.to_string())?;
        Ok(Some(Path))
    }
}

fn apply_rename(App: &mut DespiteApp, State: &RenameState) {
    let Text = State.Text.trim().to_string();
    if Text.is_empty() {
        return;
    }
    match &State.Target {
        RenameTarget::Document(DocumentRef::Image(Id)) => {
            App.record("Rename");
            if let Some(Found) = App.Project.image_mut(*Id) {
                Found.Name = Text;
            }
        }
        RenameTarget::Document(DocumentRef::Scene(Id)) => {
            App.record("Rename");
            if let Some(Found) = App.Project.scene_mut(*Id) {
                Found.Name = Text;
            }
        }
        RenameTarget::Layer(Id, Index) => {
            App.record("Rename layer");
            if let Some(Entry) = App.Project.image_mut(*Id).and_then(|Found| Found.Layers.get_mut(*Index)) {
                Entry.Name = Text;
            }
        }
        RenameTarget::Project => {
            App.Project.Name = Text;
        }
        RenameTarget::BrushPreset(Preset) => {
            let mut Saved = (**Preset).clone();
            Saved.Name = Text;
            App.Brushes.User.retain(|Existing| Existing.Name != Saved.Name);
            App.Brushes.User.push(Saved);
            App.Brushes.save_user();
            App.set_status("Brush preset saved");
        }
    }
    App.Modified = true;
    App.Generation += 1;
}

pub fn resample_combo(Ui: &mut Ui, Id: &str, Value: &mut ResampleMethod) {
    Ui.horizontal(|Ui| {
        Ui.label("Method");
        egui::ComboBox::from_id_salt(Id).selected_text(Value.name()).width(240.0).show_ui(Ui, |Ui| {
            for Method in ResampleMethod::All {
                Ui.selectable_value(Value, *Method, Method.name());
            }
        });
    });
}

fn filter_settings(Ui: &mut Ui, Chosen: &mut Filter) -> bool {
    let mut Changed = false;
    fn slide(Ui: &mut Ui, Value: &mut f32, Range: std::ops::RangeInclusive<f32>, Label: &str) -> bool {
        Ui.add(egui::Slider::new(Value, Range).text(Label)).changed()
    }
    match Chosen {
        Filter::Grayscale | Filter::Invert | Filter::Sepia => {}
        Filter::BrightnessContrast { Brightness, Contrast } => {
            Changed |= slide(Ui, Brightness, -1.0..=1.0, "Brightness");
            Changed |= slide(Ui, Contrast, -1.0..=1.0, "Contrast");
        }
        Filter::HueSaturation { Hue, Saturation, Lightness } => {
            Changed |= slide(Ui, Hue, -180.0..=180.0, "Hue");
            Changed |= slide(Ui, Saturation, -1.0..=1.0, "Saturation");
            Changed |= slide(Ui, Lightness, -1.0..=1.0, "Lightness");
        }
        Filter::GaussianBlur { Radius } => Changed |= slide(Ui, Radius, 0.0..=64.0, "Radius"),
        Filter::BoxBlur { Radius } => Changed |= Ui.add(egui::Slider::new(Radius, 0..=32).text("Radius")).changed(),
        Filter::MotionBlur { Angle, Distance } => {
            Changed |= slide(Ui, Angle, -180.0..=180.0, "Angle");
            Changed |= Ui.add(egui::Slider::new(Distance, 1..=100).text("Distance")).changed();
        }
        Filter::Sharpen { Amount, Radius } => {
            Changed |= slide(Ui, Amount, 0.0..=4.0, "Amount");
            Changed |= slide(Ui, Radius, 0.3..=10.0, "Radius");
        }
        Filter::Posterize { Levels } => Changed |= Ui.add(egui::Slider::new(Levels, 2..=32).text("Levels")).changed(),
        Filter::Threshold { Level } => Changed |= slide(Ui, Level, 0.0..=1.0, "Level"),
        Filter::Pixelate { Size } => Changed |= Ui.add(egui::Slider::new(Size, 1..=64).text("Block size")).changed(),
        Filter::Noise { Amount, Monochrome, Seed } => {
            Changed |= slide(Ui, Amount, 0.0..=1.0, "Amount");
            Changed |= Ui.checkbox(Monochrome, "Monochrome").changed();
            Changed |= Ui.add(egui::DragValue::new(Seed).prefix("seed ")).changed();
        }
        Filter::Outline { Paint, Thickness, Outside, Diagonal } => {
            Ui.horizontal(|Ui| {
                Ui.label("Color");
                Changed |= color_button(Ui, Paint);
            });
            Changed |= Ui.add(egui::Slider::new(Thickness, 1..=16).text("Thickness")).changed();
            Changed |= Ui.checkbox(Outside, "Outside (otherwise inside edge)").changed();
            Changed |= Ui.checkbox(Diagonal, "Include diagonals").changed();
        }
        Filter::PaletteQuantize { Dither, ErrorDiffusion } => {
            Ui.label("Uses the project palette (Color tab).");
            egui::ComboBox::from_id_salt("QuantizeDither").selected_text(Dither.name()).show_ui(Ui, |Ui| {
                for Pattern in DitherPattern::All {
                    Changed |= Ui.selectable_value(Dither, *Pattern, Pattern.name()).changed();
                }
            });
            Changed |= Ui.checkbox(ErrorDiffusion, "Floyd–Steinberg error diffusion").changed();
        }
        Filter::ReplaceColor { From, To, Tolerance } => {
            Ui.horizontal(|Ui| {
                Ui.label("From");
                Changed |= color_button(Ui, From);
                Ui.label("To");
                Changed |= color_button(Ui, To);
            });
            Changed |= slide(Ui, Tolerance, 0.0..=1.0, "Tolerance");
        }
        Filter::RemoveColor { Target, Tolerance } => {
            Ui.horizontal(|Ui| {
                Ui.label("Color");
                Changed |= color_button(Ui, Target);
            });
            Changed |= slide(Ui, Tolerance, 0.0..=1.0, "Tolerance");
        }
        Filter::AlphaThreshold { Level } => Changed |= slide(Ui, Level, 0.0..=1.0, "Cutoff"),
        Filter::DropShadow { OffsetX, OffsetY, Blur, Paint } => {
            Changed |= Ui.add(egui::Slider::new(OffsetX, -64..=64).text("Offset X")).changed();
            Changed |= Ui.add(egui::Slider::new(OffsetY, -64..=64).text("Offset Y")).changed();
            Changed |= slide(Ui, Blur, 0.0..=32.0, "Blur");
            Ui.horizontal(|Ui| {
                Ui.label("Color");
                Changed |= color_button(Ui, Paint);
            });
        }
        Filter::Vignette { Strength, Radius } => {
            Changed |= slide(Ui, Strength, 0.0..=1.0, "Strength");
            Changed |= slide(Ui, Radius, 0.0..=1.0, "Radius");
        }
    }
    Changed
}

fn filter_preview(App: &mut DespiteApp, State: &mut FilterState) {
    let Some(Id) = App.active_image_id() else { return };
    let Some(Entry) = App.Project.image(Id).and_then(|Found| Found.active_layer()) else { return };
    let LayerRef = Entry.Id;
    if State.Original.is_none() {
        State.Original = Some((Id, LayerRef, Entry.Pixels.clone()));
    }
    let Some((_, _, Original)) = &State.Original else { return };
    let Selection = App.selection();
    let Palette = App.Project.Palette.clone();
    let Result = if State.Preview {
        let Matrix = Entry.matrix();
        let Offset = Entry.Transform.integer_offset();
        let Clip = Selection.as_deref().map(|Found| despite_core::MaskView::new(Found, Matrix, Offset));
        let mut Pixels = (**Original).clone();
        despite_core::filters::apply_filter(&mut Pixels, &State.Chosen, Clip, &Palette);
        Arc::new(Pixels)
    } else {
        Original.clone()
    };
    if let Some(Entry) = App.Project.image_mut(Id).and_then(|Found| Found.layer_mut(LayerRef)) {
        Entry.Pixels = Result;
    }
    App.Generation += 1;
    if let Some(View) = App.active_view() {
        View.FullDirty = true;
    }
}

fn frame_source(App: &DespiteApp, State: &ExportState) -> Option<FrameSource> {
    if crate::animator::is_animation(App) {
        let Clip = crate::animator::clip_index(App);
        return Some(match State.What {
            ExportWhat::CurrentFrame => FrameSource::AnimationFrame { Clip, Frame: App.Anim.Frame as u32 },
            _ => FrameSource::Animation { Clip },
        });
    }
    let Document = App.Project.Active;
    Some(match (State.What, Document) {
        (ExportWhat::AllImages, _) => FrameSource::AllImages,
        (ExportWhat::CurrentDocument, Some(DocumentRef::Image(Id))) => FrameSource::Image(Id),
        (ExportWhat::CurrentDocument, Some(DocumentRef::Scene(Id))) => FrameSource::Scene(Id),
        (ExportWhat::CurrentFrame, Some(DocumentRef::Scene(Id))) => {
            let Frame = App.Views.get(&DocumentRef::Scene(Id)).map(|View| View.Frame).unwrap_or(0);
            FrameSource::SceneRange { Scene: Id, Start: Frame, End: Frame }
        }
        (ExportWhat::ActiveLayer, Some(DocumentRef::Image(Id))) => {
            let LayerRef = App.Project.image(Id).and_then(|Found| Found.active_layer()).map(|Entry| Entry.Id);
            FrameSource::SelectedLayers { Image: Id, Layers: LayerRef.into_iter().collect(), Space: State.Space }
        }
        (ExportWhat::EachLayer, Some(DocumentRef::Image(Id))) => FrameSource::Layers { Image: Id, Space: State.Space },
        (_, Some(DocumentRef::Image(Id))) => FrameSource::Image(Id),
        (_, Some(DocumentRef::Scene(Id))) => FrameSource::Scene(Id),
        (_, None) => return None,
    })
}

fn finish_frames(State: &ExportState, mut Frames: Vec<export::Frame>) -> Vec<export::Frame> {
    export::scale_frames(&mut Frames, &State.Scale);
    if State.Flatten {
        for Entry in &mut Frames {
            Entry.Pixels = export::flatten_onto(&Entry.Pixels, State.FlattenColor);
        }
    }
    Frames
}

fn sources(App: &DespiteApp, State: &ExportState) -> Vec<FrameSource> {
    if crate::animator::is_animation(App) && State.What == ExportWhat::AllImages {
        return (0..App.Project.Rig.Clips.len()).map(|Clip| FrameSource::Animation { Clip }).collect();
    }
    frame_source(App, State).into_iter().collect()
}

#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
fn plain_frames(App: &DespiteApp, State: &ExportState) -> Vec<export::Frame> {
    let Frames = sources(App, State).iter().flat_map(|Source| export::collect_frames(&App.Project, Source)).collect();
    finish_frames(State, Frames)
}

fn gather_frames(App: &mut DespiteApp, State: &ExportState) -> Result<Vec<export::Frame>, String> {
    #[cfg(feature = "desktop")]
    {
        use crate::desktop::export::{CodeSource, document_frames, script_frames};
        let Frames = match State.Code {
            CodeSource::Script(Id) => script_frames(App, Id)?,
            CodeSource::Document => {
                let mut Collected = Vec::new();
                for Source in sources(App, State) {
                    Collected.extend(document_frames(App, &Source)?);
                }
                Collected
            }
        };
        return Ok(finish_frames(State, Frames));
    }
    #[allow(unreachable_code)]
    Ok(plain_frames(App, State))
}

fn choose_output(App: &DespiteApp, State: &ExportState, Folder: bool) -> Option<PathBuf> {
    #[cfg(feature = "desktop")]
    {
        let _ = App;
        if Folder {
            return platform::dialogs::pick_folder();
        }
        return platform::dialogs::save_file(State.Format.name(), State.extension(), &State.FileName);
    }
    #[allow(unreachable_code)]
    {
        let Base = App.Settings.export_folder();
        platform::ensure_dir(&Base);
        if Folder {
            Some(Base.join(export::sanitize_file_name(&State.FileName)))
        } else {
            Some(Base.join(format!("{}.{}", export::sanitize_file_name(&State.FileName), State.extension())))
        }
    }
}

fn run_export(App: &mut DespiteApp, State: &mut ExportState) -> Result<String, String> {
    let Frames = gather_frames(App, State)?;
    if Frames.is_empty() {
        return Err("Nothing to export".into());
    }
    let MultiPng = State.Format == ExportFormat::Png && Frames.len() > 1;
    let Folder = matches!(State.Format, ExportFormat::PngSequence) || MultiPng;
    let Some(Target) = choose_output(App, State, Folder) else { return Err("Cancelled".into()) };
    match State.Format {
        ExportFormat::Png if Frames.len() == 1 => {
            Frames[0].Pixels.save_png(&Target).map_err(|Problem| Problem.to_string())?;
            Ok(format!("Saved {}", Target.display()))
        }
        ExportFormat::Png | ExportFormat::PngSequence => {
            let Written = export::write_png_sequence(&Frames, &Target, &State.FileName).map_err(|Problem| Problem.to_string())?;
            Ok(format!("Saved {} images to {}", Written.len(), Target.display()))
        }
        ExportFormat::SpriteSheet => {
            let Sheet = export::build_sprite_sheet(&Frames, &State.Sheet);
            Sheet.Pixels.save_png(&Target).map_err(|Problem| Problem.to_string())?;
            if State.WriteJson {
                let ImageName = Target.file_name().and_then(|Name| Name.to_str()).unwrap_or("sheet.png").to_string();
                std::fs::write(Target.with_extension("json"), export::sprite_sheet_json(&Sheet, &ImageName)).map_err(|Problem| Problem.to_string())?;
            }
            Ok(format!("Saved {}×{} sprite sheet with {} frames to {}", Sheet.Pixels.width(), Sheet.Pixels.height(), Sheet.Cells.len(), Target.display()))
        }
        ExportFormat::Gif => {
            let Bytes = export::encode_gif(&Frames, &State.Gif).map_err(|Problem| Problem.to_string())?;
            std::fs::write(&Target, Bytes).map_err(|Problem| Problem.to_string())?;
            Ok(format!("Saved {} frame GIF to {}", Frames.len(), Target.display()))
        }
        ExportFormat::Video => {
            #[cfg(feature = "desktop")]
            {
                crate::desktop::start_video_export(App, Frames, Target.clone(), State.Video.clone());
                return Ok(format!("Encoding video to {} in the background…", Target.display()));
            }
            #[allow(unreachable_code)]
            Err("Video export needs the desktop version (ffmpeg). Export a PNG sequence or GIF instead.".into())
        }
    }
}

fn export_ui(App: &mut DespiteApp, Ui: &mut Ui, State: &mut ExportState, Keep: &mut bool) {
    let Animation = crate::animator::is_animation(App);
    let IsScene = App.active_scene_id().is_some();
    egui::Grid::new("ExportGrid").num_columns(2).show(Ui, |Ui| {
        Ui.label("Export");
        egui::ComboBox::from_id_salt("ExportWhat").selected_text(State.What.label(Animation)).width(280.0).show_ui(Ui, |Ui| {
            Ui.selectable_value(&mut State.What, ExportWhat::CurrentDocument, ExportWhat::CurrentDocument.label(Animation));
            if Animation {
                Ui.selectable_value(&mut State.What, ExportWhat::CurrentFrame, ExportWhat::CurrentFrame.label(true));
                Ui.selectable_value(&mut State.What, ExportWhat::AllImages, ExportWhat::AllImages.label(true));
                return;
            }
            if IsScene {
                Ui.selectable_value(&mut State.What, ExportWhat::CurrentFrame, ExportWhat::CurrentFrame.name());
            } else {
                Ui.selectable_value(&mut State.What, ExportWhat::ActiveLayer, ExportWhat::ActiveLayer.name());
                Ui.selectable_value(&mut State.What, ExportWhat::EachLayer, ExportWhat::EachLayer.name());
            }
            Ui.selectable_value(&mut State.What, ExportWhat::AllImages, ExportWhat::AllImages.name());
        });
        Ui.end_row();
        Ui.label("Format");
        egui::ComboBox::from_id_salt("ExportFormat").selected_text(State.Format.name()).width(280.0).show_ui(Ui, |Ui| {
            for Format in [ExportFormat::Png, ExportFormat::SpriteSheet, ExportFormat::Gif, ExportFormat::PngSequence] {
                Ui.selectable_value(&mut State.Format, Format, Format.name());
            }
            if cfg!(feature = "desktop") {
                Ui.selectable_value(&mut State.Format, ExportFormat::Video, ExportFormat::Video.name());
            }
        });
        Ui.end_row();
        if matches!(State.What, ExportWhat::ActiveLayer | ExportWhat::EachLayer) {
            Ui.label("Layer area");
            egui::ComboBox::from_id_salt("LayerSpace").selected_text(State.Space.name()).show_ui(Ui, |Ui| {
                for Space in LayerSpace::All {
                    Ui.selectable_value(&mut State.Space, *Space, Space.name());
                }
            });
            Ui.end_row();
        }
        Ui.label("File name");
        Ui.text_edit_singleline(&mut State.FileName);
        Ui.end_row();
    });
    Ui.separator();
    Ui.label(RichText::new("Upscale / resize").strong());
    Ui.horizontal_wrapped(|Ui| {
        for Factor in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 16.0] {
            if Ui.selectable_label((State.Scale.Factor - Factor).abs() < 1e-4 && State.Scale.TargetWidth == 0, format!("{Factor}×")).clicked() {
                State.Scale.Factor = Factor;
                State.Scale.TargetWidth = 0;
                State.Scale.TargetHeight = 0;
            }
        }
        Ui.add(egui::DragValue::new(&mut State.Scale.Factor).range(0.05..=64.0).speed(0.05).suffix("×"));
    });
    Ui.horizontal(|Ui| {
        Ui.label("Or exact size");
        Ui.add(egui::DragValue::new(&mut State.Scale.TargetWidth).range(0..=16384).prefix("w "));
        Ui.add(egui::DragValue::new(&mut State.Scale.TargetHeight).range(0..=16384).prefix("h "));
        Ui.label(RichText::new("(0 = auto)").weak());
    });
    resample_combo(Ui, "ExportMethod", &mut State.Scale.Method);
    Ui.horizontal(|Ui| {
        Ui.checkbox(&mut State.Flatten, "Flatten onto background");
        if State.Flatten {
            color_button(Ui, &mut State.FlattenColor);
        }
    });
    #[cfg(feature = "desktop")]
    crate::desktop::export::export_section(App, Ui, &mut State.Code, State.What == ExportWhat::AllImages);
    match State.Format {
        ExportFormat::SpriteSheet => {
            Ui.separator();
            Ui.label(RichText::new("Sprite sheet").strong());
            egui::ComboBox::from_id_salt("SheetLayout").selected_text(State.Sheet.Layout.name()).show_ui(Ui, |Ui| {
                for Layout in SheetLayout::All {
                    Ui.selectable_value(&mut State.Sheet.Layout, *Layout, Layout.name());
                }
            });
            Ui.horizontal(|Ui| {
                if State.Sheet.Layout == SheetLayout::Grid {
                    Ui.add(egui::DragValue::new(&mut State.Sheet.Columns).range(0..=256).prefix("columns "));
                }
                Ui.add(egui::DragValue::new(&mut State.Sheet.Padding).range(0..=64).prefix("padding "));
                Ui.add(egui::DragValue::new(&mut State.Sheet.Margin).range(0..=64).prefix("margin "));
            });
            Ui.horizontal(|Ui| {
                Ui.checkbox(&mut State.Sheet.PowerOfTwo, "Power-of-two size");
                Ui.checkbox(&mut State.WriteJson, "Write frame JSON (engine friendly)");
            });
        }
        ExportFormat::Gif => {
            Ui.separator();
            Ui.label(RichText::new("GIF").strong());
            Ui.horizontal(|Ui| {
                Ui.checkbox(&mut State.Gif.Loop, "Loop forever");
                Ui.add(egui::Slider::new(&mut State.Gif.AlphaThreshold, 1..=255).text("alpha cutoff"));
            });
            Ui.add(egui::Slider::new(&mut State.Gif.Speed, 1..=30).text("quantize speed (1 = best)"));
        }
        ExportFormat::Video => {
            #[cfg(feature = "desktop")]
            {
                Ui.separator();
                Ui.label(RichText::new("Video").strong());
                egui::ComboBox::from_id_salt("VideoFormat").selected_text(State.Video.Format.name()).show_ui(Ui, |Ui| {
                    for Format in despite_video::VideoFormat::All {
                        Ui.selectable_value(&mut State.Video.Format, *Format, Format.name());
                    }
                });
                Ui.horizontal(|Ui| {
                    Ui.add(egui::DragValue::new(&mut State.Video.Fps).range(1.0..=120.0).prefix("fps "));
                    if Ui.small_button("From frames").clicked() {
                        let Frames = plain_frames(App, State);
                        State.Video.Fps = despite_video::fps_from_frames(&Frames);
                    }
                    Ui.add(egui::Slider::new(&mut State.Video.Quality, 0..=51).text("CRF (lower = better)"));
                });
                if !State.Video.Format.supports_alpha() {
                    Ui.horizontal(|Ui| {
                        Ui.label("Background");
                        color_button(Ui, &mut State.Video.Background);
                    });
                }
                Ui.checkbox(&mut State.Video.Loop, "Loop (GIF / WebP)");
                if despite_video::find_ffmpeg(State.Video.Ffmpeg.as_deref()).is_none() {
                    Ui.colored_label(egui::Color32::from_rgb(230, 120, 80), "ffmpeg not found. Install ffmpeg or set its path in Settings → Video.");
                }
            }
        }
        _ => {}
    }
    Ui.separator();
    if let Some(Message) = &State.Result {
        Ui.label(Message);
    }
    if !cfg!(feature = "desktop") {
        Ui.label(RichText::new(format!("Files are written to {}", App.Settings.export_folder().display())).weak());
    }
    Ui.horizontal(|Ui| {
        if Ui.button(RichText::new("Export").strong()).clicked() {
            App.Settings.ExportScale = State.Scale.Factor;
            App.Settings.ExportMethod = State.Scale.Method;
            App.Settings.Gif = State.Gif;
            App.Settings.Sheet = State.Sheet.clone();
            App.Settings.WriteSheetJson = State.WriteJson;
            match run_export(App, State) {
                Ok(Message) => {
                    App.set_status(Message.clone());
                    State.Result = Some(Message);
                }
                Err(Problem) if Problem == "Cancelled" => {}
                Err(Problem) => State.Result = Some(format!("Export failed: {Problem}")),
            }
        }
        if Ui.button("Close").clicked() {
            *Keep = false;
        }
    });
}

fn file_browser_ui(App: &mut DespiteApp, Ui: &mut Ui, State: &mut FileBrowserState, Keep: &mut bool) {
    let Documents = platform::documents_dir();
    platform::ensure_dir(&Documents);
    Ui.label(RichText::new(format!("Folder: {}", Documents.display())).weak());
    match State.Mode {
        BrowserMode::Save => {
            Ui.horizontal(|Ui| {
                Ui.label("Name");
                Ui.text_edit_singleline(&mut State.FileName);
            });
            let Path = Documents.join(format!("{}.despite", export::sanitize_file_name(&State.FileName)));
            if Path.exists() {
                Ui.colored_label(egui::Color32::from_rgb(230, 180, 80), "A project with this name exists and will be replaced.");
            }
            Ui.horizontal(|Ui| {
                if Ui.button("Save").clicked() {
                    App.save_to(&Path);
                    *Keep = false;
                }
                if Ui.button("Cancel").clicked() {
                    *Keep = false;
                }
            });
        }
        BrowserMode::Open => {
            Ui.checkbox(&mut State.ShowAutosaves, "Show autosave recovery files");
            let Folder = if State.ShowAutosaves { platform::autosave_dir() } else { Documents.clone() };
            let Files = platform::list_files(&Folder, &["despite"]);
            if Files.is_empty() {
                Ui.label("No projects here yet.");
            }
            egui::ScrollArea::vertical().max_height(360.0).show(Ui, |Ui| {
                for Path in Files {
                    let Name = Path.file_stem().and_then(|Stem| Stem.to_str()).unwrap_or("?").to_string();
                    Ui.horizontal(|Ui| {
                        if Ui.button(&Name).clicked() {
                            App.open_path(&Path);
                            *Keep = false;
                        }
                        if crate::icons::button(Ui, "Delete", false, "Delete").clicked() {
                            let _ = std::fs::remove_file(&Path);
                        }
                    });
                }
            });
            if Ui.button("Cancel").clicked() {
                *Keep = false;
            }
        }
        BrowserMode::Import { AsLayers } => {
            let Imports = platform::imports_dir();
            platform::ensure_dir(&Imports);
            Ui.label(format!("Copy images into {} (or the Documents folder) to import them.", Imports.display()));
            let mut Files = platform::list_files(&Imports, platform::ImageExtensions);
            Files.extend(platform::list_files(&Documents, platform::ImageExtensions));
            egui::ScrollArea::vertical().max_height(360.0).show(Ui, |Ui| {
                for Path in Files {
                    let Name = Path.file_name().and_then(|Name| Name.to_str()).unwrap_or("?").to_string();
                    if Ui.button(Name).clicked() {
                        App.import_images(std::slice::from_ref(&Path), AsLayers);
                        *Keep = false;
                    }
                }
            });
            if Ui.button("Cancel").clicked() {
                *Keep = false;
            }
        }
    }
}

fn section(Ui: &mut Ui, Title: &str, Open: bool, Content: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(RichText::new(Title).strong()).default_open(Open).show(Ui, Content);
}

pub fn settings_window(App: &mut DespiteApp, Context: &Context) {
    let mut Open = App.ShowSettings;
    let Screen = Context.content_rect();
    let mut Window = egui::Window::new("Settings").open(&mut Open).default_width(520.0).default_height(640.0);
    if App.Compact {
        Window = Window.fixed_rect(Screen.shrink(8.0)).collapsible(false);
    } else {
        Window = Window.max_height(Screen.height() - 24.0);
    }
    Window.show(Context, |Ui| {
        egui::ScrollArea::vertical().show(Ui, |Ui| {
            let Settings = &mut App.Settings;
            section(Ui, "Interface", true, |Ui| {
                egui::Grid::new("SettingsInterface").num_columns(2).show(Ui, |Ui| {
                    Ui.label("Layout");
                    Ui.horizontal(|Ui| {
                        Ui.selectable_value(&mut Settings.Layout, LayoutMode::Auto, "Auto");
                        Ui.selectable_value(&mut Settings.Layout, LayoutMode::Desktop, "Desktop");
                        Ui.selectable_value(&mut Settings.Layout, LayoutMode::Touch, "Touch");
                    });
                    Ui.end_row();
                    Ui.label("Theme");
                    Ui.horizontal(|Ui| {
                        Ui.selectable_value(&mut Settings.Theme, ThemeChoice::Dark, "Dark");
                        Ui.selectable_value(&mut Settings.Theme, ThemeChoice::Light, "Light");
                        Ui.selectable_value(&mut Settings.Theme, ThemeChoice::System, "System");
                    });
                    Ui.end_row();
                    Ui.label("UI scale");
                    Ui.add(egui::Slider::new(&mut Settings.UiScale, 0.6..=2.5));
                    Ui.end_row();
                    Ui.label("Accent hue");
                    Ui.add(egui::Slider::new(&mut Settings.AccentHue, 0.0..=360.0));
                    Ui.end_row();
                    Ui.label("Recent files kept");
                    Ui.add(egui::DragValue::new(&mut Settings.RecentLimit).range(1..=50));
                    Ui.end_row();
                });
                Ui.checkbox(&mut Settings.ConfirmUnsaved, "Ask before discarding unsaved changes");
            });
            section(Ui, "Canvas", false, |Ui| {
                egui::Grid::new("SettingsCanvas").num_columns(2).show(Ui, |Ui| {
                    Ui.label("Background");
                    color_button(Ui, &mut Settings.CanvasBackground);
                    Ui.end_row();
                    Ui.label("Checker size");
                    Ui.add(egui::DragValue::new(&mut Settings.CheckerSize).range(2..=64));
                    Ui.end_row();
                    Ui.label("Checker colors");
                    Ui.horizontal(|Ui| {
                        color_button(Ui, &mut Settings.CheckerLight);
                        color_button(Ui, &mut Settings.CheckerDark);
                    });
                    Ui.end_row();
                    Ui.label("Pixel grid from zoom");
                    Ui.add(egui::Slider::new(&mut Settings.PixelGridMinZoom, 2.0..=32.0).suffix("×"));
                    Ui.end_row();
                    Ui.label("Pixel grid color");
                    color_button(Ui, &mut Settings.PixelGridColor);
                    Ui.end_row();
                    Ui.label("Tile grid");
                    Ui.horizontal(|Ui| {
                        Ui.add(egui::DragValue::new(&mut Settings.TileGridWidth).range(1..=1024));
                        Ui.add(egui::DragValue::new(&mut Settings.TileGridHeight).range(1..=1024));
                        color_button(Ui, &mut Settings.TileGridColor);
                    });
                    Ui.end_row();
                    Ui.label("Scroll wheel");
                    Ui.horizontal(|Ui| {
                        Ui.selectable_value(&mut Settings.ScrollWheel, ScrollAction::Zoom, "Zooms");
                        Ui.selectable_value(&mut Settings.ScrollWheel, ScrollAction::Pan, "Pans");
                    });
                    Ui.end_row();
                    Ui.label("Zoom step");
                    Ui.add(egui::Slider::new(&mut Settings.ZoomStep, 1.05..=2.0));
                    Ui.end_row();
                    Ui.label("Marching ants speed");
                    Ui.add(egui::Slider::new(&mut Settings.MarchingAntsSpeed, 0.0..=60.0));
                    Ui.end_row();
                });
                Ui.checkbox(&mut Settings.PixelGrid, "Show pixel grid");
                Ui.checkbox(&mut Settings.TileGrid, "Show tile grid");
                Ui.checkbox(&mut Settings.TilePreview, "Tile preview (seamless check)");
                Ui.checkbox(&mut Settings.SmoothZoomedOut, "Smooth canvas when zoomed out");
                Ui.checkbox(&mut Settings.InvertScroll, "Invert scroll direction");
                Ui.checkbox(&mut Settings.SmoothZoom, "Smooth zoom (otherwise zoom snaps to crisp pixel-perfect levels)");
                Ui.checkbox(&mut Settings.ShowBrushOutline, "Show brush outline");
                Ui.checkbox(&mut Settings.ShowCrosshair, "Always show crosshair");
                Ui.checkbox(&mut Settings.CanvasBorder, "Canvas border");
            });
            section(Ui, "Painting & input", false, |Ui| {
                Ui.checkbox(&mut Settings.UsePressure, "Use pen / touch pressure");
                Ui.checkbox(&mut Settings.MouseSpeedPressure, "Simulate pressure from mouse speed (fast = lighter)");
                Ui.add(egui::Slider::new(&mut Settings.PressureCurve, 0.25..=4.0).text("global pressure curve"));
                Ui.checkbox(&mut Settings.AltClickPicksColor, "Alt-click picks color with painting tools");
                Ui.checkbox(&mut Settings.RightClickSecondary, "Right mouse paints with secondary color");
                Ui.checkbox(&mut Settings.TwoFingerUndo, "Two-finger tap undo, three-finger tap redo");
                Ui.add(egui::Slider::new(&mut Settings.ShapeSnapDegrees, 1.0..=90.0).text("line angle snap (Shift)"));
            });
            section(Ui, "History & saving", true, |Ui| {
                Ui.add(egui::Slider::new(&mut Settings.UndoLimit, 5..=1000).text("undo steps kept"));
                Ui.checkbox(&mut Settings.SaveHistoryInProject, "Save undo history inside project files");
                Ui.label(
                    RichText::new("Warning: saved history can make project files much larger and slower to open, because every undo step's pixels are stored (unchanged layers are shared).")
                        .color(egui::Color32::from_rgb(230, 170, 80)),
                );
                Ui.checkbox(&mut Settings.LoadHistoryFromProject, "Load saved history when opening projects");
                Ui.add(egui::Slider::new(&mut Settings.AutosaveMinutes, 0..=60).text("autosave every N minutes (0 = off)"));
                Ui.label(RichText::new(format!("Autosave copies go to {}", platform::autosave_dir().display())).weak().small());
            });
            section(Ui, "New documents", false, |Ui| {
                Ui.horizontal(|Ui| {
                    Ui.label("Default size");
                    Ui.add(egui::DragValue::new(&mut Settings.DefaultWidth).range(1..=16384));
                    Ui.add(egui::DragValue::new(&mut Settings.DefaultHeight).range(1..=16384));
                });
                Ui.checkbox(&mut Settings.DefaultPixelArt, "Pixel art by default");
                Ui.horizontal(|Ui| {
                    let mut HasBackground = Settings.DefaultBackground.is_some();
                    if Ui.checkbox(&mut HasBackground, "Fill background").changed() {
                        Settings.DefaultBackground = if HasBackground { Some(Color::White) } else { None };
                    }
                    if let Some(Fill) = &mut Settings.DefaultBackground {
                        color_button(Ui, Fill);
                    }
                });
            });
            section(Ui, "Export", false, |Ui| {
                Ui.add(egui::Slider::new(&mut Settings.ExportScale, 0.1..=16.0).text("default scale"));
                resample_combo(Ui, "SettingsExportMethod", &mut Settings.ExportMethod);
                Ui.checkbox(&mut Settings.Gif.Loop, "GIFs loop forever");
                Ui.checkbox(&mut Settings.WriteSheetJson, "Write sprite sheet JSON");
                Ui.add(egui::DragValue::new(&mut Settings.Sheet.Padding).range(0..=64).prefix("sheet padding "));
                Ui.label(format!("Mobile export folder: {}", Settings.export_folder().display()));
            });
            #[cfg(feature = "desktop")]
            crate::desktop::settings_sections(Ui, Settings);
            section(Ui, "Folders", false, |Ui| {
                Ui.label(format!("Settings: {}", platform::settings_path().display()));
                Ui.label(format!("Data: {}", platform::data_dir().display()));
                Ui.label(format!("Documents: {}", platform::documents_dir().display()));
            });
            section(Ui, "Keyboard shortcuts", false, |Ui| {
                shortcuts_ui(Ui);
            });
            Ui.separator();
            if Ui.button("Reset all settings to defaults").clicked() {
                *Settings = Settings::default();
            }
        });
    });
    App.ShowSettings = Open;
}
