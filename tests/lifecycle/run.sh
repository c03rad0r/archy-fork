#!/usr/bin/env bash
# tests/lifecycle/run.sh — entrypoint for lifecycle tests.
#
# Must be run on an archy host. Requires bats + jq + curl.
#
# Env:
#   ARCHY_PASSWORD                      (required unless ARCHY_ALLOW_NOAUTH=1)
#   ARCHY_HOST                          (default: 127.0.0.1)
#   ARCHY_SCHEME                        (default: https)
#   ARCHY_ALLOW_DESTRUCTIVE=1           enable stop/start/restart tests
#   ARCHY_ALLOW_CASCADE_DESTRUCTIVE=1   enable uninstall/reinstall tests (rarely used)
#   ARCHY_ALLOW_NOAUTH=1                allow running read-only suites that don't use RPC auth
#
# Usage:
#   tests/lifecycle/run.sh                       # all .bats files
#   tests/lifecycle/run.sh bitcoin-knots          # single file (no extension)
#   tests/lifecycle/run.sh required-stack required-stack-destructive
#   tests/lifecycle/run.sh package-update-smoke

set -euo pipefail

HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"

if ! command -v bats >/dev/null 2>&1; then
  echo "bats not installed. On Debian: sudo apt-get install -y bats" >&2
  exit 2
fi

if [[ -z "${ARCHY_PASSWORD:-}" && "${ARCHY_ALLOW_NOAUTH:-0}" != "1" ]]; then
  echo "ARCHY_PASSWORD env var must be set (or ARCHY_ALLOW_NOAUTH=1 for no-auth suites)." >&2
  exit 2
fi

if (( $# == 0 )); then
  exec bats bats/
fi

targets=()
for arg in "$@"; do
  if [[ -f "bats/${arg}.bats" ]]; then
    targets+=("bats/${arg}.bats")
  elif [[ -f "$arg" ]]; then
    targets+=("$arg")
  else
    echo "unknown test target: $arg" >&2
    exit 2
  fi
done
exec bats "${targets[@]}"
