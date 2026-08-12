# Archipelago Troubleshooting Guide

This guide covers the most common issues you may encounter with Archipelago, along with diagnostic commands and solutions.

## Connection & Access

### 1. Can't connect to the web UI

**Symptoms**: Browser shows "connection refused" or spins forever when accessing `http://<your-server-ip>`

**Diagnosis**:
```bash
# Check if the server is reachable on the network
ping <server-ip>

# SSH in and check Nginx
ssh archipelago@<server-ip>
sudo systemctl status nginx
sudo nginx -t

# Check if the backend is running
sudo systemctl status archipelago
curl -s http://localhost:5678/health
```

**Solutions**:
- Ensure you're on the same network (LAN) as the server
- If Nginx is down: `sudo systemctl restart nginx`
- If backend is down: `sudo systemctl restart archipelago`
- Check firewall: `sudo ufw status` — port 80 (HTTP) and 443 (HTTPS) must be allowed
- If the server IP changed, check your router's DHCP lease table or run `ip addr show` on the server

### 2. Login page loads but login fails

**Symptoms**: You see the login screen but entering the correct password shows an error

**Diagnosis**:
```bash
# Check backend logs
sudo journalctl -u archipelago --since "5 minutes ago" --no-pager

# Test the RPC endpoint directly
curl -s -X POST http://localhost:5678/rpc/v1 \
  -H 'Content-Type: application/json' \
  -d '{"method":"server.echo","params":{"message":"test"}}' | head -100
```

**Solutions**:
- There is no default password — the password is the one you created on this
  node's first-boot "Set Up Your Node" screen. Password recovery requires SSH
  access to the node; note that simply deleting `/var/lib/archipelago/user.json`
  does **not** work, because the onboarding gate refuses `auth.setup` once the
  node is provisioned
- Clear browser cookies and try again (stale session cookie)
- Restart the backend: `sudo systemctl restart archipelago`
- Check if the database is accessible: `ls -la /var/lib/archipelago/`

### 3. Web UI loads but shows blank white page

**Symptoms**: Browser loads but nothing renders, or you see a white screen

**Diagnosis**:
```bash
# Check if frontend files exist
ls -la /opt/archipelago/web-ui/index.html
ls -la /opt/archipelago/web-ui/assets/

# Check browser console (F12 > Console) for JavaScript errors
# Check Nginx error log
sudo tail -20 /var/log/nginx/error.log
```

**Solutions**:
- Redeploy the frontend: run the deploy script from the development machine
- Check if files exist in `/opt/archipelago/web-ui/` — if missing, the deploy didn't complete
- Clear browser cache (Ctrl+Shift+R or Cmd+Shift+R)
- Try a different browser or incognito mode

### 4. HTTPS certificate warning

**Symptoms**: Browser shows "Your connection is not private" or certificate error

**Solutions**:
- Archipelago uses a self-signed certificate by default — this is expected on first visit
- Click "Advanced" > "Proceed to site" (Chrome) or "Accept the Risk" (Firefox)
- For permanent fix, configure a domain name and use Let's Encrypt
- On kiosk mode, the certificate is auto-accepted

---

## App Issues

### 5. App won't start (container fails to launch)

**Symptoms**: Clicking "Start" on an app shows an error, or the app stays in "stopped" state

**Diagnosis**:
```bash
# Check container status
podman ps -a --filter "name=<app-id>"

# Check container logs
podman logs <app-id> --tail 50

# Check if the image exists
podman images | grep <app-id>

# Check available disk space
df -h /var/lib/archipelago
```

**Solutions**:
- If the image is missing: reinstall the app from the Marketplace
- If disk is full: run disk cleanup from the **Server** page (`/server`), or manually `podman system prune`
- If the container exits immediately: check logs for the root cause (usually missing config or permissions)
- Restart the Podman socket. Archipelago runs **rootless** Podman as the
  `archipelago` user, so this is a `--user` unit — `sudo systemctl restart podman`
  would restart the unrelated root socket:
  `systemctl --user restart podman.socket`

### 6. App shows "unhealthy" status

**Symptoms**: App is running but shows a yellow or red health indicator

**Diagnosis**:
```bash
# Check container health
podman healthcheck run <app-id>

# Check container resource usage
podman stats <app-id> --no-stream

# Check container logs for errors
podman logs <app-id> --tail 100 | grep -i error
```

