#!/bin/bash
# Container specification registry — SINGLE SOURCE OF TRUTH
# Every container's exact creation spec lives here.
# Sourced by reconcile-containers.sh, first-boot-containers.sh, deploy scripts.
#
# Usage:
#   source container-specs.sh
#   load_spec "bitcoin-knots"   # Sets SPEC_* variables
#   all_specs                    # Returns ordered list of all containers

[ -n "${_CONTAINER_SPECS_LOADED:-}" ] && return 0
_CONTAINER_SPECS_LOADED=1

# Source image versions
for f in /opt/archipelago/image-versions.sh \
         "$(dirname "${BASH_SOURCE[0]}")/image-versions.sh" \
         "$(dirname "${BASH_SOURCE[0]}")/../image-versions.sh"; do
  [ -f "$f" ] && { source "$f"; break; }
done

# Source common utilities (mem_limit)
for f in "$(dirname "${BASH_SOURCE[0]}")/lib/common.sh" \
         /opt/archipelago/scripts/lib/common.sh; do
  [ -f "$f" ] && { source "$f"; break; }
done

# ── Environment detection ─────────────────────────────────────────────
detect_environment() {
  # Measure disk where container data actually lives, not the OS partition.
  # Archipelago installs mount a separate (usually-encrypted) data volume at
  # /var/lib/archipelago on any host with meaningful storage, so checking /
  # would always report the ~30 GB OS partition and wrongly trip prune mode
  # on 2 TB boxes. Fall back to / only for first-boot before the data
  # partition is mounted.
  local disk_target="/var/lib/archipelago"
  [ -d "$disk_target" ] || disk_target="/"
  DISK_GB=$(df --output=size -BG "$disk_target" 2>/dev/null | tail -1 | tr -dc '0-9')
  DISK_GB=${DISK_GB:-500}
  TOTAL_MEM_MB=$(($(awk '/MemTotal/{print $2}' /proc/meminfo 2>/dev/null || echo 16000000) / 1024))
  LOW_MEM=false
  [ "$TOTAL_MEM_MB" -lt 12000 ] && LOW_MEM=true
  # Bitcoin UTXO cache (dbcache) sized to host RAM, NOT a fixed value.
  # A large dbcache on a small box pushes total memory (bitcoind + the ~20 app
  # containers) past physical RAM and forces system-wide swap thrash: the disk
  # saturates, bitcoind can't answer its own RPC, and the dashboard backend's
  # sqlite reads stall — surfacing as fleet-wide /rpc/v1 502s and a blank
  # Bitcoin UI. The old binary LOW_MEM->2048 toggle still over-committed 8 GB
  # nodes. Budget ~1/16 of RAM for the cache, leaving the bulk for the OS +
  # containers; floor 300 MB (bitcoind default is 450), cap 4096 MB.
  BTC_DBCACHE=$(( TOTAL_MEM_MB / 16 ))
  [ "$BTC_DBCACHE" -lt 300 ] && BTC_DBCACHE=300
  [ "$BTC_DBCACHE" -gt 4096 ] && BTC_DBCACHE=4096
  HOST_IP=$(hostname -I 2>/dev/null | awk '{print $1}')
  HOST_IP=${HOST_IP:-127.0.0.1}
  # Stable mDNS hostname for URLs that get baked into federation/consensus data.
  # Survives DHCP churn and reinstalls-on-different-IP (which $HOST_IP does not).
  # Requires avahi-daemon (shipped on all Archipelago nodes).
  HOST_MDNS="$(hostname 2>/dev/null).local"
  HOST_MDNS="${HOST_MDNS:-archipelago.local}"

  # Secrets
  SECRETS_DIR="/var/lib/archipelago/secrets"
  BITCOIN_RPC_USER="archipelago"
  BITCOIN_RPC_PASS=$(cat "$SECRETS_DIR/bitcoin-rpc-password" 2>/dev/null || echo "")
  MEMPOOL_DB_PASS=$(cat "$SECRETS_DIR/mempool-db-password" 2>/dev/null || echo "")
  BTCPAY_DB_PASS=$(cat "$SECRETS_DIR/btcpay-db-password" 2>/dev/null || echo "")
  MYSQL_ROOT_PASS=$(cat "$SECRETS_DIR/mysql-root-db-password" 2>/dev/null || echo "")
  FEDI_HASH=$(cat "$SECRETS_DIR/fedimint-gateway-hash" 2>/dev/null || echo "")
  # Escape $ so SPEC_ENTRYPOINT survives eval in reconcile-containers.sh:build_run_cmd.
  # bcrypt hashes have the form $2y$10$... and get mangled if $2 and $10 are
  # interpolated as positional args at eval time.
  FEDI_HASH="${FEDI_HASH//\$/\\\$}"
}

