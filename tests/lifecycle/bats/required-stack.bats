#!/usr/bin/env bats
# tests/lifecycle/bats/required-stack.bats
#
# Read-only release-gate checks for the Bitcoin/electrum/lnd/mempool stack.
# Originally written against .116's fixed app roster; the "present"/"running"
# checks below now only require containers actually installed on THIS node
# (podman_all_names — present in `podman ps -a` even if stopped), so a node
# with a different app subset (e.g. no mempool stack) doesn't hard-fail on
# apps it was never meant to have. Per-app checks further down (mempool,
# filebrowser, ...) skip individually if that app isn't installed, matching
# the mempool_skip_if_absent idiom in mempool.bats.
#
# This suite is intentionally non-destructive and does not use RPC auth;
# it can run anytime as a health gate during long sync/reindex windows.

required_containers=(
  "bitcoin-knots"
  "electrumx"
  "lnd"
  "archy-mempool-db"
  "mempool-api"
  "mempool"
  "filebrowser"
  "archy-bitcoin-ui"
  "archy-lnd-ui"
  "archy-electrs-ui"
)

fail() { echo "$@" >&2; return 1; }

podman_names() {
  podman ps --format '{{.Names}}'
}

podman_all_names() {
  podman ps -a --format '{{.Names}}'
}

container_running() {
  local name="$1"
  podman inspect --format '{{.State.Running}}' "$name" 2>/dev/null
}

container_installed() {
  local name="$1"
  podman_all_names | grep -Fx "$name" >/dev/null
}

skip_if_not_installed() {
  container_installed "$1" || skip "$1 not installed on this node"
}

# ElectrumX serves no sessions until its initial sync completes, so the
# protocol probe below can only fail mid-sync. Skip with the gap NAMED (a
# positively-detected state from a fresh log line, never inferred from
# absence). Canonical rationale lives in electrumx.bats next to its twin.
skip_if_electrumx_initial_sync() {
  local line ours daemon
  line=$(podman logs --tail 400 --since 30m electrumx 2>/dev/null \
    | grep -E 'our height: [0-9,]+ daemon: [0-9,]+' | tail -1)
  [[ -z "$line" ]] && return 0
  ours=$(echo "$line" | grep -oE 'our height: [0-9,]+' | tr -dc '0-9')
  daemon=$(echo "$line" | grep -oE 'daemon: [0-9,]+' | tr -dc '0-9')
  [[ -n "$ours" && -n "$daemon" ]] || return 0
  local gap=$((daemon - ours))
  if (( gap > 10 )); then
    skip "electrumx initial sync in progress ($gap blocks behind) — sessions are not served until it catches up"
  fi
}

# The subset of required_containers actually installed on this node.
installed_required_containers() {
  local c
  for c in "${required_containers[@]}"; do
    container_installed "$c" && echo "$c"
  done
  # Always succeed — see the identical comment in required-stack-destructive.bats.
  return 0
}

bitcoin_rpc() {
  curl -fsS --max-time 60 \
    --user "archipelago:$(cat /var/lib/archipelago/secrets/bitcoin-rpc-password)" \
    --data-binary '{"jsonrpc":"1.0","id":"required-stack","method":"getblockchaininfo","params":[]}' \
    -H 'content-type: text/plain;' \
    http://127.0.0.1:8332/
}

bitcoin_json() {
  python3 -c 'import json,sys; r=json.load(sys.stdin)["result"]; print(r[sys.argv[1]])' "$1"
}

@test "required containers are present" {
  # Under sustained 5× churn an app may still be mid-restart when this runs;
  # wait for the whole required set rather than single-shot. Only checks
  # containers actually installed on this node (see installed_required_containers).
  local targets; targets="$(installed_required_containers)"
  [[ -n "$targets" ]] || skip "none of required_containers installed on this node"
  local deadline=$(( $(date +%s) + 180 )) names missing
  while (( $(date +%s) < deadline )); do
    names="$(podman_names)"; missing=""
    while IFS= read -r c; do
      echo "$names" | grep -Fx "$c" >/dev/null || missing="$missing $c"
    done <<< "$targets"
    [[ -z "$missing" ]] && return 0
    sleep 3
  done
  fail "required containers never all present; missing:$missing"
}

@test "required containers are running" {
  local targets; targets="$(installed_required_containers)"
  [[ -n "$targets" ]] || skip "none of required_containers installed on this node"
  local deadline=$(( $(date +%s) + 180 )) notrunning
  while (( $(date +%s) < deadline )); do
    notrunning=""
    while IFS= read -r c; do
      [[ "$(container_running "$c" 2>/dev/null)" == "true" ]] || notrunning="$notrunning $c"
    done <<< "$targets"
    [[ -z "$notrunning" ]] && return 0
    sleep 3
  done
  fail "required containers never all running; not-running:$notrunning"
}

@test "bitcoin-knots RPC responds" {
  skip_if_not_installed bitcoin-knots
  run bitcoin_rpc
  [ "$status" -eq 0 ]
  echo "$output" | python3 -c 'import json,sys; r=json.load(sys.stdin)["result"]; assert r["chain"] == "main" and r["blocks"] >= 0'
}

