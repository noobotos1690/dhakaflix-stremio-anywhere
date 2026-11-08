use sha2::{Digest, Sha256};
use std::fs;
use std::os::windows::process::CommandExt;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::envfile;
use crate::logging;
use crate::paths::Paths;

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub node: Option<PathBuf>,
    pub node_version: String,
    pub modules: bool,
    pub cloudflared: bool,
    pub env: bool,
}

pub struct Dependencies {
    paths: Paths,
}

impl Dependencies {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    pub fn inspect(&self) -> Snapshot {
        let node = self.node_path();
        let node_version = node
            .as_ref()
            .and_then(|n| Command::new(n).arg("--version").creation_flags(0x08000000).output().ok())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        Snapshot {
            node,
            node_version,
            modules: self.paths.node_modules.join("express").exists(),
            cloudflared: self.paths.cloudflared.exists(),
            env: self.paths.env_file.exists(),
        }
    }

    pub fn node_path(&self) -> Option<PathBuf> {
        if self.paths.node_exe.exists() {
            return Some(self.paths.node_exe.clone());
        }
        which("node.exe")
    }

    pub fn ensure(&self, provider: &str) -> Result<Snapshot, String> {
        self.paths.ensure();
        if self.node_path().is_none() {
            logging::info("Node.js was not found. Downloading the portable runtime.");
            self.install_node()?;
        }
        if !self.paths.env_file.exists() {
            envfile::ensure_env(&self.paths.env_file)?;
            logging::info("Created .env with a new PROXY_SECRET.");
        }
        if !self.paths.node_modules.join("express").exists() {
            logging::info("Installing npm dependencies. This can take a few minutes.");
            self.npm_install()?;
        }
        if provider == "cloudflare" && !self.paths.cloudflared.exists() {
            logging::info("Downloading cloudflared.");
            self.install_cloudflared()?;
        }
        Ok(self.inspect())
    }

    fn install_node(&self) -> Result<(), String> {
        fs::create_dir_all(&self.paths.runtime_dir).map_err(|e| e.to_string())?;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| e.to_string())?;
        let sums_url = "https://nodejs.org/dist/latest-v24.x/SHASUMS256.txt";
        let sums = client.get(sums_url).send().map_err(|e| e.to_string())?.text().map_err(|e| e.to_string())?;
        let (hash, file) = sums
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let hash = parts.next()?;
                let name = parts.next()?.trim_start_matches('*');
                if name.ends_with("win-x64.zip") {
                    Some((hash.to_string(), name.to_string()))
                } else {
                    None
                }
            })
            .next()
            .ok_or("Could not find a Node.js Windows x64 archive.")?;

        let archive = self.paths.runtime_dir.join(&file);
        let url = format!("https://nodejs.org/dist/latest-v24.x/{file}");
        logging::info(format!("Downloading {file}..."));
        let mut response = client.get(&url).send().map_err(|e| e.to_string())?;
        let mut out = fs::File::create(&archive).map_err(|e| e.to_string())?;
        std::io::copy(&mut response, &mut out).map_err(|e| e.to_string())?;
        out.flush().ok();

        let mut hasher = Sha256::new();
        let mut f = fs::File::open(&archive).map_err(|e| e.to_string())?;
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = f.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let actual = format!("{:x}", hasher.finalize());
        if !actual.eq_ignore_ascii_case(&hash) {
            let _ = fs::remove_file(&archive);
            return Err("Node.js download failed its SHA-256 check.".into());
        }

        let staging = self.paths.runtime_dir.join("node-extract");
        let _ = fs::remove_dir_all(&staging);
        fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipArchive::new(fs::File::open(&archive).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        zip.extract(&staging).map_err(|e| e.to_string())?;
        let extracted = fs::read_dir(&staging)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.is_dir() && p.join("node.exe").exists())
            .ok_or("The Node.js archive did not contain node.exe.")?;

        let _ = fs::remove_dir_all(&self.paths.node_dir);
        if fs::rename(&extracted, &self.paths.node_dir).is_err() {
            copy_dir(&extracted, &self.paths.node_dir)?;
        }
        let _ = fs::remove_dir_all(&staging);
        let _ = fs::remove_file(&archive);
        logging::info("Portable Node.js is ready.");
        Ok(())
    }

    fn npm_cli(&self) -> Option<PathBuf> {
        let node = self.node_path()?;
        let dir = node.parent()?;
        let bundled = dir.join("node_modules").join("npm").join("bin").join("npm-cli.js");
        if bundled.exists() {
            return Some(bundled);
        }
        which("npm.cmd").or_else(|| which("npm"))
    }

    fn npm_install(&self) -> Result<(), String> {
        let node = self.node_path().ok_or("Node.js is not available.")?;
        let npm = self.npm_cli().ok_or("npm was not found next to Node.js.")?;
        let code = run_stream(&node, &[npm.to_string_lossy().as_ref(), "install", "--no-audit", "--no-fund"], &self.paths.root)?;
        if code != 0 {
            return Err(format!("npm install failed with exit code {code}."));
        }
        Ok(())
    }

    fn install_cloudflared(&self) -> Result<(), String> {
        let node = self.node_path().ok_or("Node.js is not available.")?;
        let script = self.paths.root.join("scripts").join("fetch-cloudflared.js");
        if !script.exists() {
            return Err("scripts/fetch-cloudflared.js is missing from the app folder.".into());
        }
        fs::create_dir_all(&self.paths.tools_dir).map_err(|e| e.to_string())?;
        let code = run_stream(&node, &[script.to_string_lossy().as_ref()], &self.paths.root)?;
        if code != 0 || !self.paths.cloudflared.exists() {
            return Err(format!("cloudflared setup failed with exit code {code}."));
        }
        Ok(())
    }
}

fn which(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).map(|p| p.join(name)).find(|p| p.exists())
}

fn run_stream(program: &Path, args: &[&str], dir: &Path) -> Result<i32, String> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(stdout) = child.stdout.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                logging::info(line);
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                logging::warn(line);
            }
        });
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    Ok(status.code().unwrap_or(-1))
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
