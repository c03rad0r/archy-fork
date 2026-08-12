#!/usr/bin/env bash
# Remote app lifecycle runner for Archipelago nodes.
#
# Exercises the same public surface the UI uses:
#   - JSON-RPC package.install/start/stop/restart/uninstall
#   - HTTPS/direct-port launch probes from appSessionConfig.ts
#
# Default mode is audit-only. Use ARCHY_FULL_LIFECYCLE=1 for destructive
# preserve-data cycles: install -> launch -> stop -> start -> restart ->
# uninstall(preserve_data=true) -> reinstall -> launch.

set -euo pipefail

ARCHY_HOST="${ARCHY_HOST:-}"
ARCHY_SCHEME="${ARCHY_SCHEME:-https}"
ARCHY_PASSWORD="${ARCHY_PASSWORD:-}"
ARCHY_ITERATIONS="${ARCHY_ITERATIONS:-1}"
ARCHY_FULL_LIFECYCLE="${ARCHY_FULL_LIFECYCLE:-0}"
ARCHY_APPS="${ARCHY_APPS:-}"
ARCHY_TIMEOUT="${ARCHY_TIMEOUT:-900}"
ARCHY_STABILITY_SECONDS="${ARCHY_STABILITY_SECONDS:-5}"
ARCHY_ALLOW_BITCOIN_SWAP="${ARCHY_ALLOW_BITCOIN_SWAP:-0}"
ARCHY_APP_CATALOG="${ARCHY_APP_CATALOG:-}"
ARCHY_PRUNED_NODE="${ARCHY_PRUNED_NODE:-auto}"

if [[ -z "$ARCHY_HOST" || -z "$ARCHY_PASSWORD" ]]; then
  echo "ARCHY_HOST and ARCHY_PASSWORD are required" >&2
  exit 2
fi
if ! [[ "$ARCHY_ITERATIONS" =~ ^[1-9][0-9]*$ ]]; then
  echo "ARCHY_ITERATIONS must be a positive integer" >&2
  exit 2
fi
if ! [[ "$ARCHY_STABILITY_SECONDS" =~ ^[0-9]+$ ]]; then
  echo "ARCHY_STABILITY_SECONDS must be a non-negative integer" >&2
  exit 2
fi

BASE_URL="${ARCHY_SCHEME}://${ARCHY_HOST}"
SESSION=""
CSRF=""
CATALOG_FILE=""

ALL_APPS=(
  bitcoin-knots
  btcpay-server
  lnd
  mempool
  homeassistant
  grafana
  searxng
  ollama
  nextcloud
  vaultwarden
  jellyfin
  photoprism
  immich
  filebrowser
  nginx-proxy-manager
  portainer
  tailscale
  uptime-kuma
  electrumx
  fedimint
  indeedhub
  dwn
  botfights
  gitea
)

ARCHIVAL_ONLY_APPS=(
  electrumx
  mempool
)

app_in_list() {
  local needle="$1"
  shift
  local item
  for item in "$@"; do
    [[ "$item" == "$needle" ]] && return 0
  done
  return 1
}

fetch_catalog() {
  CATALOG_FILE=$(mktemp)
  if [[ -n "$ARCHY_APP_CATALOG" ]]; then
    cp "$ARCHY_APP_CATALOG" "$CATALOG_FILE"
    return 0
  fi

  if curl -skfL --connect-timeout 8 -m 30 "${BASE_URL}/api/app-catalog" -o "$CATALOG_FILE" \
    && jq -e '.apps | length > 0' "$CATALOG_FILE" >/dev/null; then
    return 0
  fi

  curl -skfL --connect-timeout 8 -m 30 "${BASE_URL}/catalog.json" -o "$CATALOG_FILE"
  jq -e '.apps | length > 0' "$CATALOG_FILE" >/dev/null
}

catalog_app_ids() {
  jq -r '.apps[] | select((.dockerImage // "") != "") | .id' "$CATALOG_FILE"
}

