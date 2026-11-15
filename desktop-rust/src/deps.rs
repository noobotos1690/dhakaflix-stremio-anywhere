//! Detection and installation of the runtime pieces the relay needs: Node.js,
//! the npm packages, cloudflared and a `.env` with a signing secret.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncBufReadExt;

use crate::app::App;
use crate::config::AppPaths;
use crate::settings::AccessMode;

#[derive(Clone, Debug, Default)]
pub struct Deps {
    pub source_ok: bool,
    pub node_ok: bool,
    pub node_path: Option<PathBuf>,
    pub node_version: String,
    pub modules_ok: bool,
    pub cloudflared_ok: bool,
    pub tailscale_ok: bool,
    pub tailscale_path: Option<PathBuf>,
    pub env_ok: bool,
    pub env_problem: Option<String>,
}

impl Deps {
    pub fn ready(&self, mode: AccessMode) -> bool {
        self.source_ok
            && self.node_ok
            && self.modules_ok
            && self.env_ok
            && if mode.is_tailscale() {
                self.tailscale_ok
            } else {
                self.cloudflared_ok
            }
    }

    pub fn missing(&self, mode: AccessMode) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.source_ok {
            missing.push("relay source files");
        }
        if !self.node_ok {
            missing.push("Node.js 18+");
        }
        if !self.modules_ok {
            missing.push("npm packages");
        }
        if mode.is_tailscale() {
            if !self.tailscale_ok {
                missing.push("Tailscale");
            }
        } else if !self.cloudflared_ok {
            missing.push("cloudflared");
        }
        if !self.env_ok {
            missing.push("configuration (.env)");
        }
        missing
    }
}

fn which(executable: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").ok()?;
    path.split(';')
        .filter(|dir| !dir.trim().is_empty())
        .map(|dir| Path::new(dir.trim()).join(executable))
        .find(|candidate| candidate.is_file())
}

fn node_version(node: &Path) -> Option<String> {
    if !node.is_file() {
        return None;
    }
    let mut command = std::process::Command::new(node);
    command.arg("-v");
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());
    crate::winutil::hide_console(&mut command);
    let output = command.output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let clean = text.trim_start_matches('v').trim_start_matches('V').to_string();
    if clean.is_empty() {
        None
    } else {
        Some(clean)
    }
}

