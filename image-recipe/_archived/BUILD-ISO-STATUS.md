# Archipelago ISO Build - Quick Guide

## TL;DR - Build ISO with Live Server State

```bash
cd ~/archy/image-recipe
sudo bash build-auto-installer-iso.sh
```

The script will automatically:
1. Try to capture backend from `/usr/local/bin/archipelago`
2. Try to capture frontend from `/opt/archipelago/web-ui`
3. Fall back to building from source if capture fails

## Build Modes

### Default: Capture from Dev Server (RECOMMENDED)
```bash
# From your Mac (captures from remote dev server):
cd image-recipe
DEV_SERVER=archipelago@192.0.2.10 sudo bash build-auto-installer-iso.sh

# From the dev server itself:
cd ~/archy/image-recipe
sudo bash build-auto-installer-iso.sh
```

### Alternative: Build from Source
```bash
BUILD_FROM_SOURCE=1 sudo bash build-auto-installer-iso.sh
```

## Known Issues & Workarounds

### Issue: Can't capture from localhost via SCP

**Problem**: When running on the server itself, `scp localhost:/path` doesn't work.

**Workaround**: Use direct file copy instead:
```bash
# Instead of building on the server, build from your Mac:
cd ~/Projects/archy/image-recipe
DEV_SERVER=archipelago@192.0.2.10 sudo bash build-auto-installer-iso.sh
```

### Issue: Podman registry not configured

**Problem**: Podman can't pull images because `/etc/containers/registries.conf` has no unqualified-search registries.

**Fix**:
```bash
ssh archipelago@192.0.2.10
sudo tee -a /etc/containers/registries.conf <<EOF
[registries.search]
registries = ['docker.io']
EOF
```

## Flash ISO to USB

```bash
cd ~/Projects/archy/image-recipe
./write-usb-dd.sh /dev/diskX
```

## What Gets Captured

From your dev server (192.0.2.10):
- ✅ Backend binary: `/usr/local/bin/archipelago` (6.2M)
- ✅ Frontend: `/opt/archipelago/web-ui` (~64M)
- ✅ Nginx config: `/etc/nginx/sites-available/default`
- ✅ Systemd service: `/etc/systemd/system/archipelago.service`
- ✅ App manifests: `~/archy/apps/`

## Current Status

**Latest Working ISO**: `archipelago-debian-13-x86_64.iso` (469M, built 18:28)
- This ISO was built earlier today
- Contains the auto-installer
- **Should be tested** - might already have your live server state

## Next Steps

1. **Flash the existing ISO** and test it on the Dell OptiPlex
2. **Fix the build script** to properly capture from localhost (use `cp` instead of `scp`)
3. **Configure Podman registries** on dev server for fallback source builds
