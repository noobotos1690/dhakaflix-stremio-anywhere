//! Tailscale CLI integration.
//!
//! Every call goes through the absolute `tailscale.exe` path, never a shell, and
//! only the DhakaFlix listener on port 443 is ever touched — the tailnet
//! connection itself is never reset or disconnected.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;

pub const DOWNLOAD_URL: &str = "https://tailscale.com/download/windows";
pub const ADMIN_URL: &str = "https://login.tailscale.com/admin/machines";
pub const FUNNEL_DOCS: &str = "https://tailscale.com/kb/1223/funnel";

// ------------------------------------------------------------------ discovery

pub fn find_executable() -> Option<PathBuf> {
    if std::env::var("DHAKAFLIX_TAILSCALE_DISABLED")
        .map(|value| value == "1")
        .unwrap_or(false)
    {
        return None;
    }

    if let Ok(override_path) = std::env::var("DHAKAFLIX_TAILSCALE_PATH") {
        let path = PathBuf::from(override_path);
        if path.is_file() {
            return Some(path);
        }
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(base) = std::env::var(variable) {
            candidates.push(PathBuf::from(base).join(r"Tailscale\tailscale.exe"));
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local).join(r"Tailscale\tailscale.exe"));
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(';') {
            if dir.trim().is_empty() {
                continue;
            }
            candidates.push(Path::new(dir.trim()).join("tailscale.exe"));
        }
    }

    candidates.into_iter().find(|path| path.is_file())
}

pub fn service_running() -> Option<bool> {
    let mut command = std::process::Command::new("sc");
    command.args(["query", "Tailscale"]);
    crate::winutil::hide_console(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return Some(false);
    }
    let text = String::from_utf8_lossy(&output.stdout).to_uppercase();
    Some(text.contains("RUNNING"))
}

