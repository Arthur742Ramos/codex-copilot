#!/usr/bin/env python3
"""Minimal proxy that bridges Codex CLI to GitHub Copilot's Responses API.

Handles the full Copilot auth flow:
1. GitHub OAuth device flow (using Copilot's client ID)
2. Token exchange (GitHub token → Copilot session token)
3. Auto-refresh before expiry
4. Proxies /responses requests to api.githubcopilot.com

Usage:
    python3 proxy.py                     # interactive device flow login
    python3 proxy.py --port 4141         # custom port
"""

from __future__ import annotations

import argparse
import http.client
import json
import logging
import os
import ssl
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler
from pathlib import Path
from socketserver import ThreadingMixIn
from http.server import HTTPServer
from urllib.request import Request, urlopen
from urllib.error import HTTPError as UrllibHTTPError

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
log = logging.getLogger("copilot-proxy")

COPILOT_API = "https://api.githubcopilot.com"
GITHUB_API = "https://api.github.com"
GITHUB_BASE = "https://github.com"
EDITOR_VERSION = "vscode/1.96.0"
EDITOR_PLUGIN_VERSION = "copilot-chat/0.26.7"
USER_AGENT = "GitHubCopilotChat/0.26.7"
API_VERSION = "2025-04-01"

# This is the Copilot Chat OAuth app client ID (same as ericc-ch/copilot-api)
GITHUB_CLIENT_ID = "Iv1.b507a08c87ecfe98"
TOKEN_FILE = Path.home() / ".config" / "codex-copilot" / "token.json"


class TokenManager:
    """Manages GitHub → Copilot token exchange and auto-refresh."""

    def __init__(self, github_token: str):
        self.github_token = github_token
        self.copilot_token: str | None = None
        self.expires_at: float = 0
        self._lock = threading.Lock()

    def _github_headers(self) -> dict[str, str]:
        return {
            "Authorization": f"token {self.github_token}",
            "Content-Type": "application/json",
            "Accept": "application/json",
            "Editor-Version": EDITOR_VERSION,
            "Editor-Plugin-Version": EDITOR_PLUGIN_VERSION,
            "User-Agent": USER_AGENT,
            "X-GitHub-Api-Version": API_VERSION,
        }

    def _exchange_token(self) -> dict:
        """Exchange GitHub OAuth token for Copilot session token."""
        req = Request(
            f"{GITHUB_API}/copilot_internal/v2/token",
            headers=self._github_headers(),
        )
        with urlopen(req, timeout=15) as resp:
            return json.loads(resp.read())

    def get_token(self) -> str:
        """Get a valid Copilot session token, refreshing if needed."""
        with self._lock:
            if self.copilot_token and time.time() < self.expires_at - 120:
                return self.copilot_token

            log.info("Exchanging GitHub token for Copilot session token...")
            try:
                data = self._exchange_token()
                self.copilot_token = data["token"]
                self.expires_at = data["expires_at"]
                refresh_in = data.get("refresh_in", 1500)
                log.info(
                    "Got Copilot token (expires in %ds, refresh in %ds)",
                    self.expires_at - time.time(),
                    refresh_in,
                )
                return self.copilot_token
            except HTTPError as e:
                body = e.read().decode()
                log.error("Token exchange failed (%d): %s", e.code, body)
                raise
            except Exception as e:
                log.error("Token exchange failed: %s", e)
                raise

    def copilot_headers(self) -> dict[str, str]:
        """Headers for Copilot API requests."""
        return {
            "Authorization": f"Bearer {self.get_token()}",
            "Content-Type": "application/json",
            "Copilot-Integration-Id": "vscode-chat",
            "Editor-Version": EDITOR_VERSION,
            "Editor-Plugin-Version": EDITOR_PLUGIN_VERSION,
            "User-Agent": USER_AGENT,
            "OpenAI-Intent": "conversation-panel",
            "X-GitHub-Api-Version": API_VERSION,
        }


