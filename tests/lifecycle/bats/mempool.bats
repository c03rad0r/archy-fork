#!/usr/bin/env bats
# tests/lifecycle/bats/mempool.bats
#
# Lifecycle tests for the mempool stack:
#   - mempool          (legacy install path; the frontend container)
#   - mempool-api      (orchestrator-managed; the backend api)
#   - archy-mempool-db (orchestrator-managed; the mariadb)
#   - archy-mempool-web (orchestrator-managed; the proxy/static layer)
#
# The mempool stack is split between the legacy install path (mempool itself)
# and orchestrator-managed sub-containers — see uses_orchestrator_install_flow
# in install.rs. Tests here treat them as one stack at the package.install/stop
# level, addressed by id "mempool". UI URL coverage is in ui-coverage.bats.

load '../lib/rpc.bash'

# bats-assert is not loaded in this suite (only rpc.bash), so provide a minimal
# `fail` so the `|| fail "..."` guards below report a real assertion failure
# instead of an undefined-command status 127 that masks the actual reason.
fail() { echo "$@" >&2; return 1; }

setup_file() {
  : "${ARCHY_PASSWORD:?Set ARCHY_PASSWORD env var to the UI password}"
  export ARCHY_FORCE_LOGIN=1
  rpc_login
  unset ARCHY_FORCE_LOGIN
}

teardown_file() {
  rpc_logout_local
}

mempool_components=(
  "mempool-api"
  "archy-mempool-db"
)

mempool_optional_components=(
  "mempool"
  "archy-mempool-web"
)

mempool_skip_if_absent() {
  for c in "${mempool_components[@]}"; do
    podman inspect "$c" --format '{{.State.Status}}' >/dev/null 2>&1 && return 0
  done
  skip "mempool stack not installed"
}

@test "container-list includes the core mempool components" {
  run rpc_result container-list
  [ "$status" -eq 0 ]
  local found=0
  for c in "${mempool_components[@]}"; do
    if echo "$output" | jq -e --arg n "$c" '.[] | select(.name == $n)' >/dev/null; then
      found=$((found + 1))
    fi
  done
  (( found > 0 )) || skip "mempool stack not installed"
}

@test "every present mempool component reports a valid state" {
  run rpc_result container-list
  [ "$status" -eq 0 ]
  local present=0
  for c in "${mempool_components[@]}" "${mempool_optional_components[@]}"; do
    local state
    state=$(echo "$output" | jq -r --arg n "$c" '.[] | select(.name == $n) | .state')
    [[ -n "$state" ]] || continue
    present=$((present + 1))
    [[ "$state" =~ ^(running|stopped|exited|created|paused)$ ]] \
      || fail "invalid state for $c: $state"
  done
  (( present > 0 )) || skip "mempool stack not installed"
}

@test "no orphan mempool-related containers beyond the known set" {
  # Poll for steady state (don't single-shot): a stack restart in a prior tier
  # briefly leaves a recreated member visible alongside its replacement, so a
  # one-shot count can momentarily see total>known even though the reconciler
  # converges within seconds. A genuine orphan never clears, so this still
  # catches it — it just tolerates the transient recreate window.
  local total known deadline=$(( $(date +%s) + 30 ))
  while (( $(date +%s) < deadline )); do
    total=$(podman ps -a --format '{{.Names}}' \
      | grep -Ec '^(mempool|archy-mempool)' || true)
    known=$(podman ps -a --format '{{.Names}}' \
      | grep -Ec '^(mempool|mempool-api|archy-mempool-db|archy-mempool-web)$' || true)
    [ "$total" -eq "$known" ] && return 0
    sleep 3
  done
  echo "orphan mempool container persisted >30s (total=$total known=$known):" >&2
  podman ps -a --format '{{.Names}}' | grep -E '^(mempool|archy-mempool)' \
    | grep -vE '^(mempool|mempool-api|archy-mempool-db|archy-mempool-web)$' >&2 || true
  return 1
}

# ────────────────────────────────────────────────────────────────────
# Destructive tier — operate on the package id "mempool" which the
# legacy install path treats as the whole stack
# ────────────────────────────────────────────────────────────────────

@test "package.stop transitions mempool stack to stopped" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  mempool_skip_if_absent

  run rpc_result package.stop '{"id":"mempool"}'
  [ "$status" -eq 0 ]

  # The frontend container is the user-visible target; supporting
  # services may stay running depending on orchestrator policy.
  if podman inspect mempool --format '{{.State.Status}}' >/dev/null 2>&1; then
    run wait_for_container_status mempool stopped 60
    [ "$status" -eq 0 ]
  fi
}

@test "package.start brings mempool stack back to running" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  mempool_skip_if_absent

  run rpc_result package.start '{"id":"mempool"}'
  [ "$status" -eq 0 ]

  if podman inspect mempool --format '{{.State.Status}}' >/dev/null 2>&1; then
    run wait_for_container_status mempool running 180
    [ "$status" -eq 0 ]
  fi
}

@test "package.restart leaves mempool stack in running state" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  mempool_skip_if_absent

  run rpc_result package.restart '{"id":"mempool"}'
  [ "$status" -eq 0 ]

  if podman inspect mempool --format '{{.State.Status}}' >/dev/null 2>&1; then
    run wait_for_container_status mempool running 180
    [ "$status" -eq 0 ]
  fi
}

@test "mempool api backend remains queryable when stack is up" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  mempool_skip_if_absent

  # mempool-api on :8999 — same probe required-stack.bats uses for parity.
  # This case runs immediately after package.restart, so mempool-api has just
  # dropped + must re-establish its electrs/bitcoin connection (it reports
  # "offline" in the frontend during this window). Give it the same recovery
  # budget the passing parity probes use (required-stack-destructive: 240s,
  # package-update-smoke: 300s) — 180s was too tight for the post-restart path.
  local deadline=$(( $(date +%s) + 300 ))
  while (( $(date +%s) < deadline )); do
    if curl -fsS -m 5 "http://127.0.0.1:8999/api/v1/backend-info" >/dev/null 2>&1; then
      return 0
    fi
    sleep 3
  done
  # NB: bats-assert's `fail` is not loaded in this file (only ../lib/rpc.bash),
  # so emit + return non-zero directly rather than calling an undefined helper.
  echo "mempool-api never responded on :8999 within 300s" >&2
  return 1
}

# ────────────────────────────────────────────────────────────────────
# Cascade-destructive tier
# ────────────────────────────────────────────────────────────────────

@test "package.uninstall removes the mempool stack" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"
  mempool_skip_if_absent

  run rpc_result package.uninstall '{"id":"mempool","preserve_data":true}'
  [ "$status" -eq 0 ]

  for c in "${mempool_components[@]}" "${mempool_optional_components[@]}"; do
    if podman inspect "$c" --format '{{.State.Status}}' >/dev/null 2>&1; then
      run wait_for_container_status "$c" absent 120
      [ "$status" -eq 0 ] || fail "mempool component $c not removed by uninstall"
    fi
  done
}

@test "package.install restores the mempool stack" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"

  run rpc_result package.install '{"manifest_path":"mempool/manifest.yaml"}'
  [ "$status" -eq 0 ]

  # At minimum the core orchestrator-managed components must come back.
  for c in "${mempool_components[@]}"; do
    run wait_for_container_status "$c" running 240
    [ "$status" -eq 0 ] || fail "mempool component $c never reached running after reinstall"
  done
}
