#!/bin/bash
#
# First-boot container creation for Archipelago autoinstaller
# Creates core containers so My Apps works out of the box after ISO install
# Runs after archipelago-load-images.service and archipelago-setup-tor.service
#
# Container logic mirrors the deploy path - do not diverge.
# No set -e: each section continues even if one fails (idempotent, best-effort).
#
# Image versions: sourced from /opt/archipelago/image-versions.sh (single source of truth).
# All container image references use the $*_IMAGE variables defined there.
# Images pull from the Archipelago app registry (source.archipelago-foundation.org/lfg2025/).
#
# --- PLANNED REFACTOR (post-beta) ---
# This script is ~995 lines and should be split into a modular library.
# DO NOT split until tested on the build server — this is critical infrastructure.
#
LOG="/var/log/archipelago-first-boot.log"
# The unbundled path exits early; diagnostics must exist before either path.
log() { echo "$(date '+%Y-%m-%d %H:%M:%S') $*" | tee -a "$LOG"; }

# Source pinned image versions (single source of truth)
# ISO copies to scripts/ subdir; also check the direct path for manual installs
source /opt/archipelago/scripts/image-versions.sh 2>/dev/null \
  || source /opt/archipelago/image-versions.sh 2>/dev/null \
  || source /home/archipelago/archy/scripts/image-versions.sh 2>/dev/null \
  || true

# Verify image-versions loaded — fail loudly if not
if [ -z "$ARCHY_REGISTRY" ] || [ -z "$BITCOIN_KNOTS_IMAGE" ]; then
  log "FATAL: image-versions.sh not loaded — checked:"
  log "  /opt/archipelago/scripts/image-versions.sh"
  log "  /opt/archipelago/image-versions.sh"
  log "  /home/archipelago/archy/scripts/image-versions.sh"
  log "Container creation will fail. Check ISO build."
fi

# Source shared utility library
SCRIPT_DIR_FBC="$(cd "$(dirname "$0")" && pwd)"
[ -f "$SCRIPT_DIR_FBC/lib/common.sh" ] && source "$SCRIPT_DIR_FBC/lib/common.sh" || true

# Must run as root for system setup (sysctl, loginctl, subuid, chown).
# Podman commands run as the archipelago user (rootless) so the backend
# (which also runs as archipelago) can see and manage the containers.
[ "$(id -u)" -eq 0 ] || { echo "Must run as root" >&2; exit 1; }

# Run podman as the archipelago user (rootless) — NOT as root.
# The backend service runs as User=archipelago and connects to the rootless
# podman socket at /run/user/$(id -u archipelago)/podman/podman.sock. If we create containers
# as root (rootful podman), the backend can't see them at all.
DOCKER="runuser -u archipelago -- env XDG_RUNTIME_DIR=/run/user/$(id -u archipelago) podman"

prepare_filebrowser_credentials() {
  install -d -o archipelago -g archipelago -m 700 /var/lib/archipelago/secrets/filebrowser
  chown 100000:100000 /var/lib/archipelago/filebrowser /var/lib/archipelago/filebrowser-data
  runuser -u archipelago -- env XDG_RUNTIME_DIR="/run/user/$(id -u archipelago)" \
    python3 "$SCRIPT_DIR_FBC/filebrowser-credentials.py" --image "$FILEBROWSER_IMAGE"
}

PORT_ALLOC_FILE="/var/lib/archipelago/port-allocations.env"
mkdir -p /var/lib/archipelago 2>/dev/null || true
[ -f "$PORT_ALLOC_FILE" ] && . "$PORT_ALLOC_FILE"

port_available() {
  local port="$1"
  ss -ltn 2>/dev/null | awk -v p=":$port" '$4 == p || $4 ~ p "$" { found=1 } END { exit found ? 1 : 0 }'
}

alloc_port() {
  local key="$1" preferred="$2" var="PORT_${key//[^A-Za-z0-9]/_}" cur=""
  eval "cur=\${$var:-}"
  if [ -n "$cur" ] && port_available "$cur"; then
    printf '%s' "$cur"
    return
  fi
  if port_available "$preferred"; then
    cur="$preferred"
  else
    cur=""
    for p in $(seq 8085 9999); do
      if port_available "$p"; then cur="$p"; break; fi
    done
  fi
  [ -n "$cur" ] || cur="$preferred"
  if ! grep -q "^$var=" "$PORT_ALLOC_FILE" 2>/dev/null; then
    printf '%s=%s\n' "$var" "$cur" >> "$PORT_ALLOC_FILE"
  fi
  printf '%s' "$cur"
}

# UNBUNDLED mode: only create FileBrowser, skip all other containers.
# Users install apps on-demand from the Marketplace.
UNBUNDLED_MARKER="/opt/archipelago/.unbundled"
if [ -f "$UNBUNDLED_MARKER" ]; then
    log "UNBUNDLED mode detected — creating FileBrowser only (apps install from Marketplace)"

    # Core setup: secrets, podman prerequisites, FileBrowser
    SECRETS_DIR="/var/lib/archipelago/secrets"
    mkdir -p "$SECRETS_DIR" && chmod 700 "$SECRETS_DIR"
    if [ ! -f "$SECRETS_DIR/bitcoin-rpc-password" ]; then
        openssl rand -hex 16 > "$SECRETS_DIR/bitcoin-rpc-password"
        chmod 600 "$SECRETS_DIR/bitcoin-rpc-password"
    fi
    # Generate all DB passwords upfront so they're stable
    for svc in mempool btcpay mysql-root; do
        if [ ! -f "$SECRETS_DIR/${svc}-db-password" ]; then
            openssl rand -hex 16 > "$SECRETS_DIR/${svc}-db-password"
            chmod 600 "$SECRETS_DIR/${svc}-db-password"
        fi
    done
    chown -R 1000:1000 "$SECRETS_DIR"

    # Podman prerequisites
    loginctl enable-linger archipelago 2>/dev/null || true
    DOCKER="runuser -u archipelago -- env XDG_RUNTIME_DIR=/run/user/$(id -u archipelago) podman"
    # No ID mappings changed here. system migrate would stop existing apps
    # on a retry and race the backend's reconciliation.

    # Ensure archy-net exists
    $DOCKER network create archy-net 2>/dev/null || true

    # Helper: pull image with fallback registry
    pull_with_fallback() {
        local img="$1"
        # Pre-bundled ISO images are already loaded into the rootless
        # storage by archipelago-load-images.service — use them and skip
        # the network entirely (fresh installs must work offline).
        if $DOCKER image exists "$img" 2>/dev/null; then
            log "  Image already present locally: $img"
            return 0
        fi
        log "  Pulling $img..."
        if $DOCKER pull "$img" 2>>"$LOG"; then
            return 0
        fi
        # Try fallback registry
        local fallback_img
        fallback_img=$(echo "$img" | sed "s|${ARCHY_REGISTRY}|${ARCHY_REGISTRY_FALLBACK}|")
        if [ "$fallback_img" != "$img" ] && [ -n "$ARCHY_REGISTRY_FALLBACK" ]; then
            log "  Primary failed, trying fallback: $fallback_img"
            if $DOCKER pull "$fallback_img" --tls-verify=false 2>>"$LOG"; then
                $DOCKER tag "$fallback_img" "$img" 2>/dev/null
                return 0
            fi
        fi
        # Try docker.io as last resort for common images
        local short_name
        short_name=$(echo "$img" | sed 's|.*/||')
        local dockerhub="docker.io/library/$short_name"
        log "  Fallback failed, trying docker.io: $dockerhub"
        $DOCKER pull "$dockerhub" 2>>"$LOG" && $DOCKER tag "$dockerhub" "$img" 2>/dev/null && return 0
        return 1
    }

    # Create FileBrowser (unique private Cloud credentials)
    if ! $DOCKER container exists filebrowser; then
        log "Creating FileBrowser (secure Cloud login)..."
        mkdir -p /var/lib/archipelago/filebrowser /var/lib/archipelago/filebrowser-data
        chown 100000:100000 /var/lib/archipelago/filebrowser
        chown 100000:100000 /var/lib/archipelago/filebrowser-data
        pull_with_fallback "${FILEBROWSER_IMAGE}" || exit 1
        prepare_filebrowser_credentials || { log "ERROR: secure File Browser setup failed"; exit 1; }
        $DOCKER run -d --name filebrowser --restart unless-stopped \
            --network archy-net \
            --cap-drop=ALL --cap-add=DAC_OVERRIDE --cap-add=NET_BIND_SERVICE \
            --security-opt=no-new-privileges:true \
            --health-cmd='wget -q --spider http://localhost:80/health || exit 1' \
            --health-interval=30s --health-timeout=5s --health-retries=3 \
            --memory=256m \
            -p 127.0.0.1:8083:80 \
            -v /var/lib/archipelago/filebrowser:/srv \
            -v /var/lib/archipelago/filebrowser-data:/data \
            ${FILEBROWSER_IMAGE} \
            --config /data/.filebrowser.json 2>>"$LOG" && \
            log "  FileBrowser created" || log "  WARNING: FileBrowser creation failed"

    fi

    # Create Fedimint Client (fmcd) alongside FileBrowser so ecash / networking
    # sats work out of the box even on unbundled images. Image is pulled (not
    # pre-loaded). Resilient entrypoint retries on join failure → never crash-loops.
    if ! $DOCKER ps -a --format '{{.Names}}' 2>/dev/null | grep -q '^fedimint-clientd$'; then
        log "Creating Fedimint Client (fmcd)..."
        mkdir -p /var/lib/archipelago/fmcd
        FMCD_PW_FILE=/var/lib/archipelago/fmcd/password
        [ -s "$FMCD_PW_FILE" ] || head -c 24 /dev/urandom | base64 | tr -dc 'A-Za-z0-9' > "$FMCD_PW_FILE"
        # This script runs as root but the container is rootless (container
        # uid 0 → host 1000): a root-owned /data leaves fmcd crash-looping
        # with "Permission denied (os error 13)". Match manifest data_uid.
        chown -R 1000:1000 /var/lib/archipelago/fmcd
        FMCD_PW="$(cat "$FMCD_PW_FILE")"
        FMCD_DEFAULT_INVITE="fed11qgqyj3mfwfhksw309uuxywtxxfjrjc35xuexverpxdsnxcnrxucxvenzveskgc3kvvun2c34xp3k2ep38yunzdpexcekxe3hvd3rvvmx8pnrvdenx5mnzvtzqqqjqt0t6pc3s5z0ynqjw9s4njf6svwgu59kweawc0vvrddcjeemw6yyn4pcdp"
        pull_with_fallback "${FMCD_IMAGE}"
        $DOCKER run -d --name fedimint-clientd --restart unless-stopped \
            --network archy-net \
            --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
            --security-opt no-new-privileges:true \
            --health-cmd='curl -sf http://localhost:8080/health || exit 1' \
            --health-interval=120s --health-timeout=10s --health-retries=3 \
            --memory=256m \
            -p 8178:8080 \
            -v /var/lib/archipelago/fmcd:/data \
            -e FMCD_ADDR=0.0.0.0:8080 -e FMCD_MODE=rest -e FMCD_DATA_DIR=/data \
            -e FMCD_PASSWORD="$FMCD_PW" -e FMCD_INVITE_CODE="$FMCD_DEFAULT_INVITE" \
            ${FMCD_IMAGE} 2>>"$LOG" && log "  fmcd created" || log "  WARNING: fmcd creation failed"
    fi

    # Generate WireGuard keys for standalone VPN (archipelago-wg service)
    WG_DIR="/var/lib/archipelago/wireguard"
    if [ ! -f "$WG_DIR/private.key" ]; then
        log "Generating WireGuard keys..."
        mkdir -p "$WG_DIR"
        wg genkey > "$WG_DIR/private.key" 2>/dev/null
        chmod 600 "$WG_DIR/private.key"
        wg pubkey < "$WG_DIR/private.key" > "$WG_DIR/public.key"
        chown -R 1000:1000 "$WG_DIR"
        log "  WireGuard keypair generated: pubkey=$(cat "$WG_DIR/public.key")"
    fi
    # Start standalone WireGuard service (wg0:51820 on 10.44.0.1/16)
    modprobe wireguard 2>/dev/null || true
    systemctl enable --now archipelago-wg 2>/dev/null || true
    systemctl enable --now archipelago-wg-address 2>/dev/null || true
    if command -v ufw >/dev/null 2>&1 && ufw status | grep -q "Status: active"; then
        ufw allow 51820/udp >/dev/null 2>&1 || true
    fi
    log "  Standalone WireGuard started (wg0:51820)"

    log "Unbundled first-boot complete"
    exit 0
