#!/bin/bash
#
# Boot branding dev — iterate on GRUB theme, Plymouth, and installer visuals
# without rebuilding the ISO. Patches an existing ISO and boots in QEMU.
#
# Usage:
#   ./dev-branding.sh [path-to-iso]
#
# If no ISO is found locally, downloads the latest from the build server.
# Edit files in branding/, re-run, see changes in ~10 seconds.
#

set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORK="/tmp/archipelago-dev-branding"
PATCHED="$SCRIPT_DIR/results/archipelago-dev-patched.iso"
CACHED_ISO="$SCRIPT_DIR/results/archipelago-dev-base.iso"
DEV_SERVER="${DEV_SERVER:?DEV_SERVER must be set, e.g. archipelago@<node-host>}"
SSH_KEY="$HOME/.ssh/archipelago-deploy"

echo ""
echo "  Archipelago Boot Branding Dev"
echo ""

# --- Find or download an ISO ---
ISO="${1:-}"

# Search locally
if [ -z "$ISO" ] || [ ! -f "$ISO" ]; then
    for pattern in \
        "$HOME/Desktop/archipelago-dev-"*.iso \
        "$HOME/Desktop/archipelago-unbundled-"*.iso \
        "$HOME/Desktop/archipelago-"*.iso \
        "$SCRIPT_DIR/results/archipelago-dev-base.iso" \
        "$SCRIPT_DIR/results/archipelago-"*.iso; do
        found=$(ls -t $pattern 2>/dev/null | head -1)
        if [ -n "$found" ] && [ -f "$found" ]; then
            ISO="$found"
            break
        fi
    done
fi

# Download from server if not found
if [ -z "$ISO" ] || [ ! -f "$ISO" ]; then
    echo "  No ISO found locally. Downloading latest from build server..."
    REMOTE_ISO=$(ssh -i "$SSH_KEY" "$DEV_SERVER" \
        "ls -t /var/lib/archipelago/filebrowser/Builds/archipelago-dev-*.iso 2>/dev/null | head -1" 2>/dev/null)
    if [ -z "$REMOTE_ISO" ]; then
        REMOTE_ISO=$(ssh -i "$SSH_KEY" "$DEV_SERVER" \
            "ls -t /var/lib/archipelago/filebrowser/Builds/archipelago-unbundled-*.iso 2>/dev/null | head -1" 2>/dev/null)
    fi
    if [ -n "$REMOTE_ISO" ]; then
        mkdir -p "$SCRIPT_DIR/results"
        echo "  Downloading: $(basename "$REMOTE_ISO")..."
        scp -i "$SSH_KEY" "$DEV_SERVER:$REMOTE_ISO" "$CACHED_ISO"
        ISO="$CACHED_ISO"
        echo "  Saved to: $ISO"
    else
        echo "  No ISO on server either. Run a CI build first."
        echo "  Or place an ISO on your Desktop."
        exit 1
    fi
fi

echo "  Base ISO: $(basename "$ISO") ($(du -h "$ISO" | cut -f1))"
echo ""

# --- Extract ISO ---
echo "  [1/3] Extracting ISO..."
if [ -d "$WORK" ]; then
    chmod -R u+w "$WORK" 2>/dev/null || true
fi
rm -rf "$WORK"
mkdir -p "$WORK"

