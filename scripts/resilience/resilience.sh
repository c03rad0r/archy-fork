#!/bin/bash
# Archipelago resilience harness — black-box state-machine tester for app containers.
#
# Drives the live archipelago RPC against a real podman runtime on a target
# host. For each app in the catalog, runs every state transition a user could
# trigger (install / probe / stop / start / restart / archipelago-restart /
# host-reboot / uninstall / reinstall / vanish-watch) and asserts the system
# remains in the expected state at every step.
#
# Usage:
#     scripts/resilience/resilience.sh archipelago@192.0.2.10 [filter]
#
# `filter` is a comma-separated list of app IDs (or "smoke" for the curated
# fast subset). Default: every app in app-catalog/catalog.json.
#
# Exit codes:
#     0  every cell green
#     1  any cell red — release should not ship
#     2  setup/auth error before tests began

set -uo pipefail

# ── args ─────────────────────────────────────────────────────────
TARGET="${1:?usage: $0 <user@host> [filter]}"
FILTER="${2:-}"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
HERE="$ROOT/scripts/resilience"
RUN_TS="$(date -u +%Y%m%dT%H%M%SZ)"
OUT_DIR="$HERE/reports/$RUN_TS"
mkdir -p "$OUT_DIR"
COOKIE_JAR="$OUT_DIR/cookies.txt"

HOST="$(echo "$TARGET" | cut -d@ -f2)"
# RPC reaches archipelago through nginx on 443 (which proxies to localhost:5678).
# Direct :5678 is bound to 127.0.0.1 on the target so we can't curl it from here.
RPC_URL="https://$HOST/rpc/v1"

export TARGET RPC_URL COOKIE_JAR OUT_DIR

# shellcheck source=lib.sh
. "$HERE/lib.sh"

# ── credentials ──────────────────────────────────────────────────
# Pull from env first (so this script can be called from CI). Fall back to
# interactive prompts.
SSH_PASS="${RESILIENCE_SSH_PASS:-}"
UI_PASS="${RESILIENCE_UI_PASS:-}"
if [ -z "$SSH_PASS" ]; then
    read -rsp "SSH password for $TARGET: " SSH_PASS; echo
fi
if [ -z "$UI_PASS" ]; then
    read -rsp "Archipelago UI password: " UI_PASS; echo
fi
export SSH_PASS UI_PASS

command -v sshpass >/dev/null || { echo "sshpass required"; exit 2; }
command -v jq >/dev/null      || { echo "jq required"; exit 2; }

ssh_run 'echo ok' >/dev/null || { echo "ssh to $TARGET failed"; exit 2; }
rpc_login                    || exit 2

echo "Resilience harness — target $TARGET, run $RUN_TS"
echo "Output: $OUT_DIR/results.jsonl"
echo "─────────────────────────────────────────────────────────────"

# ── catalog & filter ─────────────────────────────────────────────
CATALOG="$ROOT/app-catalog/catalog.json"
ALL_APPS=$(jq -r '.apps[].id' "$CATALOG")

# Topo-sort the catalog by `requires`. Outputs app IDs in install order
# (deps first, then dependents). Kahn's algorithm via python — keeps the
# bash side simple and the deps logic obvious for next-time-readers.
topo_order() {
    python3 -c "
import json
with open('$CATALOG') as f: c = json.load(f)
deps = {a['id']: list(a.get('requires', [])) for a in c['apps']}
order = []
remaining = set(deps)
while remaining:
    ready = sorted(a for a in remaining if all(d not in remaining for d in deps[a]))
    if not ready:  # cycle (shouldn't happen) — emit whatever's left
        order.extend(sorted(remaining)); break
    order.extend(ready); remaining.difference_update(ready)
print('\n'.join(order))
"
}

apps_to_test() {
    local order; order=$(topo_order)
    if [ -z "$FILTER" ]; then
        # Full sweep — but skip bitcoin-core since it shares container slots
        # with bitcoin-knots; testing both back-to-back would just churn the
        # same containers. bitcoin-knots is the canonical entry.
        echo "$order" | grep -v '^bitcoin-core$'
    elif [ "$FILTER" = "smoke" ]; then
        # Fast subset exercising the bug classes we just fixed:
        # single-container, multi-container stack, credentialed UI.
        echo -e "filebrowser\nbitcoin-knots\nindeedhub"
    else
        echo "$order" | grep -E "^($(echo "$FILTER" | tr ',' '|'))$"
    fi
}

