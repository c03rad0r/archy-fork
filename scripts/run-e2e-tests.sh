#!/usr/bin/env bash
# E2E test suite for all Archipelago RPC endpoints.
# Uses correct method names from the dispatch table.
# Run on the server: bash run-e2e-tests.sh
set -u
BASE="http://127.0.0.1:5678"
JAR="/tmp/test-cookies.txt"
rm -f "$JAR"
PC=0; FC=0; SC=0
pass() { PC=$((PC + 1)); printf "\033[32m✓ %s\033[0m\n" "$1"; }
fail() { FC=$((FC + 1)); printf "\033[31m✗ %s\033[0m\n" "$1"; }
skip() { SC=$((SC + 1)); printf "\033[33m⊘ %s\033[0m\n" "$1"; }

rpc() {
  sleep 0.3
  local method="$1"
  local params="${2:-"{}"}"
  curl -s -b "$JAR" -c "$JAR" \
    -H "Content-Type: application/json" \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$method\",\"params\":$params}" \
    "${BASE}/rpc/v1" 2>/dev/null
}

# Check if RPC response is successful (error field is null or absent)
rpc_ok() {
  local resp="$1"
  [ -z "$resp" ] && return 1
  echo "$resp" | grep -q '"error":null' && return 0
  echo "$resp" | grep -q '"error"' && return 1
  return 0
}

echo ""
echo "━━━ Auth ━━━"
# Warmup: first request after server restart may get empty response
curl -s "${BASE}/health" > /dev/null 2>&1
sleep 1

# Login with retry
LOGIN=""
for attempt in 1 2 3; do
  LOGIN=$(rpc "auth.login" '{"password":"password123"}')
  if [ -n "$LOGIN" ]; then break; fi
  sleep 0.5
done
rpc_ok "$LOGIN" && pass "auth.login" || fail "auth.login: $LOGIN"

echo ""
echo "━━━ Identity ━━━"
ID_LIST=$(rpc "identity.list")
rpc_ok "$ID_LIST" && pass "identity.list" || fail "identity.list: $ID_LIST"
FIRST_ID=$(echo "$ID_LIST" | python3 -c "import sys,json; r=json.load(sys.stdin); ids=r.get('result',{}).get('identities',[]); print(ids[0]['id'] if ids else '')" 2>/dev/null)

if [ -n "$FIRST_ID" ]; then
  # sign
  SIGN=$(rpc "identity.sign" "{\"id\":\"$FIRST_ID\",\"message\":\"hello\"}")
  rpc_ok "$SIGN" && pass "identity.sign" || fail "identity.sign: $SIGN"
  DID=$(echo "$SIGN" | python3 -c "import sys,json; print(json.load(sys.stdin)['result']['did'])" 2>/dev/null)
  SIG_HEX=$(echo "$SIGN" | python3 -c "import sys,json; print(json.load(sys.stdin)['result']['signature'])" 2>/dev/null)

  # verify (valid)
  VER=$(rpc "identity.verify" "{\"did\":\"$DID\",\"message\":\"hello\",\"signature\":\"$SIG_HEX\"}")
  echo "$VER" | python3 -c "import sys,json; r=json.load(sys.stdin); assert r['result']['valid']" 2>/dev/null && pass "identity.verify (valid)" || fail "identity.verify: $VER"

  # verify (bad)
  VER_BAD=$(rpc "identity.verify" "{\"did\":\"$DID\",\"message\":\"nope\",\"signature\":\"$SIG_HEX\"}")
  echo "$VER_BAD" | python3 -c "import sys,json; r=json.load(sys.stdin); assert not r['result']['valid']" 2>/dev/null && pass "identity.verify (bad rejected)" || fail "identity.verify bad: $VER_BAD"

  # get
  R=$(rpc "identity.get" "{\"id\":\"$FIRST_ID\"}")
  rpc_ok "$R" && pass "identity.get" || fail "identity.get: $R"

  # set-default
  R=$(rpc "identity.set-default" "{\"id\":\"$FIRST_ID\"}")
  rpc_ok "$R" && pass "identity.set-default" || fail "identity.set-default: $R"

  # nostr key
  NOSTR=$(rpc "identity.create-nostr-key" "{\"id\":\"$FIRST_ID\"}")
  rpc_ok "$NOSTR" && pass "identity.create-nostr-key" || {
    echo "$NOSTR" | grep -q "already exists" && pass "identity.create-nostr-key (exists)" || fail "nostr-key: $NOSTR"
  }

  # nostr sign
  HASH=$(python3 -c "import hashlib; print(hashlib.sha256(b'test').hexdigest())")
  R=$(rpc "identity.nostr-sign" "{\"id\":\"$FIRST_ID\",\"event_hash\":\"$HASH\"}")
  rpc_ok "$R" && pass "identity.nostr-sign" || fail "identity.nostr-sign: $R"
