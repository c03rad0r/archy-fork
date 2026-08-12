#!/bin/bash
#
# Bitcoin stack lifecycle test.
#
# Exercises the production Bitcoin stack under repeated stop/start and
# remove/recreate cycles while asserting the actual user-facing surfaces:
# Bitcoin RPC, bitcoin-ui /bitcoin-rpc, ElectrumX status, and electrs-ui.
#
# This intentionally removes containers but not data volumes. It is safe for
# installed nodes, but it will briefly interrupt Bitcoin/ElectrumX service.
#
# Usage:
#   scripts/bitcoin-stack-lifecycle-test.sh --target archipelago@192.0.2.10
#   scripts/bitcoin-stack-lifecycle-test.sh --target archipelago@192.0.2.12 --cycles 5

set -euo pipefail

TARGET=""
SSH_KEY="${ARCHIPELAGO_SSH_KEY:-}"
CYCLES=3
SSH_EXTRA=()

while [ "$#" -gt 0 ]; do
  case "$1" in
    --target)
      TARGET="${2:-}"
      shift 2
      ;;
    --ssh-key)
      SSH_KEY="${2:-}"
      shift 2
      ;;
    --cycles)
      CYCLES="${2:-}"
      shift 2
      ;;
    --ssh-option)
      SSH_EXTRA+=("-o" "${2:-}")
      shift 2
      ;;
    -h|--help)
      sed -n '1,22p' "$0"
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [ -z "$TARGET" ]; then
  echo "--target is required, for example archipelago@192.0.2.10" >&2
  exit 2
fi

SSH=(ssh -F /dev/null -o BatchMode=yes -o PreferredAuthentications=publickey -o PasswordAuthentication=no -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null)
if [ -n "$SSH_KEY" ]; then
  SSH+=("-i" "$SSH_KEY")
fi
SSH+=("${SSH_EXTRA[@]}")

"${SSH[@]}" "$TARGET" "CYCLES='$CYCLES' bash -s" <<'REMOTE'
set -euo pipefail

PODMAN="${PODMAN:-podman}"
SCRIPTS_DIR="/opt/archipelago/scripts"
if [ ! -x "$SCRIPTS_DIR/reconcile-containers.sh" ]; then
  SCRIPTS_DIR="$HOME/archy/scripts"
fi
RECONCILE="$SCRIPTS_DIR/reconcile-containers.sh"

pass_count=0
fail_count=0

log() { printf '[%s] %s\n' "$(date +%H:%M:%S)" "$*"; }
pass() { pass_count=$((pass_count + 1)); printf '  PASS %s\n' "$*"; }
fail() { fail_count=$((fail_count + 1)); printf '  FAIL %s\n' "$*" >&2; }

retry() {
  local timeout="$1" label="$2"
  shift 2
  local end=$((SECONDS + timeout))
  local out rc
  while [ "$SECONDS" -lt "$end" ]; do
    set +e
    out=$("$@" 2>&1)
    rc=$?
    set -e
    if [ "$rc" -eq 0 ]; then
      pass "$label"
      return 0
    fi
    sleep 2
  done
  fail "$label: $out"
  return 1
}

rpc_pass() {
  cat /var/lib/archipelago/secrets/bitcoin-rpc-password
}

json_rpc_reachable_or_warming() {
  local url="$1" auth_arg=() body rc
  if [ "${2:-}" = "auth" ]; then
    auth_arg=(--user "archipelago:$(rpc_pass)")
  fi
  set +e
  body=$(curl --connect-timeout 3 --max-time 20 -sS "${auth_arg[@]}" \
    -H "Content-Type: application/json" \
    --data-binary '{"jsonrpc":"1.0","id":"lifecycle-test","method":"getblockchaininfo","params":[]}' \
    "$url" 2>&1)
  rc=$?
  set -e
  [ "$rc" -eq 0 ] || {
    echo "$body"
    return 1
  }
  echo "$body" | grep -q '"result"' && return 0
  echo "$body" | grep -q '"code":-28' && return 0
  echo "$body"
  return 1
}

bitcoin_status_usable() {
  local url="$1"
  local body
  body=$(curl --connect-timeout 3 --max-time 20 -fsS "$url")
  echo "$body" | grep -q '"ok":\(true\|false\)' || {
    echo "$body"
    return 1
  }
  echo "$body" | grep -q '"blockchain_info"' || echo "$body" | grep -q '"error"'
}

http_ok() {
  local url="$1"
  curl --connect-timeout 3 --max-time 20 -fsS -o /dev/null "$url"
}

