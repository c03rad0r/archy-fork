#!/bin/bash
# Quick fix to enable Archipelago auto-start on existing booted system
# Run this on the Dell OptiPlex after boot

set -e

echo "🔧 Fixing Archipelago auto-start..."

# Create the auto-start script in .bashrc
cat >> ~/.bashrc << 'EOF'

# Archipelago Auto-Start
if [ -z "$ARCHIPELAGO_STARTED" ] && [ -n "$PS1" ]; then
    export ARCHIPELAGO_STARTED=1
    
    # Find boot media
    BOOT_MEDIA=""
    for dev in /run/live/medium /lib/live/mount/medium /cdrom; do
        if [ -d "$dev/archipelago" ]; then
            BOOT_MEDIA="$dev"
            break
        fi
    done
    
    if [ -n "$BOOT_MEDIA" ]; then
        clear
        echo ""
        echo "  ╔═══════════════════════════════════════════════════════════╗"
        echo "  ║   🏝️  ARCHIPELAGO BITCOIN NODE OS                         ║"
        echo "  ╚═══════════════════════════════════════════════════════════╝"
        echo ""
        
        # Get IP
        IP=$(hostname -I 2>/dev/null | awk '{print $1}')
        [ -n "$IP" ] && echo "  🌐 Web UI: http://$IP:5678"
        echo ""
        
        # Run the setup
        bash "$BOOT_MEDIA/archipelago/setup-archipelago.sh"
    fi
fi
EOF

echo "✅ Auto-start added to .bashrc"
echo ""
echo "Now logout and login again, or run:"
echo "  source ~/.bashrc"
