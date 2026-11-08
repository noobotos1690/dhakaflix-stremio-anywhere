use std::io::{BufRead, BufReader};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::{Child, Command, Stdio};
use std::path::Path;
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::dependencies::Dependencies;
use crate::envfile;
use crate::health;
use crate::logging::{self, Level};
use crate::paths::Paths;
use crate::settings::Settings;
use crate::state::{AppState, RunStatus};
use crate::tailscale::Tailscale;

pub struct RelayManager {
    pub paths: Paths,
    pub settings: Arc<Mutex<Settings>>,
    pub shared: Arc<Mutex<AppState>>,
    pub tailscale: Arc<Tailscale>,
    pub dependencies: Arc<Dependencies>,
    child: Arc<Mutex<Option<Child>>>,
    pid: Mutex<Option<u32>>,
    stopping: Arc<AtomicBool>,
    external: AtomicBool,
}

impl RelayManager {
    pub fn new(
        paths: Paths,
        settings: Arc<Mutex<Settings>>,
        shared: Arc<Mutex<AppState>>,
        tailscale: Arc<Tailscale>,
        dependencies: Arc<Dependencies>,
    ) -> Arc<Self> {
        Arc::new(Self {
            paths,
            settings,
            shared,
            tailscale,
            dependencies,
            child: Arc::new(Mutex::new(None)),
            pid: Mutex::new(None),
            stopping: Arc::new(AtomicBool::new(false)),
            external: AtomicBool::new(false),
        })
    }

    pub fn start(self: &Arc<Self>) {
        let this = self.clone();
        std::thread::spawn(move || {
            if let Err(error) = this.start_inner() {
                logging::error(format!("Relay start failed: {error}"));
                this.kill_child();
                if let Ok(mut state) = this.shared.lock() {
                    state.set_status(RunStatus::Error, error.clone());
                    state.last_error = error;
                }
            }
            if let Ok(mut state) = this.shared.lock() {
                state.busy = None;
            }
        });
    }

    fn start_inner(&self) -> Result<(), String> {
        {
            let mut state = self.shared.lock().unwrap();
            if state.status == RunStatus::Starting || state.status == RunStatus::Running || state.status == RunStatus::Stopping {
                return Ok(());
            }
            state.set_status(RunStatus::Starting, "Starting relay...");
            state.busy = Some("Starting relay...".into());
            state.last_error.clear();
            state.clear_public();
        }
        self.stopping.store(false, Ordering::SeqCst);
        self.external.store(false, Ordering::SeqCst);

        let settings = self.settings.lock().unwrap().clone();
        if let Ok(mut state) = self.shared.lock() {
            state.max_streams = settings.max_streams;
        }
        if !self.paths.has_relay() {
            return Err(format!("Relay source is missing from {}.", self.paths.root.display()));
        }
        self.paths.ensure();
        envfile::ensure_env(&self.paths.env_file)?;
        envfile::apply_relay_settings(&self.paths.env_file, settings.port, settings.max_streams, &settings.log_level)?;

        // Never start a second listener. Reuse a running DhakaFlix relay if it answers.
        if port_open(settings.port) {
            let owner = owner_pid(settings.port);
            if let Some(health) = health::fetch(settings.port) {
                logging::info(format!("Existing relay detected on port {}.", settings.port));
                {
                    let mut state = self.shared.lock().unwrap();
                    state.external_relay = true;
                    state.started_at = Some(Instant::now());
                    state.health = health.clone();
                    state.mirrors = health::mirrors(&health);
                    state.backend_version = health.version.clone();
                    state.set_status(RunStatus::Running, "Existing relay process on this port.");
                }
                self.external.store(true, Ordering::SeqCst);
                self.discover_public(&settings)?;
                return Ok(());
            }
            return Err(format!(
                "Port {} is already in use{}. Close the other program or change the relay port.",
                settings.port,
                owner.map(|p| format!(" by PID {p}")).unwrap_or_default()
            ));
        }

        {
            let mut state = self.shared.lock().unwrap();
            state.busy = Some("Checking dependencies...".into());
        }
        self.dependencies.ensure(&settings.public_access_mode)?;
        let node = self.dependencies.node_path().ok_or("Node.js is not available after setup.")?;
        let node_version = Command::new(&node)
            .arg("--version")
            .creation_flags(0x08000000)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "Node.js".into());
        if let Ok(mut state) = self.shared.lock() {
            state.node_version = node_version;
        }