fi

TARGET_IP=$(hostname -I 2>/dev/null | awk '{print $1}')
[ -z "$TARGET_IP" ] && TARGET_IP="127.0.0.1"

# Stable mDNS hostname for federation/consensus URLs (survives DHCP / reinstalls).
# Falls back to $TARGET_IP if avahi is not available.
HOST_MDNS="$(hostname 2>/dev/null).local"
if [ -z "$HOST_MDNS" ] || [ "$HOST_MDNS" = ".local" ]; then
  HOST_MDNS="$TARGET_IP"
fi

# Map host.containers.internal to the rootless-podman host gateway.
# Podman 4.4+ supports the magic string "host-gateway" which resolves to
# the correct in-container-network gateway IP at container start. We used
# to compute a value from `ip route` here, but that returned the LAN
# router (e.g. 192.168.1.254 or 192.168.1.1) — the gateway out to the
# internet, not the gateway to the host — which broke every container
# trying to reach bitcoin-core's RPC on the host (LND, ElectrumX, etc).
ADD_HOST_FLAG="--add-host=host.containers.internal:host-gateway"

# Ensure Tor is running for hidden services (LND connect, Electrumx, etc.)
if ! systemctl is-active tor >/dev/null 2>&1; then
  log "Starting Tor..."
  systemctl enable tor 2>/dev/null || true
  systemctl start tor 2>/dev/null || true
  sleep 5
  if systemctl is-active tor >/dev/null 2>&1; then
    log "  Tor started successfully"
  else
    log "  WARNING: Tor failed to start, hidden services will be unavailable"
  fi
fi

# Populate tor-hostnames directory so the backend can read onion addresses
# The backend reads from /var/lib/archipelago/tor-hostnames/{service} at startup
TOR_HOSTNAMES="/var/lib/archipelago/tor-hostnames"
mkdir -p "$TOR_HOSTNAMES"
# Wait for Tor to generate hostname files (setup-tor.sh may still be running)
for attempt in $(seq 1 10); do
  [ -f /var/lib/tor/hidden_service_archipelago/hostname ] && break
  log "Waiting for Tor hostnames (attempt $attempt/10)..."
  sleep 3
done
for svc in archipelago bitcoin lnd electrumx btcpay mempool fedimint relay; do
  for dir in /var/lib/tor/hidden_service_${svc}; do
    if [ -f "$dir/hostname" ]; then
      cp "$dir/hostname" "$TOR_HOSTNAMES/$svc" 2>/dev/null
    fi
  done
done
chown -R archipelago:archipelago "$TOR_HOSTNAMES" 2>/dev/null
log "Tor hostnames populated: $(ls $TOR_HOSTNAMES 2>/dev/null | tr '\n' ' ')"

# ── Standalone WireGuard: generate keypair and start wg0 ──────────────
WG_DIR="/var/lib/archipelago/wireguard"
mkdir -p "$WG_DIR"
if [ ! -f "$WG_DIR/private.key" ]; then
  wg genkey > "$WG_DIR/private.key" 2>/dev/null
  chmod 600 "$WG_DIR/private.key"
  wg pubkey < "$WG_DIR/private.key" > "$WG_DIR/public.key"
  chown -R archipelago:archipelago "$WG_DIR"
  log "WireGuard keypair generated"
fi
modprobe wireguard 2>/dev/null || true
systemctl enable --now archipelago-wg 2>/dev/null || true
systemctl enable --now archipelago-wg-address 2>/dev/null || true
# Open firewall port for standalone WG
if command -v ufw >/dev/null 2>&1 && ufw status | grep -q "Status: active"; then
  ufw allow 51820/udp >/dev/null 2>&1 || true
fi
log "Standalone WireGuard (wg0:51820) started"

# ── Private Nostr Relay: start for VPN signaling and general use ──────
if command -v nostr-rs-relay >/dev/null 2>&1; then
  # Relay config is pre-installed by ISO at /var/lib/archipelago/nostr-relay/config.toml
  mkdir -p /var/lib/archipelago/nostr-relay
  if [ ! -f /var/lib/archipelago/nostr-relay/config.toml ] && [ -f /etc/archipelago/nostr-relay-config.toml ]; then
    cp /etc/archipelago/nostr-relay-config.toml /var/lib/archipelago/nostr-relay/config.toml
  fi
  chown -R archipelago:archipelago /var/lib/archipelago/nostr-relay
  systemctl enable --now nostr-relay 2>/dev/null || true
  log "Private Nostr relay started on port 7777"
else
  log "nostr-rs-relay binary not found — skipping relay setup"
fi

# ── NostrVPN: DISABLED — using standalone WireGuard only ──────────────
# NostrVPN (nvpn) is disabled for now. Standalone WireGuard (archipelago-wg)
# handles VPN with QR-based peer provisioning via the web UI.
log "NostrVPN disabled — standalone WireGuard only (wg0:51820)"

# Wait for a container to be healthy (accepting connections)
wait_for_container() {
  local name="$1" check_cmd="$2" max_wait="${3:-30}"
  local waited=0
  while [ $waited -lt $max_wait ]; do
    if eval "$check_cmd" 2>/dev/null; then
      log "  $name is ready (${waited}s)"
      return 0
    fi
    sleep 2
    waited=$((waited + 2))
  done
  log "  WARNING: $name not ready after ${max_wait}s, continuing anyway"
  return 1
}

# rpcauth: password hash in bitcoin.conf, plaintext in secrets file only.
# Credentials are STABLE across reboots, restarts, and deploys.
SECRETS_DIR="/var/lib/archipelago/secrets"
mkdir -p "$SECRETS_DIR" && chmod 700 "$SECRETS_DIR"
if [ ! -f "$SECRETS_DIR/bitcoin-rpc-password" ]; then
    openssl rand -hex 16 > "$SECRETS_DIR/bitcoin-rpc-password"
    chmod 600 "$SECRETS_DIR/bitcoin-rpc-password"
fi
# Ensure archipelago user can read secrets (backend runs as archipelago, not root)
chown -R 1000:1000 "$SECRETS_DIR"
BITCOIN_RPC_USER="archipelago"
BITCOIN_RPC_PASS=$(cat "$SECRETS_DIR/bitcoin-rpc-password" 2>/dev/null)
if [ -z "$BITCOIN_RPC_PASS" ]; then
  log "FATAL: Bitcoin RPC password is empty — secrets file missing or unreadable"
  log "  Expected: $SECRETS_DIR/bitcoin-rpc-password"
  exit 1
fi

# Generate rpcauth line for bitcoin.conf (salted HMAC-SHA256 hash)
generate_rpcauth() {
    local user="$1" pass="$2"
    local salt=$(openssl rand -hex 16)
    local hash=$(echo -n "$pass" | openssl dgst -sha256 -hmac "$salt" -hex 2>/dev/null | awk '{print $NF}')
    echo "${user}:${salt}\$${hash}"
}

# Write bitcoin.conf with rpcauth if not exists or needs update
BITCOIN_CONF="/var/lib/archipelago/bitcoin/bitcoin.conf"
if [ ! -f "$BITCOIN_CONF" ] || ! grep -q "^rpcauth=" "$BITCOIN_CONF" 2>/dev/null; then
    mkdir -p /var/lib/archipelago/bitcoin
    RPCAUTH=$(generate_rpcauth "$BITCOIN_RPC_USER" "$BITCOIN_RPC_PASS")
    cat > "$BITCOIN_CONF" << BTCCONF
# rpcauth: salted hash only — no plaintext password in config or CLI
rpcauth=${RPCAUTH}
server=1
rpcallowip=0.0.0.0/0
listen=1
# printtoconsole=0: journald log-volume fix — bitcoind's datadir debug.log
# already has everything; console duplication spammed journald during IBD.
printtoconsole=0
# ZMQ publishers for LND and other services that need real-time block/tx notifications
zmqpubrawblock=tcp://0.0.0.0:28332
zmqpubrawtx=tcp://0.0.0.0:28333
BTCCONF
    log "Generated bitcoin.conf with rpcauth (no plaintext credentials)"
fi

# Generate per-installation database passwords if not already saved
for svc in mempool btcpay mysql-root; do
    if [ ! -f "$SECRETS_DIR/${svc}-db-password" ]; then
        openssl rand -base64 24 > "$SECRETS_DIR/${svc}-db-password"
        chmod 600 "$SECRETS_DIR/${svc}-db-password"
    fi
done
MEMPOOL_DB_PASS=$(cat "$SECRETS_DIR/mempool-db-password")
BTCPAY_DB_PASS=$(cat "$SECRETS_DIR/btcpay-db-password")
MYSQL_ROOT_PASS=$(cat "$SECRETS_DIR/mysql-root-db-password")

