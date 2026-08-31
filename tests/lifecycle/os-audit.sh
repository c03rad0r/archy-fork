#!/usr/bin/env bash
# tests/lifecycle/os-audit.sh — one non-destructive OS-wide health gate.
#
# Ties together, in a single pass with one scorecard + exit code:
#   A. Backend / RPC health      — node is up, not wedged mid-OTA, core daemons answer
#   B. All-apps lifecycle audit  — every catalog app: valid state, real health,
#                                  reachable launch URL, populated launch metadata
#                                  (delegates to remote-lifecycle.sh, audit-only)
#   C. FM-guards                 — the concrete failure modes that have bitten the
#                                  fleet: port-drift (FM8), secret-completeness (FM2),
#                                  orphaned container states (FM9), OTA wedge (FM12)
#   D. Host capture (#144)        — kdump + rasdaemon baseline: crash dumps configured
#                                  and reserved, hang policy live, ECC recording running
#
# Everything here is READ-ONLY: no install/stop/start/uninstall, no service bounce.
# Safe to run against a live production node. It is the per-boot building block the
# reboot-survival harness (L3) calls after each reboot.
#
# Env:
#   ARCHY_HOST       (default 127.0.0.1)
#   ARCHY_SCHEME     (default https; use http for .116 / nginx-:80-only nodes)
#   ARCHY_PASSWORD   (required)
#   ARCHY_LOCAL      (auto: 1 when ARCHY_HOST is loopback) — gates host-only podman checks
#
# Usage:
#   ARCHY_HOST=127.0.0.1 ARCHY_SCHEME=http ARCHY_PASSWORD=... tests/lifecycle/os-audit.sh
#
# Exit: 0 = every section green; 1 = one or more checks failed; 2 = setup/usage error.

set -uo pipefail

HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

ARCHY_HOST="${ARCHY_HOST:-127.0.0.1}"
ARCHY_SCHEME="${ARCHY_SCHEME:-https}"
ARCHY_PASSWORD="${ARCHY_PASSWORD:-}"
BASE_URL="${ARCHY_SCHEME}://${ARCHY_HOST}"

# Host-only checks (podman sweeps) make sense only when this script runs ON the node.
if [[ -z "${ARCHY_LOCAL:-}" ]]; then
  case "$ARCHY_HOST" in
    127.0.0.1|localhost|::1) ARCHY_LOCAL=1 ;;
    *) ARCHY_LOCAL=0 ;;
  esac
fi

if [[ -z "$ARCHY_PASSWORD" ]]; then
  echo "ARCHY_PASSWORD env var must be set." >&2
  exit 2
fi
for tool in curl jq; do
  command -v "$tool" >/dev/null 2>&1 || { echo "missing required tool: $tool" >&2; exit 2; }
done

# ── scorecard state ───────────────────────────────────────────────────────────
PASS=0; FAIL=0; WARN=0
declare -a RESULTS=()
record() { # record <PASS|FAIL|WARN> <label> [detail]
  local status="$1" label="$2" detail="${3:-}"
  case "$status" in
    PASS) PASS=$((PASS+1)) ;;
    FAIL) FAIL=$((FAIL+1)) ;;
    WARN) WARN=$((WARN+1)) ;;
  esac
  RESULTS+=("$(printf '%-4s %-38s %s' "$status" "$label" "$detail")")
  printf '  [%s] %s %s\n' "$status" "$label" "$detail"
}

