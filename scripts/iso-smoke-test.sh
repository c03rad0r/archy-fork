#!/usr/bin/env bash
# Mount-level smoke test for an Archipelago installer ISO.
#
# Verifies boot plumbing (BIOS + UEFI + live-boot), the auto-installer
# payload, and — the check that has bitten before — that the backend
# binary inside the ISO actually embeds the version the filename claims.
#
# Usage:
#   scripts/iso-smoke-test.sh <path-to-iso> [expected-version]
#
# expected-version defaults to core/archipelago/Cargo.toml. Needs sudo
# (loop mount). Exits non-zero on the first hard failure; prints a
# PASS/FAIL table either way.

set -u

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ISO="${1:-}"
EXPECTED_VERSION="${2:-$(grep -m1 '^version' "$REPO/core/archipelago/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')}"

if [ -z "$ISO" ] || [ ! -f "$ISO" ]; then
    echo "usage: $0 <path-to-iso> [expected-version]" >&2
    exit 2
fi

FAIL=0
ok()   { echo "  OK:      $*"; }
bad()  { echo "  FAIL:    $*"; FAIL=1; }
warn() { echo "  WARN:    $*"; }

echo "ISO smoke test"
echo "  ISO:     $ISO ($(du -h "$ISO" | cut -f1))"
echo "  Version: $EXPECTED_VERSION (expected)"

# ── Filename ↔ version parity (gap: ISO version can silently drift) ──
case "$(basename "$ISO")" in
    *"$EXPECTED_VERSION"*) ok "filename contains $EXPECTED_VERSION" ;;
    *) bad "filename does not contain expected version $EXPECTED_VERSION" ;;
esac

MNT="$(mktemp -d)"
INITRD_DIR=""
cleanup() {
    sudo umount "$MNT" 2>/dev/null || true
    rmdir "$MNT" 2>/dev/null || true
    [ -n "$INITRD_DIR" ] && sudo rm -rf "$INITRD_DIR" 2>/dev/null
}
trap cleanup EXIT

if ! sudo mount -o loop,ro "$ISO" "$MNT"; then
    echo "  FAIL: could not loop-mount ISO" >&2
    exit 1
fi

# ── Required boot + installer files ──────────────────────────────────
for f in live/vmlinuz live/initrd.img live/filesystem.squashfs \
         isolinux/isolinux.bin isolinux/isolinux.cfg \
         boot/grub/grub.cfg EFI/BOOT/BOOTX64.EFI \
         archipelago/auto-install.sh archipelago/rootfs.tar; do
    if [ -e "$MNT/$f" ]; then
        ok "$f ($(sudo du -h "$MNT/$f" 2>/dev/null | cut -f1))"
    else
        bad "missing $f"
    fi
done

# ── GRUB must boot the live system ───────────────────────────────────
if grep -q "boot=live" "$MNT/boot/grub/grub.cfg" 2>/dev/null; then
    ok "grub.cfg has boot=live"
else
    bad "grub.cfg missing boot=live"
fi

# ── initrd must contain live-boot scripts ────────────────────────────
if command -v unmkinitramfs >/dev/null 2>&1; then
    INITRD_DIR="$(mktemp -d)"
    sudo unmkinitramfs "$MNT/live/initrd.img" "$INITRD_DIR" 2>/dev/null
    if [ -e "$INITRD_DIR/scripts/live" ] || [ -e "$INITRD_DIR/main/scripts/live" ]; then
        ok "initrd has live-boot scripts"
    else
        bad "initrd missing live-boot scripts"
    fi
else
    warn "unmkinitramfs not installed — skipping initrd live-boot check"
fi

# ── Backend binary inside the ISO embeds the expected version ────────
# (the v1.4.0-binary-in-a-v1.5-ISO incident: a stale captured binary
#  shipped and the fleet rejected its fips.yaml on Activate)
BIN_IN_ISO=""
if [ -f "$MNT/archipelago/bin/archipelago" ]; then
    BIN_IN_ISO="$MNT/archipelago/bin/archipelago"
    if sudo strings "$BIN_IN_ISO" 2>/dev/null | grep -qF "$EXPECTED_VERSION"; then
        ok "payload backend binary embeds $EXPECTED_VERSION"
    else
        bad "payload backend binary does NOT embed $EXPECTED_VERSION (stale binary)"
    fi
else
    # Fall back to the copy inside rootfs.tar
    TMPBIN="$(mktemp -d)"
    if sudo tar -xf "$MNT/archipelago/rootfs.tar" -C "$TMPBIN" \
            usr/local/bin/archipelago 2>/dev/null; then
        if sudo strings "$TMPBIN/usr/local/bin/archipelago" | grep -qF "$EXPECTED_VERSION"; then
            ok "rootfs backend binary embeds $EXPECTED_VERSION"
        else
            bad "rootfs backend binary does NOT embed $EXPECTED_VERSION (stale binary)"
        fi
    else
        bad "no backend binary found at archipelago/bin/ or in rootfs.tar"
    fi
    sudo rm -rf "$TMPBIN"
fi

# ── Frontend payload present ─────────────────────────────────────────
if [ -f "$MNT/archipelago/web-ui/index.html" ]; then
    ok "frontend payload (archipelago/web-ui/index.html)"
    if [ -f "$MNT/archipelago/web-ui/aiui/index.html" ]; then
        ok "AIUI included in frontend payload"
    else
        warn "AIUI missing from archipelago/web-ui (verify rootfs copy before shipping)"
    fi
else
    warn "no archipelago/web-ui payload on ISO (frontend may live in rootfs.tar only)"
fi

echo
if [ "$FAIL" = "1" ]; then
    echo "ISO SMOKE TEST: FAILED"
    exit 1
fi
echo "ISO SMOKE TEST: PASSED"
