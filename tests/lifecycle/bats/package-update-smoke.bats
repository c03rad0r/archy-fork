#!/usr/bin/env bats
# tests/lifecycle/bats/package-update-smoke.bats
#
# Destructive update smoke checks.
# Requires RPC auth (ARCHY_PASSWORD) and ARCHY_ALLOW_DESTRUCTIVE=1.

load '../lib/rpc.bash'

require_destructive() {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
}

require_auth() {
  [[ -n "${ARCHY_PASSWORD:-}" ]] || skip "ARCHY_PASSWORD not set"
}

wait_http_ok() {
  local url="$1"
  local timeout="${2:-240}"
  local deadline=$(( $(date +%s) + timeout ))
  while (( $(date +%s) < deadline )); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 2
  done
  return 1
}

wait_started_at_change() {
  local name="$1"
  local old_started_at="$2"
  local timeout="${3:-300}"
  local deadline=$(( $(date +%s) + timeout ))
  while (( $(date +%s) < deadline )); do
    local started_at running
    started_at=$(podman inspect --format '{{.State.StartedAt}}' "$name" 2>/dev/null || true)
    running=$(podman inspect --format '{{.State.Running}}' "$name" 2>/dev/null || true)
    if [[ -n "$started_at" && "$started_at" != "$old_started_at" && "$running" == "true" ]]; then
      return 0
    fi
    sleep 3
  done
  return 1
}

wait_running() {
  local name="$1"
  local timeout="${2:-240}"
  local deadline=$(( $(date +%s) + timeout ))
  while (( $(date +%s) < deadline )); do
    local running
    running=$(podman inspect --format '{{.State.Running}}' "$name" 2>/dev/null || true)
    if [[ "$running" == "true" ]]; then
      return 0
    fi
    sleep 2
  done
  return 1
}

setup_file() {
  require_auth
  export ARCHY_FORCE_LOGIN=1
  rpc_login
  unset ARCHY_FORCE_LOGIN
}

teardown_file() {
  rpc_logout_local
}

@test "package.update bitcoin-ui restarts container and recovers endpoint" {
  require_destructive

  local before
  before=$(podman inspect --format '{{.State.StartedAt}}' archy-bitcoin-ui 2>/dev/null || true)
  [[ -n "$before" ]] || skip "archy-bitcoin-ui container not found"

  run rpc_call package.update '{"id":"bitcoin-ui"}'
  [ "$status" -eq 0 ]

  local err
  err=$(echo "$output" | jq -r '.error.message // empty')
  if [[ -z "$err" ]]; then
    echo "$output" | jq -e '.result.status == "updating"' >/dev/null
    run wait_started_at_change archy-bitcoin-ui "$before" 360
    if [[ "$status" -ne 0 ]]; then
      run wait_running archy-bitcoin-ui 120
      [ "$status" -eq 0 ]
    fi
  elif [[ "$err" == *"already updating"* ]]; then
    :
  else
    echo "unexpected package.update error: $err" >&2
    return 1
  fi

  run wait_http_ok "http://127.0.0.1:8334/" 180
  [ "$status" -eq 0 ]
}

@test "package.update mempool stack smoke (optional)" {
  require_destructive
  [[ "${ARCHY_ALLOW_STACK_UPDATE:-0}" == "1" ]] || skip "ARCHY_ALLOW_STACK_UPDATE not set"

  local before
  before=$(podman inspect --format '{{.State.StartedAt}}' mempool 2>/dev/null || true)
  [[ -n "$before" ]] || skip "mempool container not found"

  run rpc_call package.update '{"id":"mempool"}'
  [ "$status" -eq 0 ]

  local err
  err=$(echo "$output" | jq -r '.error.message // empty')
  if [[ -z "$err" ]]; then
    echo "$output" | jq -e '.result.status == "updating"' >/dev/null
    run wait_started_at_change mempool "$before" 420
    if [[ "$status" -ne 0 ]]; then
      run wait_running mempool 120
      [ "$status" -eq 0 ]
    fi
  elif [[ "$err" == *"already updating"* ]]; then
    :
  else
    echo "unexpected package.update error: $err" >&2
    return 1
  fi

  run wait_http_ok "http://127.0.0.1:4080/" 240
  [ "$status" -eq 0 ]

  run wait_http_ok "http://127.0.0.1:8999/api/v1/backend-info" 300
  [ "$status" -eq 0 ]
}
