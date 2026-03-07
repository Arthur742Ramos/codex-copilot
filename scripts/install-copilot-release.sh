#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:-latest}"
INSTALL_DIR="${CODEX_INSTALL_DIR:-$HOME/.local/codex-copilot/bin}"

step() {
  printf '==> %s\n' "$1"
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 is required." >&2
    exit 1
  fi
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

resolve_repo() {
  if [[ -n "${CODEX_RELEASE_REPO:-}" ]]; then
    printf '%s\n' "$CODEX_RELEASE_REPO"
    return
  fi

  local remote
  remote="$(git config --get remote.origin.url 2>/dev/null || true)"
  if [[ "$remote" =~ github\.com[:/]([^/]+/[^/.]+)(\.git)?$ ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
    return
  fi

  echo "Set CODEX_RELEASE_REPO=owner/repo or run from a git checkout with an origin remote." >&2
  exit 1
}

resolve_latest_tag() {
  local repo="$1"
  local tag
  tag="$(gh release list --repo "$repo" --limit 100 | awk '$1 ~ /^copilot-v/ { print $1; exit }')"
  if [[ -z "$tag" ]]; then
    echo "No copilot-v release found in $repo" >&2
    exit 1
  fi
  printf '%s\n' "$tag"
}

resolve_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin)
      if [[ "$arch" == "x86_64" ]] && [[ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" == "1" ]]; then
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
      echo "Unsupported OS: $os" >&2
      exit 1
      ;;
  esac
}

require_command gh
require_command tar
require_command install
require_command mktemp

repo="$(resolve_repo)"
if [[ "$VERSION" == "latest" ]]; then
  tag="$(resolve_latest_tag "$repo")"
else
  tag="$(normalize_version "$VERSION")"
fi

target="$(resolve_target)"
asset="codex-${target}.tar.gz"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

step "Repo: $repo"
step "Release: $tag"
step "Target: $target"
step "Downloading $asset"

gh release download "$tag" --repo "$repo" -p "$asset" -D "$tmp_dir"

step "Installing to $INSTALL_DIR"
mkdir -p "$INSTALL_DIR"
tar -xzf "$tmp_dir/$asset" -C "$tmp_dir"
install -m 755 "$tmp_dir/codex" "$INSTALL_DIR/codex"

printf 'Installed %s to %s/codex\n' "$tag" "$INSTALL_DIR"
