#!/bin/bash
#
# Bitcoin Core Setup Script for Archipelago
# Sets up Bitcoin Core in a Podman container
#

set -e

echo ""
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║   ₿  BITCOIN CORE SETUP                                   ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""

# Check if running as root
if [ "$EUID" -eq 0 ]; then
    echo "⚠️  Running as root. For rootless Podman, run as regular user."
fi

# Check for Podman
if ! command -v podman >/dev/null 2>&1; then
    echo "📦 Installing Podman..."
    sudo apt-get update
    sudo apt-get install -y podman podman-compose
fi

# Create data directory
BITCOIN_DATA="${HOME}/.bitcoin"
mkdir -p "$BITCOIN_DATA"

echo "📍 Bitcoin data directory: $BITCOIN_DATA"
echo ""

# Ask for configuration
echo "Bitcoin Core Configuration:"
echo ""
read -p "Enable pruning? (saves disk space) [y/N]: " PRUNE
read -p "Enable txindex? (required for some apps) [y/N]: " TXINDEX
read -p "RPC username [bitcoin]: " RPC_USER
RPC_USER=${RPC_USER:-bitcoin}
read -p "RPC password [randomly generated]: " RPC_PASS

if [ -z "$RPC_PASS" ]; then
    RPC_PASS=$(openssl rand -hex 16)
    echo "   Generated RPC password: $RPC_PASS"
fi

# Create bitcoin.conf
cat > "$BITCOIN_DATA/bitcoin.conf" <<EOF
# Archipelago Bitcoin Core Configuration

# Network
server=1
listen=1

# RPC
rpcuser=$RPC_USER
rpcpassword=$RPC_PASS
rpcallowip=10.0.0.0/8
rpcallowip=172.16.0.0/12
rpcallowip=192.168.0.0/16

# Performance
dbcache=450
maxmempool=300

EOF

if [[ "$PRUNE" =~ ^[Yy]$ ]]; then
    echo "prune=550" >> "$BITCOIN_DATA/bitcoin.conf"
    echo "   Pruning enabled (550MB)"
fi

if [[ "$TXINDEX" =~ ^[Yy]$ ]]; then
    echo "txindex=1" >> "$BITCOIN_DATA/bitcoin.conf"
    echo "   Transaction index enabled"
fi

echo ""
echo "📋 Created bitcoin.conf"
echo ""

# Pull Bitcoin Core image
echo "🐳 Pulling Bitcoin Core container image..."
podman pull docker.io/lncm/bitcoind:v27.0

# Create systemd user service for Bitcoin Core
mkdir -p ~/.config/systemd/user

cat > ~/.config/systemd/user/bitcoind.service <<EOF
[Unit]
Description=Bitcoin Core (Podman)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStartPre=-/usr/bin/podman stop bitcoind
ExecStartPre=-/usr/bin/podman rm bitcoind
ExecStart=/usr/bin/podman run --name bitcoind \\
    --rm \\
    -v ${BITCOIN_DATA}:/data/.bitcoin:Z \\
    -p 8332:8332 \\
    -p 8333:8333 \\
    docker.io/lncm/bitcoind:v27.0
ExecStop=/usr/bin/podman stop bitcoind
Restart=always
RestartSec=10

[Install]
WantedBy=default.target
EOF

echo "📋 Created systemd service"
echo ""

# Enable and start service
systemctl --user daemon-reload
systemctl --user enable bitcoind.service

echo ""
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║   ✅ BITCOIN CORE SETUP COMPLETE!                         ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""
echo "Commands:"
echo "  Start:   systemctl --user start bitcoind"
echo "  Stop:    systemctl --user stop bitcoind"
echo "  Status:  systemctl --user status bitcoind"
echo "  Logs:    podman logs -f bitcoind"
echo ""
echo "Bitcoin CLI:"
echo "  podman exec bitcoind bitcoin-cli -getinfo"
echo ""
echo "RPC Credentials (save these!):"
echo "  User: $RPC_USER"
echo "  Pass: $RPC_PASS"
echo ""

read -p "Start Bitcoin Core now? [Y/n]: " START_NOW
if [[ ! "$START_NOW" =~ ^[Nn]$ ]]; then
    echo ""
    echo "🚀 Starting Bitcoin Core..."
    systemctl --user start bitcoind
    echo ""
    echo "Bitcoin Core is syncing. This will take several hours/days."
    echo "Monitor with: podman logs -f bitcoind"
fi
