//! Everything the interface can ask for: relay lifecycle, provider actions,
//! diagnostics, settings and quitting.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, SharedString};

use crate::app::{App, ConfirmAction};
use crate::relay::Phase;
use crate::settings::{AccessMode, Settings};
use crate::{MainWindow, TrayMenu};

// --------------------------------------------------------------------- wiring

pub fn wire(ui: &MainWindow, tray: &TrayMenu, app: &Arc<App>) {
    {
        let app = app.clone();
        ui.on_start_relay(move || {
            let handle = app.handle.clone();
            let app = app.clone();
            handle.spawn(async move {
                let _ = start_relay(&app).await;
            });
        });
    }
    {
        let app = app.clone();
        ui.on_stop_relay(move || {
            let handle = app.handle.clone();
            let app = app.clone();
            handle.spawn(async move {
                if let Err(error) = stop_relay(&app).await {
                    app.report_error(&error);
                }
            });
        });
    }
    {
        let app = app.clone();
        ui.on_restart_relay(move || {
            let handle = app.handle.clone();
            let app = app.clone();
            handle.spawn(async move {
                let _ = restart_relay(&app).await;
            });
        });
    }
    {
        let app = app.clone();
        ui.on_copy_manifest(move || copy_manifest(&app));
    }
    {
        let app = app.clone();
        ui.on_copy_endpoint(move || copy_manifest(&app));
    }
    {
        let app = app.clone();
        ui.on_open_endpoint(move || {
            if let Some(origin) = app.origin() {
                if !crate::winutil::open_url(&origin) {
                    app.report_error("The address could not be opened in a browser.");
                }
            } else {
                app.toast("warn", "There is no public address yet.");
            }
        });
    }
    {
        let app = app.clone();
        ui.on_install_stremio(move || {
            let Some(manifest) = app.manifest() else {
                app.toast("warn", "Start the relay first: there is no addon link yet.");
                return;
            };
            if crate::stremio::open_install(&manifest) {
                app.toast("ok", "Stremio is opening the addon prompt.");
            } else {
                app.toast("warn", "Stremio was not found. Opening the download page.");
                crate::stremio::open_download_page();
            }
        });
    }
    {
        let app = app.clone();
        ui.on_open_logs_page(move || {
            if let Some(ui) = app.ui.upgrade() {
                ui.set_page(3);
            }
        });
    }
    {
        let app = app.clone();
        ui.on_open_logs_folder(move || {
            if !crate::winutil::open_folder(&app.paths.logs_dir) {
                app.report_error("The log folder could not be opened.");
            }
        });
    }
    {
        let app = app.clone();
        ui.on_select_provider(move |provider: SharedString| {
            let mode = AccessMode::from_storage(provider.as_str());
            let previous = app.mode();
            app.update_settings(|settings| settings.set_mode(mode));
            tracing::info!("Default public access set to {}", mode.display_name());
            app.refresh();

            // Picking another provider while a relay is running has to move the
            // running relay too, otherwise the published address stays the old one.
            if previous != mode && app.relay_active() {
                let handle = app.handle.clone();
                let app = app.clone();
                handle.spawn(async move {
                    let _ = start_relay(&app).await;
                });
            }
        });
    }
    {
        let app = app.clone();
        ui.on_cf_start(move || {
            let handle = app.handle.clone();
            let app = app.clone();
            handle.spawn(async move {
                app.update_settings(|settings| settings.set_mode(AccessMode::Cloudflare));
                let _ = start_relay(&app).await;
            });
        });
    }
    {
        let app = app.clone();
        ui.on_cf_stop(move || {
            let handle = app.handle.clone();
            let app = app.clone();
            handle.spawn(async move {
                if let Err(error) = stop_relay(&app).await {
                    app.report_error(&error);
                }
            });
        });
    }
    {
        let app = app.clone();
        ui.on_ts_install(move || {
            crate::winutil::open_url(crate::tailscale::DOWNLOAD_URL);
            app.toast("info", "The Tailscale download page is open. Refresh after installing.");
        });
    }
    {
        let app = app.clone();
        ui.on_ts_connect(move || {
            let handle = app.handle.clone();
            let app = app.clone();
            handle.spawn(async move { connect_tailscale(&app).await });
        });
    }
    {
        let app = app.clone();
        ui.on_ts_enable(move || {
            let handle = app.handle.clone();
            let app = app.clone();
            handle.spawn(async move { enable_tailscale(&app).await });
        });
    }
    {
        let app = app.clone();
        ui.on_ts_disable(move || {
            let handle = app.handle.clone();
            let app = app.clone();
            handle.spawn(async move { disable_tailscale(&app).await });
        });
    }
    {
        let app = app.clone();
        ui.on_ts_refresh(move || {
            let handle = app.handle.clone();
            let app = app.clone();
            handle.spawn(async move { refresh_diagnostics(&app).await });
        });
    }
    ui.on_open_admin(move || {
        crate::winutil::open_url(crate::tailscale::ADMIN_URL);
    });
    ui.on_open_funnel_docs(move || {
        crate::winutil::open_url(crate::tailscale::FUNNEL_DOCS);
    });
    ui.on_open_download(move || {
        crate::winutil::open_url(crate::tailscale::DOWNLOAD_URL);
    });
    {
        let app = app.clone();
        ui.on_refresh_diagnostics(move || {
            let handle = app.handle.clone();
            let app = app.clone();
            handle.spawn(async move { refresh_diagnostics(&app).await });
        });
    }
    {
        let app = app.clone();
        ui.on_set_log_level(move |level: SharedString| {
            let level = level.to_string();
            *app.log_level.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = level.clone();
            app.update_settings(|settings| settings.log_level = level.clone());
            write_env_settings(&app, None, None, Some(&level));
            app.render_logs();
            app.refresh();
        });
    }
    {
        let app = app.clone();
        ui.on_set_log_search(move |search: SharedString| {
            *app.log_search.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = search.to_string();
            app.render_logs();
        });
    }
    {
        let app = app.clone();
        ui.on_toggle_follow(move || {
            if let Some(ui) = app.ui.upgrade() {
                let follow = ui.get_log_follow();
                ui.set_log_follow(!follow);
            }
        });
    }
    {
        let app = app.clone();
        ui.on_clear_logs(move || {
            if let Some(hub) = crate::logging::hub() {
                hub.clear();
            }
            app.render_logs();
            app.refresh();
            app.toast("info", "The log view is empty. The log file keeps everything.");
        });
    }
    {
        let app = app.clone();
        ui.on_copy_logs(move || {
            let text = app.filtered_log_text(200);
            if text.is_empty() {
                app.toast("warn", "There is nothing to copy yet.");
            } else if crate::winutil::copy_text(&text) {
                app.toast("ok", "Visible log entries copied.");
            } else {
                app.report_error("The clipboard could not be filled.");
            }
        });
    }
    {
        let app = app.clone();
        ui.on_set_startup(move |enabled: bool| {
            let result = if enabled {
                crate::startup::enable(&app.paths.exe)
            } else {
                crate::startup::disable()
            };
            match result {
                Ok(()) => {
                    tracing::info!(
                        "Launch at Windows startup {}",
                        if enabled { "enabled" } else { "disabled" }
                    );
                    app.toast(
                        "ok",
                        if enabled {
                            "DhakaFlix Relay will start with Windows."
                        } else {
                            "Startup entry removed."
                        },
                    );
                }
                Err(error) => app.report_error(&error),
            }
            app.refresh();
        });
    }
    {
        let app = app.clone();
        ui.on_set_auto_start(move |enabled: bool| {
            app.update_settings(|settings| settings.auto_start_relay = enabled);
            app.refresh();
        });
    }
    {
        let app = app.clone();
        ui.on_set_tray(move |enabled: bool| {
            app.update_settings(|settings| {
                settings.minimize_to_tray = enabled;
                // Kept for the previous build so a rollback reads the same intent.
                settings.close_to_tray = enabled;
            });
            app.refresh();
        });
    }
    {
        let app = app.clone();
        ui.on_save_settings(move || save_settings(&app));
    }
    {
        let app = app.clone();
        ui.on_open_config(move || {
            if !crate::winutil::open_folder(&app.paths.settings_dir) {
                app.report_error("The configuration folder could not be opened.");
            }
        });
    }
    {
        let app = app.clone();
        ui.on_repair_dependencies(move || {
            let handle = app.handle.clone();
            let app = app.clone();
            handle.spawn(async move { repair(&app).await });
        });
    }
    {
        let app = app.clone();
        ui.on_check_updates(move || {
            tracing::info!("Update check requested from Settings");
            app.check_for_updates();
        });
    }
    {
        let app = app.clone();
        ui.on_reset_preferences(move || {
            app.confirm_dialog(
                ConfirmAction::ResetPreferences,
                "Reset preferences?",
                "Port, provider, log level and startup toggles return to their defaults. .env, PROXY_SECRET and your Stremio addon stay as they are.",
                "Reset",
                false,
                "",
            );
        });
    }
    {
        let app = app.clone();
        ui.on_confirm(move || handle_confirm(&app));
    }
    {
        let app = app.clone();
        ui.on_cancel(move || {
            *app.confirm.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
            app.hide_confirm();
        });
    }
    ui.on_confirm_secondary(move || {
        crate::stremio::open_app();
    });
    {
        // Closing the window hides it; the tray icon stays available.
        let app = app.clone();
        ui.window().on_close_requested(move || {
            hide_window(&app);
            slint::CloseRequestResponse::KeepWindowShown
        });
    }

    // ------------------------------------------------------------- tray
    {
        let app = app.clone();
        tray.on_show_window(move || show_window(&app));
    }
    {
        let app = app.clone();
        tray.on_start_relay(move || {
            let handle = app.handle.clone();
            let app = app.clone();
            handle.spawn(async move {
                let _ = start_relay(&app).await;
            });
        });
    }
    {
        let app = app.clone();
        tray.on_stop_relay(move || {
            let handle = app.handle.clone();
            let app = app.clone();
            handle.spawn(async move {
                if let Err(error) = stop_relay(&app).await {
                    app.report_error(&error);
                }
            });
        });
    }
    {
        let app = app.clone();
        tray.on_copy_link(move || copy_manifest(&app));
    }
    {
        let app = app.clone();
        tray.on_quit(move || request_quit(&app));
    }
}

