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

Copilot is the built-in default provider in this fork, so you only need to
set a model if you want to pin one:

```toml
model = "gpt-5.2-codex"
```

Then run Codex as usual:

```bash
codex
```

If you install a release build, install the wrapper from this repo so `codex`
keeps reusing your GitHub auth before falling back to device flow:

```bash
./scripts/install-copilot-release.sh latest
./scripts/install-codex-wrapper.sh
```

The wrapper checks these sources in order:

1. `GH_COPILOT_TOKEN`
2. `gh auth token`
3. `~/.config/github-copilot/hosts.json`
4. `~/.config/github-copilot/apps.json`
5. `~/.config/codex-copilot/token.json`

If none of those are available, it runs device flow.

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
