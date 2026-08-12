#!/usr/bin/env bats
# tests/lifecycle/bats/backend-survives-archipelago-restart.bats
#
# Quadlet-everywhere promise (Phase 3 of v1.7.52): backend containers
# (bitcoin-knots / lnd / electrumx) are managed by systemd via Quadlet
# units, NOT parented under archipelago.service's cgroup. Restarting the
# archipelago service must NOT take them down.
#
# This is the regression gate for FM3 (cgroup cascade SIGKILL — observed
# live on .198 on 2026-05-01: stopping archipelago.service killed every
# container in its cgroup, leaving the box in a multi-hour recovery
# loop). Until v1.7.52 Phase 3 ships, this suite is EXPECTED TO FAIL on
# fleet boxes — it serves as the executable definition of "Phase 3
# complete". Do not gate the release on it passing pre-Phase-3.
#
# Sister to companion-survives-archipelago-restart.bats which tests the
# same property for UI companions (already shipping via Quadlet since
# commit 6e716f68).
#
# Gated by ARCHY_ALLOW_DESTRUCTIVE=1 because it bounces archipelago.

# bats-core ships no `fail`; bats-assert isn't installed on the alpha fleet.
# Define the same minimal helper the other suites use (see mempool.bats) so a
# tripped assertion reports as a real test failure, not a status-127 crash.
fail() { echo "$@" >&2; return 1; }

backend_units=(
  "bitcoin-knots"
  "bitcoin-core"
  "lnd"
  "electrumx"
)

container_running() {
  local name="$1"
  [[ "$(podman inspect --format '{{.State.Running}}' "$name" 2>/dev/null)" == "true" ]]
}

wait_archipelago_back() {
  local timeout="${1:-60}"
  local deadline=$(( $(date +%s) + timeout ))
  while (( $(date +%s) < deadline )); do
    if curl -fsS -o /dev/null "http://127.0.0.1:5678/health" 2>/dev/null; then
      return 0
    fi
    sleep 2
  done
  return 1
}

@test "destructive gate enabled" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
}

@test "at least one backend container is running before restart" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  local up=0
  for c in "${backend_units[@]}"; do
    if container_running "$c"; then
      up=$(( up + 1 ))
    fi
  done
  (( up > 0 )) || skip "No backends installed on this node"
}

@test "backends survive archipelago restart" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"

  # Snapshot: which backends were up before we touched anything.
  local before=()
  for c in "${backend_units[@]}"; do
    if container_running "$c"; then
      before+=("$c")
    fi
  done
  (( ${#before[@]} > 0 )) || skip "No backends installed on this node"

  # Capture pre-restart container IDs so we can verify the SAME process
  # survives — not "the orchestrator started a fresh container after the
  # cascade SIGKILL'd the original" (which would also be a fail; FM3 is
  # specifically about losing the running container, even if the
  # orchestrator can recreate one minutes later).
  declare -A pre_id
  for c in "${before[@]}"; do
    pre_id["$c"]=$(podman inspect --format '{{.Id}}' "$c" 2>/dev/null || echo "")
  done

  # Bounce archipelago. Same approach as companion-survives-* for parity.
  if systemctl --user list-units --no-legend archipelago.service | grep -q archipelago; then
    systemctl --user restart archipelago.service
  else
    sudo systemctl restart archipelago.service
  fi

  run wait_archipelago_back 60
  [ "$status" -eq 0 ]

  # Every backend that was up before must still be up after, AND it must
  # be the SAME container instance (same .Id). A different .Id means the
  # original was killed and a fresh one was created — that's the FM3
  # failure we're catching.
  for c in "${before[@]}"; do
    run container_running "$c"
    [ "$status" -eq 0 ] || fail "backend $c died across archipelago restart (FM3 cgroup cascade)"

    local post_id
    post_id=$(podman inspect --format '{{.Id}}' "$c" 2>/dev/null || echo "")
    [[ -n "$post_id" ]] || fail "backend $c has no container id after restart"
    [[ "$post_id" == "${pre_id[$c]}" ]] \
      || fail "backend $c was recreated across archipelago restart (FM3): pre=${pre_id[$c]:0:12} post=${post_id:0:12}"
  done
}
