#!/bin/bash
# host-secrets-audit.sh — does THIS node run the fleet-shared, image-baked SSH
# host keys and TLS private key, or its own?
#
# Audit finding F-03 / phase 10 KEY-02, deployed half (decision D-06).
#
# 10-03 fixed the ISO builder: the rootfs no longer carries identity material
# and first-boot regeneration fails closed. Nodes already in the field never
# receive any of that — the first-boot script is installed by the installer,
# not shipped by OTA — and a node that hit the old fail-open path
# (`WARNING: TLS regeneration failed, keeping baked key` plus an unconditional
# `touch $MARKER`) is running key material that every downloader of that ISO
# also holds, and will never try again. This script is how such a node is
# found, and how it is fixed.
#
# ── SAFETY MODEL (D-06: detect-report-then-apply) ────────────────────────────
#   --detect (default)  read-only. Writes only its own verdict file. Always
#                       exits 0: detection is informational and must never fail
#                       a boot.
#   --apply             prints what it WOULD do and exits 0 having touched
#                       nothing. A mistyped invocation is inert.
#   --apply --yes       rotates — and only if the detect pass returned `shared`.
#                       A node whose verdict is `per-node` cannot have its keys
#                       rotated by this script even by explicit command.
#
# The boot unit (image-recipe/configs/archipelago-host-secrets-audit.service)
# runs --detect only and contains no apply path.
#
# ── THIS IS A SANCTIONED KEY PRODUCER. THERE ARE NOW THREE. ─────────────────
# Do not unify them, and do not let their parameters drift apart:
#   1. gen_tls()/gen_ssh() in image-recipe/_archived/build-auto-installer-iso.sh
#      — first boot, on the node, from the ISO.
#   2. TlsMaterial::regenerate() in core/archipelago/src/api/rpc/system/handlers.rs
#      — TLS only, re-minted after `server.set-name` so the SAN matches.
#   3. rotate_tls()/rotate_ssh() below — deployed nodes, operator-driven, once.
# All three: rsa:2048, 3650 days, the same subject and the same SAN set, stage
# to `.new` siblings of the destination (same directory, so the final mv is a
# rename(2) and therefore atomic), parse both halves back AND prove they are a
# matching pair, then swap. A key from one generation beside a cert from
# another passes both individual parse checks and still breaks nginx.
#
# Producer 3 has to exist separately: producer 1 lives inside an ISO build
# script that is not present on a deployed node, and producer 2 does TLS only —
# nothing in the daemon has ever rotated an SSH host key.
#
# ── TEST SEAM ───────────────────────────────────────────────────────────────
# HOST_SECRETS_ROOT prefixes every absolute path, exactly as
# FIRST_BOOT_SECRETS_ROOT does for 10-03's first-boot script. Unset in
# production the expansion is empty and behaviour is byte-identical; set, it is
# what makes tests/first-boot-secrets/rotation-tests.sh able to force a
# `shared` node into existence and drive a real rotation against it.
#
# Usage:
#   host-secrets-audit.sh [--detect] [--json] [--quiet]
#   host-secrets-audit.sh --apply [--yes]

set -euo pipefail

ROOT="${HOST_SECRETS_ROOT:-}"

MARKER="$ROOT/var/lib/archipelago/.secrets-regenerated"
FAILED_RECORD="$ROOT/var/lib/archipelago/first-boot-secrets.failed"
FIRST_BOOT_LOG="$ROOT/var/log/archipelago-first-boot-secrets.log"
STRIPPED_MARKER="$ROOT/opt/archipelago/rootfs-identity-stripped"
LUKS_KEY="$ROOT/root/.luks-archipelago.key"
MACHINE_ID="$ROOT/etc/machine-id"
SSH_DIR="$ROOT/etc/ssh"
SSL_DIR="$ROOT/etc/archipelago/ssl"
TLS_KEY="$SSL_DIR/archipelago.key"
TLS_CRT="$SSL_DIR/archipelago.crt"
STATE_DIR="$ROOT/var/lib/archipelago"
AUDIT_JSON="$STATE_DIR/host-secrets-audit.json"
ROTATION_JSON="$STATE_DIR/host-key-rotation.json"
CONSOLE="$ROOT/dev/console"

