#!/bin/bash
# Test Archipelago ISO in QEMU
#
# Usage:
#   ./test-iso-qemu.sh [path-to-iso] [--bios] [--nographic]
#
# Options:
#   --bios       Force legacy BIOS mode (default: UEFI)
#   --nographic  No GUI window, serial console only (great for logging)
#
# Serial log and the 20G test disk are written under ${TMPDIR:-/tmp} — set
# TMPDIR to a disk-backed directory if /tmp is space-constrained (both were
# previously hardcoded to /tmp, which could ENOSPC a tmpfs-backed /tmp on a
# 20G sparse qcow2 regardless of what the caller set TMPDIR to).

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
QEMU_TMPDIR="${TMPDIR:-/tmp}"
SSH_FORWARD_PORT="${QEMU_SSH_PORT:-2222}"
HTTP_FORWARD_PORT="${QEMU_HTTP_PORT:-8100}"
SERIAL_LOG="$QEMU_TMPDIR/archipelago-qemu-serial.log"
FORCE_BIOS=false
NOGRAPHIC=false
TIMEOUT=0
ISO=""

# Simple arg parsing. First non-flag positional is the ISO path. A bare
# numeric (e.g. `120`) is taken as a boot-test timeout in seconds so CI
# can call `test-iso-qemu.sh <iso> 120` without hanging the job. The
# pre-fix version used `case *) ISO=...`, which silently overwrote ISO
# with the timeout value and sent QEMU looking for a file literally
# named "120".
for arg in "$@"; do
    case "$arg" in
        --bios) FORCE_BIOS=true ;;
        --nographic) NOGRAPHIC=true ;;
        --timeout=*) TIMEOUT="${arg#--timeout=}" ;;
        [0-9]*) TIMEOUT="$arg" ;;
        *) ISO="$arg" ;;
    esac
done

# A positive TIMEOUT implies headless (no DISPLAY in CI anyway) and keeps
# the entire script wrapped in `timeout` to guarantee the job returns.
if [ "$TIMEOUT" -gt 0 ] 2>/dev/null; then
    NOGRAPHIC=true
fi

# Auto-detect ISO
if [ -z "$ISO" ]; then
    ISO=$(ls -t "$SCRIPT_DIR"/results/archipelago-installer-unbundled-*.iso 2>/dev/null | head -1)
fi
if [ -z "$ISO" ] || [ ! -f "$ISO" ]; then
    ISO=$(ls -t "$SCRIPT_DIR"/results/archipelago-*.iso 2>/dev/null | head -1)
fi
if [ -z "$ISO" ] || [ ! -f "$ISO" ]; then
    echo "ISO not found."
    echo ""
    echo "Usage: $0 [path-to-iso] [--bios] [--nographic]"
    echo ""
    echo "Or place an ISO in: $SCRIPT_DIR/results/"
    exit 1
fi

echo "Testing Archipelago ISO in QEMU"
echo "  ISO:    $ISO"
echo "  Size:   $(du -h "$ISO" | cut -f1)"
echo "  RAM:    4GB"
echo "  CPU:    2 cores"
echo "  Serial: $SERIAL_LOG"
echo ""

# Never accept boot markers left by an earlier VM.
mkdir -p "$QEMU_TMPDIR"
: > "$SERIAL_LOG"

# Create test disk if it doesn't exist
DISK="$QEMU_TMPDIR/archipelago-test-disk.qcow2"
if [ ! -f "$DISK" ]; then
    echo "Creating 20GB test disk..."
    qemu-img create -f qcow2 "$DISK" 20G
fi

# Common QEMU args
QEMU_ARGS=(
    -m 4G
    -smp 2
    -boot d
    -cdrom "$ISO"
    -drive if=virtio,format=qcow2,file="$DISK"
    -net nic,model=virtio -net "user,hostfwd=tcp:127.0.0.1:${SSH_FORWARD_PORT}-:22,hostfwd=tcp:127.0.0.1:${HTTP_FORWARD_PORT}-:80"
    -serial file:"$SERIAL_LOG"
    -qmp "unix:$QEMU_TMPDIR/archipelago-qmp.sock,server=on,wait=off"
)

# Display mode
if [ "$NOGRAPHIC" = true ]; then
    QEMU_ARGS+=(-display none)