pub fn spawn_tasks(app: &Arc<App>) {
    let handle = app.handle.clone();

    let task = app.clone();
    handle.spawn(async move { initial_setup(task).await });

    let task = app.clone();
    handle.spawn(async move { ticker(task).await });

    let task = app.clone();
    handle.spawn(async move { health_loop(task).await });

    let task = app.clone();
    handle.spawn(async move { relay_guard(task).await });
}

// ------------------------------------------------------------------ lifecycle

pub async fn start_relay(app: &Arc<App>) -> Result<(), String> {
    if app.busy.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    app.refresh();

    let result = start_relay_inner(app).await;
    app.busy.store(false, Ordering::SeqCst);

    if let Err(error) = &result {
        {
            let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if relay.phase == Phase::Starting {
                relay.phase = Phase::Error;
                relay.last_error = Some(error.clone());
            }
        }
        app.report_error(error);
    }

    app.refresh();
    result
}

async fn start_relay_inner(app: &Arc<App>) -> Result<(), String> {
    let settings = app.settings_snapshot();
    let mode = settings.mode();
    let port = settings.port;

    if app.relay_active() {
        let current = {
            let relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            relay.mode
        };
        if current == mode {
            return Ok(());
        }

        tracing::info!(
            "Switching the relay from {} to {}",
            current.display_name(),
            mode.display_name()
        );
        stop_relay_inner(app).await?;
    }

    let paths = app.paths.clone();
    let deps = tokio::task::spawn_blocking({
        let paths = paths.clone();
        move || crate::deps::inspect(&paths, mode)
    })
    .await
    .map_err(|error| error.to_string())?;
    *app.deps.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(deps.clone());

    if !deps.source_ok {
        return Err(
            "index.js and package.json were not found next to the app. Reinstall DhakaFlix Relay."
                .to_string(),
        );
    }
    let (node, node_version) = match deps.node_path.clone() {
        Some(path) if deps.node_ok => (path, deps.node_version.clone()),
        _ => {
            return Err(
                "Node.js is not installed yet. Use Repair dependencies in Settings first."
                    .to_string(),
            )
        }
    };

    if crate::winutil::port_busy(port) {
        let probe = tokio::task::spawn_blocking(move || crate::health::probe_local(port, 1500))
            .await
            .ok()
            .flatten();

        return match probe {
            Some(payload) => {
                let origin = app.origin();
                let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                relay.reset();
                relay.phase = Phase::Running;
                relay.external = true;
                relay.port = port;
                relay.mode = mode;
                relay.public_url = origin;
                drop(relay);
                *app.health.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(payload);
                tracing::info!("Using the DhakaFlix relay already listening on port {port}");
                app.toast("ok", "Using the relay that was already running.");
                Ok(())
            }
            None => Err(format!(
                "Port {port} is already in use by another program. Stop it, or change the relay port in Settings."
            )),
        };
    }

    let mut tailscale_host: Option<String> = None;
    if mode.is_tailscale() {
        let status = crate::tailscale::status().await;
        *app.tailscale.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(status.clone());

        if !status.installed {
            return Err(
                "Tailscale is not installed. Use Install Tailscale on the Public access page."
                    .to_string(),
            );
        }
        if !status.service_running && !status.connected {
            return Err(
                "The Tailscale service is not running. Start Tailscale, then try again.".to_string(),
            );
        }
        if !status.connected {
            return Err(if status.needs_login {
                "Tailscale needs you to sign in. Use Connect on the Public access page.".to_string()
            } else {
                "Tailscale is not connected. Use Connect on the Public access page.".to_string()
            });
        }
        let host = status.hostname.clone().filter(|host| !host.is_empty()).ok_or_else(|| {
            "Tailscale has not assigned an HTTPS hostname yet. Enable HTTPS certificates in the admin console."
                .to_string()
        })?;

        {
            let host = host.clone();
            app.update_settings(|settings| settings.tailscale_hostname = host);
        }

        let public_url = format!("https://{host}");
        match crate::envfile::set_values(
            &app.paths.env_file,
            &[("PUBLIC_URL", public_url.as_str()), ("TUNNEL", "0"), ("TUNNEL_TOKEN", "")],
        ) {
            Ok(Some(backup)) => {
                tracing::info!("Updated .env for Tailscale (backup: {})", backup.display())
            }
            Ok(None) => {}
            Err(error) => tracing::warn!("Could not update .env: {error}"),
        }
        tailscale_host = Some(host);
    } else {
        // A named tunnel (TUNNEL_TOKEN set) prints no URL of its own, so it relies on
        // PUBLIC_URL for its stable hostname: leave it alone. A quick tunnel gets a
        // fresh random hostname on every start, and a PUBLIC_URL left behind by an
        // earlier Tailscale run would keep overriding it inside the relay, so the
        // manifest served to Stremio and the install links would still point at the
        // dead ts.net address.
        let named_tunnel = !app
            .settings_snapshot()
            .cloudflare_tunnel_token
            .trim()
            .is_empty()
            || app
                .env_value("TUNNEL_TOKEN")
                .map(|token| !token.trim().is_empty())
                .unwrap_or(false);

        if named_tunnel {
            tracing::info!("Named Cloudflare tunnel configured: keeping PUBLIC_URL");
        } else {
            match crate::envfile::set_values(&app.paths.env_file, &[("PUBLIC_URL", "")]) {
                Ok(Some(backup)) => tracing::info!(
                    "Cleared the old public URL for the quick tunnel (backup: {})",
                    backup.display()
                ),
                Ok(None) => {}
                Err(error) => tracing::warn!("Could not clear PUBLIC_URL: {error}"),
            }
        }
    }

    let node_dir = node
        .parent()
        .map(|dir| dir.to_path_buf())
        .unwrap_or_default();

    let mut command = tokio::process::Command::new(&node);
    if mode.is_tailscale() {
        command.arg("index.js");
    } else {
        command.args(["index.js", "--tunnel"]);
    }
    command.current_dir(&app.paths.root);
    command.env("PORT", port.to_string());
    command.env("LOG_LEVEL", settings.log_level.clone());
    command.env("PATH", crate::deps::prepend_path(&node_dir));
    if let Some(host) = tailscale_host.as_ref() {
        command.env("PUBLIC_URL", format!("https://{host}"));
        command.env("TUNNEL", "0");
        command.env("TUNNEL_TOKEN", "");
    }
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    crate::winutil::hide_console_async(&mut command);

    let mut child = command
        .spawn()
        .map_err(|error| format!("Node.js could not be started: {error}"))?;

    let pid = child.id();
    if let Some(stdout) = child.stdout.take() {
        pump_output(app.clone(), stdout);
    }
    if let Some(stderr) = child.stderr.take() {
        pump_output(app.clone(), stderr);
    }

    {
        let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        relay.reset();
        relay.phase = Phase::Starting;
        relay.child = Some(child);
        relay.pid = pid;
        relay.port = port;
        relay.mode = mode;
        relay.tunnel = !mode.is_tailscale();
        relay.started_at = Some(Instant::now());
        relay.public_url = if mode.is_tailscale() {
            tailscale_host.as_ref().map(|host| format!("https://{host}"))
        } else {
            app.env_value("PUBLIC_URL")
                .map(|url| url.trim_end_matches('/').to_string())
                .filter(|url| !url.is_empty())
        };
    }

    tracing::info!(
        "Starting relay: node index.js{} (port {port}, {}, Node.js v{node_version})",
        if mode.is_tailscale() { "" } else { " --tunnel" },
        mode.display_name()
    );
    app.refresh();

    let startup = async {
        let payload = wait_local_health(app, Duration::from_secs(30)).await?;
        *app.health.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(payload);

        if mode.is_tailscale() {
            let funnel = crate::tailscale::enable_funnel(port, false).await?;
            let host = funnel
                .hostname
                .clone()
                .or_else(|| tailscale_host.clone())
                .ok_or_else(|| "Tailscale did not report a Funnel hostname.".to_string())?;
            *app.funnel.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(funnel);
            let origin = format!("https://{host}");
            {
                let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                relay.public_url = Some(origin.clone());
            }
            verify_public(app, &origin, Duration::from_secs(90)).await?;
        }

        Ok::<(), String>(())
    };

    if let Err(error) = startup.await {
        kill_relay(app);
        {
            let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            relay.phase = Phase::Error;
            relay.last_error = Some(error.clone());
            relay.started_at = None;
        }
        *app.health.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        app.refresh();
        return Err(error);
    }

    {
        let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        relay.phase = Phase::Running;
        relay.last_error = None;
    }
    app.refresh();

    if let Some(manifest) = app.manifest() {
        tracing::info!("Stremio install link: {manifest}");
    }

    let first_run = app.settings_snapshot().first_run;
    if first_run {
        app.update_settings(|settings| settings.first_run = false);
        if let Some(manifest) = app.manifest() {
            if crate::stremio::open_install(&manifest) {
                tracing::info!("Opened Stremio with the addon install prompt");
            }
        }
    }

    app.toast("ok", "Relay is running.");
    Ok(())
}

