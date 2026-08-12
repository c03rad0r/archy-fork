#!/usr/bin/env bats
# tests/lifecycle/bats/all-apps-lifecycle.bats
#
# DESTRUCTIVE per-app lifecycle matrix across EVERY installed app (breadth) —
# the active counterpart to the read-only all-apps-matrix.bats and the ~8 deep
# per-app suites. For each installed, NON-protected app it drives:
#   stop → verify stopped → start → verify running → restart → verify running
# and, when ARCHY_ALLOW_CASCADE_DESTRUCTIVE=1, a FULL TEARDOWN:
#   uninstall (full, removes data) → verify GONE from My Apps (no #13 ghost) →
#   reinstall from the node catalog → verify running.
#
# Reinstall spec source: the node catalog (default /opt/archipelago/web-ui/
# catalog.json), whose `.apps[]` entries carry {dockerImage, containerConfig} —
# exactly what package.install needs. Multi-container stacks (immich, mempool,
# netbird, btcpay, indeedhub) ignore dockerImage internally but still require it,
# and route to their orchestrator/stack handler; the catalog entry is enough to
# trigger the reinstall. An app with no catalog entry is skipped (logged), not
# failed — there's no spec to reinstall it from.
#
# ── PROTECTED apps (NEVER touched — neither cycled nor torn down) ────────────
#   - chain state, expensive to resync:   bitcoin*, electrumx/electrs
#   - WALLET / financial state, teardown = IRREVERSIBLE fund/credential loss:
#                                          lnd, btcpay*, fedimint*
#   The user asked to protect only bitcoin + electrum; the wallet-bearing apps
#   are protected by DEFAULT here for safety (a full uninstall destroys their
#   seed/channel/guardian state). Override the entire set with
#   ARCHY_MATRIX_PROTECT="space separated ids" to tear them down too — you WILL
#   lose their data.
#
# ── Gating ──────────────────────────────────────────────────────────────────
#   lifecycle tier  → ARCHY_ALLOW_DESTRUCTIVE=1
#   teardown  tier  → ARCHY_ALLOW_CASCADE_DESTRUCTIVE=1
#   Both skip otherwise, so this file is inert in a normal run. ON-NODE ONLY
#   (reads catalog.json on disk + drives the local package lifecycle).
#
# This is a HEAVY suite: a full teardown of ~15-20 apps re-pulls images and can
# run for a long time. Intended as an explicit, supervised coverage pass, not a
# per-iteration gate step.

load '../lib/rpc.bash'

CATALOG="${ARCHY_CATALOG:-/opt/archipelago/web-ui/catalog.json}"

# Protected — see header. Override with ARCHY_MATRIX_PROTECT to change the set.
PROTECT="${ARCHY_MATRIX_PROTECT:-bitcoin-knots bitcoin-core bitcoin electrumx electrs mempool-electrs lnd btcpay-server btcpayserver btcpay fedimint fedimint-clientd fedimint-gateway}"

setup_file() {
  : "${ARCHY_PASSWORD:?Set ARCHY_PASSWORD env var to the UI password}"
  export ARCHY_FORCE_LOGIN=1
  rpc_login
  unset ARCHY_FORCE_LOGIN
}

teardown_file() {
  rpc_logout_local
}

is_protected() {
  local id="$1" p
  for p in $PROTECT; do [[ "$p" == "$id" ]] && return 0; done
  return 1
}

get_package_data() {
  rpc_result server.get-state '{}' 2>/dev/null | jq -c '.data["package-data"] // {}'
}

# Canonical app ids the catalog can (re)install.
catalog_ids() {
  jq -r '(.apps // [])[].id' "$CATALOG" 2>/dev/null
}

# Installed primary apps we will exercise: catalog ids present in My Apps,
# minus the protected set. (Catalog-scoped so we skip sub-containers like
# immich_postgres that surface as their own package-data entries.)
target_apps() {
  local pd; pd=$(get_package_data)
  local id
  for id in $(catalog_ids); do
    echo "$pd" | jq -e --arg i "$id" 'has($i)' >/dev/null 2>&1 || continue
    is_protected "$id" && continue
    echo "$id"
  done
}

# Top-level state of an app in My Apps, or "absent" when the entry is gone.
app_state() {
  get_package_data | jq -r --arg i "$1" '.[$i].state // "absent"'
}

# Poll My Apps until app $1 reaches state $2 (or "absent"); $3 = timeout secs.
wait_state() {
  local id="$1" target="$2" timeout="${3:-180}"
  local deadline=$(( $(date +%s) + timeout ))
  while (( $(date +%s) < deadline )); do
    [[ "$(app_state "$id")" == "$target" ]] && return 0
    sleep 3
  done
  echo "wait_state: $id never reached '$target' (last='$(app_state "$id")') within ${timeout}s" >&2
  return 1
}

