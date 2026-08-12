#!/usr/bin/env bash
# aiui-proxy-closed.sh — S-15 deployed-surface check for 13-02-PLAN.md.
#
# T-13-08..T-13-12: `/aiui/api/claude/` and `/aiui/api/ollama/` used to proxy
# to an unauthenticated Python sidecar (port 3142) holding its own API key,
# and `/aiui/api/openrouter/` was a plain unauthenticated relay to a paid
# third-party API — anyone who could reach the node's web port could spend
# the owner's budget. A green `cargo test` on model_proxy.rs proves the Rust
# handler's own logic is correct in isolation; it proves NOTHING about which
# target nginx is actually pointed at on a deployed node, whether the old
# sidecar is still listening, or whether the OpenRouter relay still exists.
# This script is that proof, against a real node (13-AI-SPEC.md S-15 — "not
# a unit test and must not be treated as one").
#
# Usage: ./aiui-proxy-closed.sh <node-host> [ssh-user]
#   HTTP checks always run, with NO session cookie, and must never see 200.
#   SSH-based infra checks (systemd unit gone, port 3142 dark, single key
#   ledger) run only when `sshpass` is installed AND $AIUI_TEST_SSH_PASS is
#   set in the environment — otherwise they report SKIP, not FAIL. Never
#   hardcode a password in this file (CLAUDE.md: never commit/push secrets).
#
# Exit 0 = every HTTP assertion passes AND every SSH assertion that ran passed.

set -uo pipefail
HOST="${1:?usage: aiui-proxy-closed.sh <node-host> [ssh-user]}"
SSH_USER="${2:-archipelago}"
BASE="http://${HOST}"
PASS=0; FAIL=0; SKIP=0
say() { printf '%s\n' "$*"; }
ok()   { PASS=$((PASS+1)); say "  PASS: $1"; }
bad()  { FAIL=$((FAIL+1)); say "  FAIL: $1"; }
skip() { SKIP=$((SKIP+1)); say "  SKIP: $1"; }

# $1=method $2=path $3=optional body
status_of() {
  if [ -n "${3:-}" ]; then
    curl -s -m 10 -o /dev/null -w '%{http_code}' -X "$1" "${BASE}${2}" -d "$3" 2>/dev/null
  else
    curl -s -m 10 -o /dev/null -w '%{http_code}' -X "$1" "${BASE}${2}" 2>/dev/null
  fi
}

# $1=label $2=observed status — closed means 401/403/404; 200 is the exposure.
assert_closed() {
  case "$2" in
    401|403|404) ok "$1 -> $2 (closed)" ;;
    200) bad "$1 -> 200 (OPEN — an unauthenticated caller reached the backend)" ;;
    *) bad "$1 -> '$2' (unexpected — want 401/403/404, and it is not 200 either)" ;;
  esac
}

say "== S-15 AIUI model-proxy closure — node ${HOST} =="

# 1) /aiui/api/claude/v1/messages — no session cookie must never reach Anthropic.
S=$(status_of POST /aiui/api/claude/v1/messages '{"model":"claude-3-5-sonnet-20241022","max_tokens":1,"messages":[]}')
assert_closed "POST /aiui/api/claude/v1/messages (no session)" "$S"

# 2) /aiui/api/ollama/api/tags — no session cookie must never reach local Ollama.
S=$(status_of GET /aiui/api/ollama/api/tags)
assert_closed "GET /aiui/api/ollama/api/tags (no session)" "$S"

# 3) /aiui/api/openrouter/ — must be entirely GONE, not merely gated: 404 specifically.
S=$(status_of GET /aiui/api/openrouter/)
if [ "$S" = "404" ]; then
  ok "GET /aiui/api/openrouter/ -> 404 (relay deleted)"
else
  bad "GET /aiui/api/openrouter/ -> $S (want 404 — the relay must not exist at all, not just be gated)"
fi

say ""
say "-- SSH-based infra checks (${SSH_USER}@${HOST}) --"
if ! command -v sshpass >/dev/null 2>&1 || [ -z "${AIUI_TEST_SSH_PASS:-}" ]; then
  skip "claude-api-proxy systemd unit (no sshpass or AIUI_TEST_SSH_PASS unset)"
  skip "port 3142 listener (no sshpass or AIUI_TEST_SSH_PASS unset)"
  skip "single key ledger (no sshpass or AIUI_TEST_SSH_PASS unset)"
else
  ssh_run() {
    sshpass -p "${AIUI_TEST_SSH_PASS}" ssh -o StrictHostKeyChecking=no \
      -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR \
      "${SSH_USER}@${HOST}" "$1" 2>/dev/null
  }

  UNIT_STATE=$(ssh_run 'systemctl is-active claude-api-proxy 2>&1')
  case "$UNIT_STATE" in
    inactive|unknown) ok "claude-api-proxy unit is '$UNIT_STATE'" ;;
    *"could not be found"*) ok "claude-api-proxy unit is gone (could not be found)" ;;
    active) bad "claude-api-proxy unit is still ACTIVE — sidecar not torn down" ;;
    *) bad "claude-api-proxy unit state unexpected: '$UNIT_STATE'" ;;
  esac

  PORT_COUNT=$(ssh_run "ss -ltn 2>/dev/null | grep -c ':3142 '")
  [ "${PORT_COUNT:-1}" = "0" ] && ok "nothing listening on :3142" || bad "port 3142 still has a listener (count=${PORT_COUNT:-?})"

  # claude-api-key's PRESENCE depends on whether an operator has configured a
  # key on this node at all (via system.settings.set claude_api_key) — a
  # freshly provisioned/dev node with no key set is expected to have neither
  # file, and that is not a defect in this fix. The security-relevant
  # invariant this plan makes is narrower and unconditional: the SECOND
  # ledger (claude-api-proxy.env) must never exist, whether or not the first
  # one does. Presence of claude-api-key is reported for visibility only.
  LEDGER=$(ssh_run 'sudo ls /var/lib/archipelago/secrets/ 2>/dev/null')
  if grep -qx 'claude-api-key' <<<"$LEDGER"; then
    say "  INFO: claude-api-key ledger present (a key is configured on this node)"
  else
    say "  INFO: claude-api-key ledger absent (no key configured on this node yet — not a defect)"
  fi
  if grep -q 'claude-api-proxy.env' <<<"$LEDGER"; then
    bad "claude-api-proxy.env still present — second key ledger not deleted"
  else
    ok "claude-api-proxy.env absent (single ledger enforced)"
  fi
fi

say ""
say "== ${HOST}: ${PASS} passed, ${FAIL} failed, ${SKIP} skipped =="
[ "$FAIL" -eq 0 ]