catalog_app_json() {
  local app="$1"
  [[ -n "$CATALOG_FILE" && -r "$CATALOG_FILE" ]] || return 1
  jq -c --arg app "$app" '
    .registry as $registry
    | .apps[]
    | select(.id == $app)
    | .dockerImage = (if ((.dockerImage // "") | contains("/")) then .dockerImage else ($registry + "/" + .dockerImage) end)
  ' "$CATALOG_FILE" | head -n 1
}

is_pruned_node() {
  case "$ARCHY_PRUNED_NODE" in
    1|true|yes) return 0 ;;
    0|false|no) return 1 ;;
  esac

  local pass body
  pass=$(ssh "${ARCHY_HOST}" 'sudo cat /var/lib/archipelago/secrets/bitcoin-rpc-password 2>/dev/null || cat /var/lib/archipelago/secrets/bitcoin-rpc-password 2>/dev/null' 2>/dev/null || true)
  [[ -n "$pass" ]] || return 1
  body=$(curl -fsS --max-time 20 \
    --user "archipelago:${pass}" \
    --data-binary '{"jsonrpc":"1.0","id":"remote-lifecycle","method":"getblockchaininfo","params":[]}' \
    -H 'content-type: text/plain;' \
    "http://${ARCHY_HOST}:8332/" 2>/dev/null || true)
  printf '%s' "$body" | jq -e '.result.pruned == true' >/dev/null 2>&1
}

image_for() {
  case "$1" in
    bitcoin-knots) echo "source.archipelago-foundation.org/lfg2025/bitcoin-knots:latest" ;;
    bitcoin-core) echo "docker.io/bitcoin/bitcoin:28.4" ;;
    btcpay-server) echo "docker.io/btcpayserver/btcpayserver:2.4.2" ;;
    lnd) echo "source.archipelago-foundation.org/lfg2025/lnd:v0.18.4-beta" ;;
    mempool) echo "source.archipelago-foundation.org/lfg2025/mempool-frontend:v3.0.0" ;;
    homeassistant) echo "source.archipelago-foundation.org/lfg2025/home-assistant:2024.1" ;;
    grafana) echo "source.archipelago-foundation.org/lfg2025/grafana:10.2.0" ;;
    searxng) echo "source.archipelago-foundation.org/lfg2025/searxng:latest" ;;
    ollama) echo "source.archipelago-foundation.org/lfg2025/ollama:latest" ;;
    nextcloud) echo "source.archipelago-foundation.org/lfg2025/nextcloud:28" ;;
    vaultwarden) echo "source.archipelago-foundation.org/lfg2025/vaultwarden:1.30.0-alpine" ;;
    jellyfin) echo "source.archipelago-foundation.org/lfg2025/jellyfin:10.8.13" ;;
    photoprism) echo "source.archipelago-foundation.org/lfg2025/photoprism:240915" ;;
    immich) echo "source.archipelago-foundation.org/lfg2025/immich-server:release" ;;
    filebrowser) echo "source.archipelago-foundation.org/lfg2025/filebrowser:v2.27.0" ;;
    nginx-proxy-manager) echo "source.archipelago-foundation.org/lfg2025/nginx-proxy-manager:latest" ;;
    portainer) echo "source.archipelago-foundation.org/lfg2025/portainer:latest" ;;
    uptime-kuma) echo "source.archipelago-foundation.org/lfg2025/uptime-kuma:1" ;;
    tailscale) echo "source.archipelago-foundation.org/lfg2025/tailscale:stable" ;;
    electrumx) echo "source.archipelago-foundation.org/lfg2025/electrumx:v1.18.0" ;;
    fedimint) echo "source.archipelago-foundation.org/lfg2025/fedimintd:v0.10.0" ;;
    indeedhub) echo "source.archipelago-foundation.org/lfg2025/indeedhub:1.0.0" ;;
    botfights) echo "source.archipelago-foundation.org/lfg2025/botfights:1.1.0" ;;
    gitea) echo "docker.io/gitea/gitea:1.23" ;;
    *) return 1 ;;
  esac
}

