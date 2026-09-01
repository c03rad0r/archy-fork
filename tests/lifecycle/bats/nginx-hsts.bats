#!/usr/bin/env bats
# tests/lifecycle/bats/nginx-hsts.bats
#
# Regression guard for the 2026-09-01 framework-pt incident: the HTTPS server
# block sent `Strict-Transport-Security: max-age=31536000; includeSubDomains`.
# Browsers cached that policy, then silently upgraded the still-open
# plain-HTTP dashboard's fetches and frames to https. A scheme change makes
# the request cross-origin, so every /rpc/v1 call was CORS-blocked — the node
# looked "not responding" while being perfectly healthy, and every app frame
# died as mixed content.
#
# Plain HTTP is a SUPPORTED access mode on purpose: the node's certificate is
# optional/self-signed (Settings → Node certificate, /ca.crt flow), and
# setup-node-ca.sh deliberately keeps port 80 serving for devices that have
# not installed the CA. So this node must never pin a live HSTS policy —
# the HTTPS listener actively clears it with max-age=0 instead.
#
# Tiers: read-only (local curl + config inspection). Runs on the archy host.

@test "nginx :80 never sends a live HSTS policy" {
  local hdr
  hdr=$(curl -sD - -o /dev/null --max-time 8 http://127.0.0.1/health 2>/dev/null || true)
  if grep -qi 'Strict-Transport-Security' <<<"$hdr"; then
    grep -qi 'max-age=0' <<<"$hdr" \
      || fail ":80 answered with a live HSTS policy — an open HTTP dashboard's fetches get force-upgraded and CORS-blocked: $(grep -i 'Strict' <<<"$hdr")"
  fi
}

@test "nginx :443 actively clears HSTS (max-age=0), never pins it" {
  # The HTTPS listener binds per-LAN-address (not loopback — tailscaled owns
  # :443 on tailnet addresses), so probe the node's first global IPv4.
  local addr hdr
  addr=$(ip -o -4 addr show scope global 2>/dev/null \
    | awk '{print $4}' | cut -d/ -f1 | grep -v '^100\.' | head -1)
  [[ -n "$addr" ]] || skip "no LAN address to probe HTTPS on"
  hdr=$(curl -skD - -o /dev/null --max-time 8 "https://$addr/health" 2>/dev/null || true)
  if grep -qi 'Strict-Transport-Security' <<<"$hdr"; then
    grep -qi 'max-age=0' <<<"$hdr" \
      || fail ":443 answered with a live HSTS policy — browsers cache it and then break the HTTP dashboard: $(grep -i 'Strict' <<<"$hdr")"
  fi
}

@test "deployed nginx config contains no long-lived HSTS pin" {
  # Config-level guard: catches the pin even when no cert is installed yet
  # (no TLS listener to probe), and catches it on both server blocks.
  local conf
  for conf in /etc/nginx/sites-available/archipelago \
              /etc/nginx/sites-available/archipelago-http; do
    [[ -r "$conf" ]] || continue
    if grep -q 'Strict-Transport-Security.*max-age=31536000' "$conf"; then
      fail "$conf still pins a year-long HSTS policy (includeSubDomains class)"
    fi
  done
  true
}