# Let the node drain before cycling the NEXT app.
#
# Without this the suite competes with itself. Cycling ten apps back-to-back
# drove this 4-core node from load 10.8 at preflight to 17.6 mid-loop, and the
# contention then landed on whatever ran next: btcpay's recovery stretched from
# 52s measured on a quiet box to 216s and then 512s, blowing through waits of
# 180s and 300s. Widening those waits is a losing game, because the run itself
# sets the load they have to survive — so pace the source instead.
#
# Waits for load1 to fall below the ceiling, capped so a genuinely busy node
# can't stall the suite forever. ARCHY_APP_SETTLE_SECS=0 disables it.
settle_between_apps() {
  local cap="${ARCHY_APP_SETTLE_SECS:-90}"
  (( cap > 0 )) || return 0
  local cores ceiling
  cores=$(nproc 2>/dev/null || echo 4)
  ceiling="${ARCHY_APP_SETTLE_LOAD:-$(( cores * 2 ))}"
  local deadline=$(( $(date +%s) + cap )) load1
  while (( $(date +%s) < deadline )); do
    load1=$(awk '{print $1}' /proc/loadavg)
    awk -v l="$load1" -v m="$ceiling" 'BEGIN { exit !(l < m) }' && return 0
    sleep 5
  done
  echo "# settle: load1 $load1 still >= $ceiling after ${cap}s — continuing anyway" >&3
}

# Build a package.install payload for $1 from the catalog, or fail (no spec).
catalog_install_payload() {
  local id="$1" img cfg
  img=$(jq -r --arg i "$id" '(.apps // [])[] | select(.id==$i) | .dockerImage // empty' "$CATALOG")
  [[ -n "$img" ]] || return 1
  cfg=$(jq -c --arg i "$id" '(.apps // [])[] | select(.id==$i) | .containerConfig // null' "$CATALOG")
  if [[ "$cfg" == "null" ]]; then
    jq -nc --arg id "$id" --arg img "$img" '{id:$id, dockerImage:$img}'
  else
    jq -nc --arg id "$id" --arg img "$img" --argjson cfg "$cfg" '{id:$id, dockerImage:$img, containerConfig:$cfg}'
  fi
}

# ────────────────────────────────────────────────────────────────────
@test "prerequisites: catalog present and at least one target app" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  [[ -f "$CATALOG" ]] || { echo "# catalog not found: $CATALOG" >&3; false; }
  run target_apps
  [ "$status" -eq 0 ]
  [ -n "$output" ] || { echo "# no non-protected installed apps to exercise" >&3; false; }
  echo "# protected (skipped): $PROTECT" >&3
  echo "# targets ($(echo "$output" | wc -w)): $(echo $output)" >&3
}

@test "lifecycle: stop → start → restart every non-protected app" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  local fails="" id first=1
  for id in $(target_apps); do
    [[ "$(app_state "$id")" == "running" ]] || continue   # only cycle running apps
    # Drain before each app after the first. Placed at the TOP of the body so it
    # still runs when a previous iteration bailed out via `continue`.
    (( first )) && first=0 || settle_between_apps
    # Each rpc_result must be allowed to fail without aborting the loop.
    # rpc_result returns non-zero whenever the response carries .error, and
    # under bats' errexit a bare call ends the test right there — so a single
    # transient RPC hiccup killed the run BEFORE the $fails summary below could
    # name the app. That is exactly what happened on 2026-08-08: the whole test
    # died at package.stop with no indication of which of the ten targets it was
    # (it was mempool, and the same call succeeded by hand moments later).
    rpc_result package.stop "{\"id\":\"$id\"}" >/dev/null 2>&1 \
      || { fails+="$id:stop-rpc "; continue; }
    wait_state "$id" stopped 120 || { fails+="$id:stop "; }
    rpc_result package.start "{\"id\":\"$id\"}" >/dev/null 2>&1 \
      || { fails+="$id:start-rpc "; continue; }
    wait_state "$id" running 240 || { fails+="$id:start "; continue; }
    rpc_result package.restart "{\"id\":\"$id\"}" >/dev/null 2>&1 \
      || { fails+="$id:restart-rpc "; continue; }
    wait_state "$id" running 240 || { fails+="$id:restart "; }
  done
  [[ -z "$fails" ]] || { echo "# lifecycle failures: $fails" >&3; false; }
}

@test "teardown: full uninstall (no ghost) → reinstall every non-protected app" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"
  local fails="" skipped="" id payload
  for id in $(target_apps); do
    if ! payload=$(catalog_install_payload "$id"); then
      skipped+="$id "
      continue
    fi
    rpc_result package.uninstall "{\"id\":\"$id\"}" >/dev/null 2>&1
    # No ghost: the entry must leave My Apps (the #13 class). 71cc9ac4 bounds the
    # teardown so this can no longer hang indefinitely.
    if ! wait_state "$id" absent 300; then
      fails+="$id:ghost "
      continue
    fi
    rpc_result package.install "$payload" >/dev/null 2>&1
    wait_state "$id" running 420 || fails+="$id:reinstall "
  done
  [[ -n "$skipped" ]] && echo "# skipped (no catalog spec to reinstall from): $skipped" >&3
  [[ -z "$fails" ]] || { echo "# teardown failures: $fails" >&3; false; }
}