# A key regenerated at first boot carries an mtime within seconds of the
# anchor. A key baked into the image carries the image build time — days or
# weeks earlier. 300s absorbs the spread between the anchor being touched and
# the last key being written, without being wide enough to hide a build-time
# key.
ANCHOR_SKEW_SECONDS=300

MODE="detect"
CONFIRMED=0
QUIET=0
EMIT_JSON=0

while [ $# -gt 0 ]; do
    case "$1" in
        --detect)  MODE="detect" ;;
        --apply)   MODE="apply" ;;
        --yes)     CONFIRMED=1 ;;
        --json)    EMIT_JSON=1 ;;
        --quiet)   QUIET=1 ;;
        -h|--help)
            sed -n '2,50p' "$0"
            exit 0
            ;;
        *)
            echo "host-secrets-audit: unknown argument: $1" >&2
            exit 2
            ;;
    esac
    shift
done

say() { [ "$QUIET" = 1 ] || echo "$*"; }

# Evidence must name production paths, not the harness's temp root.
disp() { printf '%s' "${1#"$ROOT"}"; }

json_escape() { printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g'; }

json_array() {
    local first=1 item
    printf '['
    for item in "$@"; do
        [ "$first" = 1 ] || printf ', '
        first=0
        printf '"%s"' "$(json_escape "$item")"
    done
    printf ']'
}

mtime_of() { stat -c %Y "$1" 2>/dev/null || true; }

now_iso() { date -u +%Y-%m-%dT%H:%M:%SZ; }

# ── Fingerprints ────────────────────────────────────────────────────────────
# Fingerprints of PUBLIC keys are public data (T-10-35: accept). The private
# keys are never read by this script except by the generators that replace
# them.

ssh_fingerprints() {
    local f
    for f in "$SSH_DIR"/ssh_host_*_key.pub; do
        [ -e "$f" ] || continue
        ssh-keygen -lf "$f" 2>/dev/null | sed "s|^|$(disp "$f"): |" || true
    done
}

tls_fingerprint() {
    [ -s "$TLS_CRT" ] || return 0
    openssl x509 -in "$TLS_CRT" -noout -fingerprint -sha256 2>/dev/null \
        | sed 's/^.*=//' || true
}

# ── Detection ───────────────────────────────────────────────────────────────
# Outputs (globals, so --apply can reuse the pass without re-running it):
#   VERDICT       per-node | shared | fail-closed-missing | unknown
#   EVIDENCE[]    one string per signal that fired, each naming its file
#   SSH_SHARED    1 when this node's SSH host keys are believed image-baked
#   TLS_SHARED    1 when this node's TLS key is believed image-baked

VERDICT="unknown"
EVIDENCE=()
SSH_SHARED=0
TLS_SHARED=0

detect() {
    VERDICT="unknown"
    EVIDENCE=()
    SSH_SHARED=0
    TLS_SHARED=0

    local ssh_keys=() f
    for f in "$SSH_DIR"/ssh_host_*_key; do
        [ -e "$f" ] || continue
        ssh_keys+=("$f")
    done

    local have_ssh=0 have_tls=0
    [ "${#ssh_keys[@]}" -gt 0 ] && have_ssh=1
    [ -s "$TLS_KEY" ] && have_tls=1

    # Signal 4 — rootfs provenance. Recorded on every run because it changes
    # what missing material MEANS, and a reader of the JSON needs that context
    # regardless of the verdict.
    local stripped=0
    if [ -e "$STRIPPED_MARKER" ]; then
        stripped=1
        EVIDENCE+=("provenance: $(disp "$STRIPPED_MARKER") present — this rootfs shipped identity-free (10-03 or later ISO)")
    else
        EVIDENCE+=("provenance: $(disp "$STRIPPED_MARKER") absent — this rootfs predates the 10-03 identity strip, so baked material is possible")
    fi

    # Signal 3 — 10-03's durable failure record.
    local failed_record=0
    if [ -e "$FAILED_RECORD" ]; then
        failed_record=1
        EVIDENCE+=("failure record: $(disp "$FAILED_RECORD") present — first-boot generation reported failure and did not silently continue")
    fi

    # ── Precedence step 1: is the material even there? ──────────────────────
    # Missing material can never be SHARED material. On a stripped rootfs this
    # is fail-closed working as designed; without the provenance marker it is
    # still missing, and saying so is more honest than guessing.
    if [ "$have_ssh" = 0 ] || [ "$have_tls" = 0 ]; then
        [ "$have_ssh" = 0 ] && EVIDENCE+=("missing: no $(disp "$SSH_DIR")/ssh_host_*_key on this node")
        [ "$have_tls" = 0 ] && EVIDENCE+=("missing: $(disp "$TLS_KEY") is absent or empty")
        if [ "$stripped" = 0 ]; then
            EVIDENCE+=("note: provenance marker absent, so 'fail-closed' is inferred from the absence itself, not from a build-time guarantee")
        fi
        VERDICT="fail-closed-missing"
        return 0
    fi

    # ── Precedence step 2: the fail-open fingerprint ────────────────────────
    # `.secrets-regenerated` present AND a WARNING: line in the first-boot log
    # is precisely what the pre-10-03 fail-open path produced (builder :1647,
    # :1659, :1663). This is direct evidence, not an inference from timestamps,
    # so it outranks the mtime signal — and the two WARNING strings name which
    # class survived, so the rotation can be narrowed to it.
    if [ -e "$MARKER" ] && [ -f "$FIRST_BOOT_LOG" ] && grep -q 'WARNING:' "$FIRST_BOOT_LOG" 2>/dev/null; then
        local tls_warn=0 ssh_warn=0
        grep -q 'WARNING: TLS regeneration failed' "$FIRST_BOOT_LOG" 2>/dev/null && tls_warn=1
        grep -q 'WARNING: ssh-keygen -A failed' "$FIRST_BOOT_LOG" 2>/dev/null && ssh_warn=1
        if [ "$tls_warn" = 0 ] && [ "$ssh_warn" = 0 ]; then
            # An unrecognised WARNING. Do not narrow on a guess.
            tls_warn=1
            ssh_warn=1
            EVIDENCE+=("fail-open fingerprint: $(disp "$MARKER") present and $(disp "$FIRST_BOOT_LOG") carries an unrecognised WARNING: line — both key classes treated as shared")
        else
            EVIDENCE+=("fail-open fingerprint: $(disp "$MARKER") present and $(disp "$FIRST_BOOT_LOG") records the first-boot generator giving up and keeping the baked key")
        fi
        [ "$tls_warn" = 1 ] && { TLS_SHARED=1; EVIDENCE+=("shared: $(disp "$TLS_KEY") — the first-boot log says TLS regeneration failed and the baked key was kept"); }
        [ "$ssh_warn" = 1 ] && { SSH_SHARED=1; EVIDENCE+=("shared: $(disp "$SSH_DIR")/ssh_host_*_key — the first-boot log says ssh-keygen -A failed and the baked host keys were kept"); }
        VERDICT="shared"
        return 0
    fi

    # ── Precedence step 3: the mtime anchor ─────────────────────────────────
    local anchor="" anchor_kind=""
    if [ -e "$MARKER" ]; then
        anchor="$MARKER"; anchor_kind="first-boot regeneration marker"
    elif [ -e "$LUKS_KEY" ]; then
        anchor="$LUKS_KEY"; anchor_kind="LUKS key written by the installer with dd if=/dev/urandom"
    elif [ -s "$MACHINE_ID" ]; then
        anchor="$MACHINE_ID"; anchor_kind="machine-id, populated on this node's first boot"
    fi

    if [ -z "$anchor" ]; then
        EVIDENCE+=("no anchor: none of $(disp "$MARKER"), $(disp "$LUKS_KEY"), $(disp "$MACHINE_ID") is usable, so this node's first boot cannot be dated")
        VERDICT="unknown"
        return 0
    fi

    local anchor_mtime
    anchor_mtime=$(mtime_of "$anchor")
    if [ -z "$anchor_mtime" ]; then
        EVIDENCE+=("no anchor: $(disp "$anchor") exists but could not be stat'd")
        VERDICT="unknown"
        return 0
    fi
    EVIDENCE+=("anchor: $(disp "$anchor") ($anchor_kind), mtime $(date -u -d "@$anchor_mtime" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || echo "$anchor_mtime")")

    older_than_anchor() {
        local file="$1" m age
        m=$(mtime_of "$file")
        [ -n "$m" ] || return 1
        age=$((anchor_mtime - m))
        [ "$age" -gt "$ANCHOR_SKEW_SECONDS" ]
    }

    for f in "${ssh_keys[@]}"; do
        if older_than_anchor "$f"; then
            SSH_SHARED=1
            EVIDENCE+=("shared: $(disp "$f") mtime is $(( anchor_mtime - $(mtime_of "$f") ))s older than the anchor (threshold ${ANCHOR_SKEW_SECONDS}s) — it came from the image, not from this node's first boot")
        fi
    done
    if older_than_anchor "$TLS_KEY"; then
        TLS_SHARED=1
        EVIDENCE+=("shared: $(disp "$TLS_KEY") mtime is $(( anchor_mtime - $(mtime_of "$TLS_KEY") ))s older than the anchor (threshold ${ANCHOR_SKEW_SECONDS}s) — it came from the image, not from this node's first boot")
    fi

    if [ "$SSH_SHARED" = 1 ] || [ "$TLS_SHARED" = 1 ]; then
        VERDICT="shared"
        return 0
    fi

    # Never claim per-node while the node's own generator's last word was
    # failure. A clean-looking mtime is not evidence that generation succeeded.
    if [ "$failed_record" = 1 ]; then
        EVIDENCE+=("withholding per-node: every key is newer than the anchor, but $(disp "$FAILED_RECORD") stands, so success is not established")
        VERDICT="unknown"
        return 0
    fi

    EVIDENCE+=("per-node: every SSH host key and the TLS key is newer than the anchor, so all of it was generated on this node")
    VERDICT="per-node"
    return 0
}

write_audit_json() {
    local fps=() fp tls_fp
    while IFS= read -r fp; do [ -n "$fp" ] && fps+=("$fp"); done < <(ssh_fingerprints)
    tls_fp=$(tls_fingerprint)

    mkdir -p "$STATE_DIR" 2>/dev/null || true
    local tmp="$AUDIT_JSON.tmp.$$"
    {
        printf '{\n'
        printf '  "verdict": "%s",\n' "$(json_escape "$VERDICT")"
        printf '  "checked_at": "%s",\n' "$(now_iso)"
        printf '  "evidence": %s,\n' "$(json_array "${EVIDENCE[@]}")"
        printf '  "ssh_host_key_fingerprints": %s,\n' "$(json_array "${fps[@]+"${fps[@]}"}")"
        printf '  "tls_cert_sha256": "%s"\n' "$(json_escape "$tls_fp")"
        printf '}\n'
    } > "$tmp"
    chmod 0644 "$tmp"
    mv -f "$tmp" "$AUDIT_JSON"
}

human_line() {
    case "$VERDICT" in
        per-node)
            say "host-secrets: per-node — this node's SSH host keys and TLS key were generated here." ;;
        shared)
            say "host-secrets: SHARED — this node is running image-baked key material that every downloader of its ISO also holds. Rotate it: host-secrets-audit.sh --apply --yes" ;;
        fail-closed-missing)
            say "host-secrets: fail-closed-missing — key material is absent. Generation never succeeded; this node is not serving on a shared key, it is not serving." ;;
        *)
            say "host-secrets: unknown — not enough on-disk evidence to date this node's first boot." ;;
    esac
}

