//! Line-based `.env` editing.
//!
//! Comments and unrelated keys are preserved, `PROXY_SECRET` is never touched,
//! and a timestamped backup is written before an existing file changes. This is
//! the same behaviour the C# app had, so both builds can edit the same file.

use std::io;
use std::path::{Path, PathBuf};

fn active_key(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let index = trimmed.find('=')?;
    if index == 0 {
        return None;
    }
    let key = trimmed[..index].trim();
    if key.is_empty() || key.contains(' ') || key.contains('\t') {
        return None;
    }
    Some(key)
}

pub fn read_value(path: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines() {
        match active_key(line) {
            Some(active) if active.eq_ignore_ascii_case(key) => {
                let index = line.find('=')?;
                let value = line[index + 1..].trim().trim_matches('"').trim_matches('\'');
                return Some(value.to_string());
            }
            _ => continue,
        }
    }
    None
}

/// Replace (or append) the given keys. Returns the backup path when one was made.
pub fn set_values(path: &Path, values: &[(&str, &str)]) -> io::Result<Option<PathBuf>> {
    if values.is_empty() {
        return Ok(None);
    }

    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let newline = if existing.contains("\r\n") { "\r\n" } else { "\n" };
    let mut remaining: Vec<(String, String)> = values
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();

    let mut lines: Vec<String> = if existing.is_empty() {
        Vec::new()
    } else {
        existing
            .replace("\r\n", "\n")
            .split('\n')
            .map(|line| line.to_string())
            .collect()
    };

    for line in lines.iter_mut() {
        let Some(key) = active_key(line) else { continue };
        if let Some(index) = remaining
            .iter()
            .position(|(name, _)| name.eq_ignore_ascii_case(key))
        {
            let (name, value) = remaining.remove(index);
            *line = format!("{name}={value}");
        }
    }

    while matches!(lines.last(), Some(line) if line.is_empty()) {
        lines.pop();
    }
    for (name, value) in remaining {
        lines.push(format!("{name}={value}"));
    }

    let updated = format!("{}{newline}", lines.join(newline));
    if updated == existing {
        return Ok(None);
    }

    let mkdir_target = path.parent().map(Path::to_path_buf);
    if let Some(dir) = mkdir_target {
        std::fs::create_dir_all(dir)?;
    }

    let backup = if path.exists() {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let backup = path.with_extension(format!("bak-{stamp}"));
        std::fs::copy(path, &backup)?;
        Some(backup)
    } else {
        None
    };

    std::fs::write(path, updated)?;
    Ok(backup)
}

/// Read `PROXY_SECRET` if it is long enough to be a real signing key.
pub fn read_secret(path: &Path) -> Option<String> {
    let value = read_value(path, "PROXY_SECRET")?;
    if value.len() >= 32 {
        Some(value)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_in_place_and_keeps_comments() {
        let dir = std::env::temp_dir().join(format!("dhakaflix-env-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".env");
        std::fs::write(
            &path,
            "# keep me\r\nPORT=7001\r\nPROXY_SECRET=abc\r\nTUNNEL=0\r\n",
        )
        .unwrap();

        set_values(&path, &[("PORT", "7100"), ("PUBLIC_URL", "https://x.ts.net")]).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();

        assert!(text.contains("# keep me"));
        assert!(text.contains("PORT=7100"));
        assert!(text.contains("TUNNEL=0"));
        assert!(text.contains("PUBLIC_URL=https://x.ts.net"));
        assert!(text.contains("PROXY_SECRET=abc"));
        assert_eq!(read_value(&path, "port").as_deref(), Some("7100"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
