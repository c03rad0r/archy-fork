#!/bin/bash
# Self-update: pull latest code from the OVH Gitea (source.archipelago-foundation.org) and apply
# Designed to run on installed Archipelago nodes (as archipelago user)
#
# Usage:
#   ./self-update.sh              # Check + apply if available
#   ./self-update.sh --check      # Check only, don't apply
#   ./self-update.sh --force      # Apply even if already up to date
#
# The script:
#   1. Pulls latest code from origin (source.archipelago-foundation.org)
#   2. Builds the Rust backend (release mode)
#   3. Builds the Vue frontend (production mode)
#   4. Installs the new binary and web UI
#   5. Restarts the archipelago service
#   6. Verifies health after restart

set -euo pipefail

REPO_DIR="$HOME/archy"
BACKEND_DIR="$REPO_DIR/core"
FRONTEND_DIR="$REPO_DIR/neode-ui"
INSTALL_BIN="/usr/local/bin/archipelago"
INSTALL_WEB="/opt/archipelago/web-ui"
STATE_FILE="/var/lib/archipelago/update_state.json"
LOG_FILE="/var/lib/archipelago/update.log"
LOCK_FILE="/tmp/archipelago-update.lock"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log() { echo -e "${BLUE}[$(date '+%H:%M:%S')]${NC} $*" | tee -a "$LOG_FILE"; }
ok()  { echo -e "${GREEN}[$(date '+%H:%M:%S')] OK${NC} $*" | tee -a "$LOG_FILE"; }
err() { echo -e "${RED}[$(date '+%H:%M:%S')] ERROR${NC} $*" | tee -a "$LOG_FILE"; }
warn(){ echo -e "${YELLOW}[$(date '+%H:%M:%S')] WARN${NC} $*" | tee -a "$LOG_FILE"; }

cleanup() {
    rm -f "$LOCK_FILE"
}
trap cleanup EXIT

# Prevent concurrent updates
if [ -f "$LOCK_FILE" ]; then
    pid=$(cat "$LOCK_FILE" 2>/dev/null)
    if kill -0 "$pid" 2>/dev/null; then
        err "Update already in progress (PID $pid)"
        exit 1
    fi
    warn "Stale lock file found, removing"
    rm -f "$LOCK_FILE"
fi
echo $$ > "$LOCK_FILE"

# Parse args
CHECK_ONLY=false
FORCE=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --check) CHECK_ONLY=true; shift ;;
        --force) FORCE=true; shift ;;
        *) shift ;;
    esac
done

# Ensure repo exists
if [ ! -d "$REPO_DIR/.git" ]; then
    err "Repo not found at $REPO_DIR"
    err "Clone it first: git clone https://source.archipelago-foundation.org/lfg2025/archy ~/archy"
    exit 1
fi

cd "$REPO_DIR"

if ! command -v nano >/dev/null 2>&1; then
    log "Installing nano for Archipelago terminal..."
    if sudo apt-get update -qq 2>>"$LOG_FILE" && sudo apt-get install -y -qq nano 2>>"$LOG_FILE"; then
        ok "nano installed"
    else
        warn "Unable to install nano automatically; continuing update"
    fi
fi

if ! command -v ping >/dev/null 2>&1; then
    log "Installing iputils-ping..."
    if sudo apt-get update -qq 2>>"$LOG_FILE" && sudo apt-get install -y -qq iputils-ping 2>>"$LOG_FILE"; then
        ok "ping installed"
    else
        warn "Unable to install ping automatically; continuing update"
    fi
fi

if ! command -v esptool >/dev/null 2>&1; then
    log "Installing esptool for LoRa radio firmware flashing..."
    if sudo apt-get update -qq 2>>"$LOG_FILE" && sudo apt-get install -y -qq esptool 2>>"$LOG_FILE"; then
        ok "esptool installed"
    else
        warn "Unable to install esptool automatically; radio firmware flashing will be unavailable"
    fi
fi

