//! Structured logging into three places at once: the UI log viewer, the log
//! file next to the app, and (in debug builds) stderr.
//!
//! Every message passes through [`redact`] first so a relay URL token, a
//! Tailscale key or the signing secret can never reach the interface or disk.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use tracing::field::{Field, Visit};

use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

const CAPACITY: usize = 2000;
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Entry {
    pub at: chrono::DateTime<chrono::Local>,
    pub level: Level,
    pub text: String,
}

impl Entry {
    pub fn level_label(&self) -> &'static str {
        match self.level {
            Level::ERROR => "error",
            Level::WARN => "warn",
            Level::INFO => "info",
            Level::DEBUG => "debug",
            Level::TRACE => "trace",
        }
    }
}

pub struct LogHub {
    entries: Mutex<VecDeque<Entry>>,
    file: Mutex<Option<File>>,
    path: PathBuf,
    secrets: Mutex<Vec<String>>,
}

impl LogHub {
    fn new(path: PathBuf) -> Self {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file = OpenOptions::new().create(true).append(true).open(&path).ok();
        Self {
            entries: Mutex::new(VecDeque::with_capacity(CAPACITY)),
            file: Mutex::new(file),
            path,
            secrets: Mutex::new(Vec::new()),
        }
    }

    /// Register a value that must never appear in a log line.
    pub fn add_secret(&self, value: &str) {
        if value.len() < 8 {
            return;
        }
        let mut secrets = self.secrets.lock().unwrap();
        if !secrets.iter().any(|existing| existing == value) {
            secrets.push(value.to_string());
        }
    }

    pub fn push(&self, level: Level, text: String) {
        let text = redact(&text, &self.secrets.lock().unwrap());
        let entry = Entry {
            at: chrono::Local::now(),
            level,
            text,
        };

        {
            let mut entries = self.entries.lock().unwrap();
            if entries.len() >= CAPACITY {
                entries.pop_front();
            }
            entries.push_back(entry.clone());
        }

        self.write_line(&entry);
    }

    fn write_line(&self, entry: &Entry) {
        let mut guard = self.file.lock().unwrap();
        let Some(file) = guard.as_mut() else { return };
        let line = format!(
            "{} [{:<5}] {}\r\n",
            entry.at.format("%Y-%m-%d %H:%M:%S"),
            entry.level_label().to_uppercase(),
            entry.text
        );
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();

        if let Ok(meta) = file.metadata() {
            if meta.len() > MAX_FILE_BYTES {
                let _ = file.flush();
                let mut rotated = self.path.clone();
                rotated.set_extension("log.1");
                let _ = std::fs::rename(&self.path, &rotated);
                *guard = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.path)
                    .ok();
            }
        }
    }

    pub fn snapshot(&self) -> Vec<Entry> {
        self.entries.lock().unwrap().iter().cloned().collect()
    }

    /// The newest `count` entries, oldest first.
    pub fn tail(&self, count: usize) -> Vec<Entry> {
        let entries = self.entries.lock().unwrap();
        let start = entries.len().saturating_sub(count);
        entries.iter().skip(start).cloned().collect()
    }

    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }
}

static HUB: OnceLock<Arc<LogHub>> = OnceLock::new();

pub fn install(path: PathBuf) -> Arc<LogHub> {
    let hub = Arc::new(LogHub::new(path));
    let _ = HUB.set(hub.clone());

    let subscriber = tracing_subscriber::registry().with(HubLayer { hub: hub.clone() });
    let _ = tracing::subscriber::set_global_default(subscriber);
    hub
}

pub fn hub() -> Option<Arc<LogHub>> {
    HUB.get().cloned()
}

