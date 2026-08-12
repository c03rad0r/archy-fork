#!/bin/bash
#
# Container Doctor — diagnose and fix common container health issues
#
# Usage:
#   sudo ./scripts/container-doctor.sh              # Run locally on node
#   ./scripts/container-doctor.sh user@host          # Run remotely via SSH
#
# Fixes:
#   1. Stale podman ps/stats processes (>10 = pileup)
#   2. Orphaned conmon/crun processes holding ports
#   3. System tor conflicting with container tor
#   4. Tor hidden service directory permissions (group/other must have no
#      access; Tor's own setgid 2700 is fine, restart is backed off)
#   5. SearXNG read-only root / cap-drop ALL
#   6. Bitcoin Knots prune+txindex conflict
#   7. Containers stuck with exit code 127 (binary not found)
#   8. Stopped core containers (rootless restart policy workaround)
#   9. Missing rootless port listeners while Podman still shows published ports
#  10. Nginx Proxy Manager public hosts not mirrored into host nginx
#  11. BTCPay stores producing unpayable Lightning invoices (route hints off)
#  12. Missing catatonit (Podman init binary) — init-enabled deploys fail
#
# Safe to run multiple times (idempotent). Never blocks deploy (exit 0 always).
#

set -o pipefail

# Source pinned image versions (single source of truth)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
[ -f "$SCRIPT_DIR/image-versions.sh" ] && . "$SCRIPT_DIR/image-versions.sh"

FIXES_APPLIED=0
CHECKS_PASSED=0
FIX_NAMES=()

log() { echo "[$(date +%H:%M:%S)] DOCTOR: $*"; }

podman_rootless() {
    if [ "$(id -u)" = "0" ] && id archipelago >/dev/null 2>&1; then
        local archi_uid
        archi_uid=$(id -u archipelago)
        sudo -u archipelago env XDG_RUNTIME_DIR="/run/user/$archi_uid" podman "$@"
    else
        podman "$@"
    fi
}

port_is_listening() {
    local port="$1"
    ss -ltn 2>/dev/null | awk '{print $4}' | grep -Eq "(^|:)$port$"
}

run_fix() {
    local name="$1"
    shift
    if "$@"; then
        FIXES_APPLIED=$((FIXES_APPLIED + 1))
        FIX_NAMES+=("$name")
    else
        CHECKS_PASSED=$((CHECKS_PASSED + 1))
    fi
}

# ── Fix 1: Stale podman processes ────────────────────────────
fix_stale_podman() {
    local count
    count=$(pgrep -f "podman (ps|stats)" 2>/dev/null | wc -l)
    count=${count:-0}
    if [ "$count" -gt 10 ]; then
        log "Killing $count stale podman ps/stats processes"
        pkill -f "podman (ps|stats)" 2>/dev/null || true
        sleep 2
        local after
        after=$(pgrep -f "podman (ps|stats)" 2>/dev/null | wc -l)
        after=${after:-0}
        log "Reduced from $count to $after"
        return 0
    fi
    return 1
}

# ── Fix 2: Orphaned conmon holding ports ─────────────────────
fix_orphaned_conmon() {
    local fixed=false
    # Find conmon processes whose containers no longer exist
    local pids
    pids=$(pgrep -f "conmon.*--exit-command" 2>/dev/null || true)
    if [ -z "$pids" ]; then
        return 1
    fi

    # Doctor runs as root but containers are rootless under archipelago user.
    # Must check container existence using the rootless podman database.
    local PODMANCMD="sudo -u archipelago XDG_RUNTIME_DIR=/run/user/1000 podman"

    for pid in $pids; do
        # Extract container ID from conmon args
        local cid
        cid=$(tr '\0' ' ' < /proc/"$pid"/cmdline 2>/dev/null | grep -oP '(?<=-c )[a-f0-9]{64}' || true)
        if [ -z "$cid" ]; then
            continue
        fi
        # Check if container still exists in rootless podman
        if ! $PODMANCMD inspect "$cid" &>/dev/null; then
            local port_info
            port_info=$(ss -tlnp 2>/dev/null | grep "pid=$pid" | grep -oP ':\K\d+' | head -3 | tr '\n' ',' | sed 's/,$//')
            log "Killing orphaned conmon pid=$pid (ports: ${port_info:-none})"
            kill "$pid" 2>/dev/null || kill -9 "$pid" 2>/dev/null || true
            fixed=true
        fi
    done

    $fixed && return 0 || return 1
}

