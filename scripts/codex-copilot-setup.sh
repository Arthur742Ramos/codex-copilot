#!/usr/bin/env bash
set -euo pipefail

DRY_RUN=0
FORCE_YES=0

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

info() { printf "%b\n" "${BLUE}ℹ${NC} $*"; }
success() { printf "%b\n" "${GREEN}✓${NC} $*"; }
warn() { printf "%b\n" "${YELLOW}⚠${NC} $*"; }
error() { printf "%b\n" "${RED}✗${NC} $*" >&2; }

usage() {
  cat <<'USAGE'
Usage: codex-copilot-setup.sh [OPTIONS]

Configure Codex to use GitHub Copilot.

Options:
  --dry-run     Show what would be changed without writing files
  -y, --yes     Assume yes for prompts (overwrite model)
  --help        Show this help message
USAGE
}

has_cmd() { command -v "$1" >/dev/null 2>&1; }

parse_json_field() {
  local file="$1"
  if [[ ! -f "$file" ]]; then
    return 1
  fi

  if has_cmd jq; then
    jq -r '."github.com".oauth_token // empty' "$file" 2>/dev/null || true
    return 0
  fi

  if has_cmd python3; then
    python3 - "$file" <<'PY' 2>/dev/null || true
import json
import sys

path = sys.argv[1]
try:
    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)
    print((data.get("github.com") or {}).get("oauth_token") or "")
except Exception:
    print("")
PY
    return 0
  fi

  return 1
}

parse_codex_token() {
  local file="$1"
  if [[ ! -f "$file" ]]; then
    return 1
  fi

  if has_cmd jq; then
    jq -r '.github_token // empty' "$file" 2>/dev/null || true
    return 0
  fi

  if has_cmd python3; then
    python3 - "$file" <<'PY' 2>/dev/null || true
import json
import sys

path = sys.argv[1]
try:
    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)
    print(data.get("github_token") or "")
except Exception:
    print("")
PY
    return 0
  fi

  return 1
}

extract_token() {
  local token=""

  if [[ -n "${CODEX_GH_COPILOT_TOKEN:-}" ]]; then
    token="$CODEX_GH_COPILOT_TOKEN"
    info "Using token from CODEX_GH_COPILOT_TOKEN"
    printf '%s' "$token"
    return 0
  fi

  token="$(parse_codex_token "$HOME/.config/codex-copilot/token.json")"
  if [[ -n "$token" ]]; then
    info "Using token from ~/.config/codex-copilot/token.json"
    printf '%s' "$token"
    return 0
  fi

  return 1
}

parse_token_validation() {
  local json="$1"
  if has_cmd jq; then
    local runtime_token expires_at
    runtime_token="$(printf '%s' "$json" | jq -r '.token // empty' 2>/dev/null || true)"
    expires_at="$(printf '%s' "$json" | jq -r '.expires_at // empty' 2>/dev/null || true)"
    [[ -n "$runtime_token" && -n "$expires_at" ]] && { printf '%s|%s' "$runtime_token" "$expires_at"; return 0; }
  fi

  if has_cmd python3; then
    python3 - <<'PY' "$json" 2>/dev/null
import json
import sys
raw = sys.argv[1]
try:
    data = json.loads(raw)
    token = data.get("token") or ""
    expires_at = data.get("expires_at") or ""
    if token and expires_at:
        print(f"{token}|{expires_at}")
except Exception:
    pass
PY
    return 0
  fi

  return 1
}

confirm() {
  local prompt="$1"
  if [[ "$FORCE_YES" -eq 1 ]]; then
    return 0
  fi
  if [[ ! -t 0 ]]; then
    warn "$prompt -> non-interactive shell; keeping current value"
    return 1
  fi
  read -r -p "$prompt [y/N]: " reply
  [[ "$reply" =~ ^[Yy]$ ]]
}

