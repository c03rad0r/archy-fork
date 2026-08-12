#!/usr/bin/env bash
# Rotate this node's LND macaroons after the /lnd-connect-info leak.
#
# WHY THIS EXISTS
#   GET /lnd-connect-info used to answer unauthenticated callers with the LND
#   admin macaroon, the TLS cert, the gRPC/REST ports and the node's onion
#   address. Anything that reached port 18083 — any fips0 mesh peer, LAN host
#   or Tailscale peer — could take it. Every macaroon on an affected node must
#   be treated as known to an attacker.
#
# WHAT ROTATION ACTUALLY DOES
#   LND derives every macaroon it issues from a root key kept in macaroons.db.
#   Remove that root key and the macaroon files, restart, and LND mints a fresh
#   root key and a fresh set of macaroons on unlock. Every previously issued
#   macaroon — including any the attacker holds — stops verifying.
#
# WHY YOUR FUNDS AND CHANNELS SURVIVE
#   Macaroons are bearer tokens, not keys. Coins live in wallet.db and channel
#   state in channel.db; channels are secured by the node's identity and channel
#   keys, none of which are derived from the macaroon root key. This script
#   never touches, moves or opens either database. The wallet is not re-created,
#   the seed is not re-entered, and no channel is closed or force-closed.
#
#   The only interruption is the LND restart itself, which is the same event as
#   a reboot or an update — peers reconnect and channels resume. What this
#   script verifies is exactly that: it records the node's identity pubkey and
#   its channel counts BEFORE, and aborts loudly if either differs after.
#
#   Note it does NOT assert wallet.db is byte-identical, which would be the
#   wrong test: btcwallet records chain-sync progress inside wallet.db, so the
#   file legitimately changes on every start. Asserting byte-identity would fire
#   a frightening false alarm on a completely healthy rotation.
#
# WHAT IT DELIBERATELY NEVER DOES
#   It never reads, prints, logs or copies a macaroon's CONTENT. Everything it
#   reports is a SHA-256 digest or a file size, which is enough to prove the
#   material changed without disclosing it to the terminal, the scrollback or
#   whoever is reading over your shoulder.
#
# THE ORDERING GUARD
#   Rotating before the leak is patched is worse than useless: the new macaroon
#   is readable through the same open door within seconds, and you would think
#   you were safe. So this script REFUSES to rotate unless the running binary
#   carries the fix. Override only if you genuinely know better.
set -uo pipefail

LND_DIR="/var/lib/archipelago/lnd/data/chain/bitcoin/mainnet"
BIN="/usr/local/bin/archipelago"
CONTAINER="lnd"
APPLY=no; ASSUME_YES=no; FORCE_UNPATCHED=no

usage() {
    cat <<'USAGE'
Usage: rotate-lnd-macaroon.sh [--apply] [--yes] [--force-unpatched]

  (no flags)          Detect and report only. Changes nothing. THE DEFAULT.
  --apply             Perform the rotation. Requires --yes as well.
  --yes               Confirm a destructive-to-credentials action.
  --force-unpatched   Rotate even though the running binary lacks the
                      /lnd-connect-info fix. You will almost certainly be
                      re-leaking the new macaroon. Not recommended.

Exit: 0 ok / verdict clean, 1 error or aborted, 2 rotation needed (detect mode).
USAGE
}

while [ $# -gt 0 ]; do
    case "$1" in
        --apply) APPLY=yes; shift ;;
        --yes) ASSUME_YES=yes; shift ;;
        --force-unpatched) FORCE_UNPATCHED=yes; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown argument: $1" >&2; usage; exit 1 ;;
    esac
done

say() { printf '%s\n' "$*"; }
die() { printf 'ERROR: %s\n' "$*" >&2; exit 1; }

# Digest helper. Uses sudo because LND's data dir is 0700 and owned by the
# container's mapped uid. Prints ONLY a digest, never a byte of content.
digest() {
    sudo sha256sum "$1" 2>/dev/null | awk '{print $1}'
}

# Enumerate the macaroon material. This MUST run under sudo rather than as a
# shell glob: the LND data dir is 0700 owned by the container's mapped uid, so
# `"$LND_DIR"/*.macaroon` does not expand in this (unprivileged) shell. It
# would silently stay literal, making both the backup loop and the removal
# loop no-ops while every surrounding step still reported success.
macaroon_files() {
    sudo find "$LND_DIR" -maxdepth 1 \
        \( -name '*.macaroon' -o -name 'macaroons.db' \) 2>/dev/null
}

