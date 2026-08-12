#!/bin/bash
# Mesh / Reticulum test suite — the "is the mesh stack healthy" gate.
#
# Three layers, cheapest first:
#   1. Rust unit tests        (no hardware, ~2s once built)
#   2. Daemon selftest        (full RNS+LXMF bring-up, no radio; also verifies
#                              the announce app_data wire contract + set_name)
#   3. Live-node assertions   (optional; needs a running archipelago with a
#                              radio — set MESH_TEST_LIVE=1 MESH_TEST_PW=...)
#
# Usage:
#   tests/mesh/run-mesh-tests.sh                # layers 1+2
#   MESH_TEST_LIVE=1 MESH_TEST_PW='...' tests/mesh/run-mesh-tests.sh
#   MESH_TEST_HOST=100.64.0.55 ...           # live-test a remote node
set -u
cd "$(dirname "$0")/../.."
FAIL=0
ok()   { echo "ok  - $1"; }
bad()  { echo "not ok - $1"; FAIL=1; }

# ── 1. Rust unit tests ────────────────────────────────────────────────
RUST_RESULTS=$(cd core && cargo test -p archipelago --bin archipelago mesh 2>&1 | grep "^test result:")
if [ -n "$RUST_RESULTS" ] && ! echo "$RUST_RESULTS" | grep -vq " 0 failed"; then
    ok "rust mesh unit tests ($(echo "$RUST_RESULTS" | grep -o '[0-9]* passed' | head -1))"
else
    bad "rust mesh unit tests"
fi

# ── 2. Reticulum daemon selftest (no radio) ───────────────────────────
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
head -c 32 /dev/urandom > "$TMP/key"
DAEMON=reticulum-daemon/.venv/bin/python
if [ -x "$DAEMON" ]; then
    if "$DAEMON" reticulum-daemon/reticulum_daemon.py \
        --identity-key "$TMP/key" --rns-config "$TMP/rns" \
        --socket "$TMP/sock" --display-name "SelftestNode" --selftest 2>/dev/null \
        | grep -q "announce_app_data=verified set_name=verified"; then
        ok "daemon selftest (announce wire contract + set_name)"
    else
        bad "daemon selftest"
    fi
else
    echo "skip - daemon selftest (no venv at $DAEMON)"
fi

# ── 3. Live node assertions (opt-in) ──────────────────────────────────
if [ "${MESH_TEST_LIVE:-0}" = "1" ]; then
    HOST="${MESH_TEST_HOST:-127.0.0.1}"
    PW="${MESH_TEST_PW:?set MESH_TEST_PW}"
    # Nodes differ: dev boxes serve plain http on :80, ISO installs https.
    RPC=""
    for base in "http://$HOST" "https://$HOST" "http://$HOST:5678"; do
        code=$(curl -ksS -o /dev/null -w '%{http_code}' -m 5 -X POST "$base/rpc/v1" 2>/dev/null || true)
        case "$code" in 000|"") continue ;; *) RPC="$base/rpc/v1"; break ;; esac
    done
    [ -n "$RPC" ] || { bad "live: no RPC endpoint reachable on $HOST"; echo FAIL; exit 1; }
    JAR="$TMP/jar"
    curl -ksS -c "$JAR" -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"auth.login\",\"params\":{\"password\":\"$PW\"},\"id\":1}" \
        "$RPC" > "$TMP/login"
    if grep -q '"error":null' "$TMP/login"; then ok "live: rpc login"; else bad "live: rpc login"; fi
    call() {
        local csrf; csrf=$(awk '/^[^#]/ && /csrf_token/ {print $7; exit}' "$JAR")
        curl -ksS -b "$JAR" -c "$JAR" -H "Content-Type: application/json" \
            -H "X-CSRF-Token: $csrf" \
            -d "{\"jsonrpc\":\"2.0\",\"method\":\"$1\",\"params\":${2:-{\}},\"id\":2}" \
            --max-time 60 "$RPC"
    }
    ST=$(call mesh.status)
    echo "$ST" | grep -q '"device_connected":true' \
        && ok "live: radio connected ($(echo "$ST" | grep -o '"device_type":"[a-z]*"'))" \
        || bad "live: radio connected"
    echo "$ST" | grep -q '"self_advert_name":"[^"]' \
        && ok "live: node has a mesh name" || bad "live: node has a mesh name"
    call mesh.refresh | grep -q '"refreshed":true' \
        && ok "live: mesh.refresh" || bad "live: mesh.refresh"
    call mesh.broadcast | grep -q '"broadcast":true' \
        && ok "live: mesh.broadcast" || bad "live: mesh.broadcast"
    # No peer may ever display a raw identity blob as its name.
    call mesh.peers | grep -q '"advert_name":"ARCHY:' \
        && bad "live: no ARCHY-blob peer names" || ok "live: no ARCHY-blob peer names"
fi

[ "$FAIL" = 0 ] && echo "PASS" || { echo "FAIL"; exit 1; }