        let host = if settings.is_tailscale() {
            let status = self.tailscale.status()?;
            if !status.installed {
                return Err("Tailscale is not installed. Install it from the Public access page.".into());
            }
            if !status.connected {
                return Err("Tailscale is not connected. Connect it before starting Tailscale mode.".into());
            }
            let host = status.hostname.clone();
            envfile::apply_tailscale(&self.paths.env_file, &host)?;
            Some(host)
        } else {
            if settings.cloudflare_tunnel_token.is_empty() {
                envfile::apply_cloudflare(&self.paths.env_file, "")?;
            } else {
                envfile::update(
                    &self.paths.env_file,
                    &[
                        ("TUNNEL", "1"),
                        ("TUNNEL_TOKEN", &settings.cloudflare_tunnel_token),
                        ("PUBLIC_URL", &settings.cloudflare_public_url),
                    ],
                )?;
            }
            None
        };

        let (child, public_slot) = self.spawn_node(&settings, &node, host.as_deref())?;
        let pid = child.id();
        {
            let mut guard = self.child.lock().unwrap();
            *guard = Some(child);
            *self.pid.lock().unwrap() = Some(pid);
        }
        self.spawn_exit_watch();

        self.wait_local(settings.port, 35, &public_slot, host.as_deref())?;

        if let Some(host) = host {
            {
                let mut state = self.shared.lock().unwrap();
                state.busy = Some("Enabling Tailscale Funnel...".into());
            }
            let funnel = self.tailscale.funnel()?;
            if funnel.configured && !funnel.target.is_empty() && funnel.target != format!("http://127.0.0.1:{}", settings.port) {
                return Err(format!("Funnel already points at {}. Disable it first.", funnel.target));
            }
            if !funnel.enabled {
                self.tailscale.enable_funnel(settings.port)?;
            }
            {
                let mut state = self.shared.lock().unwrap();
                state.busy = Some("Checking public endpoint...".into());
            }
            self.tailscale.public_ready(&host)?;
            let url = format!("https://{host}");
            let mut state = self.shared.lock().unwrap();
            state.set_public(&host, &url);
            if let Ok(mut settings) = self.settings.lock() {
                settings.tailscale_hostname = host.clone();
                let _ = settings.save(&self.paths);
            }
        } else {
            let deadline = Instant::now() + Duration::from_secs(75);
            let mut url = public_slot.lock().unwrap().clone();
            while url.is_none() && Instant::now() < deadline {
                if self.child_exited() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(300));
                url = public_slot.lock().unwrap().clone();
            }
            let Some(url) = url else {
                // A named tunnel may already know its public address.
                if !settings.cloudflare_public_url.is_empty() {
                    let public = settings.cloudflare_public_url.trim_end_matches('/').to_string();
                    let host = public.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_string();
                    let mut state = self.shared.lock().unwrap();
                    state.set_public(&host, &public);
                } else {
                    return Err("cloudflared did not report a public URL. Check the logs.".into());
                }
                self.finish_running()?;
                return Ok(());
            };
            let host = url
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_end_matches('/')
                .to_string();
            let mut state = self.shared.lock().unwrap();
            state.set_public(&host, &url);
        }

        self.finish_running()?;
        Ok(())
    }

    fn finish_running(&self) -> Result<(), String> {
        let host = {
            let state = self.shared.lock().unwrap();
            state.public_host.clone()
        };
        if !host.is_empty() {
            // Give the tunnel a short window to become reachable. A failure here
            // is reported but does not kill a healthy local relay.
            if let Err(error) = self.tailscale.public_ready(&host) {
                logging::warn(format!("Public checks did not pass yet: {error}"));
            }
        }
        let mut state = self.shared.lock().unwrap();
        state.started_at = Some(Instant::now());
        state.external_relay = self.external.load(Ordering::SeqCst);
        let detail = if state.external_relay {
            "Existing relay process on this port."
        } else if state.mode == "tailscale" {
            "Relay and Funnel are online."
        } else {
            "Relay and Cloudflare tunnel are online."
        };
        state.set_status(RunStatus::Running, detail);
        state.busy = None;
        logging::info("Relay is running.");
        Ok(())
    }

    fn discover_public(&self, settings: &Settings) -> Result<(), String> {
        if settings.is_tailscale() {
            if let Ok(status) = self.tailscale.status() {
                if status.connected && !status.hostname.is_empty() {
                    if let Ok(funnel) = self.tailscale.funnel() {
                        if funnel.enabled {
                            let host = status.hostname;
                            let url = format!("https://{host}");
                            if let Ok(mut state) = self.shared.lock() {
                                state.set_public(&host, &url);
                            }
                        }
                    }
                }
            }
        } else if !settings.cloudflare_public_url.is_empty() {
            let public = settings.cloudflare_public_url.trim_end_matches('/').to_string();
            let host = public.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_string();
            if let Ok(mut state) = self.shared.lock() {
                state.set_public(&host, &public);
            }
        }
        Ok(())
    }

    fn spawn_node(&self, settings: &Settings, node: &Path, host: Option<&str>) -> Result<(Child, Arc<Mutex<Option<String>>>), String> {
        let mut cmd = Command::new(node);
        cmd.arg("index.js").current_dir(&self.paths.root);
        cmd.env("PORT", settings.port.to_string());
        cmd.env("MAX_PROXY_STREAMS", settings.max_streams.to_string());
        cmd.env("LOG_LEVEL", &settings.log_level);
        cmd.env_remove("PUBLIC_URL");
        cmd.env_remove("TUNNEL_TOKEN");
        if let Some(host) = host {
            cmd.env("PUBLIC_URL", format!("https://{host}"));
            cmd.env("TUNNEL", "0");
        } else if settings.cloudflare_tunnel_token.is_empty() {
            cmd.arg("--tunnel");
            cmd.env("TUNNEL", "0");
        } else {
            cmd.env("TUNNEL", "1");
            cmd.env("TUNNEL_TOKEN", &settings.cloudflare_tunnel_token);
            cmd.env("PUBLIC_URL", &settings.cloudflare_public_url);
        }
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(0x08000000);
        let mut child = cmd.spawn().map_err(|e| format!("Could not start Node.js: {e}"))?;
        let public = Arc::new(Mutex::new(None::<String>));
        if let Some(stdout) = child.stdout.take() {
            let public = public.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    capture_line(&line, &public);
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            let public = public.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    capture_line(&line, &public);
                }
            });
        }
        Ok((child, public))
    }

    fn wait_local(
        &self,
        port: u16,
        seconds: u64,
        public_slot: &Arc<Mutex<Option<String>>>,
        host: Option<&str>,
    ) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        loop {
            if let Some(health) = health::fetch(port) {
                let mut state = self.shared.lock().unwrap();
                state.health = health.clone();
                state.mirrors = health::mirrors(&health);
                state.backend_version = health.version;
                if health.max_streams > 0 {
                    state.health.max_streams = health.max_streams;
                }
                drop(state);
                logging::info("Local health check passed.");
                return Ok(());
            }
            if self.child_exited() {
                return Err("Node.js exited before the relay became healthy. Check the logs.".into());
            }
            if Instant::now() >= deadline {
                let hint = if public_slot.lock().unwrap().is_some() {
                    "A tunnel URL appeared, but the local health endpoint did not."
                } else if host.is_some() {
                    "Tailscale mode needs the local relay first. Check the log for a startup error."
                } else {
                    "Check the log. The port may be blocked or .env may be invalid."
                };
                return Err(format!("Relay did not become healthy in {seconds} seconds. {hint}"));
            }
            std::thread::sleep(Duration::from_millis(300));
        }
    }

    fn spawn_exit_watch(&self) {
        let shared = self.shared.clone();
        let stopping = self.stopping.clone();
        let child = self.child.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            if stopping.load(Ordering::SeqCst) {
                return;
            }
            let mut guard = match child.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let Some(process) = guard.as_mut() else {
                return;
            };
            match process.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code().unwrap_or(-1);
                    drop(guard);
                    logging::error(format!("Relay exited unexpectedly (code {code})."));
                    if let Ok(mut state) = shared.lock() {
                        if state.status == RunStatus::Running || state.status == RunStatus::Starting {
                            state.set_status(RunStatus::Error, format!("Relay exited unexpectedly (code {code})."));
                        }
                    }
                    return;
                }
                Ok(None) => {}
                Err(_) => return,
            }
        });
    }
    pub fn stop(self: &Arc<Self>) {
        let this = self.clone();
        std::thread::spawn(move || {
            this.stopping.store(true, Ordering::SeqCst);
            if let Ok(mut state) = this.shared.lock() {
                state.set_status(RunStatus::Stopping, "Stopping relay...");
                state.busy = Some("Stopping relay...".into());
            }
            if this.external.load(Ordering::SeqCst) {
                if let Ok(mut state) = this.shared.lock() {
                    state.set_status(RunStatus::Stopped, "The external relay was left running.");
                    state.busy = None;
                }
                this.external.store(false, Ordering::SeqCst);
                return;
            }
            this.kill_child();
            if let Ok(mut state) = this.shared.lock() {
                state.set_status(RunStatus::Stopped, "Relay is not running.");
                state.clear_public();
                state.health = Default::default();
                state.mirrors.clear();
                state.busy = None;
            }
            logging::info("Relay stopped.");
        });
    }

    pub fn restart(self: &Arc<Self>) {
        let this = self.clone();
        std::thread::spawn(move || {
            this.stopping.store(true, Ordering::SeqCst);
            this.kill_child();
            if let Ok(mut state) = this.shared.lock() {
                state.set_status(RunStatus::Stopped, "Restarting relay...");
                state.clear_public();
            }
            std::thread::sleep(Duration::from_millis(700));
            this.stopping.store(false, Ordering::SeqCst);
            this.start();
        });
    }

    fn kill_child(&self) {
        let pid = self.pid.lock().unwrap().take();
        if let Some(pid) = pid {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .creation_flags(0x08000000)
                .output();
        }
        if let Ok(mut guard) = self.child.lock() {
            *guard = None;
        }
    }

    fn child_exited(&self) -> bool {
        let mut guard = match self.child.lock() {
            Ok(g) => g,
            Err(_) => return true,
        };
        let Some(child) = guard.as_mut() else {
            return true;
        };
        matches!(child.try_wait(), Ok(Some(_)))
    }
}

