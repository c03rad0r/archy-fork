#!/bin/bash
# OTA crash-loop guard — runs as root from ExecStartPre=+- on archipelago.service.
#
# Covers the failure mode verify_pending_update() cannot: a freshly-applied
# binary that can't even start (SEGV/ENOEXEC — e.g. the truncated 17MB binary
# .198 installed on the v1.7.103 OTA, which crash-looped 236 times with a
# perfectly good backup sitting in update-backup/). The in-binary probe never
# runs because the binary never runs, so this guard counts start attempts from
# outside and restores the backup binary once the new one has clearly failed.
#
# Scope is deliberately narrow: it acts ONLY while the post-OTA pending-verify
# marker exists (written by apply_update just before the restart, deleted by
# the new binary once it boots and passes its probes). A crash loop with no
# marker is not an OTA gone wrong, and this script stays out of it.
#
# Always exits 0 — a guard must never be the reason the service can't start.

set -u

DATA_DIR=/var/lib/archipelago
MARKER="$DATA_DIR/update-pending-verify.json"
COUNT_FILE="$DATA_DIR/ota-crash-guard.count"
BACKUP="$DATA_DIR/update-backup/archipelago"
BINARY=/usr/local/bin/archipelago
MAX_ATTEMPTS=5

log() {
    echo "$*" | systemd-cat -t ota-crash-guard -p warning 2>/dev/null || true
}

# No pending OTA verification -> nothing to guard; clear any stale counter.
if [ ! -f "$MARKER" ]; then
    rm -f "$COUNT_FILE"
    exit 0
fi

# Count this start attempt. The counter only accumulates while the marker
# exists; a healthy new binary deletes the marker on its first successful
# boot, and the next start clears the counter above.
count=$(cat "$COUNT_FILE" 2>/dev/null || echo 0)
case "$count" in ''|*[!0-9]*) count=0 ;; esac
count=$((count + 1))
echo "$count" > "$COUNT_FILE" 2>/dev/null || true

if [ "$count" -lt "$MAX_ATTEMPTS" ]; then
    exit 0
fi

if [ ! -f "$BACKUP" ]; then
    log "OTA crash guard: $count failed start attempts but no backup binary at $BACKUP — cannot roll back"
    exit 0
fi

# Already restored (or the OTA never replaced the binary)? Don't loop.
if cmp -s "$BACKUP" "$BINARY"; then
    exit 0
fi

# Restore via copy-to-temp + atomic rename; never truncate the live path.
tmp="$BINARY.rollback.$$"
if cp "$BACKUP" "$tmp" && chown root:root "$tmp" && chmod 755 "$tmp" && mv "$tmp" "$BINARY"; then
    # Leave a tombstone for the UI/logs instead of the marker so the restored
    # binary doesn't run the post-OTA probe against the rolled-back version.
    mv "$MARKER" "$DATA_DIR/update-rolled-back.json" 2>/dev/null || rm -f "$MARKER"
    rm -f "$COUNT_FILE"
    log "OTA crash guard: restored previous binary after $count failed start attempts of the updated one"
else
    rm -f "$tmp" 2>/dev/null
    log "OTA crash guard: failed to restore backup binary (cp/mv error)"
fi

exit 0