// --------------------------------------------------------------------- status

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub installed: bool,
    pub executable: Option<PathBuf>,
    pub service_running: bool,
    pub backend_state: String,
    pub connected: bool,
    pub needs_login: bool,
    pub auth_url: Option<String>,
    pub hostname: Option<String>,
    #[allow(dead_code)] // the tailnet domain, shown by future consumers of this status
    pub magic_dns_suffix: String,
    pub magic_dns_enabled: bool,
    pub cert_available: bool,
    pub version: String,
    pub error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawStatus {
    #[serde(rename = "BackendState")]
    backend_state: String,
    #[serde(rename = "AuthURL")]
    auth_url: String,
    #[serde(rename = "MagicDNSSuffix")]
    magic_dns_suffix: String,
    #[serde(rename = "Version")]
    version: String,
    #[serde(rename = "Self")]
    itself: RawSelf,
    #[serde(rename = "CurrentTailnet")]
    tailnet: RawTailnet,
    #[serde(rename = "CertDomains")]
    cert_domains: Vec<String>,
    #[serde(rename = "Health")]
    health: Vec<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawSelf {
    #[serde(rename = "DNSName")]
    dns_name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawTailnet {
    #[serde(rename = "MagicDNSEnabled")]
    magic_dns_enabled: bool,
}

pub fn parse_status(json: &str) -> Result<Status, String> {
    let raw: RawStatus = serde_json::from_str(json).map_err(|error| error.to_string())?;

    let mut status = Status {
        installed: true,
        backend_state: raw.backend_state.clone(),
        magic_dns_suffix: raw.magic_dns_suffix.clone(),
        // `tailscale status --json` reports a long build string; only the
        // semver part belongs in the diagnostics row.
        version: raw
            .version
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string(),
        ..Default::default()
    };

    status.needs_login = raw.backend_state.eq_ignore_ascii_case("NeedsLogin")
        || !raw.auth_url.trim().is_empty();
    if !raw.auth_url.trim().is_empty() {
        status.auth_url = Some(raw.auth_url.clone());
    }

    let cert_host = raw
        .cert_domains
        .iter()
        .find(|domain| !domain.trim().is_empty())
        .cloned();
    status.cert_available = cert_host.is_some();

    let dns_name = raw.itself.dns_name.trim().trim_end_matches('.');
    status.hostname = cert_host
        .map(|host| host.trim().trim_end_matches('.').to_string())
        .or_else(|| {
            if dns_name.is_empty() {
                None
            } else {
                Some(dns_name.to_string())
            }
        });

    status.magic_dns_enabled =
        raw.tailnet.magic_dns_enabled || !raw.magic_dns_suffix.trim().is_empty();

    status.connected = raw.backend_state.eq_ignore_ascii_case("Running")
        && !dns_name.is_empty()
        && !status.needs_login;

    let warnings: Vec<String> = raw
        .health
        .iter()
        .map(|entry| match entry {
            serde_json::Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .filter(|text| !text.trim().is_empty())
        .collect();
    if !warnings.is_empty() {
        status.error = Some(warnings.join(" "));
    }

    Ok(status)
}

#[derive(Debug, Clone, Default)]
pub struct Funnel {
    pub configured: bool,
    pub enabled: bool,
    pub hostname: Option<String>,
    pub target: Option<String>,
    pub other_listeners: bool,
    pub error: Option<String>,
}

impl Funnel {
    pub fn matches(&self, port: u16) -> bool {
        match self.target.as_deref() {
            Some(target) => target
                .trim_end_matches('/')
                .eq_ignore_ascii_case(&format!("http://127.0.0.1:{port}")),
            None => false,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawFunnel {
    #[serde(rename = "Web")]
    web: BTreeMap<String, RawWebEntry>,
    #[serde(rename = "AllowFunnel")]
    allow: BTreeMap<String, bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawWebEntry {
    #[serde(rename = "Handlers")]
    handlers: BTreeMap<String, RawHandler>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RawHandler {
    #[serde(rename = "Proxy")]
    proxy: String,
}

pub fn parse_funnel(json: &str) -> Result<Funnel, String> {
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return Ok(Funnel::default());
    }
    let raw: RawFunnel = serde_json::from_str(trimmed).map_err(|error| error.to_string())?;
    let mut funnel = Funnel::default();

    let Some((key, entry)) = raw.web.iter().next() else {
        return Ok(funnel);
    };

    let host = key.rsplit_once(':').map(|(host, _)| host).unwrap_or(key);
    let mut other_listeners = raw.web.len() > 1;
    for handler_key in entry.handlers.keys() {
        if handler_key != "/" {
            other_listeners = true;
        }
    }
    let target = entry
        .handlers
        .values()
        .map(|handler| handler.proxy.trim())
        .find(|proxy| !proxy.is_empty())
        .map(|proxy| proxy.to_string());

    funnel.configured = true;
    funnel.hostname = Some(host.to_string());
    funnel.target = target;
    funnel.other_listeners = other_listeners;
    funnel.enabled = raw.allow.get(key).copied().unwrap_or(false);
    Ok(funnel)
}

// --------------------------------------------------------------------- calls

async fn run(exe: &Path, args: &[&str], timeout_ms: u64) -> Result<(i32, String, String), String> {
    let mut command = tokio::process::Command::new(exe);
    command.args(args);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    crate::winutil::hide_console_async(&mut command);

    let child = command.spawn().map_err(|error| {
        format!(
            "tailscale could not be started ({}): {error}",
            exe.display()
        )
    })?;

    match tokio::time::timeout(Duration::from_millis(timeout_ms), child.wait_with_output()).await {
        Ok(Ok(output)) => Ok((
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).to_string(),
        )),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("tailscale did not answer in time".to_string()),
    }
}

pub async fn status() -> Status {
    let Some(exe) = find_executable() else {
        return Status {
            error: Some("Tailscale is not installed.".to_string()),
            ..Default::default()
        };
    };

    let mut status = Status {
        installed: true,
        executable: Some(exe.clone()),
        service_running: service_running().unwrap_or(true),
        ..Default::default()
    };

    match run(&exe, &["status", "--json"], 12_000).await {
        Ok((_, stdout, stderr)) => {
            if stdout.trim().is_empty() {
                status.error = Some(first_line(&stderr).unwrap_or_else(|| {
                    if status.service_running {
                        "Tailscale did not return a status.".to_string()
                    } else {
                        "The Tailscale service is not running.".to_string()
                    }
                }));
                return status;
            }
            match parse_status(&stdout) {
                Ok(parsed) => {
                    status = parsed;
                    status.executable = Some(exe);
                    status.service_running = service_running().unwrap_or(true);
                    if !status.service_running && !status.connected {
                        status.error =
                            Some("The Tailscale service is not running.".to_string());
                    }
                }
                Err(error) => status.error = Some(format!("Could not read Tailscale status: {error}")),
            }
        }
        Err(error) => status.error = Some(error),
    }

    status
}

pub async fn funnel_status() -> Funnel {
    let Some(exe) = find_executable() else {
        return Funnel {
            error: Some("Tailscale is not installed.".to_string()),
            ..Default::default()
        };
    };

    match run(&exe, &["funnel", "status", "--json"], 12_000).await {
        Ok((code, stdout, stderr)) => {
            let text = stdout.trim();
            let lower = text.to_lowercase();
            if lower.contains("no serve config") {
                return Funnel::default();
            }
            if text.is_empty() || text == "{}" {
                if code != 0 && !stderr.to_lowercase().contains("no serve config") {
                    return Funnel {
                        error: first_line(&stderr),
                        ..Default::default()
                    };
                }
                return Funnel::default();
            }
            match parse_funnel(text) {
                Ok(funnel) => funnel,
                Err(error) => Funnel {
                    error: Some(format!("Could not read Funnel status: {error}")),
                    ..Default::default()
                },
            }
        }
        Err(error) => Funnel {
            error: Some(error),
            ..Default::default()
        },
    }
}

pub async fn enable_funnel(port: u16, replace_existing: bool) -> Result<Funnel, String> {
    let Some(exe) = find_executable() else {
        return Err("Tailscale is not installed.".to_string());
    };

    let current = funnel_status().await;
    if let Some(error) = current.error.as_ref() {
        tracing::debug!("Funnel status before enabling: {error}");
    }
    if current.configured && current.enabled && current.matches(port) {
        return Ok(current);
    }
    if current.configured
        && current.target.as_deref().map(|t| !t.is_empty()).unwrap_or(false)
        && !current.matches(port)
        && !replace_existing
    {
        return Err(format!(
            "Port 443 is already used by another Funnel service ({}). DhakaFlix will not replace it automatically.",
            current.target.unwrap_or_default()
        ));
    }

    let target = format!("http://127.0.0.1:{port}");
    tracing::info!("Configuring Tailscale Funnel for {target}");
    let (code, stdout, stderr) = run(
        &exe,
        &["funnel", "--bg", "--yes", "--https=443", &target],
        90_000,
    )
    .await?;

    if code != 0 {
        return Err(map_funnel_error(&stderr, &stdout));
    }

    for _ in 0..20 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let status = funnel_status().await;
        if status.configured && status.enabled && status.matches(port) {
            if let Some(host) = status.hostname.as_deref() {
                tracing::info!("Funnel is active at https://{host}");
            }
            return Ok(status);
        }
    }

    Err("Tailscale accepted the command but Funnel did not become active. Check the admin console.".to_string())
}

pub async fn disable_funnel(port: u16) -> Result<(), String> {
    let Some(exe) = find_executable() else {
        return Err("Tailscale is not installed.".to_string());
    };

    let current = funnel_status().await;
    if !current.configured {
        return Ok(());
    }
    if let Some(target) = current.target.as_deref() {
        if !current.matches(port) {
            return Err(format!(
                "Funnel is currently serving {target}. DhakaFlix will not disable another service."
            ));
        }
    }
    if current.other_listeners {
        return Err(
            "Port 443 has additional Funnel handlers. DhakaFlix will not disable a shared listener."
                .to_string(),
        );
    }

    tracing::info!("Disabling the Funnel listener for DhakaFlix");
    let (code, _stdout, stderr) = run(&exe, &["funnel", "--https=443", "off"], 30_000).await?;
    if code != 0 {
        return Err(first_line(&stderr).unwrap_or_else(|| "Tailscale could not disable Funnel.".to_string()));
    }

    for _ in 0..10 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if !funnel_status().await.configured {
            return Ok(());
        }
    }

    Ok(())
}

/// Run `tailscale up` and wait for the browser sign-in to finish.
pub async fn connect(timeout_secs: u64) -> Result<Status, String> {
    let Some(exe) = find_executable() else {
        return Err("Tailscale is not installed.".to_string());
    };

    let mut command = tokio::process::Command::new(&exe);
    command.arg("up");
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    crate::winutil::hide_console_async(&mut command);

    let mut child = command
        .spawn()
        .map_err(|error| format!("Tailscale could not be started: {error}"))?;

    let (sender, mut receiver) = tokio::sync::mpsc::channel::<String>(64);
    if let Some(stdout) = child.stdout.take() {
        let sender = sender.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut lines = tokio::io::BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = sender.send(line).await;
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let sender = sender.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut lines = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = sender.send(line).await;
            }
        });
    }
    drop(sender);

    let mut opened = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let mut last_auth_url: Option<String> = None;

    loop {
        if std::time::Instant::now() >= deadline {
            let _ = child.start_kill();
            return Err(match last_auth_url {
                Some(url) => format!(
                    "Timed out waiting for Tailscale sign-in. Finish it at {url}, then click Connect again."
                ),
                None => "Timed out waiting for Tailscale sign-in.".to_string(),
            });
        }

        if let Ok(Some(line)) = receiver.try_recv().map(Some) {
            let line = line.trim().to_string();
            if !line.is_empty() {
                tracing::debug!("tailscale up: {line}");
            }
            if let Some(url) = first_url(&line) {
                if !opened {
                    opened = true;
                    crate::winutil::open_url(&url);
                    tracing::info!("Tailscale opened the sign-in page in your browser");
                }
                last_auth_url = Some(url);
            }
        }

        let status = self::status().await;
        if status.connected {
            let _ = child.start_kill();
            return Ok(status);
        }
        if !opened {
            if let Some(url) = status.auth_url.clone() {
                opened = true;
                crate::winutil::open_url(&url);
                last_auth_url = Some(url);
            }
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub fn map_funnel_error(stderr: &str, stdout: &str) -> String {
    let text = format!("{}\n{stdout}", stderr).to_lowercase();
    let first = first_line(stderr)
        .or_else(|| first_line(stdout))
        .unwrap_or_else(|| "Tailscale Funnel command failed.".to_string());

    if text.contains("https") && (text.contains("cert") || text.contains("not enabled") || text.contains("disabled"))
    {
        return "HTTPS certificates are not enabled for your tailnet. Enable HTTPS in the Tailscale admin console."
            .to_string();
    }
    if text.contains("funnel") && (text.contains("not enabled") || text.contains("policy") || text.contains("tailnet"))
    {
        return "Funnel is not enabled for your tailnet. Enable it in the Tailscale admin console, then retry."
            .to_string();
    }
    if text.contains("already") || text.contains("in use") || text.contains("listener") {
        return first;
    }
    if text.contains("login") || text.contains("logged out") || text.contains("auth") {
        return "Tailscale needs you to sign in again.".to_string();
    }
    first
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.to_string())
}

fn first_url(text: &str) -> Option<String> {
    let start = text.find("https://")?;
    let rest = &text[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_connected_status() {
        let status = parse_status(include_str!("../../desktop/tests/fixtures/status-running.json"))
            .expect("fixture parses");
        assert!(status.connected);
        assert!(!status.needs_login);
        assert!(status.cert_available);
        assert_eq!(status.hostname.as_deref(), Some("demo-host.tailnet-test.ts.net"));
        assert!(status.magic_dns_enabled);
        assert_eq!(status.version, "1.102.4-test");
    }

    #[test]
    fn detects_sign_in_requirement() {
        let status =
            parse_status(include_str!("../../desktop/tests/fixtures/status-needs-login.json"))
                .expect("fixture parses");
        assert!(status.needs_login);
        assert!(!status.connected);
        assert_eq!(
            status.auth_url.as_deref(),
            Some("https://login.tailscale.com/a/example")
        );
        assert!(!status.cert_available);
    }

    #[test]
    fn parses_active_funnel() {
        let funnel =
            parse_funnel(include_str!("../../desktop/tests/fixtures/funnel-active.json")).unwrap();
        assert!(funnel.configured);
        assert!(funnel.enabled);
        assert!(funnel.matches(7001));
        assert_eq!(
            funnel.hostname.as_deref(),
            Some("demo-host.tailnet-test.ts.net")
        );
        assert!(!funnel.other_listeners);
    }

    #[test]
    fn notices_conflicting_listener() {
        let funnel =
            parse_funnel(include_str!("../../desktop/tests/fixtures/funnel-conflict.json")).unwrap();
        assert!(funnel.configured);
        assert!(!funnel.matches(7001));
        assert_eq!(funnel.target.as_deref(), Some("http://127.0.0.1:8080"));
    }

    #[test]
    fn treats_empty_config_as_off() {
        let funnel =
            parse_funnel(include_str!("../../desktop/tests/fixtures/funnel-off.json")).unwrap();
        assert!(!funnel.configured);
        assert!(funnel.target.is_none());
    }

    #[test]
    fn classifies_cli_failures() {
        assert!(map_funnel_error("HTTPS is not enabled for your tailnet", "")
            .contains("HTTPS certificates"));
        assert!(map_funnel_error("funnel is not enabled for this tailnet", "")
            .contains("not enabled for your tailnet"));
        assert!(map_funnel_error("listener already exists", "").contains("listener"));
    }
}
