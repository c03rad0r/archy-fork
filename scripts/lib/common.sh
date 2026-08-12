#!/bin/bash
# Shared utility functions for Archipelago scripts
#
# Source this from any script:
#   source "$(dirname "$0")/lib/common.sh"
#
# Provides: logging, SSH helpers, health checks, disk checks, memory limits

# Guard against double-sourcing
[ -n "${_ARCHY_COMMON_LOADED:-}" ] && return 0
_ARCHY_COMMON_LOADED=1

# ── Colored logging ─────────────────────────────────────────────────────

log_info()  { echo -e "\033[0;32m[INFO]\033[0m  $(date '+%H:%M:%S') $*"; }
log_warn()  { echo -e "\033[0;33m[WARN]\033[0m  $(date '+%H:%M:%S') $*"; }
log_error() { echo -e "\033[0;31m[ERROR]\033[0m $(date '+%H:%M:%S') $*"; }

# ── SSH wrapper with deploy key ─────────────────────────────────────────

# Usage: ssh_cmd <host> <command...>
# Uses the standard deploy key and safe defaults.
ssh_cmd() {
  local host="$1"; shift
  local key="${ARCHIPELAGO_SSH_KEY:-$HOME/.ssh/archipelago-deploy}"
  ssh -i "$key" \
      -o StrictHostKeyChecking=no \
      -o ConnectTimeout=10 \
      -o ServerAliveInterval=15 \
      -o ServerAliveCountMax=4 \
      "archipelago@${host}" "$@"
}

# Usage: scp_cmd <src> <dest>
# Wraps scp with the same deploy key and options.
scp_cmd() {
  local key="${ARCHIPELAGO_SSH_KEY:-$HOME/.ssh/archipelago-deploy}"
  scp -i "$key" \
      -o StrictHostKeyChecking=no \
      -o ConnectTimeout=10 \
      "$@"
}

# ── Health check ────────────────────────────────────────────────────────

# Wait for an HTTP health endpoint to respond successfully.
# Usage: wait_for_health <host> [max_wait_seconds] [path]
wait_for_health() {
  local host="$1" max_wait="${2:-60}" path="${3:-/health}"
  local waited=0
  while [ $waited -lt $max_wait ]; do
    if curl -sf "http://${host}${path}" >/dev/null 2>&1; then
      log_info "Health check passed for ${host}"
      return 0
    fi
    sleep 2
    waited=$((waited + 2))
  done
  log_error "Health check failed for ${host} after ${max_wait}s"
  return 1
}

# ── Disk space check ───────────────────────────────────────────────────

# Check that disk usage on a remote host is below a threshold.
# Usage: check_disk_space <host> [max_percent]
check_disk_space() {
  local host="$1" max_pct="${2:-85}"
  local pct
  pct=$(ssh_cmd "$host" "df / | tail -1 | awk '{print \$(NF-1)}' | tr -d '%'" 2>/dev/null)
  if [ -n "$pct" ] && [ "$pct" -gt "$max_pct" ] 2>/dev/null; then
    log_error "Disk at ${pct}% on ${host} (max ${max_pct}%)"
    return 1
  fi
  return 0
}

# ── Memory limit calculator ────────────────────────────────────────────

# Returns the memory limit for a container by name.
# Checks /etc/archipelago/memory-limits.conf first (override), then falls
# back to built-in defaults. Mirrors the pattern in first-boot-containers.sh.
#
# Low-memory mode: set LOW_MEM=true before calling to get reduced limits
# on certain heavy containers.
#
# Usage: mem_limit <container-name>
mem_limit() {
  local name="$1"

  # Allow per-host overrides via config file
  local limit
  limit=$(grep "^${name}=" /etc/archipelago/memory-limits.conf 2>/dev/null | cut -d= -f2)
  if [ -n "$limit" ]; then
    echo "$limit"
    return
  fi

  # Built-in defaults (keep in sync with first-boot-containers.sh and Rust package config)
  local low="${LOW_MEM:-false}"
  case "$name" in
    bitcoin|bitcoin-core|bitcoin-knots) $low && echo "4g"   || echo "8g" ;;
    ollama)              $low && echo "1g"   || echo "4g" ;;
    lnd)                 echo "512m" ;;
    electrumx|mempool-electrs|electrs) echo "4g" ;;
    nextcloud)           echo "1g" ;;
    immich_server)       echo "1g" ;;
    btcpay-server|btcpayserver) echo "1g" ;;
    homeassistant)       echo "512m" ;;
    fedimint)            echo "512m" ;;
    fedimint-gateway)    echo "512m" ;;
    photoprism)          $low && echo "512m" || echo "1g" ;;
    mempool-api)         echo "512m" ;;
    jellyfin)            echo "1g" ;;
    searxng)             echo "512m" ;;
    archy-btcpay-db)     echo "512m" ;;
    archy-nbxplorer)     echo "512m" ;;
    archy-mempool-db)    echo "512m" ;;
    archy-mempool-web)   echo "256m" ;;
    grafana)             echo "256m" ;;
    vaultwarden)         echo "256m" ;;
    uptime-kuma)         echo "256m" ;;
    filebrowser)         echo "256m" ;;
    portainer)           echo "256m" ;;
    nginx-proxy-manager) echo "256m" ;;
    immich_postgres)     echo "256m" ;;
    immich_redis)        echo "128m" ;;
    tailscale)           echo "256m" ;;
    penpot-postgres)     echo "256m" ;;
    penpot-valkey)       echo "128m" ;;
    penpot-backend)      echo "512m" ;;
    penpot-exporter)     echo "256m" ;;
    penpot-frontend)     echo "256m" ;;
    nostr-rs-relay)      echo "256m" ;;
    strfry)              echo "256m" ;;
    indeedhub|archy-bitcoin-ui|archy-lnd-ui|archy-electrs-ui) echo "128m" ;;
    *)                   echo "512m" ;;
  esac
}

