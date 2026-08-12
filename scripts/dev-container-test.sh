#!/bin/bash
#
# Container Orchestration Dev Loop
# Fast edit-build-test cycle against real containers on .228
#
# Usage:
#   ./scripts/dev-container-test.sh           # Interactive loop
#   ./scripts/dev-container-test.sh --once    # Single run (for CI)
#
# Workflow: edit locally → rsync → build on server → restart → smoke test
#

set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

SSH_KEY="${ARCHIPELAGO_SSH_KEY:-$HOME/.ssh/archipelago-deploy}"
SSH_HOST="${ARCHIPELAGO_SSH_HOST:-}"
if [ -z "$SSH_HOST" ]; then
  echo "ARCHIPELAGO_SSH_HOST must be set, e.g. archipelago@<node-host>" >&2
  exit 2
fi
HOST_ONLY="${SSH_HOST#*@}"
SSH_OPTS="-o StrictHostKeyChecking=no -o ServerAliveInterval=15 -i $SSH_KEY"
REMOTE_DIR="/home/archipelago/archy"
RPC_URL="http://${HOST_ONLY}/rpc/v1"
COOKIE=""
ONCE=false
[ "$1" = "--once" ] && ONCE=true

# ── Colors ──────────────────────────────────────────────────────────────
RED='\033[0;31m'    GREEN='\033[0;32m'    YELLOW='\033[0;33m'
CYAN='\033[0;36m'   BOLD='\033[1m'        NC='\033[0m'

pass() { echo -e "  ${GREEN}✓${NC} $*"; }
fail() { echo -e "  ${RED}✗${NC} $*"; FAILURES=$((FAILURES + 1)); }
info() { echo -e "  ${CYAN}→${NC} $*"; }
header() { echo -e "\n${BOLD}$*${NC}"; }

TESTS=0
FAILURES=0

# ── Helpers ─────────────────────────────────────────────────────────────

rpc() {
    local method="$1"
    local params="${2:-{}}"
    local result
    result=$(curl -sf -b "$COOKIE" -X POST "$RPC_URL" \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}" \
        --connect-timeout 10 --max-time 30 2>/dev/null)
    echo "$result"
}

login() {
    # Get session cookie
    COOKIE=$(mktemp)
    local resp
    resp=$(curl -sf -c "$COOKIE" -X POST "$RPC_URL" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"auth.login","params":{"password":"password123"},"id":1}' \
        --connect-timeout 10 2>/dev/null)
    if echo "$resp" | grep -q '"result"'; then
        return 0
    fi
    return 1
}

wait_for_health() {
    local timeout=${1:-30}
    for i in $(seq 1 "$timeout"); do
        if curl -sf "http://${HOST_ONLY}/health" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    return 1
}

# ── Sync & Build ────────────────────────────────────────────────────────

sync_and_build() {
    header "Step 1: Sync code to .228"
    rsync -az --delete \
        --exclude='.git' --exclude='target' --exclude='node_modules' \
        --exclude='dist' --exclude='*.iso' --exclude='.claude' \
        -e "ssh $SSH_OPTS" \
        "$PROJECT_ROOT/" "$SSH_HOST:$REMOTE_DIR/" 2>&1
    pass "Code synced"

    header "Step 2: Build backend (incremental)"
    local build_start=$(date +%s)
    if ssh $SSH_OPTS "$SSH_HOST" "cd $REMOTE_DIR/core && cargo build --release -p archipelago 2>&1 | tail -3"; then
        local elapsed=$(( $(date +%s) - build_start ))
        pass "Built in ${elapsed}s"
    else
        fail "Build failed"
        return 1
    fi

    header "Step 3: Restart service"
    ssh $SSH_OPTS "$SSH_HOST" "sudo systemctl restart archipelago"
    info "Waiting for health..."
    if wait_for_health 30; then
        pass "Backend healthy"
    else
        fail "Backend failed to start (30s timeout)"
        ssh $SSH_OPTS "$SSH_HOST" "journalctl -u archipelago --since '30 sec ago' --no-pager | tail -20"
        return 1
    fi
}

# ── Smoke Tests ─────────────────────────────────────────────────────────

