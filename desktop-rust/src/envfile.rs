use chrono::Local;
use rand::RngCore;
use std::fs;
use std::path::Path;

/// Line-based .env editor. It never rewrites the whole file and never touches
/// PROXY_SECRET unless it is creating a brand-new .env.
pub fn ensure_env(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    let template = path
        .parent()
        .map(|p| p.join(".env.example"))
        .filter(|p| p.exists())
        .and_then(|p| fs::read_to_string(p).ok());

    let mut text = template.unwrap_or_else(|| {
        String::from(
            "# DhakaFlix Relay configuration\nPORT=7001\nHOST=0.0.0.0\nPUBLIC_URL=\nSTREAM_MODE=proxy\nPROXY_TTL=604800\nMAX_PROXY_STREAMS=8\nTUNNEL=0\nTUNNEL_TOKEN=\nLOG_LEVEL=info\n",
        )
    });
    if !has_key(&text, "PROXY_SECRET") {
        text.push_str(&format!("# Generated locally. Do not share.\nPROXY_SECRET={}\n", new_secret()));
    }
    fs::write(path, text).map_err(|e| e.to_string())
}

pub fn new_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn has_key(text: &str, key: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim();
        !line.starts_with('#') && line.split_once('=').map(|(k, _)| k.trim() == key).unwrap_or(false)
    })
}

pub fn read_value(path: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                return Some(v.trim().trim_matches('"').trim_matches('\'').to_string());
            }
        }
    }
    None
}

/// Update values, append missing keys, and make a timestamped backup first.
pub fn update(path: &Path, values: &[(&str, &str)]) -> Result<(), String> {
    let original = fs::read_to_string(path).unwrap_or_default();
    let newline = if original.contains("\r\n") { "\r\n" } else { "\n" };
    backup(path)?;

    let mut lines: Vec<String> = original.lines().map(ToString::to_string).collect();
    for (key, value) in values {
        let mut found = false;
        for line in &mut lines {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                continue;
            }
            if let Some((k, _)) = trimmed.split_once('=') {
                if k.trim() == *key {
                    *line = format!("{}={}", key, value);
                    found = true;
                    break;
                }
            }
        }
        if !found {
            lines.push(format!("{}={}", key, value));
        }
    }

    let mut text = lines.join(newline);
    text.push_str(newline);
    fs::write(path, text).map_err(|e| e.to_string())
}

fn backup(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let backup = path.with_file_name(format!(".env.bak-{stamp}"));
    fs::copy(path, backup).map(|_| ()).map_err(|e| e.to_string())
}

pub fn apply_relay_settings(path: &Path, port: u16, max_streams: u32, log_level: &str) -> Result<(), String> {
    update(
        path,
        &[
            ("PORT", &port.to_string()),
            ("MAX_PROXY_STREAMS", &max_streams.to_string()),
            ("LOG_LEVEL", log_level),
        ],
    )
}

pub fn apply_tailscale(path: &Path, host: &str) -> Result<(), String> {
    update(
        path,
        &[
            ("PUBLIC_URL", &format!("https://{host}")),
            ("TUNNEL", "0"),
            ("TUNNEL_TOKEN", ""),
        ],
    )
}

pub fn apply_cloudflare(path: &Path, public_url: &str) -> Result<(), String> {
    update(
        path,
        &[
            ("PUBLIC_URL", public_url),
            ("TUNNEL", "1"),
            ("TUNNEL_TOKEN", ""),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn updates_without_touching_secret_or_comments() {
        let dir = std::env::temp_dir().join(format!("dflix-env-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join(".env");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "# keep me\nPROXY_SECRET=abc123\nPORT=7001\nCUSTOM=x").unwrap();
        drop(f);

        update(&path, &[("PORT", "7123"), ("PUBLIC_URL", "https://x.ts.net")]).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# keep me"));
        assert!(text.contains("PROXY_SECRET=abc123"));
        assert!(text.contains("PORT=7123"));
        assert!(text.contains("PUBLIC_URL=https://x.ts.net"));
        assert!(text.contains("CUSTOM=x"));
        assert!(dir.join(format!(".env.bak-{}", Local::now().format("%Y%m%d"))).exists()
            || fs::read_dir(&dir).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with(".env.bak-")));
    }
}
