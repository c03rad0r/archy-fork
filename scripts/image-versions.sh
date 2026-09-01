#!/bin/bash
# Container image versions — single source of truth
# Source this file from all scripts that create containers
#
# Usage: source /opt/archipelago/image-versions.sh 2>/dev/null || true
#        source "$(dirname "$0")/image-versions.sh" 2>/dev/null || true
#
# Tags MUST match what's actually in the registry at source.archipelago-foundation.org/lfg2025/
# Run: podman images --format '{{.Repository}}:{{.Tag}}' | grep 'source.archipelago-foundation.org' | sort
# to verify against the registry.

# Archipelago app registries (primary + fallback)
#
# Honour a caller-supplied ARCHY_REGISTRY instead of overwriting it. Catalog
# generation MUST be able to target a host inside releases/registry-trust-floor.json
# — the hosts binaries already deployed to the fleet are known to trust. This
# file's default is the public domain, which the fleet does NOT trust yet: the
# migration is ship-binary -> confirm-fleet -> promote-in-floor -> regenerate,
# and only step 0 has happened. Generating the catalog against an untrusted host
# makes every install in the field fail with "not from a trusted registry"; the
# signer refuses to sign such a catalog, which is how this was caught.
ARCHY_REGISTRY="${ARCHY_REGISTRY:-source.archipelago-foundation.org/lfg2025}"
# No fallback registry: the old tx1138 registry host was retired (2026-06-13); empty disables the fallback path.
ARCHY_REGISTRY_FALLBACK=""

# Bitcoin stack
# Pinned, not :latest — see apps/bitcoin-knots/manifest.yml. Knots 20260508
# halts pending the BIP110/RDTS consensus decision, so a moving tag can freeze
# the fleet's chain sync. Bumping this is a consensus decision.
BITCOIN_KNOTS_IMAGE="$ARCHY_REGISTRY/bitcoin-knots:29.3.knots20260210"
LND_IMAGE="$ARCHY_REGISTRY/lnd:v0.18.4-beta"
ELECTRUMX_IMAGE="$ARCHY_REGISTRY/electrumx:v1.18.0"

# Mempool stack
MEMPOOL_BACKEND_IMAGE="$ARCHY_REGISTRY/mempool-backend:v3.3.1"
MEMPOOL_WEB_IMAGE="$ARCHY_REGISTRY/mempool-frontend:v3.3.1"
MARIADB_IMAGE="$ARCHY_REGISTRY/mariadb:11.4.10"

# BTCPay
BTCPAY_IMAGE="docker.io/btcpayserver/btcpayserver:2.4.3"
NBXPLORER_IMAGE="$ARCHY_REGISTRY/nbxplorer:2.6.0"
POSTGRES_IMAGE="$ARCHY_REGISTRY/postgres:15.17"
BTCPAY_POSTGRES_IMAGE="$ARCHY_REGISTRY/postgres:15.17"

# Apps
HOMEASSISTANT_IMAGE="$ARCHY_REGISTRY/home-assistant:2026.8.2"
GRAFANA_IMAGE="$ARCHY_REGISTRY/grafana:10.2.0"
UPTIME_KUMA_IMAGE="$ARCHY_REGISTRY/uptime-kuma:1"
JELLYFIN_IMAGE="$ARCHY_REGISTRY/jellyfin:10.11.11"
PHOTOPRISM_IMAGE="$ARCHY_REGISTRY/photoprism:240915"
OLLAMA_IMAGE="$ARCHY_REGISTRY/ollama:latest"
VAULTWARDEN_IMAGE="$ARCHY_REGISTRY/vaultwarden:1.37.1-alpine"
NEXTCLOUD_IMAGE="$ARCHY_REGISTRY/nextcloud:29"
SEARXNG_IMAGE="$ARCHY_REGISTRY/searxng:latest"
# OnlyOffice removed — incompatible with rootless Podman (internal postgres/rabbitmq fail)
# Replaced by CryptPad (single Node.js process, e2e encrypted)
FILEBROWSER_IMAGE="$ARCHY_REGISTRY/filebrowser:v2.27.0"
NPM_IMAGE="$ARCHY_REGISTRY/nginx-proxy-manager:latest"
# 2.39.1 is what the fleet has actually been running via the moving :latest
# tag, and it is the version that wrote their databases. Pinning back to
# 2.19.4 (2 years older) made Portainer refuse to start the moment a
# container was recreated: "database schema version does not align with the
# server version" — it migrates a DB forward, never backward. Pinned
# forward and published as a concrete tag so this is reproducible.
PORTAINER_IMAGE="$ARCHY_REGISTRY/portainer:2.39.6"