# ── Rotation ────────────────────────────────────────────────────────────────

# Same pair check as both other producers. Parsing each half back proves each
# is well-formed; it does NOT prove they belong together, and a key from one
# generation beside a cert from another passes both individual checks and then
# breaks nginx.
tls_pair_matches() {
    local key="$1" crt="$2" kp cp
    kp=$(openssl pkey -in "$key" -pubout 2>/dev/null) || return 1
    cp=$(openssl x509 -in "$crt" -noout -pubkey 2>/dev/null) || return 1
    [ -n "$kp" ] || return 1
    [ "$kp" = "$cp" ]
}

TLS_STAGE_KEY="$SSL_DIR/archipelago.key.rotnew"
TLS_STAGE_CRT="$SSL_DIR/archipelago.crt.rotnew"
SSH_STAGE_DIR=""

cleanup_staging() {
    rm -f "$TLS_STAGE_KEY" "$TLS_STAGE_CRT" 2>/dev/null || true
    [ -n "$SSH_STAGE_DIR" ] && rm -rf "$SSH_STAGE_DIR" 2>/dev/null || true
}

# STAGE ONLY. Touches nothing live. Parameters kept identical to the other two
# producers — see the header. Do not let rsa:2048/3650 drift here alone.
stage_tls() {
    local node_name
    node_name=$(hostname 2>/dev/null || echo archipelago)
    mkdir -p "$SSL_DIR" || return 1
    rm -f "$TLS_STAGE_KEY" "$TLS_STAGE_CRT"
    openssl req -x509 -nodes -days 3650 -newkey rsa:2048 \
        -keyout "$TLS_STAGE_KEY" -out "$TLS_STAGE_CRT" \
        -subj "/C=XX/ST=Bitcoin/L=Node/O=Archipelago/CN=${node_name}" \
        -addext "subjectAltName=DNS:${node_name},DNS:${node_name}.local,DNS:archipelago,DNS:archipelago.local,DNS:localhost,IP:127.0.0.1" \
        >/dev/null 2>&1 || return 1
    [ -s "$TLS_STAGE_KEY" ] && [ -s "$TLS_STAGE_CRT" ] || return 1
    tls_pair_matches "$TLS_STAGE_KEY" "$TLS_STAGE_CRT" || return 1
    chmod 600 "$TLS_STAGE_KEY"
    if getent passwd archipelago >/dev/null 2>&1; then
        chgrp "$(id -gn archipelago)" "$TLS_STAGE_KEY" || return 1
        chmod 640 "$TLS_STAGE_KEY" || return 1
    fi
    return 0
}

