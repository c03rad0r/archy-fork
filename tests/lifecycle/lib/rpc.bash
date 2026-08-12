#!/usr/bin/env bash
# tests/lifecycle/lib/rpc.bash
#
# Shared JSON-RPC client for archipelago lifecycle tests.
# Handles login, session cookie + CSRF token management, and request plumbing.
#
# Environment variables honored:
#   ARCHY_HOST     — default: 127.0.0.1
#   ARCHY_SCHEME   — default: https
#   ARCHY_PASSWORD — REQUIRED. The UI password.
#
# After sourcing, call `rpc_login` once per test file in setup_file or setup.
# Then call `rpc_call METHOD [JSON_PARAMS]` to invoke methods.
# rpc_call prints the raw JSON response to stdout.

set -euo pipefail

ARCHY_HOST="${ARCHY_HOST:-127.0.0.1}"
ARCHY_SCHEME="${ARCHY_SCHEME:-https}"
ARCHY_BASE_URL="${ARCHY_SCHEME}://${ARCHY_HOST}"

# Session file lives in a stable per-user location so every bats subshell
# (setup_file, setup, each @test) sees the same cookies. File format:
#   line 1: session cookie value
#   line 2: csrf cookie value
RPC_SESSION_FILE="${RPC_SESSION_FILE:-${TMPDIR:-/tmp}/archy-rpc-session-${UID:-$(id -u)}}"

RPC_SESSION=""
RPC_CSRF=""

# Load cookies from $RPC_SESSION_FILE into RPC_SESSION/RPC_CSRF.
# Returns 1 if the file is missing or malformed.
_rpc_load_session() {
  [[ -r "$RPC_SESSION_FILE" ]] || return 1
  local lines
  mapfile -t lines < "$RPC_SESSION_FILE"
  RPC_SESSION="${lines[0]:-}"
  RPC_CSRF="${lines[1]:-}"
  [[ -n "$RPC_SESSION" && -n "$RPC_CSRF" ]]
}

# Log in with $ARCHY_PASSWORD and persist session + csrf cookies to $RPC_SESSION_FILE.
# Idempotent-ish: if a valid session file already exists and ARCHY_FORCE_LOGIN
# is not set, we reuse it (saves a round-trip per test file).
rpc_login() {
  if _rpc_load_session && [[ -z "${ARCHY_FORCE_LOGIN:-}" ]]; then
    return 0
  fi

  if [[ -z "${ARCHY_PASSWORD:-}" ]]; then
    echo "rpc_login: ARCHY_PASSWORD env var not set" >&2
    return 1
  fi

  local headers body
  headers=$(mktemp)
  body=$(curl -sk -D "$headers" -X POST "${ARCHY_BASE_URL}/rpc/v1" \
    -H 'Content-Type: application/json' \
    --data-raw "{\"jsonrpc\":\"2.0\",\"method\":\"auth.login\",\"params\":{\"password\":\"${ARCHY_PASSWORD}\"},\"id\":1}")

  local err
  err=$(echo "$body" | jq -r '.error // empty')
  if [[ -n "$err" && "$err" != "null" ]]; then
    echo "rpc_login failed: $err" >&2
    rm -f "$headers"
    return 1
  fi

  RPC_SESSION=$(grep -i '^set-cookie: session=' "$headers" | head -1 | sed -E 's/.*session=([^;]+).*/\1/' | tr -d '\r')
  RPC_CSRF=$(grep -i '^set-cookie: csrf_token=' "$headers" | head -1 | sed -E 's/.*csrf_token=([^;]+).*/\1/' | tr -d '\r')
  rm -f "$headers"

  if [[ -z "$RPC_SESSION" || -z "$RPC_CSRF" ]]; then
    echo "rpc_login: missing session or csrf cookie in response" >&2
    return 1
  fi

  # Persist for subsequent subshells.
  umask 077
  printf '%s\n%s\n' "$RPC_SESSION" "$RPC_CSRF" > "$RPC_SESSION_FILE"
  return 0
}

# Forget persisted session (e.g., at end of a test run).
rpc_logout_local() {
  rm -f "$RPC_SESSION_FILE"
  RPC_SESSION=""
  RPC_CSRF=""
}

# Call an RPC method.
# Usage: rpc_call METHOD [PARAMS_JSON]
# Prints the full JSON-RPC response object to stdout.
# Returns 0 on successful HTTP call (regardless of RPC-level error).
rpc_call() {
  local method="$1"
  local params="${2:-null}"
  local id="${3:-$RANDOM}"

  if [[ -z "$RPC_SESSION" || -z "$RPC_CSRF" ]]; then
    _rpc_load_session || {
      echo "rpc_call: not logged in (call rpc_login first)" >&2
      return 1
    }
  fi

  local payload
  if [[ "$params" == "null" ]]; then
    payload=$(jq -nc --arg m "$method" --argjson id "$id" '{jsonrpc:"2.0",method:$m,id:$id}')
  else
    payload=$(jq -nc --arg m "$method" --argjson id "$id" --argjson p "$params" '{jsonrpc:"2.0",method:$m,params:$p,id:$id}')
  fi

  curl -sk -X POST "${ARCHY_BASE_URL}/rpc/v1" \
    -H 'Content-Type: application/json' \
    -H "Cookie: session=${RPC_SESSION}; csrf_token=${RPC_CSRF}" \
    -H "X-CSRF-Token: ${RPC_CSRF}" \
    --data-raw "$payload"
}

# Convenience: call rpc and return only the .result field (or fail if .error is set).
rpc_result() {
  local resp
  resp=$(rpc_call "$@")
  local err
  err=$(echo "$resp" | jq -r '.error // empty')
  if [[ -n "$err" && "$err" != "null" ]]; then
    echo "rpc_result: $1 failed: $err" >&2
    echo "full response: $resp" >&2
    return 1
  fi
  echo "$resp" | jq '.result'
}

# Wait for a container to reach a given status ("running" or "stopped" or "absent").
# Usage: wait_for_container_status NAME STATUS [TIMEOUT_SECONDS]
wait_for_container_status() {
  local name="$1"
  local target="$2"
  local timeout="${3:-60}"
  local deadline=$(( $(date +%s) + timeout ))

  while (( $(date +%s) < deadline )); do
    local list state status
    list=$(rpc_result container-list 2>/dev/null || echo '[]')
    if [[ "$target" == "absent" ]]; then
      if ! echo "$list" | jq -e --arg n "$name" '.[] | select(.name == $n)' >/dev/null 2>&1; then
        return 0
      fi
    else
      # Primary source: container-list state keyed by container name.
      state=$(echo "$list" | jq -r --arg n "$name" '.[] | select(.name == $n) | .state // "unknown"')
      if [[ "$state" == "$target" ]]; then
        return 0
      fi

      # Fallback: container-status RPC accepts app_id. For common UI-prefixed
      # names, strip archy- prefix before querying.
      local app_id="$name"
      if [[ $app_id == bitcoin-knots ]]; then
        app_id=bitcoin-core
      elif [[ $app_id == electrs || $app_id == mempool-electrs ]]; then
        app_id=electrumx
      elif [[ $app_id == archy-* ]]; then
        app_id=${app_id#archy-}
      fi
      status=$(rpc_result container-status "{\"app_id\":\"$app_id\"}" 2>/dev/null | jq -r '.status // .state // "unknown"')
      if [[ "$status" == "$target" ]]; then
        return 0
      fi
    fi
    sleep 2
  done

  echo "wait_for_container_status: $name did not reach '$target' within ${timeout}s" >&2
  return 1
}