# Debian's esptool package (4.7.0+dfsg-0.1) ships without the precompiled
# esp32s3 "stub flasher" blob (stripped for DFSG compliance — no
# buildable-from-source path Debian could verify). Without it, esptool's
# normal stub-loader mode fails outright (FileNotFoundError), and the ROM
# bootloader fallback (--no-stub) doesn't implement a full-chip erase at
# all — confirmed live 2026-07-23 flashing a real Heltec V4, both ways.
# Fetching the exact same file from the matching upstream esptool release
# tag restores full (and correct) flashing behavior — it's the same
# open-source codebase, just the one blob Debian's packaging couldn't
# include.
if command -v esptool >/dev/null 2>&1; then
    STUB_DIR="/usr/lib/python3/dist-packages/esptool/targets/stub_flasher"
    STUB_FILE="$STUB_DIR/stub_flasher_32s3.json"
    if [ ! -f "$STUB_FILE" ]; then
        log "Fetching esptool's esp32s3 stub flasher (missing from the Debian package)..."
        ESPTOOL_VERSION=$(esptool version 2>/dev/null | tail -1 | tr -d ' \t')
        if [ -n "$ESPTOOL_VERSION" ] && sudo curl -fsSL -o "$STUB_FILE" \
            "https://raw.githubusercontent.com/espressif/esptool/v${ESPTOOL_VERSION}/esptool/targets/stub_flasher/stub_flasher_32s3.json" \
            2>>"$LOG_FILE"; then
            sudo chmod 644 "$STUB_FILE"
            ok "esp32s3 stub flasher installed"
        else
            sudo rm -f "$STUB_FILE" 2>/dev/null
            warn "Unable to fetch esp32s3 stub flasher; LoRa firmware flashing will be unavailable"
        fi
    fi
fi

# Build-time prerequisites for reticulum-daemon/build.sh's PyInstaller step
# below (discovered the hard way: ensurepip needs python3-venv, and
# PyInstaller itself needs objdump + libpython3.13.so at build time — none
# of these are pulled in by a bare `python3` package on Debian trixie).
for pkg in python3-venv binutils libpython3.13; do
    if ! dpkg -s "$pkg" >/dev/null 2>&1; then
        log "Installing $pkg (reticulum-daemon build prerequisite)..."
        sudo apt-get update -qq 2>>"$LOG_FILE" && sudo apt-get install -y -qq "$pkg" 2>>"$LOG_FILE" \
            || warn "Unable to install $pkg automatically; reticulum-daemon tools build may fail"
    fi
done

# Fetch latest
log "Fetching from origin..."
git fetch origin main --quiet 2>>"$LOG_FILE"

# Check if there are updates
LOCAL=$(git rev-parse HEAD)
REMOTE=$(git rev-parse origin/main)

if [ "$LOCAL" = "$REMOTE" ] && [ "$FORCE" = "false" ]; then
    ok "Already up to date ($LOCAL)"
    if [ "$CHECK_ONLY" = "true" ]; then
        echo '{"update_available": false, "current": "'"$LOCAL"'"}'
    fi
    exit 0
fi

# Calculate what changed
COMMITS_BEHIND=$(git rev-list HEAD..origin/main --count)
log "Update available: $COMMITS_BEHIND commits behind"
log "  Local:  $LOCAL"
log "  Remote: $REMOTE"

if [ "$CHECK_ONLY" = "true" ]; then
    CHANGELOG=$(git log HEAD..origin/main --oneline --no-merges | head -20)
    echo '{"update_available": true, "current": "'"$LOCAL"'", "latest": "'"$REMOTE"'", "commits_behind": '"$COMMITS_BEHIND"'}'
    echo ""
    echo "Changes:"
    echo "$CHANGELOG"
    exit 0
fi

# Backup current binary
BACKUP_DIR="/var/lib/archipelago/update-backup"
mkdir -p "$BACKUP_DIR"
if [ -f "$INSTALL_BIN" ]; then
    cp "$INSTALL_BIN" "$BACKUP_DIR/archipelago.bak"
    log "Backed up current binary"
fi