class ThreadingHTTPServer(ThreadingMixIn, HTTPServer):
    daemon_threads = True


class ProxyHandler(BaseHTTPRequestHandler):
    token_manager: TokenManager
    protocol_version = "HTTP/1.1"

    def log_message(self, format, *args):
        log.info(format, *args)

    def do_GET(self):
        if self.path in ("/health", "/healthz"):
            self._respond(200, {"status": "ok"})
            return

        if self.path.startswith("/v1/models") or self.path.startswith("/models"):
            self._proxy("/models", "GET")
            return

        self._respond(404, {"error": "Not found"})

    def do_POST(self):
        if self.path in ("/responses", "/v1/responses"):
            self._proxy("/responses", "POST")
            return

        if self.path in ("/chat/completions", "/v1/chat/completions"):
            self._proxy("/chat/completions", "POST")
            return

        self._respond(404, {"error": "Not found"})

    def _proxy(self, upstream_path: str, method: str):
        try:
            # Read request body
            body = None
            if method == "POST":
                content_length = int(self.headers.get("Content-Length", 0))
                body = self.rfile.read(content_length) if content_length else b""

            headers = self.token_manager.copilot_headers()
            if upstream_path == "/models":
                headers["X-GitHub-Api-Version"] = "2025-05-01"
            initiator = self.headers.get("X-Initiator")
            if initiator:
                headers["X-Initiator"] = initiator

            # Use http.client for proper streaming support
            ctx = ssl.create_default_context()
            conn = http.client.HTTPSConnection("api.githubcopilot.com", timeout=300, context=ctx)
            try:
                conn.request(method, upstream_path, body=body, headers=headers)
                resp = conn.getresponse()

                content_type = resp.getheader("Content-Type", "application/json")
                is_streaming = "text/event-stream" in content_type

                self.send_response(resp.status)
                self.send_header("Content-Type", content_type)
                for h in ("X-RateLimit-Limit", "X-RateLimit-Remaining", "X-RateLimit-Reset"):
                    val = resp.getheader(h)
                    if val:
                        self.send_header(h, val)

                if is_streaming:
                    self.send_header("Transfer-Encoding", "chunked")
                    self.send_header("Cache-Control", "no-cache")
                    self.send_header("Connection", "keep-alive")
                    self.end_headers()
                    while True:
                        chunk = resp.read(4096)
                        if not chunk:
                            break
                        self.wfile.write(f"{len(chunk):x}\r\n".encode())
                        self.wfile.write(chunk)
                        self.wfile.write(b"\r\n")
                        self.wfile.flush()
                    self.wfile.write(b"0\r\n\r\n")
                    self.wfile.flush()
                else:
                    resp_body = resp.read()
                    self.send_header("Content-Length", str(len(resp_body)))
                    self.end_headers()
                    self.wfile.write(resp_body)
            finally:
                conn.close()

        except Exception as e:
            log.error("Proxy error: %s", e, exc_info=True)
            self._respond(502, {"error": str(e)})

    def _respond(self, code: int, body: dict):
        data = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def discover_github_token() -> str | None:
    """Find a GitHub OAuth token from available sources."""

    # 1. Saved token from previous device flow
    if TOKEN_FILE.exists():
        try:
            data = json.loads(TOKEN_FILE.read_text())
            token = data.get("github_token")
            if token:
                log.info("Using saved token from %s", TOKEN_FILE)
                return token
        except Exception:
            pass

    # 2. Explicit env var
    token = os.environ.get("GH_COPILOT_TOKEN")
    if token:
        log.info("Using token from GH_COPILOT_TOKEN env var")
        return token

    # 3. hosts.json (from VS Code / JetBrains Copilot extension)
    for config_dir in [
        Path.home() / ".config" / "github-copilot",
        Path.home() / "Library" / "Application Support" / "github-copilot",
    ]:
        for fname in ("hosts.json", "apps.json"):
            path = config_dir / fname
            if path.exists():
                try:
                    data = json.loads(path.read_text())
                    token = data.get("github.com", {}).get("oauth_token")
                    if token:
                        log.info("Using token from %s", path)
                        return token
                except Exception:
                    pass

    return None