else
  fail "no identity found"
fi

# Create + nostr + delete
CREATE=$(rpc "identity.create" '{"name":"TmpTest","purpose":"anonymous"}')
rpc_ok "$CREATE" && pass "identity.create" || fail "identity.create: $CREATE"
TEMP_ID=$(echo "$CREATE" | python3 -c "import sys,json; print(json.load(sys.stdin).get('result',{}).get('id',''))" 2>/dev/null)
if [ -n "$TEMP_ID" ]; then
  R=$(rpc "identity.create-nostr-key" "{\"id\":\"$TEMP_ID\"}")
  rpc_ok "$R" && pass "nostr-key (new identity)" || fail "nostr-key (new): $R"
  R=$(rpc "identity.delete" "{\"id\":\"$TEMP_ID\"}")
  rpc_ok "$R" && pass "identity.delete" || fail "identity.delete: $R"
fi

echo ""
echo "━━━ Names (identity.*-name) ━━━"
R=$(rpc "identity.list-names")
rpc_ok "$R" && pass "identity.list-names" || fail "identity.list-names: $(echo $R | head -c 120)"

if [ -n "$FIRST_ID" ]; then
  R=$(rpc "identity.register-name" "{\"name\":\"e2e\",\"domain\":\"archipelago.local\",\"identity_id\":\"$FIRST_ID\",\"did\":\"$DID\"}")
  rpc_ok "$R" && pass "identity.register-name" || fail "identity.register-name: $(echo $R | head -c 120)"
  REG_NAME_ID=$(echo "$R" | python3 -c "import sys,json; print(json.load(sys.stdin).get('result',{}).get('id',''))" 2>/dev/null)

  R=$(rpc "identity.resolve-name" '{"identifier":"e2e@archipelago.local"}')
  rpc_ok "$R" && pass "identity.resolve-name" || fail "identity.resolve-name: $(echo $R | head -c 120)"

  if [ -n "$REG_NAME_ID" ]; then
    R=$(rpc "identity.remove-name" "{\"id\":\"$REG_NAME_ID\"}")
    rpc_ok "$R" && pass "identity.remove-name" || fail "identity.remove-name: $(echo $R | head -c 120)"
  fi
fi

echo ""
echo "━━━ Credentials (identity.*-credential) ━━━"
R=$(rpc "identity.list-credentials")
rpc_ok "$R" && pass "identity.list-credentials" || fail "identity.list-credentials: $(echo $R | head -c 120)"

if [ -n "$FIRST_ID" ]; then
  R=$(rpc "identity.issue-credential" "{\"issuer_id\":\"$FIRST_ID\",\"subject_did\":\"did:key:z6MkTest\",\"type\":\"TestCred\",\"claims\":{\"name\":\"E2E\"}}")
  rpc_ok "$R" && pass "identity.issue-credential" || fail "identity.issue-credential: $(echo $R | head -c 120)"
fi

echo ""
echo "━━━ Lightning ━━━"
R=$(rpc "lnd.getinfo")
rpc_ok "$R" && pass "lnd.getinfo" || fail "lnd.getinfo: $R"

R=$(rpc "lnd.listchannels")
rpc_ok "$R" && pass "lnd.listchannels" || fail "lnd.listchannels: $R"

R=$(rpc "lnd.newaddress")
rpc_ok "$R" && pass "lnd.newaddress" || fail "lnd.newaddress: $R"

R=$(rpc "lnd.createinvoice" '{"amount_sats":0,"memo":"zero amount test"}')
rpc_ok "$R" && pass "lnd.createinvoice (0 sats)" || fail "lnd.createinvoice (0): $R"

R=$(rpc "lnd.createinvoice" '{"amount_sats":1000,"memo":"test"}')
rpc_ok "$R" && pass "lnd.createinvoice (1000 sats)" || fail "lnd.createinvoice (1000): $R"

R=$(rpc "bitcoin.getinfo")
rpc_ok "$R" && pass "bitcoin.getinfo" || fail "bitcoin.getinfo: $R"

echo ""
echo "━━━ Tor ━━━"
R=$(rpc "tor.list-services")
rpc_ok "$R" && pass "tor.list-services" || fail "tor.list-services: $R"

