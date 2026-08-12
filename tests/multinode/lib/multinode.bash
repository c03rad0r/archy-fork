#!/usr/bin/env bash
# Multi-node RPC harness library.
#
# Unlike tests/lifecycle/lib/rpc.bash (which targets a single ARCHY_HOST),
# this drives N independent archipelago nodes in one run so we can exercise
# real node-to-node paths: federation sync over Tor, FIPS anchoring, etc.
#
# A "node handle" is a short label (e.g. A, B, alice). For each handle you
# register a base URL + UI password; the lib logs in and keeps that node's
# session/CSRF cookies in its own state file so calls never cross wires.
#
# Usage:
#   source tests/multinode/lib/multinode.bash
#   node_register A https://192.0.2.10 "$A_PW"
#   node_register B http://192.0.2.12  "$B_PW"
#   node_login A; node_login B
#   node_rpc A node.tor-address
#   node_result B federation.list-nodes
#
# Requires: curl, jq.
#
# Credentials: node passwords are NEVER committed. Export *_PW env vars, or
# put them in tests/multinode/.env (git-ignored; see .env.example) — sourced
# automatically below so every suite picks them up.
#
# Note: this is a library — it does NOT set shell options (set -u/-e), since
# that would leak into the sourcing script. Each function guards its own vars
# with ${var:-} defaults. Callers set their own options.

# Auto-load git-ignored local credentials (tests/multinode/.env), if present.
_MN_ENV_FILE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)/.env"
# shellcheck disable=SC1090
[[ -f "$_MN_ENV_FILE" ]] && source "$_MN_ENV_FILE"

# Where per-node session state lives (one file per handle).
MULTINODE_STATE_DIR="${MULTINODE_STATE_DIR:-/tmp/archy-multinode}"
mkdir -p "$MULTINODE_STATE_DIR"

# handle -> base url / password, kept in associative arrays.
declare -gA _MN_URL
declare -gA _MN_PW
declare -gA _MN_SESSION
declare -gA _MN_CSRF

# node_register HANDLE BASE_URL PASSWORD
node_register() {
  local h="$1" url="$2" pw="$3"
  _MN_URL[$h]="${url%/}"
  _MN_PW[$h]="$pw"
}

_mn_session_file() { echo "$MULTINODE_STATE_DIR/session-$1"; }

# node_login HANDLE — authenticate and capture session + csrf cookies.
node_login() {
  local h="$1"
  local url="${_MN_URL[$h]:-}" pw="${_MN_PW[$h]:-}"
  if [[ -z "$url" || -z "$pw" ]]; then
    echo "node_login: handle '$h' not registered" >&2
    return 1
  fi
  local headers; headers=$(mktemp)
  local body
  body=$(curl -sk --connect-timeout 10 --max-time "${MULTINODE_RPC_TIMEOUT:-120}" \
    -D "$headers" -X POST "${url}/rpc/v1" \
    -H 'Content-Type: application/json' \
    --data-raw "{\"jsonrpc\":\"2.0\",\"method\":\"auth.login\",\"params\":{\"password\":\"${pw}\"},\"id\":1}")
  local err; err=$(echo "$body" | jq -r '.error.message // empty' 2>/dev/null)
  if [[ -n "$err" ]]; then
    echo "node_login[$h] failed: $err" >&2
    rm -f "$headers"
    return 1
  fi
  local session csrf
  session=$(grep -i '^set-cookie: session=' "$headers" | head -1 | sed -E 's/.*session=([^;]+).*/\1/' | tr -d '\r')
  csrf=$(grep -i '^set-cookie: csrf_token=' "$headers" | head -1 | sed -E 's/.*csrf_token=([^;]+).*/\1/' | tr -d '\r')
  rm -f "$headers"
  if [[ -z "$session" || -z "$csrf" ]]; then
    echo "node_login[$h]: missing session/csrf cookie" >&2
    return 1
  fi
  _MN_SESSION[$h]="$session"
  _MN_CSRF[$h]="$csrf"
  printf '%s\n%s\n' "$session" "$csrf" > "$(_mn_session_file "$h")"
}

# node_rpc HANDLE METHOD [PARAMS_JSON] — raw JSON-RPC response on stdout.
node_rpc() {
  local h="$1" method="$2" params="${3:-}"
  local url="${_MN_URL[$h]:-}"
  local session="${_MN_SESSION[$h]:-}" csrf="${_MN_CSRF[$h]:-}"
  if [[ -z "$session" || -z "$csrf" ]] && [[ -f "$(_mn_session_file "$h")" ]]; then
    mapfile -t lines < "$(_mn_session_file "$h")"
    session="${lines[0]:-}"; csrf="${lines[1]:-}"
    _MN_SESSION[$h]="$session"; _MN_CSRF[$h]="$csrf"
  fi
  local payload
  if [[ -z "$params" ]]; then
    payload=$(jq -nc --arg m "$method" '{jsonrpc:"2.0",method:$m,id:1}')
  else
    payload=$(jq -nc --arg m "$method" --argjson p "$params" '{jsonrpc:"2.0",method:$m,params:$p,id:1}')
  fi
  # Bounded so one slow/hung server-side RPC can't hang the whole suite;
  # override per-run with MULTINODE_RPC_TIMEOUT (seconds).
  curl -sk --connect-timeout 10 --max-time "${MULTINODE_RPC_TIMEOUT:-120}" \
    -X POST "${url}/rpc/v1" \
    -H 'Content-Type: application/json' \
    -H "Cookie: session=${session}; csrf_token=${csrf}" \
    -H "X-CSRF-Token: ${csrf}" \
    --data-raw "$payload"
}

# node_result HANDLE METHOD [PARAMS_JSON] — .result on success; prints error to
# stderr and returns non-zero on RPC error.
node_result() {
  local resp; resp=$(node_rpc "$@")
  local err; err=$(echo "$resp" | jq -r '.error.message // empty' 2>/dev/null)
  if [[ -n "$err" ]]; then
    echo "node_result[$1 $2] error: $err" >&2
    return 1
  fi
  echo "$resp" | jq '.result'
}

# node_onion HANDLE — echo this node's own .onion address (empty if none).
node_onion() {
  node_result "$1" node.tor-address 2>/dev/null | jq -r '. // empty | if type=="object" then (.onion // .address // .tor_address // empty) else . end' 2>/dev/null
}