else
    QEMU_ARGS+=(-vga virtio -display default)
fi

echo "Starting VM..."
echo "(Serial console logging to $SERIAL_LOG)"
echo "(Press Ctrl+Alt+G to release mouse, Ctrl+C to stop VM)"
echo ""

# Detect UEFI firmware
OVMF=""
OVMF_VARS=""
if [ "$FORCE_BIOS" = false ]; then
    if [ -f "/opt/homebrew/share/qemu/edk2-x86_64-code.fd" ]; then
        OVMF="/opt/homebrew/share/qemu/edk2-x86_64-code.fd"
    elif [ -f "/usr/share/OVMF/OVMF_CODE.fd" ]; then
        OVMF="/usr/share/OVMF/OVMF_CODE.fd"
        OVMF_VARS="/usr/share/OVMF/OVMF_VARS.fd"
    elif [ -f "/usr/share/OVMF/OVMF_CODE_4M.fd" ]; then
        OVMF="/usr/share/OVMF/OVMF_CODE_4M.fd"
        OVMF_VARS="/usr/share/OVMF/OVMF_VARS_4M.fd"
    fi
fi

QEMU_COMMAND=(qemu-system-x86_64)
if [ -r /dev/kvm ] && [ -w /dev/kvm ]; then
    QEMU_COMMAND+=(-enable-kvm)
fi
if [ -n "$OVMF" ]; then
    echo "  Boot: UEFI ($OVMF)"
    QEMU_COMMAND+=(-machine q35 -drive "if=pflash,format=raw,readonly=on,file=$OVMF")
    if [ -f "$OVMF_VARS" ]; then
        if [ ! -f "$QEMU_TMPDIR/archipelago-uefi-vars.fd" ]; then
            cp "$OVMF_VARS" "$QEMU_TMPDIR/archipelago-uefi-vars.fd" || exit 1
        fi
        QEMU_COMMAND+=(-drive "if=pflash,format=raw,file=$QEMU_TMPDIR/archipelago-uefi-vars.fd")
    fi
else
    echo "  Boot: Legacy BIOS"
    QEMU_COMMAND+=(-machine pc)
fi
QEMU_COMMAND+=("${QEMU_ARGS[@]}")

# Wrap the QEMU invocation in `timeout` when a CI caller passed one so
# the script always returns instead of hanging on a VM that never exits
# its boot loop. Exit 124 from coreutils' timeout is treated as "VM
# reached the timeout", which for a CI boot test is success as long as
# the serial log shows a kernel reaching userspace — we inspect that
# after the QEMU process ends.
if [ "$TIMEOUT" -gt 0 ] 2>/dev/null; then
    # A new bash -c loses the unexportable argument array and firmware path.
    # Invoke the complete command directly and keep timeout's distinct status.
    timeout --foreground --kill-after=10 "${TIMEOUT}s" "${QEMU_COMMAND[@]}"
    rc=$?
    if [ $rc -eq 124 ]; then
        echo "(QEMU terminated after ${TIMEOUT}s boot-test window)"
        rc=0
    fi
else
    "${QEMU_COMMAND[@]}"
    rc=$?
fi

echo ""
echo "VM stopped. Serial log: $SERIAL_LOG"
echo "Last 20 lines:"
tail -20 "$SERIAL_LOG" 2>/dev/null

# Boot-sanity check: the CI wrapper wants a non-zero exit only when the
# kernel never reached userspace. Look for a well-known marker emitted
# by live-boot/systemd early in the sequence. If the marker never
# appeared, surface the real failure; otherwise treat "timeout reached
# with a live kernel" as a pass.
if [ "${rc:-0}" -ne 0 ]; then
    exit "$rc"
fi
if [ "$TIMEOUT" -gt 0 ] 2>/dev/null && [ -f "$SERIAL_LOG" ]; then
    if grep -qE 'Welcome to Debian|Reached target|systemd\[1\]:|Debian GNU/Linux [0-9]+ archipelago-installer ttyS0' "$SERIAL_LOG"; then
        echo "  Boot sanity: OK (userspace reached in serial log; installation not yet tested)"
        exit 0
    fi
    echo "  Boot sanity: FAIL — no userspace markers in serial log within ${TIMEOUT}s"
    exit 1
fi
exit "${rc:-0}"
