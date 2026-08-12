#!/usr/bin/env bats
# tests/lifecycle/bats/companion-survives-archipelago-restart.bats
#
# Quadlet promise: companion UIs (archy-bitcoin-ui, archy-lnd-ui,
# archy-electrs-ui) are managed by systemd, not archipelago. Restarting
# the archipelago user service must NOT take them down.
#
# This is the regression gate for the .228 incident in
# feedback_container_lifecycle_failure_modes.md (FM1: companions vanished
# from `podman ps -a` after archipelago crash-loop).
#
# Gated by ARCHY_ALLOW_DESTRUCTIVE=1 because it bounces archipelago.

companion_units=(
  "archy-bitcoin-ui"
  "archy-lnd-ui"
  "archy-electrs-ui"
)

unit_dir="$HOME/.config/containers/systemd"

unit_file_present() {
  local name="$1"
  [[ -f "$unit_dir/$name.container" ]]
}

service_active() {
  local name="$1"
  systemctl --user is-active --quiet "$name.service"
}

container_running() {
  local name="$1"
  [[ "$(podman inspect --format '{{.State.Running}}' "$name" 2>/dev/null)" == "true" ]]
}

wait_service_active() {
  local name="$1"
  local timeout="${2:-60}"
  local deadline=$(( $(date +%s) + timeout ))
  while (( $(date +%s) < deadline )); do
    if service_active "$name"; then
      return 0
    fi
    sleep 2
  done
  return 1
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

@test "every installed companion has a quadlet unit on disk" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  local present=0
  for c in "${companion_units[@]}"; do
    if container_running "$c"; then
      run unit_file_present "$c"
      [ "$status" -eq 0 ]
      present=$(( present + 1 ))
    fi
  done
  (( present > 0 )) || skip "No companions installed on this node"
}

@test "every installed companion service is active before restart" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  for c in "${companion_units[@]}"; do
    if container_running "$c"; then
      run service_active "$c"
      [ "$status" -eq 0 ]
    fi
  done
}

@test "companions survive archipelago restart" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"

  # Snapshot: which companions were up before we touched anything.
  local before=()
  for c in "${companion_units[@]}"; do
    if container_running "$c"; then
      before+=("$c")
    fi
  done
  (( ${#before[@]} > 0 )) || skip "No companions installed on this node"

  # Bounce archipelago. The user service is the production canonical name;
  # fall back to the system service for older nodes.
  if systemctl --user list-units --no-legend archipelago.service | grep -q archipelago; then
    systemctl --user restart archipelago.service
  else
    sudo systemctl restart archipelago.service
  fi

  run wait_archipelago_back 60
  [ "$status" -eq 0 ]

  # Every companion that was up before must still be up + healthy after.
  for c in "${before[@]}"; do
    run service_active "$c"
    [ "$status" -eq 0 ]
    run container_running "$c"
    [ "$status" -eq 0 ]
  done
}

@test "deleted unit file is recreated within one reconcile tick" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"

  # Pick a companion that's currently running.
  local target=""
  for c in "${companion_units[@]}"; do
    if container_running "$c"; then
      target="$c"
      break
    fi
  done
  [[ -n "$target" ]] || skip "No companions installed on this node"

  # Delete the unit file behind systemd's back. The reconciler should
  # notice and rewrite it within one 30s tick, then start the service.
  rm -f "$unit_dir/$target.container"
  systemctl --user daemon-reload >/dev/null 2>&1 || true
  systemctl --user stop "$target.service" >/dev/null 2>&1 || true

  # Allow up to two reconcile ticks (60s + grace).
  run wait_service_active "$target" 90
  [ "$status" -eq 0 ]

  run unit_file_present "$target"
  [ "$status" -eq 0 ]
}
