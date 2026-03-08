#!/usr/bin/env bash
set -euo pipefail

TARGET_PATH="${CODEX_WRAPPER_PATH:-$HOME/bin/codex}"
TARGET_DIR="$(dirname "$TARGET_PATH")"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REAL_CODEX="${CODEX_REAL_BIN:-$HOME/.local/codex-copilot/bin/codex}"
PROXY_SCRIPT="$REPO_ROOT/proxy.py"
INSTALLER_SCRIPT="$REPO_ROOT/scripts/install-copilot-release.sh"

step() {
  printf '==> %s\n' "$1"
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 is required." >&2
    exit 1
  fi
}

require_command install
require_command python3

step "Installing Codex wrapper to $TARGET_PATH"
mkdir -p "$TARGET_DIR"

cat >"$TARGET_PATH" <<EOF
#!/usr/bin/env bash
set -euo pipefail

PROXY_SCRIPT="$PROXY_SCRIPT"
PROXY_DIR="$(dirname "$PROXY_SCRIPT")"
TOKEN_FILE="\$HOME/.config/codex-copilot/token.json"
INSTALLER_SCRIPT="$INSTALLER_SCRIPT"
REAL_CODEX="$REAL_CODEX"

read_json_token() {
    local token_file="\$1"
    local token_kind="\$2"

    [ -f "\$token_file" ] || return 1

    python3 - "\$token_file" "\$token_kind" <<'PY' 2>/dev/null
import json
import sys

path = sys.argv[1]
token_kind = sys.argv[2]

try:
    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)
except Exception:
    raise SystemExit(1)

token = ""
if token_kind == "device":
    token = data.get("github_token", "")
elif token_kind == "copilot-config":
    token = (data.get("github.com") or {}).get("oauth_token", "")

if token:
    print(token)
PY
}

# Prefer a codex-copilot-specific token env var and keep the legacy name as a fallback.
if [ -z "\${CODEX_GH_COPILOT_TOKEN:-}" ] && [ -n "\${GH_COPILOT_TOKEN:-}" ]; then
    CODEX_GH_COPILOT_TOKEN="\$GH_COPILOT_TOKEN"
    export CODEX_GH_COPILOT_TOKEN
fi

# Prefer reusable Copilot-specific auth sources first.
if [ -z "\${CODEX_GH_COPILOT_TOKEN:-}" ]; then
    for copilot_config in "\$HOME/.config/github-copilot/hosts.json" "\$HOME/.config/github-copilot/apps.json" "\$HOME/Library/Application Support/github-copilot/hosts.json" "\$HOME/Library/Application Support/github-copilot/apps.json"
    do
        CODEX_GH_COPILOT_TOKEN="\$(read_json_token "\$copilot_config" "copilot-config" || true)"
        if [ -n "\$CODEX_GH_COPILOT_TOKEN" ]; then
            export CODEX_GH_COPILOT_TOKEN
            break
        fi
    done
fi

if [ -z "\${CODEX_GH_COPILOT_TOKEN:-}" ]; then
    CODEX_GH_COPILOT_TOKEN="\$(read_json_token "\$TOKEN_FILE" "device" || true)"
    if [ -n "\$CODEX_GH_COPILOT_TOKEN" ]; then
        export CODEX_GH_COPILOT_TOKEN
    fi
fi

# Fall back to gh auth only when no Copilot token source is available.
if [ -z "\${CODEX_GH_COPILOT_TOKEN:-}" ] && command -v gh >/dev/null 2>&1; then
    CODEX_GH_COPILOT_TOKEN="\$(gh auth token 2>/dev/null || true)"
    if [ -n "\$CODEX_GH_COPILOT_TOKEN" ]; then
        export CODEX_GH_COPILOT_TOKEN
    fi
fi

if [ -z "\${CODEX_GH_COPILOT_TOKEN:-}" ]; then
    echo "No Copilot token found. Running device flow login..."
    python3 "\$PROXY_SCRIPT" --login-only 2>/dev/null || python3 -c "
import sys; sys.path.insert(0, '$(dirname "$PROXY_SCRIPT")')
from proxy import github_device_flow
github_device_flow()
" || {
        echo "Failed to authenticate. Run: python3 \$PROXY_SCRIPT" >&2
    }
    CODEX_GH_COPILOT_TOKEN="\$(read_json_token "\$TOKEN_FILE" "device" || true)"
    if [ -n "\$CODEX_GH_COPILOT_TOKEN" ]; then
        export CODEX_GH_COPILOT_TOKEN
    fi
fi

if [ ! -x "\$REAL_CODEX" ] && [ -x "\$INSTALLER_SCRIPT" ]; then
    echo "Installed Codex binary not found. Downloading latest release..."
    "\$INSTALLER_SCRIPT" latest
fi

if [ ! -x "\$REAL_CODEX" ]; then
    echo "Codex binary not found at \$REAL_CODEX" >&2
    echo "Run: \$INSTALLER_SCRIPT latest" >&2
    exit 1
fi

exec "\$REAL_CODEX" "\$@"
EOF

chmod 755 "$TARGET_PATH"
printf 'Installed wrapper to %s\n' "$TARGET_PATH"