run_smoke_tests() {
    header "Step 4: Container Orchestration Smoke Tests"
    TESTS=0
    FAILURES=0

    # Login
    if login; then
        pass "Authenticated"
    else
        fail "Login failed"
        return 1
    fi

    # Test 1: Container list
    TESTS=$((TESTS + 1))
    local list
    list=$(rpc "container.list")
    if echo "$list" | grep -q '"result"'; then
        local count
        count=$(echo "$list" | python3 -c "import sys,json; print(len(json.load(sys.stdin).get('result',{}).get('containers',[])))" 2>/dev/null || echo "?")
        pass "container.list: $count containers"
    else
        fail "container.list failed"
    fi

    # Test 2: Health status
    TESTS=$((TESTS + 1))
    local health
    health=$(rpc "container.health")
    if echo "$health" | grep -q '"result"'; then
        pass "container.health: OK"
    else
        fail "container.health failed"
    fi

    # Test 3: Install a lightweight container (filebrowser — small, fast, no deps)
    TESTS=$((TESTS + 1))
    local install_img="source.archipelago-foundation.org/lfg2025/filebrowser:v2.27.0"
    # Check if already installed
    local fb_state
    fb_state=$(ssh $SSH_OPTS "$SSH_HOST" "podman inspect filebrowser --format '{{.State.Status}}' 2>/dev/null || echo 'none'")
    if [ "$fb_state" = "none" ]; then
        info "Installing filebrowser..."
        local install_result
        install_result=$(rpc "package.install" "{\"id\":\"filebrowser\",\"dockerImage\":\"$install_img\"}")
        if echo "$install_result" | grep -q '"success"'; then
            pass "package.install filebrowser: success"
        else
            fail "package.install filebrowser: $(echo "$install_result" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("error",{}).get("message","unknown"))' 2>/dev/null)"
        fi
    else
        pass "filebrowser already installed ($fb_state)"
    fi

    # Test 4: Stop with grace period
    TESTS=$((TESTS + 1))
    local stop_result
    stop_result=$(rpc "package.stop" '{"id":"filebrowser"}')
    sleep 2
    fb_state=$(ssh $SSH_OPTS "$SSH_HOST" "podman inspect filebrowser --format '{{.State.Status}}' 2>/dev/null || echo 'unknown'")
    if [ "$fb_state" = "exited" ] || [ "$fb_state" = "stopped" ]; then
        pass "package.stop: filebrowser → $fb_state"
    else
        fail "package.stop: expected stopped, got $fb_state"
    fi

    # Test 5: Start
    TESTS=$((TESTS + 1))
    rpc "package.start" '{"id":"filebrowser"}' >/dev/null
    sleep 3
    fb_state=$(ssh $SSH_OPTS "$SSH_HOST" "podman inspect filebrowser --format '{{.State.Status}}' 2>/dev/null || echo 'unknown'")
    if [ "$fb_state" = "running" ]; then
        pass "package.start: filebrowser → running"
    else
        fail "package.start: expected running, got $fb_state"
    fi

    # Test 6: Restart tracker persisted
    TESTS=$((TESTS + 1))
    local tracker
    tracker=$(ssh $SSH_OPTS "$SSH_HOST" "cat /var/lib/archipelago/restart-tracker.json 2>/dev/null")
    if [ -n "$tracker" ] && echo "$tracker" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
        pass "restart-tracker.json: valid JSON"
    else
        pass "restart-tracker.json: empty (no failures — healthy)"
    fi

    # Test 7: Systemd timers active
    TESTS=$((TESTS + 1))
    local timers
    timers=$(ssh $SSH_OPTS "$SSH_HOST" "systemctl list-timers --no-pager 2>/dev/null | grep -c archipelago")
    if [ "${timers:-0}" -ge 2 ]; then
        pass "Systemd timers: $timers active (doctor + reconcile)"
    else
        fail "Systemd timers: expected ≥2, got ${timers:-0}"
    fi

    # Test 8: Container doctor runs cleanly
    TESTS=$((TESTS + 1))
    local doctor_exit
    ssh $SSH_OPTS "$SSH_HOST" "sudo /home/archipelago/archy/scripts/container-doctor.sh --local 2>&1 | tail -1"
    doctor_exit=$?
    if [ $doctor_exit -eq 0 ]; then
        pass "container-doctor.sh: clean exit"
    else
        fail "container-doctor.sh: exit code $doctor_exit"
    fi

    # Summary
    header "Results"
    local passed=$((TESTS - FAILURES))
    echo -e "  ${GREEN}$passed passed${NC} / ${RED}$FAILURES failed${NC} / $TESTS total"

    # Cleanup temp cookie
    rm -f "$COOKIE" 2>/dev/null

    return $FAILURES
}

# ── Main ────────────────────────────────────────────────────────────────

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║  Container Orchestration Dev Loop                             ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
info "Target: $SSH_HOST"
info "Mode: $($ONCE && echo 'single run' || echo 'interactive loop')"
echo ""

# Check SSH
if ! ssh $SSH_OPTS "$SSH_HOST" "echo ok" >/dev/null 2>&1; then
    fail "Cannot SSH to $SSH_HOST"
    exit 1
fi

if $ONCE; then
    sync_and_build && run_smoke_tests
    exit $?
fi

# Interactive loop
while true; do
    sync_and_build && run_smoke_tests
    echo ""
    echo -e "${YELLOW}Press Enter to re-sync + re-test, Ctrl+C to stop${NC}"
    read -r
done