pub async fn stop_relay(app: &Arc<App>) -> Result<(), String> {
    if app.busy.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let result = stop_relay_inner(app).await;
    app.busy.store(false, Ordering::SeqCst);
    if let Err(error) = &result {
        app.report_error(error);
    }
    app.refresh();
    result
}

async fn stop_relay_inner(app: &Arc<App>) -> Result<(), String> {
    let child = {
        let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if relay.external {
            return Err(
                "This relay was started outside DhakaFlix Relay, so the app will not stop it."
                    .to_string(),
            );
        }
        match relay.child.take() {
            Some(child) => {
                relay.phase = Phase::Stopping;
                Some(child)
            }
            None => None,
        }
    };

    app.refresh();

    if let Some(mut child) = child {
        let pid = child.id();
        tracing::info!("Stopping the relay");
        if let Some(pid) = pid {
            crate::winutil::stop_process_tree(pid, false);
        }
        if tokio::time::timeout(Duration::from_secs(3), child.wait())
            .await
            .is_err()
        {
            if let Some(pid) = pid {
                crate::winutil::stop_process_tree(pid, true);
            }
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
        }
    }

    {
        let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let port = app.port();
        let mode = app.mode();
        relay.reset();
        relay.port = port;
        relay.mode = mode;
    }
    *app.health.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    *app.public_check.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Default::default();

    tracing::info!("Relay stopped");
    Ok(())
}

