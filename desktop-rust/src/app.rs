use std::cell::{Cell, RefCell};
use std::io::Write;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use slint::{CloseRequestResponse, ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::dependencies::{Dependencies, Snapshot};
use crate::envfile;
use crate::health;
use crate::logging::{self, LogStore};
use crate::paths::Paths;
use crate::relay::RelayManager;
use crate::settings::Settings;
use crate::startup;
use crate::state::{AppState, RunStatus};
use crate::tailscale::Tailscale;
use crate::tray::{Tray, TrayCommand};
use crate::{DiagnosticRow, LogRow, MainWindow, MirrorRowData};

pub struct App {
    pub ui: MainWindow,
    pub paths: Paths,
    pub settings: Arc<Mutex<Settings>>,
    pub shared: Arc<Mutex<AppState>>,
    pub deps: Arc<Dependencies>,
    pub tailscale: Arc<Tailscale>,
    pub relay: Arc<RelayManager>,
    pub logs: Arc<LogStore>,
    log_model: Rc<VecModel<LogRow>>,
    last_seq: Cell<i64>,
    filter: RefCell<String>,
    search: RefCell<String>,
    follow: Cell<bool>,
    selected: Cell<i64>,
    diag_sig: RefCell<String>,
    mirror_sig: RefCell<String>,
    tray: RefCell<Option<Tray>>,
    exiting: Cell<bool>,
}

impl App {
    pub fn new(
        ui: MainWindow,
        paths: Paths,
        settings: Arc<Mutex<Settings>>,
        shared: Arc<Mutex<AppState>>,
        deps: Arc<Dependencies>,
        tailscale: Arc<Tailscale>,
        relay: Arc<RelayManager>,
        logs: Arc<LogStore>,
    ) -> Self {
        let log_model = Rc::new(VecModel::<LogRow>::from(Vec::new()));
        ui.set_logs(ModelRc::from(log_model.clone()));
        Self {
            ui,
            paths,
            settings,
            shared,
            deps,
            tailscale,
            relay,
            logs,
            log_model,
            last_seq: Cell::new(0),
            filter: RefCell::new("all".into()),
            search: RefCell::new(String::new()),
            follow: Cell::new(true),
            selected: Cell::new(-1),
            diag_sig: RefCell::new(String::new()),
            mirror_sig: RefCell::new(String::new()),
            tray: RefCell::new(None),
            exiting: Cell::new(false),
        }
    }

    pub fn wire(app: &Rc<Self>) {
        let a = app.clone();
        app.ui.on_nav(move |page| {
            a.ui.set_page(page);
            if page == 2 {
                a.refresh_diagnostics();
            }
        });

        let a = app.clone();
        app.ui.on_start_relay(move || a.start_relay());
        let a = app.clone();
        app.ui.on_stop_relay(move || a.stop_relay());
        let a = app.clone();
        app.ui.on_restart_relay(move || a.restart_relay());

        let a = app.clone();
        app.ui.on_copy_manifest(move || a.copy_manifest());
        let a = app.clone();
        app.ui.on_open_manifest(move || a.open_manifest());
        let a = app.clone();
        app.ui.on_install_stremio(move || a.install_stremio());

        let a = app.clone();
        app.ui.on_choose_provider(move |mode| a.choose_provider(&mode));

        let a = app.clone();
        app.ui.on_ts_install(move || {
            let _ = webbrowser::open(Tailscale::download_url());
            a.show_toast("Opened the Tailscale download page.", 0);
        });
        let a = app.clone();
        app.ui.on_ts_connect(move || a.tailscale_connect());
        let a = app.clone();
        app.ui.on_ts_enable(move || a.tailscale_enable());
        let a = app.clone();
        app.ui.on_ts_disable(move || a.tailscale_disable());
        let a = app.clone();
        app.ui.on_ts_refresh(move || a.refresh_tailscale(true));

        let a = app.clone();
        app.ui.on_refresh_diagnostics(move || a.refresh_diagnostics());

        let a = app.clone();
        app.ui.on_filter_logs(move |filter| {
            *a.filter.borrow_mut() = filter.to_string();
            a.rebuild_logs();
        });
        let a = app.clone();
        app.ui.on_search_logs(move |search| {
            *a.search.borrow_mut() = search.to_string().to_ascii_lowercase();
            a.rebuild_logs();
        });
        let a = app.clone();
        app.ui.on_set_log_follow(move |follow| {
            a.follow.set(follow);
            if follow {
                a.rebuild_logs();
            }
        });
        let a = app.clone();
        app.ui.on_log_clear(move || {
            a.logs.clear();
            a.log_model.set_vec(Vec::new());
            a.last_seq.set(a.logs.last_seq());
            a.selected.set(-1);
            a.ui.set_selected_log(-1);
        });
        let a = app.clone();
        app.ui.on_log_copy(move || a.copy_logs());
        let a = app.clone();
        app.ui.on_log_select(move |seq| {
            a.selected.set(seq as i64);
            a.ui.set_selected_log(seq);
        });

        let a = app.clone();
        app.ui.on_set_autostart(move |enabled| a.set_autostart(enabled));
        let a = app.clone();
        app.ui.on_set_start_on_launch(move |enabled| a.set_start_on_launch(enabled));
        let a = app.clone();
        app.ui.on_set_minimize_to_tray(move |enabled| a.set_minimize(enabled));
        let a = app.clone();
        app.ui.on_port_edited(move |text| a.port_edited(&text));
        let a = app.clone();
        app.ui.on_set_max_streams(move |n| a.set_max_streams(n));
        let a = app.clone();
        app.ui.on_set_log_level(move |level| a.set_log_level(&level));

        let a = app.clone();
        app.ui.on_open_config(move || {
            let _ = Command::new("explorer").arg(&a.paths.root).spawn();
        });
        let a = app.clone();
        app.ui.on_open_logs(move || {
            let _ = Command::new("explorer").arg(&a.paths.logs_dir).spawn();
        });
        let a = app.clone();
        app.ui.on_repair(move || a.repair());
        let a = app.clone();
        app.ui.on_reset_prefs(move || a.reset_preferences());
        let a = app.clone();
        app.ui.on_dismiss_toast(move || {
            a.ui.set_toast_visible(false);
        });
        let a = app.clone();
        app.ui.on_exit_app(move || a.exit());
    }

    pub fn initialize(self: &Rc<Self>, autostart: bool) {
        let settings = self.settings.lock().unwrap().clone();
        self.ui.set_app_version("2.0.0".into());
        self.ui.set_page(0);
        self.ui.set_port(settings.port as i32);
        self.ui.set_port_text(settings.port.to_string().into());
        self.ui.set_max_streams(settings.max_streams as i32);
        self.ui.set_max_streams_setting(settings.max_streams as i32);
        self.ui.set_log_level(settings.log_level.clone().into());
        self.ui.set_start_on_launch(settings.auto_start_relay);
        self.ui.set_minimize_to_tray(settings.minimize_to_tray || settings.close_to_tray);
        self.ui.set_autostart(startup::is_enabled());
        self.ui.set_mode(settings.public_access_mode.clone().into());
        self.ui.set_default_provider(settings.public_access_mode.clone().into());
        self.ui.set_provider_label(self.provider_label().into());
        self.ui.set_temporary_url(!settings.is_tailscale());
        self.ui.set_log_follow(true);
        self.ui.set_log_filter("all".into());

        let snapshot = self.deps.inspect();
        self.apply_snapshot(&snapshot);
        if let Ok(mut state) = self.shared.lock() {
            state.node_version = snapshot.node_version.clone();
        }

        let weak = self.ui.as_weak();
        self.ui.window().on_close_requested(move || {
            if let Some(ui) = weak.upgrade() {
                hide_native_window(&ui);
            }
            // Slint must keep its last window alive, otherwise the event loop exits.
            // Hiding through Win32 keeps the tray process running without a taskbar entry.
            CloseRequestResponse::KeepWindowShown
        });

        self.ui.show().ok();
        apply_dark_titlebar(&self.ui);

        if let Some(tray) = Tray::spawn(&self.paths.icon_file) {
            *self.tray.borrow_mut() = Some(tray);
        }

        self.refresh_tailscale(false);
        self.refresh_diagnostics();

        if settings.first_run {
            self.show_toast("Setup is ready. Start the relay when you are.", 0);
            if let Ok(mut s) = self.settings.lock() {
                s.first_run = false;
                let _ = s.save(&self.paths);
            }
            self.ui.set_first_run(true);
        }

        if autostart {
            self.ui.hide().ok();
        }

        if settings.auto_start_relay {
            let relay = self.relay.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(900));
                relay.start();
            });
        }
    }

    pub fn tick(self: &Rc<Self>) {
        if let Some(tray) = self.tray.borrow().as_ref() {
            while let Ok(command) = tray.receiver.try_recv() {
                match command {
                    TrayCommand::Show => {
                        show_native_window(&self.ui);
                        self.ui.window().request_redraw();
                    }
                    TrayCommand::Start => self.relay.start(),
                    TrayCommand::Stop => self.relay.stop(),
                    TrayCommand::Exit => self.exit(),
                }
            }
        }

        {
            let mut state = self.shared.lock().unwrap();
            let status = state.status;
            self.ui.set_status_text(status.label().into());
            self.ui.set_status_kind(status.kind());
            self.ui.set_status_detail(state.detail.clone().into());
            self.ui.set_relay_running(state.is_running());
            self.ui.set_uptime_text(state.uptime_text().into());
            self.ui.set_node_version(if state.node_version.is_empty() { "Not detected".into() } else { state.node_version.clone() }.into());
            self.ui.set_backend_version(state.backend_version.clone().into());
            self.ui.set_mirrors_online(state.mirrors.iter().filter(|m| m.online).count() as i32);
            self.ui.set_mirrors_total(state.mirrors.len() as i32);
            self.ui.set_active_streams(state.health.active_streams as i32);
            self.ui.set_max_streams(if state.health.max_streams > 0 { state.health.max_streams } else { state.max_streams } as i32);
            self.ui.set_mode(state.mode.clone().into());
            self.ui.set_provider_label((if state.mode == "tailscale" { "Tailscale Funnel" } else { "Cloudflare Quick Tunnel" }).into());
            self.ui.set_temporary_url(state.mode != "tailscale");
            if let Some(host) = state.public_url.as_ref() {
                let host_only = host.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/');
                self.ui.set_public_host(host_only.into());
            } else {
                self.ui.set_public_host("".into());
            }
            self.ui.set_manifest_url(state.manifest_url.clone().unwrap_or_default().into());
            self.ui.set_has_endpoint(state.manifest_url.is_some() && state.is_running());
            self.ui.set_busy(state.busy.is_some());
            self.ui.set_busy_text(state.busy.clone().unwrap_or_default().into());
            self.ui.set_ts_state(state.tailscale.state.clone().into());
            self.ui.set_ts_kind(state.tailscale.kind);
            self.ui.set_ts_detail(state.tailscale.detail.clone().into());
            self.ui.set_ts_device(state.tailscale.device.clone().into());
            self.ui.set_ts_host(state.tailscale.host.clone().into());
            self.ui.set_ts_funnel(if state.tailscale.funnel_active {
                format!("Active -> {}", state.tailscale.funnel_target)
            } else {
                "Off".to_string()
            }.into());
            self.ui.set_ts_error(state.tailscale.error.clone().into());
            self.ui.set_ts_installed(state.tailscale.installed);
            self.ui.set_ts_connected(state.tailscale.connected);
            self.ui.set_ts_funnel_active(state.tailscale.funnel_active);
            self.ui.set_ts_busy(state.tailscale.busy);

            let toast = state.take_toast();
            if let Some(toast) = toast {
                self.ui.set_toast_message(toast.message.into());
                self.ui.set_toast_kind(toast.kind);
                self.ui.set_toast_visible(true);
            } else {
                self.ui.set_toast_visible(false);
            }

            let diag_sig = state.diagnostics.iter().map(|d| format!("{}|{}|{}", d.name, d.status, d.state)).collect::<String>();
            if diag_sig != *self.diag_sig.borrow() {
                *self.diag_sig.borrow_mut() = diag_sig;
                let rows: Vec<DiagnosticRow> = state.diagnostics.iter().map(|d| DiagnosticRow {
                    name: d.name.clone().into(),
                    detail: d.detail.clone().into(),
                    status: d.status.clone().into(),
                    state: d.state,
                }).collect();
                self.ui.set_diagnostics(ModelRc::new(VecModel::from(rows)));
            }

            let mirror_sig = state.mirrors.iter().map(|m| format!("{}|{}", m.name, m.online)).collect::<String>();
            if mirror_sig != *self.mirror_sig.borrow() {
                *self.mirror_sig.borrow_mut() = mirror_sig;
                let rows: Vec<MirrorRowData> = state.mirrors.iter().map(|m| MirrorRowData {
                    name: m.name.clone().into(),
                    host: m.host.clone().into(),
                    online: m.online,
                }).collect();
                self.ui.set_mirrors(ModelRc::new(VecModel::from(rows)));
            }
        }

        self.append_logs();
    }

    fn provider_label(&self) -> &'static str {
        let mode = self.shared.lock().unwrap().mode.clone();
        if mode == "tailscale" { "Tailscale Funnel" } else { "Cloudflare Quick Tunnel" }
    }

    fn apply_snapshot(&self, snapshot: &Snapshot) {
        let version = if snapshot.node_version.is_empty() { "Not detected".to_string() } else { snapshot.node_version.clone() };
        self.ui.set_node_version(version.into());
    }

    fn append_logs(&self) {
        let entries = self.logs.snapshot();
        let mut new_rows = Vec::new();
        for entry in entries {
            if entry.seq <= self.last_seq.get() {
                continue;
            }
            self.last_seq.set(entry.seq);
            if !self.follow.get() {
                continue;
            }
            if self.matches(&entry.level.as_str(), &entry.message) {
                new_rows.push(LogRow {
                    seq: entry.seq as i32,
                    time: entry.time.into(),
                    level: entry.level.as_str().into(),
                    message: entry.message.into(),
                    kind: entry.level.kind(),
                });
            }
        }
        for row in new_rows {
            self.log_model.push(row);
        }
        while self.log_model.row_count() > 2000 {
            self.log_model.remove(0);
        }
    }

    fn matches(&self, level: &str, message: &str) -> bool {
        let filter = self.filter.borrow();
        let level_ok = match filter.as_str() {
            "all" => true,
            "info" => level == "INFO",
            "warn" => level == "WARN",
            "error" => level == "ERROR",
            _ => true,
        };
        if !level_ok {
            return false;
        }
        let search = self.search.borrow();
        search.is_empty() || message.to_ascii_lowercase().contains(search.as_str()) || level.to_ascii_lowercase().contains(search.as_str())
    }

    fn rebuild_logs(&self) {
        let entries = self.logs.snapshot();
        let rows: Vec<LogRow> = entries
            .iter()
            .filter(|e| self.matches(e.level.as_str(), &e.message))
            .map(|e| LogRow {
                seq: e.seq as i32,
                time: e.time.clone().into(),
                level: e.level.as_str().into(),
                message: e.message.clone().into(),
                kind: e.level.kind(),
            })
            .collect();
        self.log_model.set_vec(rows);
        self.last_seq.set(self.logs.last_seq());
        self.ui.set_selected_log(-1);
    }

    fn copy_logs(&self) {
        let entries = self.logs.snapshot();
        let text = if self.selected.get() >= 0 {
            entries
                .iter()
                .find(|e| e.seq == self.selected.get())
                .map(|e| format!("{} {} {}", e.time, e.level.as_str(), e.message))
        } else {
            Some(
                entries
                    .iter()
                    .filter(|e| self.matches(e.level.as_str(), &e.message))
                    .map(|e| format!("{} {} {}", e.time, e.level.as_str(), e.message))
                    .collect::<Vec<_>>()
                    .join("\r\n"),
            )
        };
        match text {
            Some(text) if !text.is_empty() => {
                copy_text(&text);
                self.show_toast("Logs copied.", 0);
            }
            _ => self.show_toast("Nothing to copy.", 2),
        }
    }

    fn start_relay(&self) {
        self.relay.start();
    }

    fn stop_relay(&self) {
        self.relay.stop();
    }

    fn restart_relay(&self) {
        self.relay.restart();
    }

    fn copy_manifest(&self) {
        let url = self.shared.lock().unwrap().manifest_url.clone();
        if let Some(url) = url {
            copy_text(&url);
            self.show_toast("Manifest URL copied.", 0);
        } else {
            self.show_toast("No manifest URL yet. Start the relay.", 2);
        }
    }

    fn open_manifest(&self) {
        if let Some(url) = self.shared.lock().unwrap().manifest_url.clone() {
            let _ = webbrowser::open(&url);
        } else {
            self.show_toast("No public endpoint yet.", 2);
        }
    }

    fn install_stremio(&self) {
        let url = self.shared.lock().unwrap().manifest_url.clone();
        let Some(url) = url else {
            self.show_toast("No manifest URL yet. Start the relay.", 2);
            return;
        };
        let deep = url.replacen("https://", "stremio://", 1).replacen("http://", "stremio://", 1);
        let result = Command::new("cmd")
            .args(["/C", "start", "", &deep])
            .creation_flags(0x08000000)
            .spawn();
        if result.is_err() {
            self.show_toast("Stremio could not be opened. Is it installed?", 3);
        }
    }

    fn choose_provider(&self, mode: &str) {
        let running = self.shared.lock().unwrap().is_running();
        {
            let mut settings = self.settings.lock().unwrap();
            if settings.public_access_mode == mode {
                return;
            }
            settings.set_mode(mode);
            if let Err(error) = settings.save(&self.paths) {
                logging::warn(format!("Could not save provider: {error}"));
            }
            if let Ok(mut state) = self.shared.lock() {
                state.mode = settings.public_access_mode.clone();
                state.clear_public();
            }
        }
        self.ui.set_mode(mode.into());
        self.ui.set_default_provider(mode.into());
        self.ui.set_provider_label(self.provider_label().into());
        self.ui.set_temporary_url(mode != "tailscale");
        if running {
            self.show_toast("Provider changed. Restarting the relay.", 0);
            self.relay.restart();
        } else {
            self.show_toast("Provider saved. Start the relay to apply it.", 0);
        }
    }

    fn refresh_tailscale(&self, notify: bool) {
        let shared = self.shared.clone();
        let tailscale = self.tailscale.clone();
        {
            if let Ok(mut state) = self.shared.lock() {
                state.tailscale.busy = true;
            }
        }
        std::thread::spawn(move || {
            let status = tailscale.status();
            let funnel = tailscale.funnel();
            if let Ok(mut state) = shared.lock() {
                state.tailscale.busy = false;
                match status {
                    Ok(status) => {
                        let funnel = funnel.unwrap_or_default();
                        state.apply_tailscale(&status, &funnel);
                    }
                    Err(error) => {
                        state.tailscale.error = error.clone();
                        state.tailscale.state = "Error".into();
                        state.tailscale.kind = 3;
                        state.tailscale.detail = error;
                    }
                }
            }
            if notify {
                logging::info("Tailscale status refreshed.");
            }
        });
    }

    fn tailscale_connect(&self) {
        {
            let mut state = self.shared.lock().unwrap();
            state.tailscale.busy = true;
            state.tailscale.error.clear();
            state.tailscale.detail = "Waiting for Tailscale sign-in...".into();
        }
        let shared = self.shared.clone();
        let tailscale = self.tailscale.clone();
        std::thread::spawn(move || {
            logging::info("Starting Tailscale sign-in.");
            let result = tailscale.connect(|line| logging::info(line));
            let funnel = tailscale.funnel().unwrap_or_default();
            if let Ok(mut state) = shared.lock() {
                state.tailscale.busy = false;
                match result {
                    Ok(status) => {
                        state.apply_tailscale(&status, &funnel);
                        state.tailscale.error.clear();
                        state.toast("Tailscale is connected.", 0);
                    }
                    Err(error) => {
                        state.tailscale.error = error.clone();
                        state.tailscale.kind = 3;
                        state.tailscale.state = "Sign-in failed".into();
                        state.tailscale.detail = error;
                    }
                }
            }
        });
    }

    fn tailscale_enable(&self) {
        let running = self.shared.lock().unwrap().is_running();
        let port = self.settings.lock().unwrap().port;
        if !running {
            self.show_toast("Starting the relay; Funnel will be enabled.", 0);
            self.relay.start();
            return;
        }
        {
            let mut state = self.shared.lock().unwrap();
            state.tailscale.busy = true;
            state.tailscale.error.clear();
        }
        let shared = self.shared.clone();
        let tailscale = self.tailscale.clone();
        let settings = self.settings.clone();
        let paths = self.paths.clone();
        std::thread::spawn(move || {
            let result = tailscale.enable_funnel(port).and_then(|funnel| {
                let status = tailscale.status()?;
                if status.hostname.is_empty() {
                    return Err("Tailscale did not report a hostname.".into());
                }
                tailscale.public_ready(&status.hostname)?;
                Ok((status, funnel))
            });
            if let Ok(mut state) = shared.lock() {
                state.tailscale.busy = false;
                match result {
                    Ok((status, funnel)) => {
                        state.apply_tailscale(&status, &funnel);
                        let host = status.hostname.clone();
                        let url = format!("https://{host}");
                        state.set_public(&host, &url);
                        if let Ok(mut s) = settings.lock() {
                            s.tailscale_hostname = host;
                            let _ = s.save(&paths);
                        }
                        state.toast("Funnel is active.", 1);
                    }
                    Err(error) => {
                        state.tailscale.error = error.clone();
                        state.tailscale.kind = 3;
                        state.tailscale.state = "Funnel failed".into();
                        state.tailscale.detail = error;
                    }
                }
            }
        });
    }

    fn tailscale_disable(&self) {
        let port = self.settings.lock().unwrap().port;
        {
            let mut state = self.shared.lock().unwrap();
            state.tailscale.busy = true;
            state.tailscale.error.clear();
        }
        let shared = self.shared.clone();
        let tailscale = self.tailscale.clone();
        std::thread::spawn(move || {
            let result = tailscale.disable_funnel(port);
            if let Ok(mut state) = shared.lock() {
                state.tailscale.busy = false;
                match result {
                    Ok(funnel) => {
                        let status = tailscale.status().unwrap_or_default();
                        state.apply_tailscale(&status, &funnel);
                        state.clear_public();
                        state.toast("Funnel disabled. The relay is still local.", 0);
                    }
                    Err(error) => {
                        state.tailscale.error = error.clone();
                        state.tailscale.kind = 3;
                        state.tailscale.state = "Funnel failed".into();
                        state.tailscale.detail = error;
                    }
                }
            }
        });
    }

    fn refresh_diagnostics(&self) {
        let port = self.settings.lock().unwrap().port;
        crate::diagnostics::refresh(self.shared.clone(), self.paths.clone(), self.deps.clone(), self.tailscale.clone(), port);
    }

    fn set_autostart(&self, enabled: bool) {
        match startup::set_enabled(enabled, &self.paths.exe) {
            Ok(()) => {
                self.ui.set_autostart(startup::is_enabled());
                self.show_toast(if enabled { "Startup entry added." } else { "Startup entry removed." }, 0);
            }
            Err(error) => {
                self.ui.set_autostart(startup::is_enabled());
                self.show_toast(format!("Could not update startup: {error}"), 3);
            }
        }
    }

    fn set_start_on_launch(&self, enabled: bool) {
        if let Ok(mut settings) = self.settings.lock() {
            settings.auto_start_relay = enabled;
            let _ = settings.save(&self.paths);
        }
        self.ui.set_start_on_launch(enabled);
        self.show_toast(if enabled { "The relay will start with the app." } else { "The relay will wait for Start." }, 0);
    }

    fn set_minimize(&self, enabled: bool) {
        if let Ok(mut settings) = self.settings.lock() {
            settings.minimize_to_tray = enabled;
            settings.close_to_tray = enabled;
            let _ = settings.save(&self.paths);
        }
        self.ui.set_minimize_to_tray(enabled);
        self.show_toast(if enabled { "Close keeps the app in the tray." } else { "Close still hides the window." }, 0);
    }

    fn port_edited(&self, text: &str) {
        let parsed = text.trim().parse::<u16>().ok().filter(|p| *p >= 1024);
        let Some(port) = parsed else {
            self.ui.set_port_text(self.settings.lock().unwrap().port.to_string().into());
            self.show_toast("Port must be between 1024 and 65535.", 3);
            return;
        };
        {
            let mut settings = self.settings.lock().unwrap();
            settings.port = port;
            let _ = settings.save(&self.paths);
            let _ = envfile::apply_relay_settings(&self.paths.env_file, port, settings.max_streams, &settings.log_level);
        }
        self.ui.set_port(port as i32);
        self.ui.set_port_text(port.to_string().into());
        self.show_toast("Port saved. Restart the relay to apply it.", 0);
    }

    fn set_max_streams(&self, n: i32) {
        let n = n.clamp(1, 64) as u32;
        {
            let mut settings = self.settings.lock().unwrap();
            settings.max_streams = n;
            let _ = settings.save(&self.paths);
            let _ = envfile::apply_relay_settings(&self.paths.env_file, settings.port, n, &settings.log_level);
        }
        self.ui.set_max_streams_setting(n as i32);
        if let Ok(mut state) = self.shared.lock() {
            state.health.max_streams = n;
        }
        self.show_toast("Stream limit saved.", 0);
    }

    fn set_log_level(&self, level: &str) {
        let level = level.to_ascii_lowercase();
        {
            let mut settings = self.settings.lock().unwrap();
            settings.log_level = level.clone();
            let _ = settings.save(&self.paths);
            let _ = envfile::apply_relay_settings(&self.paths.env_file, settings.port, settings.max_streams, &level);
        }
        self.ui.set_log_level(level.into());
        self.show_toast("Log level saved.", 0);
    }

    fn repair(&self) {
        let mode = self.settings.lock().unwrap().public_access_mode.clone();
        self.show_toast("Checking runtime and dependencies...", 0);
        let deps = self.deps.clone();
        let shared = self.shared.clone();
        std::thread::spawn(move || match deps.ensure(&mode) {
            Ok(_) => {
                if let Ok(mut state) = shared.lock() {
                    state.toast("Dependencies are ready.", 0);
                }
            }
            Err(error) => {
                if let Ok(mut state) = shared.lock() {
                    state.toast(format!("Repair failed: {error}"), 3);
                }
            }
        });
    }

    fn reset_preferences(&self) {
        let mut defaults = Settings::default();
        defaults.port = self.settings.lock().unwrap().port;
        if let Err(error) = defaults.save(&self.paths) {
            self.show_toast(format!("Could not reset preferences: {error}"), 3);
            return;
        }
        if let Ok(mut settings) = self.settings.lock() {
            *settings = defaults.clone();
        }
        self.ui.set_max_streams_setting(defaults.max_streams as i32);
        self.ui.set_log_level(defaults.log_level.clone().into());
        self.ui.set_start_on_launch(defaults.auto_start_relay);
        self.ui.set_minimize_to_tray(true);
        self.ui.set_mode(defaults.public_access_mode.clone().into());
        self.ui.set_default_provider(defaults.public_access_mode.clone().into());
        if let Ok(mut state) = self.shared.lock() {
            state.mode = defaults.public_access_mode;
        }
        self.show_toast("Preferences reset. .env and PROXY_SECRET were kept.", 0);
    }

    fn show_toast(&self, message: impl Into<String>, kind: i32) {
        if let Ok(mut state) = self.shared.lock() {
            state.toast(message, kind);
        }
    }

    fn exit(&self) {
        if self.exiting.get() {
            return;
        }
        self.exiting.set(true);
        logging::info("Exit requested.");
        self.relay.stop();
        if let Some(tray) = self.tray.borrow().as_ref() {
            tray.shutdown();
        }
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(1100));
            let _ = slint::quit_event_loop();
        });
    }
}

