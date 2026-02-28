# codex-copilot

Use [OpenAI Codex CLI](https://github.com/openai/codex) with your GitHub Copilot subscription — no OpenAI API key needed.

## Key Discovery

GitHub Copilot **supports the Responses API** at `https://api.githubcopilot.com/responses` — the same wire protocol Codex uses. Models with `/responses` support: `gpt-5.2-codex`, `gpt-5.1-codex`, `gpt-5.1-codex-max`, `gpt-5.1`, `gpt-5-mini`, `gpt-5.2`.

## ⚠️ Important: Client Identity Enforcement

The Copilot API **rejects requests from unrecognized clients**. A raw `gh auth token` (gho\_) can query `/models` but gets **403 Forbidden** on `/responses` and `/chat/completions`. The API enforces a client handshake that only recognized Copilot integrations (VS Code, JetBrains, the `copilot` CLI) can complete.

This means **a simple config-only approach doesn't work** — you need either a proxy that handles the handshake, or a Codex fork that implements the full Copilot auth dance.

## Quick Start: Proxy Approach (Recommended)

Use [ericc-ch/copilot-api](https://github.com/ericc-ch/copilot-api) (2.6k ⭐) as a local proxy that handles the Copilot client handshake:

```bash
# 1. Install and start the proxy
npx copilot-api

# 2. Configure Codex CLI to use it (proxy runs on port 4141)
cat >> ~/.codex/config.toml << 'EOF'
model = "gpt-5.1-codex"
model_provider = "copilot"

[model_providers.copilot]
name = "GitHub Copilot (via proxy)"
base_url = "http://127.0.0.1:4141"
wire_api = "responses"
EOF

# 3. Use Codex as normal
codex "explain this codebase"
```

> **Note:** Only models with `/responses` support work with Codex: `gpt-5.2-codex`, `gpt-5.1-codex`, `gpt-5.1-codex-max`, `gpt-5.1`, `gpt-5-mini`, `gpt-5.2`. Claude and Gemini models only support `/chat/completions` on Copilot.

### Setup script

The included setup script validates your Copilot access and generates the config:

```bash
./scripts/codex-copilot-setup.sh
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
┌─────────────┐                      ┌──────────────┐     Copilot Auth      ┌──────────────────────────┐
│  Codex CLI   │   Responses API     │  copilot-api  │  ──────────────────▶  │  api.githubcopilot.com   │
│              │ ────────────────▶   │  (proxy)      │   POST /responses    │     (Responses API)      │
│  localhost   │  POST /responses    │  port 4141    │ ◀──────────────────   │                          │
│  :4141       │ ◀────────────────   │  handles auth │   SSE streaming      │  Routes to: GPT-5.x,     │
│              │  SSE streaming      │  + handshake  │                      │  GPT-4o, etc.            │
└─────────────┘                      └──────────────┘                      └──────────────────────────┘
```

The proxy handles the Copilot client handshake (client identity, token exchange) that raw HTTP calls can't pass. Codex CLI sees a standard Responses API endpoint.

## Related

- [openai/codex#3609](https://github.com/openai/codex/issues/3609) — Feature request for built-in Copilot provider (51 👍)
- [ericc-ch/copilot-api](https://github.com/ericc-ch/copilot-api) — Alternative proxy approach (Chat Completions only)
- [Zed's Copilot implementation](https://github.com/zed-industries/zed/tree/main/crates/copilot_chat/src) — Reference for auth flow

## License

MIT
