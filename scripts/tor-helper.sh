#!/bin/bash
# tor-helper.sh — Privileged Tor operations for the Archipelago backend.
# Runs as root via systemd (archipelago-tor-helper.service), triggered by
# a path unit watching /var/lib/archipelago/tor-config/tor-action.
#
# The backend writes a JSON action file, the path unit triggers this script.
# This avoids calling sudo from within a NoNewPrivileges=yes service.
set -euo pipefail

ACTION_FILE="/var/lib/archipelago/tor-config/tor-action"
TORRC_STAGED="/var/lib/archipelago/tor-config/torrc.staged"
RESULT_FILE="/var/lib/archipelago/tor-config/tor-result"
HOSTNAMES_DIR="/var/lib/archipelago/tor-hostnames"

log() { echo "[tor-helper] $*"; }

write_result() {
  echo "$1" > "$RESULT_FILE"
  chown archipelago:archipelago "$RESULT_FILE" 2>/dev/null || true
}

sync_hostnames() {
  mkdir -p "$HOSTNAMES_DIR"
  # Clear stale copies first
  rm -f "$HOSTNAMES_DIR"/* 2>/dev/null || true
  # Prefer /var/lib/tor (system Tor, authoritative) over /var/lib/archipelago/tor
  # Only copy from secondary if not already found in primary
  for base in /var/lib/tor /var/lib/archipelago/tor; do
    for dir in "$base"/hidden_service_*; do
      [ -d "$dir" ] || continue
      svc=$(basename "$dir" | sed 's/^hidden_service_//')
      echo "$svc" | grep -q '_old_' && continue
      # Skip if already synced from a higher-priority location
      [ -f "${HOSTNAMES_DIR}/${svc}" ] && continue
      if [ -f "$dir/hostname" ]; then
        cp "$dir/hostname" "${HOSTNAMES_DIR}/${svc}"
        log "Synced hostname: $svc ($base)"
      fi
    done
  done
  chown -R archipelago:archipelago "$HOSTNAMES_DIR" 2>/dev/null || true
}

# ─── Main ─────────────────────────────────────────────────────────

if [ ! -f "$ACTION_FILE" ]; then
  log "No action file found"
  exit 0
fi

ACTION=$(cat "$ACTION_FILE")
rm -f "$ACTION_FILE"

ACTION_TYPE=$(echo "$ACTION" | python3 -c "import sys,json; print(json.load(sys.stdin).get('action',''))" 2>/dev/null || echo "")

# Restart the daemon that actually serves, and report whether it really came up.
#
# Why the dashboard's "Restart Tor" button did nothing on 2026-08-09, in order
# of importance — note it was NOT wrong-unit targeting: `systemctl restart tor`
# does propagate to tor@default (measured on austin-sapien, MainPID changed).
#
#   1. RESET-FAILED WAS MISSING. Once tor@default has failed enough times
#      systemd latches "Start request repeated too quickly" and refuses to start
#      it at all; a plain restart is then a no-op no matter which unit you name.
#      All three broken nodes were in exactly that state.
#   2. THE RESULT WAS ALWAYS {"ok":true}. This branch waited up to 30s for SOCKS
#      and then ignored the outcome; the `restart` branch slept 3s and claimed
#      success. So the UI reported "restarted" over a dead daemon.
#   3. The underlying torrc was unbindable, so every restart failed anyway —
#      fixed separately in the torrc generator.
#
# tor@default is targeted explicitly because it is the unit that carries the
# state worth resetting; single-instance installs have only `tor`, hence the
# detection and fallback.
#
# Returns 0 only when SOCKS answers. Callers must not report success without it.
restart_tor_daemon() {
  local unit=tor
  if systemctl list-unit-files 'tor@*.service' 2>/dev/null | grep -q 'tor@'; then
    unit=tor@default
  fi
  systemctl reset-failed "$unit" 2>/dev/null || true
  if ! systemctl restart "$unit" 2>/dev/null; then
    if [ "$unit" != tor ]; then
      log "restart of $unit failed — falling back to tor.service"
      systemctl reset-failed tor 2>/dev/null || true
      systemctl restart tor 2>/dev/null || true
    fi
  fi
  log "Restarted $unit"

  local i
  for i in $(seq 1 30); do
    if timeout 1 bash -c 'echo > /dev/tcp/127.0.0.1/9050' 2>/dev/null; then
      log "Tor SOCKS answering after ${i}s"
      return 0
    fi
    sleep 1
  done
  log "ERROR: Tor SOCKS not answering 30s after restarting $unit"
  return 1
}

case "$ACTION_TYPE" in
  write-torrc-and-restart)
    if [ ! -f "$TORRC_STAGED" ]; then
      log "ERROR: No staged torrc at $TORRC_STAGED"
      write_result '{"ok":false,"error":"No staged torrc"}'
      exit 1
    fi
    cp "$TORRC_STAGED" /etc/tor/torrc
    chown debian-tor:debian-tor /etc/tor/torrc 2>/dev/null || true
    log "torrc updated from staged file"

    if restart_tor_daemon; then
      sync_hostnames
      write_result '{"ok":true}'
    else
      # Never claim success on a dead daemon: the caller surfaces this straight
      # to the operator, and a false "restarted" is how an outage stays hidden.
      sync_hostnames
      write_result '{"ok":false,"error":"Tor restarted but SOCKS never came up on 127.0.0.1:9050 — check journalctl -u tor@default"}'
      exit 1
    fi
    ;;

  restart)
    if restart_tor_daemon; then
      sync_hostnames
      write_result '{"ok":true}'
    else
      sync_hostnames
      write_result '{"ok":false,"error":"Tor restarted but SOCKS never came up on 127.0.0.1:9050 — check journalctl -u tor@default"}'
      exit 1
    fi
    ;;

  delete-service)
    NAME=$(echo "$ACTION" | python3 -c "import sys,json; print(json.load(sys.stdin).get('name',''))" 2>/dev/null || echo "")
    if [ -z "$NAME" ]; then
      write_result '{"ok":false,"error":"Missing service name"}'
      exit 1
    fi
    if ! echo "$NAME" | grep -qE '^[a-zA-Z0-9_-]+$'; then
      write_result '{"ok":false,"error":"Invalid service name"}'
      exit 1
    fi
    rm -rf "/var/lib/tor/hidden_service_${NAME}" 2>/dev/null || true
    rm -rf "/var/lib/archipelago/tor/hidden_service_${NAME}" 2>/dev/null || true
    rm -f "${HOSTNAMES_DIR}/${NAME}" 2>/dev/null || true
    log "Deleted hidden service: $NAME"
    write_result '{"ok":true}'
    ;;

  rename-service)
    NAME=$(echo "$ACTION" | python3 -c "import sys,json; print(json.load(sys.stdin).get('name',''))" 2>/dev/null || echo "")
    TIMESTAMP=$(echo "$ACTION" | python3 -c "import sys,json; print(json.load(sys.stdin).get('timestamp',''))" 2>/dev/null || echo "")
    if [ -z "$NAME" ] || [ -z "$TIMESTAMP" ]; then
      write_result '{"ok":false,"error":"Missing service name or timestamp"}'
      exit 1
    fi
    if ! echo "$NAME" | grep -qE '^[a-zA-Z0-9_-]+$'; then
      write_result '{"ok":false,"error":"Invalid service name"}'
      exit 1
    fi
    if ! echo "$TIMESTAMP" | grep -qE '^[0-9]+$'; then
      write_result '{"ok":false,"error":"Invalid timestamp"}'
      exit 1
    fi
    OLD_SUFFIX="${NAME}_old_${TIMESTAMP}"
    for base in /var/lib/tor /var/lib/archipelago/tor; do
      SRC="${base}/hidden_service_${NAME}"
      DST="${base}/hidden_service_${OLD_SUFFIX}"
      if [ -d "$SRC" ]; then
        mv "$SRC" "$DST"
        log "Renamed $SRC -> $DST"
      fi
    done
    rm -f "${HOSTNAMES_DIR}/${NAME}" 2>/dev/null || true
    write_result '{"ok":true}'
    ;;

  sync-hostnames)
    sync_hostnames
    write_result '{"ok":true}'
    ;;

  reboot)
    write_result '{"ok":true}'
    log "System reboot initiated"
    sleep 1
    systemctl reboot
    ;;

  *)
    log "Unknown action: $ACTION_TYPE"
    write_result '{"ok":false,"error":"Unknown action"}'
    exit 1
    ;;
esac