launch_url_for() {
  case "$1" in
    bitcoin-knots|bitcoin-core|bitcoin-ui) echo "http://${ARCHY_HOST}:8334/" ;;
    lnd|archy-lnd-ui) echo "http://${ARCHY_HOST}:18083/" ;;
    electrumx|electrs|mempool-electrs|archy-electrs-ui) echo "http://${ARCHY_HOST}:50002/" ;;
    mempool|mempool-web|archy-mempool-web) echo "http://${ARCHY_HOST}:4080/" ;;
    fedimint|fedimintd) echo "http://${ARCHY_HOST}:8175/" ;;
    fedimint-gateway) echo "http://${ARCHY_HOST}:8176/" ;;
    filebrowser) echo "http://${ARCHY_HOST}:8083/" ;;
    grafana) echo "http://${ARCHY_HOST}:3000/" ;;
    btcpay-server) echo "http://${ARCHY_HOST}:23000/" ;;
    jellyfin) echo "http://${ARCHY_HOST}:8096/" ;;
    searxng) echo "http://${ARCHY_HOST}:8888/" ;;
    ollama) echo "http://${ARCHY_HOST}:11434/" ;;
    immich|immich_server) echo "http://${ARCHY_HOST}:2283/" ;;
    portainer) echo "http://${ARCHY_HOST}:9000/" ;;
    nginx-proxy-manager) echo "http://${ARCHY_HOST}:8081/" ;;
    tailscale) echo "http://${ARCHY_HOST}:8240/" ;;
    uptime-kuma) echo "http://${ARCHY_HOST}:3002/" ;;
    homeassistant) echo "http://${ARCHY_HOST}:8123/" ;;
    vaultwarden) echo "http://${ARCHY_HOST}:8082/" ;;
    photoprism) echo "http://${ARCHY_HOST}:2342/" ;;
    dwn) echo "http://${ARCHY_HOST}:3100/" ;;
    botfights) echo "http://${ARCHY_HOST}:9100/" ;;
    gitea) echo "http://${ARCHY_HOST}:3001/" ;;
    indeedhub) echo "http://${ARCHY_HOST}:7778/" ;;
    *) return 1 ;;
  esac
}

rpc_login() {
  local headers body err
  headers=$(mktemp)
  body=$(curl -sk -D "$headers" -X POST "${BASE_URL}/rpc/v1" \
    -H 'Content-Type: application/json' \
    --data-raw "$(jq -nc --arg p "$ARCHY_PASSWORD" '{jsonrpc:"2.0",method:"auth.login",params:{password:$p},id:1}')")
  err=$(printf '%s' "$body" | jq -r '.error.message // empty')
  if [[ -n "$err" ]]; then
    rm -f "$headers"
    echo "login failed on $ARCHY_HOST: $err" >&2
    return 1
  fi
  SESSION=$(grep -i '^set-cookie: session=' "$headers" | head -1 | sed -E 's/.*session=([^;]+).*/\1/' | tr -d '\r')
  CSRF=$(grep -i '^set-cookie: csrf_token=' "$headers" | head -1 | sed -E 's/.*csrf_token=([^;]+).*/\1/' | tr -d '\r')
  rm -f "$headers"
  [[ -n "$SESSION" && -n "$CSRF" ]]
}

rpc_call() {
  local method="$1" params="${2:-null}" id="${3:-2}"
  local payload
  if [[ "$params" == "null" ]]; then
    payload=$(jq -nc --arg m "$method" --argjson id "$id" '{jsonrpc:"2.0",method:$m,id:$id}')
  else
    payload=$(jq -nc --arg m "$method" --argjson p "$params" --argjson id "$id" '{jsonrpc:"2.0",method:$m,params:$p,id:$id}')
  fi
  curl -sk -X POST "${BASE_URL}/rpc/v1" \
    --connect-timeout 8 \
    -m "${ARCHY_RPC_TIMEOUT:-60}" \
    -H 'Content-Type: application/json' \
    -H "Cookie: session=${SESSION}; csrf_token=${CSRF}" \
    -H "X-CSRF-Token: ${CSRF}" \
    --data-raw "$payload"
}

rpc_result() {
  local resp err
  resp=$(rpc_call "$@")
  err=$(printf '%s' "$resp" | jq -r '.error.message // empty')
  if [[ -n "$err" ]]; then
    echo "$err" >&2
    return 1
  fi
  printf '%s' "$resp" | jq '.result'
}