# ── Spec variables ────────────────────────────────────────────────────
# Each load_spec_* function sets these variables:
#   SPEC_NAME          Container name
#   SPEC_IMAGE         Full image reference (pinned)
#   SPEC_NETWORK       Network mode (archy-net, bridge, host)
#   SPEC_PORTS         Space-separated host:container port pairs
#   SPEC_VOLUMES       Space-separated host:container volume mappings
#   SPEC_MEMORY        Memory limit (e.g. 2g, 512m)
#   SPEC_CAPS          Space-separated capabilities to add
#   SPEC_SECURITY      Security options
#   SPEC_RESTART       Restart policy
#   SPEC_HEALTH_CMD    Health check command
#   SPEC_ENV           Space-separated KEY=VALUE environment variables
#   SPEC_CUSTOM_ARGS   Extra args appended to podman run
#   SPEC_READONLY      true/false for --read-only
#   SPEC_TMPFS         Space-separated tmpfs mounts
#   SPEC_TIER          0=DB, 1=Core, 2=Service, 3=App, 4=UI
#   SPEC_DATA_DIR      Host data directory (for ownership fix)
#   SPEC_DATA_UID      Host UID:GID for data dir (rootless mapped)
#   SPEC_DEPENDS       Space-separated container dependencies
#   SPEC_LOCAL_IMAGE   true if image is built locally (don't pull)
#   SPEC_OPTIONAL      true if container should be skipped when image missing

reset_spec() {
  SPEC_NAME="" SPEC_IMAGE="" SPEC_NETWORK="bridge" SPEC_PORTS=""
  SPEC_VOLUMES="" SPEC_MEMORY="512m" SPEC_CAPS="CHOWN FOWNER SETUID SETGID DAC_OVERRIDE"
  SPEC_SECURITY="no-new-privileges:true" SPEC_RESTART="unless-stopped"
  SPEC_HEALTH_CMD="" SPEC_ENV="" SPEC_CUSTOM_ARGS="" SPEC_READONLY="false"
  SPEC_TMPFS="" SPEC_TIER="3" SPEC_DATA_DIR="" SPEC_DATA_UID="100000:100000"
  # Set by a loader (e.g. fedimint-gateway) to signal "spec is valid but this
  # container must not be created/recreated right now" — e.g. a required
  # per-install secret hasn't been generated yet. Empty means no skip.
  SPEC_SKIP_REASON=""
  # SPEC_OPTIONAL defaults true: reconcile-containers.sh only REPAIRS existing
  # containers — it never creates missing ones. Baseline (filebrowser) is
  # bootstrapped by first-boot-containers.sh; all other apps come from the
  # install RPC. Per-spec `SPEC_OPTIONAL="true"` lines below are now redundant
  # but kept for readability.
  SPEC_DEPENDS="" SPEC_LOCAL_IMAGE="false" SPEC_OPTIONAL="true"
  SPEC_ENTRYPOINT=""
}

if ! declare -F alloc_port >/dev/null 2>&1; then
  alloc_port() { printf '%s' "$2"; }
fi

# ── Tier 0: Databases ────────────────────────────────────────────────

load_spec_archy-mempool-db() {
  reset_spec
  SPEC_NAME="archy-mempool-db"
  SPEC_IMAGE="${MARIADB_IMAGE}"
  SPEC_NETWORK="archy-net"
  SPEC_MEMORY="$(mem_limit archy-mempool-db)"
  SPEC_VOLUMES="/var/lib/archipelago/mysql-mempool:/var/lib/mysql"
  SPEC_HEALTH_CMD="mariadb -uroot -e 'SELECT 1' || exit 1"
  SPEC_ENV="MYSQL_DATABASE=mempool MYSQL_USER=mempool MYSQL_PASSWORD=$MEMPOOL_DB_PASS MYSQL_ROOT_PASSWORD=$MYSQL_ROOT_PASS"
  SPEC_TIER="0"
  SPEC_DATA_DIR="/var/lib/archipelago/mysql-mempool"
  SPEC_DATA_UID="100999:100999"
  SPEC_CAPS="CHOWN FOWNER SETUID SETGID DAC_OVERRIDE"
}

load_spec_archy-btcpay-db() {
  reset_spec
  SPEC_NAME="archy-btcpay-db"
  SPEC_IMAGE="${BTCPAY_POSTGRES_IMAGE}"
  SPEC_NETWORK="archy-net"
  SPEC_MEMORY="$(mem_limit archy-btcpay-db)"
  SPEC_VOLUMES="/var/lib/archipelago/postgres-btcpay:/var/lib/postgresql/data"
  SPEC_HEALTH_CMD="pg_isready -U postgres || exit 1"
  SPEC_ENV="POSTGRES_DB=btcpay POSTGRES_USER=btcpay POSTGRES_PASSWORD=$BTCPAY_DB_PASS"
  SPEC_TIER="0"
  SPEC_DATA_DIR="/var/lib/archipelago/postgres-btcpay"
  SPEC_DATA_UID="100070:100070"
  SPEC_CAPS="CHOWN FOWNER SETUID SETGID DAC_OVERRIDE"
}

