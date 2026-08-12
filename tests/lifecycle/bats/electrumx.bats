#!/usr/bin/env bats
# tests/lifecycle/bats/electrumx.bats
#
# Lifecycle tests for the electrumx package (containers are named
# `electrumx` + `archy-electrs-ui`). Mirrors bitcoin-knots.bats /
# lnd.bats so the 5× release-gate run exercises electrumx through
# the same state matrix.
#
# Tiers:
#   - Read-only (always runs):        presence, valid state, TCP reachable
#   - Destructive (ARCHY_ALLOW_DESTRUCTIVE=1): stop → start → restart
#   - Cascade-destructive (ARCHY_ALLOW_CASCADE_DESTRUCTIVE=1): uninstall → reinstall
#
# Pre-req: electrumx is installed and bitcoin-knots is running (electrumx
# depends on bitcoind RPC for headers).

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

# How far electrumx is behind its bitcoin daemon, from its own recent log
# ("our height: N ... daemon: M"). Prints the gap; prints nothing when no
# fresh (<30 min) sync line exists — callers must treat that as "unknown",
# never as "synced".
#
# Why this exists: ElectrumX serves NO sessions until its initial sync has
# caught up, and it flushes its DB cache at 1GB — i.e. rarely — so every
# restart discards all unflushed progress back to the last flush. On
# 2026-08-09 this node had spent 8d14h syncing largely because gate runs and
# reboots kept taking its progress away. A gate that stop/start/restarts a
# mid-sync electrumx therefore (a) honestly fails the serving probe and
# (b) actively destroys hours of sync work. Skipping WITH THE GAP NAMED is
# the truthful behaviour — this is a positively-detected syncing state, not
# the container-absent skip trap fixed earlier in this file's history.
electrumx_sync_gap() {
  local line ours daemon
  line=$(podman logs --tail 400 --since 30m electrumx 2>/dev/null \
    | grep -E 'our height: [0-9,]+ daemon: [0-9,]+' | tail -1)
  [[ -z "$line" ]] && return 0
  ours=$(echo "$line" | grep -oE 'our height: [0-9,]+' | tr -dc '0-9')
  daemon=$(echo "$line" | grep -oE 'daemon: [0-9,]+' | tr -dc '0-9')
  [[ -n "$ours" && -n "$daemon" ]] && echo $((daemon - ours))
}

skip_if_initial_sync() {
  local gap
  gap=$(electrumx_sync_gap)
  if [[ -n "$gap" ]] && (( gap > 10 )); then
    skip "electrumx initial sync in progress ($gap blocks behind) — $1"
  fi
}

# ────────────────────────────────────────────────────────────────────
# Read-only tier
# ────────────────────────────────────────────────────────────────────

@test "container-list includes electrumx" {
  run rpc_result container-list
  [ "$status" -eq 0 ]
  echo "$output" | jq -e '.[] | select(.name == "electrumx")' >/dev/null
}

@test "container-list reports a valid state for electrumx" {
  run rpc_result container-list
  [ "$status" -eq 0 ]
  local state
  state=$(echo "$output" | jq -r '.[] | select(.name == "electrumx") | .state')
  [[ "$state" =~ ^(running|stopped|exited|created|paused)$ ]]
}

