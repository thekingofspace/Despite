use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use despite_core::brush::{self, Brush, Symmetry};
use despite_core::filters::Filter;
use despite_core::io::{LoadOptions, SaveOptions};
use despite_core::{
    BlendMode, Color, DocumentRef, History, IRect, ImageId, Layer, LayerTransform, Mask, MaskView, Project, ProjectKind, Raster, SceneId,
};
use egui::{Context, Key, KeyboardShortcut, Modifiers};

use crate::canvas::Interaction;
use crate::dialogs::{self, Dialog, PendingAction};
use crate::panels::{self, RightTab};
use crate::platform;
use crate::settings::{LayoutMode, Settings, ThemeChoice};
use crate::tools::{Tool, ToolOptions};
use crate::view::DocumentView;

#[derive(Clone, Debug)]
pub struct InstalledPack {
    pub Name: String,
    pub Author: String,
    pub Path: PathBuf,
    pub Brushes: Vec<Brush>,
}

pub struct BrushLibrary {
    pub Installed: Vec<InstalledPack>,
    pub Builtin: Vec<Brush>,
    pub Advanced: Vec<Brush>,
    pub User: Vec<Brush>,
    pub Plugin: Vec<Brush>,
    pub BrushTool: Brush,
    pub PencilTool: Brush,
    pub EraserTool: Brush,
}

impl BrushLibrary {
    fn new() -> Self {
        let Builtin = brush::default_presets();
        let Find = |Name: &str| Builtin.iter().find(|Entry| Entry.Name == Name).cloned().unwrap_or_default();
        let User = std::fs::read_to_string(platform::brushes_path())
            .ok()
            .and_then(|Text| serde_json::from_str::<Vec<Brush>>(&Text).ok())
            .unwrap_or_default();
        let mut Library = Self {
            Installed: Vec::new(),
            BrushTool: Find("Round"),
            PencilTool: Find("Pixel Pencil"),
            EraserTool: Find("Eraser"),
            Advanced: brush::advanced_presets(),
            Builtin,
            User,
            Plugin: Vec::new(),
        };
        Library.load_installed();
        Library
    }

    pub fn load_installed(&mut self) {
        self.Installed = platform::list_files(&platform::brush_packs_dir(), &[brush::PackExtension])
            .into_iter()
            .filter_map(|Path| {
                let Bytes = std::fs::read(&Path).ok()?;
                let Pack = brush::BrushPack::from_bytes(&Bytes).ok()?;
                Some(InstalledPack { Name: Pack.Name, Author: Pack.Author, Path, Brushes: Pack.Brushes })
            })
            .collect();
        self.Installed.sort_by_key(|Pack| Pack.Name.to_lowercase());
    }

    pub fn install(&mut self, Bytes: &[u8]) -> Result<String, String> {
        let Pack = brush::BrushPack::from_bytes(Bytes).map_err(|Problem| Problem.to_string())?;
        let Folder = platform::brush_packs_dir();
        platform::ensure_dir(&Folder);
        let Path = Folder.join(format!("{}.{}", Pack.file_stem(), brush::PackExtension));
        std::fs::write(&Path, Pack.to_bytes().map_err(|Problem| Problem.to_string())?).map_err(|Problem| Problem.to_string())?;
        let Message = format!("Installed \"{}\" ({} brush{})", Pack.Name, Pack.Brushes.len(), if Pack.Brushes.len() == 1 { "" } else { "es" });
        self.load_installed();
        Ok(Message)
    }

    pub fn uninstall(&mut self, Index: usize) -> Option<String> {
        let Pack = self.Installed.get(Index)?.clone();
        let _ = std::fs::remove_file(&Pack.Path);
        self.load_installed();
        Some(Pack.Name)
    }

    pub fn for_tool(&self, Active: Tool) -> &Brush {
        match Active {
            Tool::Pencil => &self.PencilTool,
            Tool::Eraser => &self.EraserTool,
            _ => &self.BrushTool,
        }
    }

    pub fn for_tool_mut(&mut self, Active: Tool) -> &mut Brush {
        match Active {
            Tool::Pencil => &mut self.PencilTool,
            Tool::Eraser => &mut self.EraserTool,
            _ => &mut self.BrushTool,
        }
    }