load_spec_immich_postgres() {
  reset_spec
  SPEC_NAME="immich_postgres"
  SPEC_IMAGE="${IMMICH_POSTGRES_IMAGE}"
  SPEC_NETWORK="bridge"
  SPEC_MEMORY="$(mem_limit immich_postgres)"
  SPEC_VOLUMES="/var/lib/archipelago/immich-db:/var/lib/postgresql/data"
  SPEC_ENV="POSTGRES_USER=postgres POSTGRES_DB=immich POSTGRES_PASSWORD=$BTCPAY_DB_PASS"
  SPEC_TIER="0"
  SPEC_DATA_DIR="/var/lib/archipelago/immich-db"
  SPEC_DATA_UID="100070:100070"
  SPEC_CAPS="CHOWN FOWNER SETUID SETGID DAC_OVERRIDE"
  SPEC_OPTIONAL="true"
}

load_spec_immich_redis() {
  reset_spec
  SPEC_NAME="immich_redis"
  SPEC_IMAGE="${VALKEY_IMAGE}"
  SPEC_NETWORK="bridge"
  SPEC_MEMORY="$(mem_limit immich_redis)"
  SPEC_TIER="0"
  SPEC_CAPS="CHOWN SETUID SETGID"
  SPEC_OPTIONAL="true"
}

# ── Tier 1: Core Infrastructure ──────────────────────────────────────

load_spec_bitcoin-knots() {
  reset_spec
  SPEC_NAME="bitcoin-knots"
  SPEC_IMAGE="${BITCOIN_KNOTS_IMAGE}"
  SPEC_NETWORK="archy-net"
  SPEC_PORTS="8332:8332 8333:8333"
  SPEC_VOLUMES="/var/lib/archipelago/bitcoin:/home/bitcoin/.bitcoin"
  SPEC_MEMORY="$(mem_limit bitcoin-knots)"
  SPEC_HEALTH_CMD="bitcoin-cli -rpcuser=\$BITCOIN_RPC_USER -rpcpassword=\$BITCOIN_RPC_PASS getblockchaininfo || exit 1"
  SPEC_TIER="1"
  SPEC_DATA_DIR="/var/lib/archipelago/bitcoin"
  SPEC_DATA_UID="100101:100101"
  local btc_rpc_headroom="-rpcthreads=16 -rpcworkqueue=256"
  local btc_txrelay_flags="-rpcwhitelistdefault=0"
  if [ -f "$SECRETS_DIR/bitcoin-rpc-txrelay-rpcauth" ]; then
    btc_txrelay_flags="$btc_txrelay_flags -rpcauth=$(cat "$SECRETS_DIR/bitcoin-rpc-txrelay-rpcauth") -rpcwhitelist=txrelay:sendrawtransaction,submitpackage,testmempoolaccept,getmempoolinfo,getrawmempool,getmempoolentry,getnetworkinfo,getblockchaininfo,getblockcount,getblockhash,getblock,getblockheader,getrawtransaction,gettxout,gettxspendingprevout,decoderawtransaction,decodescript,estimatesmartfee,uptime,ping,getconnectioncount,getpeerinfo,getindexinfo,getdeploymentinfo,getchaintips"
  fi
  # Dynamic: prune on small disk
  if [ "${DISK_GB:-0}" -lt 1000 ]; then
    SPEC_CUSTOM_ARGS="-server=1 -prune=550 -rpcallowip=0.0.0.0/0 -rpcbind=0.0.0.0:8332 -listen=1 -bind=0.0.0.0:8333 -dbcache=${BTC_DBCACHE} -par=0 -maxconnections=125 ${btc_rpc_headroom} ${btc_txrelay_flags}"
  else
    SPEC_CUSTOM_ARGS="-server=1 -txindex=1 -rpcallowip=0.0.0.0/0 -rpcbind=0.0.0.0:8332 -listen=1 -bind=0.0.0.0:8333 -dbcache=${BTC_DBCACHE} -par=0 -maxconnections=125 ${btc_rpc_headroom} ${btc_txrelay_flags}"
  fi
}

load_spec_electrumx() {
  reset_spec
  SPEC_NAME="electrumx"
  SPEC_IMAGE="${ELECTRUMX_IMAGE}"
  SPEC_NETWORK="archy-net"
  SPEC_PORTS="50001:50001"
  SPEC_VOLUMES="/var/lib/archipelago/electrumx:/data"
  SPEC_MEMORY="$(mem_limit electrumx)"
  SPEC_HEALTH_CMD="python3 -c 'import socket; socket.create_connection((\\\"localhost\\\",8000),2).close()' || exit 1"
  SPEC_ENV="DAEMON_URL=http://$BITCOIN_RPC_USER:$BITCOIN_RPC_PASS@bitcoin-knots:8332/ COIN=Bitcoin DB_DIRECTORY=/data SERVICES=tcp://:50001,rpc://0.0.0.0:8000"
  SPEC_TIER="1"
  SPEC_DATA_DIR="/var/lib/archipelago/electrumx"
  SPEC_DEPENDS="bitcoin-knots"
  SPEC_CAPS="DAC_OVERRIDE"
}