def github_device_flow() -> str:
    """Run the GitHub OAuth device flow to get a token for the Copilot app."""
    log.info("Starting GitHub device flow authentication...")

    # Step 1: Request device code
    req = Request(
        f"{GITHUB_BASE}/login/device/code",
        data=json.dumps({
            "client_id": GITHUB_CLIENT_ID,
            "scope": "read:user",
        }).encode(),
        headers={"Content-Type": "application/json", "Accept": "application/json"},
        method="POST",
    )
    with urlopen(req, timeout=15) as resp:
        device_data = json.loads(resp.read())

    user_code = device_data["user_code"]
    verification_uri = device_data["verification_uri"]
    device_code = device_data["device_code"]
    interval = device_data.get("interval", 5) + 1

    print()
    print(f"  ┌─────────────────────────────────────────────┐")
    print(f"  │  Go to: {verification_uri:<35} │")
    print(f"  │  Enter code: {user_code:<30} │")
    print(f"  └─────────────────────────────────────────────┘")
    print()
    log.info("Waiting for authorization...")

    # Step 2: Poll for access token
    while True:
        time.sleep(interval)
        req = Request(
            f"{GITHUB_BASE}/login/oauth/access_token",
            data=json.dumps({
                "client_id": GITHUB_CLIENT_ID,
                "device_code": device_code,
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
            }).encode(),
            headers={"Content-Type": "application/json", "Accept": "application/json"},
            method="POST",
        )
        try:
            with urlopen(req, timeout=15) as resp:
                token_data = json.loads(resp.read())
        except Exception:
            continue

        if "access_token" in token_data:
            token = token_data["access_token"]
            # Save for reuse
            TOKEN_FILE.parent.mkdir(parents=True, exist_ok=True)
            TOKEN_FILE.write_text(json.dumps({"github_token": token}))
            TOKEN_FILE.chmod(0o600)
            log.info("✓ Authenticated! Token saved to %s", TOKEN_FILE)
            return token

        error = token_data.get("error", "")
        if error == "authorization_pending":
            continue
        elif error == "slow_down":
            interval += 5
            continue
        elif error == "expired_token":
            log.error("Device code expired. Please try again.")
            sys.exit(1)
        elif error == "access_denied":
            log.error("Authorization denied by user.")
            sys.exit(1)
        else:
            log.debug("Poll response: %s", token_data)
            continue


def main():
    parser = argparse.ArgumentParser(description="Copilot Responses API proxy for Codex CLI")
    parser.add_argument("--port", type=int, default=4141, help="Port to listen on (default: 4141)")
    parser.add_argument("--token", help="GitHub OAuth token (or set GH_TOKEN env var)")
    args = parser.parse_args()

    github_token = args.token or discover_github_token()
    if not github_token:
        # No saved token — run device flow
        github_token = github_device_flow()

    # Validate token
    token_manager = TokenManager(github_token)
    try:
        token_manager.get_token()
    except Exception as e:
        log.error("Failed to authenticate with Copilot: %s", e)
        sys.exit(1)

    log.info("✓ Authenticated with GitHub Copilot")

    # Set up handler
    ProxyHandler.token_manager = token_manager
    server = ThreadingHTTPServer(("127.0.0.1", args.port), ProxyHandler)

    log.info("Proxy listening on http://127.0.0.1:%d", args.port)
    log.info("Codex config:")
    log.info('  model_provider = "copilot"')
    log.info("  [model_providers.copilot]")
    log.info('  name = "GitHub Copilot"')
    log.info('  base_url = "http://127.0.0.1:%d"', args.port)
    log.info("")
    log.info("Supported /responses models: gpt-5.2-codex, gpt-5.1-codex, gpt-5.1, gpt-5-mini, gpt-5.2")

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        log.info("Shutting down")
        server.shutdown()


if __name__ == "__main__":
    main()