@test "bitcoin backend is synced archival for electrumx/lnd gate" {
  skip_if_not_installed bitcoin-knots
  run bitcoin_rpc
  [ "$status" -eq 0 ]

  local pruned ibd blocks headers
  pruned="$(echo "$output" | bitcoin_json pruned)"
  ibd="$(echo "$output" | bitcoin_json initialblockdownload)"
  blocks="$(echo "$output" | bitcoin_json blocks)"
  headers="$(echo "$output" | bitcoin_json headers)"

  if [ "$pruned" = "True" ] || [ "$pruned" = "true" ]; then
    echo "bitcoin is pruned (blocks=$blocks headers=$headers); electrumx cannot index pruned historical blocks"
    return 1
  fi
  if [ "$ibd" = "True" ] || [ "$ibd" = "true" ]; then
    echo "bitcoin is still in initial block download (blocks=$blocks headers=$headers)"
    return 1
  fi
}

@test "electrumx answers the Electrum protocol (not just an open socket)" {
  skip_if_not_installed electrumx
  skip_if_electrumx_initial_sync
  # A bare connect() to the HOST-published port proves nothing: podman's port
  # forwarder accepts the TCP handshake even when nothing inside the container
  # is listening. On 2026-08-09 this test was green while mempool-api was in a
  # hard ECONNREFUSED loop against electrumx:50001 from inside archy-net —
  # electrumx was still doing its initial sync (DB height 959,774 vs chain tip
  # 961,706) and ElectrumX does not serve sessions until it has caught up.
  # So do a real protocol round-trip: the forwarder cannot fake a reply.
  run python3 - <<'PY'
import json, socket
s = socket.create_connection(("127.0.0.1", 50001), 5)
s.settimeout(10)
req = json.dumps({"id": 0, "method": "server.version",
                  "params": ["archy-gate", "1.4"]}) + "\n"
s.sendall(req.encode())
buf = b""
while b"\n" not in buf:
    chunk = s.recv(4096)
    if not chunk:
        raise SystemExit("electrumx closed the connection without replying "
                         "— it is listening but not serving (still syncing?)")
    buf += chunk
s.close()
resp = json.loads(buf.split(b"\n")[0])
if "result" not in resp:
    raise SystemExit(f"electrumx returned no result: {resp}")
print("ok", resp["result"])
PY
  [ "$status" -eq 0 ]
}

@test "lnd CLI getinfo succeeds" {
  skip_if_not_installed lnd
  # lnd RPC readiness lags the container "running" state (wallet auto-unlock on
  # start), so retry until ready rather than single-shot. See lnd.bats note.
  run sh -lc 'for i in $(seq 1 30); do
    timeout 20 podman exec lnd lncli --tlscertpath /root/.lnd/tls.cert --macaroonpath /root/.lnd/data/chain/bitcoin/mainnet/readonly.macaroon --rpcserver localhost:10009 getinfo >/dev/null 2>&1 && exit 0
    sleep 3
  done; exit 1'
  [ "$status" -eq 0 ]
}

@test "lnd REST port accepts connections" {
  skip_if_not_installed lnd
  run python3 - <<'PY'
import socket
s = socket.create_connection(("127.0.0.1", 18080), 3)
s.close()
print("ok")
PY
  [ "$status" -eq 0 ]
}

@test "mempool api endpoint responds" {
  skip_if_not_installed mempool-api
  # mempool-api reconnects to electrumx after a stack restart — retry ~180s.
  run sh -lc 'for i in $(seq 1 60); do curl -fsS -m 5 -o /dev/null "http://127.0.0.1:8999/api/v1/backend-info" && exit 0; sleep 3; done; exit 1'
  [ "$status" -eq 0 ]
}

@test "mempool frontend responds" {
  skip_if_not_installed mempool
  run sh -lc 'for i in $(seq 1 60); do curl -fsS -m 5 -o /dev/null "http://127.0.0.1:4080/" && exit 0; sleep 3; done; exit 1'
  [ "$status" -eq 0 ]
}

@test "bitcoin ui responds" {
  skip_if_not_installed archy-bitcoin-ui
  # The companion (archy-bitcoin-ui) may have just been recreated by an earlier
  # companion-survives test; its nginx takes a moment to serve. Retry ~120s
  # rather than single-shot.
  run sh -lc 'for i in $(seq 1 40); do curl -fsS -o /dev/null "http://127.0.0.1:8334/" && exit 0; sleep 3; done; exit 1'
  [ "$status" -eq 0 ]
}

@test "lnd ui responds" {
  skip_if_not_installed archy-lnd-ui
  run curl -fsS "http://127.0.0.1:18083/"
  [ "$status" -eq 0 ]
}

@test "filebrowser responds" {
  skip_if_not_installed filebrowser
  run curl -fsS "http://127.0.0.1:8083/"
  [ "$status" -eq 0 ]
}