electrs_status_ok() {
  local url="${1:-http://127.0.0.1:50002/electrs-status}"
  local body
  body=$(curl --connect-timeout 3 --max-time 20 -fsS "$url")
  echo "$body" | grep -q '"network_height":[1-9]' || {
    echo "$body"
    return 1
  }
  echo "$body" | grep -q '"status":"\(indexing\|syncing\|synced\|waiting\)"'
}

container_running() {
  local name="$1"
  [ "$($PODMAN inspect "$name" --format '{{.State.Status}}' 2>/dev/null || true)" = "running" ]
}

container_healthy_or_starting() {
  local name="$1"
  local health
  health=$($PODMAN inspect "$name" --format '{{if .State.Health}}{{.State.Health.Status}}{{end}}' 2>/dev/null || true)
  [ "$health" = "healthy" ] || [ "$health" = "starting" ] || [ -z "$health" ]
}

assert_bitcoin_stack() {
  retry 90 "bitcoin-knots running" container_running bitcoin-knots
  retry 90 "bitcoin-knots healthy/starting" container_healthy_or_starting bitcoin-knots
  retry 90 "host Bitcoin RPC reachable/ready" json_rpc_reachable_or_warming http://127.0.0.1:8332/ auth
  retry 90 "backend Bitcoin status bridge usable" bitcoin_status_usable http://127.0.0.1:5678/bitcoin-status
  retry 90 "bitcoin-ui page" http_ok http://127.0.0.1:8334/
  retry 90 "bitcoin-ui status bridge usable" bitcoin_status_usable http://127.0.0.1:8334/bitcoin-status
  retry 90 "bitcoin-ui app-session status bridge usable" bitcoin_status_usable http://127.0.0.1/app/bitcoin-ui/bitcoin-status
  retry 90 "bitcoin-ui RPC proxy reachable/ready" json_rpc_reachable_or_warming http://127.0.0.1:8334/bitcoin-rpc/
  retry 90 "bitcoin-ui app-session RPC proxy reachable/ready" json_rpc_reachable_or_warming http://127.0.0.1/app/bitcoin-ui/bitcoin-rpc/
}

assert_electrum_stack() {
  retry 120 "electrumx running" container_running electrumx
  retry 120 "electrumx healthy/starting" container_healthy_or_starting electrumx
  retry 90 "electrs-ui page" http_ok http://127.0.0.1:50002/
  retry 120 "electrs status has network height" electrs_status_ok
  retry 120 "electrs app-session status has network height" electrs_status_ok http://127.0.0.1/app/electrumx/electrs-status
  retry 120 "electrs legacy app-session status has network height" electrs_status_ok http://127.0.0.1/app/electrs/electrs-status
}

reconcile_one() {
  local name="$1"
  "$RECONCILE" --container="$name" --force --force-recreate --create-missing
}

restart_container() {
  local name="$1"
  log "restart $name"
  $PODMAN restart "$name" >/dev/null || {
    log "podman restart failed for $name; using stop/start"
    $PODMAN stop "$name" >/dev/null 2>&1 || true
    sleep 3
    $PODMAN start "$name" >/dev/null
  }
}

remove_and_reconcile() {
  local name="$1"
  log "remove/recreate $name"
  $PODMAN rm -f "$name" >/dev/null 2>&1 || true
  reconcile_one "$name"
}

log "target $(hostname) cycles=$CYCLES"
log "using reconciler: $RECONCILE"

assert_bitcoin_stack
assert_electrum_stack

for i in $(seq 1 "$CYCLES"); do
  log "cycle $i/$CYCLES: bitcoin restart"
  restart_container bitcoin-knots
  assert_bitcoin_stack
  assert_electrum_stack

  log "cycle $i/$CYCLES: bitcoin remove/reconcile"
  remove_and_reconcile bitcoin-knots
  assert_bitcoin_stack
  assert_electrum_stack

  log "cycle $i/$CYCLES: bitcoin UI remove/reconcile"
  remove_and_reconcile archy-bitcoin-ui
  assert_bitcoin_stack

  log "cycle $i/$CYCLES: electrumx restart"
  restart_container electrumx
  assert_electrum_stack

  log "cycle $i/$CYCLES: electrumx remove/reconcile"
  remove_and_reconcile electrumx
  assert_electrum_stack

  log "cycle $i/$CYCLES: electrs UI remove/reconcile"
  remove_and_reconcile archy-electrs-ui
  assert_electrum_stack
done

log "final container state"
$PODMAN ps -a --format 'table {{.Names}}\t{{.State}}\t{{.Status}}' \
  | grep -E 'bitcoin-knots|electrumx|archy-bitcoin-ui|archy-electrs-ui' || true

log "summary: pass=$pass_count fail=$fail_count"
[ "$fail_count" -eq 0 ]
REMOTE
