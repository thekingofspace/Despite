#![allow(non_snake_case, non_upper_case_globals)]

mod app;
mod canvas;
mod clipboard;
#[cfg(feature = "desktop")]
mod desktop;
mod dialogs;
mod home;
mod icons;
mod panels;
mod platform;
mod settings;
mod tools;
mod transform;
mod view;

use std::path::PathBuf;

pub use app::DespiteApp;

#[derive(Clone, Debug, Default)]
pub struct LaunchOptions {
    pub DataDirectory: Option<PathBuf>,
    pub DocumentsDirectory: Option<PathBuf>,
    pub OpenFile: Option<PathBuf>,
    pub ForceTouchLayout: bool,
}

fn requested_window_size() -> [f32; 2] {
    std::env::var("DESPITE_WINDOW")
        .ok()
        .and_then(|Text| {
            let (Width, Height) = Text.split_once('x')?;
            Some([Width.trim().parse().ok()?, Height.trim().parse().ok()?])
        })
        .unwrap_or([1440.0, 900.0])
}

fn window_icon() -> Option<egui::IconData> {
    let Bytes = include_bytes!("../../../assets/icon/despite-256.png");
    let Pixels = despite_core::Raster::decode(Bytes).ok()?;
    Some(egui::IconData { rgba: Pixels.data().to_vec(), width: Pixels.width(), height: Pixels.height() })
}

pub fn native_options() -> eframe::NativeOptions {
    let mut Viewport = egui::ViewportBuilder::default();
    if let Some(Icon) = window_icon() {
        Viewport = Viewport.with_icon(std::sync::Arc::new(Icon));
    }
    eframe::NativeOptions {
        viewport: Viewport
            .with_title("Despite")
            .with_app_id("despite")
            .with_inner_size(requested_window_size())
            .with_min_inner_size([320.0, 360.0])
            .with_drag_and_drop(true),
        ..Default::default()
    }
}

pub fn run_with_options(Launch: LaunchOptions, Options: eframe::NativeOptions) -> eframe::Result<()> {
    platform::configure(&Launch);
    eframe::run_native("Despite", Options, Box::new(move |Creation| Ok(Box::new(DespiteApp::new(Creation, Launch)))))
}

pub fn run_native(mut Launch: LaunchOptions) -> eframe::Result<()> {
    #[cfg(feature = "desktop")]
    {
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).try_init();
    }
    if let Some(Directory) = std::env::var_os("DESPITE_DATA_DIR") {
        Launch.DataDirectory = Some(PathBuf::from(Directory));
    }
    if std::env::var("DESPITE_TOUCH").is_ok_and(|Value| Value == "1") {
        Launch.ForceTouchLayout = true;
    }
    if Launch.OpenFile.is_none() {
        Launch.OpenFile = std::env::args_os().nth(1).map(PathBuf::from).filter(|Path| Path.is_file());
    }
    run_with_options(Launch, native_options())
}