stage_ssh() {
    SSH_STAGE_DIR=$(mktemp -d) || return 1
    mkdir -p "$SSH_STAGE_DIR/etc/ssh"
    ssh-keygen -A -f "$SSH_STAGE_DIR" >/dev/null 2>&1 || return 1
    ls "$SSH_STAGE_DIR"/etc/ssh/ssh_host_*_key >/dev/null 2>&1 || return 1
    return 0
}

swap_tls() {
    mv -f "$TLS_STAGE_KEY" "$TLS_KEY" || return 1
    mv -f "$TLS_STAGE_CRT" "$TLS_CRT" || return 1
    # Preserve the staged daemon-readable mode through the atomic rename.
    return 0
}

# Overwrite in place rather than rm-then-mv. rm-then-mv opens a window — small,
# but real — in which the node has ZERO host keys on disk; sshd restarting into
# that window is unrecoverable on a remote machine. mv onto the existing path
# is a rename(2), so each key is replaced atomically and the directory is never
# empty. Only after every staged key has landed are leftovers of key types the
# new set does not include removed — leaving a stale ssh_host_dsa_key behind
# would leave shared material behind, which is the whole point of rotating.
swap_ssh() {
    local f base staged=()
    for f in "$SSH_STAGE_DIR"/etc/ssh/ssh_host_*; do
        [ -e "$f" ] || continue
        base=$(basename "$f")
        mv -f "$f" "$SSH_DIR/$base" || return 1
        staged+=("$base")
    done
    [ "${#staged[@]}" -gt 0 ] || return 1
    for f in "$SSH_DIR"/ssh_host_*; do
        [ -e "$f" ] || continue
        base=$(basename "$f")
        local keep=0 s
        for s in "${staged[@]}"; do [ "$s" = "$base" ] && keep=1; done
        [ "$keep" = 0 ] && rm -f "$f"
    done
    return 0
}

