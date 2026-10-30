mod code;
mod compile;
#[cfg(target_os = "linux")]
mod tablet;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use despite_core::params::{ParamDecl, ParamKind, ParamValue};
use despite_core::{Color, CompileMode, ProjectScript, ProjectShader};
use despite_plugin::PluginRegistry;
use egui::{RichText, Ui};

use crate::app::DespiteApp;
use crate::settings::Settings;

pub use code::bottom_panel;
pub use compile::{CompiledResult, start_video_export, windows};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeTab {
    Python,
    Shaders,
    Console,
}

pub struct RunningScript {
    pub Handle: despite_script::ScriptHandle,
    pub Name: String,
    pub Started: f64,
}

pub struct ShaderWorker {
    pub Jobs: Sender<despite_shader::ShaderJob>,
    pub Results: Receiver<Result<despite_shader::ShaderOutput, despite_shader::ShaderError>>,
    pub Adapter: Arc<Mutex<String>>,
}

pub struct VideoJob {
    pub Target: PathBuf,
    pub Progress: Arc<Mutex<f32>>,
    pub Cancel: Arc<AtomicBool>,
    pub Handle: Option<std::thread::JoinHandle<Result<(), String>>>,
}

pub struct ConsoleLine {
    pub Error: bool,
    pub Text: String,
}

pub struct DesktopState {
    pub Plugins: PluginRegistry,
    pub Tab: CodeTab,
    pub ActiveScript: usize,
    pub ActiveShader: usize,
    pub Running: Option<RunningScript>,
    pub Console: Vec<ConsoleLine>,
    pub ErrorLine: Option<u32>,
    pub Compiled: Option<CompiledResult>,
    pub ShowPreview: bool,
    pub Worker: Option<ShaderWorker>,
    pub ShaderPending: Option<(String, CompileMode, u32, f32)>,
    pub ShaderStatus: Option<Result<String, despite_shader::ShaderError>>,
    pub LastValidated: String,
    pub ShowPrelude: bool,
    pub Trusted: bool,
    pub AskTrust: bool,
    pub Video: Option<VideoJob>,
    pub ShowPlugins: bool,
    pub Interpreter: Option<despite_script::InterpreterInfo>,
    pub InterpreterChecked: bool,
    pub RunRequested: bool,
    pub PendingJob: Option<despite_shader::ShaderJob>,
    #[cfg(target_os = "linux")]
    pub Tablet: Option<tablet::TabletInput>,
    pub TabletStatus: String,
}

impl DesktopState {
    pub fn new(Settings: &Settings) -> Self {
        Self {
            Plugins: scan_plugins(Settings),
            Tab: CodeTab::Python,
            ActiveScript: 0,
            ActiveShader: 0,
            Running: None,
            Console: Vec::new(),
            ErrorLine: None,
            Compiled: None,
            ShowPreview: false,
            Worker: None,
            ShaderPending: None,
            ShaderStatus: None,
            LastValidated: String::new(),
            ShowPrelude: false,
            Trusted: true,
            AskTrust: false,
            Video: None,
            ShowPlugins: false,
            Interpreter: None,
            InterpreterChecked: false,
            RunRequested: false,
            PendingJob: None,
            #[cfg(target_os = "linux")]
            Tablet: None,
            TabletStatus: String::new(),
        }
    }

    pub fn log(&mut self, Error: bool, Text: impl Into<String>) {
        self.Console.push(ConsoleLine { Error, Text: Text.into() });
        if self.Console.len() > 5000 {
            let Excess = self.Console.len() - 5000;
            self.Console.drain(0..Excess);
        }
    }
}

pub fn plugin_folders(Settings: &Settings) -> Vec<PathBuf> {
    let mut Folders = Vec::new();
    if let Some(Default) = despite_plugin::default_plugin_dir() {
        Folders.push(Default);
    }
    Folders.extend(Settings.PluginFolders.iter().cloned());
    for Candidate in [PathBuf::from("plugins"), std::env::current_exe().ok().and_then(|Exe| Exe.parent().map(|Parent| Parent.join("plugins"))).unwrap_or_default()] {
        if Candidate.is_dir() && !Folders.contains(&Candidate) {
            Folders.push(Candidate);
        }
    }
    Folders
}

