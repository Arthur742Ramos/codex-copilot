use crate::error::CodexErr;
use crate::error::ConnectionFailedError;
use crate::error::EnvVarError;
use crate::error::UnexpectedResponseError;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde::Serialize;
use std::fs::OpenOptions;
use std::io;
use std::io::IsTerminal;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

const COPILOT_TOKEN_ENDPOINT: &str = "https://api.github.com/copilot_internal/v2/token";
const GITHUB_DEVICE_CODE_ENDPOINT: &str = "https://github.com/login/device/code";
const GITHUB_DEVICE_ACCESS_TOKEN_ENDPOINT: &str = "https://github.com/login/oauth/access_token";
const GITHUB_DEVICE_VERIFICATION_URI: &str = "https://github.com/login/device";
const GITHUB_COPILOT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const COPILOT_EDITOR_VERSION: &str = "vscode/1.96.0";
const COPILOT_EDITOR_PLUGIN_VERSION: &str = "copilot-chat/0.26.7";
const COPILOT_USER_AGENT: &str = "GitHubCopilotChat/0.26.7";
const COPILOT_API_VERSION: &str = "2025-04-01";
const COPILOT_TOKEN_REFRESH_SKEW_SECS: i64 = 120;
pub const CODEX_GH_COPILOT_TOKEN_ENV_VAR: &str = "CODEX_GH_COPILOT_TOKEN";

static COPILOT_TOKEN_CACHE: OnceLock<Mutex<Option<CachedCopilotToken>>> = OnceLock::new();

#[derive(Clone)]
struct CachedCopilotToken {
    token: String,
    expires_at: i64,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct CopilotTokenResponse {
    token: String,
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: Option<String>,
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct DeviceAccessTokenResponse {
    access_token: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct SavedGithubToken {
    github_token: String,
}

enum GithubTokenSource {
    EnvVar(String),
    DeviceCache(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopilotAuthSource {
    EnvVar,
    DeviceCache,
}

impl GithubTokenSource {
    fn source(&self) -> CopilotAuthSource {
        match self {
            Self::EnvVar(_) => CopilotAuthSource::EnvVar,
            Self::DeviceCache(_) => CopilotAuthSource::DeviceCache,
        }
    }
}

pub fn copilot_auth_source() -> Option<CopilotAuthSource> {
    discover_github_token()
        .as_ref()
        .map(GithubTokenSource::source)
}

pub fn ensure_copilot_auth() -> crate::error::Result<CopilotAuthSource> {
    let source = copilot_auth_source().unwrap_or(CopilotAuthSource::DeviceCache);
    let _ = get_copilot_token()?;
    Ok(source)
}

pub fn run_copilot_device_flow() -> crate::error::Result<()> {
    run_device_flow().map(|_| ())
}

pub fn copilot_auth_file_path() -> PathBuf {
    codex_config_dir().join("token.json")
}

pub fn clear_copilot_auth() -> io::Result<bool> {
    let path = copilot_auth_file_path();
    let existed = path.exists();
    if existed {
        std::fs::remove_file(path)?;
    }

    if let Some(cache) = COPILOT_TOKEN_CACHE.get() {
        let mut guard = cache.lock().expect("copilot token cache poisoned");
        *guard = None;
    }

    Ok(existed)
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

    let (github_token, from_device_cache) = match discover_github_token() {
        Some(GithubTokenSource::EnvVar(github_token)) => (github_token, false),
        Some(GithubTokenSource::DeviceCache(github_token)) => (github_token, true),
        None => {
            if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
                return Err(missing_token_error());
            }
            (run_device_flow()?, false)
        }
    };

    let exchanged = match exchange_github_token(&github_token) {
        Ok(exchanged) => exchanged,
        Err(CodexErr::UnexpectedStatus(err))
            if err.status == reqwest::StatusCode::UNAUTHORIZED && from_device_cache =>
        {
            if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
                return Err(missing_token_error());
            }
            let github_token = run_device_flow()?;
            exchange_github_token(&github_token)?
        }
        Err(err) => return Err(err),
    };

    let token = exchanged.token.clone();
    *guard = Some(CachedCopilotToken {
        token: exchanged.token,
        expires_at: exchanged.expires_at,
    });
    Ok(token)
}

fn discover_github_token() -> Option<GithubTokenSource> {
    if let Ok(token) = std::env::var(CODEX_GH_COPILOT_TOKEN_ENV_VAR)
        && !token.trim().is_empty()
    {
        return Some(GithubTokenSource::EnvVar(token));
    }

    read_token_from_device_flow_cache().map(GithubTokenSource::DeviceCache)
}

fn codex_config_dir() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".config")
        })
        .join("codex-copilot")
}