say "── LND macaroon rotation ─────────────────────────────────────────"

sudo test -d "$LND_DIR" || die "LND data dir not found at $LND_DIR — is LND installed on this node?"

# ── The ordering guard ────────────────────────────────────────────────
# The fix adds this exact string to the binary. Its presence is the only
# machine-checkable evidence that the door is shut on THIS node.
PATCH_MARKER="/auth/session-check"
if sudo grep -qa -- "$PATCH_MARKER" "$BIN" 2>/dev/null; then
    say "patch status : PRESENT — the running binary carries the /lnd-connect-info fix"
    PATCHED=yes
else
    say "patch status : ABSENT — this binary still leaks /lnd-connect-info"
    PATCHED=no
fi

# ── Live reachability check ───────────────────────────────────────────
# Proves the hole from the outside rather than trusting the marker alone.
LEAK_CODE=$(curl -s -m 5 -o /dev/null -w '%{http_code}' http://127.0.0.1:18083/lnd-connect-info 2>/dev/null || echo "000")
case "$LEAK_CODE" in
    200) say "live probe   : LEAKING — unauthenticated GET returned 200" ;;
    401|403) say "live probe   : closed — unauthenticated GET returned $LEAK_CODE" ;;
    000) say "live probe   : lnd-ui not reachable on :18083 (app may be stopped)" ;;
    *) say "live probe   : unauthenticated GET returned $LEAK_CODE" ;;
esac

OLD_ADMIN=$(digest "$LND_DIR/admin.macaroon")
OLD_ROOT=$(digest "$LND_DIR/macaroons.db")
say "admin.macaroon : ${OLD_ADMIN:-<absent>}"
say "macaroons.db   : ${OLD_ROOT:-<absent>}"

# Node identity + channel census BEFORE. `lncli` runs INSIDE the container and
# reads the macaroon off its own disk, so the secret never crosses into this
# script, its output, or the operator's scrollback.
#
# It reports active AND inactive channel counts separately, because only their
# SUM is a safety property. `num_active_channels` counts channels whose peer is
# currently online, so it legitimately dips straight after any restart while
# peers reconnect — asserting on it alone would abort a perfectly healthy
# rotation. What must not change is the total number of channels the node
# holds, and its identity.
lnd_state() {
    podman exec "$CONTAINER" lncli --network=mainnet getinfo 2>/dev/null \
        | python3 -c 'import json,sys
try: d=json.load(sys.stdin)
except Exception: raise SystemExit(1)
print("%s %s %s %s" % (d.get("identity_pubkey",""), d.get("num_active_channels",0),
                       d.get("num_inactive_channels",0), d.get("num_pending_channels",0)))' 2>/dev/null
}
OLD_STATE=$(lnd_state)
if [ -n "$OLD_STATE" ]; then
    set -- $OLD_STATE
    OLD_PUBKEY="$1"; OLD_ACTIVE="$2"; OLD_INACTIVE="$3"; OLD_PENDING="$4"
    OLD_TOTAL=$((OLD_ACTIVE + OLD_INACTIVE))
    say "node identity  : ${OLD_PUBKEY:0:16}…"
    say "channels       : $OLD_TOTAL open ($OLD_ACTIVE active, $OLD_INACTIVE inactive), $OLD_PENDING pending  (must survive)"
else
    OLD_PUBKEY=""; OLD_ACTIVE=""; OLD_INACTIVE=""; OLD_PENDING=""; OLD_TOTAL=""
    say "channels       : could not read LND state (locked or down) — see below"
fi

if [ "$APPLY" != yes ]; then
    say
    say "Detect-only. Re-run with --apply --yes to rotate."
    [ "$PATCHED" = yes ] || say "Patch this node FIRST, or the new macaroon leaks immediately."
    exit 2
fi

[ "$ASSUME_YES" = yes ] || die "--apply requires --yes (this invalidates every existing macaroon)"

if [ "$PATCHED" != yes ] && [ "$FORCE_UNPATCHED" != yes ]; then
    die "refusing to rotate on an unpatched node — the new macaroon would leak through the same hole. Deploy the fix first, or pass --force-unpatched if you truly intend this."
fi

