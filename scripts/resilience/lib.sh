#!/bin/bash
# Resilience harness shared helpers.
# Sourced by resilience.sh — do not invoke directly.

# Required env (set by resilience.sh before sourcing):
#   TARGET     — ssh target, e.g. archipelago@192.0.2.10
#   RPC_URL    — http://<host>:5678/rpc/v1
#   COOKIE_JAR — path for curl cookie store
#   SSH_PASS   — sshpass password
#   UI_PASS    — archipelago UI password
#   OUT_DIR    — report output dir

# ── ssh ─────────────────────────────────────────────────────────
ssh_run() {
    # -n: redirect stdin from /dev/null so ssh doesn't gobble up our parent's
    #     stdin. Without this, ssh inside a `while read … done <<< "$LIST"`
    #     consumes the heredoc on the first call, ending the loop after one
    #     iteration. Cost us a smoke run that only tested filebrowser instead
    #     of all three smoke apps.
    sshpass -p "$SSH_PASS" ssh -n -o StrictHostKeyChecking=accept-new \
        -o ConnectTimeout=10 -o LogLevel=ERROR "$TARGET" "$@"
}

# Run a command and tolerate ssh failure (host rebooting, etc.).
ssh_try() {
    sshpass -p "$SSH_PASS" ssh -n -o StrictHostKeyChecking=accept-new \
        -o ConnectTimeout=5 -o LogLevel=ERROR "$TARGET" "$@" 2>/dev/null || echo "__SSH_FAIL__"
}

ssh_wait_ready() {
    local deadline=$(($(date +%s) + ${1:-180}))
    while [ "$(date +%s)" -lt "$deadline" ]; do
        if [ "$(ssh_try 'echo OK')" = "OK" ]; then return 0; fi
        sleep 3
    done
    return 1
}

# ── rpc ─────────────────────────────────────────────────────────
rpc_login() {
    local resp
    resp=$(curl -ksS -c "$COOKIE_JAR" -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"auth.login\",\"params\":{\"password\":\"$UI_PASS\"},\"id\":1}" \
        "$RPC_URL")
    if echo "$resp" | jq -e '.error' >/dev/null 2>&1; then
        echo "ERROR: login failed: $(echo "$resp" | jq -c .)" >&2
        return 1
    fi
    CSRF_TOKEN=$(awk '/csrf_token/ {print $7}' "$COOKIE_JAR" | head -1)
    [ -n "$CSRF_TOKEN" ] || { echo "ERROR: no CSRF token after login" >&2; return 1; }
    export CSRF_TOKEN
}

# Make an RPC call. Args: method, json_params, timeout_secs (optional, default 90).
# Prints raw JSON response. Caller asserts success via jq.
#
# CSRF rotates per-response: the server may issue a new csrf_token on every
# state-changing call, so we re-read it from the cookie jar before each call
# rather than caching the value from login. Also retries once on nginx-served
# BACKEND_UNAVAILABLE (5xx fallback) for transient stalls.
rpc_call() {
    local method="$1"
    # NOTE: don't use ${2:-{}} — bash matches the first unescaped `}` as the
    # end of the expansion, so the trailing `}` becomes a literal char and
    # corrupts every params value into invalid JSON. Use an if-check instead.
    local params="${2-}"
    [ -z "$params" ] && params='{}'
    local timeout="${3:-90}"
    local attempt
    for attempt in 1 2 3 4; do
        local csrf
        csrf=$(awk '/^[^#]/ && /csrf_token/ {print $7; exit}' "$COOKIE_JAR")
        local resp
        resp=$(curl -ksS -b "$COOKIE_JAR" -c "$COOKIE_JAR" \
            -H "Content-Type: application/json" \
            -H "X-CSRF-Token: $csrf" \
            -d "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}" \
            --max-time "$timeout" \
            "$RPC_URL")
        # Retry on transient errors:
        #   BACKEND_UNAVAILABLE — nginx 5xx fallback (archipelago briefly stalled)
        #   429 — nginx rate limiter exceeded (burst=40 in /etc/nginx/sites-enabled/*)
        if echo "$resp" | jq -e '.error.code == "BACKEND_UNAVAILABLE" or .error.code == 429' >/dev/null 2>&1; then
            [ "$attempt" -eq 4 ] && { echo "$resp"; return; }
            # Exponential-ish backoff: 5s, 15s, 30s. Plenty of time for the
            # nginx rate window (1s) and any archipelago restart to clear.
            sleep $((attempt * 10))
            continue
        fi
        echo "$resp"
        return
    done
}

