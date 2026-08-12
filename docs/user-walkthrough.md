# Archipelago User Walkthrough

A complete guide to setting up and using Archipelago, from hardware to daily use. Each section describes what the user sees and does, serving as the basis for video tutorials.

---

## Part 1: Hardware & Preparation

### What You Need

- **Hardware**: Any x86_64 PC (Intel NUC, mini PC, old desktop) or Raspberry Pi 5
  - Minimum: 4GB RAM, 32GB SSD
  - Recommended: 8GB+ RAM, 1TB+ NVMe SSD (for Bitcoin full node)
- **USB drive**: 8GB+ for the installer
- **Network**: Ethernet cable (recommended) or WiFi
- **Monitor + keyboard**: For initial setup (optional if using headless mode)
- **Another computer**: To flash the USB and access the web UI

### Step 1: Download the ISO

> **Screenshot**: Browser showing the Archipelago releases page with download buttons for x86_64 and ARM64 ISOs.

1. Go to the Archipelago releases page
2. Download the latest `archipelago-auto-installer-*.iso` for your architecture
3. Verify the checksum matches the published hash

### Step 2: Flash the USB Drive

> **Screenshot**: Balena Etcher with the ISO selected and a USB drive ready to flash.

1. Download [Balena Etcher](https://etcher.io) (free, cross-platform)
2. Insert your USB drive
3. Open Etcher, select the downloaded ISO
4. Select your USB drive
5. Click "Flash!" — wait for completion (2-5 minutes)

### Step 3: Boot from USB

> **Screenshot**: BIOS boot menu showing USB drive as an option.

1. Insert the flashed USB into your target hardware
2. Power on and enter BIOS/boot menu (usually F2, F12, or Del during boot)
3. Select the USB drive as the boot device
4. The installer will start automatically

---

## Part 2: Installation

### Step 4: Auto-Installer Runs

> **Screenshot**: Terminal showing the auto-installer progress — partitioning, copying files, setting up the system.

The auto-installer handles everything:
- Partitions the target disk (erases existing data)
- Copies the Archipelago system
- Installs the bootloader
- Pre-loads container images for offline app installation

**Duration**: 5-15 minutes depending on hardware speed.

### Step 5: First Boot

> **Screenshot**: Console showing systemd services starting — archipelago.service, nginx, podman.

1. Remove the USB drive when prompted
2. The system reboots into Archipelago
3. Services start automatically (takes 30-60 seconds)
4. If a monitor is connected, the kiosk mode launches showing the web UI

---

## Part 3: First-Time Setup (Onboarding)

### Step 6: Connect to the Web UI

> **Screenshot**: Browser address bar showing `http://192.168.1.x` with the Archipelago splash screen loading.

1. Find your server's IP address:
   - Check your router's DHCP client list
   - Or connect a monitor — the IP is shown on the kiosk display
2. Open a browser on any device on the same network
3. Navigate to `http://<server-ip>`
4. The splash screen plays the Archipelago intro animation

### Step 7: Tap to Start

> **Screenshot**: The splash screen with "Tap anywhere to begin" text and cosmic background animation.

1. The intro screen shows the Archipelago logo with atmospheric music
2. Tap or click anywhere to proceed
3. A typing animation welcomes you: "Welcome, Noderunner"

### Step 8: Create Your Password

> **Screenshot**: The "Set Up Your Node" screen with password and confirm-password fields, glass-morphism design.

**There is no default web password.** A freshly installed node has no user
account at all, so this screen shows a password-creation form rather than a
login form:

1. Enter a password (minimum 8 characters)
2. Confirm it in the second field
3. Click "Set Up Node"

Every boot after this one shows the normal login form and asks for the password
you chose here. Store it somewhere you can get back to — recovering it requires
SSH access to the node.

### Step 9: Choose Your Path (Onboarding)

> **Screenshot**: The onboarding path selection screen showing three options: Bitcoin Node, Home Server, Full Sovereignty.

The onboarding wizard guides you through setup:

1. **Choose your path**:
   - **Bitcoin Node**: Bitcoin Knots + LND + Mempool (focused)
   - **Home Server**: Bitcoin + Home Assistant + File Manager (balanced)
   - **Full Sovereignty**: Everything — Bitcoin, Lightning, Nostr, VPN, Cloud (maximum)

2. **Create your identity**:
   > **Screenshot**: DID creation screen showing the generated decentralized identifier.

   - A DID (Decentralized Identifier) is generated for your node
   - This is your sovereign digital identity — no third party needed

3. **Backup your seed**:
   > **Screenshot**: Seed phrase display with 12 words and a "I've saved this" checkbox.

   - Write down or save your backup passphrase
   - This is the only way to recover your node if hardware fails
   - Store it securely offline

4. **Verify your backup**:
   > **Screenshot**: Verification screen asking to confirm specific words from the backup.

   - Confirm you've saved your backup by entering requested words

5. **Setup complete**:
   > **Screenshot**: Completion screen with confetti animation and "Enter your node" button.

   - Click to enter the dashboard

---

## Part 4: The Dashboard (Daily Use)

### Step 10: Home Screen

> **Screenshot**: The Archipelago dashboard with glass-card layout — system status, Bitcoin sync progress, quick actions.

The home screen shows:
- **System status**: CPU, RAM, disk usage, uptime
- **Bitcoin sync progress**: Block height, peer count, sync percentage
- **Quick actions**: Start/stop apps, check notifications
- **Node identity**: Your DID and Nostr public key

### Step 11: My Apps

> **Screenshot**: The Apps page showing installed containers as glass cards — Bitcoin Knots (running), LND (running), Mempool (stopped).

- View all installed applications
- **Green dot**: Running
- **Red dot**: Stopped
- Click an app to see details, logs, and actions
- Start/stop apps with one click

### Step 12: App Details

> **Screenshot**: Bitcoin Knots detail page showing sync status, peer count, block height, and action buttons.

Each app detail page shows:
- Container status and health
- Live logs (scrollable)
- Start / Stop / Restart buttons
- Launch button (opens the app's own UI in a new tab)
- Resource usage

### Step 13: Marketplace

> **Screenshot**: The Marketplace page with curated app cards — each showing name, description, and install button.

- Browse available applications
- Install with one click
- Apps are verified containers with security hardening
- Categories: Bitcoin, Lightning, Home, Nostr, Other

### Step 14: Cloud (File Manager)

> **Screenshot**: The Cloud page showing folders (Documents, Photos, Music) with breadcrumb navigation.

- Browse files stored on your node
- Upload and download files
- Organized with breadcrumb navigation
- Files are stored locally — not in the cloud

### Step 15: Server Status

> **Screenshot**: The Server page showing CPU/RAM/Disk gauges, Tor status, and network information.

- Real-time system metrics
- Tor connectivity status and .onion address
- DNS configuration
- VPN status
- Federation peers (if configured)

### Step 16: Web5 Identity

> **Screenshot**: The Web5 page showing DID document, Nostr public key, and credential management.

- View your decentralized identity (DID)
- Manage verifiable credentials
- Publish your identity to Nostr relays
- Create and verify presentations

### Step 17: Settings

> **Screenshot**: The Settings page with sections for password, appearance, system update, and shutdown.

- Change password
- Configure TOTP two-factor authentication
- Check for system updates
- Restart or shutdown the server
- Reset onboarding (for testing)

---

## Part 5: Advanced Operations

### Accessing via Tor

> **Screenshot**: Browser showing the .onion address in the Tor Browser URL bar.

1. Install [Tor Browser](https://torproject.org)
2. Find your .onion address in Settings > Server
3. Access your node from anywhere in the world via Tor

### Federation (Multi-Node)

> **Screenshot**: Federation page showing connected peer nodes with status indicators.

1. Generate an invite code from Server > Federation
2. Share the code with a trusted peer
3. They join using the code on their node
4. Monitor peer status and deploy apps remotely

### Hardware Wallet Integration

> **Screenshot**: PSBT signing flow — creating a transaction, scanning QR with hardware wallet.

1. Create a PSBT (Partially Signed Bitcoin Transaction) from Web5
2. Transfer to your hardware wallet (QR code or file)
3. Sign on the hardware wallet
4. Import the signed PSBT back to finalize and broadcast

### Controller / Gamepad Navigation

Archipelago supports Xbox-style controller navigation throughout the UI.

#### Global Controls

| Button | Action |
|--------|--------|
| D-pad Up/Down | Navigate between elements |
| D-pad Left/Right | Move between zones (sidebar ↔ content) |
| A / Enter | Select / activate / enter container |
| B / Escape | Go back / exit container / return to sidebar |

#### Navigation Zones

**Sidebar** (left column — always visible on desktop):
- Up/Down = move between items (wraps), auto-navigates page links
- Right = enter main content (first container, or first button on container-free pages)
- Left = nothing

**Nav Bar** (mode-switcher tabs at top of content — e.g. My Apps / App Store / Services):
- Left/Right = move between tabs
- Down = jump to first card/container below (remembers tab for Up return)
- Up = nothing (Escape to sidebar)
- Left from leftmost = sidebar

**Container Grid** (card tiles — Apps, Discover, Network, Home):
- Arrows = spatial navigation between cards
- Enter = primary action (Install, Launch, or enter inner controls)
- Escape = sidebar
- Left from leftmost card = sidebar
- Up from top row = return to remembered nav bar tab

**Inside Container** (after Enter on a card — inner buttons/controls):
- Arrows = move between inner controls
- Escape = exit back to the card
- Cannot leave via arrows — must Escape first

**Text Inputs** (search bars, form fields):
- Up/Down = exit field, navigate to nearest element
- Enter = submit (clicks the next button)
- Left/Right = cursor movement (exits field at edges)

#### Per-Page Mapping

**Home** (`/dashboard`)
- Right from sidebar → first status card
- D-pad navigates between status cards spatially
- Enter on card → navigates to that section

**My Apps** (`/dashboard/apps`)
- Right from sidebar → first app card
- D-pad navigates app card grid spatially
- Enter on card → app details page
- Enter on focused card with Launch button → launches app

**App Store / Discover** (`/dashboard/discover`)
- Right from sidebar → first featured card
- D-pad navigates card grid (Sovereignty Stack + All Applications)
- Down from nav tabs → first card below
- Up from top card → returns to last-focused tab
- Enter on card → app detail / install
- Cards lift on hover/focus (same as My Apps)

**Network** (`/dashboard/server`)
- Right from sidebar → Quick Actions card
- D-pad navigates between cards: Quick Actions → Local Network / Web3 → Network Interfaces / Tor Services
- Enter on Quick Actions → enters inner buttons (Restart, Check Tor, View Logs)
- Escape from inner buttons → back to card
- All cards lift on hover/focus

**Settings** (`/dashboard/settings`) — **Linear navigation, no containers**
- Right from sidebar → first button (server name row)
- D-pad Up/Down steps through ALL buttons/controls top-to-bottom:
  1. Server Name / What's New
  2. Copy DID
  3. Copy Onion Address
  4. Change Password
  5. Enable/Disable 2FA
  6. Logout
  7. Choose Language
  8. Login with Claude
  9. AI Data Access toggles (each enable/disable row)
  10. Manage Updates
  11. Webhook URL input
  12. Webhook Secret input
  13. Container Crash / Update Available toggles
  14. Disk Space Warning / Backup Complete toggles
  15. Save Configuration / Send Test Webhook
  16. Enable Beta Telemetry
  17. Create Backup
  18. Export Channel Backup
  19. Network Diagnostics
  20. Reboot
  21. Factory Reset
- Enter = activates the focused button/toggle
- Escape / Left = sidebar

**Mesh** (`/dashboard/mesh`)
- Right from sidebar → Device status card (left column)
- D-pad navigates between left-column containers (Device, Actions, Peers)
- Enter on peer → opens chat, auto-focuses message input
- Type message + Enter = send
- Escape = close chat / back to sidebar

**Cloud** (`/dashboard/cloud`)
- Right from sidebar → first folder/file card
- D-pad navigates file grid spatially
- Enter = open folder / file details

**Detail Pages** (app details, marketplace app details):
- Escape / B = go back to previous page

---

## Part 6: Maintenance

### Regular Tasks

| Task | Frequency | How |
|------|-----------|-----|
| Check for updates | Weekly | Settings > System Update |
| Review app health | Daily (glance) | Home screen status cards |
| Backup | Monthly | Settings > Backup |
| Check disk space | Monthly | Server status page |

### Updating Archipelago

1. Go to Settings > System Update
2. Click "Check for Updates"
3. If available, click "Install Update"
4. The system restarts automatically — do not power off during update

### Creating a Backup

1. Go to Settings > Backup
2. Enter a passphrase (remember this!)
3. Click "Create Backup"
4. Download the backup file and store it safely offline

---

## Quick Reference

| Action | Where |
|--------|-------|
| Start/stop an app | My Apps > App Card > Start/Stop |
| Install new app | Marketplace > Find App > Install |
| Check system health | Home or Server page |
| Change password | Settings > Security |
| Enable 2FA | Settings > Security > TOTP |
| View logs | My Apps > App > Logs |
| Access via Tor | Settings > Server > Tor Address |
| Restart server | Settings > System > Restart |
| Create backup | Settings > Backup |