/// Read a GitHub token cached by the Copilot device flow
/// (`~/.config/codex-copilot/token.json`).
fn read_token_from_device_flow_cache() -> Option<String> {
    let path = copilot_auth_file_path();
    let content = std::fs::read_to_string(path).ok()?;
    let data: SavedGithubToken = serde_json::from_str(&content).ok()?;
    if data.github_token.trim().is_empty() {
        None
    } else {
        Some(data.github_token)
    }
}

fn run_device_flow() -> crate::error::Result<String> {
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::block_in_place(run_device_flow_inner)
    } else {
        run_device_flow_inner()
    }
}

fn run_device_flow_inner() -> crate::error::Result<String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(connection_failed)?;
    let response = client
        .post(GITHUB_DEVICE_CODE_ENDPOINT)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "client_id": GITHUB_COPILOT_CLIENT_ID,
            "scope": "read:user",
        }))
        .send()
        .map_err(connection_failed)?;
    let status = response.status();
    let body = response.text().map_err(connection_failed)?;
    if !status.is_success() {
        return Err(CodexErr::InvalidRequest(format!(
            "GitHub device code request failed with status {status}: {body}"
        )));
    }

    let device_code: DeviceCodeResponse = serde_json::from_str(&body)?;
    let verification_uri = device_code
        .verification_uri
        .as_deref()
        .unwrap_or(GITHUB_DEVICE_VERIFICATION_URI);
    write_device_flow_instructions(verification_uri, &device_code.user_code)?;

    let mut interval = device_code.interval.unwrap_or(5) + 1;
    loop {
        std::thread::sleep(Duration::from_secs(interval));
        let response = client
            .post(GITHUB_DEVICE_ACCESS_TOKEN_ENDPOINT)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&serde_json::json!({
                "client_id": GITHUB_COPILOT_CLIENT_ID,
                "device_code": &device_code.device_code,
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
            }))
            .send()
            .map_err(connection_failed)?;
        let status = response.status();
        let body = response.text().map_err(connection_failed)?;
        if !status.is_success() {
            return Err(CodexErr::InvalidRequest(format!(
                "GitHub device access-token request failed with status {status}: {body}"
            )));
        }

        let token_data: DeviceAccessTokenResponse = serde_json::from_str(&body)?;
        if let Some(access_token) = token_data.access_token {
            let path = copilot_auth_file_path();
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let mut options = OpenOptions::new();
            options.truncate(true).write(true).create(true);
            #[cfg(unix)]
            {
                options.mode(0o600);
            }
            let mut file = options.open(&path)?;
            file.write_all(
                serde_json::to_string(&SavedGithubToken {
                    github_token: access_token.clone(),
                })?
                .as_bytes(),
            )?;
            file.flush()?;
            file.sync_all()?;
            return Ok(access_token);
        }

        match token_data.error.as_deref() {
            Some("authorization_pending") => continue,
            Some("slow_down") => {
                interval += 5;
            }
            Some("expired_token") => {
                return Err(CodexErr::InvalidRequest(
                    "GitHub device code expired before authorization completed".to_string(),
                ));
            }
            Some("access_denied") => {
                return Err(CodexErr::InvalidRequest(
                    "GitHub device login was denied".to_string(),
                ));
            }
            Some(other) => {
                return Err(CodexErr::InvalidRequest(format!(
                    "GitHub device login failed with error: {other}"
                )));
            }
            None => {
                return Err(CodexErr::InvalidRequest(
                    "GitHub device login returned neither an access token nor an error".to_string(),
                ));
            }
        }
    }
}