# Networking
TAILSCALE_IMAGE="$ARCHY_REGISTRY/tailscale:stable"
NETBIRD_DASHBOARD_IMAGE="docker.io/netbirdio/dashboard:v2.38.0"
NETBIRD_SERVER_IMAGE="docker.io/netbirdio/netbird-server:0.71.2"
NETBIRD_PROXY_IMAGE="docker.io/library/nginx:1.27-alpine"
ALPINE_TOR_IMAGE="$ARCHY_REGISTRY/alpine-tor:0.4.8.13"
ADGUARDHOME_IMAGE="$ARCHY_REGISTRY/adguardhome:v0.107.55"

# Fedimint
FEDIMINT_IMAGE="$ARCHY_REGISTRY/fedimintd:v0.10.1"
FEDIMINT_GATEWAY_IMAGE="$ARCHY_REGISTRY/gatewayd:v0.10.1"
# fmcd = Fedimint client daemon (iroh-capable, fedimint-client 0.8.2). Built
# from minmoto/fmcd. Bundled on the ISO in BOTH modes (full CONTAINER_IMAGES
# list and the unbundled core bundle) and auto-created by first-boot as a
# baseline app so ecash works offline out of the box.
# See docs/dual-ecash-design.md.
FMCD_IMAGE="$ARCHY_REGISTRY/fmcd:0.8.1"

# Ark (bark)
# barkd = Ark wallet daemon, packaged from the pinned upstream release binary
# (apps/barkd/Dockerfile). Signet-only default config; keep the tag in
# lockstep with core/archipelago/src/wallet/ark_client.rs REST shapes. Not in
# the bundled CONTAINER_IMAGES list — install via the barkd app manifest.
BARKD_IMAGE="$ARCHY_REGISTRY/barkd:0.3.0"

# Media
REDIS_IMAGE="$ARCHY_REGISTRY/redis:7.4.8"

# Valkey (general purpose)
VALKEY_IMAGE="$ARCHY_REGISTRY/valkey:8.1.6"

# Nostr
NOSTR_RS_RELAY_IMAGE="$ARCHY_REGISTRY/nostr-rs-relay:0.10.0"
STRFRY_IMAGE="$ARCHY_REGISTRY/strfry:1.0.4"
FIPS_UI_IMAGE="$ARCHY_REGISTRY/fips-ui:1.7.123-alpha"

# AI / Routing

# Community / Gaming
BOTFIGHTS_IMAGE="$ARCHY_REGISTRY/botfights:1.2.11"

# IndeedHub stack
INDEEDHUB_IMAGE="$ARCHY_REGISTRY/indeedhub:1.0.0"
INDEEDHUB_API_IMAGE="$ARCHY_REGISTRY/indeedhub-api:1.0.0"
INDEEDHUB_FFMPEG_IMAGE="$ARCHY_REGISTRY/indeedhub-ffmpeg:1.0.0"
MINIO_IMAGE="$ARCHY_REGISTRY/minio:RELEASE.2024-11-07T00-52-20Z"
INDEEDHUB_POSTGRES_IMAGE="$ARCHY_REGISTRY/postgres:16.13-alpine"
INDEEDHUB_REDIS_IMAGE="$ARCHY_REGISTRY/redis:7.4.8-alpine"

# Gitea (Git + Container Registry)
GITEA_IMAGE="docker.io/gitea/gitea:1.23"

# DWN (Decentralized Web Node)

# Immich stack
IMMICH_POSTGRES_IMAGE="$ARCHY_REGISTRY/immich-postgres:14-vectorchord0.4.3-pgvectors0.2.0"
IMMICH_SERVER_IMAGE="$ARCHY_REGISTRY/immich-server:release"


# Custom UI containers (built from docker/ dirs, pushed to registry)
BITCOIN_UI_IMAGE="$ARCHY_REGISTRY/bitcoin-ui:1.7.123-alpha"
LND_UI_IMAGE="$ARCHY_REGISTRY/lnd-ui:1.7.123-alpha"
ELECTRS_UI_IMAGE="$ARCHY_REGISTRY/electrs-ui:1.7.123-alpha"

# Base images
NGINX_ALPINE_IMAGE="$ARCHY_REGISTRY/nginx:1.27.4-alpine"