fn capture_line(line: &str, public: &Arc<Mutex<Option<String>>>) {
    let clean = logging::sanitize(line);
    let lower = clean.to_ascii_lowercase();
    let level = if lower.contains("error") || lower.contains("fatal") || lower.contains("eaddrinuse") {
        Level::Error
    } else if lower.contains("warn") {
        Level::Warn
    } else {
        Level::Info
    };
    logging::log(level, &clean);
    if let Some(url) = parse_public_url(&clean) {
        let mut slot = public.lock().unwrap();
        if slot.is_none() {
            *slot = Some(url);
        }
    }
}

pub fn parse_public_url(line: &str) -> Option<String> {
    for token in line.split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '(' || c == ')' || c == ',') {
        let token = token.trim_end_matches(['.', ';', ':']);
        if token.starts_with("https://") && (token.contains(".trycloudflare.com") || token.contains(".ts.net")) {
            let base = token.split('/').take(3).collect::<Vec<_>>().join("/");
            return Some(base);
        }
    }
    None
}

fn port_open(port: u16) -> bool {
    if let Ok(addrs) = ("127.0.0.1", port).to_socket_addrs() {
        for addr in addrs {
            if TcpStream::connect_timeout(&addr, Duration::from_millis(350)).is_ok() {
                return true;
            }
        }
    }
    false
}

fn owner_pid(port: u16) -> Option<u32> {
    let output = Command::new("netstat").args(["-ano"]).creation_flags(0x08000000).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if !line.contains(&format!(":{port}")) || !line.contains("LISTENING") {
            continue;
        }
        if let Some(pid) = line.split_whitespace().last() {
            if let Ok(pid) = pid.parse::<u32>() {
                return Some(pid);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_trycloudflare_url() {
        let line = "INF TUNNEL Public URL: https://quiet-sky.trycloudflare.com";
        assert_eq!(parse_public_url(line).as_deref(), Some("https://quiet-sky.trycloudflare.com"));
    }

    #[test]
    fn ignores_signed_paths() {
        assert!(parse_public_url("GET /p/abcdef HTTP/1.1").is_none());
    }
}
