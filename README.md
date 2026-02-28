# codex-copilot

Use [OpenAI Codex CLI](https://github.com/openai/codex) with your GitHub Copilot subscription — no OpenAI API key needed.

## Key Discovery

GitHub Copilot **already supports the Responses API** at `https://api.githubcopilot.com/responses` — the same wire protocol Codex uses. No translation layer needed. Just point Codex at Copilot's endpoint with your GitHub token.

## Quick Start (No Fork Required)

The fastest way — works with stock Codex CLI today:

```bash
# 1. Run the setup script
./scripts/codex-copilot-setup.sh

# 2. Add the token export to your shell profile (~/.bashrc, ~/.zshrc, etc.)
export GH_COPILOT_TOKEN=$(cat ~/.config/github-copilot/hosts.json 2>/dev/null \
  | python3 -c "import sys,json; print(json.load(sys.stdin).get('github.com',{}).get('oauth_token',''))" 2>/dev/null)

# 3. Use Codex as normal
codex "explain this codebase"
```

### What the setup script does

1. **Finds your GitHub token** from `~/.config/github-copilot/hosts.json`, `apps.json`, `gh auth token`, or `$GH_COPILOT_TOKEN`
2. **Validates Copilot access** via the token exchange endpoint
3. **Generates `~/.codex/config.toml`** with the Copilot provider:

```toml
model = "gpt-4.1"
model_provider = "copilot"

[model_providers.copilot]
name = "GitHub Copilot"
base_url = "https://api.githubcopilot.com"
env_key = "GH_COPILOT_TOKEN"
wire_api = "responses"
http_headers = { "Editor-Version" = "codex-cli/1.0", "Copilot-Integration-Id" = "codex-cli" }
```

4. **Lists available models** from the Copilot API

### Manual config (even quicker)

If you just want to configure it yourself:

```bash
# Get your token
export GH_COPILOT_TOKEN=$(gh auth token)

# Add to ~/.codex/config.toml
cat >> ~/.codex/config.toml << 'EOF'
model_provider = "copilot"

[model_providers.copilot]
name = "GitHub Copilot"
base_url = "https://api.githubcopilot.com"
env_key = "GH_COPILOT_TOKEN"
wire_api = "responses"
http_headers = { "Editor-Version" = "codex-cli/1.0", "Copilot-Integration-Id" = "codex-cli" }
EOF

# Run codex
codex "hello world"
```

## Fork Approach (Built-in Provider)

For a first-class experience with automatic token discovery, you can fork openai/codex and add a built-in `copilot` provider. See [`docs/fork-guide/`](docs/fork-guide/) for:

- **[README.md](docs/fork-guide/README.md)** — Step-by-step fork & build guide
- **[model_provider_info.patch](docs/fork-guide/model_provider_info.patch)** — Rust code to add `create_copilot_provider()`
- **[copilot_auth.rs](docs/fork-guide/copilot_auth.rs)** — Token discovery module (env var → hosts.json → apps.json → gh CLI)
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
┌─────────────┐     Responses API      ┌──────────────────────────┐
│  Codex CLI   │ ────────────────────▶  │  api.githubcopilot.com   │
│              │  POST /responses       │     (Responses API)      │
│  GH_COPILOT  │  Bearer: gho_xxx      │                          │
│  _TOKEN      │ ◀────────────────────  │  Routes to: GPT, Claude, │
│              │  SSE streaming         │  Gemini, etc.            │
└─────────────┘                        └──────────────────────────┘
```

GitHub Copilot's API proxies to the same underlying models (GPT-4.1, Claude, Gemini, etc.) and speaks the exact same Responses API wire format as `api.openai.com/v1/responses`. Codex CLI doesn't know the difference.

## Related

- [openai/codex#3609](https://github.com/openai/codex/issues/3609) — Feature request for built-in Copilot provider (51 👍)
- [ericc-ch/copilot-api](https://github.com/ericc-ch/copilot-api) — Alternative proxy approach (Chat Completions only)
- [Zed's Copilot implementation](https://github.com/zed-industries/zed/tree/main/crates/copilot_chat/src) — Reference for auth flow

## License

MIT
