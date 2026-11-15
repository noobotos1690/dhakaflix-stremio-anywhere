//! Relay process state and output interpretation.

use std::time::{Duration, Instant};

use crate::settings::AccessMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

pub struct Relay {
    pub phase: Phase,
    /// True when the relay was already running before this app started it.
    pub external: bool,
    pub port: u16,
    pub pid: Option<u32>,
    pub started_at: Option<Instant>,
    pub public_url: Option<String>,
    pub tunnel: bool,
    pub last_error: Option<String>,
    pub child: Option<tokio::process::Child>,
    pub mode: AccessMode,
    pub exit_seen: bool,
}

impl Relay {
    pub fn new(port: u16, mode: AccessMode) -> Self {
        Self {
            phase: Phase::Stopped,
            external: false,
            port,
            pid: None,
            started_at: None,
            public_url: None,
            tunnel: false,
            last_error: None,
            child: None,
            mode,
            exit_seen: false,
        }
    }

    pub fn active(&self) -> bool {
        matches!(self.phase, Phase::Starting | Phase::Running | Phase::Stopping)
    }

    pub fn running(&self) -> bool {
        matches!(self.phase, Phase::Running)
    }

    pub fn uptime(&self) -> Option<Duration> {
        self.started_at.map(|started| started.elapsed())
    }

    /// Status text plus the design-token kind used for its dot.
    pub fn label(&self) -> (&'static str, &'static str) {
        match self.phase {
            Phase::Stopped => ("Stopped", "idle"),
            Phase::Starting => ("Starting…", "warn"),
            Phase::Running if self.external => ("Running (external)", "ok"),
            Phase::Running => ("Running", "ok"),
            Phase::Stopping => ("Stopping…", "warn"),
            Phase::Error => ("Needs attention", "error"),
        }
    }

    pub fn reset(&mut self) {
        self.phase = Phase::Stopped;
        self.external = false;
        self.pid = None;
        self.started_at = None;
        self.last_error = None;
        self.child = None;
        self.exit_seen = false;
    }
}

pub fn format_uptime(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    format!("{hours:02}:{minutes:02}:{secs:02}")
}

/// Remove the ANSI colour sequences the relay prints on a console.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if matches!(chars.peek(), Some('[')) {
                chars.next();
                while let Some(next) = chars.next() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out.trim_end().to_string()
}

fn url_after(text: &str) -> Option<String> {
    let start = text.find("https://")?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')')
        .unwrap_or(rest.len());
    let url = &rest[..end];

    // Keep only the origin. The banner prints "/manifest.json", "/" and
    // "/health" variants, and any path left on would corrupt every link built
    // from it (the addon link became https://host/health/manifest.json).
    let after_scheme = &url["https://".len()..];
    let authority_len = after_scheme.find('/').unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_len];
    if authority.is_empty() {
        return None;
    }
    Some(format!("https://{authority}"))
}

/// Recognise the public origin in the relay's own banner and log lines.
pub fn detect_public_url(line: &str) -> Option<String> {
    for marker in ["Public URL:", "install", "landing"] {
        if let Some(index) = line.find(marker) {
            if let Some(url) = url_after(&line[index + marker.len()..]) {
                return Some(url);
            }
        }
    }
    if line.contains("trycloudflare.com") {
        return url_after(line);
    }
    None
}

pub fn looks_like_error(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.contains("error")
        || lower.contains("failed")
        || lower.contains("fatal")
        || lower.contains("eaddrinuse")
        || lower.contains("econnrefused")
}

pub fn looks_like_warning(line: &str) -> bool {
    let lower = line.to_lowercase();
    lower.contains("warn") || lower.contains("[config]") || lower.contains("timeout")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_public_urls_from_relay_output() {
        assert_eq!(
            detect_public_url("[TUNNEL] Public URL: https://calm-river-dawn.trycloudflare.com")
                .as_deref(),
            Some("https://calm-river-dawn.trycloudflare.com")
        );
        assert_eq!(
            detect_public_url("   install    https://calm-river-dawn.trycloudflare.com/manifest.json")
                .as_deref(),
            Some("https://calm-river-dawn.trycloudflare.com")
        );
        assert_eq!(
            detect_public_url("   landing    https://calm-river-dawn.trycloudflare.com/").as_deref(),
            Some("https://calm-river-dawn.trycloudflare.com")
        );
        // The banner's health line used to overwrite the origin with "/health".
        assert_eq!(
            detect_public_url("   health     https://calm-river-dawn.trycloudflare.com/health")
                .as_deref(),
            Some("https://calm-river-dawn.trycloudflare.com")
        );
        assert_eq!(
            detect_public_url("   deeplink   stremio://calm-river-dawn.trycloudflare.com/manifest.json")
                .as_deref(),
            None
        );
        assert!(detect_public_url("Listening on 0.0.0.0:7001").is_none());
    }

    #[test]
    fn strips_ansi_and_formats_uptime() {
        assert_eq!(strip_ansi("\u{1b}[32m[BOOT] ready\u{1b}[0m"), "[BOOT] ready");
        assert_eq!(format_uptime(3725), "01:02:05");
    }

    #[test]
    fn classifies_output_levels() {
        assert!(looks_like_error("Error: listen EADDRINUSE: address already in use"));
        assert!(looks_like_warning("[WARN] PROXY_SECRET is not set"));
        assert!(!looks_like_error("[HTTP] GET /health -> 200 2ms"));
    }
}