# After a service restart the session may need re-establishing.
rpc_relogin_if_needed() {
    local probe
    probe=$(rpc_call "package.list" '{}' 2>/dev/null)
    if echo "$probe" | jq -e '.error.code == -32001' >/dev/null 2>&1; then
        rpc_login || return 1
    fi
}

# ── per-app metadata ────────────────────────────────────────────
# Mappings the harness needs that aren't expressible from catalog.json alone:
# multi-container stack rosters, alias/variant container names (bitcoin-knots
# vs bitcoin-core install the same slots), and the actual nginx UI proxy path
# (which often differs from /app/<id>/, e.g. `bitcoin-knots` → `/app/bitcoin-ui/`).
#
# Keep these tables in sync with the install code in package/stacks.rs and
# the `*_IMAGE` companion handling in install.rs (the `archy-<x>-ui` set).

# Containers an app installs. Used for app_already_installed detection AND
# for state assertions when the snapshot-diff falls back (variant apps don't
# create new containers when their alternate is already present).
expected_containers_for() {
    case "$1" in
        bitcoin-knots) echo "bitcoin-knots archy-bitcoin-ui" ;;
        bitcoin-core)  echo "bitcoin-core archy-bitcoin-ui" ;;
        lnd)           echo "lnd archy-lnd-ui" ;;
        electrumx|electrs|mempool-electrs)
                       echo "electrs archy-electrs-ui" ;;
        btcpay-server) echo "archy-btcpay-server archy-btcpay-db archy-nbxplorer archy-btcpay-ui" ;;
        mempool)       echo "mempool archy-mempool-web archy-mempool-db" ;;
        immich)        echo "immich_server immich_machine_learning immich_postgres immich_redis" ;;
        penpot|penpot-frontend)
                       echo "penpot-frontend penpot-backend penpot-exporter penpot-postgres penpot-redis" ;;
        indeedhub)     echo "indeedhub indeedhub-api indeedhub-ffmpeg indeedhub-postgres indeedhub-redis indeedhub-minio indeedhub-relay" ;;
        *)             echo "$1" ;;
    esac
}

# UI proxy URL path on the HTTPS frontend. Most apps live at /app/<id>/ but
# Bitcoin/LND/Electrs proxy through their UI companion containers, and BTCPay
# uses its own short path.
ui_proxy_path_for() {
    case "$1" in
        bitcoin-knots|bitcoin-core)  echo "/app/bitcoin-ui/" ;;
        electrumx|electrs)           echo "/app/electrumx/" ;;
        lnd)                         echo "/app/lnd-ui/" ;;
        btcpay-server)               echo "/app/btcpay/" ;;
        *)                           echo "/app/$1/" ;;
    esac
}

# Authenticated probe for credentialed UIs. Echoes the HTTP status code if
# defined, otherwise returns 1 (caller records SKIP). PASS = code in
# {200,401,403} for endpoints that prove the proxy reaches the backend
# (401/403 from app's own auth ≠ 502 from broken proxy).
auth_probe_for() {
    local app="$1"
    local host; host="$(echo "$TARGET" | cut -d@ -f2)"
    case "$app" in
        bitcoin-knots|bitcoin-core)
            # Direct bitcoin-rpc proxy on :8334 inside .228 — credential
            # plumbing is the .228 bug we just shipped, must return 200.
            ssh_run 'curl -s -o /dev/null -w "%{http_code}" --max-time 5 -X POST http://127.0.0.1:8334/bitcoin-rpc/ -H "Content-Type: application/json" -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getblockchaininfo\",\"params\":[]}"'
            ;;
        btcpay-server)
            # BTCPay's own auth returns 401 for unauthenticated API calls;
            # 502 means proxy broken / backend down.
            curl -ks -o /dev/null -w "%{http_code}" --max-time 5 \
                "https://$host/app/btcpay/api/v1/server/info"
            ;;
        lnd)
            # LND has a /lnd-connect-info passthrough on archipelago itself —
            # returns lndconnect URI when LND is up. 200 = backend reachable.
            curl -ks -o /dev/null -w "%{http_code}" --max-time 5 \
                "https://$host/lnd-connect-info"
            ;;
        electrumx|electrs)
            # ElectrumX is plain TCP (electrum protocol) — no HTTPS auth path.
            # archipelago exposes /electrs-status which queries the daemon.
            curl -ks -o /dev/null -w "%{http_code}" --max-time 5 \
                "https://$host/electrs-status"
            ;;
        *)
            return 1
            ;;
    esac
}

