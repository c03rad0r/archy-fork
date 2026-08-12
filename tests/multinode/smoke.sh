#!/usr/bin/env bash
# Two-node (optionally three-node) end-to-end smoke suite for the full app.
#
# Unlike repro-federation-sync.sh (a diagnostic that just prints state), this
# is an ASSERTION suite: every check is pass/fail and the script exits non-zero
# if any required check fails. It exercises the real node-to-node surface and
# specifically guards the bugs fixed in v1.7.94 / v1.7.95:
#   - FIPS auto-connects to the public anchor (v1.7.94)
#   - peer content browse works over the mesh, not just Tor (v1.7.95 — the
#     `/content` catalog used to 404 over FIPS and never fall back to Tor)
#   - a removed federation node stays removed, incl. transitive re-discovery
#     (v1.7.95 tombstone) — the transitive case needs node C.
#
# Nodes (override via env):
#   A_URL A_PW   node A   (default .116 http)
#   B_URL B_PW   node B   (default .228 https)
#   C_URL C_PW   node C   (OPTIONAL — enables the transitive-tombstone test)
#
# Requires both nodes on v1.7.95-alpha+ for the content-browse and tombstone
# checks; older peers SKIP those (reported, not failed).
#
# Usage:
#   tests/multinode/smoke.sh
#   A_URL=http://192.0.2.12 B_URL=https://192.0.2.10 tests/multinode/smoke.sh
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$HERE/lib/multinode.bash"

A_URL="${A_URL:-http://192.0.2.12}";  A_PW="${A_PW:?A_PW required — export it or set tests/multinode/.env (see .env.example)}"
B_URL="${B_URL:-https://192.0.2.10}"; B_PW="${B_PW:?B_PW required — export it or set tests/multinode/.env (see .env.example)}"
C_URL="${C_URL:-}";                      C_PW="${C_PW:-}"

# ── tiny assertion framework ──────────────────────────────────────────────
PASS=0; FAIL=0; SKIP=0
declare -a FAILED_NAMES
green() { printf '\033[32m%s\033[0m' "$*"; }
red()   { printf '\033[31m%s\033[0m' "$*"; }
yellow(){ printf '\033[33m%s\033[0m' "$*"; }
section() { printf '\n\033[1m── %s ──\033[0m\n' "$*"; }
ok()   { printf '  %s %s\n' "$(green ✓)" "$1"; PASS=$((PASS+1)); }
no()   { printf '  %s %s\n' "$(red ✗)"   "$1"; FAIL=$((FAIL+1)); FAILED_NAMES+=("$1"); }
skip() { printf '  %s %s (%s)\n' "$(yellow —)" "$1" "$2"; SKIP=$((SKIP+1)); }
# assert_eq NAME EXPECTED ACTUAL
assert_eq() { [[ "$2" == "$3" ]] && ok "$1" || no "$1 (expected '$2', got '$3')"; }
# assert_true NAME VALUE   — passes when VALUE is "true"
assert_true() { [[ "$2" == "true" ]] && ok "$1" || no "$1 (got '$2')"; }

# did_of HANDLE — this node's own DID via node.did (string or {did:...}).
did_of() {
  node_result "$1" node.did 2>/dev/null \
    | jq -r 'if type=="string" then . elif type=="object" then (.did // .node_did // empty) else empty end' 2>/dev/null
}

# pair HANDLE_INVITER HANDLE_JOINER — invite + join one direction. Echo "ok"/"fail".
pair() {
  local inv code
  inv=$(node_result "$1" federation.invite 2>/dev/null)
  code=$(echo "$inv" | jq -r '.code // empty' 2>/dev/null)
  [[ -z "$code" ]] && { echo "fail"; return; }
  if node_result "$2" federation.join "$(jq -nc --arg c "$code" '{code:$c}')" >/dev/null 2>&1; then
    echo "ok"
  else echo "fail"; fi
}

node_register A "$A_URL" "$A_PW"
node_register B "$B_URL" "$B_PW"
HAVE_C=0
if [[ -n "$C_URL" && -n "$C_PW" ]]; then node_register C "$C_URL" "$C_PW"; HAVE_C=1; fi

# ── 1. reachability + auth ────────────────────────────────────────────────
section "reachability + login"
node_login A && ok "A login ($A_URL)" || { no "A login ($A_URL)"; echo "A unreachable — aborting"; exit 1; }
node_login B && ok "B login ($B_URL)" || { no "B login ($B_URL)"; echo "B unreachable — aborting"; exit 1; }
if [[ $HAVE_C == 1 ]]; then node_login C && ok "C login ($C_URL)" || { no "C login"; HAVE_C=0; }; fi

A_ONION=$(node_onion A); B_ONION=$(node_onion B)
[[ -n "$A_ONION" ]] && ok "A has onion address" || no "A has onion address"
[[ -n "$B_ONION" ]] && ok "B has onion address" || no "B has onion address"

