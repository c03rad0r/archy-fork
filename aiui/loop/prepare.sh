#!/usr/bin/env sh
# Pre-run script: verify repo state and create overnight branch.
# Run before 5-6pm to ensure you can safely start the overnight loop.
set -eu

PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$PROJECT_DIR"

DATE=$(date '+%Y-%m-%d')
BRANCH="overnight/${DATE}"

echo "=== Overnight pre-run check @ $(date '+%Y-%m-%dT%H:%M:%S') ==="

# 1. Check git status is clean
if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "Error: Working tree not clean. Commit or stash changes first." >&2
  echo "  git status" >&2
  git status --short >&2
  exit 1
fi

# 2. Check we're not already on an overnight branch
current=$(git branch --show-current 2>/dev/null || true)
if [ -n "$current" ] && [ "$current" = "$BRANCH" ]; then
  echo "Already on $BRANCH. Ready to run." >&2
  exit 0
fi

# 3. Create date-stamped branch
if git rev-parse --verify "$BRANCH" >/dev/null 2>&1; then
  echo "Branch $BRANCH already exists. Checkout or use a different date." >&2
  exit 1
fi
git checkout -b "$BRANCH"
echo "Created branch $BRANCH"

# 4. Remind to push
echo ""
echo "Reminder: Push before starting overnight run: git push -u origin $BRANCH"
echo "Then run: caffeinate -i ./loop/loop.sh"
echo "=== Ready ==="