# ── Wait for container readiness ───────────────────────────────────────

# Wait for a container health check command to succeed.
# Usage: wait_for_container <name> <check_cmd> [max_wait_seconds]
wait_for_container() {
  local name="$1" check_cmd="$2" max_wait="${3:-30}"
  local waited=0
  while [ $waited -lt $max_wait ]; do
    if eval "$check_cmd" 2>/dev/null; then
      log_info "$name is ready (${waited}s)"
      return 0
    fi
    sleep 2
    waited=$((waited + 2))
  done
  log_warn "$name not ready after ${max_wait}s"
  return 1
}

# ── Same-host deploy safety guard (2026-07-31 incident, widened 13-09) ──

# Refuse a same-host deploy whose resolved source and destination differ.
#
# The 2026-07-31 incident: a same-host `rsync --delete` deploy whose source
# was INSIDE the destination (a worktree under the main checkout) mirrored
# the worktree onto the main checkout and deleted ~1810 tracked files, a
# running dev server, and two sessions' uncommitted work. The original fix
# refused only that containment shape (source-in-destination or
# destination-in-source). It missed SIBLING directories that share a parent
# but neither contains the other — e.g. archy-phase13 (this worktree) as
# source and archy (the main checkout, TARGET_DIR's resolved symlink
# target) as destination — which is the identical rsync --delete hazard
# through a shape the old two-case guard let through.
#
# This function takes two ALREADY-RESOLVED (`readlink -f`) absolute paths
# and makes no SSH calls itself — same-host detection stays in the caller
# (deploy-to-target.sh already does it via /etc/machine-id). It returns 0
# only when the two paths are equal; every other case is refused, not just
# the two containment shapes. Any resolved-path mismatch on the same host
# is the same rsync --delete hazard regardless of shape, so "refuse unless
# equal" is both the widening and a simplification.
#
# Usage: assert_safe_same_host_deploy <local_src> <remote_dst>
assert_safe_same_host_deploy() {
  local local_src="$1" remote_dst="$2"

  if [ -z "$local_src" ] || [ -z "$remote_dst" ]; then
    log_error "assert_safe_same_host_deploy: both paths are required (local_src='$local_src' remote_dst='$remote_dst')"
    return 1
  fi

  if [ "$local_src" = "$remote_dst" ]; then
    return 0
  fi

  echo "FATAL: refusing to deploy. Source '$local_src' and destination '$remote_dst' are on the same host but resolve to DIFFERENT paths." >&2
  echo "       'rsync --delete' would mirror one onto the other and delete whatever the source lacks — the 2026-07-31 data-loss incident (and the sibling-directory shape its original fix missed)." >&2
  echo "       Run this script from the deploy destination itself ('$remote_dst'), not from a worktree, sibling checkout, or copy." >&2
  return 1
}

# ── Section timing ─────────────────────────────────────────────────────

# Track elapsed time for deploy sections.
# Usage:
#   section_start "Building frontend"
#   ... do work ...
#   section_end
_SECTION_START=0
_SECTION_NAME=""

section_start() {
  _SECTION_NAME="${1:-}"
  _SECTION_START=$(date +%s)
  [ -n "$_SECTION_NAME" ] && log_info "$_SECTION_NAME"
}

section_end() {
  local elapsed=$(( $(date +%s) - _SECTION_START ))
  if [ -n "$_SECTION_NAME" ]; then
    log_info "$_SECTION_NAME done (${elapsed}s)"
  else
    echo "   (${elapsed}s)"
  fi
}
