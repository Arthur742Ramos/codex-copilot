# GitHub Copilot provider

This fork carries the latest upstream Codex tree and adds a built-in
`copilot` provider for GitHub Copilot.

## What this fork adds

- Built-in `copilot` provider registration in `codex-rs/core`
- GitHub token discovery from:
  1. `CODEX_GH_COPILOT_TOKEN`
  2. `~/.config/codex-copilot/token.json`
- Exchange of the GitHub OAuth token for a short-lived Copilot session token
  before requests are sent to `api.githubcopilot.com`

## Basic usage

Copilot is the built-in default provider in this fork, so you only need to
set a model if you want to pin one:

```toml
model = "gpt-5.2-codex"
```

Then run Codex as usual:

```bash
codex
```

This fork keeps its Codex state isolated from upstream Codex by default:

- macOS / Linux: `~/.codex-copilot`
- Windows: `%USERPROFILE%\\.codex-copilot`

## Install a release build

### macOS / Linux

```bash
curl -fsSL https://github.com/Arthur742Ramos/codex-copilot/releases/latest/download/install.sh | bash
```

### Windows PowerShell

```powershell
$tmp = Join-Path $env:TEMP "install-codex-copilot.ps1"
irm https://github.com/Arthur742Ramos/codex-copilot/releases/latest/download/install.ps1 -OutFile $tmp
& $tmp
```

The installers place the real binary on disk and update your `PATH`:

- macOS / Linux: `~/.local/codex-copilot/bin`
- Windows: `%LOCALAPPDATA%\Programs\codex-copilot\bin`

If you prefer a small shim in `~/bin/codex`, the wrapper from this repo is
still available, but it is optional and preserves the fork's isolated
`CODEX_HOME`:

```bash
./scripts/install-codex-wrapper.sh
```

The built-in binary checks these sources in order:

1. `CODEX_GH_COPILOT_TOKEN`
2. `~/.config/codex-copilot/token.json`
3. Device flow

If `CODEX_GH_COPILOT_TOKEN` is not set and no saved Codex token exists yet,
Codex runs device flow itself and saves the result.

## CI release workflow

The fork release workflow can publish installable binaries for macOS, Linux,
and Windows directly from GitHub Actions.

Manual dispatch:

```bash
gh workflow run fork-release.yml -f version=0.2.3
```

Tag-driven release:

```bash
git tag -a copilot-v0.2.3 -m "codex-copilot 0.2.3"
git push origin copilot-v0.2.3
```

## Model availability

This fork does not hardcode a permanent model allowlist beyond the built-in
provider itself. Your available Copilot models depend on your subscription and
GitHub's current API surface.

To inspect currently available Copilot models:

```bash
curl -s https://api.githubcopilot.com/models \
  -H "Authorization: Bearer $(gh auth token)" \
  -H "X-GitHub-Api-Version: 2025-05-01"
```

GPT-5.4 is not guaranteed by this fork; use the models endpoint to verify
whether your account currently exposes it.

## Proxy fallback

If you want to debug the Copilot flow outside the built-in provider, this
repository also keeps a local proxy implementation in `proxy.py` plus the setup
helper at `scripts/codex-copilot-setup.sh`.

Those files are useful as a fallback or for experimentation, but the main goal
of this fork is first-class built-in Copilot support directly in Codex.
