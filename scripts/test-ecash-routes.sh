#!/usr/bin/env bash
# Headless exercise of every Cashu route the node exposes.
#
# Runs against the node's RPC exactly as the UI does, so it covers the real
# path: dispatcher -> handler -> wallet -> mint HTTP. Intended to be run on
# the node itself.
#
#   ./scripts/test-ecash-routes.sh                  # uses testnet (safe)
#   ECASH_TEST_NETWORK=mainnet ./scripts/...        # REAL COINS, opt-in only
#
# Requires: the node's web password.
#   ARCHY_PASSWORD='...' ./scripts/test-ecash-routes.sh
#
# Exit code is the number of failed checks, so CI can gate on it.

set -uo pipefail

HOST="${ARCHY_HOST:-127.0.0.1}"
SCHEME="${ARCHY_SCHEME:-http}"
BASE="$SCHEME://$HOST"
NETWORK="${ECASH_TEST_NETWORK:-testnet}"
JAR="$(mktemp -t ecash-routes-XXXXXX.jar)"
trap 'rm -f "$JAR"' EXIT

PASS=0
FAIL=0
ORIGINAL_NETWORK=""

log()  { printf '%s\n' "$*"; }
ok()   { PASS=$((PASS+1)); printf '  \033[32mPASS\033[0m %s\n' "$*"; }
bad()  { FAIL=$((FAIL+1)); printf '  \033[31mFAIL\033[0m %s\n' "$*"; }

# rpc <method> [json-params] -> prints the full JSON-RPC response
rpc() {
  local method="$1" params="${2:-}" body csrf
  csrf="$(awk '/csrf_token/{print $NF}' "$JAR" 2>/dev/null | tail -1)"
  if [ -n "$params" ]; then
    body="{\"method\":\"$method\",\"params\":$params}"
  else
    body="{\"method\":\"$method\"}"
  fi
  curl -s --max-time 90 -b "$JAR" -H 'Content-Type: application/json' \
       ${csrf:+-H "X-CSRF-Token: $csrf"} \
       -X POST "$BASE/rpc/v1" -d "$body"
}

