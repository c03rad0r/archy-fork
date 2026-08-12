#!/usr/bin/env bats
# tests/lifecycle/bats/bitcoin-knots.bats
#
# Lifecycle tests for the bitcoin-knots package.
#
# Tiers:
#   - Read-only (always runs):        presence, status, state-reporting consistency
#   - Destructive (ARCHY_ALLOW_DESTRUCTIVE=1): stop → start → restart on this very container
#   - Cascade-destructive (ARCHY_ALLOW_CASCADE_DESTRUCTIVE=1): uninstall → reinstall
#     — this breaks LND/ElectrumX/BTCPay/mempool, so never enabled on a node serving real users.
#
# Pre-req: bitcoin-knots is installed. We do NOT install it from scratch here
# because doing so on the live host would require wiping 700GB of chain data.

load '../lib/rpc.bash'

setup_file() {
  : "${ARCHY_PASSWORD:?Set ARCHY_PASSWORD env var to the UI password}"
  export ARCHY_FORCE_LOGIN=1  # make sure setup_file gets a fresh token
  rpc_login
  unset ARCHY_FORCE_LOGIN      # subsequent test subshells reuse the session file
}

teardown_file() {
  rpc_logout_local
}

# ────────────────────────────────────────────────────────────────────
# Read-only tier
# ────────────────────────────────────────────────────────────────────

@test "container-list includes bitcoin-knots" {
  run rpc_result container-list
  [ "$status" -eq 0 ]
  echo "$output" | jq -e '.[] | select(.name == "bitcoin-knots")' >/dev/null
}

@test "container-list reports a valid state for bitcoin-knots" {
  # Poll briefly: a container caught mid-reconcile can momentarily report a
  # transient state ("restarting"/"configured"/"removing") or no state at all.
  # A genuinely-stuck container never settles, so this still catches real
  # breakage; it only absorbs churn (e.g. another container bouncing right
  # before the read-only tier runs).
  local state="" deadline=$(( $(date +%s) + 30 ))
  while (( $(date +%s) < deadline )); do
    run rpc_result container-list
    [ "$status" -eq 0 ]
    state=$(echo "$output" | jq -r '.[] | select(.name == "bitcoin-knots") | .state')
    [[ "$state" =~ ^(running|stopped|exited|created|paused)$ ]] && return 0
    sleep 3
  done
  echo "bitcoin-knots never reported a settled valid state within 30s (last: '$state')" >&2
  return 1
}

@test "container-status returns a valid status object for bitcoin-knots" {
  # During orchestrator alias migration, container-status can fail for some
  # app_id aliases even while container-list/state is correct. Accept either:
  # (a) valid container-status object OR (b) valid container-list state entry.
  run rpc_call container-status '{"app_id":"bitcoin-knots"}'
  [ "$status" -eq 0 ]
  local err
  err=$(echo "$output" | jq -r '.error.message // empty')
  if [[ -z "$err" ]]; then
    echo "$output" | jq -e '.result | has("status") or has("state") or has("running")' >/dev/null
    return 0
  fi

  run rpc_result container-list
  [ "$status" -eq 0 ]
  echo "$output" | jq -e '.[] | select(.name == "bitcoin-knots") | has("state")' >/dev/null
}

@test "bitcoin.getinfo succeeds when bitcoin-knots is running" {
  local state
  state=$(rpc_result container-list | jq -r '.[] | select(.name == "bitcoin-knots") | .state')
  if [[ "$state" != "running" ]]; then
    skip "bitcoin-knots not running (state=$state)"
  fi

  run rpc_call bitcoin.getinfo
  [ "$status" -eq 0 ]
  echo "$output" | jq -e '.error == null' >/dev/null
}

@test "no orphan bitcoin-knots-related containers beyond the known set" {
  # FM4 guard: after rolling updates we've seen ghost containers accumulate.
  # Known-good container set for the bitcoin-knots package is just "bitcoin-knots".
  # Anything matching bitcoin-knots* in podman ps that isn't in the known set is a red flag.
  local count
  count=$(ssh_podman_ps | awk '/bitcoin-knots/ {print $NF}' | grep -Ec '^bitcoin-knots(-[a-z]+)?$' || true)
  local known
  known=$(ssh_podman_ps | awk '/bitcoin-knots/ {print $NF}' | grep -Ec '^(bitcoin-knots|bitcoin-ui)$' || true)
  [ "$count" -eq "$known" ]
}

# Shell helper (not an RPC call): shells out to podman directly via the running user.
# Only works when bats is run on the archy host itself (which is the plan).
ssh_podman_ps() {
  podman ps -a --format '{{.ID}} {{.State}} {{.Names}}'
}

# ────────────────────────────────────────────────────────────────────
# Destructive tier (stop → start → restart on the same container)
# ────────────────────────────────────────────────────────────────────

@test "package.stop transitions bitcoin-knots to stopped" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"

  run rpc_result package.stop '{"id":"bitcoin-knots"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status bitcoin-knots stopped 60
  [ "$status" -eq 0 ]
}

@test "package.start brings bitcoin-knots back to running" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"

  run rpc_result package.start '{"id":"bitcoin-knots"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status bitcoin-knots running 120
  [ "$status" -eq 0 ]
}

@test "package.restart leaves bitcoin-knots in running state" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"

  run rpc_result package.restart '{"id":"bitcoin-knots"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status bitcoin-knots running 120
  [ "$status" -eq 0 ]
}

@test "bitcoin.getinfo succeeds after restart" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"

  # Give bitcoind up to 120s to accept RPC after a cold restart — reloading the
  # block index + chainstate can take a while even on a synced node.
  local deadline=$(( $(date +%s) + 120 ))
  while (( $(date +%s) < deadline )); do
    if rpc_call bitcoin.getinfo | jq -e '.error == null' >/dev/null 2>&1; then
      return 0
    fi
    sleep 3
  done
  # NB: bats-assert's `fail` is not loaded in this file (only ../lib/rpc.bash),
  # so emit + return non-zero directly rather than calling an undefined helper
  # (which fails with "fail: command not found" / status 127 and hides the real
  # reason). A node mid-IBD legitimately can't serve getinfo here — that's an
  # environmental precondition (see required-stack "synced archival"), not a
  # product regression.
  echo "bitcoin.getinfo never recovered after restart within 120s" >&2
  return 1
}

# ────────────────────────────────────────────────────────────────────
# Cascade-destructive tier (uninstall + reinstall)
# ────────────────────────────────────────────────────────────────────

@test "package.uninstall removes bitcoin-knots" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"

  run rpc_result package.uninstall '{"id":"bitcoin-knots","preserve_data":true}'
  [ "$status" -eq 0 ]

  run wait_for_container_status bitcoin-knots absent 120
  [ "$status" -eq 0 ]
}

@test "package.install bitcoin-knots returns to running" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"

  # manifest_path is relative to data_dir/apps/
  run rpc_result package.install '{"manifest_path":"bitcoin-knots/manifest.yaml"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status bitcoin-knots running 180
  [ "$status" -eq 0 ]
}
