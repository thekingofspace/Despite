use std::path::PathBuf;

use despite_core::brush::{Brush, SymmetryMode};
use despite_core::export::{GifOptions, SheetOptions};
use despite_core::{Color, OnionSkin, ResampleMethod};
use serde::{Deserialize, Serialize};

use crate::platform;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutMode {
    #[default]
    Auto,
    Desktop,
    Touch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScrollAction {
    #[default]
    Zoom,
    Pan,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewPrefs {
    pub ShowLeft: bool,
    pub ShowRight: bool,
    pub ShowBottom: bool,
    pub RightTab: crate::panels::RightTab,
    pub Wrap: bool,
    pub SymmetryMode: SymmetryMode,
    pub SymmetrySegments: u32,
    pub SymmetryGuide: bool,
    pub OnionSkin: OnionSkin,
    pub BoilPreview: bool,
    pub Tool: Option<crate::tools::Tool>,
    pub ToolOptions: crate::tools::ToolOptions,
    pub BrushTool: Option<Brush>,
    pub PencilTool: Option<Brush>,
    pub EraserTool: Option<Brush>,
    pub Primary: Color,
    pub Secondary: Color,
}

impl Default for ViewPrefs {
    fn default() -> Self {
        Self {
            ShowLeft: true,
            ShowRight: true,
            ShowBottom: false,
            RightTab: crate::panels::RightTab::Layers,
            Wrap: false,
            SymmetryMode: SymmetryMode::None,
            SymmetrySegments: 6,
            SymmetryGuide: true,
            OnionSkin: OnionSkin::default(),
            BoilPreview: true,
            Tool: None,
            ToolOptions: crate::tools::ToolOptions::default(),
            BrushTool: None,
            PencilTool: None,
            EraserTool: None,
            Primary: Color::Black,
            Secondary: Color::White,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub View: ViewPrefs,
    pub Layout: LayoutMode,
    pub Theme: ThemeChoice,
    pub UiScale: f32,
    pub AccentHue: f32,
    pub ConfirmUnsaved: bool,
    pub AutosaveMinutes: u32,
    pub RecentFiles: Vec<PathBuf>,
    pub RecentLimit: usize,
    pub HomeGridView: bool,
    pub HomeSortByName: bool,
    pub HomeCardSize: f32,
    pub HomeShowAutosaves: bool,

    pub CheckerSize: u32,
    pub CheckerLight: Color,
    pub CheckerDark: Color,
    pub CanvasBackground: Color,
    pub PixelGrid: bool,
    pub PixelGridMinZoom: f32,
    pub PixelGridColor: Color,
    pub TileGrid: bool,
    pub TileGridWidth: u32,
    pub TileGridHeight: u32,
    pub TileGridColor: Color,
    pub TilePreview: bool,
    pub SmoothZoomedOut: bool,
    pub ScrollWheel: ScrollAction,
    pub ZoomStep: f32,
    pub SmoothZoom: bool,
    pub InvertScroll: bool,
    pub ShowBrushOutline: bool,
    pub ShowCrosshair: bool,
    pub MarchingAntsSpeed: f32,
    pub CanvasBorder: bool,

    pub UsePressure: bool,
    pub MouseSpeedPressure: bool,
    pub UseTabletInput: bool,
    pub PressureCurve: f32,
    pub AltClickPicksColor: bool,
    pub RightClickSecondary: bool,
    pub TwoFingerUndo: bool,
    pub ShapeSnapDegrees: f32,

    pub UndoLimit: usize,
    pub SaveHistoryInProject: bool,
    pub LoadHistoryFromProject: bool,

    pub DefaultWidth: u32,
    pub DefaultHeight: u32,
    pub DefaultPixelArt: bool,
    pub DefaultBackground: Option<Color>,

    pub ExportScale: f32,
    pub ExportMethod: ResampleMethod,
    pub Gif: GifOptions,
    pub Sheet: SheetOptions,
    pub WriteSheetJson: bool,
    pub ExportFolder: Option<PathBuf>,

    pub PythonPath: Option<PathBuf>,
    pub ScriptTimeoutSeconds: u32,
    pub TrustProjectScripts: bool,

    pub ShaderMaxFrames: u32,
    pub ShaderLiveValidate: bool,
    pub PreviewFps: f32,

    pub FfmpegPath: Option<PathBuf>,
    pub VideoQuality: u32,

    pub PluginFolders: Vec<PathBuf>,
    pub DisabledPlugins: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            View: ViewPrefs::default(),
            Layout: LayoutMode::Auto,
            Theme: ThemeChoice::Dark,
            UiScale: if platform::IsMobile { 1.35 } else { 1.0 },
            AccentHue: 265.0,
            ConfirmUnsaved: true,
            AutosaveMinutes: 5,
            RecentFiles: Vec::new(),
            RecentLimit: 12,
            HomeGridView: true,
            HomeSortByName: false,
            HomeCardSize: 170.0,
            HomeShowAutosaves: false,

            CheckerSize: 8,
            CheckerLight: Color::rgb(0.80, 0.80, 0.80),
            CheckerDark: Color::rgb(0.62, 0.62, 0.62),
            CanvasBackground: Color::rgb(0.13, 0.13, 0.15),
            PixelGrid: true,
            PixelGridMinZoom: 8.0,
            PixelGridColor: Color::rgba(0.0, 0.0, 0.0, 0.25),
            TileGrid: false,
            TileGridWidth: 16,
            TileGridHeight: 16,
            TileGridColor: Color::rgba(0.2, 0.6, 1.0, 0.5),
            TilePreview: false,
            SmoothZoomedOut: true,
            ScrollWheel: ScrollAction::Zoom,
            ZoomStep: 1.2,
            SmoothZoom: false,
            InvertScroll: false,
            ShowBrushOutline: true,
            ShowCrosshair: false,
            MarchingAntsSpeed: 12.0,
            CanvasBorder: true,

            UsePressure: true,
            MouseSpeedPressure: false,
            UseTabletInput: true,
            PressureCurve: 1.0,
            AltClickPicksColor: true,
            RightClickSecondary: true,
            TwoFingerUndo: true,
            ShapeSnapDegrees: 15.0,

            UndoLimit: 80,
            SaveHistoryInProject: false,
            LoadHistoryFromProject: true,

            DefaultWidth: 64,
            DefaultHeight: 64,
            DefaultPixelArt: true,
            DefaultBackground: None,

            ExportScale: 1.0,
            ExportMethod: ResampleMethod::Nearest,
            Gif: GifOptions::default(),
            Sheet: SheetOptions::default(),
            WriteSheetJson: true,
            ExportFolder: None,

            PythonPath: None,
            ScriptTimeoutSeconds: 60,
            TrustProjectScripts: false,

            ShaderMaxFrames: 240,
            ShaderLiveValidate: false,
            PreviewFps: 12.0,

            FfmpegPath: None,
            VideoQuality: 18,

            PluginFolders: Vec::new(),
            DisabledPlugins: Vec::new(),
        }
    }
}

impl Settings {
    pub fn load() -> Settings {
        let Path = platform::settings_path();
        match std::fs::read_to_string(&Path) {
            Ok(Text) => serde_json::from_str(&Text).unwrap_or_else(|Problem| {
                log::warn!("settings file {} is invalid ({Problem}); using defaults", Path.display());
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self) {
        let Path = platform::settings_path();
        if let Some(Parent) = Path.parent() {
            platform::ensure_dir(Parent);
        }
        match serde_json::to_string_pretty(self) {
            Ok(Text) => {
                if let Err(Problem) = std::fs::write(&Path, Text) {
                    log::warn!("could not save settings: {Problem}");
                }
            }
            Err(Problem) => log::warn!("could not encode settings: {Problem}"),
        }
    }

    pub fn push_recent(&mut self, Path: PathBuf) {
        self.RecentFiles.retain(|Existing| *Existing != Path);
        self.RecentFiles.insert(0, Path);
        self.RecentFiles.truncate(self.RecentLimit.max(1));
    }

    pub fn export_folder(&self) -> PathBuf {
        self.ExportFolder.clone().unwrap_or_else(platform::exports_dir)
    }
}
