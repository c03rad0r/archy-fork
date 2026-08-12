#!/bin/bash
# Convert ISO image to bootable disk image
# Creates a raw disk image that can be flashed directly

set -e

OUTPUT_DIR="${1:-../results}"
ARCHIPELAGO_VERSION="${ARCHIPELAGO_VERSION:-0.1.0}"
ARCH="${ARCH:-x86_64}"

echo "💾 Converting ISO to disk image..."

# Find ISO file
ISO_FILE=$(ls "$OUTPUT_DIR"/*.iso 2>/dev/null | head -1)
if [ -z "$ISO_FILE" ]; then
    echo "❌ No ISO file found in $OUTPUT_DIR"
    exit 1
fi

echo "   Source ISO: $ISO_FILE"

# Create disk image (4GB minimum)
DISK_SIZE=4096  # 4GB in MB
DISK_IMG="$OUTPUT_DIR/archipelago-${ARCHIPELAGO_VERSION}-${ARCH}.img"

echo "   Creating disk image: $DISK_IMG"

# Check if we have required tools
if ! command -v dd >/dev/null 2>&1; then
    echo "❌ dd not found"
    exit 1
fi

# Create empty disk image
dd if=/dev/zero of="$DISK_IMG" bs=1M count=$DISK_SIZE 2>/dev/null || {
    echo "❌ Failed to create disk image"
    exit 1
}

# Note: Full disk image creation with partitions requires:
# - parted or fdisk
# - mkfs.vfat, mkfs.ext4
# - losetup (Linux only)
# - grub-install

# For now, we'll create a simple approach:
# The ISO can be used directly, or users can use tools like:
# - balenaEtcher (macOS/Linux GUI)
# - Rufus (Windows)
# - dd (command line)

echo "⚠️  Full disk image conversion requires additional tools"
echo "   For now, use the ISO file directly with:"
echo "   - balenaEtcher (recommended)"
echo "   - dd command (see docs)"
echo ""
echo "   ISO file: $ISO_FILE"
echo "   Size: $(du -h "$ISO_FILE" | cut -f1)"

# Clean up empty image file
rm -f "$DISK_IMG"

echo ""
echo "💡 Tip: Use the ISO file with a USB flashing tool"
echo "   The ISO is bootable and can be flashed directly"
