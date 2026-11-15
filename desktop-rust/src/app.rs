//! Application controller: owns the shared state, drives the relay, and pushes
//! a complete snapshot into the Slint window whenever something changes.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use slint::{ModelRc, SharedString, VecModel};

use crate::config::AppPaths;
use crate::logging::Entry;
use crate::relay::Phase;
use crate::settings::{AccessMode, Settings};
use crate::{
    CloudflareInfo, DiagRow, EndpointInfo, LogRow, MainWindow, MirrorRow, PrefsInfo, ServiceInfo,
    TailscaleInfo, TrayMenu,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmAction {
    Quit,
    ResetPreferences,
}

#[derive(Clone, Debug, Default)]
pub struct PublicCheck {
    pub health_ok: bool,
    pub manifest_ok: bool,
    pub checked: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct UpdateCheck {
    pub checked: bool,
    pub available: bool,
    pub latest_version: Option<String>,
    pub changelog: Option<String>,
}

pub struct App {
    pub paths: AppPaths,
    pub ui: slint::Weak<MainWindow>,
    pub tray: slint::Weak<TrayMenu>,
    pub handle: tokio::runtime::Handle,
    pub settings: Mutex<Settings>,
    pub relay: Mutex<crate::relay::Relay>,
    pub health: Mutex<Option<crate::health::HealthPayload>>,
    pub deps: Mutex<Option<crate::deps::Deps>>,
    pub tailscale: Mutex<Option<crate::tailscale::Status>>,
    pub funnel: Mutex<Option<crate::tailscale::Funnel>>,
    pub public_check: Mutex<PublicCheck>,
    pub update_check: Mutex<UpdateCheck>,
    pub log_level: Mutex<String>,
    pub log_search: Mutex<String>,
    pub busy: AtomicBool,
    pub diag_busy: AtomicBool,
    pub confirm: Mutex<Option<ConfirmAction>>,
    pub quitting: AtomicBool,
    pub toast_until: Mutex<Option<Instant>>,
    pub log_tick: AtomicU64,
    pub tray_hint_shown: AtomicBool,
}

impl App {
    pub fn new(
        paths: AppPaths,
        settings: Settings,
        ui: slint::Weak<MainWindow>,
        tray: slint::Weak<TrayMenu>,
        handle: tokio::runtime::Handle,
    ) -> Self {
        let port = settings.port;
        let mode = settings.mode();
        let log_level = settings.log_level.clone();
        Self {
            paths,
            ui,
            tray,
            handle,
            settings: Mutex::new(settings),
            relay: Mutex::new(crate::relay::Relay::new(port, mode)),
            health: Mutex::new(None),
            deps: Mutex::new(None),
            tailscale: Mutex::new(None),
            funnel: Mutex::new(None),
            public_check: Mutex::new(PublicCheck::default()),
            update_check: Mutex::new(UpdateCheck::default()),
            log_level: Mutex::new(log_level),
            log_search: Mutex::new(String::new()),
            busy: AtomicBool::new(false),
            diag_busy: AtomicBool::new(false),
            confirm: Mutex::new(None),
            quitting: AtomicBool::new(false),
            toast_until: Mutex::new(None),
            log_tick: AtomicU64::new(0),
            tray_hint_shown: AtomicBool::new(false),
        }
    }

    // ------------------------------------------------------------- settings

    pub fn settings_snapshot(&self) -> Settings {
        self.settings.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    pub fn update_settings(&self, change: impl FnOnce(&mut Settings)) {
        {
            let mut settings = self.settings.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            change(&mut settings);
            settings.save(&self.paths);
        }
    }

    pub fn mode(&self) -> AccessMode {
        self.settings.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).mode()
    }

    pub fn port(&self) -> u16 {
        self.settings.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).port
    }

    pub fn env_value(&self, key: &str) -> Option<String> {
        crate::envfile::read_value(&self.paths.env_file, key)
    }

    pub fn register_secret(&self, secret: &str) {
        if let Some(hub) = crate::logging::hub() {
            hub.add_secret(secret);
        }
    }

    pub fn register_secret_from_env(&self) {
        if let Some(secret) = crate::envfile::read_secret(&self.paths.env_file) {
            self.register_secret(&secret);
        }
    }

    // -------------------------------------------------------------- state

    pub fn relay_active(&self) -> bool {
        self.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).active()
    }

    /// Public origin currently published, if any.
    pub fn origin(&self) -> Option<String> {
        let relay_running;
        {
            let relay = self.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(url) = relay.public_url.clone() {
                if !url.is_empty() {
                    return Some(url);
                }
            }
            relay_running = relay.running();
        }
        let settings = self.settings_snapshot();
        if settings.mode().is_tailscale() && !settings.tailscale_hostname.is_empty() {
            return Some(format!("https://{}", settings.tailscale_hostname));
        }
        if !relay_running {
            return None;
        }
        let configured = self.env_value("PUBLIC_URL").unwrap_or_default();
        let configured = configured.trim().trim_end_matches('/').to_string();
        if configured.is_empty() {
            None
        } else {
            Some(configured)
        }
    }

    pub fn manifest(&self) -> Option<String> {
        self.origin().map(|origin| crate::health::manifest_url(&origin))
    }

    // ---------------------------------------------------------------- ui

    pub fn toast(&self, kind: &str, message: &str) {
        {
            let mut until = self.toast_until.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            *until = Some(Instant::now() + Duration::from_secs(if kind == "error" { 8 } else { 5 }));
        }
        self.set_toast(kind, message);
    }

    fn set_toast(&self, kind: &str, message: &str) {
        if message.trim().is_empty() {
            return;
        }
        // Toasts are raised from worker threads as well (background update checks,
        // downloads), so the properties are written on the UI thread.
        let kind = SharedString::from(kind);
        let message = SharedString::from(message);
        let ui = self.ui.clone();
        if let Err(error) = slint::invoke_from_event_loop(move || {
            if let Some(ui) = ui.upgrade() {
                ui.set_toast_kind(kind.clone());
                ui.set_toast_text(message.clone());
                ui.set_toast_visible(true);
            }
        }) {
            tracing::warn!("The toast could not be shown: {error}");
        }
    }

    pub fn report_error(&self, message: &str) {
        tracing::error!("{message}");
        self.toast("error", message);
    }

    pub fn confirm_dialog(
        &self,
        action: ConfirmAction,
        headline: &str,
        body: &str,
        label: &str,
        danger: bool,
        secondary: &str,
    ) {
        *self.confirm.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(action);
        if let Some(ui) = self.ui.upgrade() {
            ui.set_confirm_title(SharedString::from(headline));
            ui.set_confirm_body(SharedString::from(body));
            ui.set_confirm_label(SharedString::from(label));
            ui.set_confirm_danger(danger);
            ui.set_confirm_secondary_text(SharedString::from(secondary));
            ui.set_confirm_visible(true);
        }
    }

    pub fn hide_confirm(&self) {
        if let Some(ui) = self.ui.upgrade() {
            ui.set_confirm_visible(false);
        }
    }

    pub fn refresh(self: &Arc<Self>) {
        let app = self.clone();
        let _ = slint::invoke_from_event_loop(move || app.render_now());
    }

    pub fn render_logs(self: &Arc<Self>) {
        let app = self.clone();
        let _ = slint::invoke_from_event_loop(move || app.render_logs_now());
    }

    /// Builds the diagnostic rows from state the caller already holds.
    /// The mutexes are not reentrant, so this must never lock anything itself:
    /// `render_now` calls it while holding the snapshots it just read.
    #[allow(clippy::too_many_arguments)]
    fn build_diagnostics(
        &self,
        deps: &crate::deps::Deps,
        health: &Option<crate::health::HealthPayload>,
        relay: &crate::relay::Relay,
        ts: &crate::tailscale::Status,
        funnel: &crate::tailscale::Funnel,
        check: &PublicCheck,
        origin: Option<String>,
        manifest: Option<String>,
        port: u16,
    ) -> Vec<DiagRow> {
        let mut rows = Vec::new();

        rows.push(DiagRow {
            label: "Relay source".into(),
            detail: self.paths.root.display().to_string().into(),
            value: if deps.source_ok { "Found" } else { "Missing" }.into(),
            state: if deps.source_ok { "ok" } else { "error" }.into(),
        });

        rows.push(DiagRow {
            label: "Node.js runtime".into(),
            detail: deps
                .node_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "No node.exe was found".to_string())
                .into(),
            value: if deps.node_ok {
                format!("v{}", deps.node_version)
            } else {
                "Not installed".to_string()
            }
            .into(),
            state: if deps.node_ok { "ok" } else { "error" }.into(),
        });

        rows.push(DiagRow {
            label: "npm dependencies".into(),
            detail: self.paths.node_modules.display().to_string().into(),
            value: if deps.modules_ok { "Installed" } else { "Missing" }.into(),
            state: if deps.modules_ok { "ok" } else { "error" }.into(),
        });

        rows.push(DiagRow {
            label: "cloudflared".into(),
            detail: self.paths.cloudflared_exe.display().to_string().into(),
            value: if deps.cloudflared_ok { "Ready" } else { "Missing" }.into(),
            state: if deps.cloudflared_ok { "ok" } else { "warn" }.into(),
        });

        rows.push(DiagRow {
            label: "Tailscale client".into(),
            detail: deps
                .tailscale_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "tailscale.exe was not found".to_string())
                .into(),
            value: if deps.tailscale_ok { "Installed" } else { "Not installed" }.into(),
            state: if deps.tailscale_ok { "ok" } else { "idle" }.into(),
        });

        rows.push(DiagRow {
            label: "Tailscale service".into(),
            detail: if ts.installed {
                let state = if ts.backend_state.is_empty() {
                    "unknown state".to_string()
                } else {
                    ts.backend_state.clone()
                };
                match ts.error.clone() {
                    Some(error) => format!("{} · {error}", ts.version),
                    None => format!("{} · {state}", ts.version),
                }
            } else {
                "Not applicable".to_string()
            }
            .into(),
            value: if !ts.installed {
                "Unknown".to_string()
            } else if ts.service_running {
                "Running".to_string()
            } else {
                "Stopped".to_string()
            }
            .into(),
            state: if !ts.installed {
                "idle"
            } else if ts.service_running {
                "ok"
            } else {
                "error"
            }
            .into(),
        });

        rows.push(DiagRow {
            label: "Funnel".into(),
            detail: funnel
                .target
                .clone()
                .unwrap_or_else(|| "No Funnel listener is configured".to_string())
                .into(),
            value: if funnel.configured && funnel.enabled {
                "Active".to_string()
            } else if funnel.configured {
                "Configured, disabled".to_string()
            } else if funnel.error.is_some() {
                "Unknown".to_string()
            } else {
                "Not enabled".to_string()
            }
            .into(),
            state: if funnel.configured && funnel.enabled {
                "ok"
            } else if funnel.error.is_some() {
                "warn"
            } else {
                "idle"
            }
            .into(),
        });

        rows.push(DiagRow {
            label: "Local relay health".into(),
            detail: crate::health::local_url(port).into(),
            value: match (health.as_ref(), relay.phase) {
                (Some(payload), _) => format!("OK · v{}", payload.version),
                (None, Phase::Stopped) => "Not running".to_string(),
                (None, Phase::Error) => "Last start failed".to_string(),
                _ => "No answer".to_string(),
            }
            .into(),
            state: if health.is_some() {
                "ok"
            } else if relay.active() {
                "warn"
            } else {
                "idle"
            }
            .into(),
        });

        rows.push(DiagRow {
            label: "Public relay".into(),
            detail: origin
                .as_ref()
                .map(|origin| format!("{origin}/health"))
                .unwrap_or_else(|| "No public address".to_string())
                .into(),
            value: if check.checked {
                if check.health_ok {
                    "Reachable".to_string()
                } else {
                    "Unreachable".to_string()
                }
            } else {
                "Not checked".to_string()
            }
            .into(),
            state: if check.checked {
                if check.health_ok {
                    "ok"
                } else {
                    "error"
                }
            } else {
                "idle"
            }
            .into(),
        });

        rows.push(DiagRow {
            label: "Public manifest".into(),
            detail: manifest
                .clone()
                .unwrap_or_else(|| "No manifest URL".to_string())
                .into(),
            value: if check.checked {
                if check.manifest_ok {
                    "Served".to_string()
                } else {
                    "Missing".to_string()
                }
            } else {
                "Not checked".to_string()
            }
            .into(),
            state: if check.checked {
                if check.manifest_ok {
                    "ok"
                } else {
                    "error"
                }
            } else {
                "idle"
            }
            .into(),
        });

        rows
    }

    fn render_now(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let settings = self.settings_snapshot();
        let mode = settings.mode();
        let relay = self.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let health = self.health.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        let deps = self.deps.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone().unwrap_or_default();
        let ts = self.tailscale.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone().unwrap_or_default();
        let funnel = self.funnel.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone().unwrap_or_default();
        let origin = {
            let relay_origin = relay.public_url.clone().filter(|url| !url.is_empty());
            relay_origin
                .or_else(|| {
                    if mode.is_tailscale() && !settings.tailscale_hostname.is_empty() {
                        Some(format!("https://{}", settings.tailscale_hostname))
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    // The configured PUBLIC_URL only means something while a relay is
                    // actually serving it. Showing it while stopped made the Cloudflare
                    // panel display the previous Tailscale address.
                    if !relay.running() {
                        return None;
                    }
                    let configured = self.env_value("PUBLIC_URL").unwrap_or_default();
                    let configured = configured.trim().trim_end_matches('/').to_string();
                    if configured.is_empty() {
                        None
                    } else {
                        Some(configured)
                    }
                })
        };

        let manifest = origin
            .as_ref()
            .map(|origin| crate::health::manifest_url(origin));

        let uptime_seconds = health
            .as_ref()
            .map(|payload| payload.uptime_seconds)
            .or_else(|| relay.uptime().map(|duration| duration.as_secs()))
            .unwrap_or(0);

        let (status_text, status_kind) = relay.label();
        let exchange_note = if mode.is_tailscale() {
            "Funnel keeps the same ts.net address across restarts."
        } else if relay.running() {
            "Temporary address: it changes on every start."
        } else {
            ""
        };

        let detail = if let Some(error) = relay.last_error.clone() {
            error
        } else if relay.running() {
            format!(
                "Serving {} on port {}",
                mode.display_name(),
                relay.port
            )
        } else if deps.node_ok {
            format!(
                "Ready on port {}. Node.js v{} detected.",
                settings.port, deps.node_version
            )
        } else if deps.source_ok {
            "Node.js is not installed yet. Repair dependencies in Settings.".to_string()
        } else {
            "Relay source files were not found next to the app.".to_string()
        };

        ui.set_service(ServiceInfo {
            running: relay.running(),
            external: relay.external,
            status_text: status_text.into(),
            status_kind: status_kind.into(),
            uptime: if uptime_seconds > 0 {
                crate::relay::format_uptime(uptime_seconds)
            } else {
                "--:--:--".to_string()
            }
            .into(),
            node_version: if deps.node_ok {
                format!("v{}", deps.node_version)
            } else {
                "Missing".to_string()
            }
            .into(),
            node_ok: deps.node_ok,
            health_text: match (&health, relay.phase) {
                (Some(_), _) => "Healthy".to_string(),
                (None, Phase::Stopped) => "Stopped".to_string(),
                (None, Phase::Error) => "Failed".to_string(),
                _ => "Waiting".to_string(),
            }
            .into(),
            health_kind: if health.is_some() {
                "ok"
            } else if relay.active() {
                "warn"
            } else if relay.phase == Phase::Error {
                "error"
            } else {
                "idle"
            }
            .into(),
            mirrors_online: health
                .as_ref()
                .map(|payload| payload.mirrors_online() as i32)
                .unwrap_or(0),
            mirrors_total: health
                .as_ref()
                .map(|payload| payload.mirrors_total() as i32)
                .unwrap_or(0),
            sessions: health
                .as_ref()
                .map(|payload| payload.relay.active as i32)
                .unwrap_or(0),
            sessions_max: health
                .as_ref()
                .map(|payload| payload.relay.max as i32)
                .unwrap_or(settings.max_streams as i32),
            detail: detail.into(),
            stream_mode: health
                .as_ref()
                .map(|payload| payload.stream_mode.clone())
                .unwrap_or_else(|| "proxy".to_string())
                .into(),
            port: settings.port as i32,
            version: env!("CARGO_PKG_VERSION").into(),
        });

        ui.set_endpoint(EndpointInfo {
            provider: mode.display_name().into(),
            host: origin
                .as_ref()
                .map(|origin| crate::health::origin_host(origin))
                .unwrap_or_default()
                .into(),
            manifest: manifest.clone().unwrap_or_default().into(),
            local: format!("http://127.0.0.1:{}", settings.port).into(),
            kind: if relay.running() && manifest.is_some() {
                "ok"
            } else if relay.active() {
                "warn"
            } else if relay.phase == Phase::Error {
                "error"
            } else {
                "idle"
            }
            .into(),
            note: exchange_note.to_string().into(),
            has_url: manifest.is_some() && relay.running(),
        });

        ui.set_cloudflare(CloudflareInfo {
            selected: !mode.is_tailscale(),
            running: !mode.is_tailscale() && relay.active(),
            hostname: if mode.is_tailscale() {
                String::new()
            } else {
                origin
                    .as_ref()
                    .map(|origin| crate::health::origin_host(origin))
                    .unwrap_or_default()
            }
            .into(),
            manifest: if mode.is_tailscale() {
                String::new()
            } else {
                manifest.clone().unwrap_or_default()
            }
            .into(),
            status_text: if mode.is_tailscale() {
                "Not the selected provider"
            } else if relay.active() {
                status_text
            } else {
                "Tunnel stopped"
            }
            .into(),
            kind: if mode.is_tailscale() {
                "idle"
            } else {
                status_kind
            }
            .into(),
            detail: if mode.is_tailscale() {
                String::new()
            } else {
                relay.last_error.clone().unwrap_or_default()
            }
            .into(),
            has_url: !mode.is_tailscale() && manifest.is_some() && relay.running(),
            public_url: if mode.is_tailscale() {
                String::new()
            } else {
                origin.clone().unwrap_or_default()
            }
            .into(),
        });

        let device = ts
            .hostname
            .as_deref()
            .and_then(|host| host.split('.').next())
            .unwrap_or_default()
            .to_string();

        ui.set_tailscale(TailscaleInfo {
            selected: mode.is_tailscale(),
            installed: ts.installed,
            service_running: ts.service_running,
            connected: ts.connected,
            needs_login: ts.needs_login,
            device: device.clone().into(),
            hostname: ts.hostname.clone().unwrap_or_default().into(),
            funnel_active: funnel.configured && funnel.enabled,
            funnel_target: funnel
                .target
                .clone()
                .unwrap_or_else(|| "not configured".to_string())
                .into(),
            public_url: if mode.is_tailscale() {
                origin.clone().unwrap_or_default()
            } else {
                String::new()
            }
            .into(),
            manifest: if mode.is_tailscale() {
                manifest.clone().unwrap_or_default()
            } else {
                String::new()
            }
            .into(),
            status_text: if !ts.installed {
                "Tailscale is not installed"
            } else if !ts.service_running && !ts.connected {
                "Tailscale service is stopped"
            } else if ts.needs_login {
                "Sign-in needed"
            } else if !ts.connected {
                "Not connected"
            } else if funnel.configured && funnel.enabled {
                "Funnel is live"
            } else if relay.active() {
                "Relay running, Funnel off"
            } else {
                "Ready to enable"
            }
            .into(),
            kind: if !ts.installed {
                "warn"
            } else if ts.needs_login || (!ts.service_running && !ts.connected) {
                "warn"
            } else if funnel.configured && funnel.enabled {
                "ok"
            } else {
                "idle"
            }
            .into(),
            has_url: mode.is_tailscale() && manifest.is_some() && relay.running(),
            detail: if !ts.installed {
                "Install Tailscale first, sign in, then click Refresh.".to_string().into()
            } else if ts.needs_login {
                "Tailscale needs a sign-in. Click Connect and finish it in the browser.".to_string().into()
            } else if let Some(error) = relay.last_error.clone() {
                error.into()
            } else if let Some(error) = funnel.error.clone().filter(|_| ts.connected) {
                error.into()
            } else if let Some(error) = ts.error.clone() {
                error.into()
            } else {
                String::new().into()
            }
        });

        ui.set_prefs(PrefsInfo {
            startup: crate::startup::is_enabled(),
            auto_start: settings.auto_start_relay,
            minimize_to_tray: settings.minimize_to_tray,
            provider: settings.public_access_mode.clone().into(),
            port_text: settings.port.to_string().into(),
            max_streams_text: settings.max_streams.to_string().into(),
            log_level: settings.log_level.clone().into(),
            config_dir: self.paths.settings_dir.display().to_string().into(),
            logs_dir: self.paths.logs_dir.display().to_string().into(),
            version: env!("CARGO_PKG_VERSION").into(),
        });

        let check = self.public_check.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        ui.set_checks(ModelRc::new(VecModel::from(self.build_diagnostics(
            &deps,
            &health,
            &relay,
            &ts,
            &funnel,
            &check,
            origin.clone(),
            manifest.clone(),
            settings.port,
        ))));

        let mirrors: Vec<MirrorRow> = health
            .as_ref()
            .map(|payload| {
                payload
                    .providers
                    .iter()
                    .map(|provider| MirrorRow {
                        name: provider.name.clone().into(),
                        host: provider.host.clone().into(),
                        state: if provider.online {
                            "ok"
                        } else if provider.known {
                            "error"
                        } else {
                            "unknown"
                        }
                        .into(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        ui.set_mirrors(ModelRc::new(VecModel::from(mirrors)));

        ui.set_diag_summary(SharedString::from(if relay.running() {
            format!(
                "Relay on port {} · {} mirrors reachable · v{}",
                settings.port,
                health
                    .as_ref()
                    .map(|payload| payload.mirrors_online())
                    .unwrap_or(0),
                env!("CARGO_PKG_VERSION")
            )
        } else {
            "Nothing is listening yet. Start the relay, then refresh.".to_string()
        }));

        ui.set_busy(self.busy.load(Ordering::SeqCst));
        ui.set_diag_busy(self.diag_busy.load(Ordering::SeqCst));

        let now = Instant::now();
        let toast_visible = self
            .toast_until
            .lock()
            .map(|until| until.map(|at| at > now).unwrap_or(false))
            .unwrap_or(false);
        ui.set_toast_visible(toast_visible);

        let recent: Vec<LogRow> = crate::logging::hub()
            .map(|hub| hub.tail(6))
            .unwrap_or_default()
            .iter()
            .map(log_row)
            .collect();
        ui.set_recent(ModelRc::new(VecModel::from(recent)));

        if let Some(tray) = self.tray.upgrade() {
            tray.set_relay_running(relay.running());
            tray.set_has_link(manifest.is_some() && relay.running());
        }
    }

    fn render_logs_now(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let Some(hub) = crate::logging::hub() else { return };
        let entries = hub.snapshot();
        let level = self.log_level.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        let search = self.log_search.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).to_lowercase();

        let filtered: Vec<&Entry> = entries
            .iter()
            .filter(|entry| level_matches(&level, entry.level))
            .filter(|entry| search.is_empty() || entry.text.to_lowercase().contains(&search))
            .collect();

        let total = entries.len();
        let shown = filtered.len();
        let start = filtered.len().saturating_sub(400);
        let rows: Vec<LogRow> = filtered[start..].iter().map(|entry| log_row(entry)).collect();

        ui.set_logs(ModelRc::new(VecModel::from(rows)));
        ui.set_log_shown(shown as i32);
        ui.set_log_total(total as i32);
        ui.set_log_tick(self.log_tick.load(Ordering::Relaxed) as i32);
    }

    /// Ask the GitHub releases page for the newest version and install it when it
    /// is newer than this build.
    pub fn check_for_updates(self: &Arc<Self>) {
        let app = self.clone();
        self.handle.spawn(async move {
            app.toast("info", "Checking GitHub for a newer release…");

            tracing::info!("Asking the GitHub releases API for the newest version");
            let release = match tokio::task::spawn_blocking(crate::updates::latest_release).await {
                Ok(Ok(release)) => release,
                Ok(Err(error)) => {
                    tracing::warn!("Update check failed: {error}");
                    app.report_error(&format!("Update check failed: {error}."));
                    return;
                }
                Err(error) => {
                    tracing::warn!("Update check failed: {error}");
                    app.report_error(&format!("Update check failed: {error}."));
                    return;
                }
            };
            tracing::info!(
                "GitHub reports the newest release is {}",
                release.version
            );

            let current = env!("CARGO_PKG_VERSION");
            let newer = crate::updates::is_newer(current, &release.version);

            {
                let mut check = app
                    .update_check
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                check.checked = true;
                check.available = newer;
                check.latest_version = Some(release.version.clone());
                check.changelog = if newer {
                    let notes = release.notes.trim();
                    if notes.is_empty() {
                        None
                    } else {
                        Some(notes.to_string())
                    }
                } else {
                    None
                };
            }
            app.refresh();

            if !newer {
                app.toast(
                    "ok",
                    &format!("You are running the latest version (v{current})."),
                );
                return;
            }

            let Some(setup_url) = release.setup_url.clone() else {
                crate::winutil::open_url(&release.page_url);
                app.toast(
                    "warn",
                    &format!(
                        "v{} is out, but that release has no installer. Opening the release page.",
                        release.version
                    ),
                );
                return;
            };

            app.toast(
                "ok",
                &format!("v{} is available. Downloading the installer…", release.version),
            );

            let download =
                tokio::task::spawn_blocking(move || crate::updates::run_installer(&setup_url)).await;
            match download {
                Ok(Ok(())) => app.toast(
                    "ok",
                    "The installer is starting. The app restarts when it finishes.",
                ),
                Ok(Err(error)) => {
                    crate::winutil::open_url(&release.page_url);
                    app.report_error(&format!("{error}. Opening the release page instead."));
                }
                Err(error) => {
                    crate::winutil::open_url(&release.page_url);
                    app.report_error(&format!("The updater could not run: {error}."));
                }
            }
        });
    }

    pub fn filtered_log_text(&self, limit: usize) -> String {
        let Some(hub) = crate::logging::hub() else {
            return String::new();
        };
        let level = self.log_level.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
        let search = self.log_search.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).to_lowercase();
        let entries = hub.snapshot();
        let mut lines: Vec<String> = entries
            .iter()
            .filter(|entry| level_matches(&level, entry.level))
            .filter(|entry| search.is_empty() || entry.text.to_lowercase().contains(&search))
            .map(|entry| {
                format!(
                    "{} [{}] {}",
                    entry.at.format("%Y-%m-%d %H:%M:%S"),
                    entry.level_label(),
                    entry.text
                )
            })
            .collect();
        if lines.len() > limit {
            lines = lines.split_off(lines.len() - limit);
        }
        lines.join("\r\n")
    }
}

fn log_row(entry: &Entry) -> LogRow {
    LogRow {
        time: entry.at.format("%H:%M:%S").to_string().into(),
        level: entry.level_label().into(),
        text: entry.text.clone().into(),
    }
}

fn level_matches(filter: &str, level: tracing::Level) -> bool {
    use tracing::Level;
    match filter {
        "error" => level == Level::ERROR,
        "warn" => matches!(level, Level::ERROR | Level::WARN),
        "info" => matches!(level, Level::ERROR | Level::WARN | Level::INFO),
        _ => true,
    }
}