fn major(version: &str) -> u32 {
    version
        .split('.')
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

pub fn resolve_node(paths: &AppPaths) -> Option<(PathBuf, String)> {
    let mut candidates = vec![paths.node_exe.clone()];

    let force_portable = std::env::var("DHAKAFLIX_FORCE_PORTABLE_NODE")
        .map(|value| value == "1")
        .unwrap_or(false);
    if !force_portable {
        if let Ok(override_path) = std::env::var("DHAKAFLIX_NODE") {
            candidates.push(PathBuf::from(override_path));
        }
        if let Some(on_path) = which("node.exe") {
            candidates.push(on_path);
        }
        for variable in ["ProgramFiles", "LOCALAPPDATA"] {
            if let Ok(base) = std::env::var(variable) {
                candidates.push(Path::new(&base).join(r"Programs\nodejs\node.exe"));
                candidates.push(Path::new(&base).join(r"nodejs\node.exe"));
            }
        }
    }

    for candidate in candidates {
        if let Some(version) = node_version(&candidate) {
            if major(&version) >= 18 {
                return Some((candidate, version));
            }
        }
    }
    None
}

pub fn modules_ready(paths: &AppPaths) -> bool {
    paths
        .node_modules
        .join("stremio-addon-sdk")
        .join("package.json")
        .is_file()
        && paths.node_modules.join("axios").is_dir()
}

pub fn cloudflared_ready(paths: &AppPaths) -> bool {
    std::fs::metadata(&paths.cloudflared_exe)
        .map(|meta| meta.len() > 1024 * 1024)
        .unwrap_or(false)
}

pub fn inspect(paths: &AppPaths, mode: AccessMode) -> Deps {
    let mut deps = Deps {
        source_ok: paths.has_relay_source(),
        modules_ok: modules_ready(paths),
        cloudflared_ok: cloudflared_ready(paths),
        ..Default::default()
    };

    match resolve_node(paths) {
        Some((path, version)) => {
            deps.node_ok = true;
            deps.node_path = Some(path);
            deps.node_version = version;
        }
        None => {}
    }

    if let Some(path) = crate::tailscale::find_executable() {
        deps.tailscale_ok = true;
        deps.tailscale_path = Some(path);
    }

    match crate::envfile::read_secret(&paths.env_file) {
        Some(_) => deps.env_ok = true,
        None => {
            deps.env_problem = Some(if paths.env_file.is_file() {
                "PROXY_SECRET is missing from .env".to_string()
            } else {
                ".env has not been created yet".to_string()
            });
        }
    }

    let _ = mode;
    deps
}

// --------------------------------------------------------------------- setup

pub async fn install(app: &Arc<App>) -> Result<(), String> {
    let paths = app.paths.clone();
    let mode = app.mode();

    if !paths.has_relay_source() {
        return Err(
            "index.js and package.json were not found next to the app. Reinstall DhakaFlix Relay."
                .to_string(),
        );
    }

    if resolve_node(&paths).is_none() {
        install_portable_node(app).await?;
    }

    ensure_env(app)?;

    if !modules_ready(&paths) {
        npm_install(app).await?;
    }

    if mode.is_tailscale() {
        if crate::tailscale::find_executable().is_none() {
            return Err(
                "Tailscale is not installed. Use Install Tailscale on the Public access page, then run Repair dependencies again."
                    .to_string(),
            );
        }
    } else if !cloudflared_ready(&paths) {
        install_cloudflared(app).await?;
    }

    let deps = inspect(&paths, mode);
    let ready = deps.ready(mode);
    let missing = deps.missing(mode);
    *app.deps.lock().unwrap() = Some(deps);
    app.refresh();

    if ready {
        tracing::info!("Dependencies are ready");
        Ok(())
    } else {
        Err(format!(
            "Setup finished but these are still missing: {}",
            missing.join(", ")
        ))
    }
}

/// Render `.env` from the template with `secret` filled in, then write it.
fn write_env_from_template(paths: &AppPaths, secret: &str) -> Result<(), String> {
    let template = std::fs::read_to_string(&paths.env_template).unwrap_or_else(|_| {
        "# DhakaFlix relay configuration\nPORT=7001\nHOST=0.0.0.0\nSTREAM_MODE=proxy\nPROXY_SECRET=\n"
            .to_string()
    });
    let mut lines: Vec<String> = template
        .replace("\r\n", "\n")
        .lines()
        .map(|line| line.to_string())
        .collect();
    let mut replaced = false;
    for line in lines.iter_mut() {
        if line.trim_start().starts_with("PROXY_SECRET=") {
            *line = format!("PROXY_SECRET={secret}");
            replaced = true;
        }
    }
    if !replaced {
        lines.push(format!("PROXY_SECRET={secret}"));
    }
    let content = format!("{}\n", lines.join("\n").trim_end());
    std::fs::write(&paths.env_file, content)
        .map_err(|error| format!("Could not write .env: {error}"))
}

/// Create `.env` with a fresh PROXY_SECRET when this folder has none yet.
///
/// This runs on startup so a freshly unpacked copy does not need the Repair
/// dependencies step. An existing `.env` is never rewritten here, even when its
/// secret is missing, so a hand-edited file keeps its values; Repair
/// dependencies handles that case explicitly.
pub fn create_env_if_missing(paths: &AppPaths) -> Result<Option<String>, String> {
    if paths.env_file.exists() {
        return Ok(None);
    }

    let _ = std::fs::create_dir_all(&paths.logs_dir);
    let secret = random_hex(32);
    write_env_from_template(paths, &secret)?;
    tracing::info!("Created .env with a new PROXY_SECRET");
    Ok(Some(secret))
}

fn ensure_env(app: &Arc<App>) -> Result<(), String> {
    let paths = &app.paths;
    let _ = std::fs::create_dir_all(&paths.logs_dir);
    let _ = std::fs::create_dir_all(&paths.tools_dir);

    if crate::envfile::read_secret(&paths.env_file).is_some() {
        tracing::debug!("Keeping the existing PROXY_SECRET in .env");
        app.register_secret_from_env();
        return Ok(());
    }

    let secret = random_hex(32);
    write_env_from_template(paths, &secret)?;

    tracing::info!("Created .env with a generated PROXY_SECRET");
    app.register_secret(&secret);
    Ok(())
}

pub fn random_hex(bytes: usize) -> String {
    use windows_sys::Win32::Security::Cryptography::{
        BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
    };

    let mut buffer = vec![0u8; bytes];
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buffer.as_mut_ptr(),
            bytes as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status != 0 {
        // Extremely unlikely; still better than writing nothing.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or(0);
        tracing::warn!("The system random generator failed; using a time-based secret");
        buffer = format!("{nanos:032x}").into_bytes();
    }

    buffer.iter().map(|byte| format!("{byte:02x}")).collect()
}

async fn install_portable_node(app: &Arc<App>) -> Result<(), String> {
    tracing::info!("Node.js 18+ was not found. Downloading a private runtime.");
    app.toast("info", "Downloading Node.js…");

    let paths = app.paths.clone();
    let download = tokio::task::spawn_blocking(move || download_node_zip())
        .await
        .map_err(|error| error.to_string())??;

    let (zip_path, expected_hash) = download;
    if !verify_sha256(&zip_path, &expected_hash) {
        let _ = std::fs::remove_file(&zip_path);
        return Err("The Node.js archive did not match its published checksum. Try again.".to_string());
    }

    let extract_dir = std::env::temp_dir().join(format!("dhakaflix-node-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&extract_dir);
    std::fs::create_dir_all(&extract_dir).map_err(|error| error.to_string())?;

    tracing::info!("Extracting the Node.js runtime");
    extract_zip(&zip_path, &extract_dir)?;

    let source = std::fs::read_dir(&extract_dir)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .find(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().contains("win-x64"))
                .unwrap_or(false)
        })
        .or_else(|| {
            std::fs::read_dir(&extract_dir)
                .ok()?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .find(|path| path.is_dir())
        })
        .ok_or_else(|| "The Node.js archive was empty.".to_string())?;

    std::fs::create_dir_all(&paths.runtime_dir).map_err(|error| error.to_string())?;
    if paths.node_dir.exists() {
        let _ = std::fs::remove_dir_all(&paths.node_dir);
    }
    move_directory(&source, &paths.node_dir)?;

    let _ = std::fs::remove_file(&zip_path);
    let _ = std::fs::remove_dir_all(&extract_dir);

    match resolve_node(&paths) {
        Some((_, version)) => {
            tracing::info!("Node.js {version} is ready");
            Ok(())
        }
        None => Err("The downloaded Node.js runtime did not start correctly.".to_string()),
    }
}

fn download_node_zip() -> Result<(PathBuf, String), String> {
    let mut last_error = String::new();
    for channel in ["latest-v24.x", "latest-v22.x"] {
        let base = format!("https://nodejs.org/dist/{channel}");
        let sums = match crate::http::get(&format!("{base}/SHASUMS256.txt"), 30_000) {
            Ok(response) if response.status == 200 => response.body,
            Ok(response) => {
                last_error = format!("nodejs.org answered HTTP {}", response.status);
                continue;
            }
            Err(error) => {
                last_error = error;
                continue;
            }
        };

        let mut chosen: Option<(String, String)> = None;
        for line in sums.lines() {
            let mut parts = line.split_whitespace();
            let Some(hash) = parts.next() else { continue };
            let Some(name) = parts.next() else { continue };
            if name.ends_with("-win-x64.zip") {
                chosen = Some((name.to_string(), hash.to_string()));
                break;
            }
        }

        let Some((name, hash)) = chosen else {
            last_error = "nodejs.org did not list a Windows x64 build".to_string();
            continue;
        };

        let destination = std::env::temp_dir().join(&name);
        tracing::info!("Downloading {name}");
        crate::http::download(&format!("{base}/{name}"), &destination, 120_000)?;
        return Ok((destination, hash));
    }

    Err(format!(
        "Could not download Node.js automatically ({last_error}). Install Node.js 18+ from nodejs.org, then run Repair dependencies."
    ))
}

fn verify_sha256(path: &Path, expected: &str) -> bool {
    let mut command = std::process::Command::new("certutil");
    command.args(["-hashfile", &path.to_string_lossy(), "SHA256"]);
    crate::winutil::hide_console(&mut command);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::null());

    let Ok(output) = command.output() else {
        tracing::warn!("certutil is unavailable; skipping the checksum check");
        return true;
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let actual = text.lines().map(str::trim).find(|line| {
        line.len() == 64 && line.chars().all(|c| c.is_ascii_hexdigit())
    });
    match actual {
        Some(actual) => actual.eq_ignore_ascii_case(expected.trim()),
        None => {
            tracing::warn!("Could not read the downloaded file checksum; continuing");
            true
        }
    }
}

fn extract_zip(zip: &Path, destination: &Path) -> Result<(), String> {
    let script = format!(
        "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
        zip.display(),
        destination.display()
    );
    let mut command = std::process::Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &script]);
    crate::winutil::hide_console(&mut command);
    command.stdout(Stdio::null());
    command.stderr(Stdio::piped());

    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "The Node.js archive could not be extracted: {}",
            String::from_utf8_lossy(&output.stderr).lines().next().unwrap_or("unknown error")
        ))
    }
}

