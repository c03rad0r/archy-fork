#!/usr/bin/env bash
# tests/lifecycle/run-gate.sh — loop the lifecycle harness N times (default 5×, the release gate).
#
# Each iteration: setup-teardown → run.sh (with the same args you'd pass
# to run.sh) → setup-teardown. Tallies pass/fail per iteration and prints a
# summary at the end. Returns non-zero if any iteration failed.
#
# Env:
#   ARCHY_ITERATIONS                    (default: 5)
#   ARCHY_FAIL_FAST=1                   stop on first failed iteration
#   ARCHY_GATE_CASCADE=1                after the 5× loop, run ONE cascade pass
#                                       (uninstall→no-ghost→reinstall a throwaway
#                                       app); requires ARCHY_ALLOW_DESTRUCTIVE=1
#   ARCHY_PREFLIGHT=0                   skip the host-readiness preflight
#   ARCHY_MAX_LOAD                      load1 ceiling (default: nproc + 1)
#   ARCHY_PREFLIGHT_SECS                how long to wait for load to fall
#                                       (default: 900)
#   plus everything run.sh / lib/rpc.bash respects
#     (ARCHY_PASSWORD, ARCHY_HOST, ARCHY_SCHEME, ARCHY_ALLOW_DESTRUCTIVE,
#      ARCHY_ALLOW_CASCADE_DESTRUCTIVE, ARCHY_ALLOW_NOAUTH)
#
# Usage:
#   tests/lifecycle/run-gate.sh                       # 5× full bats/ suite
#   ARCHY_ITERATIONS=5 tests/lifecycle/run-gate.sh    # 5× full suite
#   tests/lifecycle/run-gate.sh bitcoin-knots          # 5× a single suite
#
# Suggested release-gate invocation:
#   ARCHY_PASSWORD=password123 ARCHY_ALLOW_DESTRUCTIVE=1 \
#     tests/lifecycle/run-gate.sh
#
# Release-gate WITH the cascade tier (uninstall/reinstall regression guard):
#   ARCHY_PASSWORD=password123 ARCHY_ALLOW_DESTRUCTIVE=1 ARCHY_GATE_CASCADE=1 \
#     tests/lifecycle/run-gate.sh

set -euo pipefail

HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"

ITER="${ARCHY_ITERATIONS:-5}"
if ! [[ "$ITER" =~ ^[1-9][0-9]*$ ]]; then
  echo "ARCHY_ITERATIONS must be a positive integer, got: $ITER" >&2
  exit 2
fi

passed=0
failed=0
failures=()
start=$(date +%s)

