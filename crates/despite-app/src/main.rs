#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![allow(non_snake_case, non_upper_case_globals)]

fn main() -> eframe::Result<()> {
    despite_app::run_native(despite_app::LaunchOptions::default())
}
