#!/usr/bin/env bash
# Prove the ecash backup phrase actually brings coins back.
#
# The route check (test-ecash-routes.sh) can only confirm that
# `wallet.ecash-restore` returns without an error — and on a wallet with
# nothing to find, "recovered 0 sats" is a pass there. That is exactly the
# shape of a backup that looks fine until the day you need it. This script
# does the only test that settles it: mint coins, **delete the wallet file**,
# restore, and check the coins came back.
#
#   ARCHY_PASSWORD='…' ./scripts/test-ecash-restore.sh
#
# Testnet only, and it refuses to run otherwise. It deletes a wallet file;
# doing that to real coins to prove a point is not a trade worth making, and
# a flag to override would eventually get used. The testnet purse is a
# separate file (`wallet/ecash.testnet.json`) holding valueless testnut coins,
# so the real one is never in scope.
#
# The original file is copied aside first and put back at the end on every
# exit path, so even a failed restore loses nothing.

set -uo pipefail

HOST="${ARCHY_HOST:-127.0.0.1}"
SCHEME="${ARCHY_SCHEME:-http}"
BASE="$SCHEME://$HOST"
WALLET="${ARCHY_DATA_DIR:-/var/lib/archipelago}/wallet/ecash.testnet.json"
MINT_SATS="${ECASH_RESTORE_MINT_SATS:-21}"

JAR="$(mktemp -t ecash-restore-XXXXXX.jar)"
BACKUP=""
ORIGINAL_NETWORK=""

PASS=0
FAIL=0
ok()  { PASS=$((PASS+1)); printf '  \033[32mPASS\033[0m %s\n' "$*"; }
bad() { FAIL=$((FAIL+1)); printf '  \033[31mFAIL\033[0m %s\n' "$*"; }

cleanup() {
  # Network first, while the service is still up: the restart below tears the
  # RPC out from under us, and an ecash-set-network fired at a socket that
  # isn't listening yet fails silently — which left the node parked on testnet
  # after an otherwise green run.
  [ -n "$ORIGINAL_NETWORK" ] && rpc wallet.ecash-set-network "{\"network\":\"$ORIGINAL_NETWORK\"}" >/dev/null 2>&1

  # Then put the wallet back. This is the only step whose failure could
  # actually cost someone coins, so it is not conditional on the above.
  if [ -n "$BACKUP" ] && [ -f "$BACKUP" ]; then
    sudo mv -f "$BACKUP" "$WALLET" && printf 'restored the testnet wallet file\n'
    sudo systemctl restart archipelago >/dev/null 2>&1
    # Wait for it back, so a caller running another check straight after
    # doesn't meet a dead socket.
    for _ in $(seq 1 30); do
      curl -s --max-time 3 -o /dev/null "$BASE/" && break
      sleep 2
    done
  fi
  rm -f "$JAR"
}
trap cleanup EXIT

rpc() {
  local method="$1" params="${2:-}" body csrf
  csrf="$(awk '/csrf_token/{print $NF}' "$JAR" 2>/dev/null | tail -1)"
  if [ -n "$params" ]; then body="{\"method\":\"$method\",\"params\":$params}"
  else body="{\"method\":\"$method\"}"; fi
  curl -s --max-time 300 -b "$JAR" -H 'Content-Type: application/json' \
       ${csrf:+-H "X-CSRF-Token: $csrf"} -X POST "$BASE/rpc/v1" -d "$body"
}

