#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
export ARCHY_SESSION_STATE_DIR="$TMP/state"; mkdir -p "$TMP/work"
if ! command -v tmux >/dev/null 2>&1; then echo "SKIP: tmux is unavailable"; exit 0; fi
id=$("$ROOT/scripts/archy-session" start demo "$TMP/work")
test -f "$TMP/state/$id.json"
grep -q "$id" <("$ROOT/scripts/archy-session" list)
"$ROOT/scripts/archy-session" rename "$id" renamed
grep -q $'renamed\tdetached' <("$ROOT/scripts/archy-session" list)
"$ROOT/scripts/archy-session" end "$id"
grep -q $'renamed\tended' <("$ROOT/scripts/archy-session" list)
test -d "$TMP/work"; echo "archy-session lifecycle passed"