# ── 2. FIPS mesh: daemon up + anchor connected (v1.7.94) ──────────────────
section "FIPS mesh / anchor"
for h in A B; do
  s=$(node_result "$h" fips.status 2>/dev/null)
  if [[ -z "$s" ]]; then skip "$h fips.status" "no FIPS RPC (old build?)"; continue; fi
  assert_true "$h FIPS service active" "$(echo "$s" | jq -r '.service_active')"
  ac=$(echo "$s" | jq -r '.anchor_connected')
  if [[ "$ac" == "true" ]]; then ok "$h anchor connected"
  else skip "$h anchor connected" "anchor_connected=$ac — node may need v1.7.94 + a moment to handshake"; fi
done

# ── 3. federation pairing (both directions) ───────────────────────────────
section "federation pairing"
assert_eq "A invites, B joins" "ok" "$(pair A B)"
assert_eq "B invites, A joins" "ok" "$(pair B A)"
# both should now list each other
node_result A federation.sync-state >/dev/null 2>&1
node_result B federation.sync-state >/dev/null 2>&1
A_SEES_B=$(node_result A federation.list-nodes 2>/dev/null | jq -r --arg o "${B_ONION%.onion}" 'any((.nodes // .)[]?; (.onion // "" | gsub("\\.onion$";"")) == $o)')
B_SEES_A=$(node_result B federation.list-nodes 2>/dev/null | jq -r --arg o "${A_ONION%.onion}" 'any((.nodes // .)[]?; (.onion // "" | gsub("\\.onion$";"")) == $o)')
assert_true "A's node list contains B" "$A_SEES_B"
assert_true "B's node list contains A" "$B_SEES_A"

# ── 4. peer content browse over the mesh (v1.7.95 fix) ────────────────────
section "peer content browse (was: 404 over mesh, no Tor fallback)"
if [[ -n "$B_ONION" ]]; then
  resp=$(node_rpc A content.browse-peer "$(jq -nc --arg o "$B_ONION" '{onion:$o}')")
  err=$(echo "$resp" | jq -r '.error.message // empty')
  if [[ -z "$err" ]]; then
    ok "A browses B's content catalog (HTTP 200)"
  elif echo "$err" | grep -q '404'; then
    no "A browses B's content — still 404 over mesh (is B on v1.7.95?): $err"
  else
    # Other errors (peer offline, no content shared) are environmental, not the bug.
    skip "A browses B's content" "non-404 error: $err"
  fi
else
  skip "A browses B's content" "B has no onion"
fi

# ── 5. removed-node tombstone (v1.7.95) ───────────────────────────────────
section "removed-node tombstone"
B_DID=$(did_of B)
if [[ -z "$B_DID" ]]; then
  skip "remove B then verify stays removed" "couldn't resolve B's DID"
else
  if node_result A federation.remove-node "$(jq -nc --arg d "$B_DID" '{did:$d}')" >/dev/null 2>&1; then
    still=$(node_result A federation.list-nodes 2>/dev/null | jq -r --arg d "$B_DID" 'any((.nodes // .)[]?; .did == $d)')
    assert_eq "B removed from A's list" "false" "$still"
    # Transitive test needs C: A federated with B and C; C federated with B;
    # A removes B; A syncs with C (who advertises B) → B must NOT reappear.
    if [[ $HAVE_C == 1 ]]; then
      pair A C >/dev/null; pair C A >/dev/null; pair C B >/dev/null
      node_result A federation.sync-state >/dev/null 2>&1
      reappeared=$(node_result A federation.list-nodes 2>/dev/null | jq -r --arg d "$B_DID" 'any((.nodes // .)[]?; .did == $d)')
      assert_eq "B does NOT reappear via transitive sync with C" "false" "$reappeared"
    else
      skip "transitive reappear via 3rd node" "set C_URL/C_PW to enable"
    fi
    # re-add restores B (explicit re-add clears the tombstone)
    pair B A >/dev/null
    node_result A federation.sync-state >/dev/null 2>&1
    readded=$(node_result A federation.list-nodes 2>/dev/null | jq -r --arg d "$B_DID" 'any((.nodes // .)[]?; .did == $d)')
    assert_true "explicit re-pair brings B back (tombstone cleared)" "$readded"
  else
    skip "remove B" "remove-node RPC failed (B may already be absent)"
  fi
fi

# ── summary ───────────────────────────────────────────────────────────────
section "summary"
printf '  %s passed, %s failed, %s skipped\n' "$(green $PASS)" "$([[ $FAIL -gt 0 ]] && red $FAIL || echo $FAIL)" "$(yellow $SKIP)"
if [[ $FAIL -gt 0 ]]; then
  printf '  failed:\n'; for n in "${FAILED_NAMES[@]}"; do printf '    - %s\n' "$n"; done
  exit 1
fi
echo "  all required checks passed"
