#!/usr/bin/env bats
# tests/lifecycle/bats/bitcoin-receive.bats
#
# Regression coverage for the Bitcoin "Receive" flow. Receive addresses come
# from LND's hot wallet via the `lnd.newaddress` RPC, so this exercises the
# exact path that broke on the fleet:
#   - .116: LND REST published on the wrong host port (8080 vs the manifest's
#           18080) -> connection refused -> receive failed with the generic
#           "Operation failed. Check server logs." message.
#   - .228: the same family surfaced to the UI as a *false* "wallet is locked".
#
# These tests run on the archy host (they shell into podman / curl localhost).
#
# Tiers: read-only only — generating a receive address is non-destructive.

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

# Resolve the LND REST host port from the manifest (single source of truth) so
# this test follows the manifest rather than hard-coding 18080.
_lnd_rest_host_port() {
  local mf
  for mf in \
    "${ARCHIPELAGO_APPS_DIR:-/opt/archipelago/apps}/lnd/manifest.yml" \
    "${ARCHIPELAGO_APPS_DIR:-/opt/archipelago/apps}/lnd/manifest.yaml" \
    "$BATS_TEST_DIRNAME/../../../apps/lnd/manifest.yml"; do
    [[ -r "$mf" ]] || continue
    # The REST mapping is the `- host: <N>` whose following `container:` is 8080.
    awk '
      /- host:/        { host=$3 }
      /container:/     { if ($2 == 8080 && host != "") { print host; exit } }
    ' "$mf"
    return 0
  done
}

_lnd_running() {
  rpc_result container-list 2>/dev/null \
    | jq -e '.[] | select(.name == "lnd" and .state == "running")' >/dev/null 2>&1
}

# ────────────────────────────────────────────────────────────────────
# Read-only tier
# ────────────────────────────────────────────────────────────────────

@test "LND REST is reachable on the manifest host port (catches port drift)" {
  _lnd_running || skip "lnd not running"
  local port
  port=$(_lnd_rest_host_port)
  [[ -n "$port" ]] || skip "could not resolve LND REST host port from manifest"

  # A TCP connect is enough: drift (container published on a different host
  # port) shows up as connection-refused here, exactly as on .116.
  run curl -sk -o /dev/null --max-time 8 "https://127.0.0.1:${port}/v1/getinfo"
  if [ "$status" -ne 0 ]; then
    echo "LND REST not reachable on host port ${port} (curl exit $status) — likely published-port drift" >&2
    return 1
  fi
}

@test "lnd.newaddress returns a bech32 address when lnd is running" {
  _lnd_running || skip "lnd not running"

  # The bitcoin bounce in bitcoin-knots.bats cascade-restarts lnd (24fd97ed).
  # Depending on where the probe lands in lnd's startup it sees a different
  # transient code — REST unreachable, gRPC "waiting to start" (mapped to
  # LND_ERROR), wallet locked until the auto-unlocker gets through, or a
  # post-unlock sync phase. All of those are the node settling, not broken —
  # retry until the deadline below (run A caught WALLET_LOCKED, run B caught LND_ERROR
  # while lnd was seconds into its restart; both self-healed within a couple
  # of minutes). Only LND_WALLET_UNINITIALIZED (no wallet — never self-heals)
  # fails immediately, and anything still erroring after the window fails
  # loudly below.
  # 420s, not 180s: the window starts when this TEST starts, but the lnd
  # restart that locks the wallet can land partway into it. On 2026-08-08 the
  # restart hit 65s in and the wallet unlocked at 2m25s (journal: lnd.service
  # started 20:11:05, "wallet has been unlocked without a time limit"
  # 20:13:48) — 48s after this deadline expired, so the test reported
  # LND_WALLET_LOCKED on a node that was fine. The daemon's own unlock budget
  # is ~10 min (UNLOCK_NOT_READY_ATTEMPTS=600) because opening the channel and
  # graph dbs takes minutes on a loaded box, so anything under that is the test
  # being stricter than the product it is testing.
  local deadline=$((SECONDS + ${ARCHY_LND_UNLOCK_SECS:-420})) err addr
  while :; do
    run rpc_call lnd.newaddress
    [ "$status" -eq 0 ]
    err=$(echo "$output" | jq -r '.error.message // .error // empty')
    addr=$(echo "$output" | jq -r '.result.address // empty')
    [[ -n "$err" && "$err" != *LND_WALLET_UNINITIALIZED* && $SECONDS -lt $deadline ]] || break
    sleep 10
  done

  # The whole point of the fix: a running lnd must hand back a real address.
  if [[ -n "$err" ]]; then
    echo "lnd.newaddress errored on a running node: $err" >&2
    return 1
  fi
  if [[ "$addr" != bc1* ]]; then
    echo "expected a bech32 (bc1…) address, got: '$addr'" >&2
    return 1
  fi
}

@test "receive errors are specific, never the generic catch-all" {
  # Even when receive legitimately can't produce an address, the message must be
  # actionable (start with 'Bitcoin address' and/or carry a [CODE] token) — the
  # generic 'Operation failed' is what hid the real cause on .116.
  run rpc_call lnd.newaddress
  [ "$status" -eq 0 ]
  local err
  err=$(echo "$output" | jq -r '.error.message // .error // empty')
  if [[ "$err" == "Operation failed. Check server logs for details." ]]; then
    echo "receive returned the generic catch-all instead of a specific reason" >&2
    return 1
  fi
}
