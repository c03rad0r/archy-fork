#!/bin/bash
#
# App surface smoke test.
#
# Verifies that installed containers have their published host ports listening
# and that known nginx app proxy paths return a non-5xx response. This catches
# the common "container is running but UI disappeared" failure mode.
#
# Usage:
#   scripts/app-surface-smoke-test.sh --target archipelago@192.0.2.10 --ssh-key /path/key

set -euo pipefail

TARGET=""
SSH_KEY="${ARCHIPELAGO_SSH_KEY:-}"
SSH_EXTRA=()

while [ "$#" -gt 0 ]; do
  case "$1" in
    --target) TARGET="${2:-}"; shift 2 ;;
    --ssh-key) SSH_KEY="${2:-}"; shift 2 ;;
    --ssh-option) SSH_EXTRA+=("-o" "${2:-}"); shift 2 ;;
    -h|--help) sed -n '1,12p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[ -n "$TARGET" ] || { echo "--target is required" >&2; exit 2; }

SSH_OPTS=(-F /dev/null -o BatchMode=yes -o PreferredAuthentications=publickey -o PasswordAuthentication=no)
[ -n "$SSH_KEY" ] && SSH_OPTS+=(-i "$SSH_KEY")
SSH_OPTS+=("${SSH_EXTRA[@]}")

ssh_run() {
  ssh "${SSH_OPTS[@]}" "$TARGET" "$@"
}

ssh_run 'bash -s' <<'REMOTE'
set -u

pass=0
fail=0

ok() { echo "  PASS $*"; pass=$((pass + 1)); }
bad() { echo "  FAIL $*"; fail=$((fail + 1)); }

container_exists() {
  podman ps -a --format '{{.Names}}' 2>/dev/null | grep -qx "$1"
}

port_listening() {
  ss -ltn 2>/dev/null | awk '{print $4}' | grep -Eq "(^|:)$1$"
}

http_code() {
  local url="$1" code
  for _ in 1 2 3; do
    code=$(curl -ksS -o /dev/null -w '%{http_code}' --max-time 12 "$url" 2>/dev/null || true)
    [ -n "$code" ] || code=000
    [ "$code" != "000" ] && { echo "$code"; return; }
    sleep 2
  done
  echo "$code"
}

http_post_code() {
  local url="$1" code
  for _ in 1 2 3; do
    code=$(curl -ksS -o /dev/null -w '%{http_code}' --max-time 25 \
      -H 'Content-Type: application/json' \
      -d '{"jsonrpc":"2.0","id":1,"method":"getblockchaininfo","params":[]}' \
      "$url" 2>/dev/null || true)
    [ -n "$code" ] || code=000
    [ "$code" != "000" ] && { echo "$code"; return; }
    sleep 2
  done
  echo "$code"
}

assert_http() {
  local label="$1" url="$2" code
  code=$(http_code "$url")
  case "$code" in
    200|204|301|302|307|308|401|403) ok "$label HTTP $code" ;;
    *) bad "$label HTTP $code ($url)" ;;
  esac
}

assert_http_post() {
  local label="$1" url="$2" code
  code=$(http_post_code "$url")
  case "$code" in
    200|204|401|403) ok "$label HTTP POST $code" ;;
    *) bad "$label HTTP POST $code ($url)" ;;
  esac
}

assert_container_ports() {
  local name="$1" ports port missing=0
  container_exists "$name" || return 0
  ports=$(podman inspect "$name" --format '{{range $p,$bindings := .NetworkSettings.Ports}}{{if $bindings}}{{range $bindings}}{{.HostPort}}{{"\n"}}{{end}}{{end}}{{end}}' 2>/dev/null | sort -u)
  [ -n "$ports" ] || return 0
  while IFS= read -r port; do
    [ -n "$port" ] || continue
    if port_listening "$port"; then
      ok "$name port $port listening"
    else
      bad "$name port $port missing listener"
      missing=1
    fi
  done <<< "$ports"
  return "$missing"
}

assert_env_contains() {
  local name="$1" key="$2" needle="$3" val
  container_exists "$name" || return 0
  val=$(podman inspect "$name" --format '{{range .Config.Env}}{{println .}}{{end}}' 2>/dev/null | sed -n "s/^${key}=//p" | head -n 1)
  if [ -n "$val" ] && printf '%s' "$val" | grep -qF "$needle"; then
    ok "$name env $key"
  else
    bad "$name env $key missing $needle"
  fi
}

echo "[surface] host=$(hostname) ip=$(hostname -I 2>/dev/null | awk '{print $1}')"

for c in $(podman ps -a --format '{{.Names}}' 2>/dev/null | sort); do
  assert_container_ports "$c" || true
done

container_exists archy-bitcoin-ui && {
  assert_http "bitcoin-ui" "http://127.0.0.1/app/bitcoin-ui/"
  assert_http "bitcoin status" "http://127.0.0.1/app/bitcoin-ui/bitcoin-status"
  assert_http_post "bitcoin rpc proxy" "http://127.0.0.1/app/bitcoin-ui/bitcoin-rpc/"
}

container_exists archy-electrs-ui && {
  assert_http "electrumx ui" "http://127.0.0.1/app/electrumx/"
  assert_http "electrumx status" "http://127.0.0.1/app/electrumx/electrs-status"
  assert_http "electrs legacy status" "http://127.0.0.1/app/electrs/electrs-status"
}

container_exists mempool && assert_http "mempool ui" "http://127.0.0.1/app/mempool/"
container_exists indeedhub && assert_http "indeedhub ui" "http://127.0.0.1:7778/"
container_exists uptime-kuma && assert_http "uptime-kuma" "http://127.0.0.1/app/uptime-kuma/"
container_exists filebrowser && assert_http "filebrowser" "http://127.0.0.1/app/filebrowser/"
container_exists searxng && assert_http "searxng" "http://127.0.0.1/app/searxng/"
container_exists grafana && assert_http "grafana" "http://127.0.0.1/app/grafana/"
container_exists portainer && assert_http "portainer" "http://127.0.0.1/app/portainer/"
container_exists vaultwarden && assert_http "vaultwarden" "http://127.0.0.1/app/vaultwarden/"
container_exists nextcloud && assert_http "nextcloud" "http://127.0.0.1/app/nextcloud/"
container_exists archy-nbxplorer && assert_env_contains "archy-nbxplorer" "NBXPLORER_POSTGRES" "Database=nbxplorer"
container_exists btcpay-server && {
  assert_env_contains "btcpay-server" "BTCPAY_POSTGRES" "Database=btcpay"
  assert_http "btcpay" "http://127.0.0.1/app/btcpay/"
}

echo "[surface] summary: pass=$pass fail=$fail"
[ "$fail" -eq 0 ]
REMOTE
