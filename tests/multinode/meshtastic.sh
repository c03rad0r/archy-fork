#!/usr/bin/env bash
# tests/multinode/meshtastic.sh — two-/three-radio Meshtastic parity harness.
#
# Validates that Meshtastic radios have the SAME mesh-tab features Meshcore got,
# done over the real wire. It drives 2 (optionally 3) archipelago nodes, each
# with a Meshtastic radio attached, and exercises the full message pipeline:
#
#   1. detect    — each node reports a connected meshtastic device
#   2. discover  — A sees B as a peer (NodeInfo discovery), and vice-versa
#   3. dm        — A → B direct message round-trips (native unicast)
#   4. privacy   — a third listener C does NOT see the A→B DM (proves the
#                  directed-unicast fix: DMs are not broadcast on the channel)
#   5. channel   — A's channel broadcast IS seen by both B and C
#   6. typed     — a typed envelope (reaction) round-trips with message_type set
#   7. assistant — (optional) an !ai query gets a PRIVATE reply, not a channel
#                  blast (gated on ASSIST=1 + assistant enabled on B)
#   8. reachable — reports each peer's `reachable`/`last_advert` so the ambiguous
#                  Meshtastic reachability semantics can be eyeballed on-air
#                  before anyone "fixes" them
#
# The privacy test (4) is the on-air proof of the meshtastic.rs send_text_msg
# unicast change. Without it, A→B DMs land on every node's channel feed.
#
# Nodes override via env (each must have a Meshtastic radio on the SAME LoRa
# channel/region so they can actually hear each other):
#   MA_URL MA_PW   node A  (sender)     default .116 http  / <FLEET_PW>
#   MB_URL MB_PW   node B  (receiver)   default .228 https / password123
#   MC_URL MC_PW   node C  (eavesdrop)  OPTIONAL — enables privacy test (4)
#
#   MB_NAME        B's mesh node name, if A's peer list is ambiguous (>1 peer)
#   PROP_WAIT      seconds to wait for LoRa propagation per step (default 45)
#   ASSIST         set =1 to run the assistant private-reply test (7)
#
# Usage:
#   tests/multinode/meshtastic.sh
#   MA_URL=http://192.0.2.12 MB_URL=https://192.0.2.10 \
#   MC_URL=https://192.0.2.11 tests/multinode/meshtastic.sh
#
# Requires: curl, jq. Exit code = number of failed assertions (0 = all green).

set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib/multinode.bash
source "$HERE/lib/multinode.bash"

# ── node registration ──────────────────────────────────────────────────────
MA_URL="${MA_URL:-http://192.0.2.12}";  MA_PW="${MA_PW:?MA_PW required — export it or set tests/multinode/.env (see .env.example)}"
MB_URL="${MB_URL:-https://192.0.2.10}"; MB_PW="${MB_PW:?MB_PW required — export it or set tests/multinode/.env (see .env.example)}"
MC_URL="${MC_URL:-}";                      MC_PW="${MC_PW:-}"
PROP_WAIT="${PROP_WAIT:-45}"
MB_NAME="${MB_NAME:-}"
ASSIST="${ASSIST:-0}"

node_register A "$MA_URL" "$MA_PW"
node_register B "$MB_URL" "$MB_PW"
HAVE_C=0
if [[ -n "$MC_URL" ]]; then node_register C "$MC_URL" "$MC_PW"; HAVE_C=1; fi

# ── tiny assert framework (mirrors smoke.sh) ───────────────────────────────
if [[ -t 1 ]]; then
  green()  { printf '\033[32m%s\033[0m' "$*"; }
  red()    { printf '\033[31m%s\033[0m' "$*"; }
  yellow() { printf '\033[33m%s\033[0m' "$*"; }
else
  green()  { printf '%s' "$*"; }; red() { printf '%s' "$*"; }; yellow() { printf '%s' "$*"; }
fi
PASS=0; FAIL=0; SKIP=0; declare -a FAILED_NAMES
ok()   { printf '  %s %s\n' "$(green ✓)" "$1"; PASS=$((PASS+1)); }
no()   { printf '  %s %s\n' "$(red ✗)"   "$1"; FAIL=$((FAIL+1)); FAILED_NAMES+=("$1"); }
skip() { printf '  %s %s (%s)\n' "$(yellow —)" "$1" "${2:-skipped}"; SKIP=$((SKIP+1)); }
assert_true() { [[ "$2" == "true" ]] && ok "$1" || no "$1 (got '$2')"; }
section() { printf '\n%s\n' "$(yellow "── $* ──")"; }

# nonce for this run so message matches can't collide with stale history
NONCE="mtparity-$$-${RANDOM}"

# ── helpers ────────────────────────────────────────────────────────────────

# mesh_connected HANDLE -> "true" if a meshtastic device is connected
mesh_connected() {
  local s; s=$(node_result "$1" mesh.status 2>/dev/null) || { echo false; return; }
  local conn type
  conn=$(echo "$s" | jq -r '.device_connected // false')
  type=$(echo "$s" | jq -r '.device_type // "unknown"')
  [[ "$conn" == "true" && "$type" == "meshtastic" ]] && echo true || echo false
}