# Whether an auth_probe HTTP code counts as a pass.
auth_probe_pass_codes() {
    case "$1" in
        bitcoin-knots|bitcoin-core) echo "200" ;;
        btcpay-server)              echo "200 401 403" ;;
        lnd|electrumx|electrs)      echo "200" ;;
        *)                          echo "200" ;;
    esac
}

# ── probes (state assertions) ───────────────────────────────────
# Returns container Status string ("running","exited","absent",…).
probe_container_state() {
    local name="$1"
    ssh_run "podman inspect '$name' --format '{{.State.Status}}' 2>/dev/null || echo absent"
}

# Returns RestartCount as integer.
probe_container_restart_count() {
    local name="$1"
    ssh_run "podman inspect '$name' --format '{{.RestartCount}}' 2>/dev/null || echo -1"
}

# Probe the app's UI proxy on the HTTPS frontend. Returns HTTP code.
# Uses ui_proxy_path_for so apps with non-default proxy paths (bitcoin-ui,
# lnd-ui, electrs-ui, btcpay) get probed at the right URL.
probe_app_proxy() {
    local app_id="$1"
    local host
    host="$(echo "$TARGET" | cut -d@ -f2)"
    local path
    path=$(ui_proxy_path_for "$app_id")
    curl -ks -o /dev/null -w "%{http_code}" --max-time 5 "https://$host$path" || echo "000"
}

# Check that ZERO containers are leftover for this app — catches uninstall residue.
probe_no_residue() {
    local prefix="$1"
    ssh_run "podman ps -a --format '{{.Names}}' | grep -E '^${prefix}(-|$)' | wc -l"
}

# ── waiters ─────────────────────────────────────────────────────
# Wait for the package's state in the RPC list to match expected, with timeout.
wait_for_package_state() {
    local pkg="$1"; local want="$2"; local timeout="${3:-300}"
    local deadline=$(($(date +%s) + timeout))
    while [ "$(date +%s)" -lt "$deadline" ]; do
        local got
        got=$(rpc_call "package.list" '{}' \
              | jq -r ".result.package_data[\"$pkg\"].state // \"absent\"")
        case "$want" in
            Running) [ "$got" = "Running" ] && return 0 ;;
            Stopped) [ "$got" = "Stopped" ] && return 0 ;;
            absent)  [ "$got" = "absent" ] && return 0 ;;
        esac
        sleep 4
    done
    echo "TIMEOUT waiting for $pkg → $want (last seen: $got)" >&2
    return 1
}

# Wait for podman state of a specific container.
wait_for_container_state() {
    local name="$1"; local want="$2"; local timeout="${3:-180}"
    local deadline=$(($(date +%s) + timeout))
    while [ "$(date +%s)" -lt "$deadline" ]; do
        local got
        got=$(probe_container_state "$name")
        [ "$got" = "$want" ] && return 0
        sleep 3
    done
    echo "TIMEOUT waiting for container $name → $want (last seen: $got)" >&2
    return 1
}

# Wait until restart count is stable for `stable_secs` seconds — proxy for "no crashloop".
wait_restart_count_stable() {
    local name="$1"; local stable_secs="${2:-30}"; local timeout="${3:-180}"
    local deadline=$(($(date +%s) + timeout))
    local last; local last_change_ts
    last=$(probe_container_restart_count "$name")
    last_change_ts=$(date +%s)
    while [ "$(date +%s)" -lt "$deadline" ]; do
        sleep 5
        local now
        now=$(probe_container_restart_count "$name")
        if [ "$now" != "$last" ]; then
            last="$now"
            last_change_ts=$(date +%s)
        elif [ $(( $(date +%s) - last_change_ts )) -ge "$stable_secs" ]; then
            return 0
        fi
    done
    echo "TIMEOUT waiting for $name restart-count stable (last=$last)" >&2
    return 1
}

# ── result recording ────────────────────────────────────────────
# Append a result row to the JSON-lines report.
# Args: app_id, transition, status (PASS/FAIL/SKIP), detail
record() {
    local app="$1"; local transition="$2"; local status="$3"; local detail="${4:-}"
    local ts
    ts=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    jq -nc --arg ts "$ts" --arg app "$app" --arg t "$transition" --arg s "$status" --arg d "$detail" \
        '{ts:$ts, app:$app, transition:$t, status:$s, detail:$d}' >> "$OUT_DIR/results.jsonl"
    local marker
    case "$status" in
        PASS) marker="✅" ;;
        FAIL) marker="❌" ;;
        SKIP) marker="⏭" ;;
        *)    marker="•"  ;;
    esac
    printf '%s [%-15s] %-30s %s%s\n' "$marker" "$app" "$transition" "$status" "${detail:+ — $detail}"
}
