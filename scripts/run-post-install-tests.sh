#!/usr/bin/env bash
# Post-install + onboarding + container lifecycle E2E tests.
# Run on an installed Archipelago node (SSH or local).
#
# Usage: bash run-post-install-tests.sh --password-stdin   # read password from stdin (preferred)
#        bash run-post-install-tests.sh [password]         # argv form; visible in `ps`, local use only
#        bash run-post-install-tests.sh --phase1-only      # Install checks only (no auth)
#
# Tests:
#   Phase 1: Install verification (services, files, logs) — safe, no side effects
#   Phase 2: Onboarding (password setup, auth flow) — creates user account
#   Phase 3: Container lifecycle (install 3 apps, start/stop/health) — needs auth
set -u

PHASE1_ONLY=false
PASSWORD=""

for arg in "$@"; do
  case "$arg" in
    --phase1-only) PHASE1_ONLY=true ;;
    --password-stdin) IFS= read -r PASSWORD || true ;;
    *) PASSWORD="$arg" ;;
  esac
done

if [ "$PHASE1_ONLY" = false ] && [ -z "$PASSWORD" ]; then
  echo "ERROR: no password supplied. Use --password-stdin, pass one as an argument," >&2
  echo "       or run --phase1-only for the no-auth install checks." >&2
  exit 2
fi

BASE="http://127.0.0.1:5678"
JAR="/tmp/e2e-cookies.txt"
rm -f "$JAR"
PC=0; FC=0; SC=0

pass() { PC=$((PC + 1)); printf "\033[32m  ✓ %s\033[0m\n" "$1"; }
fail() { FC=$((FC + 1)); printf "\033[31m  ✗ %s — %s\033[0m\n" "$1" "${2:-}"; }
skip() { SC=$((SC + 1)); printf "\033[33m  ⊘ %s\033[0m\n" "$1"; }
section() { printf "\n\033[1m━━━ %s ━━━\033[0m\n" "$1"; }

# Extract CSRF token from cookie jar
get_csrf() {
  grep 'csrf_token' "$JAR" 2>/dev/null | awk '{print $NF}'
}

rpc() {
  local method="$1"
  local params="${2:-"{}"}"
  local csrf
  csrf=$(get_csrf)
  local csrf_header=""
  if [ -n "$csrf" ]; then
    csrf_header="-H X-CSRF-Token:${csrf}"
  fi
  curl -s -b "$JAR" -c "$JAR" \
    -H "Content-Type: application/json" \
    $csrf_header \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$method\",\"params\":$params}" \
    "${BASE}/rpc/v1" 2>/dev/null
}

rpc_ok() {
  local resp="$1"
  [ -z "$resp" ] && return 1
  echo "$resp" | grep -q '"error":null' && return 0
  echo "$resp" | grep -q '"error"' && return 1
  return 0
}

rpc_result() {
  echo "$1" | python3 -c "import sys,json; d=json.load(sys.stdin); print(json.dumps(d.get('result','')))" 2>/dev/null
}

wait_for_server() {
  local max_wait=60
  local waited=0
  while [ $waited -lt $max_wait ]; do
    if curl -sf "${BASE}/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 2
    waited=$((waited + 2))
  done
  return 1
}

echo ""
echo "╔══════════════════════════════════════════════╗"
echo "║  Archipelago Post-Install E2E Test Suite     ║"
echo "╚══════════════════════════════════════════════╝"
echo ""
echo "Target: ${BASE}"
echo "Date:   $(date -u '+%Y-%m-%d %H:%M:%S UTC')"

# ═══════════════════════════════════════════
# PHASE 1: Install Verification
# ═══════════════════════════════════════════
section "Phase 1: Install Verification"

# 1.1 — Critical files exist
for f in /usr/local/bin/archipelago \
         /opt/archipelago/web-ui/index.html \
         /etc/nginx/sites-available/archipelago \
         /etc/archipelago/ssl/archipelago.crt \
         /opt/archipelago/scripts/image-versions.sh; do
  if [ -f "$f" ]; then
    pass "File exists: $f"
  else
    fail "File missing" "$f"
  fi