container_state() {
  local app="$1"
  rpc_result container-list | jq -r --arg app "$app" '
    (map(select(.name == $app or .id == $app)) | first | .state // "absent") | ascii_downcase
  '
}

container_health() {
  local app="$1" health
  health=$(
    ARCHY_RPC_TIMEOUT="${ARCHY_HEALTH_RPC_TIMEOUT:-20}" \
      rpc_result container-health "$(jq -nc --arg app "$app" '{app_id:$app}')" \
      | jq -r --arg app "$app" '(.[$app] // "") | if . == "" then "unknown" else ascii_downcase end'
  ) || health=unknown
  if [[ "$app" == "indeedhub" && "$health" != "healthy" ]] && probe_launch "$app" >/dev/null 2>&1; then
    health=healthy
  fi
  printf '%s\n' "$health"
}

assert_container_healthy() {
  local app="$1" health
  health=$(container_health "$app" 2>/dev/null || echo unknown)
  case "$health" in
    healthy) return 0 ;;
    *) echo "bad health: $app is $health" >&2; return 1 ;;
  esac
}

wait_container_healthy() {
  local app="$1" timeout="${2:-$ARCHY_TIMEOUT}" deadline health
  deadline=$(( $(date +%s) + timeout ))
  while (( $(date +%s) < deadline )); do
    health=$(container_health "$app" 2>/dev/null || echo unknown)
    if [[ "$health" == "healthy" ]]; then return 0; fi
    sleep 5
  done
  echo "bad health: $app is ${health:-unknown}" >&2
  return 1
}

observe_stable() {
  local app="$1" seconds="${2:-$ARCHY_STABILITY_SECONDS}" deadline state
  (( seconds == 0 )) && return 0
  deadline=$(( $(date +%s) + seconds ))
  while (( $(date +%s) < deadline )); do
    state=$(container_state "$app" 2>/dev/null || echo unknown)
    if [[ "$state" != "running" ]]; then
      if [[ "$app" == "indeedhub" ]] && probe_launch "$app" >/dev/null 2>&1; then
        sleep 5
        continue
      fi
      echo "stability failed: $app left running state (last=$state)" >&2
      return 1
    fi
    assert_container_healthy "$app" || return 1
    sleep 5
  done
}

wait_state() {
  local app="$1" target="$2" timeout="${3:-$ARCHY_TIMEOUT}"
  local deadline state
  deadline=$(( $(date +%s) + timeout ))
  while (( $(date +%s) < deadline )); do
    state=$(container_state "$app" 2>/dev/null || echo unknown)
    if [[ "$target" == "absent" && "$state" == "absent" ]]; then return 0; fi
    if [[ "$target" == "stopped" && "$state" == "absent" ]]; then return 0; fi
    if [[ "$target" != "absent" && "$state" == "$target" ]]; then return 0; fi
    if [[ "$app" == "indeedhub" && "$target" == "running" ]] && probe_launch "$app" >/dev/null 2>&1; then return 0; fi
    sleep 5
  done
  echo "$app did not reach $target within ${timeout}s (last=$state)" >&2
  return 1
}

wait_absent_settled() {
  local app="$1" timeout="${2:-$ARCHY_TIMEOUT}"
  local deadline state seen_absent=0
  deadline=$(( $(date +%s) + timeout ))
  while (( $(date +%s) < deadline )); do
    state=$(container_state "$app" 2>/dev/null || echo unknown)
    if [[ "$state" == "absent" ]]; then
      if (( seen_absent == 1 )); then return 0; fi
      seen_absent=1
    else
      seen_absent=0
    fi
    sleep 5
  done
  echo "$app did not settle absent within ${timeout}s (last=$state)" >&2
  return 1
}

wait_not_installing() {
  local app="$1" timeout="${2:-$ARCHY_TIMEOUT}"
  local deadline state
  deadline=$(( $(date +%s) + timeout ))
  while (( $(date +%s) < deadline )); do
    state=$(container_state "$app" 2>/dev/null || echo unknown)
    case "$state" in
      installing|starting|restarting|updating) sleep 5 ;;
      *) return 0 ;;
    esac
  done
  echo "$app did not settle from install transition within ${timeout}s (last=$state)" >&2
  return 1
}