# reload, NEVER restart. THIS IS THE SINGLE MOST IMPORTANT LINE IN THIS FILE:
# a reload re-execs the sshd listener while already-forked session children
# keep running, so the operator's own SSH session survives its own rotation. A
# restart kills every session, and on a remote node reached only over SSH that
# is unrecoverable without physical console access.
reload_sshd() {
    systemctl reload ssh >/dev/null 2>&1 || systemctl reload sshd >/dev/null 2>&1 || true
}

reload_nginx() {
    systemctl reload nginx >/dev/null 2>&1 || true
}

shout() {
    echo "$*"
    [ -w "$CONSOLE" ] && printf '%s\n' "$*" > "$CONSOLE" 2>/dev/null || true
}

write_rotation_json() {
    # $1 = "pre" (old only) or "post" (old + new)
    local phase="$1"
    mkdir -p "$STATE_DIR" 2>/dev/null || true
    local tmp="$ROTATION_JSON.tmp.$$"
    {
        printf '{\n'
        printf '  "rotated_at": "%s",\n' "$(json_escape "$ROTATED_AT")"
        printf '  "old_ssh_fingerprints": %s,\n' "$(json_array "${OLD_SSH_FPS[@]+"${OLD_SSH_FPS[@]}"}")"
        if [ "$phase" = "pre" ]; then
            printf '  "old_tls_sha256": "%s"\n' "$(json_escape "$OLD_TLS_FP")"
        else
            printf '  "old_tls_sha256": "%s",\n' "$(json_escape "$OLD_TLS_FP")"
            printf '  "new_ssh_fingerprints": %s,\n' "$(json_array "${NEW_SSH_FPS[@]+"${NEW_SSH_FPS[@]}"}")"
            printf '  "new_tls_sha256": "%s"\n' "$(json_escape "$NEW_TLS_FP")"
        fi
        printf '}\n'
    } > "$tmp"
    chmod 0644 "$tmp"
    mv -f "$tmp" "$ROTATION_JSON"
}

