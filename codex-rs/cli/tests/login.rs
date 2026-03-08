use std::path::Path;

use anyhow::Result;
use predicates::str::contains;
use tempfile::TempDir;

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("codex")?);
    cmd.env("CODEX_HOME", codex_home);
    Ok(cmd)
}

#[test]
fn login_status_uses_copilot_env_var_for_default_provider() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("CODEX_GH_COPILOT_TOKEN", "gho_test_token_123")
        .args(["login", "status"])
        .assert()
        .success()
        .stderr(contains(
            "Logged in using GitHub Copilot via CODEX_GH_COPILOT_TOKEN",
        ));

    Ok(())
}

#[test]
fn logout_removes_saved_copilot_device_token() -> Result<()> {
    let codex_home = TempDir::new()?;
    let xdg_config_home = TempDir::new()?;
    let token_dir = xdg_config_home.path().join("codex-copilot");
    std::fs::create_dir_all(&token_dir)?;
    let token_path = token_dir.join("token.json");
    std::fs::write(&token_path, r#"{"github_token":"gho_cached_token_123"}"#)?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("XDG_CONFIG_HOME", xdg_config_home.path())
        .args(["logout"])
        .assert()
        .success()
        .stderr(contains("Successfully logged out"));

    assert!(
        !token_path.exists(),
        "saved copilot token should be removed"
    );

    Ok(())
}

#[test]
fn logout_reports_when_copilot_env_var_still_overrides() -> Result<()> {
    let codex_home = TempDir::new()?;

    let mut cmd = codex_command(codex_home.path())?;
    cmd.env("CODEX_GH_COPILOT_TOKEN", "gho_test_token_123")
        .args(["logout"])
        .assert()
        .success()
        .stderr(contains(
            "CODEX_GH_COPILOT_TOKEN is still set in the current shell. Unset it to fully log out.",
        ));

    Ok(())
}
