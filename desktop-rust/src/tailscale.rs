use serde::Deserialize;
use std::collections::HashMap;
use std::os::windows::process::CommandExt;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::logging;

#[derive(Clone, Debug, Default)]
pub struct TsStatus {
    pub installed: bool,
    pub service_running: bool,
    pub connected: bool,
    pub backend_state: String,
    pub auth_url: String,
    pub hostname: String,
    pub device_name: String,
    pub magic_dns: bool,
    pub cert_domains: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct FunnelInfo {
    pub configured: bool,
    pub enabled: bool,
    pub host: String,
    pub target: String,
    pub other_handlers: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct RawStatus {
    backend_state: String,
    #[serde(rename = "AuthURL")]
    auth_url: String,
    #[serde(rename = "MagicDNSSuffix")]
    magic_dns_suffix: String,
    cert_domains: Vec<String>,
    #[serde(rename = "Self")]
    self_node: RawSelf,
    #[serde(rename = "CurrentTailnet")]
    current_tailnet: RawTailnet,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct RawSelf {
    #[serde(rename = "DNSName")]
    dns_name: String,
    host_name: String,
    online: bool,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct RawTailnet {
    magic_dns_enabled: bool,
    name: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct RawFunnel {
    web: Option<HashMap<String, RawWebEntry>>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct RawWebEntry {
    handlers: HashMap<String, RawHandler>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct RawHandler {
    proxy: String,
}

pub struct Tailscale {
    exe: Option<PathBuf>,
}

impl Tailscale {
    pub fn discover() -> Self {
        let exe = std::env::var("DHAKAFLIX_TAILSCALE_PATH")
            .ok()
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .or_else(find_exe);
        Self { exe }
    }

    pub fn exe(&self) -> Option<&PathBuf> {
        self.exe.as_ref()
    }

    pub fn installed(&self) -> bool {
        self.exe.is_some()
    }

    pub fn service_running(&self) -> bool {
        let output = Command::new("sc")
            .args(["query", "Tailscale"])
            .creation_flags(0x08000000)
            .output();
        match output {
            Ok(out) => String::from_utf8_lossy(&out.stdout).contains("RUNNING"),
            Err(_) => false,
        }
    }

    pub fn status(&self) -> Result<TsStatus, String> {
        let Some(exe) = &self.exe else {
            return Ok(TsStatus::default());
        };
        let out = hidden(exe).args(["status", "--json"]).output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(cli_error(&out.stderr));
        }
        let raw: RawStatus = serde_json::from_slice(&out.stdout).map_err(|e| format!("Tailscale status returned invalid JSON: {e}"))?;
        Ok(parse_status(raw, true, self.service_running()))
    }

    pub fn funnel(&self) -> Result<FunnelInfo, String> {
        let Some(exe) = &self.exe else {
            return Ok(FunnelInfo::default());
        };
        let out = hidden(exe).args(["funnel", "status", "--json"]).output().map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(cli_error(&out.stderr));
        }
        let raw: RawFunnel = serde_json::from_slice(&out.stdout).map_err(|e| format!("Tailscale funnel status returned invalid JSON: {e}"))?;
        Ok(parse_funnel(raw))
    }

    pub fn enable_funnel(&self, port: u16) -> Result<FunnelInfo, String> {
        let exe = self.exe.as_ref().ok_or("Tailscale is not installed.")?;
        let target = format!("http://127.0.0.1:{port}");
        let out = hidden(exe)
            .args(["funnel", "--bg", "--yes", "--https=443", &target])
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(map_funnel_error(&cli_error(&out.stderr)));
        }
        let info = self.funnel()?;
        if !info.configured {
            return Err("Tailscale accepted the command but no Funnel listener is configured.".into());
        }
        if !info.target.is_empty() && info.target != target {
            return Err(format!("Port 443 is pointing at {}, not {target}.", info.target));
        }
        logging::info(format!("Funnel enabled for {target}"));
        Ok(info)
    }

    pub fn disable_funnel(&self, port: u16) -> Result<FunnelInfo, String> {
        let exe = self.exe.as_ref().ok_or("Tailscale is not installed.")?;
        let target = format!("http://127.0.0.1:{port}");
        let info = self.funnel()?;
        if !info.configured {
            return Ok(info);
        }
        if !info.target.is_empty() && info.target != target {
            return Err(format!("Refusing to disable Funnel on {}. It is not the DhakaFlix listener.", info.target));
        }
        if info.other_handlers {
            return Err("Port 443 also serves other paths. Disable those in Tailscale first.".into());
        }
        let out = hidden(exe)
            .args(["funnel", "--https=443", "off"])
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(map_funnel_error(&cli_error(&out.stderr)));
        }
        logging::info("Funnel disabled for DhakaFlix");
        self.funnel()
    }

    /// Browser authentication only. No auth key is read, stored or logged.
    pub fn connect(&self, progress: impl Fn(String) + Send + 'static) -> Result<TsStatus, String> {
        let exe = self.exe.as_ref().ok_or("Tailscale is not installed.")?;
        let mut child = hidden(exe)
            .arg("up")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let (tx, rx) = mpsc::channel::<String>();
        if let Some(stdout) = stdout {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    let _ = tx.send(line);
                }
            });
        }
        if let Some(stderr) = stderr {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let _ = tx.send(line);
                }
            });
        }
        drop(tx);

        let started = Instant::now();
        let mut opened = false;
        let mut last_error = String::new();
        loop {
            while let Ok(line) = rx.try_recv() {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                if line.starts_with("https://login.tailscale.com/") {
                    if !opened {
                        opened = true;
                        let _ = webbrowser::open(&line);
                        progress("Browser opened for Tailscale sign-in.".into());
                    }
                    continue;
                }
                if line.to_ascii_lowercase().contains("error") {
                    last_error = line.clone();
                }
                progress(line);
            }
            if let Ok(Some(_)) = child.try_wait() {
                break;
            }
            if started.elapsed() > Duration::from_secs(120) {
                let _ = child.kill();
                return Err(if last_error.is_empty() {
                    "Tailscale sign-in timed out after 120 seconds.".into()
                } else {
                    last_error
                });
            }
            std::thread::sleep(Duration::from_millis(350));
        }

        let status = self.status()?;
        if status.connected {
            Ok(status)
        } else if !status.auth_url.is_empty() {
            let _ = webbrowser::open(&status.auth_url);
            Err("Sign-in is still pending. Complete it in the browser, then refresh.".into())
        } else {
            Err(if last_error.is_empty() { "Tailscale did not connect.".into() } else { last_error })
        }
    }

    pub fn public_ready(&self, host: &str) -> Result<(), String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(12))
            .build()
            .map_err(|e| e.to_string())?;
        for path in ["/health", "/manifest.json"] {
            let url = format!("https://{host}{path}");
            let response = client.get(&url).send().map_err(|e| format!("{url}: {e}"))?;
            if !response.status().is_success() {
                return Err(format!("{url} returned HTTP {}.", response.status()));
            }
        }
        Ok(())
    }

    pub fn download_url() -> &'static str {
        "https://tailscale.com/download/windows"
    }
}