# ── Tier 2: Services ─────────────────────────────────────────────────

load_spec_lnd() {
  reset_spec
  SPEC_NAME="lnd"
  SPEC_IMAGE="${LND_IMAGE}"
  SPEC_NETWORK="archy-net"
  SPEC_PORTS="9735:9735 10009:10009 18080:8080"
  SPEC_VOLUMES="/var/lib/archipelago/lnd:/root/.lnd"
  SPEC_MEMORY="$(mem_limit lnd)"
  SPEC_CAPS="CHOWN FOWNER SETUID SETGID DAC_OVERRIDE NET_RAW"
  SPEC_HEALTH_CMD="lncli --tlscertpath /root/.lnd/tls.cert --macaroonpath /root/.lnd/data/chain/bitcoin/mainnet/readonly.macaroon --rpcserver localhost:10009 getinfo > /dev/null 2>&1 || exit 1"
  SPEC_TIER="2"
  SPEC_DATA_DIR="/var/lib/archipelago/lnd"
  SPEC_DEPENDS="bitcoin-knots"
}

load_spec_mempool-api() {
  reset_spec
  SPEC_NAME="mempool-api"
  SPEC_IMAGE="${MEMPOOL_BACKEND_IMAGE}"
  SPEC_NETWORK="archy-net"
  SPEC_PORTS="8999:8999"
  SPEC_VOLUMES="/var/lib/archipelago/mempool:/data"
  SPEC_MEMORY="$(mem_limit mempool-api)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:8999/ || exit 1"
  local MYSQL_CNT="archy-mempool-db"
  SPEC_ENV="MEMPOOL_BACKEND=electrum ELECTRUM_HOST=electrumx ELECTRUM_PORT=50001 ELECTRUM_TLS_ENABLED=false CORE_RPC_HOST=bitcoin-knots CORE_RPC_PORT=8332 CORE_RPC_USERNAME=$BITCOIN_RPC_USER CORE_RPC_PASSWORD=$BITCOIN_RPC_PASS DATABASE_ENABLED=true DATABASE_HOST=$MYSQL_CNT DATABASE_DATABASE=mempool DATABASE_USERNAME=mempool DATABASE_PASSWORD=$MEMPOOL_DB_PASS"
  SPEC_TIER="2"
  SPEC_DATA_DIR="/var/lib/archipelago/mempool"
  SPEC_DEPENDS="bitcoin-knots electrumx archy-mempool-db"
  SPEC_CAPS=""
}

load_spec_archy-mempool-web() {
  reset_spec
  SPEC_NAME="archy-mempool-web"
  SPEC_IMAGE="${MEMPOOL_WEB_IMAGE}"
  SPEC_NETWORK="archy-net"
  SPEC_PORTS="4080:8080"
  SPEC_MEMORY="$(mem_limit archy-mempool-web)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:8080/ || exit 1"
  SPEC_ENV="FRONTEND_HTTP_PORT=8080 BACKEND_MAINNET_HTTP_HOST=mempool-api"
  SPEC_TIER="2"
  SPEC_DEPENDS="mempool-api"
  SPEC_CAPS=""
}

load_spec_archy-nbxplorer() {
  reset_spec
  SPEC_NAME="archy-nbxplorer"
  SPEC_IMAGE="${NBXPLORER_IMAGE}"
  SPEC_NETWORK="archy-net"
  SPEC_PORTS="32838:32838"
  SPEC_VOLUMES="/var/lib/archipelago/nbxplorer:/data"
  SPEC_MEMORY="$(mem_limit archy-nbxplorer)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:32838/ || exit 1"
  SPEC_ENV="NBXPLORER_DATADIR=/data NBXPLORER_NETWORK=mainnet NBXPLORER_CHAINS=btc NBXPLORER_BIND=0.0.0.0:32838 NBXPLORER_BTCRPCURL=http://bitcoin-knots:8332 NBXPLORER_BTCRPCUSER=$BITCOIN_RPC_USER NBXPLORER_BTCRPCPASSWORD=$BITCOIN_RPC_PASS NBXPLORER_POSTGRES=Username=btcpay;Password=$BTCPAY_DB_PASS;Host=archy-btcpay-db;Port=5432;Database=nbxplorer"
  SPEC_TIER="2"
  SPEC_DATA_DIR="/var/lib/archipelago/nbxplorer"
  SPEC_DEPENDS="bitcoin-knots archy-btcpay-db"
  SPEC_CAPS=""
}

