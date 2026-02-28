# Adding a GitHub Copilot Provider to openai/codex

A step-by-step guide for forking the [openai/codex](https://github.com/openai/codex) CLI
and adding a built-in `copilot` provider that talks to
`https://api.githubcopilot.com/responses`.

## Why This Works

GitHub Copilot's backend already supports the **OpenAI Responses API** wire
format — the same format Codex uses internally. No request/response translation
is needed; we just point at a different base URL and supply a GitHub OAuth token
instead of an OpenAI API key.

---

## 1. Fork & Clone

```bash
gh repo fork openai/codex --clone
cd codex
```

## 2. Files to Modify / Add

| File | Action |
|------|--------|
| `codex-rs/core/src/model_provider_info.rs` | Add `copilot` to `built_in_model_providers()` |
| `codex-rs/core/src/copilot_auth.rs` | **New** — token discovery module |
| `codex-rs/core/src/lib.rs` | Add `pub mod copilot_auth;` |
| `codex-rs/core/Cargo.toml` | Add `dirs` and `serde_json` deps (if not already present) |

### 2a. Apply the provider patch

See [`model_provider_info.patch`](./model_provider_info.patch) for the exact
changes to `model_provider_info.rs`. The key addition is a `create_copilot_provider()`
function and a new entry in the `built_in_model_providers()` list.

### 2b. Add the auth module

Copy [`copilot_auth.rs`](./copilot_auth.rs) into `codex-rs/core/src/`.

Then add the module declaration in `codex-rs/core/src/lib.rs`:

```rust
pub mod copilot_auth;
```

### 2c. Add dependencies

In `codex-rs/core/Cargo.toml`, ensure you have:

```toml
[dependencies]
dirs = "6"
serde_json = "1"
```

These are likely already present; check before adding duplicates.

## 3. Build

```bash
cd codex-rs
cargo build --release
```

The binary lands in `target/release/codex`.

## 4. Configure Your Token

The provider needs a GitHub OAuth token. Options (in priority order):

### Option A: Environment variable

```bash
export GH_COPILOT_TOKEN=$(gh auth token)
```

### Option B: Automatic discovery

The auth module automatically checks:

1. `GH_COPILOT_TOKEN` env var
2. `~/.config/github-copilot/hosts.json` (VS Code / JetBrains)
3. `~/.config/github-copilot/apps.json`
4. Output of `gh auth token`

If you have GitHub Copilot active in any editor, a token is probably already
present in `hosts.json`.

## 5. Use It

```bash
# With the copilot provider
codex --provider copilot --model gpt-4.1 "explain this codebase"

# Or set defaults in your config
cat >> ~/.codex/config.toml << 'EOF'
provider = "copilot"
model = "gpt-4.1"
EOF

# Then just:
codex "explain this codebase"
```

### Available models

Query available models:

```bash
curl -s https://api.githubcopilot.com/models \
  -H "Authorization: Bearer $(gh auth token)" \
  -H "x-github-api-version: 2025-05-01" \
  | jq '.[].id'
```

Common models: `gpt-4.1`, `gpt-4o`, `claude-sonnet-4`, `o4-mini`, `gemini-2.5-pro`.

## 6. Verify

```bash
# Quick smoke test
codex --provider copilot --model gpt-4.1 "say hello"

# Check that streaming works
codex --provider copilot --model gpt-4.1 "count to 10 slowly"
```

## Technical Notes

See [`integration_notes.md`](./integration_notes.md) for details on rate
limiting, premium request tracking, enterprise considerations, and the
Copilot API surface.