    pub fn save_user(&self) {
        let Path = platform::brushes_path();
        if let Some(Parent) = Path.parent() {
            platform::ensure_dir(Parent);
        }
        if let Ok(Text) = serde_json::to_string_pretty(&self.User) {
            let _ = std::fs::write(Path, Text);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Home,
    Editor,
}

pub struct DespiteApp {
    pub(crate) Screen: Screen,
    pub(crate) Home: crate::home::HomeState,
    pub(crate) HasProject: bool,
    pub(crate) LayerThumbs: HashMap<despite_core::LayerId, (egui::TextureHandle, usize, u64)>,
    pub(crate) Project: Project,
    pub(crate) History: History,
    pub(crate) ProjectPath: Option<PathBuf>,
    pub(crate) Modified: bool,
    pub(crate) Settings: Settings,
    pub(crate) SettingsSnapshot: Settings,
    pub(crate) Tool: Tool,
    pub(crate) PreviousTool: Option<Tool>,
    pub(crate) ToolOptions: ToolOptions,
    pub(crate) Brushes: BrushLibrary,
    pub(crate) Primary: Color,
    pub(crate) Secondary: Color,
    pub(crate) Symmetry: Symmetry,
    pub(crate) Wrap: bool,
    pub(crate) Views: HashMap<DocumentRef, DocumentView>,
    pub(crate) Interaction: Interaction,
    pub(crate) Dialog: Option<Dialog>,
    pub(crate) ShowSettings: bool,
    pub(crate) RightTab: RightTab,
    pub(crate) Status: Option<(String, f64)>,
    pub(crate) Clipboard: Option<crate::clipboard::Clipboard>,
    pub(crate) CheckerTexture: Option<(egui::TextureHandle, [u8; 8], u32)>,
    pub(crate) OverlayTextures: HashMap<(despite_core::OverlaySource, u32), (egui::TextureHandle, u64)>,
    pub(crate) Generation: u64,
    pub(crate) Playing: bool,
    pub(crate) PlayClock: f64,
    pub(crate) ShowLeft: bool,
    pub(crate) ShowRight: bool,
    pub(crate) ShowBottom: bool,
    pub(crate) Compact: bool,
    pub(crate) ForceTouch: bool,
    pub(crate) LastCoalesce: Option<(String, f64)>,
    pub(crate) LastAutosave: f64,
    pub(crate) SettingsDirtySince: Option<f64>,
    pub(crate) Now: f64,
    pub(crate) AllowClose: bool,
    pub(crate) PointerImagePosition: Option<(f32, f32)>,
    pub(crate) PenTilt: Option<[f32; 2]>,
    pub(crate) Floating: Option<crate::transform::FloatingSelection>,
    pub(crate) ContentBounds: HashMap<despite_core::LayerId, (usize, u64, Option<IRect>)>,
    #[cfg(feature = "desktop")]
    pub(crate) Desktop: crate::desktop::DesktopState,
}

impl DespiteApp {
    pub fn new(Creation: &eframe::CreationContext<'_>, Launch: crate::LaunchOptions) -> Self {
        let Settings = Settings::load();
        let mut Project = Project::with_image("Untitled", Settings.DefaultWidth, Settings.DefaultHeight);
        Project.Images[0].PixelArt = Settings.DefaultPixelArt;
        if let Some(Fill) = Settings.DefaultBackground {
            Project.Images[0].Layers[0].pixels_mut().fill(Fill.to_rgba8());
        }
        let mut App = Self {
            Screen: Screen::Home,
            Home: Default::default(),
            HasProject: false,
            LayerThumbs: HashMap::new(),
            History: History::new(Settings.UndoLimit),
            Project,
            ProjectPath: None,
            Modified: false,
            SettingsSnapshot: Settings.clone(),
            Tool: Tool::Pencil,
            PreviousTool: None,
            ToolOptions: ToolOptions::default(),
            Brushes: BrushLibrary::new(),
            Primary: Color::Black,
            Secondary: Color::White,
            Symmetry: Symmetry::default(),
            Wrap: false,
            Views: HashMap::new(),
            Interaction: Interaction::None,
            Dialog: None,
            ShowSettings: false,
            RightTab: RightTab::Layers,
            Status: None,
            Clipboard: None,
            CheckerTexture: None,
            OverlayTextures: HashMap::new(),
            Generation: 1,
            Playing: false,
            PlayClock: 0.0,
            ShowLeft: true,
            ShowRight: true,
            ShowBottom: false,
            Compact: platform::IsMobile || Launch.ForceTouchLayout,
            ForceTouch: Launch.ForceTouchLayout,
            LastCoalesce: None,
            LastAutosave: 0.0,
            SettingsDirtySince: None,
            Now: 0.0,
            AllowClose: false,
            PointerImagePosition: None,
            PenTilt: None,
            Floating: None,
            ContentBounds: HashMap::new(),
            #[cfg(feature = "desktop")]
            Desktop: crate::desktop::DesktopState::new(&Settings),
            Settings,
        };
        App.restore_view_prefs();
        if App.Compact {
            App.ShowLeft = false;
            App.ShowRight = false;
        }
        App.center_symmetry();
        App.apply_style(&Creation.egui_ctx);
        if let Some(Path) = Launch.OpenFile {
            App.open_path(&Path);
        }
        #[cfg(feature = "desktop")]
        crate::desktop::after_start(&mut App, Creation);
        App
    }

    pub(crate) fn apply_style(&self, Context: &Context) {
        Context.set_zoom_factor(self.Settings.UiScale.clamp(0.5, 3.0));
        let Preference = match self.Settings.Theme {
            ThemeChoice::Dark => egui::ThemePreference::Dark,
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::System => egui::ThemePreference::System,
        };
        Context.set_theme(Preference);
        let Accent = crate::view::to_color32(Color::from_hsva(self.Settings.AccentHue, 0.55, 0.85, 1.0));
        for Theme in [egui::Theme::Dark, egui::Theme::Light] {
            Context.style_mut_of(Theme, |Style| {
                Style.visuals.selection.bg_fill = Accent.gamma_multiply(0.7);
                Style.visuals.hyperlink_color = Accent;
                if self.Compact {
                    Style.spacing.interact_size.y = 34.0;
                    Style.spacing.button_padding = egui::vec2(10.0, 7.0);
                    Style.spacing.item_spacing = egui::vec2(8.0, 7.0);
                    Style.spacing.slider_width = 180.0;
                }
            });
        }
    }

    fn restore_view_prefs(&mut self) {
        let Prefs = self.Settings.View.clone();
        self.ShowLeft = Prefs.ShowLeft;
        self.ShowRight = Prefs.ShowRight;
        self.ShowBottom = Prefs.ShowBottom;
        self.RightTab = Prefs.RightTab;
        self.Wrap = Prefs.Wrap;
        self.Symmetry.Mode = Prefs.SymmetryMode;
        self.Symmetry.Segments = Prefs.SymmetrySegments;
        self.Symmetry.ShowGuide = Prefs.SymmetryGuide;
        self.ToolOptions = Prefs.ToolOptions;
        if let Some(Saved) = Prefs.Tool {
            self.Tool = Saved;
        }
        if let Some(Saved) = Prefs.BrushTool {
            self.Brushes.BrushTool = Saved;
        }
        if let Some(Saved) = Prefs.PencilTool {
            self.Brushes.PencilTool = Saved;
        }
        if let Some(Saved) = Prefs.EraserTool {
            self.Brushes.EraserTool = Saved;
        }
        self.Primary = Prefs.Primary;
        self.Secondary = Prefs.Secondary;
    }

    fn capture_view_prefs(&mut self) {
        let View = &mut self.Settings.View;
        if !self.Compact {
            View.ShowLeft = self.ShowLeft;
            View.ShowRight = self.ShowRight;
            View.ShowBottom = self.ShowBottom;
        }
        if self.RightTab != crate::panels::RightTab::Scene {
            View.RightTab = self.RightTab;
        }
        View.Wrap = self.Wrap;
        View.SymmetryMode = self.Symmetry.Mode;
        View.SymmetrySegments = self.Symmetry.Segments;
        View.SymmetryGuide = self.Symmetry.ShowGuide;
        if View.ToolOptions != self.ToolOptions {
            View.ToolOptions = self.ToolOptions.clone();
        }
        View.Tool = Some(self.Tool);
        for (Stored, Current) in [
            (&mut View.BrushTool, &self.Brushes.BrushTool),
            (&mut View.PencilTool, &self.Brushes.PencilTool),
            (&mut View.EraserTool, &self.Brushes.EraserTool),
        ] {
            if Stored.as_ref() != Some(Current) {
                *Stored = Some(Current.clone());
            }
        }
        View.Primary = self.Primary;
        View.Secondary = self.Secondary;
    }

    pub(crate) fn set_status(&mut self, Message: impl Into<String>) {
        self.Status = Some((Message.into(), self.Now));
    }

    pub(crate) fn active_document(&self) -> Option<DocumentRef> {
        self.Project.Active
    }

    pub(crate) fn active_view(&mut self) -> Option<&mut DocumentView> {
        let Document = self.Project.Active?;
        Some(self.Views.entry(Document).or_default())
    }

    pub(crate) fn selection(&self) -> Option<Arc<Mask>> {
        let Document = self.Project.Active?;
        self.Views.get(&Document).and_then(|View| View.Selection.clone())
    }

    pub(crate) fn touch(&mut self) {
        self.Generation += 1;
        self.Modified = true;
        if let Some(Document) = self.Project.Active {
            self.Views.entry(Document).or_default().FullDirty = true;
        }
    }

    pub(crate) fn touch_region(&mut self, Region: IRect) {
        self.Modified = true;
        if let Some(View) = self.active_view() {
            View.mark_dirty(Region);
        }
    }

    pub(crate) fn record(&mut self, Label: &str) {
        self.History.set_limit(self.Settings.UndoLimit);
        self.History.record(Label, &self.Project);
        self.LastCoalesce = None;
        self.Modified = true;
    }

    pub(crate) fn record_coalesced(&mut self, Key: &str, Label: &str) {
        let Fresh = self.LastCoalesce.as_ref().is_none_or(|(LastKey, Time)| LastKey != Key || self.Now - Time > 1.2);
        if Fresh {
            self.record(Label);
        }
        self.LastCoalesce = Some((Key.to_string(), self.Now));
    }

    pub(crate) fn undo(&mut self) {
        self.cancel_interaction();
        match self.History.undo(&mut self.Project) {
            Some(Label) => self.set_status(format!("Undid {Label}")),
            None => self.set_status("Nothing to undo"),
        }
        self.after_history_jump();
    }

    pub(crate) fn redo(&mut self) {
        self.cancel_interaction();
        match self.History.redo(&mut self.Project) {
            Some(Label) => self.set_status(format!("Redid {Label}")),
            None => self.set_status("Nothing to redo"),
        }
        self.after_history_jump();
    }

    fn after_history_jump(&mut self) {
        crate::transform::validate_floating(self);
        self.Generation += 1;
        self.Modified = true;
        for View in self.Views.values_mut() {
            View.FullDirty = true;
        }
        let Valid: Vec<DocumentRef> = self.Views.keys().copied().filter(|Document| self.Project.document_size(*Document).is_some()).collect();
        self.Views.retain(|Document, _| Valid.contains(Document));
        if let Some(Document) = self.Project.Active
            && let (Some(View), Some((Width, Height))) = (self.Views.get_mut(&Document), self.Project.document_size(Document))
                && View.Selection.as_ref().is_some_and(|Found| Found.width() != Width || Found.height() != Height) {
                    View.set_selection(None);
                }
    }

    pub(crate) fn cancel_interaction(&mut self) {
        if let Some(Label) = self.Interaction.history_label() {
            if self.History.undo_label() == Some(Label) {
                self.History.undo(&mut self.Project);
                self.History.Redo.pop();
                self.Generation += 1;
                self.touch();
            }
            self.Interaction = Interaction::None;
            crate::transform::validate_floating(self);
            return;
        }
        if let Some(Restore) = self.Interaction.restore_target() {
            let Recorded = self.History.Undo.last().is_some_and(|Entry| {
                matches!(Entry.Label.as_str(), "Brush stroke" | "Pencil" | "Erase" | "Shape" | "Gradient" | "Move selection" | "Duplicate selection")
            });
            if Recorded {
                self.History.undo(&mut self.Project);
                self.History.Redo.pop();
            } else if let Some(Found) = self.Project.image_mut(Restore.0)
                && let Some(Entry) = Found.layer_mut(Restore.1)
            {
                Entry.Pixels = Restore.2;
            }
            self.Generation += 1;
            self.touch();
        }
        self.Interaction = Interaction::None;
    }

    pub(crate) fn set_tool(&mut self, Next: Tool) {
        if self.Tool != Next {
            self.cancel_interaction();
            crate::transform::commit_floating(self);
            self.PreviousTool = Some(self.Tool);
            self.Tool = Next;
        }
    }

    pub(crate) fn activate(&mut self, Document: DocumentRef) {
        if self.Project.Active != Some(Document) {
            self.cancel_interaction();
            crate::transform::commit_floating(self);
            self.Project.Active = Some(Document);
            self.Playing = false;
            let View = self.Views.entry(Document).or_default();
            if View.SeenGeneration != self.Generation {
                View.FullDirty = true;
            }
            if let DocumentRef::Scene(_) = Document {
                if !self.Tool.works_on_scene() {
                    self.Tool = Tool::Move;
                }
                self.RightTab = RightTab::Scene;
            } else if self.RightTab == RightTab::Scene {
                self.RightTab = RightTab::Layers;
            }
            self.center_symmetry();
        }
    }

    pub(crate) fn center_symmetry(&mut self) {
        if let Some((Width, Height)) = self.Project.Active.and_then(|Document| self.Project.document_size(Document)) {
            self.Symmetry.CenterX = Width as f32 * 0.5;
            self.Symmetry.CenterY = Height as f32 * 0.5;
        }
    }

    pub(crate) fn active_image_id(&self) -> Option<ImageId> {
        match self.Project.Active {
            Some(DocumentRef::Image(Id)) => Some(Id),
            _ => None,
        }
    }

    pub(crate) fn active_scene_id(&self) -> Option<SceneId> {
        match self.Project.Active {
            Some(DocumentRef::Scene(Id)) => Some(Id),
            _ => None,
        }
    }

    pub(crate) fn reset_project(&mut self, Project: Project, Path: Option<PathBuf>, History: Option<History>) {
        self.Interaction = Interaction::None;
        self.Project = Project;
        self.History = History.unwrap_or_else(|| History::new(self.Settings.UndoLimit));
        self.ProjectPath = Path;
        self.Modified = false;
        self.Views.clear();
        self.OverlayTextures.clear();
        self.LayerThumbs.clear();
        self.ContentBounds.clear();
        self.Floating = None;
        self.HasProject = true;
        self.Screen = Screen::Editor;
        self.Generation += 1;
        self.Playing = false;
        if self.Project.Active.is_none() {
            self.Project.Active = self.Project.first_document();
        }
        if self.Project.active_scene().is_some() {
            self.Tool = Tool::Move;
        }
        self.center_symmetry();
        #[cfg(feature = "desktop")]
        crate::desktop::project_changed(self);
    }

    pub(crate) fn guard_unsaved(&mut self, Action: PendingAction) {
        if self.Modified && self.Settings.ConfirmUnsaved {
            self.Dialog = Some(Dialog::ConfirmDiscard(Action));
        } else {
            self.perform_pending(Action);
        }
    }

    pub(crate) fn perform_pending(&mut self, Action: PendingAction) {
        match Action {
            PendingAction::NewProject => self.Dialog = Some(Dialog::NewProject(Default::default())),
            PendingAction::Open(Some(Path)) => {
                let IsProject = Path.extension().and_then(|Extension| Extension.to_str()).is_some_and(|Extension| Extension.eq_ignore_ascii_case("despite"));
                if IsProject || self.Screen == Screen::Home {
                    if IsProject { self.open_path(&Path) } else { self.project_from_image(&Path) }
                } else {
                    self.import_images(&[Path], false);
                }
            }
            PendingAction::Open(None) => self.open_dialog(),
            PendingAction::Close => self.close_project(),
            PendingAction::Quit => self.AllowClose = true,
        }
    }

    pub(crate) fn create_project(&mut self, Kind: ProjectKind, Name: &str, Width: u32, Height: u32, PixelArt: bool, Background: Option<Color>) {
        let mut Created = Project::new_of_kind(Name, Kind);
        let Id = Created.add_image("Image 1", Width.clamp(1, despite_core::raster::MaxDimension), Height.clamp(1, despite_core::raster::MaxDimension));
        Created.Active = Some(DocumentRef::Image(Id));
        let Target = Created.image_mut(Id).expect("image exists");
        Target.PixelArt = PixelArt;
        if let Some(Fill) = Background {
            Target.Layers[0].Name = "Background".into();
            Target.Layers[0].pixels_mut().fill(Fill.to_rgba8());
        }
        self.reset_project(Created, None, None);
        if PixelArt {
            self.Tool = Tool::Pencil;
        } else {
            self.Tool = Tool::Brush;
        }
        self.set_status(format!("New {} project", Kind.name()));
    }

    pub(crate) fn open_dialog(&mut self) {
        #[cfg(feature = "desktop")]
        {
            if let Some(Path) = platform::dialogs::open_project() {
                self.open_path(&Path);
            }
            return;
        }
        #[allow(unreachable_code)]
        {
            self.Dialog = Some(Dialog::FileBrowser(dialogs::FileBrowserState::open()));
        }
    }

    pub(crate) fn install_brush_file(&mut self, Path: &Path) {
        match std::fs::read(Path).map_err(|Problem| Problem.to_string()).and_then(|Bytes| self.Brushes.install(&Bytes)) {
            Ok(Message) => {
                self.set_status(Message.clone());
                self.Dialog = Some(Dialog::Message("Brushes installed".into(), format!("{Message}.\n\nFind them in the Brush panel under Installed. Use Manage brushes to uninstall.")));
            }
            Err(Problem) => self.Dialog = Some(Dialog::Message("Could not install brushes".into(), Problem)),
        }
    }

    pub(crate) fn open_path(&mut self, Path: &Path) {
        if Path.extension().and_then(|Extension| Extension.to_str()).is_some_and(|Extension| Extension.eq_ignore_ascii_case(brush::PackExtension)) {
            self.install_brush_file(Path);
            return;
        }
        let IsProject = Path.extension().and_then(|Extension| Extension.to_str()).is_some_and(|Extension| Extension.eq_ignore_ascii_case("despite"));
        if !IsProject {
            self.import_images(&[Path.to_path_buf()], false);
            return;
        }
        match despite_core::io::load_project(Path, &LoadOptions { LoadHistory: self.Settings.LoadHistoryFromProject }) {
            Ok(Loaded) => {
                let HadHistory = Loaded.FileHadHistory;
                self.reset_project(Loaded.Project, Some(Path.to_path_buf()), Loaded.History);
                self.Settings.push_recent(Path.to_path_buf());
                self.Settings.save();
                self.set_status(if HadHistory { format!("Opened {} (with saved undo history)", Path.display()) } else { format!("Opened {}", Path.display()) });
                #[cfg(feature = "desktop")]
                crate::desktop::project_opened(self);
            }
            Err(Problem) => self.Dialog = Some(Dialog::Message("Could not open project".into(), Problem.to_string())),
        }
    }

    pub(crate) fn save(&mut self) {
        match self.ProjectPath.clone() {
            Some(Path) => self.save_to(&Path),
            None => self.save_as(),
        }
    }

    pub(crate) fn save_as(&mut self) {
        #[cfg(feature = "desktop")]
        {
            if let Some(Path) = platform::dialogs::save_project(&despite_core::export::sanitize_file_name(&self.Project.Name)) {
                self.save_to(&Path);
            }
            return;
        }
        #[allow(unreachable_code)]
        {
            self.Dialog = Some(Dialog::FileBrowser(dialogs::FileBrowserState::save(&self.Project.Name)));
        }
    }

    pub(crate) fn save_to(&mut self, Path: &Path) {
        crate::transform::commit_floating(self);
        if let Some(Parent) = Path.parent() {
            platform::ensure_dir(Parent);
        }
        let Options = SaveOptions { IncludeHistory: self.Settings.SaveHistoryInProject, ..Default::default() };
        match despite_core::io::save_project(Path, &self.Project, Some(&self.History), &Options) {
            Ok(()) => {
                self.ProjectPath = Some(Path.to_path_buf());
                self.Modified = false;
                if let Some(Stem) = Path.file_stem().and_then(|Stem| Stem.to_str()) {
                    self.Project.Name = Stem.to_string();
                }
                self.Settings.push_recent(Path.to_path_buf());
                self.Settings.save();
                let Size = std::fs::metadata(Path).map(|Meta| Meta.len()).unwrap_or(0);
                self.set_status(format!("Saved {} ({})", Path.display(), human_bytes(Size)));
            }
            Err(Problem) => self.Dialog = Some(Dialog::Message("Could not save".into(), Problem.to_string())),
        }
    }

    fn autosave(&mut self) {
        let Interval = self.Settings.AutosaveMinutes as f64 * 60.0;
        if Interval <= 0.0 || !self.Modified || self.Now - self.LastAutosave < Interval || !matches!(self.Interaction, Interaction::None) {
            return;
        }
        self.LastAutosave = self.Now;
        let Directory = platform::autosave_dir();
        platform::ensure_dir(&Directory);
        let Path = Directory.join(format!("{}.despite", despite_core::export::sanitize_file_name(&self.Project.Name)));
        if let Err(Problem) = despite_core::io::save_project(&Path, &self.Project, None, &SaveOptions { Thumbnail: false, ..Default::default() }) {
            log::warn!("autosave failed: {Problem}");
        }
    }

    pub(crate) fn project_from_image(&mut self, Path: &Path) {
        match Raster::load(Path) {
            Ok(Pixels) => {
                let Name = Path.file_stem().and_then(|Stem| Stem.to_str()).unwrap_or("Imported").to_string();
                let PixelArt = Pixels.width().max(Pixels.height()) <= 256;
                let mut Created = Project::new(&Name);
                let Id = Created.add_image_from_raster(&Name, Pixels);
                if let Some(Found) = Created.image_mut(Id) {
                    Found.PixelArt = PixelArt;
                }
                Created.Active = Some(DocumentRef::Image(Id));
                self.reset_project(Created, None, None);
                self.Modified = true;
                self.Tool = if PixelArt { Tool::Pencil } else { Tool::Brush };
                self.set_status(format!("New project from {}", Path.display()));
            }
            Err(Problem) => self.Dialog = Some(Dialog::Message("Could not open image".into(), Problem.to_string())),
        }
    }

    pub(crate) fn project_from_image_dialog(&mut self) {
        #[cfg(feature = "desktop")]
        {
            if let Some(Path) = platform::dialogs::open_file("Images", platform::ImageExtensions) {
                if self.Modified && self.HasProject && self.Settings.ConfirmUnsaved {
                    self.Dialog = Some(Dialog::ConfirmDiscard(PendingAction::Open(Some(Path))));
                } else {
                    self.project_from_image(&Path);
                }
            }
            return;
        }
        #[allow(unreachable_code)]
        {
            self.Dialog = Some(Dialog::FileBrowser(dialogs::FileBrowserState::import(false)));
        }
    }

    pub(crate) fn close_project(&mut self) {
        self.cancel_interaction();
        let Placeholder = Project::with_image("Untitled", self.Settings.DefaultWidth, self.Settings.DefaultHeight);
        self.reset_project(Placeholder, None, None);
        self.HasProject = false;
        self.Screen = Screen::Home;
        self.Home.NeedsScan = true;
        self.set_status("Project closed");
    }

    pub(crate) fn go_home(&mut self) {
        self.cancel_interaction();
        self.Screen = Screen::Home;
        self.Home.NeedsScan = true;
    }

    pub(crate) fn import_images(&mut self, Paths: &[PathBuf], AsLayers: bool) {
        if self.Screen == Screen::Home || !self.HasProject {
            if let Some(First) = Paths.first() {
                self.project_from_image(First);
            }
            return;
        }
        let mut Imported = 0;
        for Path in Paths {
            match Raster::load(Path) {
                Ok(Pixels) => {
                    let Name = Path.file_stem().and_then(|Stem| Stem.to_str()).unwrap_or("Imported").to_string();
                    if AsLayers
                        && let Some(Id) = self.active_image_id() {
                            if Imported == 0 {
                                self.record("Import as layer");
                            }
                            self.Project.add_layer_from_raster(Id, &Name, Pixels);
                            Imported += 1;
                            continue;
                        }
                    if Imported == 0 {
                        self.record("Import image");
                    }
                    let Id = self.Project.add_image_from_raster(&Name, Pixels);
                    self.activate(DocumentRef::Image(Id));
                    Imported += 1;
                }
                Err(Problem) => self.set_status(format!("Could not import {}: {Problem}", Path.display())),
            }
        }
        if Imported > 0 {
            self.touch();
            self.set_status(format!("Imported {Imported} image(s)"));
        }
    }

    pub(crate) fn add_image(&mut self, Name: &str, Width: u32, Height: u32, PixelArt: bool, Background: Option<Color>) {
        self.record("New image");
        let Id = self.Project.add_image(Name, Width, Height);
        if let Some(Target) = self.Project.image_mut(Id) {
            Target.PixelArt = PixelArt;
            if let Some(Fill) = Background {
                Target.Layers[0].Name = "Background".into();
                Target.Layers[0].pixels_mut().fill(Fill.to_rgba8());
            }
        }
        self.activate(DocumentRef::Image(Id));
        self.touch();
    }

    pub(crate) fn add_scene(&mut self, Name: &str, Width: u32, Height: u32, Fps: f32, Background: Option<Color>) {
        self.record("New scene");
        let Id = self.Project.add_scene(Name, Width, Height);
        if let Some(Target) = self.Project.scene_mut(Id) {
            Target.Fps = Fps;
            Target.Background = Background;
        }
        self.activate(DocumentRef::Scene(Id));
        self.touch();
    }

    pub(crate) fn clone_document(&mut self, Document: DocumentRef) {
        self.record("Clone");
        let Created = match Document {
            DocumentRef::Image(Id) => self.Project.clone_image(Id).map(DocumentRef::Image),
            DocumentRef::Scene(Id) => self.Project.clone_scene(Id).map(DocumentRef::Scene),
        };
        if let Some(New) = Created {
            self.activate(New);
            self.touch();
        }
    }

    pub(crate) fn delete_document(&mut self, Document: DocumentRef) {
        let Count = self.Project.Images.len() + self.Project.Scenes.len();
        if Count <= 1 {
            self.set_status("A project needs at least one image or scene");
            return;
        }
        self.record("Delete");
        match Document {
            DocumentRef::Image(Id) => {
                self.Project.remove_image(Id);
            }
            DocumentRef::Scene(Id) => {
                self.Project.remove_scene(Id);
            }
        }
        self.Views.remove(&Document);
        self.Generation += 1;
        self.touch();
    }

    pub(crate) fn with_active_layer<R>(&mut self, Label: &str, Action: impl FnOnce(&mut despite_core::Image) -> R) -> Option<R> {
        let Id = self.active_image_id()?;
        self.record(Label);
        let Target = self.Project.image_mut(Id)?;
        let Result = Action(Target);
        self.touch();
        Some(Result)
    }

    pub(crate) fn active_layer_locked(&self) -> bool {
        self.Project.active_image().and_then(|Found| Found.active_layer()).is_some_and(|Entry| Entry.Locked)
    }

    pub(crate) fn new_layer(&mut self) {
        if let Some(Id) = self.active_image_id() {
            self.record("New layer");
            let Count = self.Project.image(Id).map(|Found| Found.Layers.len()).unwrap_or(0);
            self.Project.add_layer(Id, &format!("Layer {}", Count + 1));
            self.touch();
        }
    }

    pub(crate) fn duplicate_layer(&mut self) {
        if let Some(Id) = self.active_image_id() {
            let Index = self.Project.image(Id).map(|Found| Found.ActiveLayer).unwrap_or(0);
            self.record("Duplicate layer");
            self.Project.duplicate_layer(Id, Index);
            self.touch();
        }
    }

    pub(crate) fn delete_layer(&mut self) {
        let Removed = self.with_active_layer("Delete layer", |Target| {
            let Index = Target.ActiveLayer;
            Target.remove_layer(Index).is_some()
        });
        if Removed == Some(false) {
            self.History.Undo.pop();
            self.set_status("An image needs at least one layer");
        }
    }

    pub(crate) fn merge_down(&mut self) {
        let Merged = self.with_active_layer("Merge down", |Target| {
            let Index = Target.ActiveLayer;
            Target.merge_down(Index)
        });
        if Merged == Some(false) {
            self.History.Undo.pop();
        }
    }

    pub(crate) fn flatten(&mut self) {
        let Id = self.Project.alloc_layer_id();
        self.with_active_layer("Flatten", |Target| Target.flatten(Id));
    }

    pub(crate) fn move_layer(&mut self, Up: bool) {
        self.with_active_layer("Reorder layer", |Target| {
            let Index = Target.ActiveLayer;
            let Destination = if Up { Index + 1 } else { Index.wrapping_sub(1) };
            if Destination < Target.Layers.len() {
                Target.move_layer(Index, Destination);
            }
        });
    }

    pub(crate) fn transform_layer_pixels(&mut self, Label: &str, Operation: impl Fn(&Raster) -> Raster) {
        self.with_active_layer(Label, |Target| {
            let (Width, Height, Smooth) = (Target.Width, Target.Height, Target.smooth_sampling());
            if let Some(Entry) = Target.active_layer_mut() {
                Entry.bake_transform(Width, Height, Smooth);
                let Next = Operation(&Entry.Pixels);
                Entry.Pixels = Arc::new(Next);
            }
        });
    }

    pub(crate) fn transform_image(&mut self, Label: &str, Rotate: Option<bool>, Flip: Option<bool>) {
        self.with_active_layer(Label, |Target| {
            let (Width, Height, Smooth) = (Target.Width, Target.Height, Target.smooth_sampling());
            for Entry in &mut Target.Layers {
                Entry.bake_transform(Width, Height, Smooth);
                let Next = match (Rotate, Flip) {
                    (Some(Clockwise), _) => rotate_90(&Entry.Pixels, Clockwise),
                    (_, Some(true)) => Entry.Pixels.flipped_h(),
                    (_, Some(false)) => Entry.Pixels.flipped_v(),
                    _ => continue,
                };
                Entry.Pixels = Arc::new(Next);
            }
            if Rotate.is_some() {
                std::mem::swap(&mut Target.Width, &mut Target.Height);
            }
        });
        if Rotate.is_some()
            && let Some(View) = self.active_view() {
                View.set_selection(None);
            }
    }

    pub(crate) fn bake_layer_transform(&mut self) {
        self.with_active_layer("Apply transform", |Target| {
            let (Width, Height, Smooth) = (Target.Width, Target.Height, Target.smooth_sampling());
            if let Some(Entry) = Target.active_layer_mut() {
                Entry.bake_transform(Width, Height, Smooth);
            }
        });
    }

    pub(crate) fn select_all(&mut self) {
        if let Some((Width, Height)) = self.active_image_id().and_then(|Id| self.Project.document_size(DocumentRef::Image(Id)))
            && let Some(View) = self.active_view() {
                View.set_selection(Some(Mask::full(Width, Height)));
            }
    }

    pub(crate) fn deselect(&mut self) {
        crate::transform::commit_floating(self);
        if let Some(View) = self.active_view() {
            View.set_selection(None);
        }
    }

    pub(crate) fn invert_selection(&mut self) {
        let Size = self.active_image_id().and_then(|Id| self.Project.document_size(DocumentRef::Image(Id)));
        if let (Some((Width, Height)), Some(View)) = (Size, self.active_view()) {
            let mut Next = View.Selection.as_deref().cloned().unwrap_or_else(|| Mask::new(Width, Height));
            Next.invert();
            let Empty = Next.is_empty();
            View.set_selection((!Empty).then_some(Next));
        }
    }

    pub(crate) fn grow_selection(&mut self, Amount: i32) {
        if let Some(View) = self.active_view()
            && let Some(Current) = View.Selection.as_deref().cloned() {
                let mut Next = Current;
                if Amount >= 0 {
                    Next.dilate(Amount as u32);
                } else {
                    Next.invert();
                    Next.dilate((-Amount) as u32);
                    Next.invert();
                }
                let Empty = Next.is_empty();
                View.set_selection((!Empty).then_some(Next));
            }
    }

    fn selection_view<'a>(Selected: &'a Mask, Entry: &Layer) -> MaskView<'a> {
        MaskView::new(Selected, Entry.matrix(), Entry.Transform.integer_offset())
    }

