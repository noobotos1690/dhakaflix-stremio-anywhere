//! Update checks and self-install from the GitHub releases page.
//!
//! The API is queried without a token, so it is limited to the public release
//! feed of this repository. When a newer tag is published and it carries a
//! setup executable, that installer is downloaded and started: Inno Setup
//! closes this app, replaces the files and restarts it.

use std::path::PathBuf;

pub const RELEASES_PAGE: &str = "https://github.com/noobotos1690/dhakaflix-stremio-anywhere/releases";
const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/noobotos1690/dhakaflix-stremio-anywhere/releases/latest";

#[derive(Clone, Debug, Default)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page_url: String,
    pub setup_url: Option<String>,
}

fn parse_version(text: &str) -> Vec<u64> {
    text.trim()
        .trim_start_matches(['v', 'V'])
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

/// True when `candidate` is newer than `current`, comparing number by number.
pub fn is_newer(current: &str, candidate: &str) -> bool {
    let mut current_parts = parse_version(current);
    let mut candidate_parts = parse_version(candidate);
    let length = current_parts.len().max(candidate_parts.len());
    current_parts.resize(length, 0);
    candidate_parts.resize(length, 0);
    candidate_parts > current_parts
}

/// Read the newest published release from the GitHub API.
pub fn latest_release() -> Result<Release, String> {
    let response = crate::http::get(LATEST_RELEASE_API, 12_000)?;
    if response.status == 404 {
        return Err("this repository has no published release yet".to_string());
    }
    if response.status != 200 {
        return Err(format!("GitHub answered HTTP {}", response.status));
    }

    let json: serde_json::Value = serde_json::from_str(&response.body)
        .map_err(|error| format!("unexpected answer from GitHub: {error}"))?;

    let version = json
        .get("tag_name")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    if version.is_empty() {
        return Err("GitHub did not report a release version".to_string());
    }

    let setup_url = json
        .get("assets")
        .and_then(|assets| assets.as_array())
        .and_then(|assets| {
            assets.iter().find_map(|asset| {
                let name = asset
                    .get("name")
                    .and_then(|value| value.as_str())?
                    .to_lowercase();
                if !name.ends_with(".exe") || !name.contains("setup") {
                    return None;
                }
                asset
                    .get("browser_download_url")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            })
        });

    Ok(Release {
        version,
        notes: json
            .get("body")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        page_url: json
            .get("html_url")
            .and_then(|value| value.as_str())
            .unwrap_or(RELEASES_PAGE)
            .to_string(),
        setup_url,
    })
}

/// Download a release installer and start it.
pub fn run_installer(url: &str) -> Result<(), String> {
    let target: PathBuf = std::env::temp_dir().join("DhakaFlixRelay-Setup-latest.exe");
    let bytes = crate::http::download(url, &target, 180_000)?;
    if bytes < 1_000_000 {
        return Err("the downloaded installer looks incomplete".to_string());
    }

    std::process::Command::new(&target)
        .args([
            "/SILENT",
            "/CLOSEAPPLICATIONS",
            "/RESTARTAPPLICATIONS",
            "/NORESTART",
        ])
        .spawn()
        .map_err(|error| format!("the installer could not be started: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn compares_versions_numerically() {
        assert!(is_newer("1.3.0", "1.4.0"));
        assert!(is_newer("1.3.0", "v1.10.0"));
        assert!(is_newer("1.9.0", "1.10.0"));
        assert!(!is_newer("1.3.0", "1.3.0"));
        assert!(!is_newer("1.4.0", "1.3.9"));
    }
}
