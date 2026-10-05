#!/bin/bash
# Release gate harness — seed of the full-system test harness.
#
# Ties together the checks that already exist in this repo (catalog drift,
# release manifest, lifecycle bats, vitest, cargo tests) plus live-node
# smoke probes, so "is this release OK?" is one command instead of folklore.
#
# Usage:
#   tests/release/run.sh                 # static + frontend + backend stages
#   tests/release/run.sh --quick         # static + frontend unit only
#   tests/release/run.sh --with-build    # also production-build the frontend
#                                        # and verify the dist version changed
#   tests/release/run.sh --manifest      # also validate releases/manifest.json
#                                        # (run AFTER create-release staged it)
#   tests/release/run.sh --live [URL]    # also smoke-probe a running node
#                                        # (default http://127.0.0.1)
#
# Flags compose. Exits non-zero on the first failing stage.
#
# CAUTION (.116 and other dev nodes): full `cargo test -p archipelago` has
# hung tool PTYs here before — every cargo invocation below is wrapped in
# `timeout` and scoped to focused module filters.

set -u
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO"

QUICK=0 WITH_BUILD=0 MANIFEST=0 LIVE=0 LIVE_URL="http://127.0.0.1"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --quick) QUICK=1 ;;
    --with-build) WITH_BUILD=1 ;;
    --manifest) MANIFEST=1 ;;
    --live) LIVE=1; [[ "${2:-}" == http* ]] && { LIVE_URL="$2"; shift; } ;;
    *) echo "unknown flag: $1" >&2; exit 2 ;;
  esac
  shift
done