# Best-effort settle: wait for the backend stack to be healthy before an
# iteration starts, so back-to-back destructive iterations don't compound
# restart churn (lnd wallet-unlock + the 4-container mempool stack reconnect
# need time to recover). On-node gate only (localhost probes); never fails the
# run — just delays up to the deadline. Disable with ARCHY_SETTLE=0.
settle_stack() {
  [[ "${ARCHY_SETTLE:-1}" == "1" && "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]] || return 0
  # 300s (not 180s): on heavy nodes the immich stack's recovery after the prior
  # iteration's archipelago-restart test (crash_recovery retries on a ~120s
  # cadence) can take several minutes, and the next iteration's read-only
  # lan_address probe false-fails if immich is still mid-boot. The settle is a
  # cap, not a fixed wait — it returns the instant every probe is green.
  local deadline=$(( $(date +%s) + ${ARCHY_SETTLE_SECS:-300} ))
  while (( $(date +%s) < deadline )); do
    local ok=1
    # mempool-api + frontend + bitcoin-ui = good proxies for "stack reconnected"
    curl -fsS -m 4 -o /dev/null "http://127.0.0.1:8999/api/v1/backend-info" 2>/dev/null || ok=0
    curl -fsS -m 4 -o /dev/null "http://127.0.0.1:4080/" 2>/dev/null || ok=0
    podman exec lnd lncli --tlscertpath /root/.lnd/tls.cert \
      --macaroonpath /root/.lnd/data/chain/bitcoin/mainnet/readonly.macaroon \
      --rpcserver localhost:10009 getinfo >/dev/null 2>&1 || ok=0
    # Only gate on immich where it's actually installed (heavy nodes). Its web
    # port is the same signal test 64 checks, so settling here keeps the next
    # iteration's read-only immich probe from racing a still-recovering stack.
    if podman container exists immich_server 2>/dev/null; then
      curl -fsS -m 4 -o /dev/null "http://127.0.0.1:2283/" 2>/dev/null || ok=0
    fi
    (( ok == 1 )) && { echo "  (stack settled)"; return 0; }
    sleep 4
  done
  echo "  (stack settle deadline reached — proceeding anyway)"
}

# Host readiness, checked ONCE before iteration 1.
#
# Why this exists: on 2026-08-08 a gate run on a box at load ~14 failed five
# times over, every failure reading "could not create a container" (searxng:start,
# package.start btcpay-server, 3× electrumx) and never a lifecycle fault. That
# sent two separate sessions hunting a phantom host-wide cgroup failure. It was
# load. Measured on that 4-core box: at load ~14 podman runs 9-16 processes deep
# and healthchecks time out 3-8/min; at load ~3.7, podman ~1 and zero timeouts.
#
# So: refuse to start on a loaded host rather than emit misleading failures.
# Note what is deliberately NOT checked — the count of "Failed to create
# container" in the journal. Those lines are emitted by healthcheck exec churn
# and by settling after a boot; they never reach 0 on a busy node, and gating on
# them blocks the gate forever. Prove container creation POSITIVELY instead.
preflight_host() {
  [[ "${ARCHY_PREFLIGHT:-1}" == "1" ]] || return 0
  command -v podman >/dev/null 2>&1 || return 0   # remote/off-node run

  local cores max_load
  cores=$(nproc 2>/dev/null || echo 4)
  max_load="${ARCHY_MAX_LOAD:-$((cores + 1))}"

  echo "── preflight: host readiness ──"

  # 1. aardvark-dns must be singular. Two of them serve divergent state and make
  #    container-name resolution flaky, which then looks like a lifecycle bug.
  local dns
  dns=$(pgrep -c aardvark-dns 2>/dev/null || echo 0)
  if (( dns > 1 )); then
    echo "  FAIL: $dns aardvark-dns processes running (expected 1)." >&2
    echo "        Duplicate DNS servers desync container-name resolution." >&2
    return 1
  fi
  echo "  aardvark-dns: $dns"

  # 2. Wait for load to fall. It oscillates on nodes doing IBD or media
  #    indexing, so a brief spike is not fatal — a sustained one is.
  local deadline=$(( $(date +%s) + ${ARCHY_PREFLIGHT_SECS:-900} ))
  local load1
  while :; do
    load1=$(awk '{print $1}' /proc/loadavg)
    awk -v l="$load1" -v m="$max_load" 'BEGIN { exit !(l < m) }' && break
    if (( $(date +%s) >= deadline )); then
      echo "  FAIL: load1 $load1 still above $max_load after ${ARCHY_PREFLIGHT_SECS:-900}s." >&2
      echo "        Quiesce the node (bitcoind IBD, electrumx indexing, CI runners)" >&2
      echo "        or override with ARCHY_MAX_LOAD=. Running now yields failures" >&2
      echo "        that look like lifecycle bugs but are contention." >&2
      return 1
    fi
    echo "  load1 $load1 > $max_load — waiting…"
    sleep 20
  done
  echo "  load1: $load1 (ceiling $max_load, $cores cores)"

  # 3. Prove the host can actually create a container, 3× — the positive test
  #    that the journal grep only ever approximated.
  local img
  img=$(podman images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null \
        | grep -v '<none>' | head -1)
  if [[ -z "$img" ]]; then
    echo "  (no local image — skipping container-create probe)"
  else
    local n
    for n in 1 2 3; do
      if ! timeout 60 podman run --rm "$img" /bin/true >/dev/null 2>&1; then
        echo "  FAIL: container-create probe $n/3 failed using $img." >&2
        echo "        The host genuinely cannot create containers; fix that first." >&2
        return 1
      fi
    done
    echo "  container-create probe: 3/3 via $img"
  fi

  echo "── preflight: OK ──"
}

if ! preflight_host; then
  echo "Preflight failed — refusing to start the gate. (ARCHY_PREFLIGHT=0 to skip.)" >&2
  exit 3
fi

# One initial teardown so a previous run's cookies don't poison iteration 1.
./setup-teardown.sh

for i in $(seq 1 "$ITER"); do
  echo
  echo "═══ iteration $i / $ITER ═══"
  iter_start=$(date +%s)
  settle_stack

  if ./run.sh "$@"; then
    iter_end=$(date +%s)
    passed=$((passed + 1))
    echo "── iteration $i: PASS ($((iter_end - iter_start))s) ──"
  else
    rc=$?
    iter_end=$(date +%s)
    failed=$((failed + 1))
    failures+=("$i")
    echo "── iteration $i: FAIL (exit=$rc, $((iter_end - iter_start))s) ──"
    if [[ "${ARCHY_FAIL_FAST:-0}" == "1" ]]; then
      echo "ARCHY_FAIL_FAST=1, stopping early"
      break
    fi
  fi

  # Teardown between iterations so iteration N+1 starts with a clean
  # session-cookie state regardless of what iteration N did.
  ./setup-teardown.sh
done

# Optional CASCADE pass — uninstall → no-ghost → reinstall of a throwaway app
# (default grafana, via cascade-uninstall.bats). Run ONCE, not folded into the
# 5× loop on purpose: uninstall/reinstall every iteration would balloon runtime
# and re-pull images. One pass gates the #13 ghost / #14 reinstall-stop /
# uninstall-hang class (the bug fixed in 71cc9ac4). Opt-in so default gate
# behavior is unchanged; counts into the pass/fail tally.
if [[ "${ARCHY_GATE_CASCADE:-0}" == "1" && "${ARCHY_ALLOW_DESTRUCTIVE:-0}" == "1" ]]; then
  echo
  echo "═══ CASCADE pass (1×) ═══"
  settle_stack
  if ARCHY_ALLOW_CASCADE_DESTRUCTIVE=1 ./run.sh cascade-uninstall; then
    passed=$((passed + 1))
    echo "── CASCADE: PASS ──"
  else
    failed=$((failed + 1))
    failures+=("cascade")
    echo "── CASCADE: FAIL ──"
  fi
  ./setup-teardown.sh
fi

end=$(date +%s)

echo
echo "════════════════════════════════════════"
echo " RESULTS"
echo "  iterations: $((passed + failed)) / $ITER"
echo "  passed:     $passed"
echo "  failed:     $failed"
if (( failed > 0 )); then
  echo "  failed at:  ${failures[*]}"
fi
echo "  wall time:  $((end - start))s"
echo "════════════════════════════════════════"

if (( failed > 0 )); then
  exit 1
fi