# Pull latest code
log "Pulling latest code..."
git pull origin main --ff-only 2>>"$LOG_FILE" || {
    err "Git pull failed — local changes? Run: git reset --hard origin/main"
    exit 1
}

NEW_VERSION=$(git rev-parse --short HEAD)
log "Now at: $NEW_VERSION"

# Build backend
log "Building Rust backend (release)..."
cd "$BACKEND_DIR"
if cargo build --release --workspace 2>>"$LOG_FILE"; then
    ok "Backend built successfully"
else
    err "Backend build failed — rolling back"
    cd "$REPO_DIR"
    git reset --hard "$LOCAL" 2>>"$LOG_FILE"
    exit 1
fi

# Install binary
BUILT_BIN="$BACKEND_DIR/target/release/archipelago"
if [ ! -f "$BUILT_BIN" ]; then
    err "Built binary not found at $BUILT_BIN"
    exit 1
fi
sudo cp "$BUILT_BIN" "$INSTALL_BIN"
sudo chmod +x "$INSTALL_BIN"
ok "Backend installed"

# Build + install reticulum-daemon tools (archy-reticulum-daemon, archy-rnodeconf).
# Non-fatal: archipelago falls back to its dev venv path if the packaged
# binaries aren't present, so a missing/failed build here degrades mesh
# Reticulum support rather than breaking the update. This mirrors
# the existing manual-deploy step, which until now was the
# only path that ever installed these — a node that only ever received OTA
# self-updates had neither binary.
if [ -f "$REPO_DIR/reticulum-daemon/build.sh" ]; then
    log "Building reticulum-daemon tools (archy-reticulum-daemon, archy-rnodeconf)..."
    if (cd "$REPO_DIR/reticulum-daemon" && ./build.sh) 2>>"$LOG_FILE"; then
        for tool in archy-reticulum-daemon archy-rnodeconf; do
            if [ -f "$REPO_DIR/reticulum-daemon/dist/$tool" ]; then
                sudo cp "$REPO_DIR/reticulum-daemon/dist/$tool" /usr/local/bin/
                sudo chmod +x "/usr/local/bin/$tool"
                ok "$tool installed"
            else
                warn "$tool not built — leaving existing /usr/local/bin/$tool (if any) in place"
            fi
        done
    else
        warn "reticulum-daemon tools build failed — continuing without updating them"
    fi
fi

# Build frontend
log "Building Vue frontend (production)..."
cd "$FRONTEND_DIR"
npm ci --silent 2>>"$LOG_FILE" || npm install --silent 2>>"$LOG_FILE"
if npm run build 2>>"$LOG_FILE"; then
    ok "Frontend built successfully"
else
    err "Frontend build failed — backend already updated, service may need manual fix"
    exit 1
fi

# Install frontend (always ship fresh AIUI from demo/aiui; preserve claude-login.html)
BUILT_WEB="$REPO_DIR/web/dist/neode-ui"
if [ -d "$BUILT_WEB" ]; then
    # Bake AIUI into the built tree so rsync --delete does not wipe it.
    # demo/aiui is the canonical AIUI bundle checked into the repo; copying
    # it here means every self-update ships a matching AIUI version instead
    # of preserving whatever stale copy happened to be on disk (which is
    # empty on nodes where an earlier ad-hoc deploy blew it away).
    if [ -d "$REPO_DIR/demo/aiui" ] && [ -f "$REPO_DIR/demo/aiui/index.html" ]; then
        log "Staging AIUI bundle from demo/aiui into frontend dist..."
        rm -rf "$BUILT_WEB/aiui"
        cp -r "$REPO_DIR/demo/aiui" "$BUILT_WEB/aiui"
    else
        warn "demo/aiui not found in repo; existing /opt/archipelago/web-ui/aiui will be wiped by rsync --delete"
    fi
    # Sync new files, preserving claude-login.html (per-node admin bookmark)
    sudo rsync -a --delete \
        --exclude 'claude-login.html' \
        "$BUILT_WEB/" "$INSTALL_WEB/"
    ok "Frontend installed"