ROTATED_AT=""
OLD_SSH_FPS=()
OLD_TLS_FP=""
NEW_SSH_FPS=()
NEW_TLS_FP=""

apply_rotation() {
    trap cleanup_staging EXIT

    # Step 1 — stage EVERYTHING first. If any generation fails we abort before
    # touching anything live and exit non-zero. A partial rotation is the
    # failure mode that loses access, so there is no path here in which one
    # class is swapped and the other has not been generated yet.
    if [ "$TLS_SHARED" = 1 ]; then
        if ! stage_tls; then
            echo "host-secrets: ABORTED — could not generate a replacement TLS keypair. Nothing was changed." >&2
            cleanup_staging
            return 1
        fi
        say "staged: replacement TLS keypair"
    fi
    if [ "$SSH_SHARED" = 1 ]; then
        if ! stage_ssh; then
            echo "host-secrets: ABORTED — could not generate a replacement SSH host-key set. Nothing was changed." >&2
            cleanup_staging
            return 1
        fi
        say "staged: replacement SSH host-key set"
    fi

    # Step 2 — record the OLD fingerprints BEFORE the swap. An operator who
    # loses access anyway can still identify what changed; after the swap the
    # old material is gone and unrecoverable.
    ROTATED_AT=$(now_iso)
    OLD_SSH_FPS=()
    while IFS= read -r line; do [ -n "$line" ] && OLD_SSH_FPS+=("$line"); done < <(ssh_fingerprints)
    OLD_TLS_FP=$(tls_fingerprint)
    write_rotation_json pre
    say "recorded old fingerprints to $(disp "$ROTATION_JSON") before touching anything"

    # Step 3 — TLS first. The web UI going down is recoverable over SSH; SSH
    # going down on a remote node is not. Do the recoverable one first.
    if [ "$TLS_SHARED" = 1 ]; then
        if ! swap_tls; then
            echo "host-secrets: TLS swap failed. SSH host keys were NOT touched." >&2
            cleanup_staging
            return 1
        fi
        reload_nginx
        say "rotated: TLS keypair, nginx reloaded"
    fi

    # Step 4 — SSH, then reload (never restart; see reload_sshd).
    if [ "$SSH_SHARED" = 1 ]; then
        if ! swap_ssh; then
            echo "host-secrets: SSH swap failed partway. Check $(disp "$SSH_DIR") before disconnecting." >&2
            cleanup_staging
            return 1
        fi
        reload_sshd
        say "rotated: SSH host keys, sshd reloaded (your current session is intentionally unaffected)"
    fi

    # Step 5 — new fingerprints on the record, on stdout and on the console,
    # then re-run detect so the verdict file reflects the post-rotation state.
    NEW_SSH_FPS=()
    while IFS= read -r line; do [ -n "$line" ] && NEW_SSH_FPS+=("$line"); done < <(ssh_fingerprints)
    NEW_TLS_FP=$(tls_fingerprint)
    write_rotation_json post

    shout "host-secrets: ROTATED $ROTATED_AT — new host key fingerprints for this node:"
    for line in "${NEW_SSH_FPS[@]+"${NEW_SSH_FPS[@]}"}"; do shout "  $line"; done
    [ -n "$NEW_TLS_FP" ] && shout "  TLS cert sha256: $NEW_TLS_FP"
    shout "host-secrets: every known_hosts entry for this node is now stale. Update it against the fingerprints above, never by blindly accepting whatever is offered."

    detect
    write_audit_json
    human_line

    cleanup_staging
    trap - EXIT
    return 0
}

