#!/usr/bin/env bats
# tests/lifecycle/bats/immich.bats
#
# Lifecycle tests for the manifest-driven immich stack. The user-facing package is
# "immich" (catalog title + icon); container-list reports it package-level as
# "immich". Its containers are named immich_server / immich_postgres /
# immich_redis (underscore) to match the runtime's per-app lifecycle references.
#
# Tiers:
#   - Read-only (always):                      presence + valid state
#   - Destructive (ARCHY_ALLOW_DESTRUCTIVE=1): stop → start → restart
#   - Cascade (ARCHY_ALLOW_CASCADE_DESTRUCTIVE=1): uninstall → reinstall (preserve_data)
#
# RPC-based, so correct whether run on the host or against a remote ARCHY_HOST.

load '../lib/rpc.bash'

IMMICH_IMAGE="source.archipelago-foundation.org/lfg2025/immich-server:release"

setup_file() {
  : "${ARCHY_PASSWORD:?Set ARCHY_PASSWORD env var to the UI password}"
  export ARCHY_FORCE_LOGIN=1
  rpc_login
  unset ARCHY_FORCE_LOGIN
}

teardown_file() {
  rpc_logout_local
}

# ────────────────────────────────────────────────────────────────────
# Read-only tier
# ────────────────────────────────────────────────────────────────────

@test "container-list includes immich" {
  run rpc_result container-list
  [ "$status" -eq 0 ]
  echo "$output" | jq -e '.[] | select(.name == "immich")' >/dev/null
}

@test "container-list reports a valid state for immich" {
  run rpc_result container-list
  [ "$status" -eq 0 ]
  local state
  state=$(echo "$output" | jq -r '.[] | select(.name == "immich") | .state')
  [[ "$state" =~ ^(running|stopped|exited|created|paused)$ ]]
}

@test "immich exposes its web UI lan-address (port 2283)" {
  # Poll briefly: lan_address is derived from the published host port, which is
  # momentarily absent (null) while immich_server is mid-recreate (e.g. a
  # health-monitor bounce during the read-only tier). A genuinely unexposed
  # immich never publishes 2283, so this still catches real port drift; it only
  # absorbs the transient null seen under churn.
  # 90s (not 30s): the immich stack (postgres→redis→server with DB migrations on
  # boot) can take >30s to publish its host port after a churn-induced recreate,
  # and the destructive-tier immich tests already allow 180–240s for the same
  # stack. A genuinely unexposed immich still never publishes 2283, so this keeps
  # catching real port drift while tolerating slow-but-healthy boots.
  local deadline=$(( $(date +%s) + 90 ))
  while (( $(date +%s) < deadline )); do
    run rpc_result container-list
    [ "$status" -eq 0 ]
    if echo "$output" \
      | jq -e '.[] | select(.name == "immich") | .lan_address // "" | test("2283")' >/dev/null; then
      return 0
    fi
    sleep 3
  done
  echo "immich never reported a lan_address containing 2283 within 90s" >&2
  return 1
}

# ────────────────────────────────────────────────────────────────────
# Destructive tier (stop → start → restart)
# ────────────────────────────────────────────────────────────────────

@test "package.stop transitions immich to stopped" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  # package.stop is async ({"status":"stopping"}) and a stack stop can race a
  # still-settling prior op, so the end state — not the immediate RPC return — is
  # the assertion.
  rpc_call package.stop '{"id":"immich"}' >/dev/null 2>&1 || true
  run wait_for_container_status immich stopped 90
  [ "$status" -eq 0 ]
}

@test "package.start brings immich back to running" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  # Async start; the server comes up only after postgres is ready (~30s+), so wait.
  rpc_call package.start '{"id":"immich"}' >/dev/null 2>&1 || true
  run wait_for_container_status immich running 180
  [ "$status" -eq 0 ]
}

@test "package.restart leaves immich in running state" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  run rpc_result package.restart '{"id":"immich"}'
  [ "$status" -eq 0 ]
  # Restart = ordered stop+start of the whole 3-container stack (postgres→redis→
  # server, with the server doing DB-readiness + migrations on boot), so it needs
  # at least as long as `start` (180s) — more, since it stops first. The old 120s
  # was inconsistent with the start test and false-failed on heavily-loaded nodes.
  run wait_for_container_status immich running 240
  [ "$status" -eq 0 ]
}

# ────────────────────────────────────────────────────────────────────
# Cascade tier (uninstall + reinstall the stack)
# ────────────────────────────────────────────────────────────────────

@test "package.uninstall removes immich (data preserved)" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"
  run rpc_result package.uninstall '{"id":"immich","preserve_data":true}'
  [ "$status" -eq 0 ]
  run wait_for_container_status immich absent 120
  [ "$status" -eq 0 ]
}

@test "package.install immich returns to running" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"
  run rpc_result package.install "{\"id\":\"immich\",\"dockerImage\":\"${IMMICH_IMAGE}\"}"
  [ "$status" -eq 0 ]
  run wait_for_container_status immich running 180
  [ "$status" -eq 0 ]
}