pub async fn restart_relay(app: &Arc<App>) -> Result<(), String> {
    if app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).external {
        return Err(
            "Restart is not available for a relay that was started outside this app.".to_string(),
        );
    }
    if let Err(error) = stop_relay(app).await {
        return Err(error);
    }
    start_relay(app).await
}

fn kill_relay(app: &Arc<App>) {
    let child = app.relay.lock().ok().and_then(|mut relay| relay.child.take());
    if let Some(mut child) = child {
        if let Some(pid) = child.id() {
            crate::winutil::stop_process_tree(pid, true);
        }
        let _ = child.start_kill();
    }
}

fn pump_output<R>(app: Arc<App>, stream: R)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(stream).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let text = crate::relay::strip_ansi(&line);
            if text.trim().is_empty() {
                continue;
            }
            if crate::relay::looks_like_error(&text) {
                tracing::error!("{text}");
            } else if crate::relay::looks_like_warning(&text) {
                tracing::warn!("{text}");
            } else {
                tracing::info!("{text}");
            }

            if let Some(url) = crate::relay::detect_public_url(&text) {
                let changed = {
                    let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    if relay.public_url.as_deref() != Some(url.as_str()) {
                        relay.public_url = Some(url.clone());
                        true
                    } else {
                        false
                    }
                };
                if changed {
                    tracing::info!("Public relay address: {url}");
                    app.refresh();
                }
            }
        }
    });
}