**Solutions**:
- Some apps take time to become healthy after starting (especially Bitcoin which needs to sync)
- Check if the app has enough resources (RAM, CPU)
- Restart the specific app from the UI or: `podman restart <app-id>`
- Check if dependent services are running (e.g., LND requires Bitcoin)

### 7. Bitcoin not syncing / stuck at a block height

**Symptoms**: Bitcoin node shows the same block height for an extended period

**Diagnosis**:
```bash
# Check Bitcoin logs
podman logs bitcoin-knots --tail 50

# Check if Bitcoin is connected to peers.
# The datadir inside the container is /home/bitcoin/.bitcoin, and the RPC
# credentials live in the generated /tmp/rpc.conf (the manifest's entrypoint
# writes it from the BITCOIN_RPC_USER/BITCOIN_RPC_PASS secrets) — bitcoin-cli
# needs both flags or it can't authenticate.
podman exec bitcoin-knots bitcoin-cli \
  -datadir=/home/bitcoin/.bitcoin -conf=/tmp/rpc.conf \
  getpeerinfo | grep -c '"addr"'

# Check sync progress
podman exec bitcoin-knots bitcoin-cli \
  -datadir=/home/bitcoin/.bitcoin -conf=/tmp/rpc.conf \
  getblockchaininfo | grep -E "blocks|headers|verificationprogress"
```

**Solutions**:
- Initial sync takes 1-7 days depending on hardware — be patient
- Ensure the server has a stable internet connection
- Check disk space. The manifest picks the mode from the disk it's given: under
  1000 GB it runs **pruned** (`-prune=550`, a few GB); at 1000 GB or more it runs
  a full `-txindex=1` archival node, which needs 600 GB+ and growing
- If stuck: restart the container `podman restart bitcoin-knots`
- If peers = 0: check firewall allows port 8333 outbound
- Editing `bitcoin.conf` in the data directory has **no effect** — the
  entrypoint runs bitcoind with an explicit `-conf=/tmp/rpc.conf` and logs
  "ignoring legacy datadir bitcoin.conf". Flags come from the app manifest, so
  persistent changes belong there (and, for catalog-covered apps, in the signed
  catalog entry that overrides the on-disk manifest)

### 8. LND won't connect to Bitcoin

**Symptoms**: LND shows errors about Bitcoin connection, or channels aren't working

**Diagnosis**:
```bash
# Check LND logs
podman logs lnd --tail 50

# Check if Bitcoin RPC is accessible from LND
podman exec lnd wget -qO- http://bitcoin-knots:8332/ 2>&1 | head -5

# Check LND status
podman exec lnd lncli getinfo 2>&1 | head -20
```

**Solutions**:
- Ensure Bitcoin is fully synced before starting LND
- Both containers must be on the same Podman network (`archy-net`)
- Check Bitcoin RPC credentials match what LND expects
- Restart both containers in order: Bitcoin first, then LND

---

## Backup & Recovery

### 9. Backup fails to create

**Symptoms**: Backup button shows an error, or backup file is empty

**Diagnosis**:
```bash
# Check disk space
df -h /var/lib/archipelago

# Check backup directory permissions
ls -la /var/lib/archipelago/backups/

# Check backend logs for backup errors
sudo journalctl -u archipelago --since "10 minutes ago" | grep -i backup
```

**Solutions**:
- Ensure sufficient disk space (backups can be large)
- Check permissions: backup directory should be owned by `archipelago` user
- Try creating a smaller backup (exclude app data)
- Restart the backend service and try again

### 10. Can't restore from backup

**Symptoms**: Restore process fails or data doesn't appear after restore

**Diagnosis**:
```bash
# Verify backup file integrity
file /path/to/backup.archipelago
ls -la /path/to/backup.archipelago

# Check backend logs during restore
sudo journalctl -u archipelago -f
```

**Solutions**:
- Ensure the backup file is not corrupted (check file size is reasonable)
- Passphrase must match what was used during backup creation
- Stop all running apps before restoring
- After restore, restart the backend: `sudo systemctl restart archipelago`

---

## System Updates

### 11. System update fails

**Symptoms**: Update button shows an error, or update process hangs

**Diagnosis**:
```bash
# Check internet connectivity
curl -s https://debian.org > /dev/null && echo "Internet OK" || echo "No internet"

# Check backend logs
sudo journalctl -u archipelago --since "15 minutes ago" | grep -i update

# Check disk space (updates need temporary space)
df -h /
```

