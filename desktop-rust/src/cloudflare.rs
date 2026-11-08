use std::path::Path;

/// Cloudflare Quick Tunnel is started by the Node relay (`--tunnel`).
/// This module only owns availability checks and URL parsing helpers.
pub fn cloudflared_available(path: &Path) -> bool {
    path.exists()
}

pub fn quick_url(line: &str) -> Option<String> {
    for token in line.split_whitespace() {
        let token = token.trim_matches(|c: char| c == '"' || c == '\'' || c == '(' || c == ')' || c == ',' || c == '.');
        if token.starts_with("https://") && token.contains(".trycloudflare.com") {
            return Some(token.split('/').take(3).collect::<Vec<_>>().join("/"));
        }
    }
    None
}