async fn wait_local_health(
    app: &Arc<App>,
    timeout: Duration,
) -> Result<crate::health::HealthPayload, String> {
    let port = app.port();
    let deadline = Instant::now() + timeout;

    loop {
        let payload =
            tokio::task::spawn_blocking(move || crate::health::probe_local(port, 1500))
                .await
                .ok()
                .flatten();
        if let Some(payload) = payload {
            return Ok(payload);
        }

        let exited = {
            let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            match relay.child.as_mut() {
                Some(child) => matches!(child.try_wait(), Ok(Some(_))),
                None => false,
            }
        };
        if exited {
            return Err(
                "The relay process exited during startup. Check the log output for the reason."
                    .to_string(),
            );
        }
        if Instant::now() >= deadline {
            return Err(
                "The local relay did not become healthy in time. Check the log output.".to_string(),
            );
        }
        tokio::time::sleep(Duration::from_millis(600)).await;
    }
}

async fn verify_public(app: &Arc<App>, origin: &str, timeout: Duration) -> Result<(), String> {
    let health_url = format!("{origin}/health");
    let manifest_url = format!("{origin}/manifest.json");
    let deadline = Instant::now() + timeout;
    let mut announced = false;

    loop {
        let health = health_url.clone();
        let manifest = manifest_url.clone();
        let result = tokio::task::spawn_blocking(move || {
            (
                crate::health::reachable(&health, 6000),
                crate::health::reachable(&manifest, 6000),
            )
        })
        .await
        .ok();

        if let Some((health_ok, manifest_ok)) = result {
            {
                let mut check = app.public_check.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                check.health_ok = health_ok;
                check.manifest_ok = manifest_ok;
                check.checked = true;
                check.error = if health_ok && manifest_ok {
                    None
                } else {
                    Some("The public endpoints have not answered yet.".to_string())
                };
            }
            if health_ok && manifest_ok {
                tracing::info!("Public health and manifest checks passed for {origin}");
                app.refresh();
                return Ok(());
            }
        }

        if Instant::now() >= deadline {
            return Err(format!(
                "{origin} did not pass its health checks. Check Funnel in the Tailscale admin console."
            ));
        }
        if !announced {
            announced = true;
            tracing::info!("Waiting for {origin} to answer (certificate or Funnel may still be starting)");
        }
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }
}

