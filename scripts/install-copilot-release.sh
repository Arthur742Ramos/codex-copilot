#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-latest}"
INSTALL_DIR="${CODEX_INSTALL_DIR:-$HOME/.local/codex-copilot/bin}"
REPO="${CODEX_RELEASE_REPO:-Arthur742Ramos/codex-copilot}"
path_action="already"
path_profile=""

step() {
  printf '==> %s\n' "$1"
}

normalize_version() {
  case "$1" in
    ''|latest)
      printf 'latest\n'
      ;;
    copilot-v*)
      printf '%s\n' "$1"
      ;;
    v*)
      printf 'copilot-%s\n' "$1"
      ;;
    *)
      printf 'copilot-v%s\n' "$1"
      ;;
  esac
}

download_file() {
  local url="$1"
  local output="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$output"
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -q -O "$output" "$url"
    return
  fi

  echo "curl or wget is required to install codex-copilot." >&2
  exit 1
}

download_text() {
  local url="$1"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url"
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -q -O - "$url"
    return
  fi

  echo "curl or wget is required to install codex-copilot." >&2
  exit 1
}

add_to_path() {
  path_action="already"
  path_profile=""

  case ":$PATH:" in
    *":$INSTALL_DIR:"*)
      return
      ;;
  esac

  local profile="$HOME/.profile"
  case "${SHELL:-}" in
    */zsh)
      profile="$HOME/.zshrc"
      ;;
    */bash)
      profile="$HOME/.bashrc"
      ;;
  esac

  path_profile="$profile"
  local path_line="export PATH=\"$INSTALL_DIR:\$PATH\""
  if [ -f "$profile" ] && grep -F "$path_line" "$profile" >/dev/null 2>&1; then
    path_action="configured"
    return
  fi

  {
    printf '\n# Added by codex-copilot installer\n'
    printf '%s\n' "$path_line"
  } >>"$profile"
  path_action="added"
}

release_url_for_asset() {
  local asset="$1"
  local resolved_version="$2"

  printf 'https://github.com/%s/releases/download/%s/%s\n' "$REPO" "$resolved_version" "$asset"
}

resolve_version() {
  local normalized_version
  normalized_version="$(normalize_version "$VERSION")"

  if [ "$normalized_version" != "latest" ]; then
    printf '%s\n' "$normalized_version"
    return
  fi

  local release_json resolved
  release_json="$(download_text "https://api.github.com/repos/${REPO}/releases/latest")"
  resolved="$(printf '%s\n' "$release_json" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"

  if [ -z "$resolved" ]; then
    echo "Failed to resolve the latest codex-copilot release version from ${REPO}." >&2
    exit 1
  fi

  printf '%s\n' "$resolved"
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 is required to install codex-copilot." >&2
    exit 1
  fi
}

require_command mktemp
require_command tar
require_command install

resolve_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin)
      if [ "$arch" = "x86_64" ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = "1" ]; then
        arch="arm64"
      fi
      case "$arch" in
        arm64|aarch64)
          printf 'aarch64-apple-darwin\n'
          ;;
        x86_64|amd64)
          printf 'x86_64-apple-darwin\n'
          ;;
        *)
          echo "Unsupported macOS architecture: $arch" >&2
          exit 1
          ;;
      esac
      ;;
    Linux)
      case "$arch" in
        x86_64|amd64)
          printf 'x86_64-unknown-linux-gnu\n'
          ;;
        aarch64|arm64)
          printf 'aarch64-unknown-linux-gnu\n'
          ;;
        *)
          echo "Unsupported Linux architecture: $arch" >&2
          exit 1
          ;;
      esac
      ;;
    *)
      echo "install-copilot-release.sh supports macOS and Linux. Use install-copilot-release.ps1 on Windows." >&2
      exit 1
      ;;
  esac
}

if [ -x "$INSTALL_DIR/codex" ]; then
  install_mode="Updating"
else
  install_mode="Installing"
fi

target="$(resolve_target)"
asset="codex-${target}.tar.gz"
resolved_version="$(resolve_version)"
download_url="$(release_url_for_asset "$asset" "$resolved_version")"

step "$install_mode codex-copilot"
step "Repo: $REPO"
step "Release: $resolved_version"
step "Target: $target"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

archive_path="$tmp_dir/$asset"

step "Downloading $asset"
download_file "$download_url" "$archive_path"

step "Installing to $INSTALL_DIR"
mkdir -p "$INSTALL_DIR"
tar -xzf "$archive_path" -C "$tmp_dir"
install -m 755 "$tmp_dir/codex" "$INSTALL_DIR/codex"

add_to_path

case "$path_action" in
  added)
    step "PATH updated for future shells in $path_profile"
    step "Run now: export PATH=\"$INSTALL_DIR:\$PATH\" && codex"
    step "Or open a new terminal and run: codex"
    ;;
  configured)
    step "PATH is already configured for future shells in $path_profile"
    step "Run now: export PATH=\"$INSTALL_DIR:\$PATH\" && codex"
    step "Or open a new terminal and run: codex"
    ;;
  *)
    step "$INSTALL_DIR is already on PATH"
    step "Run: codex"
    ;;
esac

printf 'codex-copilot %s installed successfully.\n' "$resolved_version"