# Fedimint gateway credential: FED-07 — no shipped fallback, ever. The
# canonical per-install credential is the manifest's `fedimint-gateway-hash`
# (+ `.pw` plaintext sibling), generated by the daemon's
# container::secrets::ensure_gateway_credential (bcrypt, 0600, idempotent,
# self-healing). This ISO first-boot path — the site that put the shipped
# default on real nodes before this fix — no longer generates the hash
# locally, which also removes the htpasswd host dependency entirely and
# keeps bcrypt generation in exactly one place.
#
# Legacy migration only: earlier builds wrote the plaintext under
# fedimint-gateway-password instead of the manifest's canonical
# fedimint-gateway-hash.pw name. Carry a pre-existing value forward under the
# canonical name rather than regenerating (migrations never destroy data or
# rotate a working credential); the legacy file is left in place — plan
# 01-16 owns its retirement.
if [ -f "$SECRETS_DIR/fedimint-gateway-password" ] && [ ! -f "$SECRETS_DIR/fedimint-gateway-hash.pw" ]; then
    cp "$SECRETS_DIR/fedimint-gateway-password" "$SECRETS_DIR/fedimint-gateway-hash.pw"
    chmod 600 "$SECRETS_DIR/fedimint-gateway-hash.pw"
    log "Migrated legacy fedimint-gateway-password -> fedimint-gateway-hash.pw"
fi
FEDI_HASH=""
if [ -f "$SECRETS_DIR/fedimint-gateway-hash" ]; then
    FEDI_HASH=$(cat "$SECRETS_DIR/fedimint-gateway-hash")
fi
if [ -z "$FEDI_HASH" ]; then
    log "Fedimint gateway credential not yet generated — the daemon/reconcile will generate a per-install bcrypt hash (no shipped default); the gateway container creation below is skipped until then"
fi

BITCOIN_READY=false
TOTAL=0
SUCCESS=0
FAILED_LIST=""

# Track container start result — call after each container creation attempt
track_container() {
  local name="$1"
  TOTAL=$((TOTAL + 1))
  if $DOCKER ps --filter "name=^${name}$" --format '{{.Names}}' 2>/dev/null | grep -q "^${name}$"; then
    SUCCESS=$((SUCCESS + 1))
    log "  [OK] $name is running"
  else
    FAILED_LIST="$FAILED_LIST $name"
    log "  [FAIL] $name is NOT running"
  fi
}

log "First-boot container creation starting (host=$TARGET_IP)"

# Create swap file if not present (50% of RAM, min 2GB, max 8GB)
if ! swapon --show | grep -q /swapfile; then
    TOTAL_MEM_KB=$(awk '/MemTotal/ {print $2}' /proc/meminfo)
    SWAP_MB=$((TOTAL_MEM_KB / 2 / 1024))
    [ "$SWAP_MB" -lt 2048 ] && SWAP_MB=2048
    [ "$SWAP_MB" -gt 8192 ] && SWAP_MB=8192
    log "Creating ${SWAP_MB}MB swap file..."
    if dd if=/dev/zero of=/swapfile bs=1M count="$SWAP_MB" status=progress 2>>"$LOG"; then
        chmod 600 /swapfile
        mkswap /swapfile >>"$LOG" 2>&1
        swapon /swapfile
        if ! grep -q '/swapfile' /etc/fstab; then
            echo '/swapfile none swap sw 0 0' >> /etc/fstab
        fi
        log "Swap created: ${SWAP_MB}MB"
    else
        log "WARNING: Failed to create swap file"
    fi
else
    log "Swap already configured"
fi

# Rootless podman prerequisites (run as root, configures for archipelago user)
log "Setting up rootless podman prerequisites..."
if ! command -v catatonit >/dev/null 2>&1; then
    log "Installing catatonit for Podman init support..."
    if command -v apt-get >/dev/null 2>&1; then
        apt-get update >>"$LOG" 2>&1 || true
        apt-get install -y catatonit >>"$LOG" 2>&1 || true
    elif command -v dnf >/dev/null 2>&1; then
        dnf install -y catatonit >>"$LOG" 2>&1 || true
    elif command -v apk >/dev/null 2>&1; then
        apk add catatonit >>"$LOG" 2>&1 || true
    fi
fi
# Allow binding to ports >= 80 (rootless default is 1024)
if ! grep -q "unprivileged_port_start=80" /etc/sysctl.d/99-rootless-podman.conf 2>/dev/null; then
    echo "net.ipv4.ip_unprivileged_port_start=80" > /etc/sysctl.d/99-rootless-podman.conf
    sysctl -p /etc/sysctl.d/99-rootless-podman.conf 2>/dev/null
    log "  Rootless port binding enabled (>=80)"
fi
# Linger for container persistence after logout
if [ "$(loginctl show-user archipelago 2>/dev/null | grep Linger)" != "Linger=yes" ]; then
    loginctl enable-linger archipelago 2>/dev/null
    log "  Linger enabled for archipelago user"
fi
# Ensure subuid/subgid mappings
grep -q "^archipelago:" /etc/subuid 2>/dev/null || {
    echo "archipelago:100000:65536" >> /etc/subuid
    echo "archipelago:100000:65536" >> /etc/subgid
    log "  subuid/subgid configured"
    # Only refresh the namespace when mappings actually changed.
    $DOCKER system migrate 2>/dev/null || true
}
# Ensure /etc/hosts is readable (rootless podman needs it)
chmod 644 /etc/hosts 2>/dev/null

# Ensure XDG_RUNTIME_DIR exists for rootless podman
mkdir -p /run/user/$(id -u archipelago)
chown archipelago:archipelago /run/user/$(id -u archipelago)
chmod 700 /run/user/$(id -u archipelago)
# Start rootless podman socket (required before first podman command)
runuser -u archipelago -- env XDG_RUNTIME_DIR=/run/user/$(id -u archipelago) \
    systemctl --user start podman.socket 2>/dev/null || true

# Ensure archy-net exists — critical for inter-container DNS (mempool→bitcoin, etc.)
$DOCKER network create archy-net 2>/dev/null || true
if ! $DOCKER network exists archy-net 2>/dev/null; then
  log "WARNING: archy-net creation failed, retrying in 5s..."
  sleep 5
  $DOCKER network create archy-net 2>>"$LOG"
  if ! $DOCKER network exists archy-net 2>/dev/null; then
    log "FATAL: Cannot create archy-net — inter-container DNS will not work."
    log "  All containers requiring archy-net will fail. Exiting."
    exit 1
  fi
fi
log "archy-net network ready"

# Rootless podman UID mapping: fix data dir ownership so container processes
# can write. Rootless podman maps container UIDs via subuid (container UID N
# → host UID 100000+N). Must run BEFORE container creation.
log "Fixing rootless podman UID mapping..."
# Containers running as root (UID 0 → host UID 100000)
for dir in lnd electrumx btcpay nbxplorer jellyfin vaultwarden \
            home-assistant fedimint fedimint-gateway photoprism ollama filebrowser \
           nextcloud uptime-kuma portainer nostr-rs-relay; do
    [ -d "/var/lib/archipelago/$dir" ] && chown -R 100000:100000 "/var/lib/archipelago/$dir" 2>/dev/null
done
# Nginx Proxy Manager runs as root in the rootless user namespace, which maps to
# the archipelago user on host bind mounts. Keep certbot's webroot writable.
# Existing NPM ownership is preserved; new storage is initialized below.
# Bitcoin Knots: container UID 101 → host UID 100101
[ -d /var/lib/archipelago/bitcoin ] && chown -R 100101:100101 /var/lib/archipelago/bitcoin 2>/dev/null
# Postgres: container UID 70 → host UID 100070
for dir in postgres-btcpay; do
    [ -d "/var/lib/archipelago/$dir" ] && chown -R 100070:100070 "/var/lib/archipelago/$dir" 2>/dev/null
done
# MariaDB: container UID 999 → host UID 100999
for dir in mempool mysql-mempool; do
    [ -d "/var/lib/archipelago/$dir" ] && chown -R 100999:100999 "/var/lib/archipelago/$dir" 2>/dev/null
done
# Grafana: chown inside podman's user namespace so container UID 472 can write SQLite.
[ -d /var/lib/archipelago/grafana ] && podman unshare chown -R 472:472 /var/lib/archipelago/grafana 2>/dev/null || true
log "UID mapping done"

# ── Memory limits per container ──────────────────────────────────────────
# Matches core/archipelago/src/api/rpc/package.rs get_memory_limit()
# Prevents a single runaway container from OOMing the whole system.
TOTAL_MEM_MB=$(($(awk '/MemTotal/{print $2}' /proc/meminfo) / 1024))
LOW_MEM=false
[ "$TOTAL_MEM_MB" -lt 12000 ] && LOW_MEM=true && log "Low-memory system (${TOTAL_MEM_MB}MB) — reducing limits"

mem_limit() {
  case "$1" in
    bitcoin|bitcoin-core|bitcoin-knots) $LOW_MEM && echo "4g"   || echo "8g";;
    cryptpad)            echo "512m";;
    ollama)              $LOW_MEM && echo "1g"   || echo "4g";;
    lnd)                 echo "512m";;
    electrumx|mempool-electrs|electrs) echo "4g";;
    nextcloud)           echo "1g";;
    btcpay-server|btcpayserver) echo "1g";;
    homeassistant)       echo "512m";;
    fedimint)            echo "512m";;
    fedimint-gateway)    echo "512m";;
    fedimint-clientd)    echo "256m";;
    photoprism)          $LOW_MEM && echo "512m" || echo "1g";;
    mempool-api)         echo "512m";;
    jellyfin)            echo "1g";;
    searxng)             echo "512m";;
    archy-btcpay-db)     echo "512m";;
    archy-nbxplorer)     echo "512m";;
    archy-mempool-db)    echo "512m";;
    archy-mempool-web)   echo "256m";;
    grafana)             echo "256m";;
    vaultwarden)         echo "256m";;
    uptime-kuma)         echo "256m";;
    filebrowser)         echo "256m";;
    portainer)           echo "256m";;
    nginx-proxy-manager) echo "256m";;
    tailscale)           echo "256m";;
    indeedhub|archy-bitcoin-ui|archy-lnd-ui|archy-electrs-ui) echo "128m";;
    *)                   echo "512m";;
  esac
}