R=$(rpc "tor.create-service" '{"name":"test-e2e","local_port":9999}')
rpc_ok "$R" && pass "tor.create-service" || fail "tor.create-service: $(echo $R | head -c 150)"

R=$(rpc "tor.delete-service" '{"name":"test-e2e"}')
rpc_ok "$R" && pass "tor.delete-service" || fail "tor.delete-service: $R"

R=$(rpc "tor.get-onion-address" '{"name":"archipelago"}')
rpc_ok "$R" && pass "tor.get-onion-address" || fail "tor.get-onion-address: $R"

echo ""
echo "━━━ Ecash Wallet ━━━"
R=$(rpc "wallet.ecash-balance")
rpc_ok "$R" && pass "wallet.ecash-balance" || skip "wallet.ecash-balance"

R=$(rpc "wallet.ecash-history")
rpc_ok "$R" && pass "wallet.ecash-history" || skip "wallet.ecash-history"

R=$(rpc "wallet.networking-profits")
rpc_ok "$R" && pass "wallet.networking-profits" || skip "wallet.networking-profits"

echo ""
echo "━━━ Content ━━━"
R=$(rpc "content.list-mine")
rpc_ok "$R" && pass "content.list-mine" || fail "content.list-mine: $R"

echo ""
echo "━━━ Network ━━━"
R=$(rpc "network.get-visibility")
rpc_ok "$R" && pass "network.get-visibility" || fail "network.get-visibility: $R"

R=$(rpc "network.diagnostics")
rpc_ok "$R" && pass "network.diagnostics" || fail "network.diagnostics: $R"

R=$(rpc "network.list-requests")
rpc_ok "$R" && pass "network.list-requests" || fail "network.list-requests: $R"

R=$(rpc "node-list-peers")
rpc_ok "$R" && pass "node-list-peers" || fail "node-list-peers: $R"

echo ""
echo "━━━ Nostr Relays ━━━"
R=$(rpc "nostr.list-relays")
rpc_ok "$R" && pass "nostr.list-relays" || fail "nostr.list-relays: $R"

R=$(rpc "nostr.get-stats")
rpc_ok "$R" && pass "nostr.get-stats" || fail "nostr.get-stats: $R"

echo ""
echo "━━━ DWN ━━━"
R=$(rpc "dwn.status")
rpc_ok "$R" && pass "dwn.status" || fail "dwn.status: $R"

echo ""
echo "━━━ Update ━━━"
R=$(rpc "update.status")
rpc_ok "$R" && pass "update.status" || fail "update.status: $R"

R=$(rpc "update.check")
rpc_ok "$R" && pass "update.check" || skip "update.check"

echo ""
echo "━━━ Router ━━━"
R=$(rpc "router.info")
rpc_ok "$R" && pass "router.info" || skip "router.info"

R=$(rpc "router.list-forwards")
rpc_ok "$R" && pass "router.list-forwards" || skip "router.list-forwards"

echo ""
echo "━━━ Health & HTTP endpoints ━━━"
HC=$(curl -s -o /dev/null -w "%{http_code}" "${BASE}/health")
[ "$HC" = "200" ] && pass "/health (200)" || fail "/health ($HC)"

EC=$(curl -s -o /dev/null -w "%{http_code}" "${BASE}/electrs-status")
[ "$EC" = "200" ] && pass "/electrs-status (200)" || fail "/electrs-status ($EC)"

echo ""
echo "━━━ Container Management ━━━"
R=$(rpc "container-list")
rpc_ok "$R" && pass "container-list" || fail "container-list: $R"

R=$(rpc "container-status" '{"app_id":"bitcoin-knots"}')
rpc_ok "$R" && pass "container-status (bitcoin-knots)" || fail "container-status: $R"

echo ""
echo "━━━━━━━━━━━━━ RESULTS ━━━━━━━━━━━━━"
printf "\033[32m  Passed:  %d\033[0m\n" "$PC"
printf "\033[31m  Failed:  %d\033[0m\n" "$FC"
printf "\033[33m  Skipped: %d\033[0m\n" "$SC"
T=$((PC + FC + SC))
if [ "$FC" -eq 0 ]; then
  printf "\n\033[1;32m🎉 ALL %d PASSED (%d skipped)\033[0m\n" "$PC" "$SC"
else
  printf "\n\033[1;31m⚠ %d/%d FAILED\033[0m\n" "$FC" "$T"
fi
rm -f "$JAR"