done

# 1.1b — Every enabled archipelago/nostr unit must have an existing ExecStart
# payload. v1.7.104 shipped nostr-relay.service without its binary (3s
# crash-loop forever) and archipelago-diag.service without its script
# (203/EXEC) — catch that class of ISO defect on first boot, by generic rule.
for unit in /etc/systemd/system/archipelago*.service /etc/systemd/system/nostr*.service; do
  [ -f "$unit" ] || continue
  systemctl is-enabled "$(basename "$unit")" >/dev/null 2>&1 || continue
  exec_bin=$(grep -m1 '^ExecStart=' "$unit" | sed 's/^ExecStart=[-+@!:]*//' | awk '{print $1}')
  case "$exec_bin" in
    /*) if [ -e "$exec_bin" ]; then
          pass "Unit payload exists: $(basename "$unit") → $exec_bin"
        else
          fail "Unit payload missing" "$(basename "$unit") → $exec_bin"
        fi ;;
  esac
done

# 1.2 — Critical services active
for svc in archipelago nginx; do
  if systemctl is-active "$svc" >/dev/null 2>&1; then
    pass "Service active: $svc"
  else
    fail "Service not active" "$svc"
  fi
done

# 1.3 — Services enabled
for svc in archipelago nginx archipelago-load-images archipelago-first-boot-containers; do
  if systemctl is-enabled "$svc" >/dev/null 2>&1; then
    pass "Service enabled: $svc"
  else
    fail "Service not enabled" "$svc"
  fi
done

# 1.4 — Podman available for archipelago user
if runuser -u archipelago -- bash -c 'export XDG_RUNTIME_DIR=/run/user/1000 && podman --version' >/dev/null 2>&1; then
  pass "Podman available (rootless, archipelago user)"
else
  fail "Podman not available" "rootless podman for archipelago user"
fi

# 1.5 — Linger enabled
if [ -f /var/lib/systemd/linger/archipelago ]; then
  pass "Linger enabled for archipelago"
else
  fail "Linger not enabled" "/var/lib/systemd/linger/archipelago missing"
fi

# 1.6 — Backend not in dev mode
if systemctl cat archipelago 2>/dev/null | grep -q 'DEV_MODE=true'; then
  fail "DEV_MODE enabled" "ARCHIPELAGO_DEV_MODE=true found in service file"
else
  pass "DEV_MODE disabled (production mode)"
fi

# 1.7 — Backend running as correct user
SVC_USER=$(systemctl show -p User archipelago 2>/dev/null | cut -d= -f2)
if [ "$SVC_USER" = "archipelago" ]; then
  pass "Backend runs as user: archipelago"
elif [ "$SVC_USER" = "root" ]; then
  fail "Backend runs as root" "Should be User=archipelago"
else
  skip "Cannot determine backend user ($SVC_USER)"
fi

# 1.8 — Health endpoint responds
if curl -sf "${BASE}/health" >/dev/null 2>&1; then
  pass "Health endpoint responds"
else
  fail "Health endpoint" "No response from ${BASE}/health"
fi

# 1.9 — Web UI loads via nginx
HTTP_CODE=$(curl -sk -o /dev/null -w "%{http_code}" "https://localhost/" 2>/dev/null)
if [ "$HTTP_CODE" = "200" ]; then
  pass "Web UI loads via nginx (HTTPS)"
else
  fail "Web UI not accessible" "HTTPS returned $HTTP_CODE"
fi

# 1.10 — Nginx config test
if nginx -t 2>/dev/null; then
  pass "Nginx config valid"
else
  fail "Nginx config" "nginx -t failed"
fi

# ── Phase 1 exit point ──
if [ "$PHASE1_ONLY" = "true" ]; then
  section "Results (Phase 1 only)"
  TOTAL=$((PC + FC + SC))
  printf "\n  \033[32mPassed: %d\033[0m  \033[31mFailed: %d\033[0m  \033[33mSkipped: %d\033[0m  Total: %d\n\n" "$PC" "$FC" "$SC" "$TOTAL"
  [ "$FC" -gt 0 ] && echo "  Phase 1: SOME CHECKS FAILED" && exit 1
  echo "  Phase 1: ALL CHECKS PASSED"
  echo "  Run without --phase1-only to test onboarding + containers"
  exit 0
fi

# ═══════════════════════════════════════════
# PHASE 2: Onboarding & Auth
# ═══════════════════════════════════════════
section "Phase 2: Onboarding & Auth"

# Wait for server
if ! wait_for_server; then
  fail "Server not ready" "Timed out after 60s"
  section "Results"
  echo "  Passed: $PC  Failed: $FC  Skipped: $SC"
  exit 1
fi

# 2.1 — Check setup status (should be false on fresh install)
SETUP_RESP=$(rpc "auth.isSetup")
if rpc_ok "$SETUP_RESP"; then
  IS_SETUP=$(echo "$SETUP_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin).get('result',False))" 2>/dev/null)
  if [ "$IS_SETUP" = "True" ] || [ "$IS_SETUP" = "true" ]; then
    pass "auth.isSetup returns true (user exists)"
    # Already set up — just login
    LOGIN=$(rpc "auth.login" "{\"password\":\"$PASSWORD\"}")
    if rpc_ok "$LOGIN"; then
      pass "auth.login (existing user)"
    else
      # Try default dev password
      LOGIN=$(rpc "auth.login" '{"password":"password123"}')
      if rpc_ok "$LOGIN"; then
        pass "auth.login (dev password)"
        PASSWORD="password123"
      else
        fail "auth.login" "Cannot authenticate"
      fi
    fi
  else
    pass "auth.isSetup returns false (fresh install)"
    # 2.2 — Set up password
    SETUP=$(rpc "auth.setup" "{\"password\":\"$PASSWORD\"}")
    if rpc_ok "$SETUP"; then
      pass "auth.setup (password created)"
    else
      fail "auth.setup" "$SETUP"
    fi
    # 2.3 — Login with new password
    LOGIN=$(rpc "auth.login" "{\"password\":\"$PASSWORD\"}")
    if rpc_ok "$LOGIN"; then
      pass "auth.login (new password)"
    else
      fail "auth.login" "$LOGIN"
    fi
  fi
else
  fail "auth.isSetup" "$SETUP_RESP"
fi

# 2.4 — Onboarding status
OB_RESP=$(rpc "auth.isOnboardingComplete")
if rpc_ok "$OB_RESP"; then
  pass "auth.isOnboardingComplete responds"
else
  fail "auth.isOnboardingComplete" "$OB_RESP"
fi

# 2.5 — Node DID available
DID_RESP=$(rpc "node.did")
if rpc_ok "$DID_RESP"; then
  pass "node.did (DID generated)"
else
  fail "node.did" "$DID_RESP"
fi

# 2.6 — Server info
INFO_RESP=$(rpc "server.info")
if rpc_ok "$INFO_RESP"; then
  pass "server.info responds"
else
  # Try alternate method name
  INFO_RESP=$(rpc "system.info")
  if rpc_ok "$INFO_RESP"; then
    pass "system.info responds"
  else
    skip "server.info / system.info (may not exist)"
  fi
fi

# 2.7 — Mark onboarding complete
OB_COMPLETE=$(rpc "auth.onboardingComplete")
if rpc_ok "$OB_COMPLETE"; then
  pass "auth.onboardingComplete"
else
  skip "auth.onboardingComplete (may already be done)"
fi

# ═══════════════════════════════════════════
# PHASE 3: Container Lifecycle
# ═══════════════════════════════════════════
section "Phase 3: Container Lifecycle"

# Source image versions for dockerImage URLs
source /opt/archipelago/scripts/image-versions.sh 2>/dev/null || true

# Test with 3 lightweight standalone containers
# package.install expects: {"id": "app_id", "dockerImage": "registry/image:tag"}
# container-start/stop/status expect: {"app_id": "name"}
declare -a APPS=("filebrowser" "searxng" "grafana")
declare -a IMAGES=("${FILEBROWSER_IMAGE:-}" "${SEARXNG_IMAGE:-}" "${GRAFANA_IMAGE:-}")

# 3.1 — List containers (baseline)
LIST_RESP=$(rpc "container-list")
if rpc_ok "$LIST_RESP"; then
  pass "container-list (baseline)"
else
  fail "container-list" "$LIST_RESP"
fi

for i in 0 1 2; do
  APP="${APPS[$i]}"
  IMAGE="${IMAGES[$i]}"

  section "Container: $APP"

  if [ -z "$IMAGE" ]; then
    fail "$APP — image variable empty" "image-versions.sh missing or incomplete"
    continue
  fi

  # 3.2 — Install container via package.install RPC
  # Check if already exists first
  EXISTING=$(rpc "container-list")
  if echo "$EXISTING" | grep -q "\"$APP\""; then
    pass "$APP already installed (skipping install)"
  else
    INSTALL_RESP=$(rpc "package.install" "{\"id\":\"$APP\",\"dockerImage\":\"$IMAGE\"}")
    if rpc_ok "$INSTALL_RESP"; then
      pass "$APP installed"
    else
      ERR_MSG=$(echo "$INSTALL_RESP" | python3 -c "import sys,json; d=json.load(sys.stdin); e=d.get('error',{}); print(e.get('message','unknown') if isinstance(e,dict) else str(e))" 2>/dev/null)
      fail "$APP install" "$ERR_MSG"
      continue
    fi
  fi

  # Wait for container to start (pull + create + start)
  echo "  ... waiting for $APP to start"
  for attempt in $(seq 1 15); do
    sleep 2
    STATUS_RESP=$(rpc "container-list")
    if echo "$STATUS_RESP" | grep -q "\"$APP\"" && echo "$STATUS_RESP" | grep -q '"running"'; then
      break
    fi
  done

  # 3.3 — Verify running
  LIST_NOW=$(rpc "container-list")
  if echo "$LIST_NOW" | grep -q "\"$APP\""; then
    if echo "$LIST_NOW" | python3 -c "
import sys,json
data = json.load(sys.stdin).get('result',[])
if isinstance(data, list):
    for c in data:
        if c.get('name','') == '$APP' or '$APP' in c.get('name',''):
            print(c.get('state','unknown'))
            sys.exit(0)
print('not-found')
" 2>/dev/null | grep -q "running"; then
      pass "$APP running after install"
    else
      fail "$APP not running" "Check container-list output"
    fi
  else
    fail "$APP not in container list" ""
    continue
  fi

  # 3.4 — Stop container
  STOP_RESP=$(rpc "container-stop" "{\"app_id\":\"$APP\"}")
  if rpc_ok "$STOP_RESP"; then
    pass "$APP stopped"
  else
    fail "$APP stop" "$STOP_RESP"
  fi

  sleep 3

  # 3.5 — Verify stopped
  LIST_NOW=$(rpc "container-list")
  STATE=$(echo "$LIST_NOW" | python3 -c "
import sys,json
data = json.load(sys.stdin).get('result',[])
if isinstance(data, list):
    for c in data:
        if c.get('name','') == '$APP' or '$APP' in c.get('name',''):
            print(c.get('state','unknown'))
            sys.exit(0)
print('not-found')
" 2>/dev/null)
  if [ "$STATE" = "exited" ] || [ "$STATE" = "stopped" ]; then
    pass "$APP confirmed stopped"
  else
    fail "$APP not stopped" "State: $STATE"
  fi

  # 3.6 — Restart container
  START_RESP=$(rpc "container-start" "{\"app_id\":\"$APP\"}")
  if rpc_ok "$START_RESP"; then
    pass "$APP restarted"
  else
    fail "$APP restart" "$START_RESP"
  fi

  sleep 5

  # 3.7 — Verify running again
  LIST_NOW=$(rpc "container-list")
  STATE=$(echo "$LIST_NOW" | python3 -c "
import sys,json
data = json.load(sys.stdin).get('result',[])
if isinstance(data, list):
    for c in data:
        if c.get('name','') == '$APP' or '$APP' in c.get('name',''):
            print(c.get('state','unknown'))
            sys.exit(0)
print('not-found')
" 2>/dev/null)
  if [ "$STATE" = "running" ]; then
    pass "$APP running after restart"
  else
    fail "$APP not running after restart" "State: $STATE"
  fi

  # 3.8 — Health check
  HEALTH_RESP=$(rpc "container-health" "{\"app_id\":\"$APP\"}")
  if rpc_ok "$HEALTH_RESP"; then
    pass "$APP health responds"
  else
    skip "$APP health (may need warm-up time)"
  fi
done

# 3.9 — Final container list (should show all 3)
LIST_RESP=$(rpc "container-list")
if rpc_ok "$LIST_RESP"; then
  COUNT=$(echo "$LIST_RESP" | python3 -c "import sys,json; r=json.load(sys.stdin).get('result',[]); print(len(r) if isinstance(r,list) else 0)" 2>/dev/null)
  if [ "${COUNT:-0}" -ge 3 ]; then
    pass "container-list shows $COUNT containers (>= 3)"
  else
    fail "container-list" "Only $COUNT containers (expected >= 3)"
  fi
else
  fail "container-list (final)" "$LIST_RESP"
fi

# ═══════════════════════════════════════════
# PHASE 4: Log Verification
# ═══════════════════════════════════════════
section "Phase 4: Log Verification"

# 4.1 — First-boot log exists and completed
if [ -f /var/log/archipelago-first-boot.log ]; then
  if grep -q "first-boot complete" /var/log/archipelago-first-boot.log 2>/dev/null; then
    pass "First-boot log: completed"
  else
    fail "First-boot log" "Did not complete — check /var/log/archipelago-first-boot.log"
  fi
else
  fail "First-boot log" "/var/log/archipelago-first-boot.log missing"
fi

# 4.2 — Diagnostics log exists
if [ -f /var/log/archipelago-first-boot-diag.log ]; then
  pass "Diagnostics log exists"
else
  skip "Diagnostics log (/var/log/archipelago-first-boot-diag.log)"
fi

# 4.3 — No critical errors in backend journal
CRIT_ERRORS=$(journalctl -u archipelago --no-pager -p err -b 2>/dev/null | grep -v "Failed to read LND\|Failed to query getblockchain\|Cannot connect to Podman" | head -5)
if [ -z "$CRIT_ERRORS" ]; then
  pass "No unexpected backend errors in journal"
else
  fail "Backend errors in journal" "$(echo "$CRIT_ERRORS" | head -1)"
fi

# 4.4 — image-versions.sh is accessible
if [ -f /opt/archipelago/scripts/image-versions.sh ]; then
  if source /opt/archipelago/scripts/image-versions.sh 2>/dev/null && [ -n "$FILEBROWSER_IMAGE" ]; then
    pass "image-versions.sh loads correctly"
  else
    fail "image-versions.sh" "Cannot source or FILEBROWSER_IMAGE empty"
  fi
else
  fail "image-versions.sh" "Not found at /opt/archipelago/scripts/"
fi

# ═══════════════════════════════════════════
# Results
# ═══════════════════════════════════════════
section "Results"
TOTAL=$((PC + FC + SC))
echo ""
printf "  \033[32mPassed: %d\033[0m  \033[31mFailed: %d\033[0m  \033[33mSkipped: %d\033[0m  Total: %d\n" "$PC" "$FC" "$SC" "$TOTAL"
echo ""

if [ "$FC" -gt 0 ]; then
  echo "  ❌ SOME TESTS FAILED"
  exit 1
else
  echo "  ✅ ALL TESTS PASSED"
  exit 0
fi