# ── Fix 3: Ensure system Tor is running (preferred over container) ──
fix_system_tor_conflict() {
    # System Tor is preferred over container Tor.
    # If archy-tor container exists, remove it and use system Tor instead.
    if podman ps -a --format '{{.Names}}' 2>/dev/null | grep -qE '^archy-tor$'; then
        podman stop archy-tor 2>/dev/null || true
        podman rm -f archy-tor 2>/dev/null || true
        log "Removed archy-tor container (system Tor is preferred)"
    fi

    # Ensure system Tor is enabled and running
    if command -v tor >/dev/null 2>&1; then
        if ! systemctl is-active tor@default >/dev/null 2>&1; then
            systemctl enable tor tor@default 2>/dev/null || true
            systemctl start tor tor@default 2>/dev/null || true
            log "Started system Tor"
            return 0
        fi
    fi

    return 1
}

# ── Fix 4: Tor hidden service permissions ────────────────────
# `stat -c '%a'` omits leading zeros, so a directory Tor manages itself
# reads "2700" (setgid bit set) rather than "700" -- but the setgid bit
# doesn't grant group/other any access. Comparing the full string against
# the literal "700" therefore flagged Tor's own correct setting as broken
# on every run, chmod'd it right back to what it already effectively was,
# and restarted Tor -- forever, every ~5 minutes (the doctor timer
# interval), so Tor never kept its consensus/HSDir cache long enough to
# resolve any .onion peer. Only the last 3 digits of the mode are the
# owner/group/other permission bits; anything before that is
# setuid/setgid/sticky, which is irrelevant to "is group/other access
# denied". Compare only those 3 digits so 700 and 2700 both pass, while a
# genuinely permissive mode (750, 707, 2755, ...) is still corrected.
#
# Restart backoff: even a real, legitimate fix must not restart Tor more
# than once per TOR_RESTART_BACKOFF_SECONDS, so no future defect in this
# (or any other) predicate can reproduce a restart storm. The timestamp
# file persists across doctor runs (and doctor is re-invoked from a fresh
# shell every run, so this can't just be a shell variable).
TOR_RESTART_BACKOFF_SECONDS=1800   # 30 minutes
TOR_RESTART_STATE_FILE="/var/lib/archipelago/doctor-tor-last-restart"
fix_tor_permissions() {
    local fixed=false
    local tor_dirs=("/var/lib/archipelago/tor" "/var/lib/tor")

    for base in "${tor_dirs[@]}"; do
        if [ ! -d "$base" ]; then
            continue
        fi
        while IFS= read -r dir; do
            local perms
            perms=$(stat -c '%a' "$dir" 2>/dev/null)
            local mode_bits="${perms: -3}"
            if [ "$mode_bits" != "700" ]; then
                chmod 700 "$dir"
                log "Fixed permissions on $dir ($perms -> 700; group/other access was NOT fully denied)"
                fixed=true
            elif [ -n "$DOCTOR_DEBUG" ]; then
                log "DEBUG: $dir already denies group/other access (mode $perms) — no change"
            fi
        done < <(find "$base" -maxdepth 1 -name "hidden_service_*" -type d 2>/dev/null)
    done

    # If we fixed a real permission defect, restart system Tor to pick up
    # the change -- but never more than once per backoff window.
    if $fixed; then
        local now last_restart elapsed
        now=$(date +%s)
        last_restart=$(cat "$TOR_RESTART_STATE_FILE" 2>/dev/null || echo 0)
        case "$last_restart" in *[!0-9]*|"") last_restart=0 ;; esac
        elapsed=$((now - last_restart))
        if [ "$elapsed" -ge "$TOR_RESTART_BACKOFF_SECONDS" ]; then
            systemctl restart tor@default 2>/dev/null || true
            mkdir -p "$(dirname "$TOR_RESTART_STATE_FILE")" 2>/dev/null
            echo "$now" > "$TOR_RESTART_STATE_FILE" 2>/dev/null
            log "Restarted Tor to apply hidden-service permission fix"
        else
            log "Skipped Tor restart: last restart was ${elapsed}s ago (backoff window ${TOR_RESTART_BACKOFF_SECONDS}s) — permissions were still corrected"
        fi
        return 0
    fi
    return 1
}

