# codex-copilot

Use [OpenAI Codex CLI](https://github.com/openai/codex) with your GitHub Copilot subscription — no OpenAI API key needed.

This fork defaults to the built-in `copilot` provider, so you do not need to
set `model_provider = "copilot"` in `~/.codex/config.toml`.

## Release install

Pre-built binaries are published by GitHub Actions for:

- macOS: Apple Silicon, Intel
- Linux: x86_64, ARM64
- Windows: x86_64

### macOS / Linux

```bash
curl -fsSL https://github.com/Arthur742Ramos/codex-copilot/releases/latest/download/install.sh | bash
```

Install a specific release:

```bash
curl -fsSL https://github.com/Arthur742Ramos/codex-copilot/releases/download/copilot-v0.2.3/install.sh | bash -s -- copilot-v0.2.3
```

### Windows PowerShell

```powershell
$tmp = Join-Path $env:TEMP "install-codex-copilot.ps1"
irm https://github.com/Arthur742Ramos/codex-copilot/releases/latest/download/install.ps1 -OutFile $tmp
& $tmp
```

Install a specific release:

```powershell
$tmp = Join-Path $env:TEMP "install-codex-copilot.ps1"
irm https://github.com/Arthur742Ramos/codex-copilot/releases/download/copilot-v0.2.3/install.ps1 -OutFile $tmp
& $tmp copilot-v0.2.3
```

The installers place the real binary here by default:

- macOS / Linux: `~/.local/codex-copilot/bin/codex`
- Windows: `%LOCALAPPDATA%\Programs\codex-copilot\bin\codex.exe`

If you prefer a small shim in `~/bin/codex`, the wrapper from this repo is
still available, but it is optional:

```bash
./scripts/install-codex-wrapper.sh
```

## Default auth behavior

The built-in binary handles GitHub Copilot auth in this order:

1. Existing `CODEX_GH_COPILOT_TOKEN`
2. `~/.config/codex-copilot/token.json`
3. Device flow

If `CODEX_GH_COPILOT_TOKEN` is unset and there is no saved Codex token yet,
Codex starts GitHub device flow automatically and stores the result in the
Codex-only token cache.

That makes the release binary the default experience for this fork. The
wrapper and proxy flows are fallbacks, not the primary auth path.

## Trigger a release

The release workflow supports both manual dispatch and pushed tags:

```bash
# Manual dispatch
gh workflow run fork-release.yml -f version=0.2.3

# Or create a release from a tag push
git tag -a copilot-v0.2.3 -m "codex-copilot 0.2.3"
git push origin copilot-v0.2.3
```

Manual dispatch also supports single-target builds with `target=<triple>`.

Available targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`,
`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`x86_64-pc-windows-msvc`, `all` (default).

## Why this fork works

GitHub Copilot supports the Responses API at
`https://api.githubcopilot.com/responses`, which is the same wire protocol
Codex already uses.

The missing piece is authentication: the Copilot backend rejects unrecognized
clients, so a raw `gh auth token` is not enough for `/responses`. This fork
implements the Copilot token exchange and device login flow directly in the
Codex binary.

Models with `/responses` support include `gpt-5.2-codex`, `gpt-5.1-codex`,
`gpt-5.1-codex-max`, `gpt-5.1`, `gpt-5-mini`, and `gpt-5.2`.

## Proxy fallback

If you want the old proxy workflow for debugging or experimentation, the repo
still includes the setup helper:

```bash
./scripts/codex-copilot-setup.sh
```

## Fork internals

For the provider implementation details, see [`docs/fork-guide/`](docs/fork-guide/):

- **[README.md](docs/fork-guide/README.md)** — Step-by-step fork & build guide
- **[model_provider_info.patch](docs/fork-guide/model_provider_info.patch)** — Rust code to add `create_copilot_provider()`
- **[copilot_auth.rs](docs/fork-guide/copilot_auth.rs)** — Token discovery module (env var → saved Codex device token)
- **[integration_notes.md](docs/fork-guide/integration_notes.md)** — API details, rate limiting, premium request tracking, enterprise support

## Available Models

Query what's available on your subscription:

```bash
curl -s https://api.githubcopilot.com/models \
  -H "Authorization: Bearer $(gh auth token)" \
  -H "x-github-api-version: 2025-05-01" \
  | jq '.[].id'
```

Common models: `gpt-4.1`, `gpt-4o`, `o4-mini`, `o3`, `claude-sonnet-4`, `gemini-2.5-pro`

## How It Works

```
┌─────────────┐    Device login / token exchange    ┌──────────────────────────┐
│  Codex CLI   │ ─────────────────────────────────▶ │  api.githubcopilot.com   │
│              │                                    │                          │
│  built-in    │    Responses API + SSE streaming   │     Responses API        │
│  copilot     │ ◀────────────────────────────────▶ │   GPT-5 / GPT-4.1 / ...  │
│  provider    │                                    │                          │
└─────────────┘                                     └──────────────────────────┘
```

The installed Codex binary handles GitHub device login, token caching, Copilot
token exchange, and standard Responses API streaming directly. No local proxy
is required for the default path.

## Related

- [openai/codex#3609](https://github.com/openai/codex/issues/3609) — Feature request for built-in Copilot provider (51 👍)
- [ericc-ch/copilot-api](https://github.com/ericc-ch/copilot-api) — Alternative proxy approach (Chat Completions only)
- [Zed's Copilot implementation](https://github.com/zed-industries/zed/tree/main/crates/copilot_chat/src) — Reference for auth flow

## License

MIT
