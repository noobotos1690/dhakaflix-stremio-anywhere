//! Relay health reading. `/health` is the single source of truth for uptime,
//! mirror reachability and active streaming sessions, so nothing here guesses.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RelayStats {
    pub active: u32,
    pub max: u32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProviderHealth {
    pub id: String,
    pub name: String,
    pub host: String,
    pub online: bool,
    pub known: bool,
    pub blocked: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HealthPayload {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub stream_mode: String,
    pub relay: RelayStats,
    pub providers: Vec<ProviderHealth>,
}

impl HealthPayload {
    /// A DhakaFlix relay always reports a relay block and the mirror table, so
    /// this distinguishes it from any other server that happens to use the port.
    pub fn is_dhakaflix(&self) -> bool {
        self.status == "ok" && !self.providers.is_empty() && self.relay.max > 0
    }

    pub fn mirrors_online(&self) -> usize {
        self.providers.iter().filter(|provider| provider.online).count()
    }

    pub fn mirrors_total(&self) -> usize {
        self.providers.len()
    }
}

pub fn parse(body: &str) -> Option<HealthPayload> {
    serde_json::from_str::<HealthPayload>(body)
        .ok()
        .map(|payload| payload)
}

pub fn local_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/health")
}

pub fn manifest_url(origin: &str) -> String {
    format!("{}/manifest.json", origin.trim_end_matches('/'))
}

pub fn origin_host(origin: &str) -> String {
    origin
        .trim_end_matches('/')
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_string()
}

pub fn deep_link(manifest: &str) -> String {
    format!(
        "stremio://{}",
        manifest
            .trim_start_matches("https://")
            .trim_start_matches("http://")
    )
}

/// Probe the local relay. Returns the payload only when it really is DhakaFlix.
pub fn probe_local(port: u16, timeout_ms: u32) -> Option<HealthPayload> {
    let response = crate::http::get(&local_url(port), timeout_ms).ok()?;
    if response.status != 200 {
        return None;
    }
    parse(&response.body).filter(HealthPayload::is_dhakaflix)
}

/// Any HTTP 200 answer from the public origin counts as reachable.
pub fn reachable(url: &str, timeout_ms: u32) -> bool {
    matches!(crate::http::get(url, timeout_ms), Ok(response) if response.status == 200)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_dhakaflix_payload() {
        let body = r#"{
            "status":"ok","version":"1.2.0","uptimeSeconds":42,"streamMode":"proxy",
            "relay":{"active":1,"max":8},
            "providers":[{"id":"dhakaflix1","name":"DhakaFlix","host":"172.16.50.7:80","online":true,"known":true,"blocked":false}]
        }"#;
        let payload = parse(body).expect("payload parses");
        assert!(payload.is_dhakaflix());
        assert_eq!(payload.mirrors_online(), 1);
        assert_eq!(payload.relay.max, 8);
        assert_eq!(payload.uptime_seconds, 42);
    }

    #[test]
    fn rejects_foreign_json() {
        let payload = parse(r#"{"status":"ok"}"#).unwrap();
        assert!(!payload.is_dhakaflix());

        let payload = parse(r#"{"hello":"world"}"#).unwrap();
        assert!(!payload.is_dhakaflix());
    }

    #[test]
    fn builds_stremio_links() {
        assert_eq!(
            deep_link("https://flix.example.ts.net/manifest.json"),
            "stremio://flix.example.ts.net/manifest.json"
        );
        assert_eq!(
            origin_host("https://flix.example.ts.net/"),
            "flix.example.ts.net"
        );
    }
}
