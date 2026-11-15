use std::env;
use std::path::{Path, PathBuf};

/// Where the app, the relay source and its generated files live.
///
/// The layout matches the previous desktop app so an existing installation can
/// be upgraded in place: the relay root is the folder that holds `index.js`,
/// with `logs\`, `tools\`, `runtime\` and `.env` next to it.
#[derive(Clone, Debug)]
pub struct AppPaths {
    pub root: PathBuf,
    pub exe: PathBuf,
    pub logs_dir: PathBuf,
    pub tools_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub node_dir: PathBuf,
    pub node_exe: PathBuf,
    pub node_modules: PathBuf,
    pub env_file: PathBuf,
    pub env_template: PathBuf,
    pub cloudflared_exe: PathBuf,
    pub log_file: PathBuf,
    pub settings_dir: PathBuf,
    pub settings_file: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Self {
        let exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("DhakaFlixRelay.exe"));

        let mut root = env::var("DHAKAFLIX_ROOT")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                exe.parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."))
            });

        // A build-tree launch (target\debug) should still find the relay source.
        if !root.join("index.js").exists() {
            if let Some(parent) = root.parent() {
                if parent.join("index.js").exists() {
                    root = parent.to_path_buf();
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

        Self {
            node_exe: node_dir.join("node.exe"),
            node_modules: root.join("node_modules"),
            env_file: root.join(".env"),
            env_template: root.join(".env.example"),
            cloudflared_exe: tools_dir.join("cloudflared.exe"),
            log_file: logs_dir.join("relay-desktop.log"),
            settings_file: settings_dir.join("settings.json"),
            settings_dir,
            logs_dir,
            tools_dir,
            runtime_dir,
            node_dir,
            root,
            exe,
        }
    }

    pub fn has_relay_source(&self) -> bool {
        self.root.join("index.js").is_file() && self.root.join("package.json").is_file()
    }

    pub fn ensure_dirs(&self) {
        for dir in [&self.logs_dir, &self.tools_dir, &self.settings_dir] {
            let _ = std::fs::create_dir_all(dir);
        }
    }
}