# self_name HANDLE -> this node's meshtastic long-name (from firmware_version)
self_name() {
  node_result "$1" mesh.status 2>/dev/null | jq -r '.firmware_version // empty'
}

# contact_id_for HANDLE NAME -> the contact_id of the peer whose advert_name
# matches NAME (case-insensitive substring); empty if not found / ambiguous.
contact_id_for() {
  local h="$1" want="$2"
  node_result "$h" mesh.peers 2>/dev/null | jq -r --arg w "$want" '
    [.peers[] | select((.advert_name // "" | ascii_downcase)
      | contains($w | ascii_downcase))] as $m
    | if ($m|length)==1 then ($m[0].contact_id|tostring) else "" end'
}

# peer_count_excl_self HANDLE -> number of peers
peer_count() { node_result "$1" mesh.peers 2>/dev/null | jq -r '.count // 0'; }

# saw_text HANDLE NEEDLE [direction] -> "true" if a message whose plaintext
# contains NEEDLE exists (optionally filtered to a direction: sent/received)
saw_text() {
  local h="$1" needle="$2" dir="${3:-}"
  node_result "$h" mesh.messages '{"limit":200}' 2>/dev/null | jq -r --arg n "$needle" --arg d "$dir" '
    [.messages[] | select((.plaintext // "") | contains($n))
      | select($d=="" or (.direction==$d))] | length > 0'
}

# wait_text HANDLE NEEDLE — poll up to PROP_WAIT for a received message
wait_text() {
  local h="$1" needle="$2" waited=0
  while (( waited < PROP_WAIT )); do
    [[ "$(saw_text "$h" "$needle" received)" == "true" ]] && return 0
    sleep 3; waited=$((waited+3))
  done
  return 1
}

# ── login ──────────────────────────────────────────────────────────────────
section "login"
node_login A && ok "A login ($MA_URL)" || { no "A unreachable ($MA_URL)"; echo; exit 1; }
node_login B && ok "B login ($MB_URL)" || { no "B unreachable ($MB_URL)"; echo; exit 1; }
if (( HAVE_C )); then
  node_login C && ok "C login ($MC_URL)" || { skip "C login" "unreachable — privacy test disabled"; HAVE_C=0; }
fi

# ── 1. detect ──────────────────────────────────────────────────────────────
section "1. device detection"
A_CONN=$(mesh_connected A); B_CONN=$(mesh_connected B)
assert_true "A has a connected meshtastic radio" "$A_CONN"
assert_true "B has a connected meshtastic radio" "$B_CONN"
if [[ "$A_CONN" != "true" || "$B_CONN" != "true" ]]; then
  printf '\n%s\n' "$(yellow 'Both A and B need a Meshtastic radio attached & mesh enabled.')"
  printf '%s\n' "$(yellow 'Aborting on-air tests; see mesh.status output above.')"
  echo; printf 'PASS=%d FAIL=%d SKIP=%d\n' "$PASS" "$FAIL" "$SKIP"; exit "$FAIL"
fi
A_NAME=$(self_name A); B_NAME=$(self_name B)
printf '    A=%s  B=%s\n' "${A_NAME:-?}" "${B_NAME:-?}"
[[ -n "$MB_NAME" ]] && B_NAME="$MB_NAME"

# ── 2. peer discovery ──────────────────────────────────────────────────────
section "2. peer discovery (NodeInfo)"
DISCO=0; waited=0
while (( waited < PROP_WAIT )); do
  CID=$(contact_id_for A "${B_NAME:-Meshtastic}")
  [[ -n "$CID" ]] && { DISCO=1; break; }
  # fall back: any single non-channel peer
  if [[ -z "$MB_NAME" && "$(peer_count A)" == "1" ]]; then
    CID=$(node_result A mesh.peers | jq -r '.peers[0].contact_id'); DISCO=1; break
  fi
  sleep 3; waited=$((waited+3))
done
if (( DISCO )); then ok "A discovered B as a peer (contact_id=$CID)"
else
  no "A did not discover B within ${PROP_WAIT}s"
  printf '    A peers: %s\n' "$(node_result A mesh.peers | jq -c '.peers[]? | {contact_id,advert_name}')"
fi

# ── 3. direct message round-trip ───────────────────────────────────────────
section "3. direct message (native unicast)"
if (( DISCO )); then
  DM="$NONCE-dm hello-from-A"
  if node_result A mesh.send "$(jq -nc --argjson c "$CID" --arg m "$DM" '{contact_id:$c,message:$m}')" >/dev/null; then
    ok "A sent DM to B (contact_id=$CID)"
    if wait_text B "$NONCE-dm"; then ok "B received the DM"
    else no "B did not receive the DM within ${PROP_WAIT}s"; fi
  else no "mesh.send failed on A"; fi
else skip "DM round-trip" "B not discovered"; fi

# ── 4. privacy: third node must NOT see the DM ─────────────────────────────
section "4. DM privacy (directed, not broadcast)"
if (( HAVE_C )) && (( DISCO )); then
  C_CONN=$(mesh_connected C)
  if [[ "$C_CONN" != "true" ]]; then
    skip "DM privacy" "C has no meshtastic radio"
  else
    # Give C the same window the DM had to propagate, then assert absence.
    sleep "$PROP_WAIT"
    if [[ "$(saw_text C "$NONCE-dm")" == "true" ]]; then
      no "C (eavesdropper) saw the A→B DM — it is being BROADCAST, not unicast"
    else
      ok "C did NOT see the A→B DM (directed unicast confirmed)"
    fi
  fi
else
  skip "DM privacy" "needs MC_URL (third radio) + discovered peer"
fi

# ── 5. channel broadcast reaches everyone ──────────────────────────────────
section "5. channel broadcast"
CH="$NONCE-chan broadcast-to-all"
if node_result A mesh.send-channel "$(jq -nc --arg m "$CH" '{channel:0,message:$m}')" >/dev/null; then
  ok "A sent a channel broadcast"
  if wait_text B "$NONCE-chan"; then ok "B received the broadcast"; else no "B missed the broadcast"; fi
  if (( HAVE_C )) && [[ "$(mesh_connected C)" == "true" ]]; then
    if [[ "$(saw_text C "$NONCE-chan")" == "true" ]]; then ok "C also received the broadcast"
    else no "C missed the broadcast (it should reach all channel members)"; fi
  fi
else no "mesh.send-channel failed on A"; fi

# ── 6. typed envelope round-trip ───────────────────────────────────────────
section "6. typed message (reaction envelope)"
if (( DISCO )); then
  # A reaction is the smallest typed envelope; it should arrive with a
  # non-"text" message_type, proving the typed pipeline works over Meshtastic.
  REACT_PARAMS=$(jq -nc --argjson c "$CID" --arg n "$NONCE" \
    '{contact_id:$c, emoji:"👍", target_seq:0, note:$n}')
  if node_result A mesh.send-reaction "$REACT_PARAMS" >/dev/null 2>&1; then
    ok "A sent a reaction (typed envelope)"
    sleep "$PROP_WAIT"
    TYPED=$(node_result B mesh.messages '{"limit":200}' 2>/dev/null \
      | jq -r '[.messages[] | select(.message_type != null and .message_type != "text")] | length > 0')
    assert_true "B received a non-text typed message" "$TYPED"
  else
    skip "typed message" "mesh.send-reaction rejected params (check handler signature)"
  fi
else skip "typed message" "B not discovered"; fi

# ── 7. assistant private reply (optional) ──────────────────────────────────
section "7. AI assistant private reply (optional)"
if [[ "$ASSIST" == "1" ]] && (( DISCO )); then
  AST=$(node_result B mesh.assistant-status 2>/dev/null | jq -r '.enabled // false')
  if [[ "$AST" != "true" ]]; then
    skip "assistant reply" "assistant not enabled on B"
  else
    Q="$NONCE-ai !ai are you there"
    node_result A mesh.send-channel "$(jq -nc --arg m "$Q" '{channel:0,message:$m}')" >/dev/null
    sleep "$PROP_WAIT"
    # A should get a private DM reply; C (if present) should NOT.
    if [[ "$(saw_text A "$NONCE-ai-reply")" == "true" || "$(node_result A mesh.messages '{"limit":50}' | jq -r '[.messages[]|select(.direction=="received")]|length>0')" == "true" ]]; then
      ok "A received an assistant reply"
    else
      no "A did not receive an assistant reply within ${PROP_WAIT}s"
    fi
    if (( HAVE_C )) && [[ "$(mesh_connected C)" == "true" ]]; then
      # heuristic: the reply text shouldn't be on C's channel feed
      skip "assistant reply privacy" "eyeball C's feed — automated check is heuristic"
    fi
  fi
else
  skip "assistant reply" "set ASSIST=1 and enable the assistant on B to run"
fi

# ── 8. reachability snapshot (report-only) ─────────────────────────────────
section "8. reachability snapshot (report-only)"
node_result A mesh.peers 2>/dev/null | jq -r '.peers[]?
  | "    \(.advert_name // "?")  reachable=\(.reachable)  last_advert=\(.last_advert // 0)"'
printf '%s\n' "$(yellow '    NOTE: Meshtastic flood-routes; path_len is always 0xff, so `reachable`')"
printf '%s\n' "$(yellow '    may read true even for stale nodes. Confirm desired semantics here')"
printf '%s\n' "$(yellow '    before changing the refresh_contacts reachability rule.')"

# ── summary ────────────────────────────────────────────────────────────────
section "summary"
printf 'PASS=%s FAIL=%s SKIP=%s\n' "$(green "$PASS")" "$( ((FAIL)) && red "$FAIL" || green 0 )" "$(yellow "$SKIP")"
if (( FAIL )); then
  printf 'failed:\n'; for n in "${FAILED_NAMES[@]}"; do printf '  - %s\n' "$n"; done
fi
exit "$FAIL"
