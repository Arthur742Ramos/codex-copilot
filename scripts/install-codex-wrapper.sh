#!/usr/bin/env bash
set -euo pipefail

TARGET_PATH="${CODEX_WRAPPER_PATH:-$HOME/bin/codex}"
TARGET_DIR="$(dirname "$TARGET_PATH")"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REAL_CODEX="${CODEX_REAL_BIN:-$HOME/.local/codex-copilot/bin/codex}"
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

step "Installing Codex wrapper to $TARGET_PATH"
mkdir -p "$TARGET_DIR"

cat >"$TARGET_PATH" <<EOF
#!/usr/bin/env bash
set -euo pipefail

INSTALLER_SCRIPT="$INSTALLER_SCRIPT"
REAL_CODEX="$REAL_CODEX"
DEFAULT_CODEX_HOME="\${CODEX_HOME:-\$HOME/.codex-copilot}"

if [ ! -x "\$REAL_CODEX" ] && [ -x "\$INSTALLER_SCRIPT" ]; then
    echo "Installed Codex binary not found. Downloading latest release..."
    "\$INSTALLER_SCRIPT" latest
fi

if [ ! -x "\$REAL_CODEX" ]; then
    echo "Codex binary not found at \$REAL_CODEX" >&2
    echo "Run: \$INSTALLER_SCRIPT latest" >&2
    exit 1
fi

exec env CODEX_HOME="\$DEFAULT_CODEX_HOME" "\$REAL_CODEX" "\$@"
EOF

chmod 755 "$TARGET_PATH"
printf 'Installed wrapper to %s\n' "$TARGET_PATH"
