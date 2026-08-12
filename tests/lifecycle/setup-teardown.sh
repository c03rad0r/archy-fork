#!/usr/bin/env bash
# tests/lifecycle/setup-teardown.sh
#
# Cleanup helper used between lifecycle test iterations. Run before AND after
# a full bats pass (run-gate.sh handles this). Idempotent — safe to run any
# time, on any host.
#
# Removes:
#   - /tmp/archy-rpc-session-* — stale RPC session cookies from earlier runs.
#     If absent we'd reuse a session that was logged out by an auth.logout
#     test, then the next iteration would silently 401.
#   - Per-bats-run scratch files our tests may leave behind (none yet, but
#     this is the place when we add them).
#
# Does NOT touch:
#   - Real archipelago state (state.json, secrets, packages.json).
#   - Running containers — destructive container teardown is the test's
#     responsibility, not the harness's. We only clean the harness's own
#     transient state.
#   - SSH known_hosts, archipelago configs, etc.

set -euo pipefail

cleaned=0

# Match the pattern from lib/rpc.bash:26
session_glob="/tmp/archy-rpc-session-*"
# shellcheck disable=SC2086 # we want word-splitting on the glob
for f in $session_glob; do
  if [[ -f "$f" ]]; then
    rm -f "$f"
    cleaned=$((cleaned + 1))
  fi
done

if [[ "${ARCHY_TEARDOWN_VERBOSE:-0}" == "1" ]]; then
  echo "setup-teardown: cleaned $cleaned stale session file(s)"
fi