probe_launch() {
  local app="$1" url code bytes body
  url=$(launch_url_for "$app") || return 0
  body=$(mktemp)
  code=$(curl -skL --connect-timeout 8 -m 20 -o "$body" -w '%{http_code}' "$url" || true)
  bytes=$(wc -c < "$body" 2>/dev/null || printf 0)
  if [[ "$code" != "200" || "$bytes" -eq 0 ]]; then
    echo "launch failed: $app $url status=$code bytes=$bytes" >&2
    rm -f "$body"
    return 1
  fi
  case "$app" in
    lnd) probe_lnd_wallet_connect "$body" || { rm -f "$body"; return 1; } ;;
    electrumx|electrs|mempool-electrs) probe_electrum_wallet_connect "$body" || { rm -f "$body"; return 1; } ;;
    indeedhub) probe_indeedhub_nostr_signer "$body" || { rm -f "$body"; return 1; } ;;
    tailscale) probe_tailscale_login_ui "$body" || { rm -f "$body"; return 1; } ;;
  esac
  rm -f "$body"
}

wait_launch() {
  local app="$1" timeout="${2:-$ARCHY_TIMEOUT}" deadline
  deadline=$(( $(date +%s) + timeout ))
  while (( $(date +%s) < deadline )); do
    if probe_launch "$app" >/dev/null 2>&1; then return 0; fi
    sleep 5
  done
  probe_launch "$app"
}

assert_launch_metadata() {
  local app="$1" timeout="${2:-$ARCHY_TIMEOUT}" deadline lan
  launch_url_for "$app" >/dev/null 2>&1 || return 0
  deadline=$(( $(date +%s) + timeout ))
  while (( $(date +%s) < deadline )); do
    lan=$(rpc_result container-list | jq -r --arg app "$app" '
      (map(select(.name == $app or .id == $app)) | first | .lan_address // "")
    ')
    if [[ -n "$lan" && "$lan" != "null" ]]; then return 0; fi
    sleep 5
  done
  if [[ -z "${lan:-}" || "$lan" == "null" ]]; then
    echo "launch metadata missing: $app has no lan_address" >&2
    return 1
  fi
}

require_body() {
  local body="$1" needle="$2" label="$3"
  if ! grep -Fq "$needle" "$body"; then
    echo "launch missing $label: $needle" >&2
    return 1
  fi
}

probe_lnd_wallet_connect() {
  local body="$1" info err
  require_body "$body" 'Connect Your Wallet' 'LND wallet heading' || return 1
  require_body "$body" 'id="lndQrBox"' 'LND QR container' || return 1
  require_body "$body" 'id="connHost"' 'LND host field' || return 1
  require_body "$body" 'value="rest-tor"' 'LND REST Tor mode' || return 1
  require_body "$body" 'value="grpc-tor"' 'LND gRPC Tor mode' || return 1
  require_body "$body" 'value="rest-local"' 'LND REST local mode' || return 1
  require_body "$body" 'value="grpc-local"' 'LND gRPC local mode' || return 1
  require_body "$body" 'Copy lndconnect URI' 'LND connect URI button' || return 1
  info=$(curl -skL --connect-timeout 8 -m 20 \
    -H "Cookie: session=${SESSION}; csrf_token=${CSRF}" \
    -H "X-CSRF-Token: ${CSRF}" \
    "${BASE_URL}/lnd-connect-info" || true)
  err=$(printf '%s' "$info" | jq -r '.error // empty' 2>/dev/null || true)
  if [[ -n "$err" ]]; then
    echo "lnd connect info error: $err" >&2
    return 1
  fi
  printf '%s' "$info" | jq -e '
    (.cert_base64url | type == "string" and length > 100) and
    (.macaroon_base64url | type == "string" and length > 50) and
    (.tor_onion | type == "string" and test("^[a-z2-7]+\\.onion$")) and
    (.rest_port == 18080) and
    (.grpc_port == 10009)
  ' >/dev/null || {
    echo "lnd connect info incomplete: $info" >&2
    return 1
  }
}

probe_electrum_wallet_connect() {
  local body="$1"
  require_body "$body" 'Connect Your Wallet' 'Electrum wallet heading' || return 1
  require_body "$body" 'id="qrLocalBox"' 'Electrum local QR container' || return 1
  require_body "$body" 'id="qrTorBox"' 'Electrum Tor QR container' || return 1
  require_body "$body" 'id="localAddress"' 'Electrum local address field' || return 1
  require_body "$body" 'id="torAddress"' 'Electrum Tor address field' || return 1
  require_body "$body" '50001' 'Electrum wallet port' || return 1
  require_body "$body" 'renderQR' 'Electrum QR renderer' || return 1
  curl -skL --connect-timeout 8 -m 20 -f "http://${ARCHY_HOST}:50002/qrcode.js" >/dev/null || {
    echo "electrum qrcode.js unavailable" >&2
    return 1
  }
  local status
  status=$(curl -skL --connect-timeout 8 -m 20 "${BASE_URL}/electrs-status" || true)
  printf '%s' "$status" | jq -e '(.tor_onion | type == "string" and test("^[a-z2-7]+\\.onion$"))' >/dev/null || {
    echo "electrum tor connection info incomplete: $status" >&2
    return 1
  }
}

probe_indeedhub_nostr_signer() {
  local body="$1" provider pubkey signed now
  require_body "$body" '/nostr-provider.js' 'IndeedHub Nostr provider injection' || return 1
  provider=$(curl -skL --connect-timeout 8 -m 20 "http://${ARCHY_HOST}:7778/nostr-provider.js" || true)
  if [[ -z "$provider" ]]; then
    echo "indeedhub nostr-provider.js unavailable" >&2
    return 1
  fi
  printf '%s' "$provider" | grep -Eq 'window\.nostr|nostr' || {
    echo "indeedhub nostr-provider.js does not look like a Nostr signer bridge" >&2
    return 1
  }

  pubkey=$(rpc_result node.nostr-pubkey | jq -r '.nostr_pubkey // empty')
  if ! [[ "$pubkey" =~ ^[0-9a-fA-F]{64}$ ]]; then
    echo "indeedhub Nostr signer pubkey unavailable: $pubkey" >&2
    return 1
  fi

  now=$(date +%s)
  signed=$(rpc_result node.nostr-sign "$(jq -nc --argjson created_at "$now" '{event:{kind:1,created_at:$created_at,tags:[],content:"archy lifecycle indeedhub signer probe"}}')")
  printf '%s' "$signed" | jq -e --arg pubkey "$pubkey" '
    .pubkey == $pubkey and
    (.id | type == "string" and test("^[0-9a-f]{64}$")) and
    (.sig | type == "string" and test("^[0-9a-f]{128}$")) and
    .content == "archy lifecycle indeedhub signer probe"
  ' >/dev/null || {
    echo "indeedhub Nostr signer did not return a valid signed event: $signed" >&2
    return 1
  }
}

