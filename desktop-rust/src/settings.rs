use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::paths::Paths;

/// Stored in the same `%APPDATA%\DhakaFlix Relay\settings.json` file as the
/// C# app. PascalCase keeps the file readable by both versions during the
/// migration window.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Settings {
    pub port: u16,
    pub auto_start_relay: bool,
    pub follow_logs: bool,
    pub first_run: bool,
    pub close_to_tray: bool,
    pub public_access_mode: String,
    pub tailscale_hostname: String,
    pub cloudflare_public_url: String,
    pub cloudflare_tunnel_token: String,
    pub cloudflare_tunnel_value: String,
    pub cloudflare_config_captured: bool,

    // Added by the Rust dashboard. Older readers simply ignore them.
    pub minimize_to_tray: bool,
    pub start_minimized: bool,
    pub max_streams: u32,
    pub log_level: String,
    pub window_width: i32,
    pub window_height: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: 7001,
            auto_start_relay: true,
            follow_logs: true,
            first_run: true,
            close_to_tray: true,
            public_access_mode: "cloudflare".into(),
            tailscale_hostname: String::new(),
            cloudflare_public_url: String::new(),
            cloudflare_tunnel_token: String::new(),
            cloudflare_tunnel_value: String::new(),
            cloudflare_config_captured: false,
            minimize_to_tray: true,
            start_minimized: false,
            max_streams: 8,
            log_level: "info".into(),
            window_width: 1180,
            window_height: 760,
        }
    }
}

impl Settings {
    pub fn load(paths: &Paths) -> Self {
        let mut settings = fs::read_to_string(&paths.settings_file)
            .ok()
            .and_then(|text| serde_json::from_str::<Settings>(&text).ok())
            .unwrap_or_default();
        settings.normalize();
        settings
    }

    pub fn normalize(&mut self) {
        if self.port < 1024 {
            self.port = 7001;
        }
        if self.max_streams == 0 {
            self.max_streams = 8;
        }
        self.max_streams = self.max_streams.clamp(1, 64);
        let mode = self.public_access_mode.trim().to_ascii_lowercase();
        self.public_access_mode = if mode == "tailscale" { "tailscale" } else { "cloudflare" }.into();
        let level = self.log_level.trim().to_ascii_lowercase();
        self.log_level = match level.as_str() {
            "error" | "warn" | "info" | "debug" => level,
            _ => "info".into(),
        };
        self.tailscale_hostname = self.tailscale_hostname.trim().trim_end_matches('.').to_string();
        self.cloudflare_public_url = self.cloudflare_public_url.trim().to_string();
    }

    pub fn is_tailscale(&self) -> bool {
        self.public_access_mode == "tailscale"
    }

    pub fn save(&self, paths: &Paths) -> Result<(), String> {
        fs::create_dir_all(&paths.settings_dir).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        atomic_write(&paths.settings_file, text.as_bytes())
    }

    pub fn set_mode(&mut self, mode: &str) {
        self.public_access_mode = if mode.eq_ignore_ascii_case("tailscale") {
            "tailscale".into()
        } else {
            "cloudflare".into()
        };
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, bytes).map_err(|e| e.to_string())?;
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&temp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csharp_json_is_migrated() {
        let raw = r#"{"Port":7123,"AutoStartRelay":false,"PublicAccessMode":"tailscale","TailscaleHostname":"gajar-nouka.tail145eb3.ts.net.","FirstRun":false}"#;
        let mut s: Settings = serde_json::from_str(raw).unwrap();
        s.normalize();
        assert_eq!(s.port, 7123);
        assert!(!s.auto_start_relay);
        assert!(s.is_tailscale());
        assert_eq!(s.tailscale_hostname, "gajar-nouka.tail145eb3.ts.net");
    }

    #[test]
    fn invalid_values_fall_back() {
        let mut s = Settings { port: 80, max_streams: 0, ..Default::default() };
        s.normalize();
        assert_eq!(s.port, 7001);
        assert_eq!(s.max_streams, 8);
    }
}
