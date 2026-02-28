# Copilot API Integration Notes for Codex

Technical reference for integrating GitHub Copilot as a provider in the
openai/codex CLI.

## Endpoint

```
POST https://api.githubcopilot.com/responses
```

This is the **Responses API** endpoint — the same wire format that
`api.openai.com/v1/responses` uses. Codex already speaks this protocol via
`WireApi::Responses`, so no request/response translation is needed.

## Authentication

### Token Types

| Prefix | Source | Notes |
|--------|--------|-------|
| `gho_` | Device OAuth flow | Written by Copilot extensions to `hosts.json` |
| `ghp_` | Personal Access Token | Created in GitHub Settings → Developer settings |
| (no prefix) | `gh auth token` | May be a `gho_` or `ghp_` token |

### Token Discovery Priority

1. `GH_COPILOT_TOKEN` environment variable
2. `~/.config/github-copilot/hosts.json` → `github.com.oauth_token`
3. `~/.config/github-copilot/apps.json` → `github.com.oauth_token`
4. `gh auth token` command output

### Authorization Header

```
Authorization: Bearer <oauth_token>
```

The GitHub OAuth token is sent directly. The Copilot backend exchanges it
internally for a short-lived session token — callers don't need to do the
`/copilot_internal/v2/token` exchange themselves for API requests.

## Required Headers

```http
Authorization: Bearer <token>
Editor-Version: codex/<version>
Copilot-Integration-Id: codex-cli
```

Without `Editor-Version` and `Copilot-Integration-Id`, requests may be
rejected or mis-categorized.

## Available Models

Query the models endpoint:

```bash
curl -s https://api.githubcopilot.com/models \
  -H "Authorization: Bearer $(gh auth token)" \
  -H "x-github-api-version: 2025-05-01" \
  | jq '.[] | {id, name}'
```

Common models as of mid-2025:

| Model ID | Notes |
|----------|-------|
| `gpt-4.1` | Fast, strong coding model |
| `gpt-4o` | Multimodal, good all-around |
| `o4-mini` | Reasoning model, fast |
| `o3` | Reasoning model, strong |
| `claude-sonnet-4` | Anthropic via Copilot |
| `gemini-2.5-pro` | Google via Copilot |

## Rate Limiting

### Headers

Every response includes rate limit headers:

```http
x-ratelimit-limit: 100
x-ratelimit-remaining: 97
x-ratelimit-reset: 1719500000
```

- `x-ratelimit-limit` — max requests in the current window
- `x-ratelimit-remaining` — requests left in the current window
- `x-ratelimit-reset` — Unix timestamp when the window resets

### Retry-After

When rate limited (HTTP 429), the response includes:

```http
Retry-After: 60
```

Codex's built-in retry logic (exponential backoff) handles this, but you can
also respect the `Retry-After` header for more precise backoff.

## Premium Request Tracking

### The `X-Initiator` Header

This header controls how requests are counted against your Copilot premium
request quota:

| Value | Meaning | Premium? |
|-------|---------|----------|
| `X-Initiator: user` | User explicitly initiated this request | Yes — counts as premium |
| `X-Initiator: agent` | Agent/tool-initiated (background, automated) | Depends on model tier |

### How to Minimize Premium Request Usage

For agentic loops where the model calls tools and iterates:

- **First turn** (user types a prompt): `X-Initiator: user`
- **Subsequent turns** (tool results fed back): `X-Initiator: agent`

This matters because premium requests are limited per billing cycle. The
`agent` initiator may use a lower-cost model tier or not count against the
premium quota, depending on the plan.

### Implementation in Codex

You could add this to the HTTP headers in `create_copilot_provider()`:

```rust
// In the request-building code, not the static provider config:
if is_first_turn {
    headers.insert("X-Initiator", "user");
} else {
    headers.insert("X-Initiator", "agent");
}
```

This requires a small change to Codex's request path to thread through
whether the current request is user-initiated or agent-initiated.

## Streaming

The Copilot API uses **Server-Sent Events (SSE)** for streaming, identical to
OpenAI's streaming format:

```
data: {"type":"response.output_item.added",...}
data: {"type":"response.content_part.delta","delta":{"type":"text_delta","text":"Hello"}}
data: {"type":"response.completed",...}
data: [DONE]
```

Codex's SSE parser works out of the box with Copilot's streaming responses.

## Enterprise Considerations

### Custom Endpoints

GitHub Enterprise Cloud customers may have a different Copilot API endpoint.
The endpoint is discovered via GraphQL:

```graphql
query {
  viewer {
    copilotEndpoints {
      api
    }
  }
}
```

For enterprise support, you'd want to:

1. Allow `base_url` override in config (Codex already supports this)
2. Optionally auto-discover via the GraphQL endpoint

### Proxy Support

Enterprise environments often use HTTP proxies. Codex should respect:

- `HTTPS_PROXY` / `https_proxy` environment variables
- System proxy settings

The `reqwest` HTTP client used by Codex respects these by default.

## Error Responses

The Copilot API returns standard OpenAI-format errors:

```json
{
  "error": {
    "message": "Rate limit exceeded",
    "type": "rate_limit_error",
    "code": "rate_limit_exceeded"
  }
}
```

| Status | Meaning | Action |
|--------|---------|--------|
| 401 | Invalid/expired token | Re-authenticate, refresh token |
| 403 | No Copilot subscription | Check subscription status |
| 429 | Rate limited | Retry after `Retry-After` seconds |
| 500 | Server error | Retry with backoff |

## Request Format

Standard Responses API request body:

```json
{
  "model": "gpt-4.1",
  "input": [
    {
      "role": "user",
      "content": "Explain this code"
    }
  ],
  "stream": true,
  "tools": [...],
  "instructions": "You are a coding assistant."
}
```

No modifications needed from the standard Codex request format.

## Testing Connectivity

Quick test to verify everything works:

```bash
TOKEN=$(gh auth token)

# Test auth
curl -s -o /dev/null -w "%{http_code}" \
  https://api.github.com/copilot_internal/v2/token \
  -H "Authorization: Bearer $TOKEN"
# Should return 200

# Test models endpoint
curl -s https://api.githubcopilot.com/models \
  -H "Authorization: Bearer $TOKEN" \
  -H "x-github-api-version: 2025-05-01" \
  | jq length
# Should return a number > 0

# Test responses endpoint (non-streaming)
curl -s https://api.githubcopilot.com/responses \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"model":"gpt-4.1","input":[{"role":"user","content":"Say hi"}]}'
# Should return a response object
```
