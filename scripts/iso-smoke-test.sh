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

# Verify the mounted artifact carries every manifest-declared build source.
if python3 "$REPO/scripts/check-app-build-contexts.py" "$MNT/archipelago"; then
    ok "app build contexts and Dockerfiles"
else
    bad "incomplete app build payload"
fi

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

# ── Host crash-capture payload (#144) ──────────────────────────────
# Check the built artifact, not just the ISO builder source: otherwise a stale
# rootfs.tar can silently omit the packages/configuration while the build passes.
ROOTFS="$MNT/archipelago/rootfs.tar"
if [ -f "$ROOTFS" ]; then
    DPKG_STATUS="$(sudo tar -xOf "$ROOTFS" var/lib/dpkg/status 2>/dev/null || true)"
    for package in kdump-tools kexec-tools makedumpfile rasdaemon; do
        if awk -v wanted="$package" 'BEGIN { RS=""; FS="\n" }
            $0 ~ "(^|\\n)Package: " wanted "(\\n|$)" &&
            $0 ~ "(^|\\n)Status: install ok installed(\\n|$)" { found=1 }
            END { exit !found }' <<<"$DPKG_STATUS"; then
            ok "rootfs package installed: $package"
        else
            bad "rootfs package missing/not installed: $package"
        fi
    done

    KDUMP_DEFAULTS="$(sudo tar -xOf "$ROOTFS" etc/default/kdump-tools 2>/dev/null || true)"
    if grep -qE '^USE_KDUMP=.?1' <<<"$KDUMP_DEFAULTS"; then
        ok "rootfs enables kdump"
    else
        bad "rootfs /etc/default/kdump-tools does not enable kdump"
    fi
    if grep -qE '^KDUMP_COREDIR=.?/var/crash' <<<"$KDUMP_DEFAULTS"; then
        ok "rootfs sends crash dumps to /var/crash"
    else
        bad "rootfs kdump target is not /var/crash"
    fi

    KDUMP_SYSCTL="$(sudo tar -xOf "$ROOTFS" etc/sysctl.d/99-archipelago-kdump.conf 2>/dev/null || true)"
    for setting in 'kernel.panic = 10' 'kernel.panic_on_oops = 1' \
                   'kernel.hung_task_panic = 1' 'kernel.hardlockup_panic = 1'; do
        if grep -Fqx "$setting" <<<"$KDUMP_SYSCTL"; then
            ok "rootfs sysctl: $setting"
        else
            bad "rootfs missing sysctl: $setting"
        fi
    done
else
    bad "cannot inspect crash-capture payload: missing archipelago/rootfs.tar"
fi

AUTO_INSTALL="$MNT/archipelago/auto-install.sh"
if grep -qF 'crashkernel=256M' "$AUTO_INSTALL" 2>/dev/null; then
    ok "installer writes crashkernel=256M"
else
    bad "installer does not write crashkernel=256M"
fi
if grep -qF 'Archipelago owns crashkernel sizing in /etc/default/grub' "$AUTO_INSTALL" 2>/dev/null; then
    ok "installer neutralizes the conflicting Debian kdump GRUB default"
else
    bad "installer does not neutralize the conflicting Debian kdump GRUB default"
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

# ── Frontend + companion payload ─────────────────────────────────────
WEB_UI="$MNT/archipelago/web-ui"
if [ -f "$WEB_UI/index.html" ]; then
    ok "frontend payload (archipelago/web-ui/index.html)"
    if [ -f "$WEB_UI/aiui/index.html" ]; then
        ok "AIUI included in frontend payload"
    else
        bad "AIUI missing from archipelago/web-ui"
    fi

    COMPANION_META="$WEB_UI/packages/archipelago-companion.json"
    COMPANION_APK="$WEB_UI/packages/archipelago-companion.apk"
    EXPECTED_COMPANION_NAME="$(sed -n 's/^[[:space:]]*versionName = "\(.*\)"/\1/p' "$REPO/Android/app/build.gradle.kts" | head -1)"
    EXPECTED_COMPANION_CODE="$(sed -n 's/^[[:space:]]*versionCode = \([0-9]*\).*/\1/p' "$REPO/Android/app/build.gradle.kts" | head -1)"
    if [ -s "$COMPANION_APK" ] && [ "$(jq -r '.versionName // empty' "$COMPANION_META" 2>/dev/null)" = "$EXPECTED_COMPANION_NAME" ] \
            && [ "$(jq -r '.versionCode // empty' "$COMPANION_META" 2>/dev/null)" = "$EXPECTED_COMPANION_CODE" ]; then
        ok "Companion $EXPECTED_COMPANION_NAME (versionCode $EXPECTED_COMPANION_CODE) APK included"
    else
        bad "Companion APK/metadata missing or stale (expected $EXPECTED_COMPANION_NAME/$EXPECTED_COMPANION_CODE)"
    fi

    SETTINGS_BUNDLE="$(grep -rlF "v$EXPECTED_VERSION" "$WEB_UI/assets" 2>/dev/null | head -1)"
    if [ -n "$SETTINGS_BUNDLE" ] \
            && grep -qF 'v1.8.0-alpha' "$SETTINGS_BUNDLE" \
            && ! grep -qE 'v1\.[0-7]\.|v1\.2\.0-alpha' "$SETTINGS_BUNDLE"; then
        ok "What's New is v1.8.6-first with a v1.8.0 history floor"
    else
        bad "What's New payload is missing v1.8 history or still contains pre-v1.8 entries"
    fi
else
    bad "no archipelago/web-ui payload on ISO"
fi

echo
if [ "$FAIL" = "1" ]; then
    echo "ISO SMOKE TEST: FAILED"
    exit 1
fi
echo "ISO SMOKE TEST: PASSED"
