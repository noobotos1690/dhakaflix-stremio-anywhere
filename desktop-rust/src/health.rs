use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::logging;
use crate::state::{AppState, Health, Mirror, Provider};

pub fn fetch(port: u16) -> Option<Health> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .ok()?;
    let response = client.get(format!("http://127.0.0.1:{port}/health")).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let value: Value = response.json().ok()?;
    Some(parse(&value))
}

pub fn fetch_public(host: &str) -> Option<Health> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;
    let response = client.get(format!("https://{host}/health")).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    Some(parse(&response.json().ok()?))
}

pub fn parse(value: &Value) -> Health {
    let mut health = Health {
        version: value.get("version").and_then(Value::as_str).unwrap_or("-").to_string(),
        ..Default::default()
    };
    if let Some(providers) = value.get("providers").and_then(Value::as_array) {
        for p in providers {
            health.providers.push(Provider {
                name: p
                    .get("name")
                    .or_else(|| p.get("id"))
                    .and_then(Value::as_str)
                    .unwrap_or("Mirror")
                    .to_string(),
                host: p.get("host").and_then(Value::as_str).unwrap_or("").to_string(),
                online: p
                    .get("online")
                    .or_else(|| p.get("ok"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            });
        }
    }
    health.active_streams = find_u32(value, &["relay.active", "proxy.active", "activeStreams", "relay.activeStreams", "streams.active"]).unwrap_or(0);
    health.max_streams = find_u32(value, &["relay.max", "maxStreams", "relay.maxStreams", "streams.max"]).unwrap_or(0);
    health.uptime_seconds = find_u32(value, &["uptime", "relay.uptime", "uptimeSeconds"]).unwrap_or(0) as u64;
    health
}

fn find_u32(value: &Value, paths: &[&str]) -> Option<u32> {
    for path in paths {
        let mut cursor = value;
        let mut found = true;
        for part in path.split('.') {
            match cursor.get(part) {
                Some(next) => cursor = next,
                None => {
                    found = false;
                    break;
                }
            }
        }
        if found {
            if let Some(n) = cursor.as_u64() {
                return Some(n as u32);
            }
            if let Some(n) = cursor.as_f64() {
                return Some(n as u32);
            }
        }
    }
    None
}

pub fn mirrors(health: &Health) -> Vec<Mirror> {
    health
        .providers
        .iter()
        .map(|p| Mirror { name: p.name.clone(), host: p.host.clone(), online: p.online })
        .collect()
}

/// Poll the local health endpoint while the relay is running.
pub fn spawn_monitor(shared: Arc<Mutex<AppState>>, port: u16) {
    std::thread::spawn(move || {
        let mut failures = 0;
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let running = {
                let guard = shared.lock().unwrap();
                guard.status == crate::state::RunStatus::Running
            };
            if !running {
                failures = 0;
                continue;
            }
            match fetch(port) {
                Some(health) => {
                    failures = 0;
                    if let Ok(mut guard) = shared.lock() {
                        guard.health = health.clone();
                        guard.mirrors = mirrors(&health);
                        guard.backend_version = health.version.clone();
                        if health.max_streams > 0 {
                            guard.health.max_streams = health.max_streams;
                        }
                    }
                }
                None => {
                    failures += 1;
                    if failures == 3 {
                        logging::warn("Local health checks are failing.");
                        if let Ok(mut guard) = shared.lock() {
                            if guard.status == crate::state::RunStatus::Running {
                                guard.detail = "Backend health is not responding.".into();
                            }
                        }
                    }
                }
            }
        }
    });
}