# ── Fix 5: SearXNG read-only / cap-drop ─────────────────────
fix_searxng() {
    if ! podman ps -a --format '{{.Names}}' 2>/dev/null | grep -q '^searxng$'; then
        return 1
    fi

    local state
    state=$(podman inspect searxng --format '{{.State.Status}}' 2>/dev/null || true)
    local readonly_root
    readonly_root=$(podman inspect searxng --format '{{.HostConfig.ReadonlyRootfs}}' 2>/dev/null || true)
    local cap_drop
    cap_drop=$(podman inspect searxng --format '{{.HostConfig.CapDrop}}' 2>/dev/null || true)

    # Fix if: exited, or has read-only root, or has cap-drop ALL
    local needs_fix=false
    if [ "$state" = "exited" ]; then
        needs_fix=true
    fi
    if [ "$readonly_root" = "true" ]; then
        needs_fix=true
    fi
    if [[ "$cap_drop" == *"ALL"* ]] || [[ "$cap_drop" == *"all"* ]]; then
        needs_fix=true
    fi

    if ! $needs_fix; then
        return 1
    fi

    log "Recreating SearXNG (readonly=$readonly_root, cap_drop=$cap_drop, state=$state)"

    # Get current port mapping
    local port
    port=$(podman inspect searxng --format '{{range $k,$v := .HostConfig.PortBindings}}{{$k}}={{range $v}}{{.HostPort}}{{end}}{{println}}{{end}}' 2>/dev/null | head -1)
    local host_port="${port##*=}"
    host_port="${host_port:-8888}"

    # Kill any stale conmon holding the port
    local conmon_pid
    conmon_pid=$(ss -tlnp 2>/dev/null | grep ":${host_port} " | grep -oP 'pid=\K\d+' | head -1)

    podman stop searxng 2>/dev/null || true
    podman rm -f searxng 2>/dev/null || true

    if [ -n "$conmon_pid" ]; then
        kill -9 "$conmon_pid" 2>/dev/null || true
        sleep 2
    fi

    podman run -d \
        --name searxng \
        --restart=unless-stopped \
        --security-opt=no-new-privileges:true \
        --tmpfs /tmp:rw,noexec,nosuid,size=256m \
        -v searxng-config:/etc/searxng:rw \
        -v searxng-cache:/var/cache/searxng:rw \
        -p "${host_port}:8080" \
        --memory=512m \
        "${SEARXNG_IMAGE}" 2>&1 || true

    log "SearXNG recreated (no readonly, no cap-drop ALL)"
    return 0
}

