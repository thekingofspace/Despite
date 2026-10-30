use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::LaunchOptions;

static DataRoot: OnceLock<PathBuf> = OnceLock::new();
static DocumentsRoot: OnceLock<PathBuf> = OnceLock::new();

pub const IsMobile: bool = cfg!(any(target_os = "android", target_os = "ios"));

pub fn configure(Launch: &LaunchOptions) {
    if let Some(Directory) = &Launch.DataDirectory {
        let _ = DataRoot.set(Directory.clone());
    }
    if let Some(Directory) = &Launch.DocumentsDirectory {
        let _ = DocumentsRoot.set(Directory.clone());
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub fn data_dir() -> PathBuf {
    if let Some(Found) = DataRoot.get() {
        return Found.clone();
    }
    let Base = if cfg!(target_os = "ios") {
        home().map(|Home| Home.join("Library").join("Application Support"))
    } else {
        dirs::data_dir()
    };
    Base.unwrap_or_else(std::env::temp_dir).join("Despite")
}

pub fn config_dir() -> PathBuf {
    if DataRoot.get().is_some() || IsMobile {
        return data_dir();
    }
    dirs::config_dir().map(|Base| Base.join("Despite")).unwrap_or_else(data_dir)
}

pub fn documents_dir() -> PathBuf {
    if let Some(Found) = DocumentsRoot.get() {
        return Found.clone();
    }
    let Base = if cfg!(target_os = "ios") { home().map(|Home| Home.join("Documents")) } else { dirs::document_dir().map(|Base| Base.join("Despite")) };
    Base.unwrap_or_else(data_dir)
}

pub fn exports_dir() -> PathBuf {
    documents_dir().join("Exports")
}

pub fn imports_dir() -> PathBuf {
    documents_dir().join("Import")
}

pub fn autosave_dir() -> PathBuf {
    data_dir().join("autosave")
}

pub fn brush_packs_dir() -> PathBuf {
    data_dir().join("brushes")
}

pub fn brush_search_dirs() -> Vec<PathBuf> {
    let Documents = documents_dir();
    let mut Folders = vec![Documents.join("Brushes"), imports_dir(), Documents.join("Inbox"), Documents.clone()];
    if cfg!(target_os = "android") {
        Folders.push(PathBuf::from("/storage/emulated/0/Download"));
        Folders.push(PathBuf::from("/storage/emulated/0/Documents"));
    }
    if let Some(Downloads) = dirs::download_dir() {
        Folders.push(Downloads);
    }
    Folders.dedup();
    Folders
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn brushes_path() -> PathBuf {
    config_dir().join("brushes.json")
}

pub fn ensure_dir(Directory: &Path) {
    if let Err(Problem) = std::fs::create_dir_all(Directory) {
        log::warn!("could not create {}: {Problem}", Directory.display());
    }
}

pub fn list_files(Directory: &Path, Extensions: &[&str]) -> Vec<PathBuf> {
    let mut Found: Vec<PathBuf> = std::fs::read_dir(Directory)
        .map(|Entries| {
            Entries
                .filter_map(|Entry| Entry.ok().map(|Item| Item.path()))
                .filter(|Path| {
                    Path.is_file()
                        && Path
                            .extension()
                            .and_then(|Extension| Extension.to_str())
                            .is_some_and(|Extension| Extensions.iter().any(|Wanted| Wanted.eq_ignore_ascii_case(Extension)))
                })
                .collect()
        })
        .unwrap_or_default();
    Found.sort_by_key(|Path| std::cmp::Reverse(std::fs::metadata(Path).and_then(|Meta| Meta.modified()).ok()));
    Found
}

pub const ImageExtensions: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];

#[cfg(feature = "desktop")]
pub mod dialogs {
    use std::path::PathBuf;

    pub fn open_project() -> Option<PathBuf> {
        rfd::FileDialog::new().add_filter("Despite project", &["despite"]).pick_file()
    }

    pub fn save_project(DefaultName: &str) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter("Despite project", &["despite"])
            .set_file_name(format!("{DefaultName}.despite"))
            .save_file()
            .map(|Path| if Path.extension().is_none() { Path.with_extension("despite") } else { Path })
    }

    pub fn open_images() -> Option<Vec<PathBuf>> {
        rfd::FileDialog::new().add_filter("Images", super::ImageExtensions).pick_files()
    }

    pub fn open_file(Label: &str, Extensions: &[&str]) -> Option<PathBuf> {
        rfd::FileDialog::new().add_filter(Label, Extensions).pick_file()
    }

    pub fn save_file(Label: &str, Extension: &str, DefaultName: &str) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter(Label, &[Extension])
            .set_file_name(format!("{DefaultName}.{Extension}"))
            .save_file()
            .map(|Path| if Path.extension().is_none() { Path.with_extension(Extension) } else { Path })
    }

    pub fn pick_folder() -> Option<PathBuf> {
        rfd::FileDialog::new().pick_folder()
    }
}

pub fn open_in_file_manager(Path: &Path) {
    #[cfg(all(feature = "desktop", target_os = "linux"))]
    let Program = Some("xdg-open");
    #[cfg(all(feature = "desktop", target_os = "macos"))]
    let Program = Some("open");
    #[cfg(all(feature = "desktop", target_os = "windows"))]
    let Program = Some("explorer");
    #[cfg(not(all(feature = "desktop", any(target_os = "linux", target_os = "macos", target_os = "windows"))))]
    let Program: Option<&str> = None;
    if let Some(Command) = Program
        && let Err(Problem) = std::process::Command::new(Command).arg(Path).spawn() {
            log::warn!("could not open {}: {Problem}", Path.display());
        }
}