fn find_exe() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(pf) = std::env::var("ProgramFiles") {
        candidates.push(PathBuf::from(pf).join("Tailscale").join("tailscale.exe"));
    }
    if let Ok(pf) = std::env::var("ProgramFiles(x86)") {
        candidates.push(PathBuf::from(pf).join("Tailscale").join("tailscale.exe"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local).join("Tailscale").join("tailscale.exe"));
    }
    candidates.into_iter().find(|p| p.exists()).or_else(|| {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|p| p.join("tailscale.exe"))
                .find(|p| p.exists())
        })
    })
}

fn hidden(exe: &PathBuf) -> Command {
    let mut cmd = Command::new(exe);
    cmd.creation_flags(0x08000000);
    cmd
}

fn cli_error(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr).trim().to_string();
    if text.is_empty() {
        "Tailscale command failed.".into()
    } else {
        text
    }
}

fn map_funnel_error(message: &str) -> String {
    let lower = message.to_ascii_lowercase();
    if lower.contains("https cert") || lower.contains("certificate") || lower.contains("tls") {
        "HTTPS certificates are not enabled for this tailnet. Enable HTTPS in the Tailscale admin console, then retry.".into()
    } else if lower.contains("funnel") && (lower.contains("not allowed") || lower.contains("policy") || lower.contains("acl")) {
        "Funnel is not allowed by the tailnet policy. Enable Funnel for this device in the admin console.".into()
    } else if lower.contains("already") || lower.contains("listener") || lower.contains("in use") {
        "Port 443 already has a Funnel listener. Disable it in Tailscale or use the existing address.".into()
    } else if lower.contains("logged out") || lower.contains("login") {
        "Tailscale is not signed in. Connect it first.".into()
    } else {
        format!("Tailscale Funnel failed: {message}")
    }
}