load_spec_btcpay-server() {
  reset_spec
  SPEC_NAME="btcpay-server"
  SPEC_IMAGE="${BTCPAY_IMAGE}"
  SPEC_NETWORK="archy-net"
  SPEC_PORTS="23000:49392"
  SPEC_VOLUMES="/var/lib/archipelago/btcpay:/datadir"
  SPEC_MEMORY="$(mem_limit btcpay-server)"
  SPEC_HEALTH_CMD="bash -ec '</dev/tcp/127.0.0.1/49392'"
  SPEC_ENV="ASPNETCORE_URLS=http://0.0.0.0:49392 BTCPAY_PROTOCOL=http BTCPAY_HOST=$HOST_IP:23000 BTCPAY_CHAINS=btc BTCPAY_BTCEXPLORERURL=http://archy-nbxplorer:32838 BTCPAY_BTCRPCURL=http://bitcoin-knots:8332 BTCPAY_BTCRPCUSER=$BITCOIN_RPC_USER BTCPAY_BTCRPCPASSWORD=$BITCOIN_RPC_PASS BTCPAY_POSTGRES=Username=btcpay;Password=$BTCPAY_DB_PASS;Host=archy-btcpay-db;Port=5432;Database=btcpay"
  SPEC_TIER="2"
  SPEC_DATA_DIR="/var/lib/archipelago/btcpay"
  SPEC_DEPENDS="archy-nbxplorer archy-btcpay-db"
}

load_spec_fedimint() {
  reset_spec
  SPEC_NAME="fedimint"
  SPEC_IMAGE="${FEDIMINT_IMAGE}"
  SPEC_NETWORK="archy-net"
  SPEC_PORTS="8173:8173 8174:8174 8175:8175"
  SPEC_VOLUMES="/var/lib/archipelago/fedimint:/data"
  SPEC_MEMORY="$(mem_limit fedimint)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:8175/ || exit 1"
  SPEC_ENV="FM_DATA_DIR=/data FM_BITCOIND_USERNAME=$BITCOIN_RPC_USER FM_BITCOIND_PASSWORD=$BITCOIN_RPC_PASS FM_BITCOIN_NETWORK=bitcoin FM_BIND_P2P=0.0.0.0:8173 FM_BIND_API=0.0.0.0:8174 FM_BIND_UI=0.0.0.0:8175 FM_P2P_URL=fedimint://$HOST_MDNS:8173 FM_API_URL=ws://$HOST_MDNS:8174 FM_BITCOIND_URL=http://bitcoin-knots:8332"
  SPEC_TIER="2"
  SPEC_DATA_DIR="/var/lib/archipelago/fedimint"
  SPEC_DEPENDS="bitcoin-knots"
  SPEC_OPTIONAL="true"
}

load_spec_fedimint-gateway() {
  reset_spec
  SPEC_NAME="fedimint-gateway"
  SPEC_IMAGE="${FEDIMINT_GATEWAY_IMAGE}"
  SPEC_NETWORK="archy-net"
  SPEC_PORTS="8176:8176"
  SPEC_VOLUMES="/var/lib/archipelago/fedimint-gateway:/data"
  SPEC_MEMORY="$(mem_limit fedimint-gateway)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:8176/ || exit 1"
  SPEC_TIER="2"
  SPEC_DATA_DIR="/var/lib/archipelago/fedimint-gateway"
  SPEC_DEPENDS="bitcoin-knots fedimint"
  SPEC_OPTIONAL="true"
  # FED-07: no shipped fallback credential. If the per-install bcrypt hash
  # (fedimint-gateway-hash, generated by container::secrets::ensure_gateway_credential
  # via the daemon, first-boot, or reconcile secret-generation step) doesn't
  # exist yet, do not build an entrypoint with an empty --bcrypt-password-hash
  # — skip creating/recreating this container and let the caller retry once
  # the credential exists.
  if [ -z "$FEDI_HASH" ]; then
    SPEC_SKIP_REASON="fedimint-gateway credential not generated yet (no shipped default; will retry once a per-install credential exists)"
    return
  fi
  # Custom entrypoint depends on whether LND is available
  local LND_CERT=/var/lib/archipelago/lnd/tls.cert
  local LND_MAC=/var/lib/archipelago/lnd/data/chain/bitcoin/mainnet/admin.macaroon
  if [ -f "$LND_CERT" ] && [ -f "$LND_MAC" ]; then
    SPEC_VOLUMES="$SPEC_VOLUMES $LND_CERT:/lnd/tls.cert:ro $LND_MAC:/lnd/admin.macaroon:ro"
    SPEC_ENTRYPOINT="gatewayd --data-dir /data --listen 0.0.0.0:8176 --bcrypt-password-hash $FEDI_HASH --network bitcoin --bitcoind-url http://bitcoin-knots:8332 --bitcoind-username $BITCOIN_RPC_USER --bitcoind-password $BITCOIN_RPC_PASS lnd --lnd-rpc-host lnd:10009 --lnd-tls-cert /lnd/tls.cert --lnd-macaroon /lnd/admin.macaroon"
  else
    SPEC_PORTS="8176:8176 9737:9737"
    SPEC_ENTRYPOINT="gatewayd --data-dir /data --listen 0.0.0.0:8176 --bcrypt-password-hash $FEDI_HASH --network bitcoin --bitcoind-url http://bitcoin-knots:8332 --bitcoind-username $BITCOIN_RPC_USER --bitcoind-password $BITCOIN_RPC_PASS ldk --ldk-lightning-port 9737 --ldk-alias archipelago-gateway"
  fi
}

