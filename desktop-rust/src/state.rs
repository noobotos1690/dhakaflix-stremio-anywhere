use std::time::{Duration, Instant};

use crate::settings::Settings;
use crate::tailscale::{FunnelInfo, TsStatus};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

impl RunStatus {
    pub fn label(self) -> &'static str {
        match self {
            RunStatus::Stopped => "Stopped",
            RunStatus::Starting => "Starting",
            RunStatus::Running => "Running",
            RunStatus::Stopping => "Stopping",
            RunStatus::Error => "Error",
        }
    }
    pub fn kind(self) -> i32 {
        match self {
            RunStatus::Stopped => 0,
            RunStatus::Starting => 4,
            RunStatus::Running => 1,
            RunStatus::Stopping => 2,
            RunStatus::Error => 3,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Provider {
    pub name: String,
    pub host: String,
    pub online: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Health {
    pub version: String,
    pub providers: Vec<Provider>,
    pub active_streams: u32,
    pub max_streams: u32,
    pub uptime_seconds: u64,
}

#[derive(Clone, Debug, Default)]
pub struct TailscaleUi {
    pub installed: bool,
    pub service_running: bool,
    pub connected: bool,
    pub backend: String,
    pub device: String,
    pub host: String,
    pub funnel_active: bool,
    pub funnel_target: String,
    pub state: String,
    pub kind: i32,
    pub detail: String,
    pub error: String,
    pub busy: bool,
}

#[derive(Clone, Debug)]
pub struct Diag {
    pub name: String,
    pub detail: String,
    pub status: String,
    pub state: i32,
}

impl Diag {
    pub fn new(name: &str, detail: impl Into<String>, status: &str, state: i32) -> Self {
        Self { name: name.into(), detail: detail.into(), status: status.into(), state }
    }
}

#[derive(Clone, Debug)]
pub struct Mirror {
    pub name: String,
    pub host: String,
    pub online: bool,
}

#[derive(Clone, Debug)]
pub struct ToastState {
    pub message: String,
    pub kind: i32,
    pub until: Instant,
}

pub struct AppState {
    pub status: RunStatus,
    pub detail: String,
    pub started_at: Option<Instant>,
    pub public_url: Option<String>,
    pub manifest_url: Option<String>,
    pub public_host: String,
    pub mode: String,
    pub health: Health,
    pub node_version: String,
    pub backend_version: String,
    pub max_streams: u32,
    pub mirrors: Vec<Mirror>,
    pub diagnostics: Vec<Diag>,
    pub tailscale: TailscaleUi,
    pub busy: Option<String>,
    pub toast: Option<ToastState>,
    pub external_relay: bool,
    pub last_error: String,
    pub logs_cleared_seq: i64,
}

impl AppState {
    pub fn from_settings(settings: &Settings) -> Self {
        let mode = settings.public_access_mode.clone();
        Self {
            status: RunStatus::Stopped,
            detail: "Relay is not running.".into(),
            started_at: None,
            public_url: None,
            manifest_url: None,
            public_host: String::new(),
            mode,
            health: Health::default(),
            node_version: "Checking...".into(),
            backend_version: "-".into(),
            max_streams: settings.max_streams,
            mirrors: Vec::new(),
            diagnostics: Vec::new(),
            tailscale: TailscaleUi {
                state: "Checking...".into(),
                detail: "Tailscale has not been checked yet.".into(),
                kind: 0,
                ..Default::default()
            },
            busy: None,
            toast: None,
            external_relay: false,
            last_error: String::new(),
            logs_cleared_seq: 0,
        }
    }

    pub fn is_running(&self) -> bool {
        self.status == RunStatus::Running
    }

    pub fn set_status(&mut self, status: RunStatus, detail: impl Into<String>) {
        self.status = status;
        self.detail = detail.into();
        if status == RunStatus::Stopped || status == RunStatus::Error {
            self.started_at = None;
        }
    }

    pub fn set_public(&mut self, host: &str, url: &str) {
        self.public_host = host.to_string();
        self.public_url = Some(url.trim_end_matches('/').to_string());
        self.manifest_url = Some(format!("{}/manifest.json", url.trim_end_matches('/')));
    }

    pub fn clear_public(&mut self) {
        self.public_url = None;
        self.manifest_url = None;
        self.public_host.clear();
    }

    pub fn uptime_text(&self) -> String {
        let Some(started) = self.started_at else {
            return "00:00:00".into();
        };
        let total = started.elapsed().as_secs();
        format!("{:02}:{:02}:{:02}", total / 3600, (total % 3600) / 60, total % 60)
    }

    pub fn uptime_seconds(&self) -> u64 {
        self.started_at.map(|s| s.elapsed().as_secs()).unwrap_or(0)
    }

    pub fn toast(&mut self, message: impl Into<String>, kind: i32) {
        self.toast = Some(ToastState {
            message: message.into(),
            kind,
            until: Instant::now() + Duration::from_secs(4),
        });
    }

    pub fn take_toast(&mut self) -> Option<ToastState> {
        if let Some(toast) = &self.toast {
            if Instant::now() >= toast.until {
                self.toast = None;
            }
        }
        self.toast.clone()
    }

    pub fn apply_tailscale(&mut self, status: &TsStatus, funnel: &FunnelInfo) {
        let ts = &mut self.tailscale;
        ts.installed = status.installed;
        ts.service_running = status.service_running;
        ts.connected = status.connected;
        ts.backend = status.backend_state.clone();
        ts.device = if status.device_name.is_empty() { "-".into() } else { status.device_name.clone() };
        ts.host = if status.hostname.is_empty() { "-".into() } else { status.hostname.clone() };
        ts.funnel_active = funnel.enabled;
        ts.funnel_target = if funnel.target.is_empty() { "-".into() } else { funnel.target.clone() };
        ts.detail = if !status.installed {
            "Tailscale is not installed.".into()
        } else if !status.service_running {
            "The Tailscale service is not running. Start it from Windows Services.".into()
        } else if !status.connected {
            if !status.auth_url.is_empty() {
                "Sign-in is required. Click Connect to open the browser.".into()
            } else {
                format!("Tailscale reports {}.", status.backend_state)
            }
        } else if funnel.enabled {
            format!("Funnel is serving {}.", funnel.target)
        } else {
            "Connected. Funnel is off.".into()
        };
        ts.state = if !status.installed {
            "Not installed".into()
        } else if !status.service_running {
            "Service stopped".into()
        } else if !status.connected {
            if !status.auth_url.is_empty() { "Sign-in needed".into() } else { "Disconnected".into() }
        } else if funnel.enabled {
            "Funnel active".into()
        } else {
            "Connected".into()
        };
        ts.kind = if !status.installed || !status.service_running || !status.connected {
            2
        } else if funnel.enabled {
            1
        } else {
            4
        };
    }
}
