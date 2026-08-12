#!/bin/bash
# Validate releases/manifest.json:
#   - version matches core/archipelago/Cargo.toml
#   - changelog contains curated release notes, not raw git log output
#   - every component's download_url exists on disk and matches sha256/size
#
# Run on every push from CI, and also locally before publishing a release:
#   scripts/check-release-manifest.sh
#
# Exits non-zero on any mismatch so the release process fails loud.

set -eo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$REPO_ROOT/releases/manifest.json"

if [ ! -f "$MANIFEST" ]; then
    echo "❌ releases/manifest.json missing"
    exit 1
fi

fail() { echo "❌ $*"; exit 1; }
ok() { echo "✅ $*"; }

MANIFEST_VERSION=$(python3 -c "import json; print(json.load(open('$MANIFEST'))['version'])")
CARGO_VERSION=$(grep '^version' "$REPO_ROOT/core/archipelago/Cargo.toml" | head -1 | sed -E 's/.*"([^"]+)".*/\1/')

if [ "$MANIFEST_VERSION" != "$CARGO_VERSION" ]; then
    fail "manifest version ($MANIFEST_VERSION) ≠ Cargo.toml ($CARGO_VERSION)"
fi
ok "version matches: $MANIFEST_VERSION"

# Release notes mandatory — ships stuff nobody can read otherwise. Require
# curated user/operator-facing notes and reject raw `git log --oneline` output.
NOTES_CHECK=$(python3 - "$MANIFEST" <<'PY'
import json
import re
import sys

manifest = sys.argv[1]
notes = json.load(open(manifest)).get("changelog", [])
if len(notes) < 3:
    print(f"FAIL: changelog has {len(notes)} lines; need at least 3 curated release-note bullets")
    sys.exit(0)

bad = []
for note in notes:
    text = str(note).strip()
    if not text:
        bad.append("empty release-note entry")
    if len(text) < 40:
        bad.append(f"too short: {text!r}")
    if re.match(r"^[0-9a-f]{7,40}\s+", text):
        bad.append(f"raw commit hash entry: {text!r}")
    if re.match(r"^(feat|fix|chore|docs|test|refactor|build|ci|perf)(\([^)]+\))?:\s", text):
        bad.append(f"raw conventional-commit entry: {text!r}")

if bad:
    print("FAIL: release notes must be curated user/operator-facing bullets, not raw git log lines:\n" + "\n".join(bad))
else:
    print(f"OK: changelog has {len(notes)} curated lines")
PY
)
case "$NOTES_CHECK" in
    OK:*) ok "${NOTES_CHECK#OK: }" ;;
    FAIL:*) fail "${NOTES_CHECK#FAIL: }" ;;
    *) fail "unexpected release-note validation output: $NOTES_CHECK" ;;
esac

# Each component: the artifact on disk under releases/v<version>/ must match
# the declared sha256 and size_bytes.
VERSION_DIR="$REPO_ROOT/releases/v${MANIFEST_VERSION}"
if [ ! -d "$VERSION_DIR" ]; then
    fail "releases/v${MANIFEST_VERSION}/ missing — artifacts not staged"
fi

COMPONENT_COUNT=$(python3 -c "import json; print(len(json.load(open('$MANIFEST'))['components']))")
for i in $(seq 0 $((COMPONENT_COUNT - 1))); do
    NAME=$(python3 -c "import json; print(json.load(open('$MANIFEST'))['components'][$i]['name'])")
    DECLARED_SHA=$(python3 -c "import json; print(json.load(open('$MANIFEST'))['components'][$i]['sha256'])")
    DECLARED_SIZE=$(python3 -c "import json; print(json.load(open('$MANIFEST'))['components'][$i]['size_bytes'])")

    # Component names other than exactly "archipelago" are the tarball's
    # filename; use as-is. The bare "archipelago" component maps to the
    # binary file literally named `archipelago`.
    FILE="$VERSION_DIR/$NAME"
    if [ "$NAME" = "archipelago" ]; then
        FILE="$VERSION_DIR/archipelago"
    fi

    if [ ! -f "$FILE" ]; then
        fail "component '$NAME' file missing at $FILE"
    fi

    ACTUAL_SHA=$(sha256sum "$FILE" | awk '{print $1}')
    ACTUAL_SIZE=$(stat -c%s "$FILE")

    if [ "$ACTUAL_SHA" != "$DECLARED_SHA" ]; then
        fail "component '$NAME' sha256 mismatch (declared=$DECLARED_SHA actual=$ACTUAL_SHA)"
    fi
    if [ "$ACTUAL_SIZE" != "$DECLARED_SIZE" ]; then
        fail "component '$NAME' size mismatch (declared=$DECLARED_SIZE actual=$ACTUAL_SIZE)"
    fi
    ok "component '$NAME': sha256 + size match on-disk artifact"
done

echo
ok "releases/manifest.json passes all checks — safe to publish v${MANIFEST_VERSION}"