probe_tailscale_login_ui() {
  local body="$1"
  if grep -Eiq 'tailscale|login|log in|sign in|authenticate|authorize|auth key|connect' "$body"; then
    return 0
  fi
  echo "tailscale launch did not present login/auth UI content" >&2
  return 1
}

install_app() {
  local app="$1" app_json image params
  app_json=$(catalog_app_json "$app" || true)
  if [[ -n "$app_json" ]]; then
    params=$(printf '%s' "$app_json" | jq -c '{id, dockerImage, version, containerConfig} | with_entries(select(.value != null))')
  else
    image=$(image_for "$app")
    params=$(jq -nc --arg id "$app" --arg img "$image" '{id:$id,dockerImage:$img,version:"latest"}')
  fi
  rpc_result package.install "$params" >/dev/null
}

expect_archival_blocked_install() {
  local app="$1" app_json resp err params
  app_json=$(catalog_app_json "$app")
  params=$(printf '%s' "$app_json" | jq -c '{id, dockerImage, version, containerConfig} | with_entries(select(.value != null))')
  resp=$(rpc_call package.install "$params")
  err=$(printf '%s' "$resp" | jq -r '.error.message // empty')
  if [[ "$err" != *"Requires an archival Bitcoin node"* && "$err" != *"requires an archival Bitcoin node"* && "$err" != *"running pruned Bitcoin"* ]]; then
    echo "expected archival Bitcoin block for $app, got: $resp" >&2
    return 1
  fi
}

