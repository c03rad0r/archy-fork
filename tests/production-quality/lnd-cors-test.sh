#!/usr/bin/env bash
# lnd-cors-test.sh — assert the LND "connect your wallet" endpoints return
# correct CORS headers for the cross-origin call from the LND UI app (:18083).
#
# Bug B5: /lnd-connect-info duplicated ACAO on some nodes; /proxy/lnd/v1/* 401
# carries no ACAO fleet-wide. Browser blocks both.
#
# Usage: ./lnd-cors-test.sh <node-host>   (e.g. 192.0.2.12 or 100.64.0.7)
# Exit 0 = all assertions pass.

set -uo pipefail
HOST="${1:?usage: lnd-cors-test.sh <node-host>}"
ORIGIN="http://${HOST}:18083"
BASE="http://${HOST}"
PASS=0; FAIL=0
say() { printf '%s\n' "$*"; }
ok()   { PASS=$((PASS+1)); say "  PASS: $1"; }
bad()  { FAIL=$((FAIL+1)); say "  FAIL: $1"; }

# Count ACAO header lines (case-insensitive) in a header dump.
acao_count() { grep -ci '^access-control-allow-origin:' <<<"$1"; }
acao_value() { grep -i '^access-control-allow-origin:' <<<"$1" | head -1 | sed 's/^[^:]*:[[:space:]]*//' | tr -d '\r'; }

say "== B5 LND CORS — node ${HOST} (origin ${ORIGIN}) =="

# 1) /lnd-connect-info — exactly ONE ACAO, value == origin
H=$(curl -s -m 8 -D - -o /dev/null -H "Origin: ${ORIGIN}" "${BASE}/lnd-connect-info" 2>/dev/null)
N=$(acao_count "$H"); V=$(acao_value "$H")
[ "$N" = "1" ] && ok "/lnd-connect-info has exactly 1 ACAO header" || bad "/lnd-connect-info ACAO count=$N (want 1)"
[ "$V" = "$ORIGIN" ] && ok "/lnd-connect-info ACAO value == origin" || bad "/lnd-connect-info ACAO='$V' (want '$ORIGIN')"

# 2) /proxy/lnd/v1/getinfo — ACAO present even on 401 (unauth)
H=$(curl -s -m 8 -D - -o /dev/null -H "Origin: ${ORIGIN}" "${BASE}/proxy/lnd/v1/getinfo" 2>/dev/null)
N=$(acao_count "$H")
[ "$N" -ge 1 ] && ok "/proxy/lnd/v1/getinfo has ACAO (even unauth)" || bad "/proxy/lnd/v1/getinfo missing ACAO (count=$N)"
[ "$N" -le 1 ] || bad "/proxy/lnd/v1/getinfo duplicate ACAO (count=$N)"

# 3) /proxy/lnd/v1/channels — same
H=$(curl -s -m 8 -D - -o /dev/null -H "Origin: ${ORIGIN}" "${BASE}/proxy/lnd/v1/channels" 2>/dev/null)
N=$(acao_count "$H")
[ "$N" = "1" ] && ok "/proxy/lnd/v1/channels has exactly 1 ACAO" || bad "/proxy/lnd/v1/channels ACAO count=$N (want 1)"

say ""
say "== ${HOST}: ${PASS} passed, ${FAIL} failed =="
[ "$FAIL" -eq 0 ]