jqf() { python3 -c "
import json,sys
try: d=json.load(sys.stdin)
except Exception: print(''); sys.exit()
cur=d
for k in sys.argv[1].split('.'):
    cur = cur.get(k) if isinstance(cur,dict) else None
print('' if cur is None else cur)" "$1"; }

[ -n "${ARCHY_PASSWORD:-}" ] || { echo "ARCHY_PASSWORD is not set."; exit 2; }
curl -s -c "$JAR" --max-time 30 -H 'Content-Type: application/json' -X POST "$BASE/rpc/v1" \
     -d "{\"method\":\"auth.login\",\"params\":{\"password\":\"$ARCHY_PASSWORD\"}}" >/dev/null
grep -q session "$JAR" 2>/dev/null || { echo "Login failed."; exit 2; }

echo "== ecash restore proof =="
ORIGINAL_NETWORK="$(rpc wallet.ecash-network | jqf result.network)"
rpc wallet.ecash-set-network '{"network":"testnet"}' >/dev/null
NETWORK="$(rpc wallet.ecash-network | jqf result.network)"
if [ "$NETWORK" != "testnet" ]; then
  echo "Refusing to run: could not switch to testnet (still '$NETWORK')."; exit 2
fi
MINT="$(rpc wallet.ecash-network | jqf result.mint_url)"
echo "mint: $MINT"

# The phrase has to exist *before* the coins are minted, or there is nothing
# to derive them from — which is the whole point being tested.
if [ "$(rpc wallet.ecash-seed-status | jqf result.active)" != "True" ]; then
  rpc wallet.ecash-seed-reveal "{\"password\":\"$ARCHY_PASSWORD\"}" >/dev/null
fi
[ "$(rpc wallet.ecash-seed-status | jqf result.active)" = "True" ] \
  && ok "backup phrase is active" || { bad "no backup phrase — cannot prove anything"; exit 1; }

echo "== minting $MINT_SATS sats under the phrase =="
QUOTE="$(rpc wallet.ecash-mint "{\"amount_sats\":$MINT_SATS}" | jqf result.quote_id)"
[ -n "$QUOTE" ] || { bad "could not get a mint quote"; exit 1; }
for _ in $(seq 1 20); do
  state="$(curl -s --max-time 15 "$MINT/v1/mint/quote/bolt11/$QUOTE" \
           | python3 -c "import json,sys;print((json.load(sys.stdin) or {}).get('state',''))" 2>/dev/null)"
  [ "$state" = PAID ] && break
  sleep 3
done
MINTED="$(rpc wallet.ecash-mint-claim "{\"quote_id\":\"$QUOTE\",\"amount_sats\":$MINT_SATS}" | jqf result.minted_sats)"
[ "${MINTED:-0}" -gt 0 ] 2>/dev/null \
  && ok "minted ${MINTED} sats with NUT-13 secrets" \
  || { bad "mint-claim failed (quote state: $state) — nothing to recover"; exit 1; }

BEFORE="$(rpc wallet.ecash-balance | jqf result.cashu_sats)"
echo "balance: ${BEFORE} sats"

echo "== deleting the wallet file =="
BACKUP="${WALLET}.restore-proof.$$"
sudo cp -a "$WALLET" "$BACKUP" || { bad "could not back up $WALLET"; exit 1; }
sudo rm -f "$WALLET"
WIPED="$(rpc wallet.ecash-balance | jqf result.cashu_sats)"
[ "${WIPED:-1}" = "0" ] && ok "wallet is empty after the wipe" \
                        || bad "balance is ${WIPED} after deleting the wallet — the wipe did not take"

echo "== restoring from the phrase alone =="
RES="$(rpc wallet.ecash-restore)"
ERR="$(printf '%s' "$RES" | jqf error.message)"
if [ -n "$ERR" ]; then
  bad "ecash-restore: $ERR"
else
  RECOVERED="$(printf '%s' "$RES" | jqf result.recovered_sats)"
  PROOFS="$(printf '%s' "$RES" | jqf result.recovered_proofs)"
  ok "restore returned ${RECOVERED} sats across ${PROOFS} coins"

  # The assertion that matters. Coins minted *before* the phrase existed used
  # random secrets and can never come back — so the bar is what this run
  # minted, not the whole prior balance. Anything less means NUT-13 derivation
  # and the mint disagree about what was signed.
  if [ "${RECOVERED:-0}" -ge "${MINTED:-1}" ] 2>/dev/null; then
    ok "every coin minted under the phrase came back (${RECOVERED} >= ${MINTED})"
  else
    bad "only ${RECOVERED} of the ${MINTED} sats minted under the phrase came back"
  fi

  FINAL="$(rpc wallet.ecash-balance | jqf result.cashu_sats)"
  [ "${FINAL:-0}" = "${RECOVERED:-x}" ] \
    && ok "the restored balance is exactly what was recovered" \
    || bad "balance ${FINAL} does not match the ${RECOVERED} sats reported"

  # A restore that invents coins is worse than one that finds none: the
  # balance would read as spendable and every spend would fail at the mint.
  rpc wallet.ecash-restore >/dev/null
  AGAIN="$(rpc wallet.ecash-balance | jqf result.cashu_sats)"
  [ "${AGAIN:-0}" = "${FINAL:-x}" ] \
    && ok "a second restore adds nothing (${AGAIN} sats)" \
    || bad "a second restore changed the balance: ${FINAL} -> ${AGAIN}"
fi

echo ""
echo "== $PASS passed, $FAIL failed =="
exit "$FAIL"