fn scan_plugins(Settings: &Settings) -> PluginRegistry {
    let mut Registry = despite_plugin::scan(&plugin_folders(Settings));
    Registry.Plugins.retain(|Plugin| !Settings.DisabledPlugins.contains(&Plugin.Info.Id));
    Registry
}

pub fn reload_plugins(App: &mut DespiteApp) {
    let Registry = scan_plugins(&App.Settings);
    App.Brushes.Plugin = Registry.brushes().map(|(_, Entry)| Entry.clone()).collect();
    let Count = Registry.Plugins.len();
    App.Desktop.Plugins = Registry;
    App.set_status(format!("Loaded {Count} plugin(s)"));
}

pub fn after_start(App: &mut DespiteApp, Creation: &eframe::CreationContext<'_>) {
    App.Brushes.Plugin = App.Desktop.Plugins.brushes().map(|(_, Entry)| Entry.clone()).collect();
    #[cfg(target_os = "linux")]
    {
        if App.Settings.UseTabletInput {
            match tablet::TabletInput::connect(Creation) {
                Ok(Connected) => {
                    App.Desktop.TabletStatus = "Wayland tablet input active".into();
                    App.Desktop.Tablet = Some(Connected);
                }
                Err(Problem) => App.Desktop.TabletStatus = format!("Tablet API unavailable ({Problem}); pressure comes from touch events only"),
            }
        } else {
            App.Desktop.TabletStatus = "Tablet input disabled in Settings".into();
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = Creation;
        App.Desktop.TabletStatus = if cfg!(windows) { "Windows pen pressure is read from pointer events".into() } else { "Pen pressure comes from touch events".into() };
    }
}

pub fn tablet_input(App: &mut DespiteApp, Context: &egui::Context, RawInput: &mut egui::RawInput) {
    #[cfg(target_os = "linux")]
    if let Some(Input) = &mut App.Desktop.Tablet {
        App.PenTilt = Input.inject(Context, RawInput);
        return;
    }
    let _ = (App, Context, RawInput);
}

pub fn shutdown(App: &mut DespiteApp) {
    #[cfg(target_os = "linux")]
    {
        App.Desktop.Tablet = None;
    }
    let _ = App;
}

pub fn project_changed(App: &mut DespiteApp) {
    App.Desktop.ActiveScript = 0;
    App.Desktop.ActiveShader = 0;
    App.Desktop.Compiled = None;
    App.Desktop.ShowPreview = false;
    App.Desktop.ErrorLine = None;
    App.Desktop.Trusted = true;
    App.Desktop.LastValidated.clear();
}

pub fn project_opened(App: &mut DespiteApp) {
    App.Desktop.Trusted = App.Settings.TrustProjectScripts || App.Project.Scripts.is_empty();
}

pub fn interpreter(App: &mut DespiteApp) -> Option<PathBuf> {
    if let Some(Explicit) = App.Settings.PythonPath.clone().filter(|Path| !Path.as_os_str().is_empty()) {
        return Some(Explicit);
    }
    if !App.Desktop.InterpreterChecked {
        App.Desktop.InterpreterChecked = true;
        App.Desktop.Interpreter = despite_script::find_interpreter_info();
    }
    App.Desktop.Interpreter.as_ref().map(|Info| Info.Executable.clone())
}

pub fn poll(App: &mut DespiteApp, Context: &egui::Context) {
    compile::poll_script(App, Context);
    compile::poll_shader(App, Context);
    compile::poll_video(App, Context);
}

fn example_shader_params(Name: &str) -> Vec<ParamDecl> {
    let Float = |Name: &str, Min: f32, Max: f32, Value: f32| ParamDecl::new(Name, ParamKind::Float { Min, Max }).with_value(ParamValue::Float(Value));
    let Colour = |Name: &str, Value: Color| ParamDecl::new(Name, ParamKind::Color).with_value(ParamValue::Color(Value));
    let Flag = |Name: &str, Value: bool| ParamDecl::new(Name, ParamKind::Bool).with_value(ParamValue::Bool(Value));
    match Name {
        "Glow" => vec![Float("Radius", 0.0, 32.0, 6.0), Float("Intensity", 0.0, 4.0, 1.2), Colour("GlowColor", Color::rgb(1.0, 0.8, 0.4)), Flag("Pulse", true)],
        "Chromatic Aberration" => vec![
            Float("Amount", 0.0, 16.0, 3.0),
            ParamDecl::new("Direction", ParamKind::Vec2).with_value(ParamValue::Vec2([1.0, 0.0])),
            Float("Wobble", 0.0, 1.0, 0.3),
        ],
        "Dissolve" => vec![
            Float("Amount", 0.0, 1.0, 0.5),
            Flag("Animate", true),
            Float("EdgeWidth", 0.0, 0.3, 0.05),
            Colour("EdgeColor", Color::rgb(1.0, 0.5, 0.1)),
            Float("NoiseScale", 0.1, 8.0, 1.0),
            ParamDecl::new("Noise", ParamKind::Image),
        ],
        "Pixel Outline" => vec![
            Colour("OutlineColor", Color::Black),
            ParamDecl::new("Thickness", ParamKind::Int { Min: 1, Max: 8 }).with_value(ParamValue::Int(1)),
            Flag("Diagonal", false),
        ],
        "Fire VFX" => vec![Float("Intensity", 0.0, 3.0, 1.0), Float("Speed", 0.0, 4.0, 1.0), Float("Height", 0.0, 1.0, 0.5)],
        _ => Vec::new(),
    }
}

pub const ShaderExamples: &[(&str, &str)] = &[
    ("Glow", include_str!("../../../despite-shader/examples/shaders/glow.wgsl")),
    ("Chromatic Aberration", include_str!("../../../despite-shader/examples/shaders/chromatic_aberration.wgsl")),
    ("Dissolve", include_str!("../../../despite-shader/examples/shaders/dissolve_with_noise_input.wgsl")),
    ("Pixel Outline", include_str!("../../../despite-shader/examples/shaders/pixel_outline.wgsl")),
    ("Fire VFX", include_str!("../../../despite-shader/examples/shaders/fire_vfx.wgsl")),
];

pub fn add_script(App: &mut DespiteApp, Name: &str, Source: &str, Mode: CompileMode, Params: Vec<ParamDecl>) {
    App.record("New script");
    let Id = App.Project.alloc_id();
    App.Project.Scripts.push(ProjectScript { Id, Name: Name.to_string(), Source: Source.to_string(), Mode, Params, TimeoutSeconds: App.Settings.ScriptTimeoutSeconds });
    App.Desktop.ActiveScript = App.Project.Scripts.len() - 1;
    App.Desktop.Tab = CodeTab::Python;
    App.ShowBottom = true;
}

pub fn add_shader(App: &mut DespiteApp, Name: &str, Source: &str, Params: Vec<ParamDecl>, FrameCount: u32, Fps: f32) {
    App.record("New shader");
    let Id = App.Project.alloc_id();
    App.Project.Shaders.push(ProjectShader {
        Id,
        Name: Name.to_string(),
        Source: Source.to_string(),
        Params,
        FrameCount,
        Fps,
        Mode: if FrameCount > 1 { CompileMode::Gif } else { CompileMode::Image },
        ..Default::default()
    });
    App.Desktop.ActiveShader = App.Project.Shaders.len() - 1;
    App.Desktop.Tab = CodeTab::Shaders;
    App.Desktop.LastValidated.clear();
    App.ShowBottom = true;
}

pub fn menus(App: &mut DespiteApp, Ui: &mut Ui) {
    Ui.menu_button("Code", |Ui| {
        Ui.label(RichText::new("Python project code").strong());
        if Ui.button("New Python script").clicked() {
            add_script(App, "Script", despite_script::example_script("starter").unwrap_or(""), CompileMode::Image, Vec::new());
            Ui.close();
        }
        Ui.menu_button("Python examples", |Ui| {
            for (Name, Source) in despite_script::ExampleScripts {
                if Ui.button(*Name).clicked() {
                    let Mode = match *Name {
                        "glow_animation" => CompileMode::Gif,
                        "scene_compose" => CompileMode::Scene,
                        _ => CompileMode::Image,
                    };
                    add_script(App, Name, Source, Mode, Vec::new());
                    Ui.close();
                }
            }
        });
        if Ui.button("Run current script (F5)").clicked() {
            compile::run_script(App);
            Ui.close();
        }
        Ui.separator();
        Ui.label(RichText::new("Shaders / VFX").strong());
        if Ui.button("New shader").clicked() {
            add_shader(App, "Shader", despite_shader::DefaultEffectSource, Vec::new(), 1, 12.0);
            Ui.close();
        }
        Ui.menu_button("Shader examples", |Ui| {
            for (Name, Source) in ShaderExamples {
                if Ui.button(*Name).clicked() {
                    let Animated = matches!(*Name, "Fire VFX" | "Glow" | "Dissolve");
                    add_shader(App, Name, Source, example_shader_params(Name), if Animated { 24 } else { 1 }, 12.0);
                    Ui.close();
                }
            }
        });
        if Ui.button("Run current shader (F6)").clicked() {
            compile::run_shader(App);
            Ui.close();
        }
        Ui.separator();
        if Ui.button("Show code panel").clicked() {
            App.ShowBottom = true;
            Ui.close();
        }
        if App.Desktop.Compiled.is_some() && Ui.button("Show last result").clicked() {
            App.Desktop.ShowPreview = true;
            Ui.close();
        }
    });
    Ui.menu_button("Plugins", |Ui| {
        let Shaders: Vec<(String, String, despite_plugin::PluginShader)> =
            App.Desktop.Plugins.shaders().map(|(Plugin, Shader)| (Plugin.Info.Name.clone(), Shader.Category.clone(), Shader.clone())).collect();
        let Scripts: Vec<(String, String, despite_plugin::PluginScript)> =
            App.Desktop.Plugins.scripts().map(|(Plugin, Script)| (Plugin.Info.Name.clone(), Script.Category.clone(), Script.clone())).collect();
        if Shaders.is_empty() && Scripts.is_empty() {
            Ui.label(RichText::new("No plugin effects or tools installed").weak());
        }
        if !Shaders.is_empty() {
            Ui.menu_button("Effects (shaders)", |Ui| {
                for (PluginName, Category, Shader) in &Shaders {
                    let Label = if Category.is_empty() { format!("{} — {PluginName}", Shader.Name) } else { format!("{} / {} — {PluginName}", Category, Shader.Name) };
                    if Ui.button(Label).on_hover_text(&Shader.Description).clicked() {
                        add_shader(App, &Shader.Name, &Shader.Source, Shader.Params.clone(), Shader.FrameCount, Shader.Fps);
                        if let Some(Added) = App.Project.Shaders.last_mut() {
                            Added.Smooth = Shader.Smooth;
                        }
                        compile::run_shader(App);
                        Ui.close();
                    }
                }
            });
        }
        if !Scripts.is_empty() {
            Ui.menu_button("Tools (Python)", |Ui| {
                for (PluginName, Category, Script) in &Scripts {
                    let Label = if Category.is_empty() { format!("{} — {PluginName}", Script.Name) } else { format!("{} / {} — {PluginName}", Category, Script.Name) };
                    if Ui.button(Label).on_hover_text(&Script.Description).clicked() {
                        add_script(App, &Script.Name, &Script.Source, Script.Mode, Script.Params.clone());
                        Ui.close();
                    }
                }
            });
        }
        Ui.separator();
        if Ui.button("Manage plugins…").clicked() {
            App.Desktop.ShowPlugins = true;
            Ui.close();
        }
        if Ui.button("Reload plugins").clicked() {
            reload_plugins(App);
            Ui.close();
        }
    });
}

pub fn plugins_window(App: &mut DespiteApp, Context: &egui::Context) {
    let mut Open = App.Desktop.ShowPlugins;
    egui::Window::new("Plugins").open(&mut Open).default_width(520.0).show(Context, |Ui| {
        Ui.label("Plugins are folders with a plugin.toml that add shader effects, Python tools, brushes and palettes.");
        Ui.horizontal_wrapped(|Ui| {
            if let Some(Folder) = despite_plugin::default_plugin_dir() {
                if Ui.button("Open plugin folder").clicked() {
                    crate::platform::ensure_dir(&Folder);
                    crate::platform::open_in_file_manager(&Folder);
                }
                if Ui.button("Create plugin template").clicked() {
                    let Target = Folder.join("my-plugin");
                    match despite_plugin::create_template(&Target) {
                        Ok(()) => {
                            App.set_status(format!("Template created in {}", Target.display()));
                            reload_plugins(App);
                        }
                        Err(Problem) => App.set_status(Problem.to_string()),
                    }
                }
                if Ui.button("Install from folder…").clicked()
                    && let Some(Source) = crate::platform::dialogs::pick_folder() {
                        crate::platform::ensure_dir(&Folder);
                        match despite_plugin::install_from_folder(&Source, &Folder) {
                            Ok(Path) => {
                                App.set_status(format!("Installed to {}", Path.display()));
                                reload_plugins(App);
                            }
                            Err(Problem) => App.set_status(Problem.to_string()),
                        }
                    }
            }
            if Ui.button("Reload").clicked() {
                reload_plugins(App);
            }
        });
        Ui.separator();
        let Plugins: Vec<(String, String, String, String, usize, usize, usize, usize, Vec<String>)> = App
            .Desktop
            .Plugins
            .Plugins
            .iter()
            .map(|Plugin| {
                (
                    Plugin.Info.Id.clone(),
                    Plugin.Info.Name.clone(),
                    Plugin.Info.Version.clone(),
                    Plugin.Info.Description.clone(),
                    Plugin.Shaders.len(),
                    Plugin.Scripts.len(),
                    Plugin.Brushes.len(),
                    Plugin.Palettes.len(),
                    Plugin.Warnings.clone(),
                )
            })
            .collect();
        for (Id, Name, Version, Description, Shaders, Scripts, Brushes, Palettes, Warnings) in Plugins {
            egui::Frame::group(Ui.style()).show(Ui, |Ui| {
                Ui.horizontal(|Ui| {
                    Ui.label(RichText::new(format!("{Name} {Version}")).strong());
                    if Ui.button("Disable").clicked() {
                        App.Settings.DisabledPlugins.push(Id.clone());
                        reload_plugins(App);
                    }
                });
                if !Description.is_empty() {
                    Ui.label(Description);
                }
                Ui.label(RichText::new(format!("{Shaders} effects · {Scripts} tools · {Brushes} brushes · {Palettes} palettes")).weak());
                for Warning in Warnings {
                    Ui.colored_label(egui::Color32::from_rgb(230, 170, 80), Warning);
                }
            });
        }
        if !App.Settings.DisabledPlugins.is_empty() {
            Ui.separator();
            Ui.label("Disabled:");
            let mut Enable = None;
            for (Index, Id) in App.Settings.DisabledPlugins.iter().enumerate() {
                Ui.horizontal(|Ui| {
                    Ui.label(Id);
                    if Ui.small_button("Enable").clicked() {
                        Enable = Some(Index);
                    }
                });
            }
            if let Some(Index) = Enable {
                App.Settings.DisabledPlugins.remove(Index);
                reload_plugins(App);
            }
        }
        let Errors: Vec<(PathBuf, String)> = App.Desktop.Plugins.Errors.clone();
        if !Errors.is_empty() {
            Ui.separator();
            Ui.label(RichText::new("Plugins that failed to load").strong());
            for (Path, Problem) in Errors {
                Ui.colored_label(egui::Color32::from_rgb(230, 110, 90), format!("{}: {Problem}", Path.display()));
            }
        }
    });
    App.Desktop.ShowPlugins = Open;
}

pub fn settings_sections(Ui: &mut Ui, Settings: &mut Settings) {
    egui::CollapsingHeader::new(RichText::new("Python (project code)").strong()).show(Ui, |Ui| {
        let mut Path = Settings.PythonPath.as_ref().map(|Found| Found.display().to_string()).unwrap_or_default();
        Ui.horizontal(|Ui| {
            Ui.label("Interpreter");
            if Ui.add(egui::TextEdit::singleline(&mut Path).hint_text("auto-detect")).changed() {
                Settings.PythonPath = (!Path.trim().is_empty()).then(|| PathBuf::from(Path.trim()));
            }
            if Ui.button("Browse…").clicked()
                && let Some(Found) = rfd::FileDialog::new().pick_file() {
                    Settings.PythonPath = Some(Found);
                }
        });
        if Ui.button("Detect Python").clicked() {
            match despite_script::find_interpreter_info() {
                Some(Info) => {
                    Settings.PythonPath = Some(Info.Executable.clone());
                }
                None => Settings.PythonPath = None,
            }
        }
        if let Some(Found) = &Settings.PythonPath {
            match despite_script::probe_interpreter(Found) {
                Some(Info) => {
                    Ui.label(format!("Python {} · numpy {} · Pillow {}", Info.Version, if Info.HasNumpy { "yes" } else { "no" }, if Info.HasPillow { "yes" } else { "no" }));
                }
                None => {
                    Ui.colored_label(egui::Color32::from_rgb(230, 110, 90), "This path is not a working Python 3 interpreter");
                }
            }
        }
        Ui.add(egui::Slider::new(&mut Settings.ScriptTimeoutSeconds, 1..=3600).text("default timeout (s)"));
        Ui.checkbox(&mut Settings.TrustProjectScripts, "Run scripts in opened projects without asking");
        Ui.label(RichText::new("Python code runs with your user's permissions. Only run projects you trust.").weak().small());
    });
    egui::CollapsingHeader::new(RichText::new("Drawing tablet").strong()).show(Ui, |Ui| {
        Ui.checkbox(&mut Settings.UseTabletInput, "Read pen tablets directly (Wayland; restart to apply)");
        Ui.label(RichText::new("Windows pens, iPad/iPhone Apple Pencil and Android styluses report pressure automatically. On Linux Wayland, Despite talks to the tablet protocol for pressure and tilt. X11 and macOS tablets currently behave like a mouse; enable 'Simulate pressure from mouse speed' under Painting for tapered lines.").weak().small());
    });
    egui::CollapsingHeader::new(RichText::new("Shaders").strong()).show(Ui, |Ui| {
        Ui.add(egui::Slider::new(&mut Settings.ShaderMaxFrames, 1..=2000).text("max frames per run"));
        Ui.checkbox(&mut Settings.ShaderLiveValidate, "Check shader code while typing");
        Ui.add(egui::Slider::new(&mut Settings.PreviewFps, 1.0..=60.0).text("preview fps cap"));
    });
    egui::CollapsingHeader::new(RichText::new("Video (ffmpeg)").strong()).show(Ui, |Ui| {
        let mut Path = Settings.FfmpegPath.as_ref().map(|Found| Found.display().to_string()).unwrap_or_default();
        Ui.horizontal(|Ui| {
            Ui.label("ffmpeg");
            if Ui.add(egui::TextEdit::singleline(&mut Path).hint_text("auto-detect")).changed() {
                Settings.FfmpegPath = (!Path.trim().is_empty()).then(|| PathBuf::from(Path.trim()));
            }
            if Ui.button("Browse…").clicked()
                && let Some(Found) = rfd::FileDialog::new().pick_file() {
                    Settings.FfmpegPath = Some(Found);
                }
        });
        match despite_video::find_ffmpeg(Settings.FfmpegPath.as_deref()) {
            Some(Found) => Ui.label(format!("Using {}", Found.display())),
            None => Ui.colored_label(egui::Color32::from_rgb(230, 110, 90), "ffmpeg not found — video export disabled"),
        };
        Ui.add(egui::Slider::new(&mut Settings.VideoQuality, 0..=51).text("default CRF (lower = better)"));
    });
    egui::CollapsingHeader::new(RichText::new("Plugins").strong()).show(Ui, |Ui| {
        for Folder in plugin_folders(Settings) {
            Ui.label(Folder.display().to_string());
        }
        let mut Remove = None;
        for (Index, Folder) in Settings.PluginFolders.iter().enumerate() {
            Ui.horizontal(|Ui| {
                Ui.label(Folder.display().to_string());
                if Ui.small_button("Remove").clicked() {
                    Remove = Some(Index);
                }
            });
        }
        if let Some(Index) = Remove {
            Settings.PluginFolders.remove(Index);
        }
        if Ui.button("Add plugin folder…").clicked()
            && let Some(Found) = rfd::FileDialog::new().pick_folder() {
                Settings.PluginFolders.push(Found);
            }
        Ui.label(RichText::new("Use Plugins → Reload after changing folders.").weak().small());
    });
}

pub fn cancel_flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