// --------------------------------------------------------------------- tasks

async fn initial_setup(app: Arc<App>) {
    app.register_secret_from_env();

    let paths = app.paths.clone();
    let mode = app.mode();
    let mut deps = tokio::task::spawn_blocking(move || crate::deps::inspect(&paths, mode))
        .await
        .unwrap_or_default();
    *app.deps.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(deps.clone());

    let status = crate::tailscale::status().await;
    *app.tailscale.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(status);
    let funnel = crate::tailscale::funnel_status().await;
    *app.funnel.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(funnel);

    app.refresh();

    // A freshly unpacked copy has no .env. Create one here so the first start
    // already signs stream links with a secret that survives a restart.
    if deps.source_ok && !deps.env_ok {
        match crate::deps::create_env_if_missing(&app.paths) {
            Ok(Some(secret)) => {
                app.register_secret(&secret);
                deps.env_ok = true;
                deps.env_problem = None;
                *app.deps.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(deps.clone());
                app.refresh();
            }
            Ok(None) => {
                tracing::warn!(".env exists but has no PROXY_SECRET; run Repair dependencies")
            }
            Err(error) => {
                tracing::warn!("Could not create .env: {error}");
                app.toast("warn", &format!("Could not create .env: {error}"));
            }
        }
    }

    if deps.source_ok && !deps.node_ok {
        tracing::warn!("Node.js was not found; dependency setup is required");
        app.toast("warn", "Node.js is missing. Open Settings and run Repair dependencies.");
        app.refresh();
        return;
    }

    if deps.source_ok && deps.node_ok && !deps.modules_ok {
        tracing::warn!("npm packages are missing");
        app.toast("warn", "npm packages are missing. Run Repair dependencies in Settings.");
    }

    if app.settings_snapshot().auto_start_relay && deps.source_ok && deps.node_ok {
        let _ = start_relay(&app).await;
    }
    app.refresh();
}

async fn ticker(app: Arc<App>) {
    let mut last_len = crate::logging::hub().map(|hub| hub.len()).unwrap_or(0);
    let mut tick: u64 = 0;
    let mut seconds: u64 = 0;
    let mut window_shown: Option<bool> = Some(true);
    let mut repaints_left: u32 = 0;

    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        seconds += 1;

        // Repaint as soon as the window is back from minimize or the tray. The
        // surface it comes back with has the top rows unpainted, and nothing
        // else invalidates them, so without this the desktop shows through.
        let shown = crate::winutil::window_shown("DhakaFlix Relay");
        if shown != window_shown {
            if shown == Some(true) {
                repaints_left = 2;
            }
            window_shown = shown;
        }
        if repaints_left > 0 {
            repaints_left -= 1;
            crate::winutil::refresh_surface("DhakaFlix Relay");
        }

        if let Some(hub) = crate::logging::hub() {
            let len = hub.len();
            if len != last_len {
                last_len = len;
                tick += 1;
                app.log_tick.store(tick, Ordering::Relaxed);
                app.render_logs();
                app.refresh();
            }
        }

        if seconds % 2 == 0 {
            app.refresh();
        }
    }
}

async fn health_loop(app: Arc<App>) {
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;

        let active = app.relay_active();
        if !active {
            continue;
        }

        let port = app.port();
        let payload = tokio::task::spawn_blocking(move || crate::health::probe_local(port, 1500))
            .await
            .ok()
            .flatten();

        if let Some(payload) = payload {
            *app.health.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(payload);
            let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if relay.phase == Phase::Starting && !relay.mode.is_tailscale() {
                relay.phase = Phase::Running;
            }
        }

        app.refresh();
    }
}

async fn relay_guard(app: Arc<App>) {
    loop {
        tokio::time::sleep(Duration::from_millis(700)).await;

        let exit = {
            let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if relay.external {
                continue;
            }
            match relay.child.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(Some(status)) => Some(status.code()),
                    _ => None,
                },
                None => None,
            }
        };

        if let Some(code) = exit {
            let stopping = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).phase == Phase::Stopping;
            if stopping {
                continue;
            }
            {
                let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                relay.child = None;
                relay.phase = Phase::Error;
                relay.started_at = None;
                relay.last_error = Some(format!(
                    "The relay process stopped unexpectedly (exit code {}).",
                    code.unwrap_or(-1)
                ));
            }
            *app.health.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
            tracing::error!(
                "The relay process stopped unexpectedly (exit code {})",
                code.unwrap_or(-1)
            );
            app.toast("error", "The relay stopped unexpectedly. Check the logs.");
            app.refresh();
        }
    }
}

// ------------------------------------------------------------------ actions

async fn connect_tailscale(app: &Arc<App>) {
    if app.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    app.refresh();
    tracing::info!("Starting Tailscale and waiting for authentication");

    let result = crate::tailscale::connect(120).await;
    app.busy.store(false, Ordering::SeqCst);

    match result {
        Ok(status) => {
            *app.tailscale.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(status);
            let funnel = crate::tailscale::funnel_status().await;
            *app.funnel.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(funnel);
            tracing::info!("Tailscale is connected");
            app.toast("ok", "Tailscale is connected.");
        }
        Err(error) => app.report_error(&error),
    }
    app.refresh();
}

