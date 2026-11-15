use serde::{Deserialize, Serialize};

use crate::config::AppPaths;

/// Which provider publishes the relay to the internet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessMode {
    Cloudflare,
    Tailscale,
}

impl AccessMode {
    pub fn from_storage(value: &str) -> Self {
        if value.eq_ignore_ascii_case("tailscale") {
            Self::Tailscale
        } else {
            Self::Cloudflare
        }
    }

    pub fn storage(self) -> &'static str {
        match self {
            Self::Cloudflare => "cloudflare",
            Self::Tailscale => "tailscale",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Cloudflare => "Cloudflare Quick Tunnel",
            Self::Tailscale => "Tailscale Funnel",
        }
    }

    pub fn is_tailscale(self) -> bool {
        self == Self::Tailscale
    }
}

/// Preferences in `%APPDATA%\DhakaFlix Relay\settings.json`.
///
/// The property names match the previous C# `/`s JSON so an existing file is
/// read without migration and an older build can still read what this app
/// writes during a rollback.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub max_streams: u16,
    pub log_level: String,
    pub minimize_to_tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            port: 7001,
            auto_start_relay: true,
            follow_logs: true,
            first_run: true,
            close_to_tray: false,
            public_access_mode: "cloudflare".to_string(),
            tailscale_hostname: String::new(),
            cloudflare_public_url: String::new(),
            cloudflare_tunnel_token: String::new(),
            cloudflare_tunnel_value: String::new(),
            cloudflare_config_captured: false,
            max_streams: 8,
            log_level: "info".to_string(),
            minimize_to_tray: true,
        }
    }
}

impl Settings {
    pub fn load(paths: &AppPaths) -> Self {
        let text = match std::fs::read_to_string(&paths.settings_file) {
            Ok(text) => text,
            Err(_) => return Self::default(),
        };

        let mut loaded: Settings = serde_json::from_str(&text).unwrap_or_default();
        if loaded.port < 1024 {
            loaded.port = 7001;
        }
        if loaded.max_streams == 0 || loaded.max_streams > 64 {
            loaded.max_streams = 8;
        }
        loaded.public_access_mode = AccessMode::from_storage(&loaded.public_access_mode)
            .storage()
            .to_string();
        match loaded.log_level.as_str() {
            "error" | "warn" | "info" | "debug" => {}
            _ => loaded.log_level = "info".to_string(),
        }
        loaded
    }

    pub fn save(&self, paths: &AppPaths) {
        let json = match serde_json::to_string_pretty(self) {
            Ok(json) => json,
            Err(_) => return,
        };
        let _ = std::fs::create_dir_all(&paths.settings_dir);
        if let Err(error) = std::fs::write(&paths.settings_file, json) {
            tracing::warn!("Could not write settings: {error}");
        }
    }

    pub fn mode(&self) -> AccessMode {
        AccessMode::from_storage(&self.public_access_mode)
    }

    pub fn set_mode(&mut self, mode: AccessMode) {
        self.public_access_mode = mode.storage().to_string();
    }
}
