use crate::error::CodexErr;
use crate::error::ConnectionFailedError;
use crate::error::EnvVarError;
use crate::error::UnexpectedResponseError;
use reqwest::blocking::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

const COPILOT_TOKEN_ENDPOINT: &str = "https://api.github.com/copilot_internal/v2/token";
const COPILOT_EDITOR_VERSION: &str = "vscode/1.96.0";
const COPILOT_EDITOR_PLUGIN_VERSION: &str = "copilot-chat/0.26.7";
const COPILOT_USER_AGENT: &str = "GitHubCopilotChat/0.26.7";
const COPILOT_API_VERSION: &str = "2025-04-01";
const COPILOT_TOKEN_REFRESH_SKEW_SECS: i64 = 120;

static COPILOT_TOKEN_CACHE: OnceLock<Mutex<Option<CachedCopilotToken>>> = OnceLock::new();

#[derive(Clone)]
struct CachedCopilotToken {
    token: String,
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
struct CopilotTokenResponse {
    token: String,
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
struct HostEntry {
    oauth_token: String,
}

pub(crate) fn get_copilot_token() -> crate::error::Result<String> {
    let cache = COPILOT_TOKEN_CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock().expect("copilot token cache poisoned");
    let now = unix_timestamp_now();

    if let Some(cached) = guard.as_ref()
        && now < cached.expires_at - COPILOT_TOKEN_REFRESH_SKEW_SECS
    {
        return Ok(cached.token.clone());
    }

    let github_token = discover_github_token().ok_or_else(missing_token_error)?;
    let exchanged = exchange_github_token(&github_token)?;
    let token = exchanged.token.clone();
    *guard = Some(CachedCopilotToken {
        token: exchanged.token,
        expires_at: exchanged.expires_at,
    });
    Ok(token)
}

fn discover_github_token() -> Option<String> {
    if let Ok(token) = std::env::var("GH_COPILOT_TOKEN")
        && !token.trim().is_empty()
    {
        return Some(token);
    }

    read_token_from_config("hosts.json")
        .or_else(|| read_token_from_config("apps.json"))
        .or_else(read_token_from_gh_cli)
}

fn read_token_from_config(filename: &str) -> Option<String> {
    for config_dir in copilot_config_dirs() {
        let path = config_dir.join(filename);
        let content = std::fs::read_to_string(path).ok()?;
        let hosts = serde_json::from_str::<HashMap<String, HostEntry>>(&content).ok()?;
        let entry = hosts.get("github.com")?;
        if !entry.oauth_token.trim().is_empty() {
            return Some(entry.oauth_token.clone());
        }
    }

    None
}

fn copilot_config_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let xdg_config = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".config")
        });
    dirs.push(xdg_config.join("github-copilot"));

    #[cfg(target_os = "macos")]
    if let Some(home) = dirs::home_dir() {
        dirs.push(
            home.join("Library")
                .join("Application Support")
                .join("github-copilot"),
        );
    }

    dirs
}

fn read_token_from_gh_cli() -> Option<String> {
    let output = Command::new("gh").args(["auth", "token"]).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() { None } else { Some(token) }
}

fn exchange_github_token(github_token: &str) -> crate::error::Result<CopilotTokenResponse> {
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(connection_failed)?;
    let response = client
        .get(COPILOT_TOKEN_ENDPOINT)
        .header("Authorization", format!("token {github_token}"))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("Editor-Version", COPILOT_EDITOR_VERSION)
        .header("Editor-Plugin-Version", COPILOT_EDITOR_PLUGIN_VERSION)
        .header("User-Agent", COPILOT_USER_AGENT)
        .header("X-GitHub-Api-Version", COPILOT_API_VERSION)
        .send()
        .map_err(connection_failed)?;

    let status = response.status();
    let body = response.text().map_err(connection_failed)?;
    if !status.is_success() {
        return Err(CodexErr::UnexpectedStatus(UnexpectedResponseError {
            status,
            body,
            url: Some(COPILOT_TOKEN_ENDPOINT.to_string()),
            cf_ray: None,
            request_id: None,
        }));
    }

    let exchanged: CopilotTokenResponse = serde_json::from_str(&body)?;
    if exchanged.token.trim().is_empty() {
        return Err(CodexErr::InvalidRequest(
            "GitHub Copilot token exchange returned an empty session token".to_string(),
        ));
    }

    Ok(exchanged)
}

fn connection_failed(source: reqwest::Error) -> CodexErr {
    CodexErr::ConnectionFailed(ConnectionFailedError { source })
}

fn missing_token_error() -> CodexErr {
    CodexErr::EnvVar(EnvVarError {
        var: "GH_COPILOT_TOKEN".to_string(),
        instructions: Some(
            "Set GH_COPILOT_TOKEN, sign into GitHub Copilot in VS Code/JetBrains so hosts.json exists, or run `gh auth login`.".to_string(),
        ),
    })
}

fn unix_timestamp_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    #[serial]
    fn test_env_var_takes_priority() {
        unsafe {
            std::env::set_var("GH_COPILOT_TOKEN", "gho_test_token_123");
            std::env::remove_var("XDG_CONFIG_HOME");
        }
        assert_eq!(
            discover_github_token(),
            Some("gho_test_token_123".to_string())
        );
        unsafe {
            std::env::remove_var("GH_COPILOT_TOKEN");
        }
    }

    #[test]
    #[serial]
    fn test_parse_hosts_json() {
        let tmp = TempDir::new().unwrap();
        let copilot_dir = tmp.path().join("github-copilot");
        std::fs::create_dir_all(&copilot_dir).unwrap();
        let mut file = std::fs::File::create(copilot_dir.join("hosts.json")).unwrap();
        file.write_all(
            br#"{
                "github.com": {
                    "oauth_token": "gho_from_hosts_json"
                }
            }"#,
        )
        .unwrap();

        unsafe {
            std::env::remove_var("GH_COPILOT_TOKEN");
            std::env::set_var("XDG_CONFIG_HOME", tmp.path());
        }
        assert_eq!(
            read_token_from_config("hosts.json"),
            Some("gho_from_hosts_json".to_string())
        );
        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
    }
}
