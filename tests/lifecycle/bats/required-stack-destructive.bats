#!/usr/bin/env bats
# tests/lifecycle/bats/required-stack-destructive.bats
#
# Controlled destructive lifecycle checks for required stack containers.
# Runs only when ARCHY_ALLOW_DESTRUCTIVE=1.

required_containers=(
  "archy-bitcoin-ui"
  "archy-lnd-ui"
  "archy-electrs-ui"
  "mempool"
  "mempool-api"
)

container_installed() {
  podman ps -a --format '{{.Names}}' | grep -Fx "$1" >/dev/null
}

# Only the subset of required_containers actually installed on this node —
# a node without the mempool stack (or another optional app) shouldn't
# hard-fail restarting/probing something it was never meant to have.
installed_required_containers() {
  local c
  for c in "${required_containers[@]}"; do
    container_installed "$c" && echo "$c"
  done
  # Always succeed — under `set -e`, the function's own exit code is that of
  # its last statement, so if the last array entry happens to be a container
  # NOT installed on this node, the whole function (and any bare
  # `x="$(installed_required_containers)"` caller) would spuriously fail even
  # though earlier entries matched fine.
  return 0
}

wait_running() {
  local name="$1"
  local timeout="${2:-120}"
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

wait_http_ok() {
  local url="$1"
  local timeout="${2:-180}"
  local deadline=$(( $(date +%s) + timeout ))
  while (( $(date +%s) < deadline )); do
    if curl -fsS "$url" >/dev/null 2>&1; then
      return 0
    fi
    sleep 2
  done
  return 1
}

restart_with_retry() {
  local name="$1"
  local attempts="${2:-3}"
  local i
  for ((i=1; i<=attempts; i++)); do
    if podman restart "$name" >/dev/null 2>&1; then
      return 0
    fi
    sleep 3
  done
  return 1
}

@test "required-stack destructive gate enabled" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
}

@test "restart each required service container and verify it recovers" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"

  local targets; targets="$(installed_required_containers)"
  [[ -n "$targets" ]] || skip "none of required_containers installed on this node"
  while IFS= read -r c; do
    run restart_with_retry "$c" 4
    [ "$status" -eq 0 ]
    run wait_running "$c" 180
    [ "$status" -eq 0 ]
  done <<< "$targets"
}

@test "required endpoints still respond after restarts" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"

  if container_installed archy-bitcoin-ui; then
    run wait_http_ok "http://127.0.0.1:8334/" 180
    [ "$status" -eq 0 ]
  fi

  # :8081 is nginx-proxy-manager — an OPTIONAL app (not in required_containers).
  # Only assert it when NPM is actually installed on this node; otherwise the
  # required-endpoints check false-fails on nodes that don't run NPM.
  if podman ps --format '{{.Names}}' | grep -q '^nginx-proxy-manager$'; then
    run wait_http_ok "http://127.0.0.1:8081/" 180
    [ "$status" -eq 0 ]
  fi

  if container_installed mempool; then
    run wait_http_ok "http://127.0.0.1:4080/" 180
    [ "$status" -eq 0 ]
  fi

  if container_installed mempool-api; then
    run wait_http_ok "http://127.0.0.1:8999/api/v1/backend-info" 240
    [ "$status" -eq 0 ]
  fi

  if container_installed lnd; then
    # lnd RPC readiness lags container 'running' (wallet unlock + graph sync) —
    # retry rather than single-shot. See lnd.bats.
    run sh -lc 'for i in $(seq 1 60); do
      podman exec lnd lncli --tlscertpath /root/.lnd/tls.cert --macaroonpath /root/.lnd/data/chain/bitcoin/mainnet/readonly.macaroon --rpcserver localhost:10009 getinfo >/dev/null 2>&1 && exit 0
      sleep 3
    done; exit 1'
    [ "$status" -eq 0 ]
  fi
}
