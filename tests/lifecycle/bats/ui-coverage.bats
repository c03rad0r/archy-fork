#!/usr/bin/env bats
# tests/lifecycle/bats/ui-coverage.bats
#
# UI surface tests — exercises the URLs a real user actually clicks
# through, not just the JSON-RPC API. Fills the coverage gap where the
# previous bats suites would report "container is up" while the iframe
# behind /app/<id>/ was returning 502 because nginx had a stale upstream
# or the proxy port was wrong.
#
# URL map sourced from neode-ui/src/views/appSession/appSessionConfig.ts
# (the frontend's own resolveAppUrl). Tests here MUST stay in sync with
# that file — divergence is the whole bug class we're guarding against.
#
# Each app probe is gated on its container being running:
#   - container down  → skip (clean dependency report, no false-fail)
#   - container up    → URL MUST return 200 with non-empty body
#
# Looped 5× via tests/lifecycle/run-gate.sh.

load '../lib/rpc.bash'
load '../lib/ui-probes.bash'

setup_file() {
  : "${ARCHY_PASSWORD:?Set ARCHY_PASSWORD env var to the UI password}"
  export ARCHY_FORCE_LOGIN=1
  rpc_login
  unset ARCHY_FORCE_LOGIN
  HOST="${ARCHY_HOST:-127.0.0.1}"
  export HOST
  # Honour ARCHY_SCHEME like lib/rpc.bash does, defaulting to https so nodes
  # that already pass keep testing the TLS path. These probes used to hardcode
  # https, which made the whole suite unrunnable on a node that serves the
  # dashboard over http: on archi-dev-box :443 is bound to the Tailscale /
  # WireGuard / LAN interface addresses but NOT to loopback, while :80 is bound
  # on 0.0.0.0 — so every proxy probe failed "curl failed (network/timeout)"
  # against http-reachable endpoints that were in fact serving 200.
  UI_BASE="${ARCHY_SCHEME:-https}://$HOST"
  export UI_BASE
}

teardown_file() {
  rpc_logout_local
}

# ────────────────────────────────────────────────────────────────────
# Dashboard shell + catalog (always required)
# ────────────────────────────────────────────────────────────────────

@test "dashboard / returns the Vue SPA shell" {
  run probe_dashboard_shell
  [ "$status" -eq 0 ]
}

@test "dashboard catalog endpoint responds with apps" {
  run probe_dashboard_catalog
  [ "$status" -eq 0 ]
}

# ────────────────────────────────────────────────────────────────────
# Bitcoin UI — direct host port (8334), companion container
# ────────────────────────────────────────────────────────────────────

@test "bitcoin-ui is reachable on :8334 when archy-bitcoin-ui is running" {
  probe_app_url archy-bitcoin-ui "http://$HOST:8334/" "bitcoin-ui (direct port 8334)"
}

# ────────────────────────────────────────────────────────────────────
# HTTPS proxy paths — match HTTPS_PROXY_PATHS in appSessionConfig.ts
# ────────────────────────────────────────────────────────────────────

@test "lnd proxy /app/lnd/ responds when lnd is running" {
  probe_app_url lnd "$UI_BASE/app/lnd/" "lnd (proxy /app/lnd/)"
}

@test "electrumx proxy /app/electrumx/ responds when electrumx is running" {
  # electrumx companion (archy-electrs-ui) is what serves the iframe HTML;
  # the electrumx daemon is just the TCP backend.
  probe_app_url archy-electrs-ui "$UI_BASE/app/electrumx/" "electrumx (proxy /app/electrumx/)"
}

@test "mempool proxy /app/mempool/ responds when mempool is running" {
  probe_app_url mempool "$UI_BASE/app/mempool/" "mempool (proxy /app/mempool/)"
}

@test "fedimint proxy /app/fedimint/ responds when fedimint is running" {
  probe_app_url fedimint "$UI_BASE/app/fedimint/" "fedimint (proxy /app/fedimint/)"
}

@test "btcpay proxy /app/btcpay/ responds when btcpay-server is running" {
  probe_app_url btcpay-server "$UI_BASE/app/btcpay/" "btcpay (proxy /app/btcpay/)"
}

@test "filebrowser proxy /app/filebrowser/ responds when filebrowser is running" {
  probe_app_url filebrowser "$UI_BASE/app/filebrowser/" "filebrowser (proxy /app/filebrowser/)"
}

# ────────────────────────────────────────────────────────────────────
# Companion-served URLs that aren't in HTTPS_PROXY_PATHS but show up
# in the dashboard. archy-lnd-ui shares lnd's iframe path; archy-electrs-ui
# shares electrumx's. The earlier test already covers those — leaving
# this section for future companion-direct probes (none today).
# ────────────────────────────────────────────────────────────────────