else
    warn "Frontend build output not found at $BUILT_WEB — skipping"
fi

# Update helper scripts in /opt/archipelago/scripts/
# These are canonical home; keep a copy at /opt/archipelago/image-versions.sh
# for backward compatibility with older binaries that still look there.
SCRIPTS_DEST="/opt/archipelago/scripts"
sudo mkdir -p "$SCRIPTS_DEST"
for script in image-versions.sh reconcile-containers.sh container-specs.sh container-doctor.sh sync-npm-public-hosts.sh app-surface-smoke-test.sh bitcoin-stack-lifecycle-test.sh; do
    src="$REPO_DIR/scripts/$script"
    if [ -f "$src" ]; then
        sudo install -m 755 "$src" "$SCRIPTS_DEST/$script"
        ok "Updated $script"
    else
        warn "Missing $src — skipping"
    fi
done
# Legacy path for image-versions.sh (older binaries looked here first)
if [ -f "$REPO_DIR/scripts/image-versions.sh" ]; then
    sudo cp "$REPO_DIR/scripts/image-versions.sh" /opt/archipelago/image-versions.sh
fi

# Sync app manifests and app-local build contexts into the canonical
# production manifest root. The backend orchestrator loads install specs from
# /opt/archipelago/apps; updating only the binary/frontend can leave a node
# with new installer logic but stale or missing app manifests.
APPS_DEST="/opt/archipelago/apps"
if [ -d "$REPO_DIR/apps" ]; then
    sudo mkdir -p "$APPS_DEST"
    sudo rsync -a --delete "$REPO_DIR/apps/" "$APPS_DEST/"
    ok "App manifests synced"
else
    warn "Apps directory not found at $REPO_DIR/apps — install manifests may be stale"
fi

# Update first-boot-containers.sh too (the canonical first-boot orchestrator).
# Nodes run it once on install, but keeping a fresh copy on disk means any
# future boot or reconciler invocation uses current port specs and caps.
if [ -f "$REPO_DIR/scripts/first-boot-containers.sh" ]; then
    sudo install -m 755 "$REPO_DIR/scripts/first-boot-containers.sh" \
        "$SCRIPTS_DEST/first-boot-containers.sh"
fi

# Sync UI container source trees (docker/bitcoin-ui, docker/lnd-ui,
# docker/electrs-ui) into /opt/archipelago/docker/<name>/. If any file in a
# UI tree changed since last update, rebuild that image and recreate its
# container using the spec from container-specs.sh. This is what prevented
# the lnd-ui port mismatch from reaching nodes through OTA: self-update used
# to update only the backend + frontend, never the UI container images.
UI_DOCKER_DEST="/opt/archipelago/docker"
sudo mkdir -p "$UI_DOCKER_DEST"
UI_REBUILD_LIST=""
# fips-ui and fedimint-ui are synced but NOT added to UI_REBUILD_LIST below:
# container-specs.sh has no spec for either (and their container names break
# the archy-<ui> assumption — the FIPS one is plain `fips-ui`). Their rebuilds
# come from elsewhere — the daemon's companion installer for fedimint-ui, the
# orchestrator's build context for fips-ui — but BOTH read
# /opt/archipelago/docker/<ui>, and nothing was ever updating that directory.
# So source edits to those two trees reached nodes through no path at all:
# their nginx kept listening on 0.0.0.0 and served the Guardian and FIPS
# screens unauthenticated on every interface (found by scanning a test node
# from outside, 2026-08-05 — the in-node audit could not see them).
for ui in bitcoin-ui lnd-ui electrs-ui fips-ui fedimint-ui; do
    src="$REPO_DIR/docker/$ui"
    dst="$UI_DOCKER_DEST/$ui"
    [ -d "$src" ] || continue
    # Hash source tree to decide if rebuild is needed. Any content change
    # (Dockerfile, nginx.conf, index.html, assets) triggers a rebuild.
    # Hash file contents only (not paths or metadata) so src and dst match
    # when their contents are identical regardless of directory prefix.
    src_hash=$( (cd "$src" && find . -type f | LC_ALL=C sort | xargs sha256sum 2>/dev/null) | sha256sum | cut -d' ' -f1)
    dst_hash=""
    if [ -d "$dst" ]; then
        dst_hash=$( (cd "$dst" && find . -type f | LC_ALL=C sort | xargs sha256sum 2>/dev/null) | sha256sum | cut -d' ' -f1)
    fi
    if [ "$src_hash" != "$dst_hash" ]; then
        log "UI source changed for $ui; syncing"
        sudo rsync -a --delete "$src/" "$dst/"
        case "$ui" in
            # Rebuilt below from container-specs.sh.
            bitcoin-ui|lnd-ui|electrs-ui)
                UI_REBUILD_LIST="$UI_REBUILD_LIST $ui" ;;
            # Synced only — rebuilt by the daemon (companion installer /
            # orchestrator build context), which watches this directory.
            # Adding them to the rebuild list would fail: no spec exists and
            # the container names are not archy-<ui>.
            *)
                log "  $ui synced; rebuild is owned by the daemon" ;;
        esac
    else
        ok "UI source unchanged for $ui"
    fi