# ── Main ────────────────────────────────────────────────────────────────────

detect

if [ "$MODE" = "detect" ]; then
    write_audit_json
    human_line
    [ "$EMIT_JSON" = 1 ] && cat "$AUDIT_JSON"
    exit 0
fi

# --apply. Deliberately writes NOTHING — not even its own verdict file — until
# --yes is given and a rotation actually starts. "Touches nothing" is a
# property worth being able to state without a footnote, and a footnote is what
# "except for one file it rewrites" would be.
if [ "$VERDICT" != "shared" ]; then
    human_line
    say "host-secrets: nothing to rotate (verdict is '$VERDICT', not 'shared'). No changes made."
    exit 0
fi

if [ "$CONFIRMED" != 1 ]; then
    say "host-secrets: DRY RUN — this node's verdict is 'shared'. Nothing has been changed."
    say ""
    say "Would rotate:"
    [ "$TLS_SHARED" = 1 ] && say "  - TLS keypair at $(disp "$TLS_KEY") (+ cert), then reload nginx"
    [ "$SSH_SHARED" = 1 ] && say "  - every $(disp "$SSH_DIR")/ssh_host_*_key, then reload (not restart) sshd"
    say ""
    say "Old fingerprints would be written to $(disp "$ROTATION_JSON") before the swap."
    say "This is ONE-WAY: every known_hosts entry for this node breaks and the old key is destroyed."
    say "Re-run with --yes from a session you are willing to lose."
    exit 0
fi

apply_rotation
