//! Thin wrappers around the handful of Win32 calls the app needs. Keeping them
//! here means no extra crate, no console window flashing and one place to audit.

use std::ffi::OsStr;
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HWND};
use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetWindowRect, IsIconic, IsWindowVisible, IsZoomed, MessageBoxW, SetWindowPos,
    MB_ICONINFORMATION, MB_OK, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SW_SHOWNORMAL,
};

pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const ERROR_ALREADY_EXISTS: u32 = 183;

pub fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

/// Hide the console window of a child process.
pub fn hide_console(command: &mut Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

pub fn hide_console_async(command: &mut tokio::process::Command) {
    command.creation_flags(CREATE_NO_WINDOW);
}

fn shell_open(target: &str) -> bool {
    let verb = wide("open");
    let file = wide(target);
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    result as isize > 32
}

/// Paint the native title bar dark so it matches the window body.
///
/// Windows 10 1809+ honours DWMWA_USE_IMMERSIVE_DARK_MODE (20); older builds
/// simply ignore it and keep the default frame.
pub fn use_dark_title_bar(title: &str) {
    let title = wide(title);
    for _ in 0..40 {
        let handle: HWND = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
        if !handle.is_null() {
            let enabled: i32 = 1;
            let result = unsafe {
                DwmSetWindowAttribute(
                    handle,
                    DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                    &enabled as *const i32 as *const _,
                    std::mem::size_of::<i32>() as u32,
                )
            };
            if result == 0 {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// True when the window is on screen and not minimized, `None` when it is gone.
///
/// Windows discards the drawing surface of a minimized or hidden window, and
/// Slint repaints only the regions it believes changed, so the window can come
/// back with a blank strip at the top where whatever is behind shows through.
/// The ticker watches this to repaint once the window is back.
pub fn window_shown(title: &str) -> Option<bool> {
    let title = wide(title);
    let handle = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if handle.is_null() {
        return None;
    }

    let visible = unsafe { IsWindowVisible(handle) } != 0;
    let minimized = unsafe { IsIconic(handle) } != 0;
    Some(visible && !minimized)
}

/// Make the window paint itself again from a fresh surface.
///
/// Windows throws the drawing surface of a minimized or hidden window away, and
/// the window comes back with its top rows never painted, so whatever is behind
/// shows through them. Asking for a redraw does not repair that, because the
/// region is not considered damaged; the surface has to be recreated, which a
/// one-pixel resize does. Maximized windows are skipped, since resizing one
/// restores it.
pub fn refresh_surface(title: &str) {
    let title = wide(title);
    let handle = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if handle.is_null() {
        return;
    }
    if unsafe { IsIconic(handle) } != 0 || unsafe { IsZoomed(handle) } != 0 {
        return;
    }

    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    if unsafe { GetWindowRect(handle, &mut rect) } == 0 {
        return;
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 2 || height < 2 {
        return;
    }

    unsafe {
        SetWindowPos(
            handle,
            std::ptr::null_mut(),
            0,
            0,
            width,
            height + 1,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
        SetWindowPos(
            handle,
            std::ptr::null_mut(),
            0,
            0,
            width,
            height,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub fn open_url(url: &str) -> bool {
    shell_open(url)
}

/// Open a URL, falling back to the Stremio executable if the protocol handler
/// is not registered.
pub fn open_stremio_deep_link(link: &str) -> bool {
    if shell_open(link) {
        return true;
    }
    if let Some(exe) = crate::stremio::find_executable() {
        let mut command = Command::new(exe);
        command.arg(link);
        hide_console(&mut command);
        return command.spawn().is_ok();
    }
    false
}

pub fn open_folder(path: &Path) -> bool {
    if path.exists() {
        shell_open(&path.to_string_lossy())
    } else if let Some(parent) = path.parent() {
        shell_open(&parent.to_string_lossy())
    } else {
        false
    }
}

/// Copy text through the Windows clipboard helper that ships with Windows.
pub fn copy_text(text: &str) -> bool {
    let mut command = Command::new("cmd");
    command.args(["/C", "clip"]);
    hide_console(&mut command);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());

    match command.spawn() {
        Ok(mut child) => {
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(text.as_bytes());
            }
            child.wait().map(|status| status.success()).unwrap_or(false)
        }
        Err(_) => false,
    }
}

pub fn message_box(title: &str, text: &str) {
    let title = wide(title);
    let text = wide(text);
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

/// True when something is already listening on the port.
pub fn port_busy(port: u16) -> bool {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    TcpStream::connect_timeout(&address, Duration::from_millis(350)).is_ok()
}

/// Terminate a process and its children. `taskkill /T` first (no `/F`) asks the
/// tree to close, then a forced pass makes sure nothing survives.
pub fn stop_process_tree(pid: u32, force: bool) {
    let mut arguments = vec!["/PID".to_string(), pid.to_string(), "/T".to_string()];
    if force {
        arguments.push("/F".to_string());
    }
    let mut command = Command::new("taskkill");
    command.args(&arguments);
    hide_console(&mut command);
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    let _ = command.status();
}

/// Named mutex so a second launch cannot start a second relay. The name matches
/// the one the installer uses for `CloseApplications`.
pub struct InstanceGuard(*mut core::ffi::c_void);

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub fn acquire_instance() -> Option<InstanceGuard> {
    let name = wide("Local\\DhakaFlixRelay.Desktop");
    let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
    if handle.is_null() {
        return None;
    }
    let error = unsafe { GetLastError() };
    if error == ERROR_ALREADY_EXISTS {
        unsafe {
            CloseHandle(handle);
        }
        return None;
    }
    Some(InstanceGuard(handle))
}

pub const ACTIVATION_PORT: u16 = 47812;

/// A running instance listens here so a second launch can raise the window
/// instead of failing silently.
pub fn notify_existing_instance() -> bool {
    use std::io::Write as _;
    use std::net::TcpStream;
    match TcpStream::connect(("127.0.0.1", ACTIVATION_PORT)) {
        Ok(mut stream) => {
            let _ = stream.write_all(b"show\n");
            true
        }
        Err(_) => false,
    }
}

pub fn spawn_activation_listener<F: Fn() + Send + 'static>(on_activate: F) {
    std::thread::spawn(move || {
        use std::io::{BufRead, BufReader};
        use std::net::TcpListener;
        let Ok(listener) = TcpListener::bind(("127.0.0.1", ACTIVATION_PORT)) else {
            return;
        };
        for stream in listener.incoming().flatten() {
            let mut line = String::new();
            if BufReader::new(stream).read_line(&mut line).is_ok() && line.starts_with("show") {
                on_activate();
            }
        }
    });
}