# Resolve `requires` chain for $1 in install-order (deps first).
deps_for_app() {
    local app="$1"
    python3 -c "
import json
with open('$CATALOG') as f: c = json.load(f)
deps_map = {a['id']: list(a.get('requires', [])) for a in c['apps']}
visited, order = set(), []
def visit(x):
    if x in visited or x not in deps_map: return
    visited.add(x)
    for d in deps_map.get(x, []): visit(d)
    order.append(x)
for d in deps_map.get('$app', []): visit(d)
print('\n'.join(order))
"
}

# ── per-app transitions ──────────────────────────────────────────
# Diff helper: capture container names matching a sane prefix for $app_id.
# Approach: snapshot before install, snapshot after, take the difference =
# this app's containers.
snapshot_containers() {
    ssh_run "podman ps -a --format '{{.Names}}' | sort"
}

# Whether $app currently has ALL of its expected containers running. Uses
# the per-app metadata table in lib.sh (expected_containers_for) so variant
# apps (bitcoin-knots/bitcoin-core sharing slots) and stacks are detected
# correctly. Falls back to name-prefix match for apps the table doesn't know.
#
# Returns true only when every expected container is present. Earlier
# versions returned true on ANY match — that caused dep installs (e.g.
# bitcoin-knots required by btcpay) to be declared "installed" as soon as
# the backend container appeared, before the UI companion (archy-bitcoin-ui)
# was up. The before-snapshot then missed the companion, the after-snapshot
# caught it, and it leaked into the dependent app's "new containers" set,
# false-positive-FAILing stop/uninstall when the companion (correctly) did
# not respond to the dependent app's package.stop.
app_already_installed() {
    local app="$1"
    local snap; snap=$(snapshot_containers)
    local expected
    expected=$(expected_containers_for "$app")
    if [ -n "$expected" ] && [ "$expected" != "$app" ]; then
        local c missing=0
        for c in $expected; do
            echo "$snap" | grep -qxF "$c" || missing=1
        done
        [ "$missing" -eq 0 ] && return 0
        # Fall through to prefix match if the expected_containers list has
        # gaps; a partial install still counts as "installed enough" for
        # preclean purposes.
    fi
    # Generic prefix fallback for apps not in the expected_containers_for table.
    echo "$snap" | grep -qE "^(${app}|${app}-|archy-${app}|archy-${app}-)"
}

# Install missing deps for $app via the regular install path. Idempotent —
# already-installed deps are skipped. Records dep_install per dep so we can
# tell from the report whether the bitcoin pre-req was actually green by the
# time lnd's matrix started.
ensure_deps_installed() {
    local app="$1"
    local dep
    for dep in $(deps_for_app "$app"); do
        if app_already_installed "$dep"; then
            continue
        fi
        echo "  · dep install: $dep (required by $app)"
        local img ver resp
        img=$(jq -r --arg id "$dep" '.apps[] | select(.id==$id) | .dockerImage // ""' "$CATALOG")
        ver=$(jq -r --arg id "$dep" '.apps[] | select(.id==$id) | .version // ""' "$CATALOG")
        if [ -z "$img" ]; then
            record "$app" "dep_$dep" FAIL "no dockerImage in catalog for dep $dep"
            return 1
        fi
        resp=$(rpc_call "package.install" "$(jq -nc \
            --arg id "$dep" --arg img "$img" --arg ver "$ver" \
            '{id:$id, dockerImage:$img, version:$ver}')")
        if echo "$resp" | jq -e '.error' >/dev/null 2>&1; then
            record "$app" "dep_$dep" FAIL "rpc error: $(echo "$resp" | jq -c '.error')"
            return 1
        fi
        # Wait for at least one expected container to appear running.
        local deadline=$(($(date +%s) + 600))
        while [ "$(date +%s)" -lt "$deadline" ]; do
            if app_already_installed "$dep"; then
                record "$app" "dep_$dep" PASS "installed"
                break
            fi
            sleep 5
        done
        if ! app_already_installed "$dep"; then
            record "$app" "dep_$dep" FAIL "containers did not appear within 10min"
            return 1
        fi
    done
    return 0
}

# Pre-clean: if the app is currently installed, uninstall it and wait for
# all containers to disappear. We can't measure install correctness without
# starting from a clean slate. Fail-soft — if the uninstall RPC errors we
# log but proceed; the install step will catch any residual state.
preclean_app() {
    local app="$1"
    if ! app_already_installed "$app"; then
        return 0
    fi
    echo "  · pre-clean: $app already installed, uninstalling first"
    local resp; resp=$(rpc_call "package.uninstall" "{\"id\":\"$app\"}")
    if echo "$resp" | jq -e '.error' >/dev/null 2>&1; then
        echo "    pre-clean uninstall RPC error: $(echo "$resp" | jq -c '.error')"
    fi
    # Multi-container stacks (indeedhub: 7, immich: 5, mempool: 3, btcpay: 6)
    # take noticeably longer to tear down than single-container apps. 240s was
    # too tight for indeedhub's 7-container teardown — bump to 10 min for
    # safety; per-container timeout is still bounded inside archipelago itself.
    local deadline=$(($(date +%s) + 600))
    while [ "$(date +%s)" -lt "$deadline" ]; do
        if ! app_already_installed "$app"; then return 0; fi
        sleep 5
    done
    echo "    pre-clean: timeout waiting for $app to uninstall"
    return 1
}

