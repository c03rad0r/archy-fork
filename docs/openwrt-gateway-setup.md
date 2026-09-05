# OpenWrt Gateway Setup

How to connect an OpenWrt router to an Archipelago node and, optionally, turn
it into a pay-as-you-go WiFi gateway with **TollGate**. Written for a node
operator following the UI; a developer-facing RPC/architecture reference is
at the bottom.

This feature manages a **separate physical (or virtual) router** running
OpenWrt over SSH/UCI — it is not a containerized app. Archipelago itself does
not flash or install OpenWrt; you bring a router that already runs it.

## What you get

- **Status dashboard**: hostname, uptime, firmware release, WiFi interfaces,
  WAN state — polled live from the router.
- **WAN/WISP wizard**: point the router's radio at an upstream WiFi network
  (turns it into a wireless bridge/repeater) with DHCP + NAT configured for
  you.
- **TollGate provisioning** (optional): installs the
  [TollGate](https://tollgate.me) captive-portal package
  (`tollgate-module-basic-go`) and stands up an `archipelago` SSID that
  sells timed internet access for sats, settled against this node's local
  Cashu mint.

## Prerequisites

1. **A router already flashed with OpenWrt.** Check the
   [OpenWrt Table of Hardware](https://openwrt.org/toh/start) for your model
   and follow OpenWrt's own install/flashing instructions — that part is
   outside Archipelago's scope. See below for a worked example (GL.iNet
   AX3000).
2. **SSH reachable.** Fresh OpenWrt images enable `dropbear` (SSH) on LAN by
   default, listening as `root` with no password (or the password you set
   during OpenWrt's first-boot wizard at `192.168.1.1`). Archipelago
   connects with `ssh2` over a password (key-based auth is supported at the
   library level but the UI only offers password so far).
3. **Same LAN as the Archipelago node**, at least for setup — plug the
   router's LAN port into the same switch/network segment the node is on.
4. **For TollGate**: a running Cashu mint app (`nutshell`/`cashu-mint`) on
   this node — provisioning defaults `mint_url` to
   `http://<node-ip>:3338` and TollGate customers must be able to reach that
   URL from outside the node's loopback.

## Worked example: flashing a GL.iNet AX3000 to stock OpenWrt

GL.iNet's "AX3000" travel router is the **Beryl AX (GL-MT3000)** —
MediaTek MT7981B (Cortex-A53), OpenWrt target `mediatek/filogic`. It ships
running a GL.iNet fork of OpenWrt with its own web UI and LuCI already
enabled, but the steps below replace that with stock/vanilla OpenWrt so it
matches the prebuilt TollGate `.ipk` architectures exactly
(`aarch64_cortex-a53`).

1. **Download the sysupgrade image** for the current stable release from
   `https://downloads.openwrt.org/releases/<version>/targets/mediatek/filogic/`
   — the file you want is
   `openwrt-<version>-mediatek-filogic-glinet_gl-mt3000-squashfs-sysupgrade.bin`.
2. **Verify the checksum** against the `sha256sums` file in that same
   directory before flashing anything.
3. **Flash from the GL.iNet UI**: on the router's default address
   (`192.168.8.1`), go to **More Settings → Upgrade → Local Upgrade**, or
   open **Advanced → LuCI** and use **System → Backup / Flash Firmware →
   Flash new firmware image**.
4. Upload the `.bin` file. **Uncheck "Keep Settings"** — going from the
   GL.iNet fork to stock OpenWrt needs a clean reset, not a config carry-over.
5. Confirm and wait ~3–5 minutes without power-cycling the router.
6. **After it reboots** you're on stock OpenWrt: LAN at `192.168.1.1`, DHCP
   on, SSH (dropbear) open as `root` with **no password set yet** — set one
   via LuCI at `192.168.1.1` or `passwd` over SSH before doing anything else.
   From here, continue with the Prerequisites/Step 2 flow above to connect
   it to the Archipelago node.

**If the flash fails / the router doesn't come back**: filogic devices
don't use a reset-button recovery. Instead, connect to the router's LAN
port and, during boot, press a key within the first ~2 seconds to enter
U-Boot; per the OpenWrt wiki, typing `gl` then `httpd` at the U-Boot prompt
brings up a recovery web UI at `192.168.1.2` that accepts a firmware image.

## Step 1: Open the OpenWrt Gateway panel

1. In the Archipelago UI, go to **Server**.
2. Under the network status list, click **OpenWrt Gateway**
   (`/dashboard/server/openwrt`).

If no router has been connected before, you'll land on the connect form.

## Step 2: Connect the router

You have two options:

- **Detect**: click **Detect** — this reads the node's own active wired
  Ethernet interface, derives its subnet, and probes every host on it for
  `TCP/22` + a valid `/etc/openwrt_release`. If it finds exactly one router
  it fills in the host automatically; if it finds several you pick from the
  list. A `/24` scan can take up to ~2 minutes (255 sequential probes at
  500 ms each on hosts that don't respond).
- **Manual**: type the router's LAN IP (commonly `192.168.1.1` on a router
  freshly bridged in, or whatever address it has on your network) plus the
  SSH username (default `root`) and password.

Click **Connect**. On success the panel switches to the status dashboard and
the connection (host + credentials) is persisted server-side — you won't
need to re-enter them on future visits or from other views (e.g. the Home
dashboard's network tile also polls this without prompting again).

> Credentials are stored in `router_config.json` under the node's data
> directory alongside other node config. There's no separate secrets
> vault entry for this yet — treat the router's SSH password like any other
> node-local config.

## Step 3: (Optional) Configure WAN/WISP

Use this to make the OpenWrt router pull its internet connection from an
upstream WiFi network instead of a wired uplink — useful for a
battery/off-grid TollGate node or extending coverage from an existing
network.

1. From the status dashboard, start the **WAN setup** wizard.
2. **Scan** — the router's radio scans for visible networks (a few seconds
   of SSH round-trips).
3. **Select network** — pick the upstream SSID from the list.
4. **Password** — enter the upstream network's WiFi password (encryption
   defaults to `psk2`; leave blank only for open networks).
5. **DHCP / NAT** — review the LAN DHCP pool (default `.100`–`.249`) and
   whether to enable NAT/masquerade on the WAN zone (leave this on unless
   you have a specific reason not to).
6. **Connect** — this writes a `wwan` STA `wifi-iface` + `network` interface
   over UCI, enables the radio if it was disabled (OpenWrt ships with
   `radio0.disabled=1` on a fresh flash), and adds `wwan` to the WAN
   firewall zone.

The dashboard's WAN panel shows the resulting association state, assigned
IP, and whether the router currently has internet reachability.

## Step 4: (Optional) Install TollGate

Once connected (and with a local Cashu mint app running), the dashboard
shows a **TollGate: not installed** panel with a single **Install TollGate**
button — there's no config form at this stage, it installs with defaults.
The panel itself warns: *"Router needs internet access to install TollGate
— configure WAN above first"* (Step 3), since the router has to reach the
internet to download the package.

1. Click **Install TollGate**. The button relabels to *"Installing… this
   may take a few minutes"* while it works.
2. Under the hood this installs `tollgate-module-basic-go` on the router
   (via `opkg` on OpenWrt ≤24.x, or a manual `.ipk` extract on 25.x images
   where `opkg` isn't available), writes `/etc/tollgate/config.json`, and
   creates the `archipelago` SSID — all with default pricing (10 sats per
   1-minute step, minimum 1 step, `mint_url` auto-filled to
   `http://<node-ip>:3338`, enabled).
3. On success you'll see *"TollGate provisioned successfully"* and the
   panel switches to the installed view (Enabled/Disabled badge, current
   price/step/mint).

### Configuring price, step size, or mint (after install)

The installed-state panel has an **Edit** button — this is the only place
you set price/step/mint, and it only appears once TollGate is already
installed:

1. Click **Edit**.
2. Set **Price** (sats), **Step size** (minutes — billed as `step_size_ms`
   under the hood), **Minimum steps** a customer must buy at once, **Mint
   URL** (leave as the auto-filled node URL unless pointing at an external
   mint), and the **Enable TollGate** toggle.
3. Click **Save**. Changes are pushed to `/etc/tollgate/config.json` and the
   daemon is restarted to pick them up — it does not hot-reload.

Anyone who joins the `archipelago` SSID sees TollGate's captive portal and
pays sats (via the configured Cashu mint) for timed access.

## Reconfiguring or moving to a different router

Use **Disconnect** on the status dashboard to return to the connect form —
this only clears the panel's client-side state, it doesn't delete the
persisted `router_config.json`, so reconnecting to the same router needs no
re-entry. To point at a *different* router, disconnect and connect with a
new host/credentials; the newly connected router becomes the persisted one.

## Troubleshooting

- **"No router configured"**: nothing has been connected yet, or the saved
  config didn't include a host — go through Step 2 again.
- **Connect hangs or times out**: the router isn't reachable on `TCP/22`
  from the node's network, or SSH auth failed. Confirm you can `ssh
  root@<router-ip>` manually from the node (or a machine on the same LAN)
  with the same credentials.
- **Router "moved networks" / stale saved host**: SSH/status calls are
  bounded (5s TCP connect, 30s read/write) precisely so an unreachable
  saved router can't stall other RPCs — but the dashboard will show a
  connection error until you reconnect with the router's current address.
- **TollGate provision fails with "No pre-built TollGate package for
  architecture..."**: your router's SoC isn't one of the prebuilt
  `.ipk` targets (`mips_24kc`, `mipsel_24kc`, `aarch64_cortex-a53`,
  `aarch64_cortex-a72`, `arm_cortex-a7`). You'll need a custom opkg feed or
  to build `tollgate-module-basic-go` from source for your architecture.
- **TollGate download looks like it succeeded but provisioning still
  fails**: the node sanity-checks the downloaded `.ipk` is at least 50 KB —
  a smaller file usually means `wget` captured an HTML error page instead
  (no internet access from the router, or a bad release URL).
- **Install fails right after a reboot or a fresh WAN setup** with `apk
  update failed ... router may have no internet access` even though WAN
  looks configured: this is usually just timing, not a real problem — the
  router's WiFi-uplink association (`wwan`/`hakodosh`-style STA interface)
  can take a few seconds longer to reconnect than the dashboard takes to
  let you click Install. Wait ~10–15 seconds after WAN shows `sta_state:
  up` and retry; it should succeed on the next attempt.
- **Install fails with `opkg not found at /usr/bin/opkg` (or similar) even
  though the router clearly has `opkg`/`apk` installed**: fixed as of
  2026-09-05 — the backend used to hardcode `/usr/bin/opkg`/`/usr/bin/apk`,
  which some official OpenWrt builds don't symlink into `/bin`. If you're
  running an Archipelago build from before that fix, update first.

---

## Developer reference

Backend crate: `core/openwrt` (`archipelago-openwrt`) — SSH/UCI plumbing,
WAN/WISP config, WiFi scanning, and TollGate install/config. See
[`architecture.md`](architecture.md) for where it sits in the workspace.

RPC methods (`core/archipelago/src/api/rpc/openwrt.rs`, dispatched in
`core/archipelago/src/api/rpc/dispatcher.rs`):

| Method | Purpose |
|---|---|
| `openwrt.scan` | Probe a subnet for OpenWrt routers (`subnet`, `prefix`, `ssh_user`, `ssh_password`) |
| `openwrt.get-status` | Full status: release, WiFi interfaces, WAN, TollGate state. No params → uses saved `router_config.json`; params with `host` also persist the connection |
| `openwrt.configure-wan` | Write WISP/WAN config (`ssid`, `password`, `encryption`, `dhcp_start`, `dhcp_limit`, `masq`) |
| `openwrt.scan-wifi` | Radio scan for visible upstream networks |
| `openwrt.provision-tollgate` | Install/reconfigure TollGate (`price_sats`, `step_size_ms`, `min_steps`, `mint_url`, `enabled`) |

Note: these are distinct from the unrelated `router.*` methods
(`router.discover`, `router.configure`, `router.list-forwards`, ...), which
handle UPnP/NAT-PMP port forwarding on the node's own upstream home router —
not the OpenWrt gateway feature described here.

Frontend: `neode-ui/src/views/server/OpenWrtGateway.vue`, routed at
`server/openwrt` (`neode-ui/src/router/index.ts`), linked from
`neode-ui/src/views/Server.vue`.

Persisted connection state: `router_config.json` in the node's data
directory (`core/archipelago/src/network/router.rs`:
`load_router_config`/`save_router_config`).
