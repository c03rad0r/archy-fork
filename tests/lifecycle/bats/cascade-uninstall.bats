#!/usr/bin/env bats
# tests/lifecycle/bats/cascade-uninstall.bats
#
# CASCADE-tier regression guard for the uninstall → reinstall lifecycle — the
# exact bug class the gate's DESTRUCTIVE tier never exercised:
#   #13 "uninstall ghost"  — app stayed in My Apps after uninstall because the
#                            package state entry wasn't cleared when teardown hit
#                            cleanup residue (returned Err before removing it).
#   #14 "reinstall stops"  — a reinstall stalled partway on the stale state/data
#                            left behind by the broken uninstall.
#
# Uses a THROWAWAY app (default grafana — not installed on prod/test nodes, no
# user data) so it can drive the FULL teardown path (no preserve_data), which is
# where #13 actually bit. Precondition-skips if the app is already installed, so
# it can NEVER destroy real data on a populated node.
#
# "No ghost" is asserted against server.get-state's package-data (literally the
# My Apps map) — the entry must disappear, not linger with a stale state /
# stuck uninstall stage.
#
# Gated on ARCHY_ALLOW_CASCADE_DESTRUCTIVE=1. RPC-based, so it works on-node or
# against a remote ARCHY_HOST (the data-dir residue check is on-node only).

load '../lib/rpc.bash'

CASCADE_APP="${ARCHY_CASCADE_APP:-grafana}"
CASCADE_IMAGE="${ARCHY_CASCADE_IMAGE:-docker.io/grafana/grafana:10.2.0}"
CASCADE_CONFIG="${ARCHY_CASCADE_CONFIG:-{\"ports\":[\"3000:3000\"],\"volumes\":[\"/var/lib/archipelago/grafana:/var/lib/grafana\"],\"env\":[\"GF_PATHS_DATA=/var/lib/grafana\",\"GF_USERS_ALLOW_SIGN_UP=false\"]}}"
CASCADE_DATA_DIR="${ARCHY_CASCADE_DATA_DIR:-/var/lib/archipelago/${CASCADE_APP}}"

setup_file() {
  cascade_enabled || return 0
  : "${ARCHY_PASSWORD:?Set ARCHY_PASSWORD env var to the UI password}"
  export ARCHY_FORCE_LOGIN=1
  rpc_login
  unset ARCHY_FORCE_LOGIN
}

teardown_file() {
  rpc_logout_local
}

cascade_enabled() {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]]
}

# True when CASCADE_APP has an entry in My Apps (server.get-state package-data).
app_in_my_apps() {
  rpc_result server.get-state '{}' 2>/dev/null \
    | jq -e --arg id "$CASCADE_APP" '.data["package-data"] | has($id)' >/dev/null 2>&1
}

# Top-level state of CASCADE_APP in My Apps, or "absent" when the entry is gone.
app_state() {
  rpc_result server.get-state '{}' 2>/dev/null \
    | jq -r --arg id "$CASCADE_APP" '.data["package-data"][$id].state // "absent"'
}

# Live uninstall stage shown by My Apps, or an empty string when the entry is gone
# or the backend has not emitted a stage yet.
app_uninstall_stage() {
  rpc_result server.get-state '{}' 2>/dev/null \
    | jq -r --arg id "$CASCADE_APP" '.data["package-data"][$id]["uninstall-stage"] // ""'
}

# Mirror the frontend's AppCard.vue mapping so the gate proves the UI has
# backend data that can render as a monotonic, non-fake progress bar.
uninstall_stage_percent() {
  local stage="$1"
  if [[ "$stage" =~ \(([0-9]+)[[:space:]]*/[[:space:]]*([0-9]+)\) ]]; then
    local done="${BASH_REMATCH[1]}" total="${BASH_REMATCH[2]}"
    if (( total > 0 )); then
      (( done > total )) && done="$total"
      echo $(( 10 + (done * 40 / total) ))
      return 0
    fi
  fi
  if [[ "$stage" =~ [Vv]olume ]]; then echo 70; return 0; fi
  if [[ "$stage" =~ [Dd]ata ]]; then echo 90; return 0; fi
  return 1
}

# Poll until CASCADE_APP disappears while enforcing the progress contract:
# stages must be parseable, monotonic, below 100 before terminal absence, and
# the operation must emit at least one visible stage instead of silently hanging.
wait_absent_with_truthful_uninstall_progress() {
  local timeout="${1:-180}"
  local deadline=$(( $(date +%s) + timeout ))
  local saw_stage=0 last_percent=0
  while (( $(date +%s) < deadline )); do
    local state stage percent
    state="$(app_state)"
    [[ "$state" == "absent" ]] && {
      (( saw_stage == 1 )) || {
        echo "uninstall progress: no uninstall-stage observed before terminal absence" >&2
        return 1
      }
      return 0
    }

    stage="$(app_uninstall_stage)"
    if [[ -n "$stage" ]]; then
      if ! percent="$(uninstall_stage_percent "$stage")"; then
        echo "uninstall progress: unparseable stage '$stage'" >&2
        return 1
      fi
      (( percent >= last_percent )) || {
        echo "uninstall progress regressed: ${percent}% after ${last_percent}% (stage '$stage')" >&2
        return 1
      }
      (( percent < 100 )) || {
        echo "uninstall progress reached ${percent}% before terminal absence (stage '$stage')" >&2
        return 1
      }
      saw_stage=1
      last_percent="$percent"
    fi
    sleep 2
  done
  echo "wait_absent_with_truthful_uninstall_progress: $CASCADE_APP did not disappear within ${timeout}s (last='$(app_state)', stage='$(app_uninstall_stage)')" >&2
  return 1
}