# Run the full per-app matrix. Records a row per transition.
run_app_matrix() {
    local app="$1"
    echo
    echo "═══ $app ═══"

    if ! ensure_deps_installed "$app"; then
        record "$app" install FAIL "dep install failed; skipping rest of matrix"
        return
    fi
    preclean_app "$app" || record "$app" preclean FAIL "uninstall before test did not complete"

    # ── 01 install ───────────────────────────────────────────────
    local before after new_containers
    before=$(snapshot_containers)
    # The install handler requires `id` + `dockerImage` from the catalog
    # entry. Match what the UI passes (Discover.vue / MarketplaceAppDetails.vue).
    local docker_image version
    docker_image=$(jq -r --arg id "$app" '.apps[] | select(.id==$id) | .dockerImage // ""' "$CATALOG")
    version=$(jq -r --arg id "$app" '.apps[] | select(.id==$id) | .version // ""' "$CATALOG")
    if [ -z "$docker_image" ]; then
        record "$app" install FAIL "no dockerImage in catalog for $app"
        return
    fi
    local install_resp
    install_resp=$(rpc_call "package.install" "$(jq -nc \
        --arg id "$app" --arg img "$docker_image" --arg ver "$version" \
        '{id:$id, dockerImage:$img, version:$ver}')")
    if echo "$install_resp" | jq -e '.error' >/dev/null 2>&1; then
        record "$app" install FAIL "rpc error: $(echo "$install_resp" | jq -c '.error')"
        return  # cannot continue this app
    fi

    # Wait for the EXPECTED containers (per expected_containers_for) to all
    # appear. The old "snapshot stable for 10s + count > before" heuristic
    # terminated early on apps with deps: e.g. mempool's wait would break
    # when archy-electrs-ui (electrumx dep companion) appeared, long before
    # mempool's own containers were created (those take ~10min to pull and
    # start). Waiting on the expected-set is exact, not heuristic.
    #
    # Cap at 15 minutes — mempool stack with cold image cache needs ~12 min.
    local expected; expected=$(expected_containers_for "$app")
    local deadline=$(($(date +%s) + 900))
    while [ "$(date +%s)" -lt "$deadline" ]; do
        after=$(snapshot_containers)
        local missing=0
        for c in $expected; do
            echo "$after" | grep -qxF "$c" || missing=1
        done
        [ "$missing" -eq 0 ] && break
        sleep 5
    done
    new_containers=$(comm -13 <(echo "$before") <(echo "$after"))
    if [ -z "$new_containers" ]; then
        record "$app" install FAIL "no containers created within 10min"
        return
    fi
    # Assert each new container is in 'running' state.
    local install_ok=1; local detail=""
    while read -r c; do
        [ -z "$c" ] && continue
        local s
        s=$(probe_container_state "$c")
        if [ "$s" != "running" ]; then
            install_ok=0
            detail="$detail $c=$s"
        fi
    done <<< "$new_containers"
    if [ "$install_ok" -eq 1 ]; then
        record "$app" install PASS "$(echo "$new_containers" | tr '\n' ',' | sed 's/,$//')"
    else
        record "$app" install FAIL "containers not running:$detail"
    fi

    # ── 02 ui_probe ──────────────────────────────────────────────
    # Retry with backoff — install just finished, but the app's backend
    # (fedimint, immich, mempool stack) may take 30+s to be ready to serve
    # HTTP. Probing immediately false-positive-FAILed those apps; pass on
    # first 2xx/3xx within 60s.
    local code
    local ui_deadline=$(($(date +%s) + 60))
    while :; do
        code=$(probe_app_proxy "$app")
        [[ "$code" =~ ^(2[0-9][0-9]|3[0-9][0-9])$ ]] && break
        [ "$(date +%s)" -ge "$ui_deadline" ] && break
        sleep 5
    done
    # Accept all 2xx/3xx — proxy reaches backend, app may redirect to login,
    # serve OAuth flow (307), or use 308 permanent. 401/403 still fail because
    # those mean "backend reached, app rejected request" which is the
    # credential-plumbing failure mode we DO want to catch.
    if [[ "$code" =~ ^(2[0-9][0-9]|3[0-9][0-9])$ ]]; then
        record "$app" ui_probe PASS "HTTP $code"
    else
        record "$app" ui_probe FAIL "HTTP $code (expected 2xx/3xx, retried 60s)"
    fi

    # ── 03 auth_probe (only for apps with a credentialed/data endpoint) ──
    # Same backoff treatment: bitcoin-ui's nginx config bind-mount is
    # picked up at start, but the bitcoin-core backend may not have
    # accepted RPC connections yet on a fresh install.
    local probe_code; local pass_codes
    pass_codes=$(auth_probe_pass_codes "$app")
    if probe_code=$(auth_probe_for "$app" 2>/dev/null) && [ -n "$probe_code" ]; then
        local auth_deadline=$(($(date +%s) + 60))
        while :; do
            echo " $pass_codes " | grep -qF " $probe_code " && break
            [ "$(date +%s)" -ge "$auth_deadline" ] && break
            sleep 5
            probe_code=$(auth_probe_for "$app" 2>/dev/null) || break
        done
        if echo " $pass_codes " | grep -qF " $probe_code "; then
            record "$app" auth_probe PASS "HTTP $probe_code"
        else
            record "$app" auth_probe FAIL "HTTP $probe_code (expected one of: $pass_codes; retried 60s — credential plumbing broken)"
        fi
    else
        record "$app" auth_probe SKIP "no authenticated probe defined"
    fi

    # ── 04 stop ──────────────────────────────────────────────────
    local stop_resp
    stop_resp=$(rpc_call "package.stop" "{\"id\":\"$app\"}")
    if echo "$stop_resp" | jq -e '.error' >/dev/null 2>&1; then
        record "$app" stop FAIL "rpc error: $(echo "$stop_resp" | jq -c '.error')"
    else
        local all_stopped=1
        while read -r c; do
            [ -z "$c" ] && continue
            wait_for_container_state "$c" "exited" 60 || all_stopped=0
        done <<< "$new_containers"
        if [ "$all_stopped" -eq 1 ]; then
            record "$app" stop PASS
        else
            record "$app" stop FAIL "not all containers reached exited state"
        fi
    fi

    # ── 05 start ─────────────────────────────────────────────────
    local start_resp
    start_resp=$(rpc_call "package.start" "{\"id\":\"$app\"}")
    if echo "$start_resp" | jq -e '.error' >/dev/null 2>&1; then
        record "$app" start FAIL "rpc error: $(echo "$start_resp" | jq -c '.error')"
    else
        local all_started=1
        while read -r c; do
            [ -z "$c" ] && continue
            wait_for_container_state "$c" "running" 90 || all_started=0
        done <<< "$new_containers"
        if [ "$all_started" -eq 1 ]; then
            record "$app" start PASS
        else
            record "$app" start FAIL "not all containers reached running state"
        fi
    fi

    # ── 06 restart_container ─────────────────────────────────────
    # `package.restart` returns immediately and spawns the actual restart.
    # `podman restart -t <stop_timeout>` blocks for up to stop_timeout
    # seconds (e.g. 600s for bitcoin-core). Polling once after sleep 5
    # races on slow-stopping apps and false-positive-FAILs them. Poll
    # each container up to 90s for "running" instead.
    local restart_resp
    restart_resp=$(rpc_call "package.restart" "{\"id\":\"$app\"}")
    if echo "$restart_resp" | jq -e '.error' >/dev/null 2>&1; then
        record "$app" restart FAIL "rpc error: $(echo "$restart_resp" | jq -c '.error')"
    else
        local all_running=1
        while read -r c; do
            [ -z "$c" ] && continue
            wait_for_container_state "$c" "running" 90 || all_running=0
        done <<< "$new_containers"
        if [ "$all_running" -eq 1 ]; then
            record "$app" restart PASS
        else
            record "$app" restart FAIL "container not running 90s after restart"
        fi
    fi

    # ── 09 uninstall (skip 07 archipelago-restart and 08 host-reboot
    #    here — those are batch tests run once across all installed apps) ─
    local uninst_resp
    uninst_resp=$(rpc_call "package.uninstall" "{\"id\":\"$app\"}")
    if echo "$uninst_resp" | jq -e '.error' >/dev/null 2>&1; then
        record "$app" uninstall FAIL "rpc error: $(echo "$uninst_resp" | jq -c '.error')"
    else
        # Wait for all this-app containers to be absent.
        local all_gone=1
        while read -r c; do
            [ -z "$c" ] && continue
            wait_for_container_state "$c" "absent" 120 || all_gone=0
        done <<< "$new_containers"
        if [ "$all_gone" -eq 1 ]; then
            record "$app" uninstall PASS
        else
            record "$app" uninstall FAIL "not all containers removed"
        fi
    fi
}