async fn enable_tailscale(app: &Arc<App>) {
    app.update_settings(|settings| settings.set_mode(AccessMode::Tailscale));
    app.refresh();

    if app.relay_active() {
        if app.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        let port = app.port();
        let result = crate::tailscale::enable_funnel(port, false).await;
        app.busy.store(false, Ordering::SeqCst);

        match result {
            Ok(funnel) => {
                *app.funnel.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(funnel.clone());
                if let Some(host) = funnel.hostname.clone() {
                    let origin = format!("https://{host}");
                    app.update_settings(|settings| settings.tailscale_hostname = host);
                    {
                        let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        relay.public_url = Some(origin.clone());
                    }
                    app.refresh();
                    if let Err(error) = verify_public(app, &origin, Duration::from_secs(60)).await {
                        app.report_error(&error);
                    } else {
                        app.toast("ok", "Funnel is live.");
                    }
                }
            }
            Err(error) => app.report_error(&error),
        }
        app.refresh();
        return;
    }

    let _ = start_relay(app).await;
}

async fn disable_tailscale(app: &Arc<App>) {
    if app.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let port = app.port();
    let result = crate::tailscale::disable_funnel(port).await;
    app.busy.store(false, Ordering::SeqCst);

    match result {
        Ok(()) => {
            let funnel = crate::tailscale::funnel_status().await;
            *app.funnel.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(funnel);
            tracing::info!("Funnel disabled for the DhakaFlix port");
            app.toast("ok", "Funnel disabled. Your tailnet stays connected.");
        }
        Err(error) => app.report_error(&error),
    }
    app.refresh();
}

async fn refresh_diagnostics(app: &Arc<App>) {
    if app.diag_busy.swap(true, Ordering::SeqCst) {
        return;
    }
    app.refresh();

    let paths = app.paths.clone();
    let mode = app.mode();
    let deps = tokio::task::spawn_blocking(move || crate::deps::inspect(&paths, mode))
        .await
        .unwrap_or_default();
    *app.deps.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(deps);

    let status = crate::tailscale::status().await;
    *app.tailscale.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(status);
    let funnel = crate::tailscale::funnel_status().await;
    *app.funnel.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(funnel);

    let port = app.port();
    if let Some(payload) =
        tokio::task::spawn_blocking(move || crate::health::probe_local(port, 2000))
            .await
            .ok()
            .flatten()
    {
        *app.health.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(payload);
    }

    if let Some(origin) = app.origin() {
        let health = format!("{origin}/health");
        let manifest = format!("{origin}/manifest.json");
        let result = tokio::task::spawn_blocking(move || {
            (
                crate::health::reachable(&health, 5000),
                crate::health::reachable(&manifest, 5000),
            )
        })
        .await
        .ok();

        if let Some((health_ok, manifest_ok)) = result {
            let mut check = app.public_check.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            check.health_ok = health_ok;
            check.manifest_ok = manifest_ok;
            check.checked = true;
            check.error = if health_ok && manifest_ok {
                None
            } else {
                Some("The public endpoints did not answer.".to_string())
            };
        }
    }

    app.diag_busy.store(false, Ordering::SeqCst);
    app.refresh();
}

async fn repair(app: &Arc<App>) {
    if app.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    app.refresh();
    tracing::info!("Checking the relay dependencies");

    let result = crate::deps::install(app).await;
    app.busy.store(false, Ordering::SeqCst);

    match result {
        Ok(()) => app.toast("ok", "Dependencies are ready."),
        Err(error) => app.report_error(&error),
    }
    app.refresh();
}

// ------------------------------------------------------------------ settings

fn save_settings(app: &Arc<App>) {
    let Some(ui) = app.ui.upgrade() else { return };
    let port_text = ui.get_port_text().to_string();
    let streams_text = ui.get_max_streams_text().to_string();
    let current = app.settings_snapshot();

    let port: u16 = match port_text.trim().parse() {
        Ok(port) if (1024..=65535).contains(&port) => port,
        _ => {
            app.toast("error", "The relay port must be a number between 1024 and 65535.");
            ui.set_port_text(SharedString::from(current.port.to_string()));
            return;
        }
    };
    let streams: u16 = match streams_text.trim().parse() {
        Ok(streams) if (1..=64).contains(&streams) => streams,
        _ => {
            app.toast("error", "Maximum streams must be a number between 1 and 64.");
            ui.set_max_streams_text(SharedString::from(current.max_streams.to_string()));
            return;
        }
    };

    let port_changed = port != current.port;
    app.update_settings(|settings| {
        settings.port = port;
        settings.max_streams = streams;
    });
    write_env_settings(app, Some(port), Some(streams), None);

    {
        let mut relay = app.relay.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if !relay.active() {
            relay.port = port;
        }
    }

    if port_changed && app.relay_active() {
        app.toast("info", "Port saved. Restart the relay to use it.");
    } else {
        app.toast("ok", "Settings saved.");
    }
    app.refresh();
}

