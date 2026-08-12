#!/bin/bash
#
# Create bootable FAT32 USB from Archipelago ISO
# This extracts the ISO contents to FAT32 for UEFI boot
#
# Usage: ./create-fat32-usb.sh /dev/diskN
#

set -e

# Get absolute path of script directory
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

if [ -z "$1" ]; then
    echo "Usage: $0 /dev/diskN"
    echo ""
    echo "Available disks:"
    diskutil list external
    exit 1
fi

USB_DISK="$1"
ISO_FILE="${ARCHIPELAGO_ISO:-}"
if [ -z "$ISO_FILE" ]; then
    ISO_FILE="$SCRIPT_DIR/results/archipelago-installer-x86_64.iso"
    [ -f "$ISO_FILE" ] || ISO_FILE="$SCRIPT_DIR/results/archipelago-installer-unbundled-x86_64.iso"
fi
WORK_DIR="$SCRIPT_DIR/build/usb-extract"

if [ ! -f "$ISO_FILE" ]; then
    echo "❌ ISO not found: $ISO_FILE"
    echo ""
    echo "Build the ISO first with: ./build-debian-iso.sh"
    echo "Or set ARCHIPELAGO_ISO=/path/to/archipelago-installer-x86_64.iso"
    exit 1
fi

echo "╔════════════════════════════════════════════════════════╗"
echo "║   Create FAT32 Bootable USB                            ║"
echo "╚════════════════════════════════════════════════════════╝"
echo ""
echo "⚠️  WARNING: This will COMPLETELY ERASE $USB_DISK"
echo ""
echo "📀 ISO: $(basename "$ISO_FILE")"
echo "💾 USB: $USB_DISK"
echo ""
echo "Press Ctrl+C to cancel, or Enter to continue..."
read

# Clean up work directory
rm -rf "$WORK_DIR"
mkdir -p "$WORK_DIR"

echo "🔓 Unmounting USB..."
diskutil unmountDisk "$USB_DISK" || true

echo ""
echo "🗂️  Formatting USB as FAT32 with MBR..."
diskutil eraseDisk FAT32 ARCHIPELAGO MBR "$USB_DISK"

echo ""
echo "📦 Extracting ISO contents..."
cd "$WORK_DIR"
7z x -y "$ISO_FILE"

echo ""
echo "📋 Copying files to USB (this may take a few minutes)..."
USB_MOUNT="/Volumes/ARCHIPELAGO"
if [ ! -d "$USB_MOUNT" ]; then
    echo "❌ USB not mounted at $USB_MOUNT"
    exit 1
fi

# Copy all extracted files
cp -Rv "$WORK_DIR"/* "$USB_MOUNT/" 2>/dev/null | tail -5
echo "   (showing last 5 files copied)"

echo ""
echo "🔄 Syncing..."
sync

echo ""
echo "✅ USB created successfully!"
echo ""
echo "Now:"
echo "  1. Eject: diskutil eject $USB_DISK"
echo "  2. Insert into target machine"
echo "  3. Boot from USB (F12 or similar for boot menu)"
echo ""
echo "Default login (live mode):"
echo "  Username: user"
echo "  Password: live"

# Cleanup
rm -rf "$WORK_DIR"
