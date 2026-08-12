#!/usr/bin/env bats
# tests/lifecycle/bats/fedimint.bats
#
# Lifecycle tests for the fedimint package. The fedimint federation
# daemon runs as a single container; the gateway is its own package
# (fedimint-gateway). Mirrors the single-container pattern of
# lnd.bats / electrumx.bats for L1 (RPC API) + L3 (lifecycle survival).
# UI URL coverage is in ui-coverage.bats.

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

fedimint_skip_if_absent() {
  podman inspect fedimint --format '{{.State.Status}}' >/dev/null 2>&1 \
    || skip "fedimint not installed"
}

@test "container-list includes fedimint" {
  run rpc_result container-list
  [ "$status" -eq 0 ]
  echo "$output" | jq -e '.[] | select(.name == "fedimint")' >/dev/null \
    || skip "fedimint not installed"
}

@test "container-list reports a valid state for fedimint" {
  fedimint_skip_if_absent
  run rpc_result container-list
  [ "$status" -eq 0 ]
  local state
  state=$(echo "$output" | jq -r '.[] | select(.name == "fedimint") | .state')
  [[ "$state" =~ ^(running|stopped|exited|created|paused)$ ]]
}

@test "no orphan fedimint-related containers beyond the known set" {
  local total known
  total=$(podman ps -a --format '{{.Names}}' \
    | grep -Ec '^(fedimint|fedimintd|fedimint-gateway)' || true)
  # `fedimint-clientd` (the dual-ecash HTTP bridge) is a legitimate, known
  # container — and the unanchored `total` regex above counts it (it starts
  # with "fedimint"). It must therefore be in the known set too, or every node
  # running fedimint-clientd false-fails this orphan check.
  known=$(podman ps -a --format '{{.Names}}' \
    | grep -Ec '^(fedimint|fedimint-clientd|fedimint-gateway)$' || true)
  [ "$total" -eq "$known" ]
}

# ────────────────────────────────────────────────────────────────────
# Destructive tier
# ────────────────────────────────────────────────────────────────────

@test "package.stop transitions fedimint to stopped" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  fedimint_skip_if_absent

  run rpc_result package.stop '{"id":"fedimint"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status fedimint stopped 60
  [ "$status" -eq 0 ]
}

@test "package.start brings fedimint back to running" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  fedimint_skip_if_absent

  run rpc_result package.start '{"id":"fedimint"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status fedimint running 180
  [ "$status" -eq 0 ]
}

@test "package.restart leaves fedimint in running state" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  fedimint_skip_if_absent

  run rpc_result package.restart '{"id":"fedimint"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status fedimint running 180
  [ "$status" -eq 0 ]
}

# ────────────────────────────────────────────────────────────────────
# Cascade-destructive tier
# ────────────────────────────────────────────────────────────────────

@test "package.uninstall removes fedimint" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"
  fedimint_skip_if_absent

  run rpc_result package.uninstall '{"id":"fedimint","preserve_data":true}'
  [ "$status" -eq 0 ]

  run wait_for_container_status fedimint absent 120
  [ "$status" -eq 0 ]
}

@test "package.install fedimint returns to running" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"

  run rpc_result package.install '{"manifest_path":"fedimint/manifest.yaml"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status fedimint running 240
  [ "$status" -eq 0 ]
}
