#!/usr/bin/env bash
#
# Run Archipelago tests.
#
# By default this runs frontend tests and local backend Rust tests. Set
# ARCHIPELAGO_SSH_HOST and ARCHIPELAGO_SSH_KEY to run backend tests on a Linux
# target instead.
#
set -euo pipefail

SSH_KEY="${ARCHIPELAGO_SSH_KEY:-}"
SSH_HOST="${ARCHIPELAGO_SSH_HOST:-}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

FRONTEND_OK=0
BACKEND_OK=0

echo "========================================="
echo "  Archipelago Test Runner"
echo "========================================="
echo ""

# --- Frontend Tests ---
echo "--- Frontend Tests (local) ---"
if (cd "$PROJECT_DIR/neode-ui" && npm test 2>&1); then
    echo "✅ Frontend tests PASSED"
    FRONTEND_OK=1
else
    echo "❌ Frontend tests FAILED"
fi

echo ""

# --- Backend Tests ---
if [[ -n "$SSH_HOST" && -n "$SSH_KEY" ]]; then
    echo "--- Backend Tests (Linux target: $SSH_HOST) ---"
    echo "Syncing source to target..."
    rsync -az --exclude 'target' --exclude 'node_modules' --exclude '.git' \
        -e "ssh -i $SSH_KEY" \
        "$PROJECT_DIR/core/" "$SSH_HOST:~/archy/core/" 2>&1

    if ssh -i "$SSH_KEY" "$SSH_HOST" \
        "source ~/.cargo/env && cd ~/archy/core && cargo test --all-features 2>&1"; then
        echo "✅ Backend tests PASSED"
        BACKEND_OK=1
    else
        echo "❌ Backend tests FAILED"
    fi
else
    echo "--- Backend Tests (local) ---"
    if (cd "$PROJECT_DIR/core" && cargo test --all-features 2>&1); then
        echo "✅ Backend tests PASSED"
        BACKEND_OK=1
    else
        echo "❌ Backend tests FAILED"
    fi
fi

echo ""
echo "========================================="
echo "  Results"
echo "========================================="

if [ "$FRONTEND_OK" -eq 1 ]; then
    echo "  Frontend: ✅ PASS"
else
    echo "  Frontend: ❌ FAIL"
fi

if [ "$BACKEND_OK" -eq 1 ]; then
    echo "  Backend:  ✅ PASS"
else
    echo "  Backend:  ❌ FAIL"
fi

echo "========================================="

if [ "$FRONTEND_OK" -eq 1 ] && [ "$BACKEND_OK" -eq 1 ]; then
    echo "All tests passed!"
    exit 0
else
    echo "Some tests failed."
    exit 1
fi
