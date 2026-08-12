#!/bin/bash
#
# Build Archipelago UNBUNDLED Auto-Installer ISO
#
# Same as build-auto-installer-iso.sh but WITHOUT pre-bundled container images.
# Users install all apps on-demand from the Marketplace (requires internet).
#
# Benefits:
#   - Much smaller ISO (~1-2GB vs ~8-10GB)
#   - Faster build (no image pulling/saving)
#   - Faster install (no image copying/loading)
#
# Trade-offs:
#   - Internet required after first boot to install apps
#   - No apps pre-loaded — everything comes from Marketplace
#
# Usage:
#   sudo ./build-unbundled-iso.sh
#   DEV_SERVER=archipelago@192.0.2.10 sudo ./build-unbundled-iso.sh
#   BUILD_FROM_SOURCE=1 sudo ./build-unbundled-iso.sh
#

set -e

# Configuration
DEV_SERVER="${DEV_SERVER:-archipelago@192.0.2.10}"
BUILD_FROM_SOURCE="${BUILD_FROM_SOURCE:-0}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Delegate to the main build script with UNBUNDLED mode
export UNBUNDLED=1
exec "$SCRIPT_DIR/build-auto-installer-iso.sh" "$@"