start_app() { rpc_result package.start "$(jq -nc --arg id "$1" '{id:$id}')" >/dev/null; }
stop_app() { rpc_result package.stop "$(jq -nc --arg id "$1" '{id:$id}')" >/dev/null; }
restart_app() { rpc_result package.restart "$(jq -nc --arg id "$1" '{id:$id}')" >/dev/null; }
uninstall_app() { rpc_result package.uninstall "$(jq -nc --arg id "$1" '{id:$id,preserve_data:true}')" >/dev/null; }

audit_app() {
  local app="$1" state rc=0
  state=$(container_state "$app" || echo unknown)
  printf '%-22s state=%s\n' "$app" "$state"
  case "$state" in
    absent) ;;
    running)
      wait_container_healthy "$app" || rc=1
      wait_launch "$app" || rc=1
      assert_launch_metadata "$app" || rc=1
      observe_stable "$app" || rc=1
      ;;
    *) echo "bad state: $app is $state" >&2; rc=1 ;;
  esac
  return "$rc"
}

full_lifecycle_app() {
  local app="$1"
  if [[ "$app" == "bitcoin-core" && "$ARCHY_ALLOW_BITCOIN_SWAP" != "1" ]]; then
    echo "skip bitcoin-core: set ARCHY_ALLOW_BITCOIN_SWAP=1 to test mutually-exclusive Bitcoin implementation"
    return 0
  fi
  if app_in_list "$app" "${ARCHIVAL_ONLY_APPS[@]}" && is_pruned_node; then
    echo "== $app: expect archival Bitcoin block =="
    expect_archival_blocked_install "$app"
    return $?
  fi
  echo "== $app: install =="
  install_app "$app" || return 1
  wait_not_installing "$app" || return 1
  wait_state "$app" running || return 1
  wait_container_healthy "$app" || return 1
  wait_launch "$app" || return 1
  assert_launch_metadata "$app" || return 1
  observe_stable "$app" || return 1

  echo "== $app: stop =="
  stop_app "$app" || return 1
  wait_state "$app" stopped 300 || return 1

  echo "== $app: start =="
  start_app "$app" || return 1
  wait_state "$app" running || return 1
  wait_container_healthy "$app" || return 1
  wait_launch "$app" || return 1
  assert_launch_metadata "$app" || return 1
  observe_stable "$app" || return 1

  echo "== $app: restart =="
  restart_app "$app" || return 1
  wait_state "$app" running || return 1
  wait_container_healthy "$app" || return 1
  wait_launch "$app" || return 1
  assert_launch_metadata "$app" || return 1
  observe_stable "$app" || return 1

  echo "== $app: uninstall preserve_data =="
  uninstall_app "$app" || return 1
  wait_absent_settled "$app" 600 || return 1

  echo "== $app: reinstall =="
  install_app "$app" || return 1
  wait_not_installing "$app" || return 1
  wait_state "$app" running || return 1
  wait_container_healthy "$app" || return 1
  wait_launch "$app" || return 1
  assert_launch_metadata "$app" || return 1
  observe_stable "$app" || return 1
}

apps=()
if [[ -n "$ARCHY_APPS" ]]; then
  IFS=',' read -r -a apps <<< "$ARCHY_APPS"
  fetch_catalog || true
elif [[ "$ARCHY_FULL_LIFECYCLE" == "1" ]]; then
  fetch_catalog
  mapfile -t apps < <(catalog_app_ids)
else
  if fetch_catalog; then
    mapfile -t apps < <(catalog_app_ids)
  else
    apps=("${ALL_APPS[@]}")
  fi
fi

rpc_login

failed=0
for i in $(seq 1 "$ARCHY_ITERATIONS"); do
  echo "### $ARCHY_HOST iteration $i / $ARCHY_ITERATIONS ###"
  for app in "${apps[@]}"; do
    if [[ "$ARCHY_FULL_LIFECYCLE" == "1" ]]; then
      full_lifecycle_app "$app" || failed=$((failed + 1))
    else
      audit_app "$app" || failed=$((failed + 1))
    fi
  done
done

if (( failed > 0 )); then
  echo "FAILED checks: $failed" >&2
  exit 1
fi
echo "all checks passed"
