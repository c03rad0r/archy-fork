#!/bin/bash
#
# Trust the Archipelago server's self-signed certificate on macOS.
# Run this to eliminate "Not secure" when accessing https://<node-host>
#
# Usage: ./scripts/trust-archipelago-cert.sh [host]
# Host is required: pass it as $1 or set ARCHY_HOST
#
# Requires: SSH access to archipelago@host (uses deploy-config.sh password)
#

set -e

HOST="${1:-${ARCHY_HOST:-}}"
if [ -z "$HOST" ]; then
  echo "usage: $0 <node-host>   (or set ARCHY_HOST)" >&2
  exit 2
fi
CERT_FILE="/tmp/archipelago-${HOST}.crt"
KEYCHAIN="${HOME}/Library/Keychains/login.keychain-db"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Try to fetch cert from server via SSH (most reliable)
SSH_KEY="${ARCHIPELAGO_SSH_KEY:-$HOME/.ssh/archipelago-deploy}"
echo "Fetching certificate from server..."
if [ -f "$SSH_KEY" ]; then
  ssh -o StrictHostKeyChecking=no -i "$SSH_KEY" archipelago@${HOST} \
    'sudo -n cat /etc/archipelago/ssl/archipelago.crt' > "$CERT_FILE" 2>/dev/null || true
elif [ -f "$SCRIPT_DIR/deploy-config.sh" ]; then
  # Last-resort fallback: password auth (leaks credentials to process list)
  . "$SCRIPT_DIR/deploy-config.sh"
  echo "WARNING: SSH key not found at $SSH_KEY — falling back to password auth"
  if command -v sshpass >/dev/null 2>&1; then
    sshpass -p "$ARCHIPELAGO_PASSWORD" ssh -o StrictHostKeyChecking=no archipelago@${HOST} \
      'sudo -n cat /etc/archipelago/ssl/archipelago.crt' > "$CERT_FILE" 2>/dev/null || true
  else
    echo "WARNING: No SSH key and sshpass not installed — skipping SSH fetch"
  fi
fi

# Fallback: fetch via openssl (can hang on some systems)
if [ ! -s "$CERT_FILE" ]; then
  echo "Fetching certificate via TLS..."
  (echo "Q"; sleep 1) | openssl s_client -connect "${HOST}:443" -servername "${HOST}" 2>/dev/null | \
    openssl x509 -outform PEM > "$CERT_FILE"
fi

if [ ! -s "$CERT_FILE" ]; then
  echo "Failed to fetch certificate. Ensure deploy-config.sh exists and SSH works, or the server is reachable."
  exit 1
fi

echo "Adding to your login keychain..."

# Remove old cert if present (by common name)
security delete-certificate -c "archipelago.local" "$KEYCHAIN" 2>/dev/null || true

# Add to user keychain with trust (no sudo needed)
if security add-trusted-cert -d -r trustRoot -k "$KEYCHAIN" "$CERT_FILE" 2>/dev/null; then
  echo "   Certificate trusted successfully."
elif security add-trusted-cert -d -r trustAsRoot -k "$KEYCHAIN" "$CERT_FILE" 2>/dev/null; then
  echo "   Certificate trusted successfully."
else
  # Fallback: add cert and open Keychain Access for manual trust
  cp "$CERT_FILE" "$HOME/Desktop/archipelago-${HOST}.crt"
  echo ""
  echo "   Could not auto-trust. Certificate saved to Desktop."
  echo "   Double-click archipelago-${HOST}.crt to add it, then in Keychain Access"
  echo "   find it, double-click, expand Trust → set to 'Always Trust'."
  CERT_FILE=""  # Don't delete, we copied to Desktop
fi

rm -f "$CERT_FILE"

echo ""
echo "✅ Done. Restart your browser fully (quit Chrome/Safari) and visit https://${HOST}"