# ── Fix 6: Bitcoin Knots prune+txindex conflict ──────────────
fix_bitcoin_txindex() {
    if ! podman ps -a --format '{{.Names}}' 2>/dev/null | grep -q '^bitcoin-knots$'; then
        return 1
    fi

    # Check if bitcoin.conf has prune enabled
    local conf="/var/lib/archipelago/bitcoin/bitcoin.conf"
    if [ ! -f "$conf" ] || ! grep -q '^prune=' "$conf"; then
        return 1
    fi

    # Check if container args include txindex
    local cmd
    cmd=$(podman inspect bitcoin-knots --format '{{json .Config.Cmd}}' 2>/dev/null || true)
    if ! echo "$cmd" | grep -q "txindex"; then
        return 1
    fi

    log "Bitcoin Knots: prune+txindex conflict detected"

    # Get current config
    local image
    image=$(podman inspect bitcoin-knots --format '{{.ImageName}}' 2>/dev/null)
    local network
    network=$(podman inspect bitcoin-knots --format '{{.HostConfig.NetworkMode}}' 2>/dev/null)

    # Read per-installation RPC password
    local SECRETS_DIR="/var/lib/archipelago/secrets"
    local BTC_RPC_PASS="archipelago"
    if [ -f "$SECRETS_DIR/bitcoin-rpc-password" ]; then
        BTC_RPC_PASS=$(cat "$SECRETS_DIR/bitcoin-rpc-password")
    fi

    # Ensure bitcoin.conf has all RPC settings
    if ! grep -q 'rpcuser=' "$conf"; then
        cat > "$conf" <<BCONF
server=1
prune=550
rpcuser=archipelago
rpcpassword=$BTC_RPC_PASS
rpcallowip=127.0.0.1/32
rpcallowip=10.88.0.0/16
listen=1
printtoconsole=0
BCONF
        log "Updated bitcoin.conf with full RPC settings"
    fi

    # Remove stale txindex if present
    if [ -d "/var/lib/archipelago/bitcoin/indexes/txindex" ]; then
        find /var/lib/archipelago/bitcoin/indexes/txindex -type f -delete 2>/dev/null
        rmdir /var/lib/archipelago/bitcoin/indexes/txindex 2>/dev/null || true
        log "Removed stale txindex directory"
    fi

    # Recreate without txindex
    podman stop bitcoin-knots 2>/dev/null || true
    podman rm -f bitcoin-knots 2>/dev/null || true
    sleep 2

    # Kill stale conmon on port 8332/8333
    for p in 8332 8333; do
        local cpid
        cpid=$(ss -tlnp 2>/dev/null | grep ":${p} " | grep -oP 'pid=\K\d+' | head -1)
        if [ -n "$cpid" ]; then
            kill -9 "$cpid" 2>/dev/null || true
        fi
    done
    sleep 1

    local net_arg=""
    if [ -n "$network" ] && [ "$network" != "bridge" ] && [ "$network" != "host" ]; then
        net_arg="--network=$network"
    elif [ "$network" = "host" ]; then
        net_arg="--network=host"
    else
        net_arg="--network=archy-net"
    fi

    podman run -d \
        --name bitcoin-knots \
        --restart=always \
        $net_arg \
        -p 8332:8332 \
        -p 8333:8333 \
        -v /var/lib/archipelago/bitcoin:/home/bitcoin/.bitcoin \
        --memory=2g \
        --cap-drop=ALL \
        --cap-add=CHOWN \
        --cap-add=FOWNER \
        --cap-add=SETUID \
        --cap-add=SETGID \
        --cap-add=DAC_OVERRIDE \
        --security-opt=no-new-privileges:true \
        --health-cmd="bitcoin-cli -rpcuser=archipelago -rpcpassword=$BTC_RPC_PASS getblockchaininfo || exit 1" \
        --health-interval=30s \
        --health-retries=3 \
        "$image" 2>&1 || true

    log "Bitcoin Knots recreated without txindex (prune mode)"
    return 0
}