update_config() {
  local codex_home="${CODEX_HOME:-$HOME/.codex-copilot}"
  local config_file="$codex_home/config.toml"

  if [[ "$DRY_RUN" -eq 1 ]]; then
    info "[dry-run] Would ensure $config_file exists"
  else
    mkdir -p "$codex_home"
    touch "$config_file"
  fi

  local current_model=""
  if [[ -f "$config_file" ]]; then
    current_model="$(grep -E '^[[:space:]]*model[[:space:]]*=' "$config_file" | head -n1 | sed -E 's/^[^=]*=[[:space:]]*"?([^"#]+)"?.*$/\1/' || true)"
  fi

  local set_model=1
  if [[ -n "$current_model" && "$current_model" != "gpt-4.1" ]]; then
    if ! confirm "config.toml has model='$current_model'. Overwrite to 'gpt-4.1'?"; then
      set_model=0
    fi
  fi

  if [[ "$DRY_RUN" -eq 1 ]]; then
    [[ "$set_model" -eq 1 ]] && info "[dry-run] Would set model = \"gpt-4.1\""
    return 0
  fi

  if [[ "$set_model" -eq 1 ]]; then
    if grep -qE '^[[:space:]]*model[[:space:]]*=' "$config_file"; then
      python3 - "$config_file" <<'PY'
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
text, n = re.subn(r'(?m)^[ \t]*model[ \t]*=.*$', 'model = "gpt-4.1"', text, count=1)
if n:
    path.write_text(text, encoding="utf-8")
PY
    else
      { printf 'model = "gpt-4.1"\n'; cat "$config_file"; } >"$config_file.new" && mv "$config_file.new" "$config_file"
    fi
  fi

  success "Updated $config_file"
}

print_snippet() {
  cat <<'SNIPPET'
# Trigger Codex device login once, or set CODEX_GH_COPILOT_TOKEN explicitly
codex
SNIPPET
}

print_models() {
  local token="$1"
  info "Available models from GitHub Copilot:"
  local model_json
  model_json="$(curl -s -H "Authorization: Bearer $token" -H "x-github-api-version: 2025-05-01" https://api.githubcopilot.com/models || true)"
  if [[ -z "$model_json" ]]; then
    warn "No response from models endpoint"
    return 0
  fi

  if has_cmd jq; then
    printf '%s\n' "$model_json" | jq . 2>/dev/null || printf '%s\n' "$model_json"
    return 0
  fi
  if has_cmd python3; then
    python3 - <<'PY' "$model_json" 2>/dev/null || printf '%s\n' "$model_json"
import json
import sys
print(json.dumps(json.loads(sys.argv[1]), indent=2))
PY
    return 0
  fi

  printf '%s\n' "$model_json"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      ;;
    -y|--yes)
      FORCE_YES=1
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      error "Unknown option: $1"
      usage
      exit 1
      ;;
  esac
  shift
done

if ! has_cmd curl; then
  error "curl is required"
  exit 1
fi
if ! has_cmd python3 && ! has_cmd jq; then
  error "python3 or jq is required to parse JSON"
  exit 1
fi

info "Locating GitHub Copilot token"
if ! TOKEN="$(extract_token)" || [[ -z "$TOKEN" ]]; then
  error "Could not find a Codex GitHub token. Run codex once from an interactive terminal to complete device login, or set CODEX_GH_COPILOT_TOKEN explicitly."
  exit 1
fi
success "Found token"

info "Validating Copilot access"
VALIDATION_JSON="$(curl -s -H "Authorization: Bearer $TOKEN" https://api.github.com/copilot_internal/v2/token || true)"
PARSED="$(parse_token_validation "$VALIDATION_JSON" || true)"
if [[ -z "$PARSED" || "$PARSED" != *"|"* ]]; then
  error "Token validation failed. Response: $VALIDATION_JSON"
  error "This usually means your account does not have GitHub Copilot access."
  exit 1
fi
COPILOT_RUNTIME_TOKEN="${PARSED%%|*}"
COPILOT_EXPIRES_AT="${PARSED#*|}"
success "Copilot access valid (expires: $COPILOT_EXPIRES_AT)"

update_config

printf "\n${BOLD}Shell snippet to add to your profile:${NC}\n"
print_snippet

printf "\n"
if [[ "$DRY_RUN" -eq 1 ]]; then
  info "[dry-run] Would query https://api.githubcopilot.com/models"
else
  print_models "$TOKEN"
fi

success "Setup complete"