**Solutions**:
- Ensure stable internet connection during updates
- Ensure at least 2GB free disk space
- If update hangs: wait 10 minutes, then restart the backend
- Do NOT power off during an update — this can corrupt the system
- If the system is in a bad state after a failed update, recover over SSH — the
  USB installer has no repair mode (its boot menu offers only "Install
  Archipelago", "Install Archipelago (verbose)" and "Boot from local disk")

### 12. Server won't boot after update

**Symptoms**: Server doesn't respond after a system update

**Solutions**:
- Wait 5 minutes — the first boot after update may take longer
- If still unresponsive: connect a monitor/keyboard to check boot messages
- If it's a bootloader problem rather than a disk problem, boot the USB and pick
  "Boot from local disk" to chainload the installed system
- As a last resort: reinstall from USB and restore from backup. The installer is
  interactive — it asks for the target disk and requires typing `yes` — so
  booting it does not by itself destroy the existing install

---

## Kiosk Mode

### 13. Kiosk display shows black screen

**Symptoms**: Connected monitor shows black screen instead of the Archipelago UI

**Diagnosis**:
```bash
# SSH in and check kiosk service
sudo systemctl status archipelago-kiosk

# Check if X11/Wayland is running
ps aux | grep -E "(Xorg|weston|chromium|firefox)"

# Check display output
ls /dev/dri/
xrandr --query 2>/dev/null || echo "No display server"
```

**Solutions**:
- Restart the kiosk service: `sudo systemctl restart archipelago-kiosk`
- Check HDMI cable is securely connected
- Try a different HDMI port or cable
- Check if the display is set to the correct input source
- Review kiosk logs: `sudo journalctl -u archipelago-kiosk --since "5 minutes ago"`

### 14. Kiosk display is stuck or frozen

**Symptoms**: Kiosk shows the UI but it's unresponsive to touch/mouse

**Solutions**:
- The watchdog service should auto-restart frozen kiosk — wait 30 seconds
- SSH in and restart: `sudo systemctl restart archipelago-kiosk`
- Check if the backend is responsive: `curl -s http://localhost:5678/health`
- If backend is down too, restart everything: `sudo systemctl restart archipelago archipelago-kiosk`

---

## Network & Connectivity

### 15. Tor address not available

**Symptoms**: Settings shows "Tor: Not configured" or the .onion address is missing

Tor is **not** a container — it's the host's Debian `tor` package, running as
`debian-tor`. Archipelago never touches it directly: it stages a torrc and asks
`archipelago-tor-helper` (a `.path` unit watching
`/var/lib/archipelago/tor-config/tor-action`) to install it and restart Tor.

**Diagnosis**:
```bash
# Check the host Tor service and the helper that drives it
sudo systemctl status tor
sudo journalctl -u archipelago-tor-helper --since "10 minutes ago"

# Is the SOCKS port up? (this is the liveness check the backend itself uses)
nc -z 127.0.0.1 9050 && echo "Tor SOCKS OK"

# The readable hostname copy the backend actually reads
cat /var/lib/archipelago/tor-hostnames/archipelago

# The hidden-service dir itself (root-owned 0700 — needs sudo)
sudo cat /var/lib/tor/hidden_service_archipelago/hostname 2>/dev/null \
  || sudo cat /var/lib/archipelago/tor/hidden_service_archipelago/hostname
```

**Solutions**:
- Tor takes 30-60 seconds to bootstrap — wait and refresh
- If `/var/lib/archipelago/tor-hostnames/archipelago` is missing but the
  hidden-service dir has a `hostname`, the readable copy didn't sync — the
  helper's `sync-hostnames` action rewrites it
- Check that the Tor data directory exists and is owned by `debian-tor`
- Restart Tor: `sudo systemctl restart tor`

### 16. Peers can't reach my node

**Symptoms**: Federation peers show "unreachable" status

**Diagnosis**:
```bash
# Check if Tor is running (the fallback transport for peer connectivity)
sudo systemctl status tor

# Check your Tor address
cat /var/lib/archipelago/tor-hostnames/archipelago

# Test connectivity from the server side
curl -s http://localhost:5678/rpc/v1 \
  -H 'Content-Type: application/json' \
  -d '{"method":"node.tor-address","params":{}}' | head -50
```

**Solutions**:
- Tor is the last-resort transport, not the only one: peering prefers mesh
  radio, then LAN, then FIPS, and only falls back to Tor. A peer stuck on
  "unreachable" with Tor healthy usually means the higher transports are all
  down too — check the FIPS anchor first
- Tor circuits can be slow — connections may take 30+ seconds
- Share your correct .onion address with peers
- Both nodes must be on the same federation

### 17. DNS resolution issues

**Symptoms**: Apps can't reach external services, container downloads fail

**Diagnosis**:
```bash
# Test DNS from the server
nslookup google.com
dig google.com

# Check DNS configuration
cat /etc/resolv.conf

# Test from within a container
podman exec bitcoin-knots nslookup seed.bitcoin.sipa.be
```

**Solutions**:
- Configure DNS from the **Server** page (`/server`): try Cloudflare (1.1.1.1) or Google (8.8.8.8)
- If using custom DNS, verify the server addresses are correct
- Restart networking: `sudo systemctl restart systemd-resolved`

---

## Performance & Resources

### 18. Server is very slow / high CPU usage

**Symptoms**: Web UI is slow to respond, apps are laggy

**Diagnosis**:
```bash
# Check CPU and memory usage
top -bn1 | head -15

# Check per-container resource usage
podman stats --no-stream

# Check disk I/O
iostat -x 1 3
```

**Solutions**:
- Bitcoin initial sync uses heavy CPU — this is normal and temporary
- Check which container is using the most resources with `podman stats`
- Stop apps you don't need
- If RAM is full: add swap space or upgrade hardware
- Consider using an SSD if running on HDD (massive I/O improvement)

### 19. Disk full

**Symptoms**: Apps fail, UI shows disk warning, new installs fail

**Diagnosis**:
```bash
# Check disk usage
df -h /var/lib/archipelago

# Find largest directories
du -sh /var/lib/archipelago/*/ | sort -rh | head -10

# Check Podman image/container sizes
podman system df
```

**Solutions**:
- Run disk cleanup from the **Server** page (`/server`)
- Remove unused app data: `podman system prune -a` (WARNING: removes all stopped containers and unused images)
- Move Bitcoin data to external drive if chain data is too large
- Check for large log files: `du -sh /var/log/*/ | sort -rh`
- Consider upgrading to a larger disk

### 20. WebSocket disconnections / "Reconnecting..." banner

**Symptoms**: UI shows a reconnecting indicator, real-time updates stop

**Diagnosis**:
```bash
# Check backend health
curl -s http://localhost:5678/health

# Check backend logs for WebSocket errors
sudo journalctl -u archipelago --since "5 minutes ago" | grep -i websocket

# Check system resources (WebSocket can drop under load)
free -h
```

**Solutions**:
- Brief disconnections are normal during backend restarts — the UI auto-reconnects
- If persistent: check if the backend is overloaded (high CPU/RAM)
- Restart the backend: `sudo systemctl restart archipelago`
- Check Nginx WebSocket proxy config: `/etc/nginx/sites-available/archipelago` must include `proxy_set_header Upgrade $http_upgrade`
- If on WiFi, try wired Ethernet for more stable connectivity

### 21. LoRa radio firmware flash failed / board unresponsive

**Symptoms**: The "Erase & Flash Now" flow in the mesh hot-swap modal reports
an error, or the radio no longer enumerates as a serial device after a flash
attempt.

**Diagnosis**:
```bash
# Poll the flash job's last-known stage/error directly
curl -s http://localhost:5678/rpc/v1 \
  -H 'Content-Type: application/json' \
  -d '{"method":"mesh.flash-status","params":{}}'

# Confirm the board is still enumerating at all
ls -la /dev/ttyUSB* /dev/ttyACM* /dev/mesh-radio 2>&1

# esptool/rnodeconf binaries present?
which esptool; ls -la /usr/local/bin/archy-rnodeconf
```

**Solutions**:
- A failure during `erasing`/`writing` (MeshCore/Meshtastic) or
  `autoinstalling` (Reticulum) can leave the chip erased or half-written —
  this is expected risk of the "always erase first" default, not a bug.
- Heltec V3/V4 boards can be forced back into bootloader mode manually: hold
  **BOOT**, tap **RST**, then release **BOOT** — this puts the chip in a
  state esptool can always talk to, regardless of what firmware (if any) is
  currently on it.
- With the board in bootloader mode, a manual recovery flash can be run
  directly over SSH without the UI:
  ```bash
  esptool --chip esp32s3 --port /dev/ttyACM0 erase_flash
  esptool --chip esp32s3 --port /dev/ttyACM0 write_flash 0x0 <known-good-image.bin>
  ```
- For Reticulum/RNode boards, the equivalent manual recovery is
  `archy-rnodeconf /dev/ttyACM0 --autoinstall` (or `/usr/local/bin/archy-rnodeconf`
  if it's not on `PATH`) — it re-runs the same fetch+erase+flash+bootstrap
  sequence the UI triggers.
- If `esptool`/`archy-rnodeconf` are missing entirely, they should have been
  installed by the last `self-update.sh` run — check
  `sudo journalctl -u archipelago-update` for install failures, or install
  `esptool` via `sudo apt-get install esptool` directly.
- Once a fresh image is confirmed written, unplug/replug the radio (or wait
  for the next detection poll) — the hot-swap modal re-probes automatically
  and shows whatever firmware is actually on the board now.

**Known incident (2026-07-23) — reconnect storm / device boot-loop after a
failed flash**: a real Heltec V3 got stuck cycling "connect → partial
handshake → drop" every 5-15s for 5+ minutes after a `mesh.flash-device`
attempt failed with `Reading firmware download stream`. Root cause was two
compounding issues, both now fixed:
1. `spawn_mesh_listener`'s reconnect backoff (`core/archipelago/src/mesh/listener/mod.rs`)
   reset to its 5s minimum any time the prior session had been `device_connected`
   at all, even for under a second — so a device that connects-then-drops
   repeatedly never actually backed off. Every retry's `open()` toggles
   DTR/RTS, which resets many ESP32 boards' MCU (native-USB *and*
   CP2102/CH340 auto-reset-circuit boards), so the aggressive retries were
   themselves *causing* the boot loop, not just observing one. Fixed by only
   resetting backoff when a session ran for at least `STABLE_SESSION_THRESHOLD`
   (20s) — see that constant's doc comment.
2. `mesh::flash::start_flash_job`'s post-completion handler auto-resumed the
   listener unconditionally, even after a *failed* flash, immediately
   re-entering the reconnect loop above with no cooldown. Fixed: on failure
   the listener is now deliberately left stopped (reconnect manually via the
   UI once the board is confirmed alive); on success there's a 5s settle
   delay before resuming, so the board finishes booting from the flash
   tool's own reset before Archipelago starts probing it again.
3. Separately, the download itself was failing because `mesh::flash`'s HTTP
   client had a blanket 30s request timeout that covered the *entire*
   download (including streaming a 170MB Meshtastic zip), not just
   connection setup — fixed with a per-chunk stall timeout instead of a
   fixed total-transfer cap.

If this symptom recurs (rapid repeating `mesh::serial: Opened serial
port... Starting Meshcore handshake` lines in `journalctl -u archipelago`
without a `LoRa firmware flash` job in progress), it's a NEW instance of the
same class of bug, not the one above — check whether backoff is actually
escalating (`Mesh session error: ... (retry in Xs)` — X should grow past 5s
within a few cycles) before assuming it's flashing-related.

---

## General Maintenance

### Quick Health Check Commands

```bash
# Overall system status
sudo systemctl status archipelago nginx

# All containers
podman ps -a

# Disk usage
df -h /var/lib/archipelago

# Memory usage
free -h

# Recent errors
sudo journalctl -u archipelago --since "1 hour ago" -p err

# Backend health endpoint
curl -s http://localhost:5678/health
```

### Emergency Recovery

If the system is completely unresponsive:

1. **Power cycle**: Hold power button for 10 seconds, then turn back on
2. **Wait 5 minutes**: Services take time to start, especially if containers need to recover
3. **SSH in**: If web UI is down but SSH works, restart services manually
4. **Chainload the installed system**: Boot the Archipelago USB and pick "Boot
   from local disk" — this rules out a broken bootloader
5. **Clean install + restore**: As last resort, do a fresh install and restore from backup

### Collecting Diagnostic Information

If you need to report an issue, collect this information:

```bash
# System info
uname -a
cat /etc/os-release

# Service status
sudo systemctl status archipelago nginx

# Recent logs (last 100 lines)
sudo journalctl -u archipelago --no-pager -n 100

# Container status
podman ps -a

# Disk and memory
df -h
free -h

# Network
ip addr show
```