fn exchange_github_token(github_token: &str) -> crate::error::Result<CopilotTokenResponse> {
    exchange_github_token_with_endpoint(github_token, COPILOT_TOKEN_ENDPOINT)
}

fn exchange_github_token_with_endpoint(
    github_token: &str,
    endpoint: &str,
) -> crate::error::Result<CopilotTokenResponse> {
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::block_in_place(|| exchange_github_token_inner(github_token, endpoint))
    } else {
        exchange_github_token_inner(github_token, endpoint)
    }
}

fn exchange_github_token_inner(
    github_token: &str,
    endpoint: &str,
) -> crate::error::Result<CopilotTokenResponse> {
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(connection_failed)?;
    let response = client
        .get(endpoint)
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
            url: Some(endpoint.to_string()),
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

fn write_device_flow_instructions(verification_uri: &str, user_code: &str) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(
        format!(
            "\nGitHub Copilot login required.\nOpen this URL in your browser:\n  {verification_uri}\nEnter this one-time code:\n  {user_code}\n\n"
        )
        .as_bytes(),
    )?;
    stdout.flush()
}

fn missing_token_error() -> CodexErr {
    CodexErr::EnvVar(EnvVarError {
        var: CODEX_GH_COPILOT_TOKEN_ENV_VAR.to_string(),
        instructions: Some(
            "Set CODEX_GH_COPILOT_TOKEN or run Codex from an interactive terminal so it can complete device login."
                .to_string(),
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
    use pretty_assertions::assert_eq;
    use serial_test::serial;
    use std::io::Write;
    use tempfile::TempDir;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    #[test]
    #[serial]
    fn test_codex_env_var_takes_priority() {
        unsafe {
            std::env::set_var(CODEX_GH_COPILOT_TOKEN_ENV_VAR, "gho_codex_token_123");
            std::env::remove_var("XDG_CONFIG_HOME");
        }
        assert!(matches!(
            discover_github_token(),
            Some(GithubTokenSource::EnvVar(token)) if token == "gho_codex_token_123"
        ));
        unsafe {
            std::env::remove_var(CODEX_GH_COPILOT_TOKEN_ENV_VAR);
        }
    }

    #[test]
    #[serial]
    fn test_device_flow_cache_is_used_when_env_var_is_missing() {
        let tmp = TempDir::new().unwrap();
        let codex_dir = tmp.path().join("codex-copilot");
        std::fs::create_dir_all(&codex_dir).unwrap();
        let mut file = std::fs::File::create(codex_dir.join("token.json")).unwrap();
        file.write_all(br#"{"github_token":"gho_device_flow_token_123"}"#)
            .unwrap();

        unsafe {
            std::env::remove_var(CODEX_GH_COPILOT_TOKEN_ENV_VAR);
            std::env::set_var("XDG_CONFIG_HOME", tmp.path());
        }
        assert!(matches!(
            discover_github_token(),
            Some(GithubTokenSource::DeviceCache(token)) if token == "gho_device_flow_token_123"
        ));
        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn exchange_github_token_in_async_context_does_not_panic() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/copilot_internal/v2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "token": "copilot-session-token",
                "expires_at": 1234,
            })))
            .mount(&server)
            .await;

        let response = exchange_github_token_with_endpoint(
            "gho_test_token",
            &format!("{}/copilot_internal/v2/token", server.uri()),
        )
        .expect("token exchange should succeed");

        assert_eq!(
            response,
            CopilotTokenResponse {
                token: "copilot-session-token".to_string(),
                expires_at: 1234,
            }
        );
    }
}