# ── batch transitions (run after per-app loop) ───────────────────
batch_archipelago_service_restart() {
    echo
    echo "═══ batch: archipelago.service restart ═══"
    local before; before=$(snapshot_containers)
    if ! ssh_run 'sudo systemctl restart archipelago'; then
        record "_batch" archipelago_restart FAIL "systemctl restart errored"
        return
    fi
    ssh_wait_ready 60 || { record "_batch" archipelago_restart FAIL "ssh did not return"; return; }
    sleep 30  # let containers re-stabilize
    rpc_login || { record "_batch" archipelago_restart FAIL "rpc relogin failed"; return; }
    local after; after=$(snapshot_containers)
    if [ "$before" = "$after" ]; then
        record "_batch" archipelago_restart PASS "container set unchanged"
    else
        record "_batch" archipelago_restart FAIL "container set drifted across restart"
    fi
}

batch_host_reboot() {
    echo
    echo "═══ batch: host reboot ═══"
    local before; before=$(snapshot_containers)
    ssh_run 'sudo systemctl reboot' || true  # ssh disconnects immediately
    sleep 30
    # 5 min was too short — .228 took ~9min for full BIOS+kernel+systemd+
    # rootless-podman boot. 12 min gives margin for slower hardware.
    ssh_wait_ready 720 || { record "_batch" host_reboot FAIL "host did not come back in 12min"; return; }
    sleep 60  # let containers auto-restart
    rpc_login || { record "_batch" host_reboot FAIL "rpc unreachable after reboot"; return; }
    local after; after=$(snapshot_containers)
    if [ "$before" = "$after" ]; then
        record "_batch" host_reboot PASS "all containers came back"
    else
        local missing
        missing=$(comm -23 <(echo "$before") <(echo "$after") | tr '\n' ',' | sed 's/,$//')
        record "_batch" host_reboot FAIL "missing: $missing"
    fi

    # ── L3 per-boot health gate ──────────────────────────────────
    # Container-set equality proves the right containers exist; os-audit proves
    # the node is actually *healthy* after the reboot: RPC up, OTA not wedged
    # (FM12), every app reachable with valid launch metadata, FM-guards green.
    # This is the per-boot building block os-audit.sh was written to be.
    if [ -x "$ROOT/tests/lifecycle/os-audit.sh" ]; then
        echo "── per-boot os-audit gate ──"
        if ARCHY_HOST="$HOST" ARCHY_SCHEME=https ARCHY_PASSWORD="$UI_PASS" ARCHY_LOCAL=0 \
             "$ROOT/tests/lifecycle/os-audit.sh" >"$OUT_DIR/os-audit-postboot.log" 2>&1; then
            record "_batch" host_reboot_osaudit PASS "os-audit green after reboot"
        else
            record "_batch" host_reboot_osaudit FAIL "os-audit not green after reboot (see $OUT_DIR/os-audit-postboot.log)"
        fi
    fi
}

