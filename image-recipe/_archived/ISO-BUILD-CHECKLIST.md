# ISO Build Checklist

This checklist ensures that all changes from the live development server are properly integrated into the ISO build.

## Pre-Build Steps

### 1. Sync System Configurations from Live Server

```bash
cd image-recipe
```

This captures:
- [ ] Systemd service configuration (`archipelago.service`) - **User=root** required for Podman
- [ ] Nginx configuration (`nginx-archipelago.conf`) - includes app proxies (Nextcloud, Vaultwarden, Immich, Penpot)
- [ ] Logrotate configuration (if exists)
- [ ] Any custom scripts in `/opt/archipelago/scripts/`

**Critical**: `build-auto-installer-iso.sh` uses `configs/` for nginx and archipelago.service. Ensure these are synced before building.

### 2. Verify Code Changes

Ensure all code changes are committed:
- [ ] Backend changes in `core/`
- [ ] Frontend changes in `neode-ui/`
- [ ] Script changes in `scripts/`

### 3. Build Components

```bash
cd image-recipe

# Build backend
./scripts/build-backend.sh

# Build frontend  
./scripts/build-frontend.sh
```

Verify builds:
- [ ] Backend binary exists: `build/backend/archipelago`
- [ ] Frontend files exist: `build/frontend/index.html`

## Integration Check

### 4. Update Build Scripts

Review and update if needed:
- [ ] `integrate-archipelago.sh` - Includes all config files
- [ ] `build-debian-iso.sh` - Installs to correct paths

### 5. Critical Configuration Values

Verify in `configs/archipelago.service`:
- [ ] `User=root` (required for Podman root context)
- [ ] `Environment="ARCHIPELAGO_DEV_MODE=true"` (enables container detection)
- [ ] `Environment="ARCHIPELAGO_BIND=127.0.0.1:5678"`

Verify in `configs/nginx-archipelago.conf`:
- [ ] Root path: `/opt/archipelago/web-ui`
- [ ] RPC proxy: `/rpc/` → `http://127.0.0.1:5678`
- [ ] WebSocket proxy: `/ws` → `http://127.0.0.1:5678`

## Build Process

### 6. Build the ISO

```bash
./build-debian-iso.sh
```

Expected output:
- [ ] ISO created in `results/` directory
- [ ] No build errors
- [ ] File size reasonable (~500MB - 2GB)

### 7. Test in QEMU

```bash
./test-iso-qemu.sh
```

Test checklist:
- [ ] ISO boots successfully
- [ ] Backend service starts: `systemctl status archipelago`
- [ ] Nginx serves frontend
- [ ] Can access UI at `http://localhost:8080` (or mapped port)
- [ ] Container detection works: Check logs for "Detected container"

## App Stack Hardening (Immich, Penpot, etc.)

The first-boot script and deploy script ensure:
- [ ] **Immich**: Old single-container `immich` (wrong port) is removed before creating `immich_server` stack
- [ ] **First-boot**: Waits for postgres (pg_isready) before starting Immich server
- [ ] **Backend**: `package.install` for Immich removes old container before creating stack
- [ ] **Deploy**: Ensures Immich stack on every deploy, cleans up conflicts

## Post-Build

### 8. Write to USB (Optional)

```bash
./write-usb-dd.sh /dev/diskN
```

Or use Balena Etcher to flash the ISO.

### 9. Test on Real Hardware

- [ ] Boot from USB
- [ ] Network configuration works
- [ ] All services start automatically
- [ ] Can access web UI
- [ ] Containers are detected and managed

## Deployment Paths Reference

The ISO build must install to these paths:

| Component | Path | Source |
|-----------|------|--------|
| Backend binary | `/usr/local/bin/archipelago` | `build/backend/archipelago` |
| Frontend files | `/opt/archipelago/web-ui/` | `build/frontend/*` |
| Systemd service | `/etc/systemd/system/archipelago.service` | `configs/archipelago.service` |
| Nginx config | `/etc/nginx/sites-available/archipelago` | `configs/nginx-archipelago.conf` |
| Nginx symlink | `/etc/nginx/sites-enabled/archipelago` | Link to sites-available |

## Common Issues

### Backend Not Detecting Containers
- Verify service runs as `root` user
- Check Podman context: `sudo podman ps` should show containers
- Enable dev mode: `ARCHIPELAGO_DEV_MODE=true`

### UI Not Loading
- Check nginx configuration paths
- Verify frontend files deployed to `/opt/archipelago/web-ui/`
- Check nginx error logs: `/var/log/nginx/error.log`

### Service Fails to Start
- Check binary permissions: Should be executable
- Check systemd logs: `journalctl -u archipelago`
- Test binary manually: `sudo /usr/local/bin/archipelago`

## Version Tracking

When building a new ISO, document:
- Date: _______________
- Git commit: _______________
- Backend version: _______________
- Frontend version: _______________
- ISO filename: _______________
- Tested on hardware: _______________
- Issues found: _______________
