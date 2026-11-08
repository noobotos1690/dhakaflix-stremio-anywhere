use std::env;
use std::path::{Path, PathBuf};

/// Locations used by the Rust shell. The Node relay stays in the same folder
/// as the executable so portable and installed builds behave identically.
#[derive(Clone, Debug)]
pub struct Paths {
    pub root: PathBuf,
    pub exe: PathBuf,
    pub logs_dir: PathBuf,
    pub tools_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub node_dir: PathBuf,
    pub node_exe: PathBuf,
    pub node_modules: PathBuf,
    pub env_file: PathBuf,
    pub cloudflared: PathBuf,
    pub log_file: PathBuf,
    pub settings_dir: PathBuf,
    pub settings_file: PathBuf,
    pub icon_file: PathBuf,
}

impl Paths {
    pub fn discover() -> Self {
        let exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("DhakaFlixRelay.exe"));
        let mut root = exe.parent().unwrap_or(Path::new(".")).to_path_buf();

        if let Ok(explicit) = env::var("DHAKAFLIX_ROOT") {
            if !explicit.trim().is_empty() {
                root = PathBuf::from(explicit);
            }
        } else if !root.join("index.js").exists() {
            // Build-tree launch: target/debug/DhakaFlixRelay.exe.
            let mut cursor = root.clone();
            for _ in 0..5 {
                if cursor.join("index.js").exists() {
                    root = cursor;
                    break;
                }
                match cursor.parent() {
                    Some(parent) => cursor = parent.to_path_buf(),
                    None => break,
                }
            }
        }

        let logs_dir = root.join("logs");
        let tools_dir = root.join("tools");
        let runtime_dir = root.join("runtime");
        let node_dir = runtime_dir.join("node");
        let settings_dir = env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| root.clone())
            .join("DhakaFlix Relay");
        let settings_file = settings_dir.join("settings.json");

        Self {
            exe: exe.clone(),
            node_exe: node_dir.join("node.exe"),
            node_modules: root.join("node_modules"),
            env_file: root.join(".env"),
            cloudflared: tools_dir.join("cloudflared.exe"),
            log_file: logs_dir.join("relay-desktop.log"),
            icon_file: root.join("app.ico"),
            root: root.clone(),
            logs_dir,
            tools_dir,
            runtime_dir,
            node_dir,
            settings_dir,
            settings_file,
        }
    }

    pub fn ensure(&self) {
        let _ = std::fs::create_dir_all(&self.logs_dir);
        let _ = std::fs::create_dir_all(&self.tools_dir);
        let _ = std::fs::create_dir_all(&self.runtime_dir);
        let _ = std::fs::create_dir_all(&self.settings_dir);
    }

    pub fn has_relay(&self) -> bool {
        self.root.join("index.js").exists() && self.root.join("package.json").exists()
    }
}