# ── main ─────────────────────────────────────────────────────────
APPS_LIST=$(apps_to_test)
if [ -z "$APPS_LIST" ]; then
    echo "no apps match filter '$FILTER'" >&2; exit 2
fi

while read -r app; do
    [ -z "$app" ] && continue
    run_app_matrix "$app"
done <<< "$APPS_LIST"

# Batch transitions only run on full sweep (skip in filtered/smoke mode).
if [ -z "$FILTER" ]; then
    batch_archipelago_service_restart
    batch_host_reboot
fi

# ── summary ──────────────────────────────────────────────────────
echo
echo "═══ summary ═══"
count_status() {
    local pat="$1"
    [ -s "$OUT_DIR/results.jsonl" ] || { echo 0; return; }
    awk -v pat="$pat" '$0 ~ pat { n++ } END { print n+0 }' "$OUT_DIR/results.jsonl"
}
PASS=$(count_status '"status":"PASS"')
FAIL=$(count_status '"status":"FAIL"')
SKIP=$(count_status '"status":"SKIP"')
TOTAL=$((PASS + FAIL + SKIP))
echo "PASS: $PASS / FAIL: $FAIL / SKIP: $SKIP / TOTAL: $TOTAL"
echo "Report: $OUT_DIR/results.jsonl"

[ "$FAIL" -eq 0 ] || exit 1
exit 0
