#!/usr/bin/env bats
# tests/lifecycle/bats/lnd-api-compat.bats
#
# Regression guard for the 2026-09-01 fleet breakage: LND 0.21 REMOVED the
# deprecated Lightning.SendPaymentSync REST route (`/v1/channels/transactions`)
# that the backend paid through — every Lightning send answered the literal
# HTTP 404 "Not Found" and the wallet UI showed "Payment failed: Not Found".
# The backend now pays via Router.SendPaymentV2 (`/v2/router/send`).
#
# This test does not send sats. It POSTs a deliberately-invalid invoice to the
# v2 route on the RUNNING LND and asserts the route itself answers: a
# 400/500 "cannot parse" proves the endpoint exists; a 404 means the pinned
# image no longer serves the route the backend calls — the exact image/backend
# skew that shipped silently last time because no gate test ever spoke the
# payment endpoint.
#
# Tiers: read-only (invalid payment request; nothing is sent).
#
# Runs on the archy host (sudo for the macaroon, curl to localhost).

LND_MAINNET_DIR="/var/lib/archipelago/lnd/data/chain/bitcoin/mainnet"

_lnd_rest_host_port() {
  local mf
  for mf in \
    "${ARCHIPELAGO_APPS_DIR:-/opt/archipelago/apps}/lnd/manifest.yml" \
    "${ARCHIPELAGO_APPS_DIR:-/opt/archipelago/apps}/lnd/manifest.yaml" \
    "$BATS_TEST_DIRNAME/../../../apps/lnd/manifest.yml"; do
    [[ -r "$mf" ]] || continue
    awk '
      /- host:/        { host=$3 }
      /container:/     { if ($2 == 8080 && host != "") { print host; exit } }
    ' "$mf"
    return 0
  done
}

@test "running LND serves /v2/router/send (the route the backend pays through)" {
  if ! podman ps --format '{{.Names}}' 2>/dev/null | grep -qx lnd; then
    skip "lnd not running"
  fi
  local port
  port=$(_lnd_rest_host_port)
  [[ -n "$port" ]] || skip "could not resolve LND REST host port from manifest"

  local mac
  mac=$(sudo cat "$LND_MAINNET_DIR/admin.macaroon" 2>/dev/null | od -An -tx1 -v | tr -d " \n")
  [[ -n "$mac" ]] || skip "LND admin macaroon not readable (LND installed but wallet not initialized?)"

  local code body
  body=$(mktemp)
  code=$(curl -sk -o "$body" -w '%{http_code}' --max-time 10 -X POST \
    -H "Grpc-Metadata-macaroon: $mac" \
    -H 'Content-Type: application/json' \
    --data '{"payment_request":"lnbc1notarealinvoice","timeout_seconds":5,"no_inflight_updates":true}' \
    "https://127.0.0.1:${port}/v2/router/send" || echo 000)
  rm -f "$body"

  # 000 = LND REST unreachable at all — that is port-drift's failure class
  # (port-drift.bats), but it also breaks payments, so fail loudly here too.
  if [[ "$code" == "000" ]]; then
    fail "LND REST not reachable on ${port} — payments cannot be sent at all"
  fi
  if [[ "$code" == "404" ]]; then
    fail "running LND does not serve /v2/router/send (HTTP 404) — the backend's payment route is gone; every Lightning send fails 'Not Found'"
  fi
}

@test "backend no longer references the removed /v1/channels/transactions route" {
  # Source-level guard: the removed route must not creep back into the
  # payment path (the runtime fix is in api/rpc/lnd/payments.rs).
  local src="$BATS_TEST_DIRNAME/../../../core/archipelago/src/api/rpc/lnd/payments.rs"
  [[ -r "$src" ]] || skip "source tree not present"
  if grep -q 'v1/channels/transactions' "$src"; then
    fail "payments.rs references /v1/channels/transactions — removed in LND 0.21, answers 404"
  fi
}
