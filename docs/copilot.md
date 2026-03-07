# GitHub Copilot provider

This fork carries the latest upstream Codex tree and adds a built-in
`copilot` provider for GitHub Copilot.

## What this fork adds

- Built-in `copilot` provider registration in `codex-rs/core`
- GitHub token discovery from:
  1. `GH_COPILOT_TOKEN`
  2. `~/.config/github-copilot/hosts.json`
  3. `~/.config/github-copilot/apps.json`
  4. `gh auth token`
- Exchange of the GitHub OAuth token for a short-lived Copilot session token
  before requests are sent to `api.githubcopilot.com`

## Basic usage

Set your provider and model in `~/.codex/config.toml`:

```toml
model_provider = "copilot"
model = "gpt-5.2-codex"
```

Then run Codex as usual:

```bash
codex
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
