#!/usr/bin/env bash
# verify-aiui-deploy.sh — post-deploy check that a marker string is actually
# reachable by a browser loading AIUI, not merely present somewhere on disk.
#
# The node's /opt/archipelago/web-ui/aiui/assets/ directory is a
# never-pruned graveyard: nginx never deletes an old build's chunk files
# when a new build lands (only the tar+rsync step below AIUI_DIST replaces
# what's there, and even that has left orphaned files in past incidents —
# see feedback_node_side_frontend_verify_stale_chunks). A disk grep over
# assets/ can therefore report "deployed" before the deploy actually
# happened, because a DEAD chunk from an old build still contains the old
# string. The only honest check fetches what a browser would actually load:
# resolve the LIVE chunk set via the service worker's precache manifest
# (sw.js), fetch each live chunk over HTTP, and grep the fetched bytes.
#
# This script never opens a remote shell session onto the node and never
# greps the node's filesystem directly — every check here is an HTTP
# fetch, exactly what a browser session would do.
#
# Usage:
#   ./verify-aiui-deploy.sh <node-host> "<marker string>"
#
# Exit 0  = the marker was found in at least one live chunk fetched over HTTP.
# Exit 1  = the marker was not found in any live chunk (negative control
#           should also produce this — a check that always passes is not a
#           check).

set -uo pipefail

HOST="${1:?usage: verify-aiui-deploy.sh <node-host> \"<marker string>\"}"
MARKER="${2:?usage: verify-aiui-deploy.sh <node-host> \"<marker string>\"}"

# Accept a bare host or a host:port; default to plain HTTP on :80, matching
# how neode-ui/AIUI are actually served on a node (nginx terminates TLS
# elsewhere; the lifecycle gate and other production-quality scripts in
# this directory talk to nodes over plain HTTP the same way).
BASE="http://${HOST}"
SW_URL="${BASE}/aiui/sw.js"

timestamp() { echo "[$(date +%H:%M:%S)]"; }

echo "$(timestamp) Fetching service worker manifest: $SW_URL"
SW_BODY="$(curl -sf -m 15 "$SW_URL" 2>/dev/null || true)"
if [ -z "$SW_BODY" ]; then
    echo "FATAL: could not fetch $SW_URL — is AIUI deployed and nginx up on $HOST?" >&2
    exit 1
fi

# vite-plugin-pwa's generateSW mode emits sw.js with a
# workbox.precacheAndRoute([{url:"...",revision:"..."|null}, ...]) call —
# a JS array literal (unquoted keys), not JSON. Extract every url:"..."
# value without a full JS parser.
mapfile -t LIVE_PATHS < <(
    grep -oE 'url:"[^"]*"' <<<"$SW_BODY" | sed -E 's/^url:"//; s/"$//'
)

if [ "${#LIVE_PATHS[@]}" -eq 0 ]; then
    echo "FATAL: $SW_URL fetched but no precache entries found — cannot resolve live chunks." >&2
    exit 1
fi

echo "$(timestamp) Resolved ${#LIVE_PATHS[@]} live chunk(s) from the precache manifest."

FOUND=0
CHECKED=0
# Fetch to a temp file and grep that, NOT `curl | grep -q`: under this
# script's pipefail, grep -q's early exit EPIPEs curl (exit 23) whenever
# the marker sits before the tail of a >64KB chunk, turning a genuine
# match into a nondeterministic FAIL.
BODY_TMP="$(mktemp)"
trap 'rm -f "$BODY_TMP"' EXIT
for path in "${LIVE_PATHS[@]}"; do
    [ -z "$path" ] && continue
    CHECKED=$((CHECKED + 1))
    url="${BASE}/aiui/${path}"
    if curl -sf -m 15 -o "$BODY_TMP" "$url" 2>/dev/null && grep -q -- "$MARKER" "$BODY_TMP"; then
        echo "$(timestamp)   MATCH: $path"
        FOUND=1
        break
    fi
done

echo "$(timestamp) Checked $CHECKED live chunk(s) fetched over HTTP for marker: $MARKER"

if [ "$FOUND" -eq 1 ]; then
    echo "$(timestamp) PASS — marker found in a live, browser-fetchable chunk."
    exit 0
else
    echo "$(timestamp) FAIL — marker not found in any live chunk (fetched via sw.js manifest, not a disk grep)." >&2
    exit 1
fi
