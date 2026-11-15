#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! DhakaFlix Relay — native Windows companion for the DhakaFlix Stremio relay.
//!
//! The Node.js relay stays a separate process; this crate only drives it and
//! presents its state.

mod app;
mod config;
mod controller;
mod deps;
mod envfile;
mod health;
mod http;
mod logging;
mod relay;
mod settings;
mod startup;
mod stremio;
mod tailscale;
mod updates;
mod winutil;

slint::include_modules!();

use std::sync::Arc;

use slint::ComponentHandle;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths = config::AppPaths::discover();
    paths.ensure_dirs();

    logging::install(paths.log_file.clone());
    // Anything that goes wrong after this point leaves a trail on disk.
    logging::install_panic_hook(paths.settings_dir.join("crash.log"));
    tracing::info!(
        "DhakaFlix Relay {} starting (relay root: {})",
        env!("CARGO_PKG_VERSION"),
        paths.root.display()
    );

    // A second launch raises the existing window instead of starting a second relay.
    let Some(_instance) = winutil::acquire_instance() else {
        tracing::info!("Another DhakaFlix Relay instance is running; raising it instead");
        if !winutil::notify_existing_instance() {
            winutil::message_box(
                "DhakaFlix Relay",
                "DhakaFlix Relay is already running. Look for its icon in the notification area.",
            );
        }
        return Ok(());
    };

    let settings = settings::Settings::load(&paths);
    let autostart = std::env::args().any(|argument| argument == "--autostart");

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("dhakaflix-worker")
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            fatal(&format!("The background runtime could not start: {error}"));
            return Ok(());
        }
    };
    let _runtime_guard = runtime.enter();

    // A dashboard repaints a few times a second, so the CPU renderer is plenty
    // and it behaves the same on machines without usable OpenGL drivers.
    if let Err(error) = slint::BackendSelector::new()
        .renderer_name("software".to_string())
        .select()
    {
        tracing::warn!("Software renderer unavailable ({error}); using the default renderer");
    }

    tracing::info!("Creating the window");
    let ui = match MainWindow::new() {
        Ok(ui) => ui,
        Err(error) => {
            fatal(&format!("The window could not be created: {error}"));
            return Ok(());
        }
    };
    tracing::info!("Creating the tray icon");
    let tray = match TrayMenu::new() {
        Ok(tray) => tray,
        Err(error) => {
            fatal(&format!("The tray icon could not be created: {error}"));
            return Ok(());
        }
    };

    let app = Arc::new(app::App::new(
        paths.clone(),
        settings.clone(),
        ui.as_weak(),
        tray.as_weak(),
        runtime.handle().clone(),
    ));

    controller::wire(&ui, &tray, &app);

    ui.set_page(0);
    ui.set_port_text(slint::SharedString::from(settings.port.to_string()));
    ui.set_max_streams_text(slint::SharedString::from(settings.max_streams.to_string()));
    ui.set_log_level(slint::SharedString::from(settings.log_level.clone()));
    ui.set_log_follow(settings.follow_logs);

    app.refresh();
    app.render_logs();

    // Listen for a second launch before the window appears, so double-clicking
    // the shortcut twice never ends in two processes or a silent exit.
    {
        let ui_weak = ui.as_weak();
        winutil::spawn_activation_listener(move || {
            let ui_weak = ui_weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = ui_weak.upgrade() {
                    let _ = ui.show();
                    winutil::refresh_surface("DhakaFlix Relay");
                }
            });
        });
    }

    tracing::info!("Showing the tray icon");
    // A missing tray icon is not worth losing the window over.
    if let Err(error) = tray.show() {
        tracing::warn!("The tray icon could not be shown: {error}");
    }
    if !autostart {
        tracing::info!("Showing the window");
        if let Err(error) = ui.show() {
            fatal(&format!("The window could not be shown: {error}"));
            return Ok(());
        }
    }
    std::thread::spawn(|| winutil::use_dark_title_bar("DhakaFlix Relay"));

    tracing::info!("Starting background tasks");
    controller::spawn_tasks(&app);
    app.check_for_updates();

    tracing::info!("Entering the event loop");
    if let Err(error) = slint::run_event_loop() {
        fatal(&format!("The interface stopped unexpectedly: {error}"));
        return Ok(());
    }
    tracing::info!("Event loop finished, exiting");
    Ok(())
}

/// Report a startup failure where the user can see it and keep the reason on disk.
fn fatal(message: &str) {
    tracing::error!("{message}");
    winutil::message_box("DhakaFlix Relay", message);
}