/// Record panics in `crash.log`.
///
/// The release build has no console, so without this a panic closes the window
/// and leaves nothing behind on disk.
pub fn install_panic_hook(crash_file: std::path::PathBuf) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|location| format!("{}:{}:{}", location.file(), location.line(), location.column()))
            .unwrap_or_else(|| "unknown location".to_string());
        let message = if let Some(text) = info.payload().downcast_ref::<&str>() {
            (*text).to_string()
        } else if let Some(text) = info.payload().downcast_ref::<String>() {
            text.clone()
        } else {
            "panic with a non-string payload".to_string()
        };

        let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let backtrace = std::backtrace::Backtrace::force_capture();
        let record = format!(
            "\n===== {stamp} =====\n{message}\nat {location}\n{backtrace}\n"
        );
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&crash_file)
        {
            let _ = file.write_all(record.as_bytes());
        }
        tracing::error!("Unexpected error: {message} (at {location})");
        default_hook(info);
    }));
}

struct HubLayer {
    hub: Arc<LogHub>,
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
    fields: Vec<String>,
}

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields.push(format!("{}={}", field.name(), value));
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let rendered = format!("{value:?}");
        if field.name() == "message" {
            // tracing hands the message through as a Debug wrapper that quotes strings.
            self.message = rendered.trim_matches('"').to_string();
        } else {
            self.fields.push(format!("{}={}", field.name(), rendered));
        }
    }
}

impl<S: Subscriber> Layer<S> for HubLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);
        let mut text = visitor.message;
        if !visitor.fields.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&visitor.fields.join(" "));
        }
        self.hub.push(*event.metadata().level(), text);
    }
}

/// Mask anything that looks like a credential.
pub fn redact(text: &str, secrets: &[String]) -> String {
    let mut out = text.to_string();
    for secret in secrets {
        if secret.len() >= 8 && out.contains(secret.as_str()) {
            out = out.replace(secret.as_str(), "<redacted>");
        }
    }
    out = mask_prefixed(&out, "tskey-");
    out = mask_prefixed(&out, "TS_AUTHKEY=");
    out = mask_prefixed(&out, "authkey=");
    out = mask_prefixed(&out, "TUNNEL_TOKEN=");
    mask_signed_paths(&out)
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'
}

fn mask_prefixed(text: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find(prefix) {
        out.push_str(&rest[..index]);
        out.push_str(prefix);
        let after = &rest[index + prefix.len()..];
        let mut chars = after.char_indices();
        let mut end = 0;
        while let Some((offset, c)) = chars.next() {
            if !is_token_char(c) {
                end = offset;
                break;
            }
            end = offset + c.len_utf8();
        }
        if end > 0 {
            out.push_str("<redacted>");
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// `/p/<signed token>` identifies one file; keep the path, drop the token.
fn mask_signed_paths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find("/p/") {
        out.push_str(&rest[..index + 3]);
        let after = &rest[index + 3..];
        let mut end = 0;
        for (offset, c) in after.char_indices() {
            if !is_token_char(c) {
                end = offset;
                break;
            }
            end = offset + c.len_utf8();
        }
        if end >= 20 {
            out.push_str("<token>");
        } else {
            out.push_str(&after[..end]);
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_hook_writes_a_crash_log() {
        let dir = std::env::temp_dir().join(format!("dhakaflix-crash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("crash.log");
        install_panic_hook(file.clone());
        let result = std::panic::catch_unwind(|| panic!("boom for the test"));
        assert!(result.is_err());
        let text = std::fs::read_to_string(&file).expect("crash log written");
        assert!(text.contains("boom for the test"));
        assert!(text.contains("logging.rs"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn masks_secrets_and_tokens() {
        let secrets = vec!["0123456789abcdef0123456789abcdef".to_string()];
        let text = "PUBLIC_URL=https://x.ts.net/p/AbCdEfGhIjKlMnOpQrStUvWxYz0123456789.1700000000.sig";
        let masked = redact(text, &secrets);
        assert!(masked.contains("/p/<token>"));
        assert!(!masked.contains("AbCdEfGhIjKlMnOp"));
        assert!(redact("PROXY_SECRET=0123456789abcdef0123456789abcdef", &secrets)
            .contains("<redacted>"));
        let flag = redact("tailscale up --authkey=tskey-auth-kABC123-xyz", &[]);
        assert!(!flag.contains("kABC123"));
        assert!(flag.contains("--authkey=<redacted>"));
        assert!(redact("using tskey-auth-kABC123-xyz now", &[]).contains("<redacted>"));
    }
}