# ── Back up, so a mistake is recoverable ──────────────────────────────
# Kept 0700 and OUTSIDE the dir LND rescans. Still secret material: it is the
# old root key. Delete it once you have confirmed every client re-paired.
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
BACKUP="/var/lib/archipelago/lnd/macaroon-rotation-$STAMP"
sudo mkdir -p "$BACKUP" || die "could not create $BACKUP"
sudo chmod 700 "$BACKUP"
mapfile -t MAC_FILES < <(macaroon_files)
[ "${#MAC_FILES[@]}" -gt 0 ] || die "no macaroon material found in $LND_DIR — nothing to rotate, and restarting LND for no reason would be a pointless outage"

say
say "backing up old macaroon material to $BACKUP (0700, contents never printed)"
for f in "${MAC_FILES[@]}"; do
    sudo cp -a "$f" "$BACKUP/" || die "backup of $(basename "$f") failed — aborting before any deletion"
    say "  backed up $(basename "$f")"
done

# Count what actually landed. A backup that silently copied nothing is the one
# failure mode that would make the deletion below unrecoverable.
BACKED_UP=$(sudo find "$BACKUP" -maxdepth 1 -type f 2>/dev/null | wc -l)
[ "$BACKED_UP" -eq "${#MAC_FILES[@]}" ] \
    || die "backup incomplete — $BACKED_UP of ${#MAC_FILES[@]} files in $BACKUP. Refusing to delete anything."

say "stopping $CONTAINER"
podman stop "$CONTAINER" >/dev/null 2>&1 || say "  (stop reported non-zero; continuing to check state)"

# Only now, with a verified backup in hand, remove the credential material.
say "removing macaroon root key and issued macaroons"
for f in "${MAC_FILES[@]}"; do
    sudo rm -f "$f" || die "could not remove $(basename "$f") — restore from $BACKUP"
done

say "starting $CONTAINER — archipelago auto-unlocks and LND re-mints on unlock"
podman start "$CONTAINER" >/dev/null 2>&1 || die "could not start $CONTAINER — restore from $BACKUP"

# ── Wait for regeneration ─────────────────────────────────────────────
printf 'waiting for a fresh admin.macaroon'
NEW_ADMIN=""
for _ in $(seq 1 60); do
    sleep 5
    NEW_ADMIN=$(digest "$LND_DIR/admin.macaroon")
    [ -n "$NEW_ADMIN" ] && break
    printf '.'
done
printf '\n'

[ -n "$NEW_ADMIN" ] || die "no new admin.macaroon after 5 minutes. LND may not have unlocked. Old material is intact in $BACKUP — restore it there and investigate before retrying."

# ── Verify: credentials changed, node and channels did not ────────────
FAIL=""

[ "$NEW_ADMIN" != "$OLD_ADMIN" ] || FAIL="$FAIL admin-macaroon-UNCHANGED"

# Deliberately NOT a wallet.db byte-identity check. btcwallet records chain
# sync progress inside wallet.db, so that file legitimately changes on every
# start; asserting equality would fire a frightening false alarm on a
# completely healthy rotation. The meaningful invariant is that this is still
# the SAME Lightning node holding the SAME channels — so assert that instead.
printf 'waiting for LND to report its state'
NEW_STATE=""
for _ in $(seq 1 60); do
    NEW_STATE=$(lnd_state)
    [ -n "$NEW_STATE" ] && break
    printf '.'
    sleep 5
done
printf '\n'

if [ -n "$OLD_PUBKEY" ]; then
    if [ -n "$NEW_STATE" ]; then
        set -- $NEW_STATE
        NEW_PUBKEY="$1"; NEW_ACTIVE="$2"; NEW_INACTIVE="$3"; NEW_PENDING="$4"
        NEW_TOTAL=$((NEW_ACTIVE + NEW_INACTIVE))
        [ "$NEW_PUBKEY" = "$OLD_PUBKEY" ] || FAIL="$FAIL NODE-IDENTITY-CHANGED"
        [ "$NEW_TOTAL" = "$OLD_TOTAL" ] || FAIL="$FAIL OPEN-CHANNELS-$OLD_TOTAL-to-$NEW_TOTAL"
        [ "$NEW_PENDING" = "$OLD_PENDING" ] || FAIL="$FAIL PENDING-CHANNELS-$OLD_PENDING-to-$NEW_PENDING"
        say
        say "node identity  : ${NEW_PUBKEY:0:16}…  (unchanged)"
        say "channels       : $NEW_TOTAL open ($NEW_ACTIVE active, $NEW_INACTIVE inactive), $NEW_PENDING pending"
        if [ "$NEW_ACTIVE" != "$OLD_ACTIVE" ]; then
            say "                 active count differs from before ($OLD_ACTIVE -> $NEW_ACTIVE) — this is"
            say "                 normal for a few minutes after any restart while peers reconnect."
        fi
    else
        FAIL="$FAIL LND-STATE-UNREADABLE-AFTER"
    fi