@test "electrumx TCP port accepts connections when running" {
  local state
  state=$(rpc_result container-list | jq -r '.[] | select(.name == "electrumx") | .state')
  if [[ "$state" != "running" ]]; then
    skip "electrumx not running (state=$state)"
  fi

  # ElectrumX serves no sessions until initial sync completes, so probing a
  # mid-sync instance can only fail — skip with the gap named instead.
  skip_if_initial_sync "sessions are not served until it catches up"

  # Same probe required-stack.bats uses — divergence flags a real regression.
  # It is a real Electrum round-trip, not a bare connect(): podman's port
  # forwarder accepts the TCP handshake on the host-published port even when
  # nothing inside the container is serving, which kept this test green for
  # days while mempool-api could not reach electrumx at all. See the longer
  # note in required-stack.bats.
  run python3 - <<'PY'
import json, socket
s = socket.create_connection(("127.0.0.1", 50001), 5)
s.settimeout(10)
s.sendall((json.dumps({"id": 0, "method": "server.version",
                       "params": ["archy-gate", "1.4"]}) + "\n").encode())
buf = b""
while b"\n" not in buf:
    chunk = s.recv(4096)
    if not chunk:
        raise SystemExit("electrumx closed the connection without replying "
                         "— listening but not serving (still syncing?)")
    buf += chunk
s.close()
resp = json.loads(buf.split(b"\n")[0])
if "result" not in resp:
    raise SystemExit(f"electrumx returned no result: {resp}")
print("ok", resp["result"])
PY
  [ "$status" -eq 0 ]
}

@test "no orphan electrumx-related containers beyond the known set" {
  # FM4 guard: known-good electrumx-package set is {electrumx, archy-electrs-ui}.
  local total known
  total=$(podman ps -a --format '{{.Names}}' \
    | grep -Ec '^(electrumx|electrs|archy-electrs(-[a-z]+)?)$' || true)
  known=$(podman ps -a --format '{{.Names}}' \
    | grep -Ec '^(electrumx|archy-electrs-ui)$' || true)
  [ "$total" -eq "$known" ]
}

# ────────────────────────────────────────────────────────────────────
# Destructive tier (stop → start → restart on the same container)
# ────────────────────────────────────────────────────────────────────

@test "package.stop transitions electrumx to stopped" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  skip_if_initial_sync "restarting discards unflushed sync progress (1GB flush cache)"

  run rpc_result package.stop '{"id":"electrumx"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status electrumx stopped 60
  [ "$status" -eq 0 ]
}

@test "package.start brings electrumx back to running" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  skip_if_initial_sync "restarting discards unflushed sync progress (1GB flush cache)"

  run rpc_result package.start '{"id":"electrumx"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status electrumx running 120
  [ "$status" -eq 0 ]
}

@test "package.restart leaves electrumx in running state" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  skip_if_initial_sync "restarting discards unflushed sync progress (1GB flush cache)"

  run rpc_result package.restart '{"id":"electrumx"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status electrumx running 120
  [ "$status" -eq 0 ]
}

@test "electrumx TCP port recovers after restart" {
  [[ "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_DESTRUCTIVE not set"
  skip_if_initial_sync "restarting discards unflushed sync progress (1GB flush cache)"

  # electrumx replays its index against bitcoind on cold start; allow 120s.
  local deadline=$(( $(date +%s) + 120 ))
  while (( $(date +%s) < deadline )); do
    if python3 -c 'import socket; socket.create_connection(("127.0.0.1", 50001), 3).close()' \
        >/dev/null 2>&1; then
      return 0
    fi
    sleep 3
  done
  fail "electrumx TCP port never reopened after restart"
}

# ────────────────────────────────────────────────────────────────────
# Cascade-destructive tier (uninstall + reinstall)
# ────────────────────────────────────────────────────────────────────

@test "package.uninstall removes electrumx" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"

  run rpc_result package.uninstall '{"id":"electrumx","preserve_data":true}'
  [ "$status" -eq 0 ]

  run wait_for_container_status electrumx absent 120
  [ "$status" -eq 0 ]
}

@test "package.install electrumx returns to running" {
  [[ "${ARCHY_ALLOW_CASCADE_DESTRUCTIVE:-0}" == "1" ]] || skip "ARCHY_ALLOW_CASCADE_DESTRUCTIVE not set"

  run rpc_result package.install '{"manifest_path":"electrumx/manifest.yaml"}'
  [ "$status" -eq 0 ]

  run wait_for_container_status electrumx running 240
  [ "$status" -eq 0 ]
}
