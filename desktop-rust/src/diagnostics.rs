use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::dependencies::Dependencies;
use crate::envfile;
use crate::health;
use crate::paths::Paths;
use crate::state::{AppState, Diag};
use crate::tailscale::Tailscale;

pub fn collect(paths: &Paths, deps: &Dependencies, tailscale: &Tailscale, state: &AppState, port: u16) -> Vec<Diag> {
    let snapshot = deps.inspect();
    let mut rows = Vec::new();

    rows.push(if let Some(node) = &snapshot.node {
        Diag::new("Node.js runtime", format!("{} ({})", snapshot.node_version, node.display()), "Ready", 1)
    } else {
        Diag::new("Node.js runtime", "Not found. Setup will download a portable runtime.", "Missing", 3)
    });

    rows.push(if snapshot.modules {
        Diag::new("npm dependencies", "node_modules is present.", "Ready", 1)
    } else {
        Diag::new("npm dependencies", "Install with Repair dependencies.", "Missing", 2)
    });

    let secret = envfile::read_value(&paths.env_file, "PROXY_SECRET").unwrap_or_default();
    rows.push(if paths.env_file.exists() && !secret.is_empty() {
        Diag::new(".env and PROXY_SECRET", "Configuration is present. Secret value is not displayed.", "Ready", 1)
    } else {
        Diag::new(".env and PROXY_SECRET", "Run setup to create a local configuration.", "Missing", 3)
    });

    if state.mode == "cloudflare" {
        rows.push(if snapshot.cloudflared {
            Diag::new("cloudflared", paths.cloudflared.display().to_string(), "Ready", 1)
        } else {
            Diag::new("cloudflared", "Not downloaded yet. It is installed on first start.", "Missing", 2)
        });
    } else {
        rows.push(Diag::new("cloudflared", "Not used in Tailscale mode.", "Skipped", 0));
    }

    let ts = tailscale.status().unwrap_or_default();
    rows.push(if ts.installed {
        Diag::new("Tailscale executable", tailscale.exe().map(|p| p.display().to_string()).unwrap_or_default(), "Ready", 1)
    } else {
        Diag::new("Tailscale executable", "Install from the Public access page.", "Missing", 2)
    });
    rows.push(if ts.service_running {
        Diag::new("Tailscale service", "The Windows service is running.", "Running", 1)
    } else {
        Diag::new("Tailscale service", "Service is stopped or not installed.", "Offline", 2)
    });
    rows.push(if ts.connected {
        Diag::new("Tailscale connection", format!("{} ({})", ts.device_name, ts.backend_state), "Connected", 1)
    } else {
        Diag::new("Tailscale connection", "Not connected. Browser sign-in may be required.", "Offline", 2)
    });

    let funnel = tailscale.funnel().unwrap_or_default();
    rows.push(if funnel.configured {
        Diag::new("Funnel listener", format!("{} -> {}", funnel.host, funnel.target), "Active", 1)
    } else {
        Diag::new("Funnel listener", "No Funnel listener is configured.", "Off", 0)
    });

    match health::fetch(port) {
        Some(_) => rows.push(Diag::new("Local relay health", "127.0.0.1 health endpoint answered.", "Healthy", 1)),
        None => rows.push(Diag::new("Local relay health", "No answer on the configured port.", "Offline", if state.is_running() { 3 } else { 0 })),
    }

    if !state.public_host.is_empty() {
        let public = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(8))
            .build()
            .ok()
            .and_then(|client| client.get(format!("https://{}/health", state.public_host)).send().ok())
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        rows.push(if public {
            Diag::new("Public relay health", format!("https://{}/health", state.public_host), "Healthy", 1)
        } else {
            Diag::new("Public relay health", "The public health check did not pass.", "Failed", 3)
        });
    } else {
        rows.push(Diag::new("Public relay health", "No public endpoint is active.", "Offline", 0));
    }

    let online = state.health.providers.iter().filter(|p| p.online).count();
    rows.push(if state.health.providers.is_empty() {
        Diag::new("BDIX mirrors", "No mirror data yet.", "Unknown", 0)
    } else {
        Diag::new("BDIX mirrors", format!("{online} of {} reachable.", state.health.providers.len()), if online > 0 { "Reachable" } else { "Unreachable" }, if online > 0 { 1 } else { 3 })
    });

    rows
}

pub fn refresh(shared: Arc<Mutex<AppState>>, paths: Paths, deps: Arc<Dependencies>, tailscale: Arc<Tailscale>, port: u16) {
    std::thread::spawn(move || {
        let snapshot = {
            let state = shared.lock().unwrap();
            collect(&paths, &deps, &tailscale, &state, port)
        };
        if let Ok(mut state) = shared.lock() {
            state.diagnostics = snapshot;
        }
        crate::logging::info("Diagnostics refreshed.");
    });
}
