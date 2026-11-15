//! Finding and launching the Stremio desktop app, plus the download page.

use std::path::PathBuf;
use std::sync::OnceLock;

static CACHED: OnceLock<Option<PathBuf>> = OnceLock::new();

pub fn find_executable() -> Option<PathBuf> {
    CACHED
        .get_or_init(|| {
            let mut candidates: Vec<PathBuf> = Vec::new();
            if let Ok(local) = std::env::var("LOCALAPPDATA") {
                candidates.push(PathBuf::from(&local).join(r"Programs\Stremio\Stremio.exe"));
                candidates.push(PathBuf::from(&local).join(r"Programs\stremio\Stremio.exe"));
            }
            for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
                if let Ok(base) = std::env::var(variable) {
                    candidates.push(PathBuf::from(base).join(r"Stremio\Stremio.exe"));
                }
            }
            candidates.into_iter().find(|path| path.is_file())
        })
        .clone()
}

pub fn open_install(manifest_url: &str) -> bool {
    if manifest_url.is_empty() {
        return false;
    }
    let link = crate::health::deep_link(manifest_url);
    crate::winutil::open_stremio_deep_link(&link)
}

pub fn open_app() -> bool {
    if let Some(exe) = find_executable() {
        let mut command = std::process::Command::new(exe);
        crate::winutil::hide_console(&mut command);
        if command.spawn().is_ok() {
            return true;
        }
    }
    crate::winutil::open_url("stremio://")
}

pub fn open_download_page() -> bool {
    crate::winutil::open_url("https://www.stremio.com/downloads")
}