# ── minimal RPC client (session + CSRF) ────────────────────────────────────────
SESSION=""; CSRF=""
rpc_login() {
  local hdr; hdr=$(mktemp)
  curl -sk -D "$hdr" -X POST "${BASE_URL}/rpc/v1" -H 'Content-Type: application/json' \
    -d "$(jq -nc --arg p "$ARCHY_PASSWORD" '{jsonrpc:"2.0",id:1,method:"auth.login",params:{password:$p}}')" \
    -o /dev/null 2>/dev/null
  SESSION=$(grep -i '^set-cookie: session=' "$hdr" | head -1 | sed -E 's/.*session=([^;]+).*/\1/' | tr -d '\r')
  CSRF=$(grep -i '^set-cookie: csrf_token=' "$hdr" | head -1 | sed -E 's/.*csrf_token=([^;]+).*/\1/' | tr -d '\r')
  rm -f "$hdr"
  [[ -n "$SESSION" && -n "$CSRF" ]]
}
# rpc <method> [params-json] -> prints raw JSON response
rpc() {
  local method="$1" params="${2:-{\}}"
  curl -sk -X POST "${BASE_URL}/rpc/v1" -H 'Content-Type: application/json' \
    -H "Cookie: session=${SESSION}; csrf_token=${CSRF}" -H "X-CSRF-Token: ${CSRF}" \
    -d "$(jq -nc --arg m "$method" --argjson p "$params" '{jsonrpc:"2.0",id:2,method:$m,params:$p}')" 2>/dev/null
}
# rpc_ok <method> [params] -> 0 if a result came back with no error
rpc_ok() {
  local resp; resp=$(rpc "$@")
  [[ -n "$resp" ]] && [[ "$(jq -r '.error // empty' <<<"$resp" 2>/dev/null)" == "" ]] \
    && [[ "$(jq -r 'has("result")' <<<"$resp" 2>/dev/null)" == "true" ]]
}