fn parse_status(raw: RawStatus, installed: bool, service_running: bool) -> TsStatus {
    let dns = raw.self_node.dns_name.trim_end_matches('.').to_string();
    let cert = raw.cert_domains.iter().map(|h| h.trim_end_matches('.').to_string()).collect::<Vec<_>>();
    let hostname = cert.first().cloned().unwrap_or_else(|| dns.clone());
    TsStatus {
        installed,
        service_running,
        connected: raw.backend_state.eq_ignore_ascii_case("Running") && !hostname.is_empty(),
        backend_state: raw.backend_state,
        auth_url: raw.auth_url,
        hostname,
        device_name: raw.self_node.host_name,
        magic_dns: raw.current_tailnet.magic_dns_enabled || !raw.magic_dns_suffix.is_empty(),
        cert_domains: cert,
    }
}

fn parse_funnel(raw: RawFunnel) -> FunnelInfo {
    let mut result = FunnelInfo::default();
    let mut all_handlers = 0usize;
    for (host_key, entry) in raw.web.unwrap_or_default() {
        let host = host_key.rsplit_once(':').map(|(h, _)| h).unwrap_or(&host_key).trim_end_matches('.').to_string();
        for (path, handler) in entry.handlers {
            all_handlers += 1;
            if result.host.is_empty() {
                result.host = host.clone();
                result.target = handler.proxy.clone();
            }
            if path != "/" {
                result.other_handlers = true;
            }
        }
    }
    result.configured = all_handlers > 0;
    result.enabled = result.configured;
    if all_handlers > 2 {
        result.other_handlers = true;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_running_status_and_cert_domain() {
        let raw: RawStatus = serde_json::from_str(include_str!("../tests/fixtures/status-running.json")).unwrap();
        let s = parse_status(raw, true, true);
        assert!(s.connected);
        assert_eq!(s.hostname, "gajar-nouka.tail145eb3.ts.net");
        assert_eq!(s.device_name, "gajar-nouka");
    }

    #[test]
    fn needs_login_is_not_connected() {
        let raw: RawStatus = serde_json::from_str(include_str!("../tests/fixtures/status-needs-login.json")).unwrap();
        let s = parse_status(raw, true, true);
        assert!(!s.connected);
        assert!(s.auth_url.starts_with("https://login.tailscale.com/"));
    }

    #[test]
    fn parses_funnel_target() {
        let raw: RawFunnel = serde_json::from_str(include_str!("../tests/fixtures/funnel-active.json")).unwrap();
        let f = parse_funnel(raw);
        assert!(f.configured);
        assert_eq!(f.target, "http://127.0.0.1:7001");
        assert_eq!(f.host, "gajar-nouka.tail145eb3.ts.net");
    }
}