fn write_env_settings(app: &Arc<App>, port: Option<u16>, streams: Option<u16>, log_level: Option<&str>) {
    let port_text = port.map(|value| value.to_string());
    let streams_text = streams.map(|value| value.to_string());
    let level = log_level.map(|value| value.to_string());

    let mut values: Vec<(&str, &str)> = Vec::new();
    if let Some(value) = port_text.as_deref() {
        values.push(("PORT", value));
    }
    if let Some(value) = streams_text.as_deref() {
        values.push(("MAX_PROXY_STREAMS", value));
    }
    if let Some(value) = level.as_deref() {
        values.push(("LOG_LEVEL", value));
    }
    if values.is_empty() {
        return;
    }

    match crate::envfile::set_values(&app.paths.env_file, &values) {
        Ok(Some(backup)) => tracing::info!("Updated .env (backup: {})", backup.display()),
        Ok(None) => {}
        Err(error) => {
            tracing::warn!("Could not update .env: {error}");
            app.toast("error", ".env could not be updated. Check that the folder is writable.");
        }
    }
}

fn reset_preferences(app: &Arc<App>) {
    let defaults = Settings::default();
    {
        let mut settings = app.settings.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *settings = defaults.clone();
        settings.save(&app.paths);
    }
    *app.log_level.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = defaults.log_level.clone();
    if let Some(ui) = app.ui.upgrade() {
        ui.set_port_text(SharedString::from(defaults.port.to_string()));
        ui.set_max_streams_text(SharedString::from(defaults.max_streams.to_string()));
    }
    tracing::info!("Preferences reset to defaults (.env and PROXY_SECRET untouched)");
    app.toast("ok", "Preferences reset. .env and PROXY_SECRET are unchanged.");
    app.refresh();
}

// --------------------------------------------------------------------- window

pub fn copy_manifest(app: &Arc<App>) {
    let Some(manifest) = app.manifest() else {
        app.toast("warn", "There is no addon link yet. Start the relay first.");
        return;
    };
    if crate::winutil::copy_text(&manifest) {
        app.toast("ok", "Manifest URL copied.");
    } else {
        app.report_error("The clipboard could not be filled.");
    }
}

pub fn show_window(app: &Arc<App>) {
    if let Some(ui) = app.ui.upgrade() {
        let _ = ui.show();
        // The window comes back from the tray with its top rows unpainted.
        crate::winutil::refresh_surface("DhakaFlix Relay");
    }
}

pub fn hide_window(app: &Arc<App>) {
    if !app.settings_snapshot().minimize_to_tray {
        request_quit(app);
        return;
    }
    if let Some(ui) = app.ui.upgrade() {
        let _ = ui.hide();
    }
    if !app.tray_hint_shown.swap(true, Ordering::SeqCst) {
        // Off the UI thread: a modal box here would freeze the window, the tray
        // menu and every repaint until it is dismissed.
        std::thread::spawn(|| {
            crate::winutil::message_box(
                "DhakaFlix Relay",
                "DhakaFlix Relay keeps running in the notification area.\r\n\r\n\
                 Right-click the tray icon to open the dashboard or to quit.",
            );
        });
    }
}

pub fn request_quit(app: &Arc<App>) {
    show_window(app);
    let running = app.relay_active();

    if !running {
        quit_now(app);
        return;
    }

    if app.mode().is_tailscale() {
        app.confirm_dialog(
            ConfirmAction::Quit,
            "Quit DhakaFlix Relay?",
            "The relay stops, so the ts.net address answers again only after the app restarts. The addon in Stremio stays installed.",
            "Quit and stop relay",
            false,
            "",
        );
    } else {
        app.confirm_dialog(
            ConfirmAction::Quit,
            "Remove the addon before quitting",
            "The quick-tunnel address changes on every start, so Stremio keeps a dead link. Remove DhakaFlix in Stremio → Addons → Installed, then quit.",
            "Quit and stop relay",
            true,
            "Open Stremio",
        );
    }
}

pub fn handle_confirm(app: &Arc<App>) {
    let action = app.confirm.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
    app.hide_confirm();
    match action {
        Some(ConfirmAction::Quit) => quit_now(app),
        Some(ConfirmAction::ResetPreferences) => reset_preferences(app),
        None => {}
    }
}

pub fn quit_now(app: &Arc<App>) {
    if app.quitting.swap(true, Ordering::SeqCst) {
        return;
    }

    let handle = app.handle.clone();
    let app = app.clone();
    handle.spawn(async move {
        if app.relay_active() {
            let _ = stop_relay(&app).await;
        }
        let _ = slint::invoke_from_event_loop(|| {
            let _ = slint::quit_event_loop();
        });
    });
}