# ══ Section A — Backend / RPC health ═══════════════════════════════════════════
section_a() {
  echo
  echo "== A. Backend / RPC health =="

  # unauth health probe first (doesn't need a session)
  local health; health=$(curl -sk -X POST "${BASE_URL}/rpc/v1" -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"health","params":{}}' 2>/dev/null)
  if [[ "$(jq -r '.result.status // empty' <<<"$health" 2>/dev/null)" =~ ^(ok|degraded)$ ]]; then
    record PASS "node responds (health)" "status=$(jq -r '.result.status' <<<"$health")"
  else
    record FAIL "node responds (health)" "no/invalid health response — node down?"
    return
  fi

  if ! rpc_login; then
    record FAIL "auth.login" "could not establish session (wrong password or rate-limited)"
    return
  fi
  record PASS "auth.login" "session established"

  # FM12 — OTA must not be wedged mid-apply.
  # NB: must use has() not `//` — jq's `//` treats a legit `false` as empty and
  # would fall through to "unknown" on a perfectly healthy node.
  local us; us=$(rpc update.status)
  local inprog; inprog=$(jq -r '
    if   (.result|type=="object") and (.result|has("update_in_progress")) then .result.update_in_progress
    elif (.result|type=="object") and (.result|has("in_progress"))        then .result.in_progress
    else "unknown" end' <<<"$us" 2>/dev/null)
  if [[ "$inprog" == "false" ]]; then
    record PASS "OTA not wedged (update.status)" "update_in_progress=false"
  elif [[ "$inprog" == "unknown" ]]; then
    record WARN "OTA not wedged (update.status)" "could not read update_in_progress"
  else
    record FAIL "OTA not wedged (update.status)" "update_in_progress=$inprog (FM12 wedge)"
  fi

  # Core daemons answer (only assert for ones present on this node)
  if rpc_ok bitcoin.getinfo || rpc_ok bitcoin.relay-status; then
    record PASS "bitcoin RPC reachable" ""
  else
    record WARN "bitcoin RPC reachable" "bitcoin.getinfo/relay-status did not answer (not installed?)"
  fi
  # LND wallet must be UNLOCKED. NB: lnd.getinfo masks a locked wallet (it
  # returns an all-zero success, error:null), so it can't detect the lock. Probe
  # the actual receive path (lnd.newaddress) instead: a LOCKED wallet returns the
  # LND_WALLET_LOCKED reason code — the exact fleet-wide receive breakage. A
  # locked wallet is a hard FAIL; "not installed" is a WARN. (newaddress derives
  # a fresh address — harmless; LND tolerates address gaps.)
  if rpc_ok lnd.getinfo; then
    local na; na=$(rpc lnd.newaddress)
    if grep -qE "LND_WALLET_LOCKED|wallet is locked|WALLET_LOCKED" <<<"$na"; then
      record FAIL "lnd wallet unlocked (lnd.newaddress)" "wallet LOCKED — auto-unlock failed (Bitcoin-receive broken)"
    elif [[ "$(jq -r '(has("result") and (.result!=null))' <<<"$na" 2>/dev/null)" == "true" ]]; then
      record PASS "lnd wallet unlocked (lnd.newaddress)" ""
    else
      record WARN "lnd wallet unlocked (lnd.newaddress)" "newaddress: $(jq -rc '.error.message // "no address"' <<<"$na" 2>/dev/null | head -c 60)"
    fi
  else
    record WARN "lnd RPC reachable" "lnd.getinfo did not answer (not installed?)"
  fi
  if rpc_ok system.stats || rpc_ok system.get-metrics; then
    record PASS "system metrics reachable" ""
  else
    record WARN "system metrics reachable" "system.stats/get-metrics did not answer"
  fi

  # FM13 — disk pressure early-warning (best-effort; field names vary by version)
  local ds; ds=$(rpc system.disk-status)
  local usep; usep=$(jq -r '[.result.use_percent,.result.used_percent,.result.percent]|map(select(.!=null))|first // empty' <<<"$ds" 2>/dev/null)
  if [[ -n "$usep" ]]; then
    if (( ${usep%.*} >= 90 )); then
      record FAIL "disk pressure (system.disk-status)" "${usep}% used (FM13 risk)"
    else
      record PASS "disk pressure (system.disk-status)" "${usep}% used"
    fi
  fi
}

# ══ Section B — All-apps lifecycle audit (delegates to remote-lifecycle.sh) ═════
section_b() {
  echo
  echo "== B. All-apps lifecycle audit (non-destructive, all catalog apps) =="
  local out rc
  # No ARCHY_APPS + no ARCHY_FULL_LIFECYCLE => audit every catalog app (audit_app).
  out=$(ARCHY_HOST="$ARCHY_HOST" ARCHY_SCHEME="$ARCHY_SCHEME" ARCHY_PASSWORD="$ARCHY_PASSWORD" \
        ARCHY_APPS="" ARCHY_FULL_LIFECYCLE=0 \
        "$HERE/remote-lifecycle.sh" 2>&1)
  rc=$?
  # Surface the per-app lines but drop the noisy optional-probe jq parse errors.
  echo "$out" | grep -vE '^jq: (parse )?error' | sed 's/^/    /'
  if (( rc == 0 )); then
    record PASS "broad all-apps audit" "remote-lifecycle.sh exit 0"
  else
    local n; n=$(echo "$out" | grep -oE 'FAILED checks: [0-9]+' | grep -oE '[0-9]+' | tail -1)
    record FAIL "broad all-apps audit" "remote-lifecycle.sh exit $rc (${n:-?} app checks failed)"
  fi
}

# ══ Section C — FM-guards ══════════════════════════════════════════════════════
run_bats_guard() { # run_bats_guard <suite> <label> <fm>
  local suite="$1" label="$2" fm="$3" out rc
  if ! command -v bats >/dev/null 2>&1; then
    record WARN "$label" "bats not installed — $fm guard skipped"
    return
  fi
  out=$(ARCHY_HOST="$ARCHY_HOST" ARCHY_SCHEME="$ARCHY_SCHEME" ARCHY_PASSWORD="$ARCHY_PASSWORD" \
        "$HERE/run.sh" "$suite" 2>&1); rc=$?
  if (( rc == 0 )); then
    record PASS "$label" "$fm guard green"
  else
    record FAIL "$label" "$fm — $(echo "$out" | grep -E '^not ok' | head -1)"
  fi
}

section_c() {
  echo
  echo "== C. FM-guards (the concrete fleet failure modes) =="
  run_bats_guard port-drift         "port bindings match manifest" "FM8"
  run_bats_guard secret-completeness "all referenced secrets exist" "FM2"

  # FM9 — orphaned container states (host-only: needs local podman)
  if [[ "$ARCHY_LOCAL" == "1" ]] && command -v podman >/dev/null 2>&1; then
    local orphans
    orphans=$(podman ps -a --format '{{.Names}} {{.Status}}' 2>/dev/null \
              | grep -iE '(^| )(stopping|removing|created)( |$)' || true)
    if [[ -z "$orphans" ]]; then
      record PASS "no orphaned container states" "no stopping/removing/created"
    else
      record FAIL "no orphaned container states" "FM9: $(echo "$orphans" | tr '\n' ';')"
    fi
  else
    record WARN "no orphaned container states" "remote node — host podman sweep skipped"
  fi
}

# ══ Section D — host capture (#144): kdump + rasdaemon ═══════════════════════
section_d() {
  echo
  echo "== D. Host capture — crash + hardware-error evidence (#144) =="
  if [[ "$ARCHY_LOCAL" != "1" ]]; then
    record WARN "kdump + rasdaemon baseline" "remote node — host checks skipped"
    return
  fi
  # D1. kdump enabled in config (image bakes it in; OTA host fixups converge)
  if grep -qE '^USE_KDUMP=.?1' /etc/default/kdump-tools 2>/dev/null; then
    record PASS "kdump-tools configured" "USE_KDUMP=1, dumps to /var/crash"
  else
    record FAIL "kdump-tools configured" "/etc/default/kdump-tools missing USE_KDUMP=1 — host fixup didn't land"
  fi
  # D2. crashkernel reservation — memory is reserved at BOOT, so a node that
  # took the OTA fixup but hasn't rebooted yet is WARN, not FAIL.
  if grep -q 'crashkernel=' /proc/cmdline 2>/dev/null; then
    record PASS "crashkernel reserved" "$(grep -oE 'crashkernel=[^ ]+' /proc/cmdline | head -1)"
  elif grep -q 'crashkernel=' /etc/default/grub 2>/dev/null; then
    record WARN "crashkernel reserved" "written to GRUB — applies on next reboot"
  else
    record FAIL "crashkernel reserved" "absent from /proc/cmdline AND /etc/default/grub"
  fi
  # D3. hang/panic capture policy — runtime-settable, expected immediately
  if [[ "$(cat /proc/sys/kernel/hung_task_panic 2>/dev/null)" == "1" ]]; then
    record PASS "hang-capture policy live" "kernel.hung_task_panic=1"
  else
    record FAIL "hang-capture policy live" "kernel.hung_task_panic!=1 — sysctl drop-in not applied"
  fi
  # D4. rasdaemon recording hardware errors (ECC/AER events → sqlite)
  if systemctl is-active --quiet rasdaemon 2>/dev/null; then
    record PASS "rasdaemon active" "hardware-error events recorded to /var/lib/rasdaemon"
  else
    record FAIL "rasdaemon active" "service not running — package missing or host fixup failed"
  fi
}

# ── run ────────────────────────────────────────────────────────────────────────
echo "=============================================================="
echo " OS-wide audit — ${BASE_URL}  ($(date '+%Y-%m-%d %H:%M:%S'))"
echo " local=${ARCHY_LOCAL}"
echo "=============================================================="
section_a
# Only proceed to apps/FM-guards if the node itself answered.
if (( FAIL == 0 )) || [[ -n "$SESSION" ]]; then
  section_b
  section_c
fi
# Host-capture baseline is independent of RPC health: a wedged backend must
# not mask that the node also stopped capturing evidence.
section_d

echo
echo "=============================================================="
echo " SCORECARD: ${PASS} pass / ${FAIL} fail / ${WARN} warn"
echo "=============================================================="
printf '%s\n' "${RESULTS[@]}"
echo
if (( FAIL > 0 )); then
  echo "RESULT: FAIL ($FAIL critical checks failed)"
  exit 1
fi
echo "RESULT: PASS"
exit 0