done

# Rebuild changed UI images + recreate containers as the archipelago user
# (rootless podman storage lives under ~archipelago). Port mappings and caps
# come from scripts/container-specs.sh so spec drift can't sneak in.
if [ -n "$UI_REBUILD_LIST" ]; then
    log "Rebuilding UI containers:$UI_REBUILD_LIST"
    # shellcheck disable=SC1091
    # container-specs.sh provides load_spec_archy-<ui> and mem_limit <name>.
    SPECS="$SCRIPTS_DEST/container-specs.sh"
    if [ ! -f "$SPECS" ]; then
        warn "container-specs.sh missing at $SPECS; skipping UI rebuild"
    else
        for ui in $UI_REBUILD_LIST; do
            cname="archy-$ui"
            log "  rebuilding $cname from $UI_DOCKER_DEST/$ui"
            # Build image as archipelago user so it lands in the right store.
            if ! sudo -u archipelago bash -c "
                export XDG_RUNTIME_DIR=/run/user/\$(id -u archipelago)
                cd '$UI_DOCKER_DEST/$ui' &&
                podman build --no-cache -t 'localhost/$ui:local' . >>'$LOG_FILE' 2>&1
            "; then
                err "  build failed for $ui; keeping existing container"
                continue
            fi
            # Recreate container using spec from container-specs.sh.
            if ! sudo -u archipelago bash -c "
                export XDG_RUNTIME_DIR=/run/user/\$(id -u archipelago)
                source '$SPECS'
                load_spec_$cname || { echo 'spec load failed for $cname'; exit 1; }
                podman stop '$cname' 2>/dev/null || true
                podman rm '$cname' 2>/dev/null || true
                PORT_ARG=''
                [ -n \"\$SPEC_PORTS\" ] && PORT_ARG=\"-p \$SPEC_PORTS\"
                NET_ARG=''
                [ \"\$SPEC_NETWORK\" = 'host' ] && NET_ARG='--network host'
                CAP_ARGS='--cap-drop ALL'
                for c in \$SPEC_CAPS; do CAP_ARGS=\"\$CAP_ARGS --cap-add \$c\"; done
                podman run -d --name '$cname' \$PORT_ARG \$NET_ARG \\
                    --user 0:0 \$CAP_ARGS \\
                    --memory=\"\$SPEC_MEMORY\" \\
                    --restart unless-stopped \\
                    --security-opt \"\$SPEC_SECURITY\" \\
                    'localhost/$ui:local' >>'$LOG_FILE' 2>&1
            "; then
                err "  recreate failed for $cname"
                continue
            fi
            ok "  $cname rebuilt and running"
        done
    fi
fi

# Update kiosk display helpers used by HDMI/TV installs.
if [ -f "$REPO_DIR/image-recipe/configs/archipelago-kiosk-launcher.sh" ]; then
    sudo install -m 755 "$REPO_DIR/image-recipe/configs/archipelago-kiosk-launcher.sh" \
        /usr/local/bin/archipelago-kiosk-launcher
    ok "Updated archipelago-kiosk-launcher"
fi

# Update systemd services if changed
SYSTEMD_UNITS_CHANGED=false
for unit in archipelago.service archipelago-fips.service archipelago-kiosk.service archipelago-kiosk-watchdog.service; do
    src="$REPO_DIR/image-recipe/configs/$unit"
    dst="/etc/systemd/system/$unit"
    [ -f "$src" ] || continue
    if [ ! -f "$dst" ] || ! diff -q "$src" "$dst" &>/dev/null; then
        sudo install -m 644 "$src" "$dst"
        SYSTEMD_UNITS_CHANGED=true
        ok "Updated $unit"
    fi
done
if [ "$SYSTEMD_UNITS_CHANGED" = "true" ]; then
    sudo systemctl daemon-reload
fi

# Keep the doctor timer/service current too. Container uptime fixes rely on
# these units as much as on the helper scripts themselves.
DOCTOR_UNITS_CHANGED=false
for unit in archipelago-doctor.service archipelago-doctor.timer; do
    src="$REPO_DIR/image-recipe/configs/$unit"
    dst="/etc/systemd/system/$unit"
    [ -f "$src" ] || continue
    if [ ! -f "$dst" ] || ! diff -q "$src" "$dst" &>/dev/null; then
        sudo install -m 644 "$src" "$dst"
        DOCTOR_UNITS_CHANGED=true
        ok "Updated $unit"
    fi
done
if [ "$DOCTOR_UNITS_CHANGED" = "true" ]; then
    sudo systemctl daemon-reload
    sudo systemctl enable --now archipelago-doctor.timer 2>>"$LOG_FILE" || \
        warn "Failed to enable archipelago-doctor.timer"
fi

# Install/refresh tmpfiles.d rules. The logs rule creates
# /var/log/archipelago/ + container-installs.log with archipelago:archipelago
# ownership so the non-root backend can append install audit lines.
# Apply immediately so existing nodes don't need a reboot.
if [ -f "$REPO_DIR/image-recipe/configs/archipelago-tmpfiles.conf" ]; then
    sudo install -m 644 "$REPO_DIR/image-recipe/configs/archipelago-tmpfiles.conf" \
        /usr/lib/tmpfiles.d/archipelago-logs.conf
    sudo systemd-tmpfiles --create /usr/lib/tmpfiles.d/archipelago-logs.conf 2>/dev/null || true
    ok "Log tmpfiles rule installed"
fi

# Restart service
log "Restarting archipelago service..."
sudo systemctl restart archipelago

# Wait for health
log "Waiting for backend health..."
for i in $(seq 1 30); do
    if curl -sf http://127.0.0.1:5678/health > /dev/null 2>&1; then
        ok "Backend healthy after ${i}s"
        break
    fi
    if [ "$i" = "30" ]; then
        err "Backend failed to start within 30s"
        warn "Rolling back binary..."
        if [ -f "$BACKUP_DIR/archipelago.bak" ]; then
            sudo cp "$BACKUP_DIR/archipelago.bak" "$INSTALL_BIN"
            sudo systemctl restart archipelago
            err "Rolled back to previous binary"
        fi
        exit 1
    fi
    sleep 1
done

# Update state file for the UI
python3 -c "
import json, datetime
state = {
    'current_version': '$NEW_VERSION',
    'last_check': datetime.datetime.utcnow().isoformat() + 'Z',
    'available_update': None,
    'update_in_progress': False,
    'rollback_available': True,
    'schedule': 'daily_check'
}
with open('$STATE_FILE', 'w') as f:
    json.dump(state, f, indent=2)
" 2>/dev/null || true

echo ""
ok "Update complete: $LOCAL -> $NEW_VERSION"
log "Changelog:"
git log "$LOCAL".."$NEW_VERSION" --oneline --no-merges | head -10 | tee -a "$LOG_FILE"
