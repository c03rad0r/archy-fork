#!/usr/bin/env bats
# tests/lifecycle/bats/btcpay.bats
#
# Lifecycle tests for the btcpay-server multi-container stack:
#   - btcpay-server      (the main app)
#   - archy-btcpay-db    (postgres)
#   - archy-nbxplorer    (Bitcoin watcher)
#
# Multi-container variant of bitcoin-knots.bats / lnd.bats / electrumx.bats.
# UI URL coverage is in ui-coverage.bats; this suite is L1 (RPC API) + L3
# (lifecycle survival).
#
# Pre-req: btcpay-server installed, bitcoin-knots running.

load '../lib/rpc.bash'

setup_file() {
  : "${ARCHY_PASSWORD:?Set ARCHY_PASSWORD env var to the UI password}"
  export ARCHY_FORCE_LOGIN=1
  rpc_login
  unset ARCHY_FORCE_LOGIN
}

teardown_file() {
  rpc_logout_local
}

btcpay_components=(
  "btcpay-server"
  "archy-btcpay-db"
  "archy-nbxplorer"
)

@test "container-list includes every btcpay-stack component" {
  run rpc_result container-list
  [ "$status" -eq 0 ]
  for c in "${btcpay_components[@]}"; do
    echo "$output" | jq -e --arg n "$c" '.[] | select(.name == $n)' >/dev/null \
      || skip "btcpay component $c not present (stack not installed)"
  done
}

@test "container-list reports valid states for every btcpay component" {
  run rpc_result container-list
  [ "$status" -eq 0 ]
  local present=0
  for c in "${btcpay_components[@]}"; do
    local state
    state=$(echo "$output" | jq -r --arg n "$c" '.[] | select(.name == $n) | .state')
    [[ -n "$state" ]] || continue
    present=$((present + 1))
    [[ "$state" =~ ^(running|stopped|exited|created|paused)$ ]] \
      || fail "invalid state for $c: $state"
  done
  (( present > 0 )) || skip "btcpay stack not installed"
}

@test "no orphan btcpay-related containers beyond the known set" {
  local total known
  total=$(podman ps -a --format '{{.Names}}' \
    | grep -Ec '^(btcpay|archy-btcpay|archy-nbxplorer)' || true)
  known=$(podman ps -a --format '{{.Names}}' \
    | grep -Ec '^(btcpay-server|archy-btcpay-db|archy-nbxplorer)$' || true)
  [ "$total" -eq "$known" ]
}

# ────────────────────────────────────────────────────────────────────
# Destructive tier
# ────────────────────────────────────────────────────────────────────

@test "package.stop transitions btcpay-server to stopped" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  podman inspect btcpay-server --format '{{.State.Status}}' >/dev/null 2>&1 \
    || skip "btcpay-server not installed"

  run rpc_result package.stop '{"id":"btcpay-server"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status btcpay-server stopped 60
  [ "$status" -eq 0 ]
}

@test "package.start brings btcpay-server back to running" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  podman inspect btcpay-server --format '{{.State.Status}}' >/dev/null 2>&1 \
    || skip "btcpay-server not installed"

  run rpc_result package.start '{"id":"btcpay-server"}'
  [ "$status" -eq 0 ]

  # 300s, not 180s: stopping btcpay DELETES the container (quadlet renders
  # --rm), so this is a full recreate of a dotnet image, not a container start.
  # Measured 52s on a quiet box; it blew past 180s during a gate run on the
  # same node at load ~11. That is the app being slow under contention, not a
  # lifecycle fault.
  run wait_for_container_status btcpay-server running "${ARCHY_BTCPAY_START_SECS:-300}"
  [ "$status" -eq 0 ]
}

@test "package.restart leaves btcpay-server in running state" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  podman inspect btcpay-server --format '{{.State.Status}}' >/dev/null 2>&1 \
    || skip "btcpay-server not installed"

  run rpc_result package.restart '{"id":"btcpay-server"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status btcpay-server running "${ARCHY_BTCPAY_START_SECS:-300}"
  [ "$status" -eq 0 ]
}

@test "db + nbxplorer remain running across btcpay-server restart" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  for c in archy-btcpay-db archy-nbxplorer; do
    podman inspect "$c" --format '{{.State.Status}}' >/dev/null 2>&1 \
      || skip "btcpay supporting container $c not installed"
  done

  for c in archy-btcpay-db archy-nbxplorer; do
    local state
    state=$(podman inspect --format '{{.State.Status}}' "$c" 2>/dev/null)
    [[ "$state" == "running" ]] \
      || fail "supporting btcpay container $c is not running (state=$state) — package.restart cascaded into it"
  done
}

@test "package.uninstall removes the whole btcpay stack" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"
  podman inspect btcpay-server --format '{{.State.Status}}' >/dev/null 2>&1 \
    || skip "btcpay-server not installed"

  run rpc_result package.uninstall '{"id":"btcpay-server","preserve_data":true}'
  [ "$status" -eq 0 ]

  for c in "${btcpay_components[@]}"; do
    run wait_for_container_status "$c" absent 120
    [ "$status" -eq 0 ] || fail "btcpay component $c not removed by uninstall"
  done
}

@test "package.install restores the whole btcpay stack" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"

  run rpc_result package.install '{"manifest_path":"btcpay-server/manifest.yaml"}'
  [ "$status" -eq 0 ]

  for c in "${btcpay_components[@]}"; do
    run wait_for_container_status "$c" running 240
    [ "$status" -eq 0 ] || fail "btcpay component $c never reached running after reinstall"
  done
}