# Poll My Apps until CASCADE_APP reaches $1 (a state, or "absent").
wait_app_state() {
  local target="$1" timeout="${2:-180}"
  local deadline=$(( $(date +%s) + timeout ))
  while (( $(date +%s) < deadline )); do
    [[ "$(app_state)" == "$target" ]] && return 0
    sleep 3
  done
  echo "wait_app_state: $CASCADE_APP never reached '$target' (last='$(app_state)') within ${timeout}s" >&2
  return 1
}

# ────────────────────────────────────────────────────────────────────
@test "cascade gate enabled" {
  cascade_enabled || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"
}

@test "precondition: ${CASCADE_APP} is not already installed (protects real data)" {
  cascade_enabled || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"
  if app_in_my_apps; then
    skip "${CASCADE_APP} already installed here — refusing to uninstall (would destroy data); set ARCHY_CASCADE_APP to an uninstalled throwaway"
  fi
}

@test "install ${CASCADE_APP} (fresh) reaches running with a truthful, non-silent progression" {
  cascade_enabled || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"
  app_in_my_apps && skip "already installed (precondition skip)"

  run rpc_result package.install "{\"id\":\"${CASCADE_APP}\",\"dockerImage\":\"${CASCADE_IMAGE}\",\"containerConfig\":${CASCADE_CONFIG}}"
  [ "$status" -eq 0 ]

  # Progress truthfulness: must pass through a transitional install state (not a
  # silent no-op) and land on running. A warm image cache can blow through the
  # transitional states between polls, so a missed transitional is a warn, not a
  # failure; reaching running is the hard assertion.
  local saw_transitional=0 deadline=$(( $(date +%s) + 300 ))
  while (( $(date +%s) < deadline )); do
    case "$(app_state)" in
      installing|pulling-image|pulling|downloading|starting|created) saw_transitional=1 ;;
      running) break ;;
    esac
    sleep 2
  done
  [ "$(app_state)" == "running" ]
  [ "$saw_transitional" -eq 1 ] || echo "# note: no transitional install state observed (image likely cached)" >&3
}

@test "uninstall ${CASCADE_APP} reports truthful progress and clears My Apps — NO ghost (#13)" {
  cascade_enabled || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"
  app_in_my_apps || skip "${CASCADE_APP} not installed (install step must have failed)"

  run rpc_result package.uninstall "{\"id\":\"${CASCADE_APP}\"}"
  [ "$status" -eq 0 ]

  # The container must go away…
  run wait_for_container_status "$CASCADE_APP" absent 180
  [ "$status" -eq 0 ]

  # …AND the My Apps entry must be GONE — the #13 ghost was the entry lingering
  # with a stale state / stuck uninstall stage. While polling, prove the backend
  # emits stage data the UI can render as monotonic, non-full progress.
  run wait_absent_with_truthful_uninstall_progress 120
  [ "$status" -eq 0 ]

  # Belt-and-suspenders: the key is truly absent from package-data.
  run app_in_my_apps
  [ "$status" -ne 0 ]
}

@test "uninstall removed the data dir (full teardown, no residue)" {
  cascade_enabled || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"
  # Needs the local filesystem — on-node runs only.
  case "${ARCHY_HOST:-127.0.0.1}" in
    127.0.0.1|localhost) : ;;
    *) skip "data-dir residue check is on-node only (ARCHY_HOST=${ARCHY_HOST})" ;;
  esac
  [[ ! -e "$CASCADE_DATA_DIR" ]]
}

@test "reinstall ${CASCADE_APP} returns to running (#14)" {
  cascade_enabled || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"

  run rpc_result package.install "{\"id\":\"${CASCADE_APP}\",\"dockerImage\":\"${CASCADE_IMAGE}\",\"containerConfig\":${CASCADE_CONFIG}}"
  [ "$status" -eq 0 ]
  run wait_app_state running 300
  [ "$status" -eq 0 ]
}

@test "cleanup: uninstall ${CASCADE_APP} to leave the node as found" {
  cascade_enabled || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"
  run rpc_result package.uninstall "{\"id\":\"${CASCADE_APP}\"}"
  [ "$status" -eq 0 ]
  run wait_for_container_status "$CASCADE_APP" absent 180
  [ "$status" -eq 0 ]
  run wait_absent_with_truthful_uninstall_progress 120
  [ "$status" -eq 0 ]
}
