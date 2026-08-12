#!/usr/bin/env bash
#
# Build the Android companion app and publish it as the served download
# (neode-ui/public/packages/archipelago-companion.apk — a plain APK a phone can
# install straight from the link), then commit + push.
#
# Use this INSTEAD of `git push` when shipping the companion app, so the
# downloadable APK on the node always matches what's on main.
#
#   ./Android/ship-companion.sh
#
# The actual build/sign/verify/stage is done by scripts/publish-companion-apk.sh
# (single source of truth, shared with the pre-push hook). It does a CLEAN build,
# forces v1+v2+v3 signing, and ABORTS if any signature scheme is missing — so a
# broken or v2-only APK can never be shipped.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export JAVA_HOME="${JAVA_HOME:-/opt/homebrew/opt/openjdk@17}"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Library/Android/sdk}"

DEST="neode-ui/public/packages/archipelago-companion.apk"

echo "==> Building + signing + verifying companion APK"
bash scripts/publish-companion-apk.sh

[ -f "$DEST" ] || { echo "ERROR: served APK not found at $DEST" >&2; exit 1; }

if git diff --cached --quiet -- "$DEST"; then
  echo "==> Nothing to commit (APK unchanged)"
else
  git commit -q -m "chore(android): update companion apk download"
  echo "==> Committed"
fi

echo "==> Pushing $(git branch --show-current)"
# SHIP_COMPANION lets the pre-push guard know the APK was just refreshed.
SHIP_COMPANION=1 git push origin "$(git branch --show-current)"
echo "==> Done — companion APK published and pushed."
