#![cfg(windows)]

use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy, Debug)]
pub enum TrayCommand {
    Show,
    Start,
    Stop,
    Exit,
}

static SENDER: OnceLock<Mutex<Option<Sender<TrayCommand>>>> = OnceLock::new();
static ICON_PATH: OnceLock<String> = OnceLock::new();

pub struct Tray {
    hwnd: isize,
    pub receiver: Receiver<TrayCommand>,
}

impl Tray {
    pub fn spawn(icon: &Path) -> Option<Self> {
        let icon_path = icon.to_string_lossy().to_string();
        crate::logging::info(format!("Tray: creating icon from {icon_path}"));
        let _ = ICON_PATH.set(icon_path);
        let (tx, rx) = channel();
        if SENDER.set(Mutex::new(Some(tx))).is_err() {
            crate::logging::warn("Tray: sender already initialized");
            return None;
        }
        let (ready_tx, ready_rx) = channel::<isize>();
        std::thread::spawn(move || unsafe {
            let hwnd = create_tray_window();
            crate::logging::info(format!("Tray: window handle {hwnd:?}"));
            let _ = ready_tx.send(hwnd as isize);
            if hwnd.is_null() {
                crate::logging::warn("Tray: window creation failed");
                return;
            }
            let mut msg = std::mem::zeroed::<MSG>();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
        let hwnd = match ready_rx.recv_timeout(std::time::Duration::from_secs(3)) {
            Ok(hwnd) => hwnd,
            Err(error) => {
                crate::logging::warn(format!("Tray: ready timeout: {error}"));
                return None;
            }
        };
        if hwnd == 0 {
            return None;
        }
        Some(Self { hwnd, receiver: rx })
    }

    pub fn hide(&self) {
        // Menu-driven commands are sent through the channel; hiding is handled
        // by the Slint side so the window object stays alive.
    }

    pub fn shutdown(&self) {
        unsafe {
            PostMessageW(self.hwnd as HWND, WM_CLOSE, 0, 0);
        }
    }
}

unsafe fn create_tray_window() -> HWND {
    let instance = GetModuleHandleW(std::ptr::null());
    let class_name = wide("DhakaFlixRelayTrayWindow");
    let window_name = wide("DhakaFlix Relay");
    let wc = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: std::ptr::null_mut(),
        hCursor: std::ptr::null_mut(),
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: class_name.as_ptr(),
    };
    RegisterClassW(&wc);
    let hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        window_name.as_ptr(),
        0,
        0,
        0,
        0,
        0,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        instance,
        std::ptr::null_mut(),
    );
    if hwnd.is_null() {
        return hwnd;
    }

    let icon_path = ICON_PATH.get().map(|s| wide(s)).unwrap_or_default();
    let mut icon = 0usize;
    if !icon_path.is_empty() {
        icon = LoadImageW(
            std::ptr::null_mut(),
            icon_path.as_ptr(),
            IMAGE_ICON,
            16,
            16,
            LR_LOADFROMFILE | LR_DEFAULTSIZE,
        ) as usize;
    }
    if icon == 0 {
        icon = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) as usize;
    }

    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_APP + 1;
    nid.hIcon = icon as *mut core::ffi::c_void;
    let tip = wide("DhakaFlix Relay");
    for (i, c) in tip.iter().take(127).enumerate() {
        nid.szTip[i] = *c;
    }
    Shell_NotifyIconW(NIM_ADD, &nid);
    hwnd
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        x if x == WM_APP + 1 => {
            let event = lparam as u32;
            if event == WM_LBUTTONDBLCLK {
                send(TrayCommand::Show);
            } else if event == WM_RBUTTONUP {
                show_menu(hwnd);
            }
            0
        }
        WM_COMMAND => {
            match wparam & 0xffff {
                1 => send(TrayCommand::Show),
                2 => send(TrayCommand::Start),
                3 => send(TrayCommand::Stop),
                4 => send(TrayCommand::Exit),
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
            nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = hwnd;
            nid.uID = 1;
            Shell_NotifyIconW(NIM_DELETE, &nid);
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn show_menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    let show = wide("Open DhakaFlix Relay");
    let start = wide("Start relay");
    let stop = wide("Stop relay");
    let exit = wide("Exit");
    AppendMenuW(menu, MF_STRING, 1, show.as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
    AppendMenuW(menu, MF_STRING, 2, start.as_ptr());
    AppendMenuW(menu, MF_STRING, 3, stop.as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
    AppendMenuW(menu, MF_STRING, 4, exit.as_ptr());

    let mut point = POINT { x: 0, y: 0 };
    GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    let command = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        point.x,
        point.y,
        0,
        hwnd,
        std::ptr::null(),
    );
    DestroyMenu(menu);
    match command {
        1 => send(TrayCommand::Show),
        2 => send(TrayCommand::Start),
        3 => send(TrayCommand::Stop),
        4 => send(TrayCommand::Exit),
        _ => {}
    }
}

fn send(command: TrayCommand) {
    if let Some(lock) = SENDER.get() {
        if let Ok(guard) = lock.lock() {
            if let Some(tx) = guard.as_ref() {
                let _ = tx.send(command);
            }
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
