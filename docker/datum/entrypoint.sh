#!/bin/sh
set -eu
umask 077
: "${BITCOIN_RPC_HOST:?Bitcoin host is required}"
: "${BITCOIN_RPC_PASSWORD:?Bitcoin RPC password is required}"
: "${DATUM_ADMIN_PASSWORD:?Datum admin password is required}"

# Keep operator settings, including the payout address, across recreation.
# Refresh platform-owned credentials and DNS names on every container start.
config=/data/config.json
if [ ! -e "$config" ]; then printf '{}\n' > "$config"; fi
tmp=$(mktemp /data/config.json.XXXXXX)
trap 'rm -f "$tmp"' EXIT HUP INT TERM
jq -e -f /app/configure.jq "$config" > "$tmp"
mv "$tmp" "$config"
exec /app/datum_gateway --config "$config"
