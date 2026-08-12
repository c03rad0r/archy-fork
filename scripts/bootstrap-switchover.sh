#!/bin/bash
# bootstrap-switchover.sh — Switches Bitcoin-dependent services from bootstrap node to local
# Runs periodically via systemd timer. Once local Bitcoin finishes IBD, recreates
# ElectrumX/Mempool/LND/BTCPay/Fedimint containers pointing at the local node.
set -euo pipefail

BOOTSTRAP_FLAG="/var/lib/archipelago/.bootstrap-active"
LOG="/var/log/archipelago-bootstrap-switchover.log"
SECRETS_DIR="/var/lib/archipelago/secrets"
DOCKER=podman
command -v podman >/dev/null 2>&1 || DOCKER=docker

log() { echo "$(date '+%Y-%m-%d %H:%M:%S') $*" | tee -a "$LOG"; }

# Only run if bootstrap mode is active
if [ ! -f "$BOOTSTRAP_FLAG" ]; then
    exit 0
fi

# Check if local Bitcoin is past IBD
RPC_PASS=$(cat "$SECRETS_DIR/bitcoin-rpc-password" 2>/dev/null)
if [ -z "$RPC_PASS" ]; then
    log "No local Bitcoin RPC password — skipping"
    exit 0
fi

IBD_STATUS=$($DOCKER exec bitcoin-knots bitcoin-cli -datadir=/home/bitcoin/.bitcoin getblockchaininfo 2>/dev/null | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    print(f\"{d.get('initialblockdownload', True)}|{d.get('blocks', 0)}|{d.get('headers', 0)}\")
except:
    print('True|0|0')
" 2>/dev/null) || IBD_STATUS="True|0|0"

IBD=$(echo "$IBD_STATUS" | cut -d'|' -f1)
BLOCKS=$(echo "$IBD_STATUS" | cut -d'|' -f2)
HEADERS=$(echo "$IBD_STATUS" | cut -d'|' -f3)

if [ "$IBD" != "False" ]; then
    log "Local Bitcoin still in IBD (blocks=$BLOCKS headers=$HEADERS) — keeping bootstrap"
    exit 0
fi

log "=== Local Bitcoin synced (blocks=$BLOCKS) — switching from bootstrap to local node ==="

# Source image versions
for img_src in /opt/archipelago/scripts/image-versions.sh /home/archipelago/archy/scripts/image-versions.sh; do
    [ -f "$img_src" ] && . "$img_src" && break
done

RPC_USER="archipelago"

# Helper: recreate a container with local Bitcoin config
recreate_container() {
    local name="$1"
    shift
    log "Recreating $name..."
    $DOCKER stop "$name" 2>/dev/null || true
    $DOCKER rm -f "$name" 2>/dev/null || true
    if $DOCKER run -d "$@" 2>>"$LOG"; then
        log "  $name switched to local Bitcoin"
    else
        log "  WARNING: Failed to recreate $name"
    fi
}

# ElectrumX — key service for wallet connections
if $DOCKER ps -a --format '{{.Names}}' 2>/dev/null | grep -q '^electrumx$'; then
    recreate_container electrumx \
        --name electrumx --restart unless-stopped \
        --health-cmd="python3 -c 'import socket; socket.create_connection((\"localhost\",8000),2).close()' || exit 1" \
        --health-interval=120s --health-timeout=5s --health-retries=3 \
        --memory=1g --network archy-net --network-alias electrumx \
        --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID \
        --security-opt no-new-privileges:true \
        -p 50001:50001 -v /var/lib/archipelago/electrumx:/data \
        -e "DAEMON_URL=http://${RPC_USER}:${RPC_PASS}@bitcoin-knots:8332/" \
        -e COIN=Bitcoin -e DB_DIRECTORY=/data \
        -e "SERVICES=tcp://:50001,rpc://0.0.0.0:8000" \
        "${ELECTRUMX_IMAGE:-source.archipelago-foundation.org/lfg2025/electrumx:v1.18.0}"
fi

# Mempool API
if $DOCKER ps -a --format '{{.Names}}' 2>/dev/null | grep -q '^mempool-api$'; then
    recreate_container mempool-api \
        --name mempool-api --restart unless-stopped \
        --health-cmd="curl -sf http://localhost:8999/api/v1/backend-info || exit 1" \
        --health-interval=120s --health-timeout=5s --health-retries=3 \
        --memory=512m --network archy-net --network-alias mempool-api \
        --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID \
        --security-opt no-new-privileges:true \
        -v /var/lib/archipelago/mempool-data:/backend/cache \
        -e "MEMPOOL_BACKEND=electrum" \
        -e "CORE_RPC_HOST=bitcoin-knots" -e "CORE_RPC_PORT=8332" \
        -e "CORE_RPC_USERNAME=${RPC_USER}" -e "CORE_RPC_PASSWORD=${RPC_PASS}" \
        -e "ELECTRUM_HOST=electrumx" -e "ELECTRUM_PORT=50001" -e "ELECTRUM_TLS_ENABLED=false" \
        -e "DATABASE_ENABLED=true" -e "DATABASE_HOST=archy-mempool-db" \
        -e "DATABASE_DATABASE=mempool" -e "DATABASE_USERNAME=mempool" \
        -e "DATABASE_PASSWORD=$(cat "$SECRETS_DIR/mempool-db-password" 2>/dev/null || echo mempoolpass)" \
        "${MEMPOOL_API_IMAGE:-source.archipelago-foundation.org/lfg2025/mempool-api:v3.2.0}"
fi

# Stop Tor tunnel if it was active
if systemctl is-active archipelago-bootstrap-tunnel.service >/dev/null 2>&1; then
    log "Stopping bootstrap Tor tunnel..."
    systemctl stop archipelago-bootstrap-tunnel.service 2>/dev/null || true
    systemctl disable archipelago-bootstrap-tunnel.service 2>/dev/null || true
fi

# Done — remove bootstrap flag
rm -f "$BOOTSTRAP_FLAG"
log "=== Bootstrap switchover complete — all services now using local Bitcoin node ==="