    pub(crate) fn clear_selected(&mut self) {
        let Selected = self.selection();
        self.with_active_layer("Clear", |Target| {
            if let Some(Entry) = Target.active_layer_mut() {
                match &Selected {
                    None => Entry.pixels_mut().clear(),
                    Some(Found) => {
                        let Matrix = Entry.matrix();
                        let Offset = Entry.Transform.integer_offset();
                        let Pixels = Entry.pixels_mut();
                        let View = MaskView::new(Found, Matrix, Offset);
                        for Y in 0..Pixels.height() {
                            for X in 0..Pixels.width() {
                                let Amount = View.coverage(X as i32, Y as i32);
                                if Amount > 0.0 {
                                    let mut Pixel = Pixels.get(X, Y);
                                    Pixel[3] = (Pixel[3] as f32 * (1.0 - Amount)).round() as u8;
                                    Pixels.put(X, Y, if Pixel[3] == 0 { [0; 4] } else { Pixel });
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    pub(crate) fn fill_selected(&mut self, Paint: Color) {
        let Selected = self.selection();
        self.with_active_layer("Fill", |Target| {
            let (Width, Height) = (Target.Width, Target.Height);
            if let Some(Entry) = Target.active_layer_mut() {
                let Whole = Mask::full(Width, Height);
                let Region = Selected.as_deref().unwrap_or(&Whole);
                let View = Self::selection_view(Region, Entry);
                let Style = despite_core::fill::FillStyle { Paint, Opacity: 1.0, Blend: BlendMode::Normal, AlphaLocked: Entry.AlphaLocked };
                let Matrix = Entry.matrix();
                let Offset = Entry.Transform.integer_offset();
                let _ = View;
                despite_core::fill::fill_region(Entry.pixels_mut(), MaskView::new(Region, Matrix, Offset), None, Style);
            }
        });
    }

    pub(crate) fn selection_to_layer(&mut self) {
        let Saved = self.Clipboard.take();
        let Context = egui::Context::default();
        if self.copy_pixels(&Context, false) {
            if let Some(Copied) = self.Clipboard.take() {
                self.paste_internal(Copied.Content);
            }
        }
        self.Clipboard = Saved;
    }

    pub(crate) fn crop_to_selection(&mut self) {
        let Bounds = self.selection().and_then(|Found| Found.bounds());
        match Bounds {
            Some(Rect) => {
                self.with_active_layer("Crop", |Target| Target.crop(Rect));
                self.deselect();
            }
            None => self.set_status("Make a selection first"),
        }
    }

    pub(crate) fn trim_to_content(&mut self) {
        let Bounds = self.Project.active_image().and_then(|Found| Found.composite().opaque_bounds());
        match Bounds {
            Some(Rect) => {
                self.with_active_layer("Trim", |Target| Target.crop(Rect));
                self.deselect();
            }
            None => self.set_status("Image is empty"),
        }
    }

    pub(crate) fn apply_filter(&mut self, Chosen: &Filter) {
        if self.active_layer_locked() {
            self.set_status("Layer is locked");
            return;
        }
        let Selected = self.selection();
        let Palette = self.Project.Palette.clone();
        let Name = Chosen.name();
        self.with_active_layer(Name, |Target| {
            if let Some(Entry) = Target.active_layer_mut() {
                let Matrix = Entry.matrix();
                let Offset = Entry.Transform.integer_offset();
                let Clip = Selected.as_deref().map(|Found| MaskView::new(Found, Matrix, Offset));
                let mut Pixels = (*Entry.Pixels).clone();
                despite_core::filters::apply_filter(&mut Pixels, Chosen, Clip, &Palette);
                Entry.Pixels = Arc::new(Pixels);
            }
        });
        self.set_status(format!("Applied {Name}"));
    }

    pub(crate) fn swap_colors(&mut self) {
        std::mem::swap(&mut self.Primary, &mut self.Secondary);
    }

    fn handle_shortcuts(&mut self, Context: &Context) {
        let Command = Modifiers::COMMAND;
        let CommandShift = Modifiers::COMMAND | Modifiers::SHIFT;
        let Shortcut = |Mods: Modifiers, Pressed: Key| KeyboardShortcut::new(Mods, Pressed);
        let Consume = |Mods: Modifiers, Pressed: Key| Context.input_mut(|Input| Input.consume_shortcut(&Shortcut(Mods, Pressed)));
        if Consume(CommandShift, Key::Z) || Consume(Command, Key::Y) {
            self.redo();
        }
        if Consume(Command, Key::Z) {
            self.undo();
        }
        if Consume(CommandShift, Key::S) {
            self.save_as();
        }
        if Consume(Command, Key::S) {
            self.save();
        }
        if Consume(Command, Key::O) {
            self.guard_unsaved(PendingAction::Open(None));
        }
        if Consume(Command, Key::N) {
            self.guard_unsaved(PendingAction::NewProject);
        }
        if Consume(Command, Key::E) {
            self.Dialog = Some(Dialog::Export(dialogs::ExportState::new(self)));
        }
        if Consume(CommandShift, Key::I) {
            self.invert_selection();
        }
        if Consume(CommandShift, Key::N) {
            self.new_layer();
        }
        if Context.egui_wants_keyboard_input() {
            return;
        }
        if Consume(Modifiers::NONE, Key::Enter) {
            if self.Floating.is_some() {
                crate::transform::commit_floating(self);
                self.set_status("Applied");
            } else if self.selection().is_some() {
                self.deselect();
                self.set_status("Deselected");
            }
        }
        if Consume(Modifiers::NONE, Key::Escape) {
            if !matches!(self.Interaction, Interaction::None) {
                self.cancel_interaction();
            } else if self.Floating.is_some() {
                crate::transform::cancel_floating(self);
            } else {
                self.deselect();
            }
        }
        if Consume(Command, Key::A) {
            self.select_all();
        }
        if Consume(Command, Key::D) {
            self.deselect();
        }
        self.handle_clipboard_events(Context);
        if Consume(Command, Key::J) {
            self.selection_to_layer();
        }
        if Consume(Modifiers::NONE, Key::Delete) || Consume(Modifiers::NONE, Key::Backspace) {
            self.clear_selected();
        }
        if Consume(Modifiers::NONE, Key::Tab) {
            let Visible = !(self.ShowLeft || self.ShowRight);
            self.ShowLeft = Visible;
            self.ShowRight = Visible;
        }
        for Candidate in Tool::All {
            if let Some((Pressed, Shift)) = Candidate.shortcut() {
                let Mods = if Shift { Modifiers::SHIFT } else { Modifiers::NONE };
                if Consume(Mods, Pressed) {
                    self.set_tool(*Candidate);
                }
            }
        }
        if Consume(Modifiers::NONE, Key::X) {
            self.swap_colors();
        }
        if Consume(Modifiers::NONE, Key::D) {
            self.Primary = Color::Black;
            self.Secondary = Color::White;
        }
        if Consume(Modifiers::NONE, Key::OpenBracket) {
            let Active = self.Tool;
            let Target = self.Brushes.for_tool_mut(Active);
            Target.Size = (Target.Size - if Target.Size > 10.0 { 2.0 } else { 1.0 }).max(1.0);
        }
        if Consume(Modifiers::NONE, Key::CloseBracket) {
            let Active = self.Tool;
            let Target = self.Brushes.for_tool_mut(Active);
            Target.Size = (Target.Size + if Target.Size >= 10.0 { 2.0 } else { 1.0 }).min(1000.0);
        }
        if Consume(Modifiers::NONE, Key::Num0)
            && let Some(View) = self.active_view() {
                View.NeedsFit = true;
            }
        if Consume(Modifiers::NONE, Key::Num1)
            && let Some(View) = self.active_view() {
                View.Zoom = 1.0;
                View.Pan = egui::Vec2::ZERO;
            }
        if Consume(Modifiers::NONE, Key::Equals) || Consume(Modifiers::NONE, Key::Plus) {
            let Step = self.Settings.ZoomStep;
            if let Some(View) = self.active_view() {
                View.Zoom = (View.Zoom * Step).min(256.0);
                View.AutoFit = false;
            }
        }
        if Consume(Modifiers::NONE, Key::Minus) {
            let Step = self.Settings.ZoomStep;
            if let Some(View) = self.active_view() {
                View.Zoom = (View.Zoom / Step).max(0.02);
                View.AutoFit = false;
            }
        }
        if self.active_scene_id().is_some() && Consume(Modifiers::NONE, Key::Space) {
            self.Playing = !self.Playing;
        }
    }

    pub(crate) fn menu_bar(&mut self, Ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(Ui, |Ui| {
            if crate::icons::button(Ui, "Home", false, "Home: all projects").clicked() {
                self.go_home();
            }
            Ui.menu_button("File", |Ui| {
                if crate::icons::text_button(Ui, "Home", "Home").clicked() {
                    self.go_home();
                    Ui.close();
                }
                if Ui.button("New Project…").clicked() {
                    self.guard_unsaved(PendingAction::NewProject);
                    Ui.close();
                }
                if Ui.button("Open…").clicked() {
                    self.guard_unsaved(PendingAction::Open(None));
                    Ui.close();
                }
                let Recent = self.Settings.RecentFiles.clone();
                Ui.add_enabled_ui(!Recent.is_empty(), |Ui| {
                    Ui.menu_button("Open Recent", |Ui| {
                        for Path in Recent {
                            if Ui.button(Path.display().to_string()).clicked() {
                                self.guard_unsaved(PendingAction::Open(Some(Path.clone())));
                                Ui.close();
                            }
                        }
                    });
                });
                Ui.separator();
                if Ui.button("Close Project").clicked() {
                    self.guard_unsaved(PendingAction::Close);
                    Ui.close();
                }
                Ui.separator();
                if Ui.button("Save").clicked() {
                    self.save();
                    Ui.close();
                }
                if Ui.button("Save As…").clicked() {
                    self.save_as();
                    Ui.close();
                }
                Ui.separator();
                if Ui.button("Import Image as New Image…").clicked() {
                    self.import_dialog(false);
                    Ui.close();
                }
                if Ui.button("Import Image as Layer…").clicked() {
                    self.import_dialog(true);
                    Ui.close();
                }
                if Ui.button("Export…").clicked() {
                    self.Dialog = Some(Dialog::Export(dialogs::ExportState::new(self)));
                    Ui.close();
                }
                Ui.separator();
                if Ui.button("Settings…").clicked() {
                    self.ShowSettings = true;
                    Ui.close();
                }
                if Ui.button("Open Documents Folder").clicked() {
                    let Folder = platform::documents_dir();
                    platform::ensure_dir(&Folder);
                    platform::open_in_file_manager(&Folder);
                    Ui.close();
                }
                if !platform::IsMobile && Ui.button("Quit").clicked() {
                    Ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    Ui.close();
                }
            });
            Ui.menu_button("Edit", |Ui| {
                let UndoText = self.History.undo_label().map(|Label| format!("Undo {Label}")).unwrap_or_else(|| "Undo".into());
                if Ui.add_enabled(self.History.can_undo(), egui::Button::new(UndoText)).clicked() {
                    self.undo();
                }
                let RedoText = self.History.redo_label().map(|Label| format!("Redo {Label}")).unwrap_or_else(|| "Redo".into());
                if Ui.add_enabled(self.History.can_redo(), egui::Button::new(RedoText)).clicked() {
                    self.redo();
                }
                Ui.separator();
                self.edit_clipboard_items(Ui);
                Ui.separator();
                if Ui.button("Fill with Primary Color").clicked() {
                    let Paint = self.Primary;
                    self.fill_selected(Paint);
                    Ui.close();
                }
                if Ui.button("Fill with Secondary Color").clicked() {
                    let Paint = self.Secondary;
                    self.fill_selected(Paint);
                    Ui.close();
                }
                if Ui.button("Clear").clicked() {
                    self.clear_selected();
                    Ui.close();
                }
            });
            Ui.menu_button("Select", |Ui| {
                if Ui.button("Select All").clicked() {
                    self.select_all();
                    Ui.close();
                }
                if Ui.button("Deselect").clicked() {
                    self.deselect();
                    Ui.close();
                }
                if Ui.button("Invert Selection").clicked() {
                    self.invert_selection();
                    Ui.close();
                }
                Ui.separator();
                for (Label, Amount) in [("Grow 1px", 1), ("Grow 4px", 4), ("Shrink 1px", -1), ("Shrink 4px", -4)] {
                    if Ui.button(Label).clicked() {
                        self.grow_selection(Amount);
                        Ui.close();
                    }
                }
                Ui.separator();
                if Ui.button("Selection to New Layer").clicked() {
                    self.selection_to_layer();
                    Ui.close();
                }
                if Ui.button("Crop to Selection").clicked() {
                    self.crop_to_selection();
                    Ui.close();
                }
            });
            Ui.menu_button("Image", |Ui| {
                if Ui.button("New Image…").clicked() {
                    self.Dialog = Some(Dialog::NewImage(dialogs::NewImageState::new(&self.Settings)));
                    Ui.close();
                }
                if Ui.button("New Scene…").clicked() {
                    self.Dialog = Some(Dialog::NewScene(Default::default()));
                    Ui.close();
                }
                if let Some(Document) = self.active_document()
                    && Ui.button("Clone Current").clicked() {
                        self.clone_document(Document);
                        Ui.close();
                    }
                Ui.separator();
                let HasImage = self.active_image_id().is_some();
                Ui.add_enabled_ui(HasImage, |Ui| {
                    if Ui.button("Resize Canvas…").clicked() {
                        self.Dialog = Some(Dialog::ResizeCanvas(dialogs::ResizeState::new(self)));
                        Ui.close();
                    }
                    if Ui.button("Scale Image…").clicked() {
                        self.Dialog = Some(Dialog::ScaleImage(dialogs::ScaleState::new(self)));
                        Ui.close();
                    }
                    if Ui.button("Trim to Content").clicked() {
                        self.trim_to_content();
                        Ui.close();
                    }
                    Ui.separator();
                    if Ui.button("Rotate 90° Clockwise").clicked() {
                        self.transform_image("Rotate image", Some(true), None);
                        Ui.close();
                    }
                    if Ui.button("Rotate 90° Counter-clockwise").clicked() {
                        self.transform_image("Rotate image", Some(false), None);
                        Ui.close();
                    }
                    if Ui.button("Flip Horizontal").clicked() {
                        self.transform_image("Flip image", None, Some(true));
                        Ui.close();
                    }
                    if Ui.button("Flip Vertical").clicked() {
                        self.transform_image("Flip image", None, Some(false));
                        Ui.close();
                    }
                    Ui.separator();
                    let mut PixelArt = self.Project.active_image().is_some_and(|Found| Found.PixelArt);
                    if Ui.checkbox(&mut PixelArt, "Pixel Art Mode (nearest sampling)").changed() {
                        self.with_active_layer("Pixel art mode", |Target| Target.PixelArt = PixelArt);
                    }
                });
            });
            Ui.menu_button("Layer", |Ui| {
                Ui.add_enabled_ui(self.active_image_id().is_some(), |Ui| {
                    if Ui.button("New Layer").clicked() {
                        self.new_layer();
                        Ui.close();
                    }
                    if Ui.button("Duplicate Layer").clicked() {
                        self.duplicate_layer();
                        Ui.close();
                    }
                    if Ui.button("Delete Layer").clicked() {
                        self.delete_layer();
                        Ui.close();
                    }
                    if Ui.button("Merge Down").clicked() {
                        self.merge_down();
                        Ui.close();
                    }
                    if Ui.button("Flatten Image").clicked() {
                        self.flatten();
                        Ui.close();
                    }
                    Ui.separator();
                    if Ui.button("Move Up").clicked() {
                        self.move_layer(true);
                        Ui.close();
                    }
                    if Ui.button("Move Down").clicked() {
                        self.move_layer(false);
                        Ui.close();
                    }
                    Ui.separator();
                    if Ui.button("Flip Layer Horizontal").clicked() {
                        self.transform_layer_pixels("Flip layer", Raster::flipped_h);
                        Ui.close();
                    }
                    if Ui.button("Flip Layer Vertical").clicked() {
                        self.transform_layer_pixels("Flip layer", Raster::flipped_v);
                        Ui.close();
                    }
                    if Ui.button("Apply Layer Transform").clicked() {
                        self.bake_layer_transform();
                        Ui.close();
                    }
                    if Ui.button("Reset Layer Transform").clicked() {
                        self.with_active_layer("Reset transform", |Target| {
                            if let Some(Entry) = Target.active_layer_mut() {
                                Entry.Transform = LayerTransform::default();
                            }
                        });
                        Ui.close();
                    }
                });
            });
            Ui.menu_button("Filters", |Ui| {
                Ui.add_enabled_ui(self.active_image_id().is_some(), |Ui| {
                    for Chosen in Filter::catalogue() {
                        let Label = if Chosen.has_settings() { format!("{}…", Chosen.name()) } else { Chosen.name().to_string() };
                        if Ui.button(Label).clicked() {
                            if Chosen.has_settings() {
                                self.Dialog = Some(Dialog::Filter(dialogs::FilterState::new(Chosen)));
                            } else {
                                self.apply_filter(&Chosen);
                            }
                            Ui.close();
                        }
                    }
                });
            });
            #[cfg(feature = "desktop")]
            crate::desktop::menus(self, Ui);
            Ui.menu_button("View", |Ui| {
                Ui.checkbox(&mut self.ShowLeft, "Left Panel");
                Ui.checkbox(&mut self.ShowRight, "Right Panel");
                Ui.checkbox(&mut self.ShowBottom, "Code Panel");
                Ui.separator();
                Ui.checkbox(&mut self.Settings.PixelGrid, "Pixel Grid");
                Ui.checkbox(&mut self.Settings.TileGrid, "Tile Grid");
                Ui.checkbox(&mut self.Settings.TilePreview, "Tile Preview (seamless)");
                Ui.checkbox(&mut self.Wrap, "Wrap-around Painting");
                Ui.separator();
                if Ui.button("Fit to Window").clicked() {
                    if let Some(View) = self.active_view() {
                        View.request_fit();
                    }
                    Ui.close();
                }
                if Ui.button("Actual Size (100%)").clicked() {
                    if let Some(View) = self.active_view() {
                        View.Zoom = 1.0;
                        View.Pan = egui::Vec2::ZERO;
                        View.AutoFit = false;
                    }
                    Ui.close();
                }
                Ui.separator();
                let mut Touch = self.Compact;
                if Ui.checkbox(&mut Touch, "Touch Layout").changed() {
                    self.Settings.Layout = if Touch { LayoutMode::Touch } else { LayoutMode::Desktop };
                }
            });
            Ui.menu_button("Help", |Ui| {
                if Ui.button("Keyboard Shortcuts").clicked() {
                    self.Dialog = Some(Dialog::Shortcuts);
                    Ui.close();
                }
                if Ui.button("About Despite").clicked() {
                    self.Dialog = Some(Dialog::About);
                    Ui.close();
                }
            });
        });
    }

    pub(crate) fn edit_clipboard_items(&mut self, Ui: &mut egui::Ui) {
        let Context = Ui.ctx().clone();
        if Ui.button("Cut").clicked() {
            self.cut_smart(&Context);
            Ui.close();
        }
        if Ui.button("Copy").clicked() {
            self.copy_smart(&Context);
            Ui.close();
        }
        if self.active_image_id().is_some() {
            if Ui.button("Copy Merged").clicked() {
                self.copy_pixels(&Context, true);
                Ui.close();
            }
            if Ui.button("Copy Layer").clicked() {
                self.copy_layer(&Context);
                Ui.close();
            }
        }
        if let Some(Document) = self.Project.Active {
            let Label = if matches!(Document, DocumentRef::Scene(_)) { "Copy Whole Scene" } else { "Copy Whole Image" };
            if Ui.button(Label).clicked() {
                self.copy_document(&Context, Document);
                Ui.close();
            }
        }
        let PasteLabel = match &self.Clipboard {
            Some(Copied) => format!("Paste {}", Copied.Content.describe()),
            None => "Paste".to_string(),
        };
        if Ui.add_enabled(self.Clipboard.is_some(), egui::Button::new(PasteLabel)).clicked() {
            self.paste_smart();
            Ui.close();
        }
    }

    pub(crate) fn import_dialog(&mut self, AsLayers: bool) {
        #[cfg(feature = "desktop")]
        {
            if let Some(Paths) = platform::dialogs::open_images() {
                self.import_images(&Paths, AsLayers);
            }
            return;
        }
        #[allow(unreachable_code)]
        {
            self.Dialog = Some(Dialog::FileBrowser(dialogs::FileBrowserState::import(AsLayers)));
        }
    }

    fn update_layout_mode(&mut self, Context: &Context) {
        let Width = Context.content_rect().width();
        let Next = match self.Settings.Layout {
            LayoutMode::Desktop => false,
            LayoutMode::Touch => true,
            LayoutMode::Auto => platform::IsMobile || self.ForceTouch || Width < 760.0,
        };
        if Next != self.Compact {
            self.Compact = Next;
            if Next {
                self.ShowLeft = false;
                self.ShowRight = false;
            } else {
                self.ShowLeft = true;
                self.ShowRight = true;
            }
            self.apply_style(Context);
        }
    }

    fn handle_dropped_files(&mut self, Context: &Context) {
        let Dropped: Vec<PathBuf> = Context.input(|Input| Input.raw.dropped_files.iter().map(|File| File.path().to_path_buf()).filter(|Path| !Path.as_os_str().is_empty()).collect());
        if Dropped.is_empty() {
            return;
        }
        let (Packs, Dropped): (Vec<PathBuf>, Vec<PathBuf>) =
            Dropped.into_iter().partition(|Path| Path.extension().and_then(|Extension| Extension.to_str()) == Some(brush::PackExtension));
        for Pack in Packs {
            self.install_brush_file(&Pack);
        }
        if Dropped.is_empty() {
            return;
        }
        let (Projects, Images): (Vec<PathBuf>, Vec<PathBuf>) =
            Dropped.into_iter().partition(|Path| Path.extension().and_then(|Extension| Extension.to_str()) == Some("despite"));
        if let Some(First) = Projects.into_iter().next() {
            self.guard_unsaved(PendingAction::Open(Some(First)));
        }
        if !Images.is_empty() {
            let AsLayers = Context.input(|Input| Input.modifiers.shift);
            self.import_images(&Images, AsLayers);
        }
    }

    fn update_title(&self, Context: &Context) {
        let Marker = if self.Modified { "•" } else { "" };
        let Title = if self.Screen == Screen::Home { "Despite".to_string() } else { format!("{}{Marker} — Despite", self.Project.Name) };
        Context.send_viewport_cmd(egui::ViewportCommand::Title(Title));
    }

    fn handle_close(&mut self, Context: &Context) {
        if Context.input(|Input| Input.viewport().close_requested()) && !self.AllowClose && self.Modified && self.Settings.ConfirmUnsaved {
            Context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.Dialog = Some(Dialog::ConfirmDiscard(PendingAction::Quit));
        }
        if self.AllowClose {
            Context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn compact_top_bar(&mut self, Ui: &mut egui::Ui) {
        Ui.horizontal_wrapped(|Ui| {
            if crate::icons::button(Ui, "Home", false, "Home").clicked() {
                self.go_home();
            }
            let MenuIcon = crate::icons::image(Ui, "Menu", Ui.visuals().text_color());
            let MenuContents = |Ui: &mut egui::Ui| {
                if Ui.button("New Project").clicked() {
                    self.guard_unsaved(PendingAction::NewProject);
                    Ui.close();
                }
                if Ui.button("Open").clicked() {
                    self.guard_unsaved(PendingAction::Open(None));
                    Ui.close();
                }
                if Ui.button("Save").clicked() {
                    self.save();
                    Ui.close();
                }
                if Ui.button("Save As").clicked() {
                    self.save_as();
                    Ui.close();
                }
                if Ui.button("Close Project").clicked() {
                    self.guard_unsaved(PendingAction::Close);
                    Ui.close();
                }
                if Ui.button("Import Image").clicked() {
                    self.import_dialog(false);
                    Ui.close();
                }
                if Ui.button("Export").clicked() {
                    self.Dialog = Some(Dialog::Export(dialogs::ExportState::new(self)));
                    Ui.close();
                }
                if Ui.button("New Image").clicked() {
                    self.Dialog = Some(Dialog::NewImage(dialogs::NewImageState::new(&self.Settings)));
                    Ui.close();
                }
                if Ui.button("New Scene").clicked() {
                    self.Dialog = Some(Dialog::NewScene(Default::default()));
                    Ui.close();
                }
                Ui.menu_button("Filters", |Ui| {
                    for Chosen in Filter::catalogue() {
                        if Ui.button(Chosen.name()).clicked() {
                            if Chosen.has_settings() {
                                self.Dialog = Some(Dialog::Filter(dialogs::FilterState::new(Chosen)));
                            } else {
                                self.apply_filter(&Chosen);
                            }
                            Ui.close();
                        }
                    }
                });
                Ui.menu_button("Edit", |Ui| self.edit_clipboard_items(Ui));
                Ui.menu_button("Select", |Ui| {
                    if Ui.button("Select All").clicked() {
                        self.select_all();
                        Ui.close();
                    }
                    if Ui.button("Deselect").clicked() {
                        self.deselect();
                        Ui.close();
                    }
                    if Ui.button("Invert").clicked() {
                        self.invert_selection();
                        Ui.close();
                    }
                    if Ui.button("To New Layer").clicked() {
                        self.selection_to_layer();
                        Ui.close();
                    }
                });
                if Ui.button("Settings").clicked() {
                    self.ShowSettings = true;
                    Ui.close();
                }
                if Ui.button("About").clicked() {
                    self.Dialog = Some(Dialog::About);
                    Ui.close();
                }
            };
            match MenuIcon {
                Some(Icon) => {
                    Ui.menu_image_button(Icon, MenuContents);
                }
                None => {
                    Ui.menu_button("Menu", MenuContents);
                }
            }
            if crate::icons::enabled_button(Ui, self.History.can_undo(), "Undo", "Undo").clicked() {
                self.undo();
            }
            if crate::icons::enabled_button(Ui, self.History.can_redo(), "Redo", "Redo").clicked() {
                self.redo();
            }
            Ui.separator();
            if crate::icons::toggle_text(Ui, &mut self.ShowLeft, "Image", "Project").changed() && self.ShowLeft {
                self.ShowRight = false;
            }
            if crate::icons::toggle_text(Ui, &mut self.ShowRight, "Layers", "Panels").changed() && self.ShowRight {
                self.ShowLeft = false;
            }
            Ui.separator();
            crate::icons::show(Ui, self.Tool.icon_name());
            Ui.label(self.Tool.name());
            if let Some((Message, Time)) = &self.Status {
                if self.Now - Time < 4.0 {
                    Ui.separator();
                    Ui.label(egui::RichText::new(Message).weak());
                }
            }
        });
    }

    fn compact_sheets(&mut self, Context: &Context, Area: egui::Rect) {
        let Width = (Area.width() * 0.92).min(380.0);
        let Height = Area.height();
        let Sheet = |Name: &str, Left: bool| {
            let Position = if Left { Area.left_top() } else { egui::pos2(Area.right() - Width, Area.top()) };
            egui::Window::new(Name)
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .fixed_pos(Position)
                .fixed_size(egui::vec2(Width - 16.0, Height - 16.0))
        };
        if self.ShowLeft {
            let mut Close = false;
            Sheet("ProjectSheet", true).show(Context, |Ui| {
                Ui.horizontal(|Ui| {
                    Ui.heading("Project");
                    Ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |Ui| {
                        Close = crate::icons::large_button(Ui, "Close", false, "Close", 40.0).clicked();
                    });
                });
                egui::ScrollArea::vertical().id_salt("ProjectSheetScroll").show(Ui, |Ui| panels::project_browser(self, Ui));
            });
            if Close {
                self.ShowLeft = false;
            }
        }
        if self.ShowRight {
            let mut Close = false;
            Sheet("PanelSheet", false).show(Context, |Ui| {
                Ui.horizontal(|Ui| {
                    Ui.heading("Panels");
                    Ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |Ui| {
                        Close = crate::icons::large_button(Ui, "Close", false, "Close", 40.0).clicked();
                    });
                });
                panels::right_panel(self, Ui);
            });
            if Close {
                self.ShowRight = false;
            }
        }
    }

    fn compact_tool_bar(&mut self, Ui: &mut egui::Ui) {
        egui::ScrollArea::horizontal().show(Ui, |Ui| {
            Ui.horizontal(|Ui| {
                for Candidate in Tool::All {
                    let Selected = self.Tool == *Candidate;
                    if crate::icons::large_button(Ui, Candidate.icon_name(), Selected, Candidate.name(), 44.0).clicked() {
                        self.set_tool(*Candidate);
                    }
                }
                Ui.separator();
                panels::color_swatches(self, Ui);
            });
        });
    }
}

impl eframe::App for DespiteApp {
    fn raw_input_hook(&mut self, Context: &Context, RawInput: &mut egui::RawInput) {
        #[cfg(feature = "desktop")]
        crate::desktop::tablet_input(self, Context, RawInput);
        #[cfg(not(feature = "desktop"))]
        let _ = (Context, RawInput);
    }

    fn logic(&mut self, Context: &Context, _Frame: &mut eframe::Frame) {
        self.Now = Context.input(|Input| Input.time);
        self.update_layout_mode(Context);
        self.handle_dropped_files(Context);
        self.handle_close(Context);
        #[cfg(feature = "desktop")]
        crate::desktop::poll(self, Context);
        self.autosave();
        self.capture_view_prefs();
        if self.Settings != self.SettingsSnapshot {
            if self.Settings.UiScale != self.SettingsSnapshot.UiScale
                || self.Settings.Theme != self.SettingsSnapshot.Theme
                || self.Settings.AccentHue != self.SettingsSnapshot.AccentHue
            {
                self.apply_style(Context);
            }
            self.History.set_limit(self.Settings.UndoLimit);
            self.SettingsSnapshot = self.Settings.clone();
            self.SettingsDirtySince.get_or_insert(self.Now);
        }
        if self.SettingsDirtySince.is_some_and(|Since| self.Now - Since > 1.5) {
            self.Settings.save();
            self.SettingsDirtySince = None;
        }
        if self.SettingsDirtySince.is_some() {
            Context.request_repaint_after(std::time::Duration::from_millis(1600));
        }
    }

    fn ui(&mut self, Ui: &mut egui::Ui, _Frame: &mut eframe::Frame) {
        let Context = Ui.ctx().clone();
        if self.Screen == Screen::Home {
            crate::home::ui(self, Ui);
            dialogs::show(self, &Context);
            if self.ShowSettings {
                dialogs::settings_window(self, &Context);
            }
            self.update_title(&Context);
            return;
        }
        if self.Dialog.is_none() && !self.ShowSettings {
            self.handle_shortcuts(&Context);
        }
        if self.Compact {
            egui::Panel::top("CompactTop").show(Ui, |Ui| self.compact_top_bar(Ui));
            egui::Panel::bottom("CompactTools").show(Ui, |Ui| self.compact_tool_bar(Ui));
            egui::Panel::bottom("CompactOptions").show(Ui, |Ui| {
                egui::ScrollArea::horizontal().id_salt("CompactOptionsScroll").show(Ui, |Ui| panels::tool_options_bar(self, Ui));
            });
        } else {
            egui::Panel::top("MenuBar").show(Ui, |Ui| self.menu_bar(Ui));
            egui::Panel::top("ToolOptions").show(Ui, |Ui| panels::tool_options_bar(self, Ui));
            egui::Panel::bottom("StatusBar").show(Ui, |Ui| panels::status_bar(self, Ui));
        }
        #[cfg(feature = "desktop")]
        if self.ShowBottom && !self.Compact {
            egui::Panel::bottom("CodePanel").resizable(true).default_size(320.0).min_size(140.0).show(Ui, |Ui| crate::desktop::bottom_panel(self, Ui));
        }
        let SheetArea = Ui.available_rect_before_wrap();
        if self.Compact {
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(crate::view::to_color32(self.Settings.CanvasBackground))).show(Ui, |Ui| {
                crate::canvas::canvas_ui(self, Ui);
            });
            self.compact_sheets(&Context, SheetArea);
            dialogs::show(self, &Context);
            #[cfg(feature = "desktop")]
            crate::desktop::windows(self, &Context);
            if self.ShowSettings {
                dialogs::settings_window(self, &Context);
            }
            self.update_title(&Context);
            return;
        }
        if self.ShowLeft {
            let Width = 210.0;
            egui::Panel::left("LeftPanel").resizable(true).default_size(Width).min_size(150.0).show(Ui, |Ui| {
                egui::ScrollArea::vertical().show(Ui, |Ui| {
                    if !self.Compact {
                        panels::tool_palette(self, Ui);
                        Ui.separator();
                        panels::color_swatches(self, Ui);
                        Ui.separator();
                    }
                    panels::project_browser(self, Ui);
                });
            });
        }
        if self.ShowRight {
            let Width = 300.0;
            egui::Panel::right("RightPanel").resizable(true).default_size(Width).min_size(220.0).show(Ui, |Ui| panels::right_panel(self, Ui));
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(crate::view::to_color32(self.Settings.CanvasBackground))).show(Ui, |Ui| {
            crate::canvas::canvas_ui(self, Ui);
        });
        dialogs::show(self, &Context);
        #[cfg(feature = "desktop")]
        crate::desktop::windows(self, &Context);
        if self.ShowSettings {
            dialogs::settings_window(self, &Context);
        }
        self.update_title(&Context);
    }

    fn on_exit(&mut self) {
        #[cfg(feature = "desktop")]
        crate::desktop::shutdown(self);
        self.capture_view_prefs();
        self.Settings.save();
        self.Brushes.save_user();
    }
}

pub fn rotate_90(Source: &Raster, Clockwise: bool) -> Raster {
    let (Width, Height) = Source.size();
    let mut Out = Raster::new(Height, Width);
    for Y in 0..Height {
        for X in 0..Width {
            let (NewX, NewY) = if Clockwise { (Height - 1 - Y, X) } else { (Y, Width - 1 - X) };
            Out.put(NewX, NewY, Source.get(X, Y));
        }
    }
    Out
}

pub fn human_bytes(Bytes: u64) -> String {
    const Units: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut Value = Bytes as f64;
    let mut Unit = 0;
    while Value >= 1024.0 && Unit < Units.len() - 1 {
        Value /= 1024.0;
        Unit += 1;
    }
    if Unit == 0 { format!("{Bytes} B") } else { format!("{Value:.1} {}", Units[Unit]) }
}
