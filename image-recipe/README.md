# Archipelago OS Image Recipes

Build scripts for creating bootable Debian Linux OS images for Archipelago Bitcoin Node OS.

## Quick Start

### Build the ISO

```bash
# 1. Sync latest configs from live dev server

# 2. Build components
./scripts/build-backend.sh
./scripts/build-frontend.sh

# 3. Build the ISO
./build-debian-iso.sh
```

This creates a bootable Debian Live ISO with Archipelago pre-installed.

### Write to USB

```bash
# Using dd (recommended)
./write-usb-dd.sh /dev/diskN

# Or use Balena Etcher to flash the ISO
```

See the **ISO-BUILD-CHECKLIST.md** for a comprehensive build workflow.

See the Architecture documentation for detailed system information.

## What's Included

- **Debian Linux Base**: Debian 13 (Trixie) with security updates applied during ISO/install creation
- **Podman**: Container runtime for apps (rootless by default)
- **Archipelago Backend**: Rust-based API server
- **Archipelago Frontend**: Vue.js web interface
- **Systemd Services**: Automatic service management
- **Network Configuration**: NetworkManager for easy setup

## Build Output

- `results/archipelago-installer-x86_64.iso` - Bootable hybrid ISO image

## Supported Platforms

- **x86_64**: Dell OptiPlex, HP ProDesk 400 G4 DM, Start9 Server Pure, and other x86_64 machines
- **Build Systems**: macOS (requires Docker) and Linux (native or Docker)

## Installation Methods

### 1. Live USB Boot
Boot from USB, run in live mode to test, or install to disk.

### 2. Full Disk Installation
From the live environment, run:
```bash
sudo /archipelago/install-to-disk.sh
```

This installs Archipelago to a target disk using debootstrap.

## Directory Structure

```
image-recipe/
├── build-debian-iso.sh          # Main ISO builder
├── write-usb-dd.sh              # Write ISO to USB with dd
├── create-fat32-usb.sh          # Alternative USB creation
├── archipelago-scripts/         # Scripts included in ISO
│   ├── install-to-disk.sh       # Disk installer
│   └── setup-bitcoin.sh         # Bitcoin Core setup
├── scripts/                     # Build helper scripts
│   ├── build-backend.sh         # Compile Rust backend
│   ├── build-frontend.sh        # Build Vue.js frontend
│   └── check-dependencies.sh    # Verify build requirements
└── results/                     # Built ISO output
```

## Requirements

- Docker (for macOS builds)
- xorriso (for ISO creation): `brew install xorriso`
- 7zip (for ISO extraction): `brew install p7zip`
