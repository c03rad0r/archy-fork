#!/usr/bin/env bash
# One-shot node-side repair: pull the current companion-UI manifests
# (session_passthrough on the gated ports) from the public repo, install
# them into every location the daemon reads, restart, and report.
#
# Run on a node:
#   curl -sf https://source.archipelago-foundation.org/lfg2025/archy/raw/branch/main/scripts/fix-companion-manifests.sh | bash
#
# Idempotent and safe to re-run. Needs passwordless sudo (fleet default).
set -u

BASE="https://source.archipelago-foundation.org/lfg2025/archy/raw/branch/main/apps"
RUNTIME="/opt/archipelago/web-ui/archipelago-runtime/apps"
updated=0

for app in lnd-ui bitcoin-ui electrs-ui fips-ui; do
    tmp="/tmp/${app}-manifest.yml"
    if ! curl -sf --max-time 30 "$BASE/$app/manifest.yml" -o "$tmp"; then
        echo "✗ $app: download failed"; continue
    fi
    if ! grep -q session_passthrough "$tmp"; then
        echo "✗ $app: fetched file missing session_passthrough — refusing"; continue
    fi
    sudo cp "$tmp" "/opt/archipelago/apps/$app/manifest.yml" || { echo "✗ $app: install failed"; continue; }
    # The frontend's runtime payload is restored over /opt/archipelago/apps at
    # every daemon boot on nodes that carry it — update it too or the fix
    # reverts on the next restart.
    if [ -d "$RUNTIME/$app" ]; then
        sudo cp "$tmp" "$RUNTIME/$app/manifest.yml"
    fi
    echo "✓ $app updated"
    updated=$((updated + 1))
done

if [ "$updated" -eq 0 ]; then
    echo "Nothing updated — not restarting."
    exit 1
fi

sudo systemctl restart archipelago
echo "Daemon restarted; waiting for the gate…"
sleep 15
ip=$(hostname -I | tr ' ' '\n' | grep '^100\.' | head -1)
code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 5 "http://$ip:18083/" 2>/dev/null)
echo "ext :18083 -> $code  (401 = gate holds the port: CORRECT)"