pub fn start_timer(app: Rc<App>) -> Timer {
    let timer = Timer::default();
    let tick_app = app.clone();
    timer.start(TimerMode::Repeated, Duration::from_millis(250), move || tick_app.tick());
    timer
}

fn copy_text(text: &str) {
    if let Ok(mut child) = Command::new("clip").stdin(Stdio::piped()).creation_flags(0x08000000).spawn() {
        if let Some(stdin) = child.stdin.as_mut() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

#[cfg(windows)]
fn native_hwnd(ui: &MainWindow) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = ui.window().window_handle();
    match handle.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(win) => Some(win.hwnd.get()),
        _ => None,
    }
}

#[cfg(windows)]
fn hide_native_window(ui: &MainWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    if let Some(hwnd) = native_hwnd(ui) {
        unsafe {
            ShowWindow(hwnd as *mut core::ffi::c_void, SW_HIDE);
        }
    }
}

#[cfg(windows)]
fn show_native_window(ui: &MainWindow) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, ShowWindow, SW_SHOW,
    };
    if let Some(hwnd) = native_hwnd(ui) {
        unsafe {
            ShowWindow(hwnd as *mut core::ffi::c_void, SW_SHOW);
            SetForegroundWindow(hwnd as *mut core::ffi::c_void);
        }
    }
}

fn apply_dark_titlebar(ui: &MainWindow) {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
        let handle = ui.window().window_handle();
        if let Ok(raw) = handle.window_handle() {
            if let RawWindowHandle::Win32(win) = raw.as_raw() {
                let value: i32 = 1;
                let caption: u32 = 0x00181211; // COLORREF for #111218
                let caption_text: u32 = 0x00f8f1f3; // COLORREF for #f3f1f8
                unsafe {
                    for attr in [20u32, 19u32] {
                        DwmSetWindowAttribute(
                            win.hwnd.get() as *mut core::ffi::c_void,
                            attr,
                            &value as *const i32 as *const core::ffi::c_void,
                            std::mem::size_of::<i32>() as u32,
                        );
                    }
                    for (attr, color) in [(35u32, caption), (36u32, caption_text)] {
                        DwmSetWindowAttribute(
                            win.hwnd.get() as *mut core::ffi::c_void,
                            attr,
                            &color as *const u32 as *const core::ffi::c_void,
                            std::mem::size_of::<u32>() as u32,
                        );
                    }
                }
            }
        }
    }
}
