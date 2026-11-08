use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "DhakaFlix Relay";

pub fn is_enabled() -> bool {
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_KEY, KEY_READ)
        .ok()
        .and_then(|key| key.get_value::<String, _>(VALUE).ok())
        .is_some()
}

pub fn set_enabled(enabled: bool, exe: &std::path::Path) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE).map_err(|e| e.to_string())?;
    if enabled {
        let command = format!("\"{}\" --autostart", exe.display());
        key.set_value(VALUE, &command).map_err(|e| e.to_string())
    } else {
        match key.delete_value(VALUE) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}