else
    say
    say "channels       : NOT VERIFIED — LND state was already unreadable before the"
    say "                 rotation, so there is no baseline to compare against."
    say "                 Check 'lncli getinfo' yourself before trusting this run."
fi

say
say "new admin.macaroon : $NEW_ADMIN"

if [ -n "$FAIL" ]; then
    say
    die "rotation verification FAILED:$FAIL — old material is in $BACKUP"
fi

# ── BTCPay's inline copy ──────────────────────────────────────────────
# BTCPay reaches the internal LND node with a connection string that carries
# the macaroon INLINE as hex, not as a file path: LND's datadir is owned by its
# container's mapped uid, so btcpay cannot bind-mount the file. That copy is
# therefore now a dead credential, and nothing else will notice — the daemon
# only regenerates this secret when LND's TLS *cert* thumbprint changes, which
# macaroon rotation does not touch. The node keeps looking healthy (btcpay up,
# LND up) while every Lightning invoice BTCPay tries to create fails.
#
# Deleting the secret file gets the daemon to regenerate it from the new
# macaroon on its next reconcile tick. That is necessary but NOT sufficient, and
# the difference matters: the RUNNING container still holds the dead value, and
# the periodic reconciler only ever runs in `ExistingOnly` mode, where env drift
# on a restart-sensitive app (btcpay-server is one) is detected and then
# deliberately skipped — "leaving running restart-sensitive app untouched". So
# the container has to be recreated on purpose. The dashboard path
# (Settings → Lightning credentials) does this itself by flagging the app as
# credential-rotated; a shell script cannot reach that in-process flag, so it
# removes the container instead and lets the orchestrator's own desired-state
# recovery rebuild it around unchanged data, ports and volumes.
#
# Nothing is printed but a path — never the value.
BTCPAY_SECRET="/var/lib/archipelago/secrets/btcpay-lnd-connection"
BTCPAY_NOTE=no
if sudo test -f "$BTCPAY_SECRET"; then
    if sudo rm -f "$BTCPAY_SECRET"; then
        say
        say "btcpay         : removed its stale connection string ($BTCPAY_SECRET)."
        say "                 The daemon regenerates it from the new macaroon within a minute."
        BTCPAY_NOTE=yes
        if podman container exists btcpay-server 2>/dev/null; then
            say "                 Recreating btcpay-server so it stops using the dead one."
            podman stop btcpay-server >/dev/null 2>&1 || true
            if podman rm -f btcpay-server >/dev/null 2>&1; then
                say "                 Removed; the orchestrator rebuilds it around its existing"
                say "                 data (it was running, so desired-state recovery restores it)."
            else
                say "                 ⚠ could not remove btcpay-server. Its Lightning payments will"
                say "                 fail until it is recreated."
                BTCPAY_NOTE=warn
            fi
        fi
    else
        say
        say "btcpay         : ⚠ could not remove $BTCPAY_SECRET. BTCPay is still holding"
        say "                 the OLD macaroon, so its Lightning payments will fail until"
        say "                 that file is deleted and btcpay-server is recreated."
        BTCPAY_NOTE=warn
    fi
fi

say
say "✅ Rotated. Every macaroon issued before now no longer verifies."
say
say "WHAT BREAKS, AND WHAT TO DO:"
say "  Anything paired with the old admin macaroon must be re-paired — most"
say "  importantly Zeus or any other remote wallet. Open the LND app in the UI"
say "  and scan the new pairing QR; it serves the new macaroon."
say
say "  Your funds and channels are untouched: the node kept its identity and"
say "  no channel was closed."
if [ "${BTCPAY_NOTE:-no}" != no ]; then
    say
    say "  CONFIRM BTCPAY CAME BACK. A silent failure here looks identical to success:"
    say "  btcpay stays up and healthy while every Lightning payment it tries fails."
    say "    podman inspect btcpay-server --format '{{.Created}}'   # should be just now"
    say "    sudo test -f $BTCPAY_SECRET && echo regenerated"
fi
say
say "  Once every client is re-paired, delete the backup — it holds the OLD"
say "  root key, which is still sensitive:"
say "    sudo rm -rf $BACKUP"
exit 0
