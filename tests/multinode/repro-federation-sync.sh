#!/usr/bin/env bash
# Controlled two-node reproduction of node-to-node federation sync.
#
# Pairs two real nodes via federation.invite/join, triggers federation.sync-state
# in both directions, and reports which transport actually carried the call and
# any per-peer error. This is the controlled repro for the reported
# "Tor connection cloud->node not working" symptom: raw Tor transport is known
# good (see README), so this isolates whether the APP-level sync path works and,
# if it fails, surfaces the exact error string.
#
# Env (override as needed):
#   A_URL A_PW   node A base url + UI password   (default .116 http)
#   B_URL B_PW   node B base url + UI password   (default .228 https)
#   FORCE_TOR=1  set both nodes' federation transport preference to Tor first
#
# Usage: tests/multinode/repro-federation-sync.sh
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$HERE/lib/multinode.bash"

A_URL="${A_URL:-http://192.0.2.12}";  A_PW="${A_PW:?A_PW required — export it or set tests/multinode/.env (see .env.example)}"
B_URL="${B_URL:-https://192.0.2.10}"; B_PW="${B_PW:?B_PW required — export it or set tests/multinode/.env (see .env.example)}"

bar() { printf '\n=== %s ===\n' "$*"; }

node_register A "$A_URL" "$A_PW"
node_register B "$B_URL" "$B_PW"

bar "login"
node_login A || { echo "A login failed"; exit 1; }
node_login B || { echo "B login failed"; exit 1; }
echo "A=$A_URL  B=$B_URL  logged in"

bar "onions"
A_ONION=$(node_onion A); B_ONION=$(node_onion B)
echo "A onion: ${A_ONION:-<none>}"
echo "B onion: ${B_ONION:-<none>}"

if [[ "${FORCE_TOR:-0}" == "1" ]]; then
  bar "force federation transport = tor on both"
  node_rpc A transport.set-preference '{"service":"federation","pref":"tor"}' | jq -c '.result // .error'
  node_rpc B transport.set-preference '{"service":"federation","pref":"tor"}' | jq -c '.result // .error'
fi

bar "federation state BEFORE"
echo "A knows:"; node_result A federation.list-nodes | jq -r '.[]? | "  \(.name // "?")  did=\(.did[0:24])…  last_seen=\(.last_seen // "never")"' 2>/dev/null || echo "  (none/err)"
echo "B knows:"; node_result B federation.list-nodes | jq -r '.[]? | "  \(.name // "?")  did=\(.did[0:24])…  last_seen=\(.last_seen // "never")"' 2>/dev/null || echo "  (none/err)"

bar "pair: A invites, B joins"
INV_A=$(node_result A federation.invite)
CODE_A=$(echo "$INV_A" | jq -r '.code // empty')
echo "A invite code: ${CODE_A:0:40}…"
if [[ -n "$CODE_A" ]]; then
  node_result B federation.join "$(jq -nc --arg c "$CODE_A" '{code:$c}')" \
    && echo "B joined A" || echo "B join FAILED"
fi

bar "pair: B invites, A joins"
INV_B=$(node_result B federation.invite)
CODE_B=$(echo "$INV_B" | jq -r '.code // empty')
echo "B invite code: ${CODE_B:0:40}…"
if [[ -n "$CODE_B" ]]; then
  node_result A federation.join "$(jq -nc --arg c "$CODE_B" '{code:$c}')" \
    && echo "A joined B" || echo "A join FAILED"
fi

bar "trigger sync-state on A (A dials its peers)"
node_result A federation.sync-state | jq '.'

bar "trigger sync-state on B (B dials its peers)"
node_result B federation.sync-state | jq '.'

bar "federation state AFTER (look for fresh last_seen + transport)"
echo "A knows:"; node_result A federation.list-nodes | jq -r '.[]? | "  \(.name // "?")  last_seen=\(.last_seen // "never")  transport=\(.last_transport // .transport // "?")"' 2>/dev/null
echo "B knows:"; node_result B federation.list-nodes | jq -r '.[]? | "  \(.name // "?")  last_seen=\(.last_seen // "never")  transport=\(.last_transport // .transport // "?")"' 2>/dev/null

bar "done"
