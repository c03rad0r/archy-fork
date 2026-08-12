#!/bin/bash
# Smoke test for Archipelago — verifies critical endpoints
# Usage: ./scripts/smoke-test.sh [host]
# Exit 0 if all pass, exit 1 on any failure.

set -euo pipefail

HOST="${1:-${ARCHY_HOST:-}}"
if [ -z "$HOST" ]; then
  echo "usage: $0 <node-host>   (or set ARCHY_HOST)" >&2
  exit 2
fi
PASS=0
FAIL=0
FAILURES=""

check() {
  local name="$1" cmd="$2"
  if eval "$cmd" >/dev/null 2>&1; then
    echo "  ✓ $name"
    PASS=$((PASS + 1))
  else
    echo "  ✗ $name"
    FAIL=$((FAIL + 1))
    FAILURES="$FAILURES\n  - $name"
  fi
}

echo "=== Archipelago Smoke Test ==="
echo "Target: $HOST"
echo ""

# 1. Health endpoint
check "GET /health returns OK" \
  "curl -sf http://${HOST}/health | grep -q '\"status\"'"

# 2. Login via RPC
SESSION=$(curl -sf -X POST "http://${HOST}/rpc/v1" \
  -H 'Content-Type: application/json' \
  -d '{"method":"auth.login","params":{"password":"'"${TEST_PASSWORD:-password123}"'"}}' \
  -c - 2>/dev/null | grep session | awk '{print $NF}' || echo "")

if [ -n "$SESSION" ]; then
  echo "  ✓ Login via RPC"
  PASS=$((PASS + 1))
else
  echo "  ✗ Login via RPC"
  FAIL=$((FAIL + 1))
  FAILURES="$FAILURES\n  - Login via RPC"
fi

# 3. Authenticated RPC call
check "server.get-info returns JSON" \
  "curl -sf -X POST http://${HOST}/rpc/v1 \
    -H 'Content-Type: application/json' \
    -b 'session=${SESSION}' \
    -d '{\"method\":\"server.get-info\"}' | grep -q '\"result\"'"

# 4. Container list
check "container.list returns JSON" \
  "curl -sf -X POST http://${HOST}/rpc/v1 \
    -H 'Content-Type: application/json' \
    -b 'session=${SESSION}' \
    -d '{\"method\":\"container.list\"}' | grep -q '\"result\"'"

# 5. WebSocket upgrade
check "WebSocket upgrade (101)" \
  "curl -sf -o /dev/null -w '%{http_code}' \
    -H 'Upgrade: websocket' -H 'Connection: Upgrade' \
    -H 'Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==' \
    -H 'Sec-WebSocket-Version: 13' \
    http://${HOST}/ws/db | grep -q '101'"

# 6. Static assets served
check "Frontend index.html served" \
  "curl -sf http://${HOST}/ | grep -q '<div id=\"app\"'"

# 7. Onboarding check (unauthenticated)
check "auth.isOnboardingComplete RPC" \
  "curl -sf -X POST http://${HOST}/rpc/v1 \
    -H 'Content-Type: application/json' \
    -d '{\"method\":\"auth.isOnboardingComplete\"}' | grep -q '\"result\"'"

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
if [ $FAIL -gt 0 ]; then
  echo -e "Failures:$FAILURES"
  exit 1
fi
exit 0
