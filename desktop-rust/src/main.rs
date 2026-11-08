#![windows_subsystem = "windows"]

mod app;
mod cloudflare;
mod dependencies;
mod diagnostics;
mod envfile;
mod health;
mod logging;
mod paths;
mod relay;
mod settings;
mod startup;
mod state;
mod tailscale;
mod tray;

use std::rc::Rc;
use std::sync::{Arc, Mutex};

use slint::ComponentHandle;
use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK};

slint::include_modules!();

fn main() {
    let autostart = std::env::args().any(|a| a.eq_ignore_ascii_case("--autostart"));
    let _mutex = match single_instance() {
        Ok(handle) => handle,
        Err(()) => return,
    };

    let paths = paths::Paths::discover();
    paths.ensure();
    let settings = settings::Settings::load(&paths);
    let log_store = logging::init(&paths.log_file);
    logging::info(format!("DhakaFlix Relay 2.0.0 starting from {}", paths.root.display()));

    let shared = Arc::new(Mutex::new(state::AppState::from_settings(&settings)));
    let settings = Arc::new(Mutex::new(settings));
    let tailscale = Arc::new(tailscale::Tailscale::discover());
    let deps = Arc::new(dependencies::Dependencies::new(paths.clone()));
    let relay = relay::RelayManager::new(
        paths.clone(),
        settings.clone(),
        shared.clone(),
        tailscale.clone(),
        deps.clone(),
    );

    let ui = MainWindow::new().expect("Could not create the Slint window");
    let app = Rc::new(app::App::new(
        ui,
        paths.clone(),
        settings.clone(),
        shared.clone(),
        deps,
        tailscale,
        relay,
        log_store,
    ));
    app::App::wire(&app);
    app.initialize(autostart);
    health::spawn_monitor(shared, settings.lock().unwrap().port);

    let _timer = app::start_timer(app.clone());
    if let Err(error) = app.ui.run() {
        logging::error(format!("UI loop ended with an error: {error}"));
    }
    logging::info("DhakaFlix Relay exited.");
}

fn single_instance() -> Result<isize, ()> {
    let name: Vec<u16> = "Local\\DhakaFlixRelayRust2".encode_utf16().chain(std::iter::once(0)).collect();
    let handle = unsafe { CreateMutexW(std::ptr::null_mut(), 1, name.as_ptr()) };
    if handle.is_null() {
        return Ok(0);
    }
    let error = unsafe { GetLastError() };
    if error == ERROR_ALREADY_EXISTS {
        let text: Vec<u16> = "DhakaFlix Relay is already running. Open it from the notification area.".encode_utf16().chain(std::iter::once(0)).collect();
        let title: Vec<u16> = "DhakaFlix Relay".encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONINFORMATION);
        }
        return Err(());
    }
    Ok(handle as isize)
}