PASS=() FAIL=()
stage() { # stage <name> <cmd...>
  local name="$1"; shift
  echo
  echo "=== [$name] $*"
  if "$@"; then
    echo "=== [$name] PASS"
    PASS+=("$name")
  else
    local rc=$?
    # 124 is `timeout` killing the stage. Reporting it as a bare exit code
    # reads as "the tests failed" and sends you hunting for a broken test
    # that does not exist — say plainly that it ran out of wall clock.
    if [ "$rc" -eq 124 ]; then
      echo "=== [$name] FAIL (exit 124 — TIMED OUT, not a test failure)"
      echo "===   the command ran out of wall clock; re-run, or raise its timeout"
    else
      echo "=== [$name] FAIL (exit $rc)"
    fi
    FAIL+=("$name")
    summary 1
  fi
}
summary() {
  echo
  echo "──────── release gate summary ────────"
  printf 'PASS: %s\n' "${PASS[@]:-none}"
  [[ ${#FAIL[@]} -gt 0 ]] && printf 'FAIL: %s\n' "${FAIL[@]}"
  exit "${1:-0}"
}

# ── Stage 1: static ──────────────────────────────────────────────────
stage "git-diff-check"   git diff --check
stage "mirror-gate-regression" python3 scripts/tests/test_git_mirrors.py
stage "iso-boot-runner-regression" python3 scripts/tests/test_iso_qemu_runner.py
stage "demo-rpc-parity" timeout 120 node neode-ui/scripts/mock-rpc-parity.mjs
stage "demo-resumable-uploads" timeout 120 node --test neode-ui/scripts/demo-upload-test.mjs
stage "companion-signature-regression" python3 scripts/tests/test_companion_apk_verification.py
stage "cargo-fmt"        timeout 240 cargo fmt --manifest-path core/Cargo.toml --all --check
stage "app-build-contexts" python3 tests/regression/app-build-contexts.py
stage "manifest-shell"  python3 scripts/check-manifest-shell.py
stage "npm-tunnel-migration" python3 -m unittest discover -s scripts/tests -p test_repair_npm_tunnel.py
stage "npm-public-bridge" python3 -m unittest discover -s scripts/tests -p test_npm_public_bridge.py
stage "filebrowser-credentials" python3 -m unittest discover -s scripts/tests -p test_filebrowser_credentials.py
stage "dashboard-source-guard" python3 -m unittest discover -s scripts/tests -p test_dashboard_public_guard.py
stage "catalog-capabilities" python3 -m unittest discover -s scripts/tests -p test_catalog_capabilities.py
stage "dashboard-network-isolated" bash scripts/test-dashboard-guard-isolated.sh
stage "iso-doctor-overlay" python3 tests/regression/iso-doctor-overlay.py
stage "doctor-egress" bash tests/regression/container-doctor-egress.sh
stage "doctor-ports" bash tests/regression/container-doctor-ports.sh
stage "bitcoin-pruning" python3 tests/regression/bitcoin-prune-entrypoint.py
stage "lnd-ui-readiness" node --test tests/regression/lnd-ui-readiness.cjs
stage "catalog-drift"    python3 scripts/check-app-catalog-drift.py --release --strict

# Validate the artifact that will actually be signed and published, not only
# the dashboard's list-form source catalog. This catches missing embedded
# manifests, an untrusted image registry, or a storefront promotion that names
# an app absent from the signed registry before the release-root ceremony.
registry_candidate() {
  local candidate rc=0
  candidate=$(mktemp -t archipelago-app-catalog.XXXXXX.json)
  bash scripts/generate-app-catalog.sh "$candidate" || rc=$?
  if [[ $rc -eq 0 ]]; then
    python3 scripts/check-app-catalog-drift.py \
      --catalog "$candidate" --release --strict || rc=$?
  fi
  if [[ $rc -eq 0 ]]; then
    python3 scripts/check-catalog-registry-trust.py \
      --repo "$REPO" --catalog "$candidate" || rc=$?
  fi
  if [[ $rc -eq 0 ]]; then
    python3 - "$candidate" <<'PY' || rc=$?
import json
import sys

catalog = json.load(open(sys.argv[1], encoding="utf-8"))
apps = catalog.get("apps", {})
storefront = catalog.get("storefront", {})
promotions = storefront.get("promotions", [])
if "archipelago-source" not in apps:
    raise SystemExit("registry candidate omits archipelago-source")
source_promotions = [item for item in promotions if item.get("id") == "archipelago-source"]
if not source_promotions:
    raise SystemExit("registry candidate omits the Archipelago source promotion")
expected_path = "/npub1w3sqdkrhn0gyuvsex32effzgnfpyde6qrrc4u467flg5e9txh4wsfn5vjg/relay.ngit.dev/archy"
if source_promotions[0].get("path") != expected_path:
    raise SystemExit("source promotion does not open the canonical Archipelago repository")
print("registry candidate includes GitWorkshop and its source promotion")
PY
  fi
  rm -f -- "$candidate"
  return "$rc"
}
stage "registry-candidate" registry_candidate
# Every release must surface its CHANGELOG entry in the Settings "What's New"
# modal. The modal hardcodes a block per version and has drifted behind before
# (sat at v1.7.84 while the fleet shipped to v1.7.92). Fail if any CHANGELOG
# version is missing a block; `python3 scripts/sync-whats-new.py` inserts them.
stage "whats-new-sync"   python3 scripts/sync-whats-new.py --check
if [[ $MANIFEST -eq 1 ]]; then
  stage "release-manifest" scripts/check-release-manifest.sh
fi

# ── Stage 2: frontend ────────────────────────────────────────────────
stage "ui-type-check"    bash -c 'cd neode-ui && npm run --silent type-check'
# A red vitest run must name the failing test. `| tail -4` used to swallow the
# failure block, so the gate reported only "1 failed" and nothing else —
# undiagnosable after the fact. Quiet 4-line summary on success, full log on
# failure (kept on disk so a scrolled-off terminal is not the end of it).
ui_unit_tests() {
  local log; log=$(mktemp -t vitest-gate.XXXXXX.log)
  if ( cd neode-ui && npx vitest run --silent ) >"$log" 2>&1; then
    tail -4 "$log"; rm -f "$log"; return 0
  fi
  echo "──── vitest FAILED — full log kept at $log ────"
  cat "$log"
  return 1
}
stage "ui-unit-tests"    ui_unit_tests

if [[ $WITH_BUILD -eq 1 ]]; then
  # npm run build can fail silently (vue-tsc EACCES burned us before) —
  # require the packaged output to actually contain the current version.
  VERSION=$(grep -m1 '^version' core/archipelago/Cargo.toml | cut -d'"' -f2)
  stage "ui-build"       bash -c 'cd neode-ui && npm run build'
  stage "ui-dist-version" bash -c "grep -rqo '${VERSION}' web/dist/neode-ui/assets/*.js"
fi

[[ $QUICK -eq 1 ]] && summary 0

# ── Stage 3: backend ─────────────────────────────────────────────────
stage "cargo-check"      timeout 580 cargo check --manifest-path core/Cargo.toml -p archipelago
# Focused suites for the subsystems this release train touched:
#   update::   — OTA download/apply/rollback/probe (v1.7.89 hardening)
#   lnd        — receive address + wallet readiness (v1.7.85–.89), incl. the
#                structured receive-error reason-code classifier
#   container::image_versions — image pinning / false-update detection
#   scanner    — RAII in-flight guard (v1.7.84)
#   drift      — published-port drift detection (the .116 self-heal)
#   missing_secret — secret-resolution names the missing file (the .198 fix)
# TIMEOUT, NOT TEST FAILURE: exit 124 here means the compile ran out of wall
# clock, not that anything failed. The non-incremental test-profile build has
# outgrown every ceiling set for it — 580s expired mid-compile, then on
# 2026-08-20 1500s died at unit 427/429 (the archipelago bin test, the biggest
# link) on a loaded, swapping dev box, again without running a single test.
# 3600s leaves headroom; a warm target/ finishes in a fraction of it.
stage "cargo-test-isolated" timeout 3600 bash scripts/test-backend-isolated.sh

# ── Stage 4: live node smoke ─────────────────────────────────────────
if [[ $LIVE -eq 1 ]]; then
  stage "live-frontend"  bash -c "curl -skf -o /dev/null '$LIVE_URL/' || curl -skf -o /dev/null '${LIVE_URL/http:/https:}/'"
  stage "live-aiui"      curl -sf -o /dev/null "$LIVE_URL/aiui/"
  stage "live-rpc"       bash -c "curl -s -X POST '$LIVE_URL/rpc/v1' -H 'Content-Type: application/json' -d '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"update.status\",\"params\":{}}' | grep -qE '\"(result|error)\"'"

  # Bitcoin-receive regression guard. The backend asks LND REST for a new
  # on-chain address with ?type=<AddressType>. The REST gateway parses that
  # as the proto enum (WITNESS_PUBKEY_HASH / 0), NOT the lncli aliases —
  # sending "p2wkh" returns 400 "parsing field type ... is not a valid
  # value" and bitcoin-receive silently breaks for the whole fleet (the bug
  # that slipped through v1.7.88/89 because nothing exercised LND live).
  # This hits LND REST directly and FAILS only on that exact parse-error
  # signature; a "wallet locked" / "still syncing" reply means the type was
  # accepted, which is all we're validating here.
  stage "live-lnd-address-type" bash -c '
    mac=$(sudo cat /var/lib/archipelago/lnd/data/chain/bitcoin/mainnet/admin.macaroon 2>/dev/null | od -An -tx1 | tr -d " \n")
    for port in 18080 8080; do
      resp=$(curl -sk --max-time 8 "https://127.0.0.1:$port/v1/newaddress?type=WITNESS_PUBKEY_HASH" -H "Grpc-Metadata-macaroon: $mac" 2>/dev/null)
      [ -z "$resp" ] && continue
      echo "LND($port): $resp"
      echo "$resp" | grep -q "is not a valid value" && { echo "FAIL: LND rejected the address type the backend sends"; exit 1; }
      echo "OK: LND accepted the address type"; exit 0
    done
    echo "SKIP: LND REST not reachable on 18080/8080 — cannot validate address type live"; exit 0
  '

  # Wallet-unlock guard. After a restart/OTA, LND comes up LOCKED and the backend
  # must auto-unlock it; if the unlock password is wrong (e.g. a fleet-wide
  # constant vs a per-wallet password) the wallet stays LOCKED forever and ALL
  # Bitcoin-receive / Lightning ops fail — fleet-wide, silently. Nothing else in
  # this harness catches that: live-lnd-address-type explicitly treats "wallet
  # locked" as a PASS, and os-audit treats lnd-unreachable as a WARN. This stage
  # polls LND's unauthenticated /v1/state and FAILS if it is still LOCKED after a
  # grace window. RPC_ACTIVE = unlocked (pass); NON_EXISTING/WAITING = no wallet
  # yet (not a regression); unreachable = skip.
  stage "live-lnd-unlocked" bash -c '
    deadline=$(( $(date +%s) + 60 ))
    while :; do
      seen=""
      for port in 18080 8080; do
        st=$(curl -sk --max-time 6 "https://127.0.0.1:$port/v1/state" 2>/dev/null)
        [ -z "$st" ] && continue
        seen=1
        echo "LND($port) state: $st"
        echo "$st" | grep -qE "UNLOCKED|RPC_ACTIVE|SERVER_ACTIVE" && { echo "OK: LND wallet is unlocked"; exit 0; }
        echo "$st" | grep -qE "NON_EXISTING|WAITING_TO_START" && { echo "OK: LND wallet not initialized yet — not a lock regression"; exit 0; }
      done
      [ -z "$seen" ] && { echo "SKIP: LND /v1/state not reachable on 18080/8080"; exit 0; }
      [ "$(date +%s)" -ge "$deadline" ] && { echo "FAIL: LND wallet still LOCKED after 60s — auto-unlock failed; Bitcoin-receive/Lightning are broken"; exit 1; }
      sleep 5
    done
  '
fi

summary 0
