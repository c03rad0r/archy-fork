#!/usr/bin/env bash
# Start Claude proxy and Vite dev server together.
# Both are killed when either exits or when this script receives SIGINT/SIGTERM.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
APP_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$APP_DIR"

# Generate a dev API token if not already set
if [ -z "${VITE_DEV_API_TOKEN:-}" ]; then
  export VITE_DEV_API_TOKEN=$(openssl rand -hex 16)
fi

cleanup() {
  kill 0 2>/dev/null || true
  wait 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# Use local node_modules binaries
BIN="$APP_DIR/node_modules/.bin"

# Kill stale proxy from previous session (it has a different auth token)
if lsof -ti:3141 > /dev/null 2>&1; then
  echo "  Killing stale Claude proxy on :3141"
  kill $(lsof -ti:3141) 2>/dev/null || true
  sleep 0.3
fi
"$BIN/tsx" server/claude-proxy.ts &
sleep 0.3

# Start Vite dev server in background (host binding controlled by vite.config.ts / VITE_HOST)
"$BIN/vite" &

# Wait for all background jobs (compatible with bash 3.2 on macOS)
wait
