//! GitHub Copilot token discovery and validation.
//!
//! Discovers GitHub OAuth tokens from multiple sources, in priority order:
//! 1. `GH_COPILOT_TOKEN` environment variable
//! 2. `~/.config/github-copilot/hosts.json` (VS Code, JetBrains, etc.)
//! 3. `~/.config/github-copilot/apps.json` (newer Copilot installations)
//! 4. `gh auth token` CLI command output
//!
//! # Usage
//!
//! ```rust,no_run
//! use codex_core::copilot_auth::discover_github_token;
//!
//! if let Some(token) = discover_github_token() {
//!     println!("Found token: {}...", &token[..8]);
//! }
//! ```

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

/// Entry in hosts.json or apps.json keyed by hostname.
#[derive(Debug, Deserialize)]
struct HostEntry {
    oauth_token: String,
    #[allow(dead_code)]
    #[serde(default)]
    user: Option<String>,
}

/// Discover a GitHub OAuth token for Copilot API access.
///
/// Checks sources in priority order and returns the first valid token found.
/// Returns `None` if no token is available from any source.
pub fn discover_github_token() -> Option<String> {
    // 1. Environment variable (highest priority — explicit user intent)
    if let Ok(token) = std::env::var("GH_COPILOT_TOKEN") {
        if !token.is_empty() {
            return Some(token);
        }
    }

    // 2. hosts.json (written by VS Code / JetBrains Copilot extensions)
    if let Some(token) = read_token_from_config("hosts.json") {
        return Some(token);
    }

    // 3. apps.json (newer Copilot installations)
    if let Some(token) = read_token_from_config("apps.json") {
        return Some(token);
    }

    // 4. GitHub CLI (requires `gh` to be installed and authenticated)
    if let Some(token) = read_token_from_gh_cli() {
        return Some(token);
    }

    None
}

/// Resolve the Copilot config directory.
///
/// Respects `XDG_CONFIG_HOME` on Linux/macOS, falls back to `~/.config`.
/// On macOS, also checks `~/Library/Application Support` as a secondary
/// location (some editors use this path).
fn copilot_config_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // Primary: XDG_CONFIG_HOME or ~/.config
    let xdg_config = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".config")
        });
    dirs.push(xdg_config.join("github-copilot"));

    // Secondary on macOS: ~/Library/Application Support
    #[cfg(target_os = "macos")]
    if let Some(home) = dirs::home_dir() {
        let app_support = home
            .join("Library")
            .join("Application Support")
            .join("github-copilot");
        dirs.push(app_support);
    }

    dirs
}

/// Read an OAuth token from a Copilot config file.
///
/// The file format is a JSON object mapping hostnames to entries:
/// ```json
/// {
///   "github.com": {
///     "oauth_token": "gho_xxxxxxxxxxxx",
///     "user": "username"
///   }
/// }
/// ```
fn read_token_from_config(filename: &str) -> Option<String> {
    for dir in copilot_config_dirs() {
        let path = dir.join(filename);
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(hosts) = serde_json::from_str::<HashMap<String, HostEntry>>(&content) {
                if let Some(entry) = hosts.get("github.com") {
                    if !entry.oauth_token.is_empty() {
                        return Some(entry.oauth_token.clone());
                    }
                }
            }
        }
    }
    None
}

/// Read a token from the GitHub CLI.
fn read_token_from_gh_cli() -> Option<String> {
    let output = Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

/// Validate that a token has active Copilot access.
///
/// Calls the Copilot internal token endpoint. This exchanges the GitHub
/// OAuth token for a short-lived Copilot session token. If the exchange
/// succeeds, the user has an active Copilot subscription.
///
/// Note: This is an async function that requires a Tokio runtime.
/// For the built-in provider, you may want to call this during
/// provider initialization to give early feedback.
#[cfg(feature = "validate")]
pub async fn validate_copilot_token(token: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.github.com/copilot_internal/v2/token")
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "codex-cli")
        .send()
        .await?;
    Ok(resp.status().is_success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_env_var_takes_priority() {
        // Set env var and verify it's returned
        std::env::set_var("GH_COPILOT_TOKEN", "gho_test_token_123");
        let token = discover_github_token();
        assert_eq!(token, Some("gho_test_token_123".to_string()));
        std::env::remove_var("GH_COPILOT_TOKEN");
    }

    #[test]
    fn test_empty_env_var_is_skipped() {
        std::env::set_var("GH_COPILOT_TOKEN", "");
        // Won't return empty string; falls through to other sources
        // (which may or may not find a token depending on the test env)
        let token = discover_github_token();
        assert_ne!(token, Some("".to_string()));
        std::env::remove_var("GH_COPILOT_TOKEN");
    }

    #[test]
    fn test_parse_hosts_json() {
        let tmp = TempDir::new().unwrap();
        let copilot_dir = tmp.path().join("github-copilot");
        std::fs::create_dir_all(&copilot_dir).unwrap();

        let hosts_json = r#"{
            "github.com": {
                "oauth_token": "gho_from_hosts_json",
                "user": "testuser"
            }
        }"#;

        let mut f = std::fs::File::create(copilot_dir.join("hosts.json")).unwrap();
        f.write_all(hosts_json.as_bytes()).unwrap();

        // Point XDG_CONFIG_HOME to our temp dir
        std::env::set_var("XDG_CONFIG_HOME", tmp.path());
        std::env::remove_var("GH_COPILOT_TOKEN");

        let token = read_token_from_config("hosts.json");
        assert_eq!(token, Some("gho_from_hosts_json".to_string()));

        std::env::remove_var("XDG_CONFIG_HOME");
    }
}