# ── Fix 7: Exit code 127 containers ─────────────────────────
fix_exit_127() {
    local containers
    containers=$(podman ps -a --format '{{.Names}} {{.Status}}' 2>/dev/null | grep 'Exited (127)' | awk '{print $1}' || true)

    if [ -z "$containers" ]; then
        return 1
    fi

    local fixed_names=()
    for name in $containers; do
        # Skip containers handled by other fixes
        if [ "$name" = "searxng" ]; then
            continue
        fi

        log "Container $name has exit code 127 — recreating"
        # Get image and create command for recreation
        local image
        image=$(podman inspect "$name" --format '{{.ImageName}}' 2>/dev/null || true)
        local create_cmd
        create_cmd=$(podman inspect "$name" --format '{{json .Config.CreateCommand}}' 2>/dev/null || true)

        podman rm -f "$name" 2>/dev/null || true

        if [ -n "$create_cmd" ] && [ "$create_cmd" != "null" ]; then
            # Re-run the original create command (strip the leading "podman" and "run")
            local recreate_args
            recreate_args=$(echo "$create_cmd" | python3 -c "
import json, sys
args = json.load(sys.stdin)
# Skip 'podman' and 'run', output the rest
print(' '.join(['\"' + a + '\"' if ' ' in a else a for a in args[2:]]))
" 2>/dev/null || true)

            if [ -n "$recreate_args" ]; then
                eval "podman run $recreate_args" 2>&1 || true
                fixed_names+=("$name")
                log "Recreated $name from original args"
            else
                fixed_names+=("$name(removed)")
                log "Removed $name — will be recreated on next deploy"
            fi
        else
            fixed_names+=("$name(removed)")
            log "Removed $name — will be recreated on next deploy"
        fi
    done

    [ ${#fixed_names[@]} -gt 0 ] && return 0 || return 1
}

# ── Fix 8: Rootless netns egress lost ────────────────────────
# Rootless podman uses pasta to give containers internet egress. If pasta's
# tap vanishes (host link flap, mount churn, pasta dying during a boot-time
# restart storm), the rootless-netns keeps inter-container traffic working
# but silently loses outbound. Bitcoin IBD stalls at 0 peers; package pulls
# fail. The repair must rebuild the netns from scratch: merely cycling the
# containers reuses the existing (broken) netns because its holders
# (aardvark-dns, podman's pause process) survive — observed on a test node
# 2026-07-10, where the old stop/start-only cycle bounced all 35 containers
# every timer run for ~an hour without ever restoring egress. So: stop the
# containers, kill the netns holders, `podman system migrate`, clear the
# stale netns state, then start everything back up.
#
# Destructive-action latch: cycling the whole fleet is a last resort. After
# NETNS_CYCLE_MAX consecutive failed repairs we stop cycling (and log loudly)
# until a run observes egress healthy again, which resets the counter.
NETNS_CYCLE_STATE="/var/lib/archipelago/doctor-netns-cycle-failures"
NETNS_CYCLE_MAX=3
fix_rootless_netns_egress() {
    # Needs root for nsenter. When doctor runs as the rootless container owner,
    # a failed nsenter probe is a permissions artifact, not evidence of broken
    # egress; do not cycle the fleet from that context.
    [ "$(id -u)" = "0" ] || return 1

    local archi_uid
    archi_uid=$(id -u archipelago 2>/dev/null) || return 1

    # Locate the rootless-netns via aardvark-dns (it lives inside it).
    local aardvark_pid
    aardvark_pid=$(pgrep -U "$archi_uid" -f '^/usr/lib/podman/aardvark-dns' 2>/dev/null | head -1)
    [ -z "$aardvark_pid" ] && return 1   # no rootless network active

    # Host precheck: if the host itself can't reach the internet, no point
    # cycling containers — this is an upstream problem.
    if ! timeout 3 bash -c '</dev/tcp/1.1.1.1/443' 2>/dev/null; then
        return 1
    fi

    # Probe egress from inside the rootless-netns. One probe is noisy;
    # require two consecutive failures 10s apart to rule out transients.
    if timeout 3 nsenter -t "$aardvark_pid" -n bash -c '</dev/tcp/1.1.1.1/443' 2>/dev/null; then
        rm -f "$NETNS_CYCLE_STATE"   # healthy again — re-arm the latch
        return 1   # first probe succeeded
    fi
    sleep 10
    aardvark_pid=$(pgrep -U "$archi_uid" -f '^/usr/lib/podman/aardvark-dns' 2>/dev/null | head -1)
    [ -z "$aardvark_pid" ] && return 1
    if timeout 3 nsenter -t "$aardvark_pid" -n bash -c '</dev/tcp/1.1.1.1/443' 2>/dev/null; then
        rm -f "$NETNS_CYCLE_STATE"
        return 1   # recovered on its own
    fi

    # Latch: don't keep bouncing the fleet when the rebuild demonstrably
    # isn't fixing it.
    local failures
    failures=$(cat "$NETNS_CYCLE_STATE" 2>/dev/null || echo 0)
    case "$failures" in *[!0-9]*|"") failures=0;; esac
    if [ "$failures" -ge "$NETNS_CYCLE_MAX" ]; then
        log "Rootless-netns egress still broken but $failures rebuilds already failed — NOT cycling again (manual intervention needed; rm $NETNS_CYCLE_STATE to re-arm)"
        return 1
    fi

    log "Rootless-netns egress is broken (host online, container netns unreachable) — rebuilding netns"

    local PODMANCMD="sudo -u archipelago XDG_RUNTIME_DIR=/run/user/$archi_uid podman"
    local running
    running=$($PODMANCMD ps --format '{{.Names}}' 2>/dev/null)
    if [ -z "$running" ]; then
        log "  No running containers to cycle — skipping"
        return 1
    fi

    local count
    count=$(echo "$running" | wc -l)
    log "  Stopping $count running containers (graceful, 30s)..."
    $PODMANCMD stop --all --time 30 >/dev/null 2>&1
    sleep 5

    # Tear the broken netns down for real: kill its holders and drop the
    # stale state so the first container start rebuilds pasta + aardvark-dns
    # from scratch. Without this, podman re-enters the old netns and the
    # missing pasta tap never comes back.
    log "  Rebuilding rootless netns (killing holders, clearing state)..."
    pkill -U "$archi_uid" -x aardvark-dns 2>/dev/null
    pkill -U "$archi_uid" -x pasta 2>/dev/null
    pkill -U "$archi_uid" -x pasta.avx2 2>/dev/null
    pkill -U "$archi_uid" -x slirp4netns 2>/dev/null
    sleep 2
    $PODMANCMD system migrate >/dev/null 2>&1
    rm -rf "/run/user/$archi_uid/containers/networks"

    log "  Starting containers back up..."
    for c in $running; do
        $PODMANCMD start "$c" >/dev/null 2>&1 &
    done
    wait
    sleep 5

    aardvark_pid=$(pgrep -U "$archi_uid" -f '^/usr/lib/podman/aardvark-dns' 2>/dev/null | head -1)
    if [ -n "$aardvark_pid" ] && timeout 3 nsenter -t "$aardvark_pid" -n bash -c '</dev/tcp/1.1.1.1/443' 2>/dev/null; then
        log "  Rootless-netns egress restored ($count containers cycled)"
        rm -f "$NETNS_CYCLE_STATE"
    else
        failures=$((failures + 1))
        echo "$failures" > "$NETNS_CYCLE_STATE"
        log "  WARN: egress still broken after rebuild (failure $failures/$NETNS_CYCLE_MAX) — may need manual intervention"
    fi
    return 0
}

# ── Fix 9: Restart stopped core containers ──────────────────
# Rootless Podman 4.x restart policies don't auto-restart on crash.
# This check restarts any exited core containers (tiers 0-2).
fix_stopped_core_containers() {
    local core_containers="bitcoin-knots lnd electrumx mempool-api archy-mempool-web archy-mempool-db archy-btcpay-db archy-nbxplorer btcpay-server"
    local restarted=()
    # Doctor runs as root but containers are rootless under archipelago user
    local PODMANCMD="sudo -u archipelago XDG_RUNTIME_DIR=/run/user/1000 podman"

    for name in $core_containers; do
        local state
        state=$($PODMANCMD inspect "$name" --format '{{.State.Status}}' 2>/dev/null || echo "missing")
        if [ "$state" = "exited" ] || [ "$state" = "stopped" ]; then
            log "Restarting stopped container: $name"
            $PODMANCMD start "$name" 2>/dev/null && restarted+=("$name") || true
        fi
    done

    [ ${#restarted[@]} -gt 0 ] && return 0 || return 1
}

# ── Fix 10: Missing rootless port listeners ─────────────────
# Rootless Podman can leave a container running with PortBindings still present
# while the host-side rootlessport process has disappeared. Nginx then returns
# 502 and direct app ports refuse connections even though `podman ps` looks OK.
fix_missing_rootless_ports() {
    local containers
    containers=$(podman_rootless ps --format '{{.Names}}' 2>/dev/null || true)
    [ -n "$containers" ] || return 1

    local fixed=false
    local name
    for name in $containers; do
        local ports
        ports=$(podman_rootless inspect "$name" --format '{{range $p,$bindings := .NetworkSettings.Ports}}{{if $bindings}}{{range $bindings}}{{.HostPort}}{{"\n"}}{{end}}{{end}}{{end}}' 2>/dev/null | sort -u)
        [ -n "$ports" ] || continue

        local missing=()
        local port
        for port in $ports; do
            [ -n "$port" ] || continue
            if ! port_is_listening "$port"; then
                missing+=("$port")
            fi
        done

        if [ ${#missing[@]} -gt 0 ]; then
            log "Restarting $name: missing rootlessport listener(s): ${missing[*]}"
            if podman_rootless restart "$name" >/dev/null 2>&1; then
                fixed=true
            else
                log "WARN: failed to restart $name for missing rootlessport listener(s)"
            fi
        fi
    done

    $fixed && return 0 || return 1
}

# ── Fix 11: Nginx Proxy Manager public host bridge ───────────
# Host nginx owns public 80/443 on Archipelago. Mirror NPM proxy hosts into
# host nginx so issued certs and public traffic reach the intended upstreams.
fix_npm_public_hosts() {
    local script="/opt/archipelago/scripts/sync-npm-public-hosts.sh"
    [ -x "$script" ] || script="$SCRIPT_DIR/sync-npm-public-hosts.sh"
    [ -x "$script" ] || return 1
    [ -f /var/lib/archipelago/nginx-proxy-manager/data/database.sqlite ] || return 1

    if "$script" >/dev/null 2>&1; then
        log "Synced Nginx Proxy Manager public hosts into host nginx"
        return 0
    fi
    return 1
}

# ── Fix 12: BTCPay Lightning route hints ─────────────────────
# A BTCPay store whose LND node has only private (unannounced) channels
# produces BOLT11 invoices that external wallets cannot route to unless the
# store's lightningPrivateRouteHints flag is on — payers see "no way to pay
# this invoice" (observed on a test node 2026-07-10 with a Blink payer). Route
# hints are a no-op with public channels and essential with private ones, so
# the doctor enforces the flag on every store. BTCPay reads store blobs from
# Postgres per request; no restart needed.
fix_btcpay_route_hints() {
    local state
    state=$(podman_rootless inspect archy-btcpay-db --format '{{.State.Status}}' 2>/dev/null || echo "missing")
    [ "$state" = "running" ] || return 1

    local count
    count=$(podman_rootless exec archy-btcpay-db psql -U btcpay -d btcpay -t -A -c \
        "SELECT count(*) FROM \"Stores\" WHERE (\"StoreBlob\"->>'lightningPrivateRouteHints') = 'false';" 2>/dev/null)
    [ -n "$count" ] && [ "$count" -gt 0 ] 2>/dev/null || return 1

    if podman_rootless exec archy-btcpay-db psql -U btcpay -d btcpay -q -c \
        "UPDATE \"Stores\" SET \"StoreBlob\" = jsonb_set(\"StoreBlob\", '{lightningPrivateRouteHints}', 'true'::jsonb) WHERE (\"StoreBlob\"->>'lightningPrivateRouteHints') = 'false';" >/dev/null 2>&1; then
        log "Enabled Lightning route hints on $count BTCPay store(s) (private-channel invoices were unpayable)"
        return 0
    fi
    return 1
}

# ── Fix 13: Missing catatonit (container init binary) ────────
# Podman resolves `--init` (and any Portainer/compose deploy with
# "init: true") through catatonit; Debian's podman package only
# Recommends it, so a node installed or upgraded without it fails those
# deploys with a missing-init error (observed on a test node 2026-07-10
# deploying sites via Portainer). install-podman.sh covers fresh ISO
# installs; this heals nodes that predate it.
fix_missing_catatonit() {
    command -v catatonit >/dev/null 2>&1 && return 1
    command -v apt-get >/dev/null 2>&1 || return 1

    if DEBIAN_FRONTEND=noninteractive apt-get install -y catatonit >/dev/null 2>&1; then
        log "Installed catatonit (init-enabled container deploys were failing)"
        return 0
    fi
    log "WARNING: catatonit missing and apt-get install failed — init-enabled deploys will fail"
    return 1
}

# ── Fix 14: archipelago user missing dialout (mesh radios) ───
# The image used to create the archipelago user with only `sudo`, so the
# backend couldn't open /dev/ttyUSB*/ttyACM* serial LoRa radios — Mesh
# never detected a plugged-in device (observed on the 1.7.99 RC install
# 2026-07-13). Group change takes effect on the next service restart; we
# restart archipelago only if a serial device is actually present.
fix_archipelago_dialout() {
    id -nG archipelago 2>/dev/null | grep -qw dialout && return 1
    usermod -aG dialout archipelago 2>/dev/null || return 1
    log "Added archipelago to dialout (serial mesh radios were unreadable)"
    if ls /dev/ttyUSB* /dev/ttyACM* >/dev/null 2>&1; then
        systemctl try-restart archipelago 2>/dev/null || true
        log "Restarted archipelago to pick up dialout (radio present)"
    fi
    return 0
}

# ── Main ─────────────────────────────────────────────────────

# If remote host provided, run via SSH
if [ -n "$1" ] && [ "$1" != "--local" ]; then
    REMOTE_HOST="$1"
    SSH_KEY="${ARCHIPELAGO_SSH_KEY:-$HOME/.ssh/archipelago-deploy}"
    SSH_OPTS="-o StrictHostKeyChecking=no -o ServerAliveInterval=15 -o ServerAliveCountMax=4 -i $SSH_KEY"

    log "Running container doctor on $REMOTE_HOST"

    # Copy script to remote and execute
    scp $SSH_OPTS "$0" "$REMOTE_HOST:/tmp/container-doctor.sh" 2>/dev/null
    ssh $SSH_OPTS "$REMOTE_HOST" "sudo bash /tmp/container-doctor.sh --local" 2>&1
    exit 0
fi

# Running locally (on the node itself)
log "Starting container health check"

run_fix "stale-podman"    fix_stale_podman
run_fix "orphaned-conmon" fix_orphaned_conmon
run_fix "system-tor"      fix_system_tor_conflict
run_fix "tor-permissions"  fix_tor_permissions
run_fix "searxng"         fix_searxng
run_fix "bitcoin-txindex" fix_bitcoin_txindex
run_fix "exit-127"        fix_exit_127
run_fix "netns-egress"    fix_rootless_netns_egress
run_fix "stopped-core"    fix_stopped_core_containers
run_fix "rootless-ports" fix_missing_rootless_ports
run_fix "npm-public-hosts" fix_npm_public_hosts
run_fix "btcpay-route-hints" fix_btcpay_route_hints
run_fix "catatonit"       fix_missing_catatonit
run_fix "dialout"         fix_archipelago_dialout

echo ""
if [ $FIXES_APPLIED -gt 0 ]; then
    log "Done: $FIXES_APPLIED fixes applied (${FIX_NAMES[*]}), $CHECKS_PASSED checks passed"
else
    log "Done: all $CHECKS_PASSED checks passed — no fixes needed"
fi

exit 0
