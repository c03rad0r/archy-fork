#!/bin/bash
# Configure Nginx to listen on Tailscale IP address
# This script should be run after Tailscale is set up and connected

set -e

echo "🔍 Detecting Tailscale IP..."

# Get Tailscale IP from tailscale0 interface
TAILSCALE_IP=$(ip -4 addr show tailscale0 2>/dev/null | grep -oP '(?<=inet\s)\d+(\.\d+){3}' || echo "")

if [ -z "$TAILSCALE_IP" ]; then
    echo "❌ Tailscale interface not found. Is Tailscale running with host networking?"
    exit 1
fi

echo "✅ Found Tailscale IP: $TAILSCALE_IP"

NGINX_CONFIG="/etc/nginx/sites-available/archipelago"

# Check if Tailscale IP is already in the config
if grep -q "listen $TAILSCALE_IP:80" "$NGINX_CONFIG"; then
    echo "✅ Nginx already configured for Tailscale IP $TAILSCALE_IP"
    exit 0
fi

echo "📝 Adding Tailscale IP to Nginx configuration..."

# Backup the config
sudo cp "$NGINX_CONFIG" "$NGINX_CONFIG.backup.$(date +%s)"

# Add Tailscale IP to listen directive (after the first "listen 80;")
sudo sed -i "0,/listen 80;/s//listen 80;\n    listen $TAILSCALE_IP:80;/" "$NGINX_CONFIG"

echo "🔍 Testing Nginx configuration..."
sudo nginx -t

echo "🔄 Reloading Nginx..."
sudo systemctl reload nginx

echo "✅ Nginx configured to accept connections from Tailscale!"
echo "   Access your Archipelago UI via Tailscale at:"
echo "   http://$(hostname).tail<your-tailnet>.ts.net/"