fn move_directory(source: &Path, destination: &Path) -> Result<(), String> {
    if std::fs::rename(source, destination).is_ok() {
        return Ok(());
    }
    copy_directory(source, destination).map_err(|error| error.to_string())?;
    let _ = std::fs::remove_dir_all(source);
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

async fn npm_install(app: &Arc<App>) -> Result<(), String> {
    let paths = app.paths.clone();
    let (node, version) = resolve_node(&paths)
        .ok_or_else(|| "Node.js is not ready, so npm cannot run.".to_string())?;

    let node_dir = node
        .parent()
        .ok_or_else(|| "The Node.js folder could not be determined.".to_string())?;

    let npm_cli = node_dir.join(r"node_modules\npm\bin\npm-cli.js");
    if !npm_cli.is_file() {
        return Err("npm was not found next to Node.js. Install Node.js from nodejs.org, then retry.".to_string());
    }

    tracing::info!("Installing npm packages with Node.js {version} (this can take a minute)");
    app.toast("info", "Installing npm packages…");

    let mut command = tokio::process::Command::new(&node);
    command.arg(npm_cli);
    command.args(["install", "--omit=dev", "--no-audit", "--no-fund"]);
    command.current_dir(&paths.root);
    command.env("PATH", prepend_path(node_dir));
    command.env("npm_config_audit", "false");
    command.env("npm_config_fund", "false");
    command.env("npm_config_update_notifier", "false");
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    crate::winutil::hide_console_async(&mut command);

    let code = stream_command(command, Duration::from_secs(600)).await?;
    if code != 0 {
        return Err(format!(
            "npm install failed (exit code {code}). Check your connection and try again."
        ));
    }
    tracing::info!("npm packages installed");
    Ok(())
}

async fn install_cloudflared(app: &Arc<App>) -> Result<(), String> {
    let paths = app.paths.clone();
    let (node, _) = resolve_node(&paths)
        .ok_or_else(|| "Node.js is not ready, so cloudflared cannot be downloaded.".to_string())?;
    let script = paths.root.join(r"scripts\fetch-cloudflared.js");
    if !script.is_file() {
        return Err("scripts/fetch-cloudflared.js is missing. Reinstall the app.".to_string());
    }

    tracing::info!("Downloading cloudflared for the public tunnel");
    app.toast("info", "Downloading cloudflared…");

    let mut command = tokio::process::Command::new(&node);
    command.arg(script);
    command.current_dir(&paths.root);
    if let Some(dir) = node.parent() {
        command.env("PATH", prepend_path(dir));
    }
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    crate::winutil::hide_console_async(&mut command);

    let code = stream_command(command, Duration::from_secs(600)).await?;
    if code != 0 {
        return Err(
            "cloudflared could not be downloaded. The relay can still run locally, but Stremio needs the tunnel."
                .to_string(),
        );
    }
    tracing::info!("cloudflared installed");
    Ok(())
}

/// Run a command and forward both streams into the log at debug level.
pub async fn stream_command(
    mut command: tokio::process::Command,
    timeout: Duration,
) -> Result<i32, String> {
    let mut child = command.spawn().map_err(|error| error.to_string())?;

    let pump = |stream: Option<Box<dyn tokio::io::AsyncRead + Unpin + Send>>, error: bool| {
        if let Some(stream) = stream {
            tokio::spawn(async move {
                let mut lines = tokio::io::BufReader::new(stream).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let line = line.trim_end().to_string();
                    if line.is_empty() {
                        continue;
                    }
                    if error {
                        tracing::warn!("{line}");
                    } else {
                        tracing::debug!("{line}");
                    }
                }
            });
        }
    };

    pump(
        child.stdout.take().map(|stream| Box::new(stream) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
        false,
    );
    pump(
        child.stderr.take().map(|stream| Box::new(stream) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
        true,
    );

    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => Ok(status.code().unwrap_or(-1)),
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => {
            let _ = child.start_kill();
            Err("The command timed out.".to_string())
        }
    }
}

pub fn prepend_path(directory: &Path) -> String {
    let existing = std::env::var("PATH").unwrap_or_default();
    format!("{};{}", directory.display(), existing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths(name: &str) -> AppPaths {
        let root = std::env::temp_dir().join(format!("dhakaflix-deps-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        AppPaths {
            exe: root.join("DhakaFlixRelay.exe"),
            logs_dir: root.join("logs"),
            tools_dir: root.join("tools"),
            runtime_dir: root.join("runtime"),
            node_dir: root.join("runtime").join("node"),
            node_exe: root.join("runtime").join("node").join("node.exe"),
            node_modules: root.join("node_modules"),
            env_file: root.join(".env"),
            env_template: root.join(".env.example"),
            cloudflared_exe: root.join("tools").join("cloudflared.exe"),
            log_file: root.join("logs").join("relay-desktop.log"),
            settings_dir: root.join("settings"),
            settings_file: root.join("settings").join("settings.json"),
            root,
        }
    }

    #[test]
    fn env_is_created_once_and_keeps_its_secret() {
        let paths = temp_paths("env");
        std::fs::write(&paths.env_template, "PORT=7001\nPROXY_SECRET=\n").unwrap();

        let secret = create_env_if_missing(&paths).unwrap().expect("first run writes a secret");
        assert_eq!(secret.len(), 64, "a 32-byte hex secret");
        let contents = std::fs::read_to_string(&paths.env_file).unwrap();
        assert!(contents.contains(&format!("PROXY_SECRET={secret}")));
        assert!(contents.contains("PORT=7001"), "the template is preserved");

        // A second start must neither replace the file nor rotate the secret.
        assert!(create_env_if_missing(&paths).unwrap().is_none());
        assert_eq!(std::fs::read_to_string(&paths.env_file).unwrap(), contents);

        let _ = std::fs::remove_dir_all(&paths.root);
    }
}
