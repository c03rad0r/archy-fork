#!/usr/bin/env bash
# deploy-guard-same-host.sh — regression pin for assert_safe_same_host_deploy
# (scripts/lib/common.sh), widened in 13-09 from containment-only to any
# resolved-path mismatch on the same host.
#
# The 2026-07-31 incident: a same-host `rsync --delete` deploy whose source
# was INSIDE the destination mirrored the source onto the destination and
# deleted ~1810 tracked files, a running dev server, and two sessions'
# uncommitted work. The original fix refused only containment (source-in-
# destination or destination-in-source). It missed SIBLING directories that
# share a parent but neither contains the other — e.g. this session's own
# worktree topology, archy-phase13 (source) vs archy (the main checkout,
# TARGET_DIR's resolved symlink target) — which is the identical rsync
# --delete hazard through a shape the old two-case guard let through.
#
# No SSH, no rsync, no real deploy — pure fixture strings against the
# function. Usage: ./deploy-guard-same-host.sh   (takes no host argument)
# Exit 0 = all assertions pass.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$(dirname "$SCRIPT_DIR")")"

# shellcheck source=../../scripts/lib/common.sh
source "$PROJECT_DIR/scripts/lib/common.sh"

PASS=0; FAIL=0
say() { printf '%s\n' "$*"; }
ok()  { PASS=$((PASS+1)); say "  PASS: $1"; }
bad() { FAIL=$((FAIL+1)); say "  FAIL: $1"; }

# Helper: run assert_safe_same_host_deploy and check its exit code against
# an expectation ("allow" or "refuse"), silencing its stderr message so the
# test output stays readable.
check() {
  local desc="$1" src="$2" dst="$3" expect="$4"
  local rc
  assert_safe_same_host_deploy "$src" "$dst" >/dev/null 2>&1
  rc=$?
  if [ "$expect" = "allow" ]; then
    [ "$rc" -eq 0 ] && ok "$desc" || bad "$desc (expected allow/exit 0, got exit $rc)"
  else
    [ "$rc" -ne 0 ] && ok "$desc" || bad "$desc (expected refuse/non-zero, got exit $rc)"
  fi
}

say "== assert_safe_same_host_deploy — fixture matrix =="

# 1) Identical resolved source and destination: allowed. The normal
#    in-place deploy from the main checkout onto its own symlinked
#    destination.
check "identical resolved paths are allowed" \
  "/home/archipelago/Projects/archy" \
  "/home/archipelago/Projects/archy" \
  "allow"

# 2) Source is inside (a subdirectory of) the destination: refused. The
#    original 2026-07-31 containment case.
check "source-inside-destination is refused" \
  "/home/archipelago/Projects/archy/.claude/worktrees/some-agent" \
  "/home/archipelago/Projects/archy" \
  "refuse"

# 3) Destination is inside the source: refused. The mirror-image
#    containment case.
check "destination-inside-source is refused" \
  "/home/archipelago/Projects/archy" \
  "/home/archipelago/Projects/archy/.claude/worktrees/some-agent" \
  "refuse"

# 4) Sibling-directory regression pin — the exact shape this session's own
#    worktree topology exhibits, and the gap the old two-case guard let
#    through: archy-phase13 (this worktree) as source, archy (the main
#    checkout, TARGET_DIR's resolved symlink target) as destination. Share
#    a parent (/home/archipelago/Projects); neither contains the other.
check "sibling directories (archy-phase13 vs archy) are refused [SIBLING REGRESSION PIN]" \
  "/home/archipelago/Projects/archy-phase13" \
  "/home/archipelago/Projects/archy" \
  "refuse"

# 5) Two completely unrelated same-host paths with no shared parent at
#    all: refused. Same-host plus any mismatch is refused, not just the
#    two containment shapes.
check "unrelated paths with no shared parent are refused" \
  "/home/archipelago/Projects/archy" \
  "/opt/archipelago/web-ui" \
  "refuse"

# 6) A refused case names both resolved paths and the 2026-07-31 incident
#    on stderr, so a future operator understands why rather than
#    reflexively retrying with a force flag.
MSG="$(assert_safe_same_host_deploy "/home/archipelago/Projects/archy-phase13" "/home/archipelago/Projects/archy" 2>&1 >/dev/null)"
if echo "$MSG" | grep -q '/home/archipelago/Projects/archy-phase13' \
  && echo "$MSG" | grep -q '/home/archipelago/Projects/archy' \
  && echo "$MSG" | grep -q '2026-07-31'; then
  ok "refusal message names both resolved paths and the 2026-07-31 incident"
else
  bad "refusal message missing an expected component: $MSG"
fi

say ""
say "== ${PASS} passed, ${FAIL} failed =="
[ "$FAIL" -eq 0 ]
