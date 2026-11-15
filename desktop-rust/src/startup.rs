//! Per-user "start with Windows" entry (HKCU Run key, no admin needed).
//!
//! The value name is the same one the C# build used, so enabling startup before
//! the upgrade keeps working and a rollback sees the setting intact.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "DhakaFlix Relay";

fn wide(value: &str) -> Vec<u16> {
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(Some(0))
        .collect()
}

fn open_key(write: bool) -> Option<HKEY> {
    let sub = wide(RUN_KEY);
    let mut key: HKEY = std::ptr::null_mut();
    let access = if write { KEY_READ | KEY_WRITE } else { KEY_READ };
    let result = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, access, &mut key) };
    if result == ERROR_SUCCESS && !key.is_null() {
        Some(key)
    } else {
        None
    }
}

pub fn current_command() -> Option<String> {
    let key = open_key(false)?;
    let name = wide(VALUE_NAME);
    let mut size: u32 = 0;
    let status = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS || size == 0 {
        unsafe { RegCloseKey(key) };
        return None;
    }
    let mut buffer = vec![0u16; (size as usize / 2) + 2];
    let mut read = size;
    let status = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null(),
            std::ptr::null_mut(),
            buffer.as_mut_ptr() as *mut u8,
            &mut read,
        )
    };
    unsafe { RegCloseKey(key) };
    if status != ERROR_SUCCESS {
        return None;
    }
    let chars = (read as usize / 2).saturating_sub(1);
    let value = String::from_utf16_lossy(&buffer[..chars.min(buffer.len())]);
    let value = value.trim_end_matches('\0').to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

pub fn is_enabled() -> bool {
    current_command().is_some()
}

pub fn enable(exe: &Path) -> Result<(), String> {
    let key = open_key(true).ok_or_else(|| "the Run registry key could not be opened".to_string())?;
    let name = wide(VALUE_NAME);
    let command = format!("\"{}\" --autostart", exe.display());
    let data: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
    let bytes = data.len() * std::mem::size_of::<u16>();

    let status = unsafe {
        RegSetValueExW(
            key,
            name.as_ptr(),
            0,
            REG_SZ,
            data.as_ptr() as *const u8,
            bytes as u32,
        )
    };
    unsafe { RegCloseKey(key) };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(format!("Windows rejected the startup entry (code {status})"))
    }
}

pub fn disable() -> Result<(), String> {
    let key = open_key(true).ok_or_else(|| "the Run registry key could not be opened".to_string())?;
    let name = wide(VALUE_NAME);
    let status = unsafe { RegDeleteValueW(key, name.as_ptr()) };
    unsafe { RegCloseKey(key) };
    if status == ERROR_SUCCESS || status == 2 {
        Ok(())
    } else {
        Err(format!("Windows rejected the removal (code {status})"))
    }
}

/// Guard against unused-import warnings on the helper type above.
#[allow(dead_code)]
fn _assert_handle_type(_: *mut c_void) {}