load_spec_immich_server() {
  reset_spec
  SPEC_NAME="immich_server"
  SPEC_IMAGE="${IMMICH_SERVER_IMAGE}"
  SPEC_NETWORK="bridge"
  SPEC_PORTS="2283:2283"
  SPEC_VOLUMES="/var/lib/archipelago/immich:/usr/src/app/upload"
  SPEC_MEMORY="$(mem_limit immich_server)"
  SPEC_ENV="DB_HOSTNAME=immich_postgres DB_DATABASE_NAME=immich DB_USERNAME=postgres DB_PASSWORD=$BTCPAY_DB_PASS REDIS_HOSTNAME=immich_redis UPLOAD_LOCATION=/usr/src/app/upload"
  SPEC_TIER="2"
  SPEC_DATA_DIR="/var/lib/archipelago/immich"
  SPEC_DEPENDS="immich_postgres immich_redis"
  SPEC_CAPS=""
  SPEC_OPTIONAL="true"
}

# ── Tier 3: Applications ─────────────────────────────────────────────

load_spec_homeassistant() {
  reset_spec
  SPEC_NAME="homeassistant"
  SPEC_IMAGE="${HOMEASSISTANT_IMAGE}"
  SPEC_PORTS="8123:8123"
  SPEC_VOLUMES="/var/lib/archipelago/home-assistant:/config"
  SPEC_MEMORY="$(mem_limit homeassistant)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:8123/ || exit 1"
  SPEC_ENV="TZ=UTC"
  SPEC_TIER="3"
  SPEC_DATA_DIR="/var/lib/archipelago/home-assistant"
  SPEC_CAPS="CHOWN SETUID SETGID DAC_OVERRIDE NET_BIND_SERVICE"
  SPEC_OPTIONAL="true"
}

load_spec_grafana() {
  reset_spec
  SPEC_NAME="grafana"
  SPEC_IMAGE="${GRAFANA_IMAGE}"
  SPEC_PORTS="3000:3000"
  SPEC_VOLUMES="/var/lib/archipelago/grafana:/var/lib/grafana"
  SPEC_MEMORY="$(mem_limit grafana)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:3000/api/health || exit 1"
  SPEC_ENV="GF_PATHS_DATA=/var/lib/grafana GF_USERS_ALLOW_SIGN_UP=false"
  SPEC_READONLY="true"
  SPEC_TMPFS="/tmp:rw,noexec,nosuid,size=256m /run:rw,noexec,nosuid,size=64m"
  SPEC_TIER="3"
  SPEC_DATA_DIR="/var/lib/archipelago/grafana"
  SPEC_DATA_UID="100472:100472"
  SPEC_CAPS="CHOWN SETUID SETGID DAC_OVERRIDE NET_BIND_SERVICE"
  SPEC_OPTIONAL="true"
}

load_spec_uptime-kuma() {
  reset_spec
  SPEC_NAME="uptime-kuma"
  SPEC_IMAGE="${UPTIME_KUMA_IMAGE}"
  SPEC_PORTS="3002:3001"
  SPEC_VOLUMES="/var/lib/archipelago/uptime-kuma:/app/data"
  SPEC_MEMORY="$(mem_limit uptime-kuma)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:3001/ || exit 1"
  SPEC_ENV="TZ=UTC"
  SPEC_TIER="3"
  SPEC_DATA_DIR="/var/lib/archipelago/uptime-kuma"
  SPEC_CAPS="CHOWN FOWNER SETUID SETGID"
  SPEC_OPTIONAL="true"
}

load_spec_jellyfin() {
  reset_spec
  SPEC_NAME="jellyfin"
  SPEC_IMAGE="${JELLYFIN_IMAGE}"
  SPEC_PORTS="8096:8096"
  SPEC_VOLUMES="/var/lib/archipelago/jellyfin/config:/config /var/lib/archipelago/jellyfin/cache:/cache"
  SPEC_MEMORY="$(mem_limit jellyfin)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:8096/ || exit 1"
  SPEC_TIER="3"
  SPEC_DATA_DIR="/var/lib/archipelago/jellyfin"
  SPEC_CAPS=""
  SPEC_OPTIONAL="true"
}

load_spec_photoprism() {
  reset_spec
  SPEC_NAME="photoprism"
  SPEC_IMAGE="${PHOTOPRISM_IMAGE}"
  SPEC_PORTS="2342:2342"
  SPEC_VOLUMES="/var/lib/archipelago/photoprism:/photoprism/storage"
  SPEC_MEMORY="$(mem_limit photoprism)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:2342/ || exit 1"
  SPEC_ENV="PHOTOPRISM_ADMIN_PASSWORD=archipelago PHOTOPRISM_DEFAULT_LOCALE=en"
  SPEC_TIER="3"
  SPEC_DATA_DIR="/var/lib/archipelago/photoprism"
  SPEC_CAPS="CHOWN SETUID SETGID"
  SPEC_OPTIONAL="true"
}

