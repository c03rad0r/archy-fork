#!/bin/bash
#
# Write Archipelago ISO to USB using dd
#
# Usage: ./write-usb-dd.sh /dev/diskN
#

set -e

if [ -z "$1" ]; then
    echo "Usage: $0 /dev/diskN"
    echo ""
    echo "Available disks:"
    diskutil list external
    exit 1
fi

USB_DISK="$1"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ISO_FILE="${ARCHIPELAGO_ISO:-}"
if [ -z "$ISO_FILE" ]; then
    ISO_FILE="$SCRIPT_DIR/results/archipelago-installer-x86_64.iso"
    [ -f "$ISO_FILE" ] || ISO_FILE="$SCRIPT_DIR/results/archipelago-installer-unbundled-x86_64.iso"
fi

if [ ! -f "$ISO_FILE" ]; then
    echo "❌ ISO not found: $ISO_FILE"
    echo ""
    echo "Build the ISO first with: ./build-debian-iso.sh"
    echo "Or set ARCHIPELAGO_ISO=/path/to/archipelago-installer-x86_64.iso"
    exit 1
fi

# Get raw disk for faster writes
RAW_DISK="${USB_DISK/disk/rdisk}"

echo "╔════════════════════════════════════════════════════════╗"
echo "║   Write Archipelago ISO to USB                         ║"
echo "╚════════════════════════════════════════════════════════╝"
echo ""
echo "⚠️  WARNING: This will COMPLETELY ERASE $USB_DISK"
echo ""
echo "📀 ISO: $(basename "$ISO_FILE")"
echo "💾 USB: $USB_DISK (raw: $RAW_DISK)"
echo ""
echo "Press Ctrl+C to cancel, or Enter to continue..."
read

echo "🔓 Unmounting USB..."
diskutil unmountDisk "$USB_DISK" || true

echo ""
echo "📝 Writing ISO with dd (this may take a few minutes)..."
echo "   Using raw disk $RAW_DISK for faster write..."
echo ""

# Use dd to write the ISO directly
sudo dd if="$ISO_FILE" of="$RAW_DISK" bs=4m status=progress

echo ""
echo "🔄 Syncing..."
sync

echo ""
echo "✅ Done! USB is ready."
echo ""
echo "Now:"
echo "  1. Eject the USB safely: diskutil eject $USB_DISK"
echo "  2. Insert into target machine"
echo "  3. Boot from USB (F12 or similar for boot menu)"
echo ""
echo "Default login (live mode):"
echo "  Username: user"
echo "  Password: live"
