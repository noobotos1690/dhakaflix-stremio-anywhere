use chrono::Local;
use once_cell::sync::OnceCell;
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use tracing_subscriber::fmt::writer::MakeWriter;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
        }
    }

    pub fn kind(self) -> i32 {
        match self {
            Level::Debug => 0,
            Level::Info => 1,
            Level::Warn => 2,
            Level::Error => 3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub seq: i64,
    pub time: String,
    pub level: Level,
    pub message: String,
}

pub struct LogStore {
    entries: Mutex<VecDeque<Entry>>,
    seq: AtomicI64,
    cap: usize,
}

impl LogStore {
    pub fn new(cap: usize) -> Self {
        Self { entries: Mutex::new(VecDeque::with_capacity(cap.min(512))), seq: AtomicI64::new(0), cap }
    }

    pub fn push(&self, level: Level, message: impl AsRef<str>) {
        let message = sanitize(message.as_ref());
        if message.trim().is_empty() {
            return;
        }
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        let entry = Entry {
            seq,
            time: Local::now().format("%H:%M:%S").to_string(),
            level,
            message,
        };
        let mut guard = self.entries.lock().unwrap();
        guard.push_back(entry);
        while guard.len() > self.cap {
            guard.pop_front();
        }
    }

    pub fn snapshot(&self) -> Vec<Entry> {
        self.entries.lock().unwrap().iter().cloned().collect()
    }

    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }

    pub fn last_seq(&self) -> i64 {
        self.seq.load(Ordering::Relaxed)
    }
}

/// Remove values that must never reach the GUI or the log file.
pub fn sanitize(input: &str) -> String {
    let mut text = strip_ansi(input);
    for key in ["PROXY_SECRET", "TUNNEL_TOKEN", "AUTH_KEY", "PASSWORD", "SECRET", "TOKEN"] {
        text = redact_key(&text, key);
    }
    text = redact_relay_paths(&text);
    text
}

fn redact_key(text: &str, key: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(key) {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + key.len()..];
        if let Some(eq) = after.find('=') {
            let between = &after[..eq];
            if between.chars().all(|c| c.is_whitespace()) {
                out.push_str(key);
                out.push('=');
                let value = &after[eq + 1..];
                let end = value
                    .find(|c: char| c.is_whitespace() || c == '&' || c == '"' || c == '\'' || c == ',')
                    .unwrap_or(value.len());
                out.push_str("<redacted>");
                rest = &value[end..];
                continue;
            }
        }
        out.push_str(key);
        rest = after;
    }
    out.push_str(rest);
    out
}

fn redact_relay_paths(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("/p/") {
        out.push_str(&rest[..pos]);
        out.push_str("/p/<redacted>");
        let after = &rest[pos + 3..];
        let end = after
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ')' || c == ']' || c == '&')
            .unwrap_or(after.len());
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

static STORE: OnceCell<Arc<LogStore>> = OnceCell::new();
static LOG_PATH: OnceCell<PathBuf> = OnceCell::new();

pub fn init(path: &Path) -> Arc<LogStore> {
    let store = Arc::new(LogStore::new(4000));
    let _ = STORE.set(store.clone());
    let _ = LOG_PATH.set(path.to_path_buf());
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    rotate(path);

    let file = OpenOptions::new().create(true).append(true).open(path).ok();
    if let Some(file) = file {
        let writer = SharedFile(Arc::new(Mutex::new(file)));
        let _ = tracing_subscriber::fmt()
            .with_writer(writer)
            .with_ansi(false)
            .with_target(false)
            .with_level(true)
            .try_init();
    }
    store
}

fn rotate(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        if meta.len() > 5 * 1024 * 1024 {
            let _ = fs::rename(path, path.with_extension("log.1"));
        }
    }
}

pub fn store() -> Option<Arc<LogStore>> {
    STORE.get().cloned()
}

pub fn log(level: Level, message: impl AsRef<str>) {
    let clean = sanitize(message.as_ref());
    if let Some(store) = STORE.get() {
        store.push(level, &clean);
    }
    match level {
        Level::Debug => tracing::debug!("{}", clean),
        Level::Info => tracing::info!("{}", clean),
        Level::Warn => tracing::warn!("{}", clean),
        Level::Error => tracing::error!("{}", clean),
    }
}

pub fn info(message: impl AsRef<str>) {
    log(Level::Info, message);
}
pub fn warn(message: impl AsRef<str>) {
    log(Level::Warn, message);
}
pub fn error(message: impl AsRef<str>) {
    log(Level::Error, message);
}
pub fn debug(message: impl AsRef<str>) {
    log(Level::Debug, message);
}

struct SharedFile(Arc<Mutex<File>>);

impl<'a> MakeWriter<'a> for SharedFile {
    type Writer = SharedFileWriter;
    fn make_writer(&'a self) -> Self::Writer {
        SharedFileWriter(self.0.clone())
    }
}

struct SharedFileWriter(Arc<Mutex<File>>);

impl Write for SharedFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut file = self.0.lock().map_err(|_| io::Error::other("log lock poisoned"))?;
        file.write_all(buf)?;
        file.flush()?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_and_signed_paths_are_redacted() {
        let input = "PROXY_SECRET=deadbeef TUNNEL_TOKEN=abc https://x/p/abcdef123?x=1 /p/zzz";
        let out = sanitize(input);
        assert!(!out.contains("deadbeef"));
        assert!(!out.contains("abcdef123"));
        assert!(!out.contains("/p/zzz"));
        assert!(out.contains("<redacted>"));
    }

    #[test]
    fn ansi_is_removed() {
        assert_eq!(sanitize("\u{1b}[31mhello\u{1b}[0m"), "hello");
    }
}
