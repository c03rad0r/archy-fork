#!/usr/bin/env bats
# tests/lifecycle/bats/lnd.bats
#
# Lifecycle tests for the lnd package. Mirrors bitcoin-knots.bats so the
# 5× release-gate run exercises lnd through the same state matrix.
#
# Tiers:
#   - Read-only (always runs):        presence, state-reporting consistency, RPC reachable
#   - Destructive (ARCHY_ALLOW_DESTRUCTIVE=1): stop → start → restart
#   - Cascade-destructive (ARCHY_ALLOW_CASCADE_DESTRUCTIVE=1): uninstall → reinstall
#
# Pre-req: lnd is installed. Reinstall path is gated separately because it
# wipes the wallet macaroons and forces re-onboarding.

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

# ────────────────────────────────────────────────────────────────────
# Read-only tier
# ────────────────────────────────────────────────────────────────────

@test "container-list includes lnd" {
  run rpc_result container-list
  [ "$status" -eq 0 ]
  echo "$output" | jq -e '.[] | select(.name == "lnd")' >/dev/null
}

@test "container-list reports a valid state for lnd" {
  run rpc_result container-list
  [ "$status" -eq 0 ]
  local state
  state=$(echo "$output" | jq -r '.[] | select(.name == "lnd") | .state')
  [[ "$state" =~ ^(running|stopped|exited|created|paused)$ ]]
}

@test "lnd cli getinfo succeeds when lnd is running" {
  local state
  state=$(rpc_result container-list | jq -r '.[] | select(.name == "lnd") | .state')
  if [[ "$state" != "running" ]]; then
    skip "lnd not running (state=$state)"
  fi

  # lnd's RPC readiness LAGS the container "running" state: after a (re)start the
  # wallet must auto-unlock before lncli answers, so a single-shot getinfo races
  # that window and false-fails. Retry until ready (~90s), like a health probe.
  # `timeout 10` per attempt: a wedged lnd RPC (e.g. chain-blind after its
  # bitcoin backend was recreated under it, .228 2026-07-08) otherwise hangs
  # a single exec — and with it the whole suite — indefinitely.
  run sh -lc 'for i in $(seq 1 80); do
    timeout 10 podman exec lnd lncli \
      --tlscertpath /root/.lnd/tls.cert \
      --macaroonpath /root/.lnd/data/chain/bitcoin/mainnet/readonly.macaroon \
      --rpcserver localhost:10009 getinfo >/dev/null 2>&1 && exit 0
    sleep 3
  done; exit 1'
  [ "$status" -eq 0 ]
}

@test "no orphan lnd-related containers beyond the known set" {
  # FM4 guard: rolling updates have left ghost containers behind in the past.
  # Known-good lnd-package container set is {lnd, archy-lnd-ui}.
  local total known
  total=$(podman ps -a --format '{{.Names}}' | grep -Ec '^(archy-)?lnd(-[a-z]+)?$' || true)
  known=$(podman ps -a --format '{{.Names}}' | grep -Ec '^(lnd|archy-lnd-ui)$' || true)
  [ "$total" -eq "$known" ]
}

# ────────────────────────────────────────────────────────────────────
# Destructive tier (stop → start → restart on the same container)
# ────────────────────────────────────────────────────────────────────

@test "package.stop transitions lnd to stopped" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"

  run rpc_result package.stop '{"id":"lnd"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status lnd stopped 60
  [ "$status" -eq 0 ]
}

@test "package.start brings lnd back to running" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"

  run rpc_result package.start '{"id":"lnd"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status lnd running 240
  [ "$status" -eq 0 ]
}

@test "package.restart leaves lnd in running state" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"

  run rpc_result package.restart '{"id":"lnd"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status lnd running 240
  [ "$status" -eq 0 ]
}

@test "lncli getinfo recovers after restart" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"

  # lnd takes longer than bitcoind to accept RPC after cold restart because
  # the wallet has to be unlocked first, then it reconnects to bitcoind and
  # re-syncs the graph. On a loaded node this exceeds 90s (observed ~2min on
  # .228, then synced_to_chain:true). Give it 240s.
  local deadline=$(( $(date +%s) + 240 ))
  while (( $(date +%s) < deadline )); do
    if sh -lc 'podman exec lnd lncli \
        --tlscertpath /root/.lnd/tls.cert \
        --macaroonpath /root/.lnd/data/chain/bitcoin/mainnet/readonly.macaroon \
        --rpcserver localhost:10009 getinfo >/dev/null' 2>/dev/null; then
      return 0
    fi
    sleep 3
  done
  fail "lncli getinfo never recovered after restart"
}

# ────────────────────────────────────────────────────────────────────
# Cascade-destructive tier (uninstall + reinstall)
# ────────────────────────────────────────────────────────────────────

@test "package.uninstall removes lnd" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"

  run rpc_result package.uninstall '{"id":"lnd","preserve_data":true}'
  [ "$status" -eq 0 ]

  run wait_for_container_status lnd absent 120
  [ "$status" -eq 0 ]
}

@test "package.install lnd returns to running" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"

  run rpc_result package.install '{"manifest_path":"lnd/manifest.yaml"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status lnd running 180
  [ "$status" -eq 0 ]
}
