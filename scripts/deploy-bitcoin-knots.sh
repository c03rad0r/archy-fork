#!/bin/bash
#
# Complete Bitcoin Knots Deployment for Archipelago
# This script deploys Bitcoin Knots with a working web UI
# 
# For production/beta releases, this needs to be captured in the auto-installer
# or provided as a one-click install in the App Store
#

set -e

# Source pinned image versions (single source of truth)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
[ -f "$SCRIPT_DIR/image-versions.sh" ] && . "$SCRIPT_DIR/image-versions.sh"

# Read per-installation Bitcoin RPC credentials
SECRETS_DIR="/var/lib/archipelago/secrets"
sudo mkdir -p "$SECRETS_DIR" && sudo chmod 700 "$SECRETS_DIR"
if [ ! -f "$SECRETS_DIR/bitcoin-rpc-password" ]; then
    openssl rand -base64 24 | sudo tee "$SECRETS_DIR/bitcoin-rpc-password" > /dev/null
    sudo chmod 600 "$SECRETS_DIR/bitcoin-rpc-password"
fi
BITCOIN_RPC_USER="archipelago"
BITCOIN_RPC_PASS=$(sudo cat "$SECRETS_DIR/bitcoin-rpc-password")

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║  Deploying Bitcoin Knots with Web UI                          ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Step 1: Create data directory
echo "📁 Creating Bitcoin data directory..."
sudo mkdir -p /var/lib/archipelago/bitcoin
echo "   ✅ Directory created"

# Step 2: Deploy Bitcoin Knots node
echo ""
echo "₿ Deploying Bitcoin Knots node..."
podman run -d \
  --name bitcoin-knots \
  --restart unless-stopped \
  -p 8332:8332 \
  -p 8333:8333 \
  -v /var/lib/archipelago/bitcoin:/home/bitcoin/.bitcoin \
  --label "com.archipelago.app=bitcoin-knots" \
  --label "com.archipelago.title=Bitcoin Knots" \
  --label "com.archipelago.version=28.1" \
  --label "com.archipelago.category=bitcoin" \
  --label "com.archipelago.description.short=Full Bitcoin node implementation" \
  --label "com.archipelago.description.long=Bitcoin Knots is a derivative of Bitcoin Core with additional features and bug fixes. Maintain the full blockchain and validate all transactions." \
  --label "com.archipelago.license=MIT" \
  --label "com.archipelago.icon=/assets/img/app-icons/bitcoin-knots.webp" \
  --label "com.archipelago.port=8332" \
  --label "com.archipelago.repo=https://github.com/bitcoinknots/bitcoin" \
  "${BITCOIN_KNOTS_IMAGE}" \
  -server=1 \
  -txindex=1 \
  -rpcallowip=127.0.0.1/32 -rpcallowip=10.88.0.0/16 \
  -rpcbind=0.0.0.0:8332 \
  -rpcuser=archipelago \
  -rpcpassword=$BITCOIN_RPC_PASS \
  -dbcache=4096

echo "   ✅ Bitcoin Knots node starting"

# Step 3: Build and deploy web UI
echo ""
echo "🌐 Building Bitcoin Knots web UI..."

# Create temporary build directory
BUILD_DIR="/tmp/bitcoin-ui-build"
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

# Create Dockerfile
cat > "$BUILD_DIR/Dockerfile" << 'EOF'
FROM ${NGINX_ALPINE_IMAGE:-source.archipelago-foundation.org/lfg2025/nginx:1.29.6-alpine}

# Copy the static UI
COPY index.html /usr/share/nginx/html/

# Create assets directories
RUN mkdir -p /usr/share/nginx/html/assets/img/app-icons && \
    mkdir -p /usr/share/nginx/html/assets/img

EXPOSE 80

CMD ["nginx", "-g", "daemon off;"]
EOF

# Copy UI file from the project
# For beta: this needs to be included in the ISO or downloadable
cp /home/archipelago/archy/docker/bitcoin-ui/index.html "$BUILD_DIR/"

# Build the image
podman build -t localhost/bitcoin-ui:local "$BUILD_DIR"

# Deploy UI container
podman run -d \
  --name bitcoin-ui \
  --restart unless-stopped \
  -p 8334:80 \
  --label "com.archipelago.app=bitcoin-ui" \
  --label "com.archipelago.parent=bitcoin-knots" \
  localhost/bitcoin-ui:local

echo "   ✅ Bitcoin UI deployed on port 8334"

# Cleanup
rm -rf "$BUILD_DIR"

# Step 4: Wait for backend to detect
echo ""
echo "⏳ Waiting for backend to detect containers..."
sleep 5

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║  ✅ BITCOIN KNOTS DEPLOYED!                                   ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "📊 Status:"
podman ps | grep bitcoin
echo ""
echo "🌐 Access:"
echo "   • Web UI:  http://YOUR-SERVER-IP:8334"
echo "   • RPC:     http://localhost:8332"
echo "   • Network: Port 8333 (Bitcoin P2P)"
echo ""
echo "📝 RPC Credentials:"
echo "   • User: archipelago"
echo "   • Pass: (stored in /var/lib/archipelago/secrets/bitcoin-rpc-password)"
echo ""
echo "⏰ Blockchain sync will take several hours to days."
echo "   Check progress: podman logs -f bitcoin-knots"
echo ""