jqf() { python3 -c "
import json,sys
try: d=json.load(sys.stdin)
except Exception: print(''); sys.exit()
cur=d
for k in sys.argv[1].split('.'):
    if isinstance(cur,dict): cur=cur.get(k)
    else: cur=None
print('' if cur is None else cur)" "$1"; }

err_of() { jqf error.message; }

login() {
  local pw="${ARCHY_PASSWORD:-}"
  if [ -z "$pw" ]; then
    log "ARCHY_PASSWORD is not set — cannot authenticate."; exit 2
  fi
  curl -s -c "$JAR" --max-time 30 -H 'Content-Type: application/json' \
       -X POST "$BASE/rpc/v1" \
       -d "{\"method\":\"auth.login\",\"params\":{\"password\":\"$pw\"}}" >/dev/null
  if ! grep -q session "$JAR" 2>/dev/null; then
    log "Login failed — check ARCHY_PASSWORD."; exit 2
  fi
}

log "== ecash route check =="
log "node:    $BASE"
log "network: $NETWORK"
login

# ── network routes ────────────────────────────────────────────────────────
res="$(rpc wallet.ecash-network)"
ORIGINAL_NETWORK="$(printf '%s' "$res" | jqf result.network)"
[ -n "$ORIGINAL_NETWORK" ] && ok "ecash-network reports '$ORIGINAL_NETWORK'" \
                           || bad "ecash-network: $(printf '%s' "$res" | err_of)"

# Always restore whatever the node was on, even on failure.
restore_network() {
  if [ -n "$ORIGINAL_NETWORK" ]; then
    rpc wallet.ecash-set-network "{\"network\":\"$ORIGINAL_NETWORK\"}" >/dev/null
    log "restored network to '$ORIGINAL_NETWORK'"
  fi
}
trap 'restore_network; rm -f "$JAR"' EXIT

res="$(rpc wallet.ecash-set-network "{\"network\":\"$NETWORK\"}")"
[ "$(printf '%s' "$res" | jqf result.network)" = "$NETWORK" ] \
  && ok "switched to $NETWORK" || bad "set-network: $(printf '%s' "$res" | err_of)"

res="$(rpc wallet.ecash-set-network '{"network":"bogus"}')"
[ -n "$(printf '%s' "$res" | err_of)" ] \
  && ok "unknown network rejected" || bad "unknown network was accepted"

MINT="$(rpc wallet.ecash-network | jqf result.mint_url)"
log "mint:    $MINT"

# ── read routes ───────────────────────────────────────────────────────────
res="$(rpc wallet.ecash-balance)"
START_BAL="$(printf '%s' "$res" | jqf result.cashu_sats)"
[ -n "$START_BAL" ] && ok "ecash-balance = ${START_BAL} sats" \
                    || bad "ecash-balance: $(printf '%s' "$res" | err_of)"

res="$(rpc wallet.ecash-history)"
[ -z "$(printf '%s' "$res" | err_of)" ] && ok "ecash-history" \
  || bad "ecash-history: $(printf '%s' "$res" | err_of)"

# ── mint reachability (the route every other one depends on) ──────────────
if curl -s --max-time 20 "$MINT/v1/keysets" | grep -q keysets; then
  ok "mint keysets reachable"
else
  bad "mint $MINT unreachable — remaining checks will fail"
fi

# ── mint quote (invoice issuance) ─────────────────────────────────────────
res="$(rpc wallet.ecash-mint '{"amount_sats":16}')"
QUOTE="$(printf '%s' "$res" | jqf result.quote_id)"
[ -n "$QUOTE" ] && ok "ecash-mint issued quote ${QUOTE:0:12}…" \
                || bad "ecash-mint: $(printf '%s' "$res" | err_of)"

# The test mint pays its own quotes, so the claim can be attempted directly.
# On mainnet this is expected to stay unpaid — that is not a failure.
if [ -n "$QUOTE" ]; then
  res="$(rpc wallet.ecash-mint-claim "{\"quote_id\":\"$QUOTE\",\"amount_sats\":16}")"
  claimed="$(printf '%s' "$res" | jqf result.amount_sats)"
  if [ -n "$claimed" ]; then
    ok "ecash-mint-claim minted ${claimed} sats"
  elif [ "$NETWORK" = mainnet ]; then
    ok "ecash-mint-claim correctly unpaid on mainnet"
  else
    bad "ecash-mint-claim: $(printf '%s' "$res" | err_of)"
  fi
fi

BAL="$(rpc wallet.ecash-balance | jqf result.cashu_sats)"
log "balance after mint: ${BAL:-?} sats"

# ── send + receive round trip (the route that broke) ──────────────────────
if [ "${BAL:-0}" -ge 4 ] 2>/dev/null; then
  res="$(rpc wallet.ecash-send '{"amount_sats":4}')"
  TOKEN="$(printf '%s' "$res" | jqf result.token)"
  if [ -n "$TOKEN" ]; then
    ok "ecash-send produced a token (${#TOKEN} chars, ${TOKEN:0:7}…)"

    res="$(rpc wallet.ecash-receive "{\"token\":\"$TOKEN\"}")"
    got="$(printf '%s' "$res" | jqf result.received_sats)"
    [ -n "$got" ] && ok "ecash-receive redeemed ${got} sats" \
                  || bad "ecash-receive: $(printf '%s' "$res" | err_of)"

    # Double-spend must be refused, not silently accepted.
    res="$(rpc wallet.ecash-receive "{\"token\":\"$TOKEN\"}")"
    [ -n "$(printf '%s' "$res" | err_of)" ] \
      && ok "double-redeem refused" || bad "double-redeem was ACCEPTED"
  else
    bad "ecash-send: $(printf '%s' "$res" | err_of)"
  fi
else
  log "  (skipping send/receive — balance ${BAL:-0} sats too low)"
fi

# ── malformed input handling ──────────────────────────────────────────────
res="$(rpc wallet.ecash-receive '{"token":"not-a-token"}')"
[ -n "$(printf '%s' "$res" | err_of)" ] \
  && ok "garbage token rejected" || bad "garbage token was accepted"

# ── melt quote (spend to Lightning) ───────────────────────────────────────
if [ -n "${ECASH_TEST_BOLT11:-}" ]; then
  res="$(rpc wallet.ecash-melt "{\"bolt11\":\"$ECASH_TEST_BOLT11\"}")"
  mq="$(printf '%s' "$res" | jqf result.quote_id)"
  [ -n "$mq" ] && ok "ecash-melt quoted ${mq:0:12}…" \
               || bad "ecash-melt: $(printf '%s' "$res" | err_of)"
else
  log "  (skipping melt — set ECASH_TEST_BOLT11 to a payable invoice)"
fi

log ""
log "== $PASS passed, $FAIL failed =="
exit "$FAIL"
