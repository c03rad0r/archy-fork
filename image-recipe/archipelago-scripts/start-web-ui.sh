#!/bin/bash
#
# Start Archipelago Backend Server
# Serves the Vue.js UI and handles all API calls
#

# Find archipelago binary
ARCHIPELAGO_BIN=""
for path in /usr/local/bin/archipelago /opt/archipelago/bin/archipelago /run/live/medium/archipelago/bin/archipelago; do
    if [ -x "$path" ]; then
        ARCHIPELAGO_BIN="$path"
        break
    fi
done

# Get IP address
IP=$(hostname -I 2>/dev/null | awk '{print $1}')
if [ -z "$IP" ]; then
    IP="localhost"
fi

echo ""
echo "╔═══════════════════════════════════════════════════════════╗"
echo "║   🏝️  ARCHIPELAGO BITCOIN NODE OS                         ║"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""
echo "  Starting Archipelago server..."
echo ""
echo "  ┌─────────────────────────────────────────────────────────┐"
echo "  │                                                         │"
echo "  │   🌐 OPEN IN YOUR BROWSER:                             │"
echo "  │                                                         │"
echo "  │      http://$IP                                        "
echo "  │                                                         │"
echo "  └─────────────────────────────────────────────────────────┘"
echo ""

if [ -n "$ARCHIPELAGO_BIN" ]; then
    echo "  Using backend: $ARCHIPELAGO_BIN"
    echo "  Press Ctrl+C to stop"
    echo ""
    
    # Set environment for dev mode
    export ARCHIPELAGO_DEV_MODE=true
    export ARCHIPELAGO_DATA_DIR=/var/lib/archipelago
    export RUST_LOG=info
    
    # Create data directory
    sudo mkdir -p /var/lib/archipelago 2>/dev/null
    
    exec "$ARCHIPELAGO_BIN"
else
    echo "  ⚠️  Backend binary not found, using static file server"
    echo ""
    
    # Fallback to static file server
    WEB_UI_DIR=""
    for path in /opt/archipelago/web-ui /run/live/medium/archipelago/web-ui; do
        if [ -d "$path" ]; then
            WEB_UI_DIR="$path"
            break
        fi
    done
    
    if [ -n "$WEB_UI_DIR" ]; then
        cd "$WEB_UI_DIR"
        exec python3 -m http.server 80 --bind 0.0.0.0
    else
        echo "❌ Neither backend nor web UI found"
        exit 1
    fi
fi
