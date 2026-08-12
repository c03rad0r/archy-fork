#!/bin/bash
#
# LND (Lightning Network Daemon) Setup Script for Archipelago
# Sets up LND in a Podman container
#

set -e

echo ""
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║   ⚡ LND (LIGHTNING) SETUP                                ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""

# Check for Podman
if ! command -v podman >/dev/null 2>&1; then
    echo "❌ Podman not found. Please run setup-bitcoin.sh first."
    exit 1
fi

# Check if Bitcoin Core is running
if ! podman ps | grep -q bitcoind; then
    echo "⚠️  Bitcoin Core is not running."
    echo "   LND requires a synced Bitcoin Core node."
    read -p "Continue anyway? [y/N]: " CONTINUE
    if [[ ! "$CONTINUE" =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Create data directory
LND_DATA="${HOME}/.lnd"
mkdir -p "$LND_DATA"

echo "📍 LND data directory: $LND_DATA"
echo ""

# Get Bitcoin RPC credentials
BITCOIN_CONF="${HOME}/.bitcoin/bitcoin.conf"
if [ -f "$BITCOIN_CONF" ]; then
    RPC_USER=$(grep "^rpcuser=" "$BITCOIN_CONF" | cut -d= -f2)
    RPC_PASS=$(grep "^rpcpassword=" "$BITCOIN_CONF" | cut -d= -f2)
else
    read -p "Bitcoin RPC username: " RPC_USER
    read -p "Bitcoin RPC password: " RPC_PASS
fi

# Ask for LND configuration
echo "LND Configuration:"
echo ""
read -p "Node alias (public name): " LND_ALIAS
LND_ALIAS=${LND_ALIAS:-archipelago-node}

# Create lnd.conf
cat > "$LND_DATA/lnd.conf" <<EOF
# Archipelago LND Configuration

[Application Options]
alias=$LND_ALIAS
color=#FF9900
listen=0.0.0.0:9735
rpclisten=0.0.0.0:10009
restlisten=0.0.0.0:8080

# Automatically unlock wallet (create password file after first run)
# wallet-unlock-password-file=/data/.lnd/password.txt

[Bitcoin]
bitcoin.active=true
bitcoin.mainnet=true
bitcoin.node=bitcoind

[Bitcoind]
bitcoind.rpchost=host.containers.internal:8332
bitcoind.rpcuser=$RPC_USER
bitcoind.rpcpass=$RPC_PASS
bitcoind.zmqpubrawblock=tcp://host.containers.internal:28332
bitcoind.zmqpubrawtx=tcp://host.containers.internal:28333

[tor]
tor.active=false

[wtclient]
wtclient.active=true
EOF

echo "📋 Created lnd.conf"
echo ""

# Pull LND image
echo "🐳 Pulling LND container image..."
podman pull docker.io/lightninglabs/lnd:v0.18.0-beta

# Create systemd user service for LND
mkdir -p ~/.config/systemd/user

cat > ~/.config/systemd/user/lnd.service <<EOF
[Unit]
Description=LND Lightning Network Daemon (Podman)
After=bitcoind.service
Requires=bitcoind.service

[Service]
Type=simple
ExecStartPre=-/usr/bin/podman stop lnd
ExecStartPre=-/usr/bin/podman rm lnd
ExecStart=/usr/bin/podman run --name lnd \\
    --rm \\
    --add-host=host.containers.internal:host-gateway \\
    -v ${LND_DATA}:/data/.lnd:Z \\
    -p 9735:9735 \\
    -p 10009:10009 \\
    -p 18080:8080 \\
    docker.io/lightninglabs/lnd:v0.18.0-beta \\
    --configfile=/data/.lnd/lnd.conf
ExecStop=/usr/bin/podman stop lnd
Restart=always
RestartSec=10

[Install]
WantedBy=default.target
EOF

echo "📋 Created systemd service"
echo ""

# Enable service
systemctl --user daemon-reload
systemctl --user enable lnd.service

echo ""
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║   ✅ LND SETUP COMPLETE!                                  ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""
echo "Commands:"
echo "  Start:   systemctl --user start lnd"
echo "  Stop:    systemctl --user stop lnd"
echo "  Status:  systemctl --user status lnd"
echo "  Logs:    podman logs -f lnd"
echo ""
echo "LND CLI:"
echo "  podman exec lnd lncli --network=mainnet getinfo"
echo ""
echo "⚠️  IMPORTANT: On first start, you need to create a wallet:"
echo "  1. Start LND: systemctl --user start lnd"
echo "  2. Create wallet: podman exec -it lnd lncli create"
echo "  3. Save your seed phrase securely!"
echo ""

read -p "Start LND now? [Y/n]: " START_NOW
if [[ ! "$START_NOW" =~ ^[Nn]$ ]]; then
    echo ""
    echo "🚀 Starting LND..."
    systemctl --user start lnd
    sleep 3
    echo ""
    echo "LND is starting. Create your wallet with:"
    echo "  podman exec -it lnd lncli create"
fi