xorriso -osirrox on -indev "$ISO" -extract / "$WORK" 2>/dev/null || {
    echo "  xorriso extraction failed, trying hdiutil..."
    MNT=$(mktemp -d)
    hdiutil attach "$ISO" -mountpoint "$MNT" -readonly -nobrowse 2>/dev/null || {
        echo "  Could not mount ISO. Is it corrupt?"
        exit 1
    }
    cp -a "$MNT"/* "$WORK/" 2>/dev/null || true
    hdiutil detach "$MNT" 2>/dev/null || true
    rmdir "$MNT" 2>/dev/null || true
}
# Ensure files are writable after extraction
chmod -R u+w "$WORK" 2>/dev/null || true

# --- Patch branding ---
echo "  [2/3] Patching branding..."
THEME_DST="$WORK/boot/grub/themes/archipelago"
mkdir -p "$THEME_DST"

# GRUB theme.txt
if [ -f "$SCRIPT_DIR/branding/grub-theme/theme.txt" ]; then
    cp "$SCRIPT_DIR/branding/grub-theme/theme.txt" "$THEME_DST/"
    echo "    theme.txt"
fi

# GRUB background — use static file from branding dir
if [ -f "$SCRIPT_DIR/branding/grub-theme/background.png" ]; then
    cp "$SCRIPT_DIR/branding/grub-theme/background.png" "$THEME_DST/background.png"
    echo "    background.png (static)"
elif [ -f "$SCRIPT_DIR/branding/generate-grub-background.py" ]; then
    python3 "$SCRIPT_DIR/branding/generate-grub-background.py" "$THEME_DST/background.png" 2>/dev/null
    echo "    background.png (generated)"
fi

# Plymouth theme
PLYMOUTH_DST="$WORK/archipelago/plymouth-theme"
mkdir -p "$PLYMOUTH_DST"
if [ -d "$SCRIPT_DIR/branding/plymouth-theme" ]; then
    cp "$SCRIPT_DIR/branding/plymouth-theme/"* "$PLYMOUTH_DST/" 2>/dev/null || true
    echo "    plymouth theme"
fi

# --- Repackage ISO ---
echo "  [3/3] Repackaging ISO..."
mkdir -p "$SCRIPT_DIR/results"

# Find isohdpfx.bin — project copy first, then system
ISOHDPFX=""
for p in "$SCRIPT_DIR/branding/isohdpfx.bin" \
         "$WORK/isolinux/isohdpfx.bin" \
         /usr/lib/ISOLINUX/isohdpfx.bin \
         /usr/share/syslinux/isohdpfx.bin \
         /opt/homebrew/share/syslinux/isohdpfx.bin; do
    [ -f "$p" ] && ISOHDPFX="$p" && break
done

if [ -z "$ISOHDPFX" ]; then
    echo "  ERROR: No isohdpfx.bin found. Cannot create bootable ISO."
    echo "  Preview only — open the background:"
    open "$THEME_DST/background.png" 2>/dev/null || true
    exit 1
fi

EFI_IMG="$WORK/boot/grub/efi.img"
if [ -f "$EFI_IMG" ]; then
    xorriso -as mkisofs -o "$PATCHED" \
        -volid "ARCHIPELAGO" \
        -iso-level 3 -J -joliet-long -R \
        -isohybrid-mbr "$ISOHDPFX" \
        -c isolinux/boot.cat \
        -b isolinux/isolinux.bin \
        -no-emul-boot -boot-load-size 4 -boot-info-table \
        -eltorito-alt-boot \
        -e boot/grub/efi.img \
        -no-emul-boot -isohybrid-gpt-basdat \
        -partition_offset 16 \
        "$WORK" 2>/dev/null
else
    xorriso -as mkisofs -o "$PATCHED" \
        -volid "ARCHIPELAGO" \
        -iso-level 3 -J -joliet-long -R \
        -isohybrid-mbr "$ISOHDPFX" \
        -c isolinux/boot.cat \
        -b isolinux/isolinux.bin \
        -no-emul-boot -boot-load-size 4 -boot-info-table \
        -partition_offset 16 \
        "$WORK" 2>/dev/null
fi

echo ""
echo "  Patched: $PATCHED ($(du -h "$PATCHED" | cut -f1))"
echo ""

# --- Boot in QEMU ---
if ! command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo "  QEMU not found. Install: brew install qemu"
    echo "  Opening background preview instead..."
    open "$THEME_DST/background.png" 2>/dev/null || true
    exit 0
fi

echo "  Booting in QEMU (BIOS mode — shows ISOLINUX menu)..."
echo "  Press Ctrl+C to stop."
echo ""

# Create test disk (use separate disk from other QEMU instances)
DISK="/tmp/archipelago-branding-test.qcow2"
# Kill any leftover QEMU from previous branding test
pkill -f "archipelago-branding-test" 2>/dev/null || true
sleep 1
if [ ! -f "$DISK" ]; then
    qemu-img create -f qcow2 "$DISK" 20G 2>/dev/null
fi

# Boot with BIOS to see the ISOLINUX/GRUB menu
qemu-system-x86_64 \
    -machine pc \
    -m 4G \
    -smp 2 \
    -boot d \
    -cdrom "$PATCHED" \
    -drive if=virtio,format=qcow2,file="$DISK" \
    -net nic,model=virtio -net user,hostfwd=tcp::2222-:22,hostfwd=tcp::8100-:80 \
    -vga virtio \
    -display default \
    -serial file:/tmp/archipelago-qemu-serial.log

echo ""
echo "  QEMU stopped. Serial log: /tmp/archipelago-qemu-serial.log"
echo "  Re-run to test again after editing branding files."
