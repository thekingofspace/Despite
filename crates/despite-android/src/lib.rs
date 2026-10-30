#![allow(non_snake_case, non_upper_case_globals)]

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(Application: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("Despite"));
    let DataDirectory = Application.internal_data_path();
    let DocumentsDirectory = Application.external_data_path().or_else(|| DataDirectory.clone()).map(|Path| Path.join("Despite"));
    let Launch = despite_app::LaunchOptions { DataDirectory, DocumentsDirectory, OpenFile: None, ForceTouchLayout: true };
    let mut Options = despite_app::native_options();
    Options.android_app = Some(Application);
    if let Err(Problem) = despite_app::run_with_options(Launch, Options) {
        log::error!("Despite stopped: {Problem}");
    }
}