# ── Verify critical images are loaded ──────────────────────────────────
# archipelago-load-images.service should have loaded these from tarballs.
# If any are missing (corrupt tarball, disk full, etc.), try re-loading.
log "Verifying container images..."
MISSING_IMAGES=""
for img_var in BITCOIN_KNOTS_IMAGE MARIADB_IMAGE ELECTRUMX_IMAGE \
               MEMPOOL_BACKEND_IMAGE MEMPOOL_WEB_IMAGE BTCPAY_POSTGRES_IMAGE \
               NBXPLORER_IMAGE BTCPAY_IMAGE LND_IMAGE FEDIMINT_IMAGE \
               FEDIMINT_GATEWAY_IMAGE FMCD_IMAGE HOMEASSISTANT_IMAGE GRAFANA_IMAGE \
               UPTIME_KUMA_IMAGE JELLYFIN_IMAGE VAULTWARDEN_IMAGE \
               NEXTCLOUD_IMAGE SEARXNG_IMAGE FILEBROWSER_IMAGE; do
  img="${!img_var}"
  if [ -z "$img" ]; then
    continue  # Variable not defined in image-versions.sh
  fi
  if ! $DOCKER images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep -qF "$img"; then
    MISSING_IMAGES="$MISSING_IMAGES $img_var"
  fi