load_spec_vaultwarden() {
  reset_spec
  SPEC_NAME="vaultwarden"
  SPEC_IMAGE="${VAULTWARDEN_IMAGE}"
  SPEC_PORTS="8082:80"
  SPEC_VOLUMES="/var/lib/archipelago/vaultwarden:/data"
  SPEC_MEMORY="$(mem_limit vaultwarden)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:80/ || exit 1"
  SPEC_TIER="3"
  SPEC_DATA_DIR="/var/lib/archipelago/vaultwarden"
  SPEC_CAPS="CHOWN SETUID SETGID NET_BIND_SERVICE"
  SPEC_OPTIONAL="true"
}

load_spec_nextcloud() {
  reset_spec
  SPEC_NAME="nextcloud"
  SPEC_IMAGE="${NEXTCLOUD_IMAGE}"
  SPEC_PORTS="8085:80"
  SPEC_VOLUMES="/var/lib/archipelago/nextcloud:/var/www/html"
  SPEC_MEMORY="$(mem_limit nextcloud)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:80/ || exit 1"
  SPEC_TIER="3"
  SPEC_DATA_DIR="/var/lib/archipelago/nextcloud"
  SPEC_CAPS="CHOWN SETUID SETGID DAC_OVERRIDE NET_BIND_SERVICE"
  SPEC_OPTIONAL="true"
}

load_spec_searxng() {
  reset_spec
  SPEC_NAME="searxng"
  SPEC_IMAGE="${SEARXNG_IMAGE}"
  SPEC_PORTS="8888:8080"
  SPEC_MEMORY="$(mem_limit searxng)"
  SPEC_VOLUMES="/var/lib/archipelago/searxng:/etc/searxng"
  SPEC_HEALTH_CMD="curl -sf http://localhost:8080/ || exit 1"
  SPEC_READONLY="true"
  SPEC_TMPFS="/tmp:rw,noexec,nosuid,size=256m /run:rw,noexec,nosuid,size=64m"
  SPEC_TIER="3"
  SPEC_CAPS=""
  SPEC_DATA_DIR="/var/lib/archipelago/searxng"
  SPEC_OPTIONAL="true"
}

load_spec_filebrowser() {
  reset_spec
  SPEC_NAME="filebrowser"
  SPEC_IMAGE="${FILEBROWSER_IMAGE}"
  SPEC_NETWORK="archy-net"
  SPEC_PORTS="8083:80"
  SPEC_VOLUMES="/var/lib/archipelago/filebrowser:/srv /var/lib/archipelago/filebrowser-data:/data"
  SPEC_MEMORY="$(mem_limit filebrowser)"
  SPEC_HEALTH_CMD="wget -q --spider http://localhost:80/health || exit 1"
  SPEC_TIER="3"
  SPEC_DATA_DIR="/var/lib/archipelago/filebrowser"
  SPEC_DATA_UID="100000:100000"
  # first-boot-containers.sh writes /data/.filebrowser.json (see filebrowser
  # creation block at ~line 1128). Config path is required or filebrowser
  # opens /database.db in CWD and fails with permission denied.
  SPEC_CUSTOM_ARGS="--config /data/.filebrowser.json"
  # Needs default caps (CHOWN FOWNER SETUID SETGID DAC_OVERRIDE) from reset_spec
  # for rootless userns-root to write /data/filebrowser.db, plus NET_BIND_SERVICE
  # to listen on port 80.
  SPEC_CAPS="CHOWN FOWNER SETUID SETGID DAC_OVERRIDE NET_BIND_SERVICE"
  SPEC_OPTIONAL="true"
}

load_spec_nginx-proxy-manager() {
  reset_spec
  SPEC_NAME="nginx-proxy-manager"
  SPEC_IMAGE="${NPM_IMAGE}"
  local admin_port http_port https_port
  admin_port=$(alloc_port nginx-proxy-manager 8081 81)
  http_port=$(alloc_port nginx-proxy-manager-http 8084 80)
  https_port=$(alloc_port nginx-proxy-manager-https 8444 443)
  SPEC_PORTS="$admin_port:81 $http_port:80 $https_port:443"
  SPEC_VOLUMES="/var/lib/archipelago/nginx-proxy-manager/data:/data /var/lib/archipelago/nginx-proxy-manager/letsencrypt:/etc/letsencrypt"
  SPEC_MEMORY="$(mem_limit nginx-proxy-manager)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:81/ || exit 1"
  SPEC_TIER="3"
  SPEC_DATA_DIR="/var/lib/archipelago/nginx-proxy-manager"
  SPEC_CAPS="CHOWN FOWNER SETUID SETGID DAC_OVERRIDE NET_BIND_SERVICE"
  SPEC_OPTIONAL="true"
}

