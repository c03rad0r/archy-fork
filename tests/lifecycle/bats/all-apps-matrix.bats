#!/usr/bin/env bats
# tests/lifecycle/bats/all-apps-matrix.bats
#
# Manifest-driven, fleet-wide lifecycle health matrix. The per-app suites
# (bitcoin-knots, lnd, mempool, immich, …) cover ~8 core apps in depth; this
# covers EVERY installed app in breadth, automatically — no hardcoded list.
#
# It derives the app set from server.get-state's package-data (the My Apps map)
# and asserts baseline health across all of them. Read-only (no destructive env
# needed), so it joins run.sh / run-gate.sh on every node and grows coverage as
# nodes install more apps.
#
# Catches, fleet-wide, the bug classes the narrow gate missed:
#   - apps STUCK in a transitional state (the #13/#14 ghost: installing/removing
#     that never settles)
#   - apps sitting in error/failed
#   - running UI apps with no reachable lan-address (generalized port-drift)

load '../lib/rpc.bash'

# Transitional states are legitimate momentarily but must not PERSIST. Steady:
# running/stopped/exited/created/paused/installed/not-installed.
TRANSITIONAL_RE='^(installing|pulling-image|pulling|downloading|removing|uninstalling|updating|starting|stopping|restarting)$'
BAD_RE='^(error|failed)$'

# Apps whose state is allowed to be non-running at rest (no UI/health expectation
# beyond "settled"). Empty by default; override via ARCHY_MATRIX_ALLOW_STOPPED
# (space-separated ids) on nodes where an app is intentionally left stopped.
ALLOW_STOPPED="${ARCHY_MATRIX_ALLOW_STOPPED:-}"

setup_file() {
  : "${ARCHY_PASSWORD:?Set ARCHY_PASSWORD env var to the UI password}"
  export ARCHY_FORCE_LOGIN=1
  rpc_login
  unset ARCHY_FORCE_LOGIN
}

teardown_file() {
  rpc_logout_local
}

# Echo the package-data object (the My Apps map) once.
get_package_data() {
  rpc_result server.get-state '{}' 2>/dev/null | jq -c '.data["package-data"] // {}'
}

# Space-separated list of installed app ids.
app_ids() {
  get_package_data | jq -r 'keys[]'
}

# ────────────────────────────────────────────────────────────────────
@test "matrix has apps to check (get-state returns a non-empty My Apps map)" {
  run app_ids
  [ "$status" -eq 0 ]
  [ -n "$output" ]
  echo "# matrix covers $(echo "$output" | wc -w) apps: $(echo $output)" >&3
}

@test "no installed app is STUCK in a transitional state (settles within window)" {
  local settle="${ARCHY_MATRIX_SETTLE_SECS:-45}"
  local deadline=$(( $(date +%s) + settle ))
  local stuck=""
  # Re-poll: a transitional state right now may just be a genuine in-progress op,
  # so only fail apps that are STILL transitional after the settle window.
  while :; do
    stuck=""
    local pd; pd=$(get_package_data)
    for id in $(echo "$pd" | jq -r 'keys[]'); do
      local st; st=$(echo "$pd" | jq -r --arg i "$id" '.[$i].state // "unknown"')
      [[ "$st" =~ $TRANSITIONAL_RE ]] && stuck+="${id}=${st} "
    done
    [[ -z "$stuck" ]] && break
    (( $(date +%s) >= deadline )) && break
    sleep 5
  done
  [[ -z "$stuck" ]] || { echo "# STUCK transitional after ${settle}s: $stuck" >&3; false; }
}

@test "no installed app is in an error/failed state" {
  local pd; pd=$(get_package_data)
  local bad=""
  for id in $(echo "$pd" | jq -r 'keys[]'); do
    local st; st=$(echo "$pd" | jq -r --arg i "$id" '.[$i].state // "unknown"')
    [[ "$st" =~ $BAD_RE ]] && bad+="${id}=${st} "
  done
  [[ -z "$bad" ]] || { echo "# error/failed apps: $bad" >&3; false; }
}

@test "every running app reports a recognized state (no empty/garbage state)" {
  local pd; pd=$(get_package_data)
  local junk=""
  for id in $(echo "$pd" | jq -r 'keys[]'); do
    local st; st=$(echo "$pd" | jq -r --arg i "$id" '.[$i].state // "unknown"')
    case "$st" in
      running|stopped|exited|created|paused|installed|not-installed|\
installing|pulling-image|pulling|downloading|removing|uninstalling|updating|starting|stopping|restarting|\
error|failed|degraded) : ;;
      *) junk+="${id}='${st}' " ;;
    esac
  done
  [[ -z "$junk" ]] || { echo "# unrecognized state values: $junk" >&3; false; }
}

@test "every running UI app exposes a lan-address (generalized port-drift)" {
  # A running app whose manifest declares a UI interface (ui=="true") must have a
  # non-null lan-address on that interface — otherwise its UI is unreachable
  # (the immich/port-drift failure mode, asserted across ALL UI apps). Poll
  # briefly to absorb the transient null seen while a container is mid-recreate.
  local deadline=$(( $(date +%s) + ${ARCHY_MATRIX_UI_SECS:-30} ))
  local missing=""
  while :; do
    missing=""
    local pd; pd=$(get_package_data)
    for id in $(echo "$pd" | jq -r 'keys[]'); do
      local st; st=$(echo "$pd" | jq -r --arg i "$id" '.[$i].state // "unknown"')
      [[ "$st" == "running" ]] || continue
      # interface keys whose manifest marks ui=="true"
      local ui_ifaces
      ui_ifaces=$(echo "$pd" | jq -r --arg i "$id" \
        '.[$i].manifest.interfaces // {} | to_entries[] | select(.value.ui=="true") | .key')
      for k in $ui_ifaces; do
        local addr
        addr=$(echo "$pd" | jq -r --arg i "$id" --arg k "$k" \
          '.[$i].installed["interface-addresses"][$k]["lan-address"] // "null"')
        [[ "$addr" == "null" || -z "$addr" ]] && missing+="${id}:${k} "
      done
    done
    [[ -z "$missing" ]] && break
    (( $(date +%s) >= deadline )) && break
    sleep 3
  done
  [[ -z "$missing" ]] || { echo "# running UI apps missing lan-address: $missing" >&3; false; }
}