done
if [ -n "$MISSING_IMAGES" ]; then
  log "WARNING: Missing images:$MISSING_IMAGES"
  log "Attempting to re-load from /opt/archipelago/container-images/..."
  RELOAD_COUNT=0
  for tarfile in /opt/archipelago/container-images/*.tar; do
    if [ -f "$tarfile" ]; then
      if $DOCKER load -i "$tarfile" 2>>"$LOG"; then
        RELOAD_COUNT=$((RELOAD_COUNT + 1))
      else
        log "  Failed to load: $tarfile"
      fi
    fi
  done
  log "Re-loaded $RELOAD_COUNT image tarballs"
else
  log "All critical images verified"
fi

# ── Tier 1: Databases & Core Infrastructure ──────────────────────────────
log "=== Tier 1: Databases & Core Infrastructure ==="

# 1. Bitcoin Knots (matches deploy exactly)
# Auto-detect: if disk < 1TB, use pruning to prevent disk-full crashes
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -qE 'bitcoin-knots|archy-bitcoin-knots'; then
  log "Creating Bitcoin Knots..."
  mkdir -p /var/lib/archipelago/bitcoin
  # Check the DATA partition size, not root — Bitcoin data goes to /var/lib/archipelago
  DISK_GB=$(df --output=size -BG /var/lib/archipelago 2>/dev/null | tail -1 | tr -dc '0-9')
  [ -z "$DISK_GB" ] && DISK_GB=$(df --output=size -BG / 2>/dev/null | tail -1 | tr -dc '0-9')
  if [ "${DISK_GB:-0}" -lt 1000 ]; then
    BTC_EXTRA_ARGS="-prune=550"
    if [ "$LOW_MEM" = "true" ]; then
      BTC_DBCACHE=2048
    else
      BTC_DBCACHE=4096
    fi
    log "  Small disk (${DISK_GB}GB) — enabling pruning"
  else
    BTC_EXTRA_ARGS="-txindex=1"
    BTC_DBCACHE=4096
    log "  Large disk (${DISK_GB}GB) — enabling txindex"
  fi
  if $DOCKER run -d --name bitcoin-knots --restart unless-stopped \
    --health-cmd="bitcoin-cli -datadir=/home/bitcoin/.bitcoin getblockchaininfo || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit bitcoin-knots) --network archy-net --network-alias bitcoin-knots \
    $ADD_HOST_FLAG \
    --cap-drop ALL --cap-add CHOWN --cap-add FOWNER --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
    --security-opt no-new-privileges:true \
    -p 8332:8332 -p 8333:8333 -p 28332:28332 -p 28333:28333 \
    -v /var/lib/archipelago/bitcoin:/home/bitcoin/.bitcoin \
    "${BITCOIN_KNOTS_IMAGE}" \
    $BTC_EXTRA_ARGS \
    -printtoconsole=0 -dbcache=$BTC_DBCACHE -par=0 -maxconnections=125 -rpcthreads=16 -rpcworkqueue=256 2>>"$LOG"; then
    log "Bitcoin Knots started"
  else
    log "Bitcoin Knots failed (may already exist)"
  fi
else
  $DOCKER network connect archy-net bitcoin-knots 2>/dev/null || true
  log "Bitcoin Knots already running"
fi
# Check Bitcoin Knots RPC (informational — containers created regardless)
# Dependent containers use --restart=unless-stopped and the health monitor
# will auto-restart them once Bitcoin becomes responsive.
if wait_for_container "Bitcoin Knots RPC" "$DOCKER exec bitcoin-knots bitcoin-cli -datadir=/home/bitcoin/.bitcoin getblockchaininfo" 60; then
  BITCOIN_READY=true
  log "Bitcoin Knots is ready"
else
  BITCOIN_READY=false
  log "Bitcoin Knots not yet responsive (normal during IBD) — creating dependent containers anyway"
  log "  They will auto-restart via health monitor once Bitcoin is ready"
fi
track_container "bitcoin-knots"

# Ensure wallet exists (Bitcoin Knots no longer auto-creates a default wallet)
if ! $DOCKER exec bitcoin-knots bitcoin-cli -datadir=/home/bitcoin/.bitcoin listwallets 2>/dev/null | grep -q "archipelago"; then
  $DOCKER exec bitcoin-knots bitcoin-cli -datadir=/home/bitcoin/.bitcoin loadwallet "archipelago" 2>/dev/null || \
  $DOCKER exec bitcoin-knots bitcoin-cli -datadir=/home/bitcoin/.bitcoin createwallet "archipelago" 2>/dev/null
  log "Bitcoin Knots wallet 'archipelago' created/loaded"
fi

# ── Bootstrap: use a remote Bitcoin node during IBD ───────────────────
# If the local node is still syncing (IBD=true), point dependent services at
# a fully-synced bootstrap node so wallets/payments work immediately.
BOOTSTRAP_CONF="/opt/archipelago/bootstrap.conf"
BOOTSTRAP_FLAG="/var/lib/archipelago/.bootstrap-active"
USE_BOOTSTRAP=false
BTC_HOST="bitcoin-knots"       # default: local container via archy-net DNS
BTC_RPC_USER="$BITCOIN_RPC_USER"
BTC_RPC_PASS="$BITCOIN_RPC_PASS"

if [ -f "$BOOTSTRAP_CONF" ]; then
  . "$BOOTSTRAP_CONF"
  if [ -n "${BOOTSTRAP_RPC_PASS:-}" ]; then
    # Check if local Bitcoin is in IBD
    LOCAL_IBD=$($DOCKER exec bitcoin-knots bitcoin-cli -datadir=/home/bitcoin/.bitcoin getblockchaininfo 2>/dev/null \
      | python3 -c "import sys,json; print(json.load(sys.stdin).get('initialblockdownload',True))" 2>/dev/null) || LOCAL_IBD="True"
    if [ "$LOCAL_IBD" = "True" ]; then
      BOOT_USER="${BOOTSTRAP_RPC_USER:-archipelago}"
      BOOT_TEST='{"jsonrpc":"1.0","id":"boot","method":"getblockcount","params":[]}'

      # Try 1: LAN (fast, ~1ms)
      if [ -n "${BOOTSTRAP_LAN_HOST:-}" ] && \
         curl -sf --max-time 5 -u "${BOOT_USER}:${BOOTSTRAP_RPC_PASS}" \
           -H "Content-Type: application/json" -d "$BOOT_TEST" \
           "http://${BOOTSTRAP_LAN_HOST}:8332/" >/dev/null 2>&1; then
        USE_BOOTSTRAP=true
        BTC_HOST="$BOOTSTRAP_LAN_HOST"
        BTC_RPC_USER="$BOOT_USER"
        BTC_RPC_PASS="$BOOTSTRAP_RPC_PASS"
        touch "$BOOTSTRAP_FLAG"
        echo "lan" > "$BOOTSTRAP_FLAG"
        log "BOOTSTRAP: Local Bitcoin in IBD — using LAN ${BOOTSTRAP_LAN_HOST} for dependent services"

      # Try 2: Tor (works from any network, ~5-15s)
      elif [ -n "${BOOTSTRAP_ONION:-}" ] && command -v socat >/dev/null 2>&1; then
        log "BOOTSTRAP: LAN unreachable, trying Tor (${BOOTSTRAP_ONION})..."
        # Create a socat tunnel: localhost:18332 → onion:8332 via Tor SOCKS
        socat TCP-LISTEN:18332,bind=127.0.0.1,reuseaddr,fork \
          SOCKS4A:127.0.0.1:${BOOTSTRAP_ONION}:8332,socksport=9050 &
        SOCAT_PID=$!
        sleep 3
        if curl -sf --max-time 30 -u "${BOOT_USER}:${BOOTSTRAP_RPC_PASS}" \
             -H "Content-Type: application/json" -d "$BOOT_TEST" \
             "http://127.0.0.1:18332/" >/dev/null 2>&1; then
          USE_BOOTSTRAP=true
          # Containers reach host via host.containers.internal (set by $ADD_HOST_FLAG)
          BTC_HOST="$TARGET_IP"
          BTC_HOST_PORT=18332
          BTC_RPC_USER="$BOOT_USER"
          BTC_RPC_PASS="$BOOTSTRAP_RPC_PASS"
          echo "tor:$SOCAT_PID" > "$BOOTSTRAP_FLAG"
          log "BOOTSTRAP: Using Tor tunnel (socat pid=$SOCAT_PID) for dependent services"
          # Persist the tunnel as a systemd service so it survives first-boot
          cat > /etc/systemd/system/archipelago-bootstrap-tunnel.service <<TUNNELSVC
[Unit]
Description=Bootstrap Bitcoin RPC tunnel via Tor
After=tor.service

[Service]
Type=simple
User=archipelago
ExecStart=/usr/bin/socat TCP-LISTEN:18332,bind=127.0.0.1,reuseaddr,fork SOCKS4A:127.0.0.1:${BOOTSTRAP_ONION}:8332,socksport=9050
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
TUNNELSVC
          systemctl daemon-reload
          systemctl enable --now archipelago-bootstrap-tunnel.service 2>/dev/null || true
          # Kill the ad-hoc socat — systemd takes over
          kill "$SOCAT_PID" 2>/dev/null || true
        else
          kill "$SOCAT_PID" 2>/dev/null || true
          log "BOOTSTRAP: Tor tunnel test failed — using local Bitcoin"
        fi
      else
        log "BOOTSTRAP: No reachable bootstrap node — using local Bitcoin"
      fi

      if [ "$USE_BOOTSTRAP" = "true" ]; then
        log "  Services will auto-switch to local node when synced (bootstrap-switchover timer)"
      fi
    else
      log "BOOTSTRAP: Local Bitcoin already synced — no bootstrap needed"
    fi
  fi
fi

# Override port if Tor tunnel is active (containers use host gateway:18332 instead of :8332)
BTC_PORT=${BTC_HOST_PORT:-8332}

# 2. Mempool stack (matches deploy) — depends on Bitcoin
# Note: containers created regardless of BITCOIN_READY — they will restart
# automatically once Bitcoin becomes responsive (--restart=unless-stopped).
if ! $DOCKER ps -a --format '{{.Names}}' 2>/dev/null | grep -qE 'archy-mempool-db|mysql-mempool'; then
  log "Creating mysql-mempool..."
  mkdir -p /var/lib/archipelago/mysql-mempool
  $DOCKER run -d --name archy-mempool-db --restart unless-stopped \
    --health-cmd="mariadb -uroot -e 'SELECT 1' || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit archy-mempool-db) --network archy-net --network-alias archy-mempool-db \
    --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
    --security-opt no-new-privileges:true \
    -v /var/lib/archipelago/mysql-mempool:/var/lib/mysql \
    -e MYSQL_DATABASE=mempool -e MYSQL_USER=mempool -e "MYSQL_PASSWORD=$MEMPOOL_DB_PASS" \
    -e "MYSQL_ROOT_PASSWORD=$MYSQL_ROOT_PASS" \
    "$MARIADB_IMAGE" 2>>"$LOG" || true
  wait_for_container "Mempool MariaDB" "echo 'SELECT 1' | $DOCKER exec -i archy-mempool-db mariadb -uroot --password=\"$MYSQL_ROOT_PASS\"" 30
fi
MYSQL_CNT=$($DOCKER ps -a --format '{{.Names}}' 2>/dev/null | grep -E 'mysql-mempool|archy-mempool-db' | head -1)
MYSQL_CNT=${MYSQL_CNT:-archy-mempool-db}
$DOCKER network connect archy-net "$MYSQL_CNT" 2>/dev/null || true
track_container "archy-mempool-db"

if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q electrumx; then
  if $DOCKER ps -a --format '{{.Names}}' 2>/dev/null | grep -q electrumx; then
    $DOCKER start electrumx 2>/dev/null || true
  else
    log "Creating electrumx..."
    mkdir -p /var/lib/archipelago/electrumx
    $DOCKER run -d --name electrumx --restart unless-stopped \
      --health-cmd="curl -sf http://localhost:8000/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
      --memory=$(mem_limit electrumx) --network archy-net --network-alias electrumx \
      --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID \
      --security-opt no-new-privileges:true \
      -p 50001:50001 -v /var/lib/archipelago/electrumx:/data \
      -e "DAEMON_URL=http://$BTC_RPC_USER:$BTC_RPC_PASS@$BTC_HOST:$BTC_PORT/" \
      -e COIN=Bitcoin -e DB_DIRECTORY=/data \
      -e SERVICES=tcp://:50001,rpc://0.0.0.0:8000 \
      "$ELECTRUMX_IMAGE" 2>>"$LOG" || true
  fi
fi
track_container "electrumx"

if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q mempool-api; then
  log "Creating mempool-api..."
  mkdir -p /var/lib/archipelago/mempool
  $DOCKER run -d --name mempool-api --restart unless-stopped \
    --health-cmd="curl -sf http://localhost:8999/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit mempool-api) --network archy-net --network-alias mempool-api \
    --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID \
    --security-opt no-new-privileges:true \
    -p 8999:8999 -v /var/lib/archipelago/mempool:/data \
    -e MEMPOOL_BACKEND=electrum -e ELECTRUM_HOST=electrumx -e ELECTRUM_PORT=50001 \
    -e ELECTRUM_TLS_ENABLED=false -e "CORE_RPC_HOST=$BTC_HOST" -e CORE_RPC_PORT=8332 \
    -e "CORE_RPC_USERNAME=$BTC_RPC_USER" -e "CORE_RPC_PASSWORD=$BTC_RPC_PASS" \
    -e DATABASE_ENABLED=true -e DATABASE_HOST="$MYSQL_CNT" -e DATABASE_DATABASE=mempool \
    -e DATABASE_USERNAME=mempool -e "DATABASE_PASSWORD=$MEMPOOL_DB_PASS" \
    "$MEMPOOL_BACKEND_IMAGE" 2>>"$LOG" || true
fi
track_container "mempool-api"

if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -qE 'archy-mempool-web|mempool-web'; then
  log "Creating mempool frontend..."
  $DOCKER run -d --name archy-mempool-web --restart unless-stopped \
    --health-cmd="curl -sf http://localhost:8080/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit archy-mempool-web) --network archy-net --network-alias archy-mempool-web \
    --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID \
    --security-opt no-new-privileges:true \
    -p 4080:8080 -e FRONTEND_HTTP_PORT=8080 -e BACKEND_MAINNET_HTTP_HOST=mempool-api \
    "$MEMPOOL_WEB_IMAGE" 2>>"$LOG" || true
fi
track_container "archy-mempool-web"

# 2b. ElectrumX UI (status dashboard on port 50002, host network for backend access)
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q electrs-ui; then
  if $DOCKER images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep -q 'electrs-ui'; then
    log "Starting ElectrumX UI from pre-built image..."
    $DOCKER run -d --name archy-electrs-ui --network host --restart unless-stopped \
      --health-cmd="curl -sf http://localhost:80/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
      --user 0:0 \
      --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
      localhost/electrs-ui:local 2>>"$LOG" || \
    $DOCKER run -d --name archy-electrs-ui --network host --restart unless-stopped \
      --health-cmd="curl -sf http://localhost:80/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
      --user 0:0 \
      --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
      electrs-ui:local 2>>"$LOG" || true
  elif [ -d /opt/archipelago/docker/electrs-ui ]; then
    log "Building and starting ElectrumX UI from source..."
    $DOCKER build -t electrs-ui:local /opt/archipelago/docker/electrs-ui 2>>"$LOG" && \
    $DOCKER run -d --name archy-electrs-ui --network host --restart unless-stopped \
      --health-cmd="curl -sf http://localhost:80/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
      --user 0:0 \
      --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
      electrs-ui:local 2>>"$LOG" || true
  else
    log "ElectrumX UI: no image or source found, skipping"
  fi
fi

# 3. BTCPay stack (matches deploy) — depends on Bitcoin
if ! $DOCKER ps -a --format '{{.Names}}' 2>/dev/null | grep -qE 'archy-btcpay-db|postgres-btcpay'; then
  log "Creating PostgreSQL for BTCPay..."
  mkdir -p /var/lib/archipelago/postgres-btcpay
  $DOCKER run -d --name archy-btcpay-db --restart unless-stopped \
    --health-cmd="pg_isready -U postgres || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit archy-btcpay-db) --network archy-net --network-alias archy-btcpay-db \
    --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
    --security-opt no-new-privileges:true \
    -v /var/lib/archipelago/postgres-btcpay:/var/lib/postgresql/data \
    -e POSTGRES_DB=btcpay -e POSTGRES_USER=btcpay -e "POSTGRES_PASSWORD=$BTCPAY_DB_PASS" \
    "$BTCPAY_POSTGRES_IMAGE" 2>>"$LOG" || true
  wait_for_container "BTCPay PostgreSQL" "$DOCKER exec archy-btcpay-db pg_isready -U postgres" 30
fi
track_container "archy-btcpay-db"
# Create nbxplorer DB only if postgres is running
if $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -qE 'archy-btcpay-db|postgres-btcpay'; then
  $DOCKER exec archy-btcpay-db psql -U postgres -tc "SELECT 1 FROM pg_database WHERE datname='nbxplorer'" 2>/dev/null | grep -q 1 || \
    $DOCKER exec -e "PGPASSWORD=$BTCPAY_DB_PASS" archy-btcpay-db psql -U postgres -c "CREATE DATABASE nbxplorer;" 2>/dev/null || true
fi

if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q archy-nbxplorer; then
  if $DOCKER ps -a --format '{{.Names}}' 2>/dev/null | grep -q archy-nbxplorer; then
    $DOCKER start archy-nbxplorer 2>/dev/null || true
  else
    log "Creating NBXplorer..."
    mkdir -p /var/lib/archipelago/nbxplorer/Main
    $DOCKER run -d --name archy-nbxplorer --restart unless-stopped \
      --health-cmd="curl -sf http://localhost:32838/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
      --memory=$(mem_limit archy-nbxplorer) --network archy-net --network-alias archy-nbxplorer \
      --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID \
      --security-opt no-new-privileges:true \
      -p 32838:32838 -v /var/lib/archipelago/nbxplorer:/data \
      -e NBXPLORER_DATADIR=/data -e NBXPLORER_NETWORK=mainnet -e NBXPLORER_CHAINS=btc \
      -e NBXPLORER_BIND=0.0.0.0:32838 -e "NBXPLORER_BTCRPCURL=http://$BTC_HOST:$BTC_PORT" \
      -e "NBXPLORER_BTCRPCUSER=$BTC_RPC_USER" -e "NBXPLORER_BTCRPCPASSWORD=$BTC_RPC_PASS" \
      -e NBXPLORER_POSTGRES='User ID=btcpay;Password=$BTCPAY_DB_PASS;Host=archy-btcpay-db;Port=5432;Database=nbxplorer;Include Error Detail=true' \
      "$NBXPLORER_IMAGE" 2>>"$LOG" && sleep 5 || true
  fi
fi
track_container "archy-nbxplorer"

if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q btcpay-server; then
  log "Creating BTCPay Server..."
  mkdir -p /var/lib/archipelago/btcpay/Main
  $DOCKER run -d --name btcpay-server --restart unless-stopped \
    --health-cmd="bash -ec '</dev/tcp/127.0.0.1/49392'" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit btcpay-server) --network archy-net --network-alias btcpay-server \
    --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
    --security-opt no-new-privileges:true \
    -p 23000:49392 -v /var/lib/archipelago/btcpay:/datadir \
    -e ASPNETCORE_URLS=http://0.0.0.0:49392 -e BTCPAY_PROTOCOL=http \
    -e BTCPAY_HOST="$TARGET_IP:23000" -e BTCPAY_CHAINS=btc \
    -e BTCPAY_BTCEXPLORERURL=http://archy-nbxplorer:32838 \
    -e "BTCPAY_BTCRPCURL=http://$BTC_HOST:$BTC_PORT" \
    -e "BTCPAY_BTCRPCUSER=$BTC_RPC_USER" -e "BTCPAY_BTCRPCPASSWORD=$BTC_RPC_PASS" \
    -e BTCPAY_POSTGRES='User ID=btcpay;Password=$BTCPAY_DB_PASS;Host=archy-btcpay-db;Port=5432;Database=btcpay;Include Error Detail=true' \
    "$BTCPAY_IMAGE" 2>>"$LOG" || true
fi
track_container "btcpay-server"

# ── Tier 2: Core Services ─────────────────────────────────────────────────
log "=== Tier 2: Core Services ==="
sleep 5  # Let databases stabilize

# 4. LND — depends on Bitcoin
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -qE '^lnd$'; then
  log "Creating LND..."
  mkdir -p /var/lib/archipelago/lnd
  # Create lnd.conf with rpcauth credentials (stable across restarts)
  if [ ! -f /var/lib/archipelago/lnd/lnd.conf ]; then
    cat > /var/lib/archipelago/lnd/lnd.conf <<LNDCONF
[Application Options]
listen=0.0.0.0:9735
rpclisten=0.0.0.0:10009
restlisten=0.0.0.0:8080
debuglevel=info
noseedbackup=true
tlsextraip=0.0.0.0
tlsextradomain=lnd
tor.active=true
tor.socks=host.containers.internal:9050
tor.streamisolation=true

[Bitcoin]
bitcoin.mainnet=true
bitcoin.node=bitcoind

[Bitcoind]
bitcoind.rpchost=$BTC_HOST:$BTC_PORT
bitcoind.rpcuser=$BTC_RPC_USER
bitcoind.rpcpass=$BTC_RPC_PASS
bitcoind.rpcpolling=true
bitcoind.estimatemode=ECONOMICAL

[autopilot]
autopilot.active=false
LNDCONF
    log "LND config created (rpcauth credentials, Tor via system)"
  fi
  $DOCKER run -d --name lnd --restart unless-stopped \
    --health-cmd="curl -sf --insecure https://localhost:8080/v1/getinfo || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit lnd) --network archy-net --network-alias lnd \
    $ADD_HOST_FLAG \
    --cap-drop ALL --cap-add CHOWN --cap-add FOWNER --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE --cap-add NET_RAW \
    --security-opt no-new-privileges:true \
    -p 9735:9735 -p 10009:10009 -p 18080:8080 \
    -v /var/lib/archipelago/lnd:/root/.lnd \
    "$LND_IMAGE" 2>>"$LOG" || true
fi
track_container "lnd"

# 5. Fedimint — depends on Bitcoin
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q fedimint; then
  log "Creating Fedimint..."
  mkdir -p /var/lib/archipelago/fedimint
  chmod 775 /var/lib/archipelago/fedimint  # fedimint container runs as non-root
  $DOCKER run -d --name fedimint --restart unless-stopped \
    --health-cmd="curl -sf http://localhost:8174/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit fedimint) --network archy-net --network-alias fedimint \
    --cap-drop ALL --cap-add CHOWN --cap-add FOWNER --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
    --security-opt no-new-privileges:true \
    -p 8173:8173 -p 8174:8174 -p 8175:8175 \
    -v /var/lib/archipelago/fedimint:/data \
    -e FM_DATA_DIR=/data -e "FM_BITCOIND_USERNAME=$BTC_RPC_USER" -e "FM_BITCOIND_PASSWORD=$BTC_RPC_PASS" \
    -e FM_BITCOIN_NETWORK=bitcoin -e FM_BIND_P2P=0.0.0.0:8173 \
    -e FM_BIND_API=0.0.0.0:8174 -e FM_BIND_UI=0.0.0.0:8175 \
    -e FM_P2P_URL=fedimint://"$HOST_MDNS":8173 -e FM_API_URL=ws://"$HOST_MDNS":8174 \
    -e "FM_BITCOIND_URL=http://$BTC_HOST:$BTC_PORT" \
    "$FEDIMINT_IMAGE" 2>>"$LOG" || true
fi
track_container "fedimint"

# 5b. Fedimint Gateway (companion to fedimint)
# Auto-detect LND: if running with credentials, use lnd mode; otherwise use ldk (built-in)
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q fedimint-gateway; then
  if [ -z "$FEDI_HASH" ]; then
    # FED-07: no shipped fallback. Without a per-install credential yet, do
    # not start gatewayd with an empty --bcrypt-password-hash — skip creation
    # here; reconcile-containers.sh creates it once the daemon has generated
    # the credential.
    log "  Skipping Fedimint Gateway creation — no per-install credential yet (no shipped default); reconcile will create it once one is generated"
  else
    log "Creating Fedimint Gateway..."
    mkdir -p /var/lib/archipelago/fedimint-gateway
    LND_CERT=/var/lib/archipelago/lnd/tls.cert
    LND_MACAROON=/var/lib/archipelago/lnd/data/chain/bitcoin/mainnet/admin.macaroon
    if $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q '^lnd$' && [ -f "$LND_CERT" ] && [ -f "$LND_MACAROON" ]; then
      log "  LND detected — using lnd mode"
      $DOCKER run -d --name fedimint-gateway --restart unless-stopped \
        --health-cmd="curl -sf http://localhost:8176/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
        --memory=$(mem_limit fedimint-gateway) --network archy-net --network-alias fedimint-gateway \
        --cap-drop ALL --cap-add CHOWN --cap-add FOWNER --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
        --security-opt no-new-privileges:true \
        -p 8176:8176 \
        -v /var/lib/archipelago/fedimint-gateway:/data \
        -v "$LND_CERT":/lnd/tls.cert:ro \
        -v "$LND_MACAROON":/lnd/admin.macaroon:ro \
        "$FEDIMINT_GATEWAY_IMAGE" \
        gatewayd --data-dir /data --listen 0.0.0.0:8176 \
        --bcrypt-password-hash "$FEDI_HASH" \
        --network bitcoin --bitcoind-url "http://$BTC_HOST:$BTC_PORT" \
        --bitcoind-username "$BTC_RPC_USER" --bitcoind-password "$BTC_RPC_PASS" \
        lnd --lnd-rpc-host lnd:10009 --lnd-tls-cert /lnd/tls.cert --lnd-macaroon /lnd/admin.macaroon 2>>"$LOG" || true
    else
      log "  No LND found — using ldk (built-in Lightning)"
      $DOCKER run -d --name fedimint-gateway --restart unless-stopped \
        --health-cmd="curl -sf http://localhost:8176/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
        --memory=$(mem_limit fedimint-gateway) --network archy-net --network-alias fedimint-gateway \
        --cap-drop ALL --cap-add CHOWN --cap-add FOWNER --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
        --security-opt no-new-privileges:true \
        -p 8176:8176 -p 9737:9737 \
        -v /var/lib/archipelago/fedimint-gateway:/data \
        "$FEDIMINT_GATEWAY_IMAGE" \
        gatewayd --data-dir /data --listen 0.0.0.0:8176 \
        --bcrypt-password-hash "$FEDI_HASH" \
        --network bitcoin --bitcoind-url "http://$BTC_HOST:$BTC_PORT" \
        --bitcoind-username "$BTC_RPC_USER" --bitcoind-password "$BTC_RPC_PASS" \
        ldk --ldk-lightning-port 9737 --ldk-alias archipelago-gateway 2>>"$LOG" || true
    fi
  fi
fi
track_container "fedimint-gateway"

# 5c. Fedimint Client (fmcd) — ecash client daemon; auto-joins the default
# federation. The image's resilient entrypoint retries on join failure (fmcd
# needs >=1 federation to boot), so an unreachable default never crash-loops the
# node. The archipelago wallet bridge reads the password from
# /var/lib/archipelago/fmcd/password and talks to it on host port 8178.
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q '^fedimint-clientd$'; then
  log "Creating Fedimint Client (fmcd)..."
  mkdir -p /var/lib/archipelago/fmcd
  FMCD_PW_FILE=/var/lib/archipelago/fmcd/password
  [ -s "$FMCD_PW_FILE" ] || head -c 24 /dev/urandom | base64 | tr -dc 'A-Za-z0-9' > "$FMCD_PW_FILE"
  # Root-created /data breaks the rootless container (uid 0 → host 1000):
  # fmcd crash-loops with "Permission denied (os error 13)". Match data_uid.
  chown -R 1000:1000 /var/lib/archipelago/fmcd
  FMCD_PW="$(cat "$FMCD_PW_FILE")"
  FMCD_DEFAULT_INVITE="fed11qgqyj3mfwfhksw309uuxywtxxfjrjc35xuexverpxdsnxcnrxucxvenzveskgc3kvvun2c34xp3k2ep38yunzdpexcekxe3hvd3rvvmx8pnrvdenx5mnzvtzqqqjqt0t6pc3s5z0ynqjw9s4njf6svwgu59kweawc0vvrddcjeemw6yyn4pcdp"
  $DOCKER run -d --name fedimint-clientd --restart unless-stopped \
    --health-cmd="curl -sf http://localhost:8080/health || exit 1" --health-interval=120s --health-timeout=10s --health-retries=3 \
    --memory=$(mem_limit fedimint-clientd) --network archy-net --network-alias fedimint-clientd \
    --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
    --security-opt no-new-privileges:true \
    -p 8178:8080 \
    -v /var/lib/archipelago/fmcd:/data \
    -e FMCD_ADDR=0.0.0.0:8080 -e FMCD_MODE=rest -e FMCD_DATA_DIR=/data \
    -e FMCD_PASSWORD="$FMCD_PW" -e FMCD_INVITE_CODE="$FMCD_DEFAULT_INVITE" \
    "$FMCD_IMAGE" 2>>"$LOG" || true
fi
track_container "fedimint-clientd"

# (Bitcoin-dependent containers created above regardless of BITCOIN_READY)

# ── Tier 3: Applications (independent — always attempt) ───────────────────
log "=== Tier 3: Applications ==="
sleep 5  # Let core services stabilize

# 6. Home Assistant
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -qE 'homeassistant|home-assistant'; then
  log "Creating Home Assistant..."
  mkdir -p /var/lib/archipelago/home-assistant
  $DOCKER run -d --name homeassistant --restart unless-stopped \
    --health-cmd="curl -sf http://localhost:8123/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit homeassistant) \
    --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
    --security-opt no-new-privileges:true \
    -p 8123:8123 -v /var/lib/archipelago/home-assistant:/config \
    -e TZ=UTC \
    "$HOMEASSISTANT_IMAGE" 2>>"$LOG" || true
fi
track_container "homeassistant"

# 7. Single-container apps (Grafana, Uptime Kuma, Jellyfin, PhotoPrism, Ollama, Vaultwarden)
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q grafana; then
  log "Creating Grafana..."
  mkdir -p /var/lib/archipelago/grafana
  podman unshare chown -R 472:472 /var/lib/archipelago/grafana 2>/dev/null || true
  $DOCKER run -d --name grafana --restart unless-stopped \
    --health-cmd="test -w /var/lib/grafana && test -w /var/lib/grafana/grafana.db && curl -sf http://localhost:3000/api/health || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit grafana) \
    --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID \
    --security-opt no-new-privileges:true \
    --read-only --tmpfs /tmp:rw,noexec,nosuid,size=256m --tmpfs /run:rw,noexec,nosuid,size=64m \
    -p 3000:3000 -v /var/lib/archipelago/grafana:/var/lib/grafana \
    -e GF_PATHS_DATA=/var/lib/grafana -e GF_USERS_ALLOW_SIGN_UP=false \
    "$GRAFANA_IMAGE" 2>>"$LOG" || true
fi
track_container "grafana"
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q uptime-kuma; then
  log "Creating Uptime Kuma..."
  mkdir -p /var/lib/archipelago/uptime-kuma
  $DOCKER run -d --name uptime-kuma --restart unless-stopped \
    --health-cmd="curl -sf http://localhost:3001/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit uptime-kuma) \
    --cap-drop ALL --cap-add CHOWN --cap-add FOWNER --cap-add SETUID --cap-add SETGID \
    --security-opt no-new-privileges:true \
    -p 3001:3001 -v /var/lib/archipelago/uptime-kuma:/app/data \
    -e TZ=UTC \
    "$UPTIME_KUMA_IMAGE" 2>>"$LOG" || true
fi
track_container "uptime-kuma"
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q jellyfin; then
  log "Creating Jellyfin..."
  mkdir -p /var/lib/archipelago/jellyfin/config /var/lib/archipelago/jellyfin/cache
  $DOCKER run -d --name jellyfin --restart unless-stopped \
    --health-cmd="curl -sf http://localhost:8096/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit jellyfin) \
    --cap-drop ALL --cap-add CHOWN --cap-add FOWNER --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
    --security-opt no-new-privileges:true \
    --tmpfs /tmp:rw,exec,size=256m \
    -p 8096:8096 \
    -v /var/lib/archipelago/jellyfin/config:/config \
    -v /var/lib/archipelago/jellyfin/cache:/cache \
    "$JELLYFIN_IMAGE" 2>>"$LOG" || true
fi
track_container "jellyfin"
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q photoprism; then
  log "Creating PhotoPrism..."
  mkdir -p /var/lib/archipelago/photoprism
  $DOCKER run -d --name photoprism --restart unless-stopped \
    --health-cmd="curl -sf http://localhost:2342/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit photoprism) \
    --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID \
    --security-opt no-new-privileges:true \
    -p 2342:2342 -v /var/lib/archipelago/photoprism:/photoprism/storage \
    -e PHOTOPRISM_ADMIN_PASSWORD=archipelago -e PHOTOPRISM_DEFAULT_LOCALE=en \
    "${PHOTOPRISM_IMAGE}" 2>>"$LOG" || true
fi
track_container "photoprism"
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q ollama; then
  log "Creating Ollama..."
  mkdir -p /var/lib/archipelago/ollama
  $DOCKER run -d --name ollama --restart unless-stopped \
    --health-cmd="curl -sf http://localhost:11434/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit ollama) \
    --cap-drop ALL --security-opt no-new-privileges:true \
    --read-only --tmpfs /tmp:rw,noexec,nosuid,size=256m --tmpfs /run:rw,noexec,nosuid,size=64m \
    -p 11434:11434 -v /var/lib/archipelago/ollama:/root/.ollama \
    "${OLLAMA_IMAGE}" 2>>"$LOG" || true
fi
track_container "ollama"
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q vaultwarden; then
  log "Creating Vaultwarden..."
  mkdir -p /var/lib/archipelago/vaultwarden
  $DOCKER run -d --name vaultwarden --restart unless-stopped \
    --health-cmd="curl -sf http://localhost:80/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit vaultwarden) \
    --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID --cap-add NET_BIND_SERVICE \
    --security-opt no-new-privileges:true \
    -p 8082:80 -v /var/lib/archipelago/vaultwarden:/data \
    "$VAULTWARDEN_IMAGE" 2>>"$LOG" || true
fi
track_container "vaultwarden"
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q nextcloud; then
  log "Creating Nextcloud..."
  mkdir -p /var/lib/archipelago/nextcloud
  $DOCKER run -d --name nextcloud --restart unless-stopped \
    --health-cmd="curl -sf http://localhost:80/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit nextcloud) \
    --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
    --security-opt no-new-privileges:true \
    -p 8085:80 -v /var/lib/archipelago/nextcloud:/var/www/html \
    "$NEXTCLOUD_IMAGE" 2>>"$LOG" || true
fi
track_container "nextcloud"
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q searxng; then
  log "Creating SearXNG..."
  # SearXNG requires settings.yml or it exits immediately
  SEARXNG_CONF="/var/lib/archipelago/searxng"
  if [ ! -f "$SEARXNG_CONF/settings.yml" ]; then
    mkdir -p "$SEARXNG_CONF"
    SEARX_SECRET=$(openssl rand -hex 32)
    cat > "$SEARXNG_CONF/settings.yml" <<SEARXCFG
use_default_settings: true
general:
  instance_name: Archipelago Search
server:
  secret_key: "$SEARX_SECRET"
  bind_address: "0.0.0.0"
  port: 8080
  limiter: false
# json is required: SearXNG defaults to html-only and answers 403 on the
# JSON API, which is the only thing AIUI's web search speaks.
search:
  formats:
    - html
    - json
ui:
  default_theme: simple
SEARXCFG
    chown -R 100000:100000 "$SEARXNG_CONF" 2>/dev/null
    log "  Created SearXNG settings.yml"
  fi
  $DOCKER run -d --name searxng --restart unless-stopped \
    --health-cmd="curl -sf http://localhost:8080/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit searxng) \
    --cap-drop ALL --security-opt no-new-privileges:true \
    --read-only --tmpfs /tmp:rw,noexec,nosuid,size=256m --tmpfs /run:rw,noexec,nosuid,size=64m \
    -p 8888:8080 \
    -v /var/lib/archipelago/searxng:/etc/searxng \
    "${SEARXNG_IMAGE}" 2>>"$LOG" || true
fi
track_container "searxng"
# OnlyOffice removed — incompatible with rootless Podman (internal postgres/rabbitmq)
# CryptPad is the replacement (single Node.js process, e2e encrypted)
if ! $DOCKER container exists filebrowser; then
  log "Creating File Browser (secure Cloud login)..."
  mkdir -p /var/lib/archipelago/filebrowser /var/lib/archipelago/filebrowser-data
  $DOCKER image exists "$FILEBROWSER_IMAGE" || $DOCKER pull "$FILEBROWSER_IMAGE" || exit 1
  prepare_filebrowser_credentials || { log "ERROR: secure File Browser setup failed"; exit 1; }
  $DOCKER run -d --name filebrowser --restart unless-stopped \
    --health-cmd="wget -q --spider http://localhost:80/health || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit filebrowser) \
    --cap-drop ALL --cap-add=DAC_OVERRIDE --cap-add=NET_BIND_SERVICE --security-opt no-new-privileges:true \
    --tmpfs=/tmp:rw,noexec,nosuid,size=256m --tmpfs=/run:rw,noexec,nosuid,size=64m \
    -p 127.0.0.1:8083:80 \
    -v /var/lib/archipelago/filebrowser:/srv \
    -v /var/lib/archipelago/filebrowser-data:/data \
    "$FILEBROWSER_IMAGE" \
    --config /data/.filebrowser.json 2>>"$LOG" || true

fi
track_container "filebrowser"
# Resolve existing storage before creating anything; never revive a stopped
# container through a duplicate run or recursively rewrite its ownership.
if ! $DOCKER container exists nginx-proxy-manager; then
  if NPM_LAYOUT=$(python3 "$SCRIPT_DIR_FBC/npm-public-bridge.py" --resolve); then
    NPM_DATA_DIR=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["data"])' <<<"$NPM_LAYOUT")
    NPM_CERT_DIR=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["certificates"])' <<<"$NPM_LAYOUT")
    if [ -n "$NPM_DATA_DIR" ] && [ -n "$NPM_CERT_DIR" ]; then
      for npm_dir in "$NPM_DATA_DIR" "$NPM_CERT_DIR" "$NPM_DATA_DIR/letsencrypt-acme-challenge/.well-known/acme-challenge"; do
        if [ ! -d "$npm_dir" ]; then
          install -d -o archipelago -g archipelago "$npm_dir" || exit 1
        fi
      done
      NPM_ADMIN_PORT=$(alloc_port nginx-proxy-manager 8081)
      python3 "$SCRIPT_DIR_FBC/npm-public-bridge.py" --resolve --prepare-realip >/dev/null || exit 1
      NPM_HTTP_PORT=$(alloc_port nginx-proxy-manager-http 8088)
      NPM_HTTPS_PORT=$(alloc_port nginx-proxy-manager-https 8444)
      if ! $DOCKER run -d --name nginx-proxy-manager --restart unless-stopped \
        --network slirp4netns:allow_host_loopback=true,cidr=169.254.1.0/24 \
        --health-cmd="curl -sf http://127.0.0.1:81/api/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
        --memory=$(mem_limit nginx-proxy-manager) \
        --cap-drop ALL --cap-add CHOWN --cap-add FOWNER --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE --cap-add NET_BIND_SERVICE \
        --security-opt no-new-privileges:true \
        -p "127.0.0.1:${NPM_ADMIN_PORT}:81" -p "127.0.0.1:${NPM_HTTP_PORT}:80" -p "127.0.0.1:${NPM_HTTPS_PORT}:443" \
        -v "$NPM_DATA_DIR:/data" -v "$NPM_CERT_DIR:/etc/letsencrypt" \
        "${NPM_IMAGE}" 2>>"$LOG"; then
        log "ERROR: NPM creation failed; existing persistent state preserved"
      fi
    fi
  else
    log "ERROR: NPM storage resolution failed; no directories or containers changed"
  fi
fi
track_container "nginx-proxy-manager"
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q portainer; then
  log "Creating Portainer..."
  mkdir -p /var/lib/archipelago/portainer/compose
  chown -R archipelago:archipelago /var/lib/archipelago/portainer 2>/dev/null || true
  if [ ! -e /data ]; then
    ln -s /var/lib/archipelago/portainer /data 2>/dev/null || true
  elif [ -d /data ] && [ ! -L /data ] && [ ! -e /data/compose ]; then
    ln -s /var/lib/archipelago/portainer/compose /data/compose 2>/dev/null || true
  fi
  $DOCKER run -d --name portainer --restart unless-stopped \
    --health-cmd="curl -sf http://localhost:9000/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit portainer) \
    --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE \
    --security-opt no-new-privileges:true \
    -p 9000:9000 \
    -v /var/lib/archipelago/portainer:/data \
    -v /var/lib/archipelago/portainer/compose:/data/compose \
    -v /run/user/$(id -u archipelago)/podman/podman.sock:/var/run/docker.sock \
    "$PORTAINER_IMAGE" 2>>"$LOG" || true
fi
track_container "portainer"
if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q tailscale; then
  log "Creating Tailscale..."
  mkdir -p /var/lib/archipelago/tailscale
  # Tailscale needs NET_ADMIN + NET_RAW + TUN device (no --privileged)
  $DOCKER run -d --name tailscale --restart unless-stopped \
    --health-cmd="tailscale status || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
    --memory=$(mem_limit tailscale) \
    --network host \
    --cap-drop=ALL \
    --cap-add=NET_ADMIN \
    --cap-add=NET_RAW \
    --security-opt no-new-privileges:true \
    --device=/dev/net/tun:/dev/net/tun \
    --read-only \
    --tmpfs /tmp \
    -v /var/lib/archipelago/tailscale:/var/lib/tailscale \
    -e TS_STATE_DIR=/var/lib/tailscale \
    "$TAILSCALE_IMAGE" \
    sh -c 'tailscaled --tun=userspace-networking & for i in $(seq 1 30); do [ -S /var/run/tailscale/tailscaled.sock ] && break; sleep 1; done; tailscale web --listen 0.0.0.0:8240 & wait' 2>>"$LOG" || true
fi
track_container "tailscale"

# Immich stack (postgres + redis + server - ML optional)
# 8. Nostr relays (optional - only if images were loaded; deploy does not create these on first boot)
# nostr-rs-relay and strfry are in ISO image bundle; create if image exists
if $DOCKER images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep -q 'nostr-rs-relay'; then
  if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q nostr-rs-relay; then
    log "Creating nostr-rs-relay..."
    mkdir -p /var/lib/archipelago/nostr-rs-relay
    $DOCKER run -d --name nostr-rs-relay --restart unless-stopped \
      --health-cmd="curl -sf http://localhost:8080/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
      --memory=$(mem_limit nostr-rs-relay) \
      --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID \
      --security-opt no-new-privileges:true \
      --read-only --tmpfs /tmp:rw,noexec,nosuid,size=32m \
      -p 7047:7047 -v /var/lib/archipelago/nostr-rs-relay:/data \
      "${NOSTR_RS_RELAY_IMAGE}" 2>>"$LOG" || true
  fi
fi
if $DOCKER images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep -q 'strfry'; then
  if ! $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q strfry; then
    log "Creating strfry..."
    mkdir -p /var/lib/archipelago/strfry
    $DOCKER run -d --name strfry --restart unless-stopped \
      --health-cmd="curl -sf http://localhost:7777/ || exit 1" --health-interval=120s --health-timeout=5s --health-retries=3 \
      --memory=$(mem_limit strfry) \
      --cap-drop ALL --cap-add CHOWN --cap-add SETUID --cap-add SETGID \
      --security-opt no-new-privileges:true \
      -p 7777:7777 -v /var/lib/archipelago/strfry:/data \
      "${STRFRY_IMAGE}" 2>>"$LOG" || true
  fi
fi

# 9. Custom UI containers (bitcoin-ui, lnd-ui)
# These are built from Dockerfiles in /opt/archipelago/docker/ or loaded from pre-built images.

# Inject Bitcoin RPC auth into bitcoin-ui nginx.conf BEFORE building
RPC_USER="archipelago"
RPC_PASS_FILE="/var/lib/archipelago/secrets/bitcoin-rpc-password"
if [ -f "$RPC_PASS_FILE" ]; then
  RPC_PASS=$(cat "$RPC_PASS_FILE")
  AUTH_B64=$(echo -n "${RPC_USER}:${RPC_PASS}" | base64)
  for ui_dir in /opt/archipelago/docker/bitcoin-ui /home/archipelago/archy/docker/bitcoin-ui; do
    if [ -f "$ui_dir/nginx.conf" ]; then
      sed -i "s|__BITCOIN_RPC_AUTH__|${AUTH_B64}|g" "$ui_dir/nginx.conf"
      log "Injected Bitcoin RPC auth into $ui_dir/nginx.conf"
    fi
  done
fi

for ui in bitcoin-ui lnd-ui electrs-ui; do
  if $DOCKER ps --format '{{.Names}}' 2>/dev/null | grep -q "$ui"; then
    continue
  fi
  case $ui in
    # UI containers use --network host so they can proxy to localhost services
    # Internal nginx ports: bitcoin-ui=8334, electrs-ui=50002, lnd-ui=80 (host 18083)
    bitcoin-ui)  PORT_ARG=""; NET_ARG="--network host"; REG_IMG="${BITCOIN_UI_IMAGE}" ;;
    # Host-networked like its siblings, NOT bridge with -p 18083:80.
    # docker/lnd-ui/nginx.conf listens on 18083 directly (it must, to proxy the
    # backend on 127.0.0.1:5678 same-origin), so publishing 18083->80 maps the
    # host port at a container port nothing serves: reproduced as HTTP 000.
    # container-specs.sh and apps/lnd-ui/manifest.yml were corrected; this was
    # the third copy of the same declaration and still broke FRESH installs.
    lnd-ui)      PORT_ARG=""; NET_ARG="--network host"; REG_IMG="${LND_UI_IMAGE}" ;;
    electrs-ui)  PORT_ARG=""; NET_ARG="--network host"; REG_IMG="${ELECTRS_UI_IMAGE}" ;;
  esac
  CONTAINER_NAME="archy-$ui"
  UI_CAPS="--user 0:0 --cap-drop ALL --cap-add CHOWN --cap-add FOWNER --cap-add SETUID --cap-add SETGID --cap-add DAC_OVERRIDE"

  # Try registry image first, then local image, then build from source
  if [ -n "$REG_IMG" ] && $DOCKER pull --tls-verify=false "$REG_IMG" 2>>"$LOG"; then
    log "Starting $ui from registry ($REG_IMG)..."
    $DOCKER run -d --name "$CONTAINER_NAME" $PORT_ARG --restart unless-stopped --memory=$(mem_limit "$CONTAINER_NAME") $NET_ARG \
      $UI_CAPS "$REG_IMG" 2>>"$LOG" || true
  elif $DOCKER images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep -q "$ui"; then
    log "Starting $ui from local image..."
    IMG=$($DOCKER images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep "$ui" | head -1)
    $DOCKER run -d --name "$CONTAINER_NAME" $PORT_ARG --restart unless-stopped --memory=$(mem_limit "$CONTAINER_NAME") $NET_ARG \
      $UI_CAPS "$IMG" 2>>"$LOG" || true
  elif [ -d "/opt/archipelago/docker/$ui" ]; then
    log "Building $ui from source..."
    if $DOCKER build -t "$ui:local" "/opt/archipelago/docker/$ui" 2>>"$LOG"; then
      $DOCKER run -d --name "$CONTAINER_NAME" $PORT_ARG --restart unless-stopped --memory=$(mem_limit "$CONTAINER_NAME") $NET_ARG \
        $UI_CAPS "$ui:local" 2>>"$LOG" || true
    fi
  else
    log "$ui: no image or source found, skipping"
  fi
done

# 10. Initialize backend data directories
# tor-config: backend stores tor service configs here (writable by archipelago user)
mkdir -p /var/lib/archipelago/tor-config
SERVICES_JSON=/var/lib/archipelago/tor-config/services.json
if [ ! -f "$SERVICES_JSON" ]; then
  cat > "$SERVICES_JSON" <<'SJSON'
{"services":[
  {"name":"archipelago","local_port":80,"enabled":true},
  {"name":"lnd","local_port":18083,"enabled":true},
  {"name":"btcpay","local_port":23000,"enabled":true},
  {"name":"mempool","local_port":4080,"enabled":true},
  {"name":"fedimint","local_port":8175,"enabled":true}
]}
SJSON
  log "Created initial tor-config/services.json"
fi

# identity: node Ed25519 keypair (DID) — MUST persist across deployments
mkdir -p /var/lib/archipelago/identity
# identities: backend identity manager stores user DIDs here
mkdir -p /var/lib/archipelago/identities

# Ensure archipelago user can write to these directories
chown -R 1000:1000 /var/lib/archipelago/tor-config /var/lib/archipelago/identity /var/lib/archipelago/identities 2>/dev/null || true

# 11. Run container doctor for any remaining issues
log "Running container doctor..."
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
if [ -x "$SCRIPT_DIR/container-doctor.sh" ]; then
    bash "$SCRIPT_DIR/container-doctor.sh" --local 2>&1 | tee -a "$LOG"
elif [ -x "/opt/archipelago/scripts/container-doctor.sh" ]; then
    bash "/opt/archipelago/scripts/container-doctor.sh" --local 2>&1 | tee -a "$LOG"
fi

# 11b. If any containers failed, run the reconciler to attempt recovery
FAILED=$((TOTAL - SUCCESS))
if [ "$FAILED" -gt 0 ]; then
  log "Attempting to recover $FAILED failed container(s) via reconciler..."
  RECONCILE_SCRIPT=""
  if [ -x "$SCRIPT_DIR/reconcile-containers.sh" ]; then
    RECONCILE_SCRIPT="$SCRIPT_DIR/reconcile-containers.sh"
  elif [ -x "/opt/archipelago/scripts/reconcile-containers.sh" ]; then
    RECONCILE_SCRIPT="/opt/archipelago/scripts/reconcile-containers.sh"
  fi
  if [ -n "$RECONCILE_SCRIPT" ]; then
    runuser -u archipelago -- bash "$RECONCILE_SCRIPT" 2>&1 | tee -a "$LOG"
    # Recount after reconciliation
    SUCCESS=0
    for name in $($DOCKER ps --format '{{.Names}}' 2>/dev/null); do
      SUCCESS=$((SUCCESS + 1))
    done
    FAILED=$((TOTAL - SUCCESS))
    log "After reconciliation: $SUCCESS running, $FAILED still failed"
  fi
fi

# 12. Final summary
log "============================================="
log "  FIRST-BOOT CONTAINER SUMMARY"
log "============================================="
log "  Total tracked:  $TOTAL"
log "  Running:        $SUCCESS"
log "  Failed:         $FAILED"
if [ "$BITCOIN_READY" != "true" ]; then
  log "  Bitcoin:        NOT READY (dependent containers will auto-restart when ready)"
fi
if [ -n "$FAILED_LIST" ]; then
  log "  Failed list:   $FAILED_LIST"
fi
log "============================================="
log "First-boot container creation complete"