load_spec_portainer() {
  reset_spec
  SPEC_NAME="portainer"
  SPEC_IMAGE="${PORTAINER_IMAGE}"
  SPEC_PORTS="9000:9000"
  SPEC_VOLUMES="/var/lib/archipelago/portainer:/data /run/user/1000/podman/podman.sock:/var/run/docker.sock /var/lib/archipelago/portainer/compose:/data/compose"
  SPEC_MEMORY="$(mem_limit portainer)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:9000/ || exit 1"
  SPEC_TIER="3"
  SPEC_DATA_DIR="/var/lib/archipelago/portainer"
  SPEC_DATA_UID="1000:1000"
  SPEC_OPTIONAL="true"
}

load_spec_ollama() {
  reset_spec
  SPEC_NAME="ollama"
  SPEC_IMAGE="${OLLAMA_IMAGE}"
  SPEC_PORTS="11434:11434"
  SPEC_VOLUMES="/var/lib/archipelago/ollama:/root/.ollama"
  SPEC_MEMORY="$(mem_limit ollama)"
  SPEC_HEALTH_CMD="curl -sf http://localhost:11434/ || exit 1"
  SPEC_READONLY="true"
  SPEC_TMPFS="/tmp:rw,noexec,nosuid,size=256m /run:rw,noexec,nosuid,size=64m"
  SPEC_TIER="3"
  SPEC_DATA_DIR="/var/lib/archipelago/ollama"
  SPEC_CAPS=""
  SPEC_OPTIONAL="true"
}

# ── Tier 4: Frontend UIs ─────────────────────────────────────────────

load_spec_archy-bitcoin-ui() {
  reset_spec
  SPEC_NAME="archy-bitcoin-ui"
  SPEC_IMAGE="localhost/bitcoin-ui:local"
  SPEC_NETWORK="host"
  SPEC_VOLUMES="/var/lib/archipelago/bitcoin-ui/nginx.conf:/etc/nginx/conf.d/default.conf:ro"
  SPEC_MEMORY="$(mem_limit archy-bitcoin-ui)"
  SPEC_TIER="4"
  SPEC_LOCAL_IMAGE="true"
  SPEC_CAPS="CHOWN SETUID SETGID"
  SPEC_SECURITY="no-new-privileges:true"
}

load_spec_archy-lnd-ui() {
  reset_spec
  SPEC_NAME="archy-lnd-ui"
  SPEC_IMAGE="localhost/lnd-ui:local"
  # Host-networked, NOT bridge with 18083:80. docker/lnd-ui/nginx.conf listens
  # on 18083 directly (it must, so it can proxy to the backend on
  # 127.0.0.1:5678 without a cross-origin hop). This spec used to say
  # SPEC_PORTS="18083:80", which published host 18083 to container port 80 —
  # where nothing listens. Nobody noticed because the running containers were
  # created by first-boot-containers.sh, which is host-networked and never
  # consults this file; the spec is only read when self-update.sh rebuilds a
  # UI image, and that only fires when a file under docker/lnd-ui/ changes.
  # Verified on a test node: recreating from the old spec left :18083
  # refusing connections.
  SPEC_NETWORK="host"
  SPEC_MEMORY="$(mem_limit archy-lnd-ui)"
  SPEC_TIER="4"
  SPEC_LOCAL_IMAGE="true"
  SPEC_CAPS="CHOWN SETUID SETGID"
  SPEC_SECURITY="no-new-privileges:true"
}

load_spec_archy-electrs-ui() {
  reset_spec
  SPEC_NAME="archy-electrs-ui"
  SPEC_IMAGE="localhost/electrs-ui:local"
  SPEC_NETWORK="host"
  SPEC_MEMORY="$(mem_limit archy-electrs-ui)"
  SPEC_TIER="4"
  SPEC_LOCAL_IMAGE="true"
  SPEC_CAPS="CHOWN SETUID SETGID"
  SPEC_SECURITY="no-new-privileges:true"
}

# ── Registry ─────────────────────────────────────────────────────────
# Ordered by tier, then dependency order within tier

ALL_CONTAINER_SPECS=(
  # Tier 0: Databases
  archy-mempool-db
  archy-btcpay-db
  immich_postgres
  immich_redis
  # Tier 1: Core
  bitcoin-knots
  electrumx
  # Tier 2: Services
  lnd
  mempool-api
  archy-mempool-web
  archy-nbxplorer
  btcpay-server
  fedimint
  fedimint-gateway
  immich_server
  # Tier 3: Apps
  homeassistant
  grafana
  uptime-kuma
  jellyfin
  photoprism
  vaultwarden
  nextcloud
  searxng
  filebrowser
  nginx-proxy-manager
  portainer
  ollama
  # Tier 4: UIs
  archy-bitcoin-ui
  archy-lnd-ui
  archy-electrs-ui
)

# Load a spec by name. Usage: load_spec "bitcoin-knots"
load_spec() {
  local fn="load_spec_${1}"
  if declare -f "$fn" >/dev/null 2>&1; then
    "$fn"
    return 0
  fi
  return 1
}

# Return all spec names
all_specs() {
  echo "${ALL_CONTAINER_SPECS[@]}"
}
