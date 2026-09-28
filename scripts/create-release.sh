#!/usr/bin/env bash
# create-release.sh — Full release automation for Archipelago
#
# Bumps version in Cargo.toml and package.json, generates changelog from git log,
# creates a pending release manifest, and creates git tag. The live manifest is
# promoted only by publish-release-assets.sh after the assets are verified.
#
# Usage:
#   ./scripts/create-release.sh 1.0.0         # Release v1.0.0
#   ./scripts/create-release.sh 1.0.0 --dry-run  # Preview without changes
#
# Releases are tarball-only. ISO builds are archived under
# image-recipe/_archived/. Nodes OTA-update from releases/manifest.json.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DRY_RUN=false
VERSION=""

# Parse args
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=true ;;
        --help|-h)
            echo "Usage: $0 VERSION [--dry-run]"
            echo ""
            echo "Steps performed:"
            echo "  1. Validate version format (SemVer)"
            echo "  2. Bump version in Cargo.toml and package.json"
            echo "  3. Build backend"
            echo "  4. Build frontend"
            echo "  5. Validate the curated changelog"
            echo "  6. Create pending release manifest"
            echo "  7. Commit release preparation"
            echo "  8. Create git tag v{VERSION}"
            echo ""
            echo "Options:"
            echo "  --dry-run    Show what would be done without making changes"
            exit 0
            ;;
        *)
            if [ -z "$VERSION" ]; then
                VERSION="$arg"
            else
                echo "Error: Unknown argument: $arg"
                exit 1
            fi
            ;;
    esac
done

if [ -z "$VERSION" ]; then
    echo "Error: VERSION argument required"
    echo "Usage: $0 VERSION [--dry-run]"
    exit 1
fi

# Validate SemVer format
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$'; then
    echo "Error: Version '$VERSION' is not valid SemVer (expected: X.Y.Z or X.Y.Z-suffix)"
    exit 1
fi

# Check we're on main branch
BRANCH=$(git -C "$PROJECT_ROOT" branch --show-current)
if [ "$BRANCH" != "main" ]; then
    echo "Error: Must be on 'main' branch (currently on '$BRANCH')"
    exit 1
fi

# Check for uncommitted changes
if ! git -C "$PROJECT_ROOT" diff --quiet HEAD; then
    echo "Error: Uncommitted changes detected. Commit or stash first."
    exit 1
fi

# ── Pre-flight test gate ──────────────────────────────────────────────
# A release must not ship if the static/frontend/backend checks fail. This
# runs the release gate harness (cargo fmt/check, catalog drift, vitest, and
# the focused cargo suites — incl. the receive/port-drift/secret regressions).
# Skipped on --dry-run, or set SKIP_RELEASE_TESTS=1 to bypass in an emergency.
# The lifecycle bats harness (tests/lifecycle/run-gate.sh) still runs separately
# against live nodes — see tests/lifecycle/TESTING.md.
if ! $DRY_RUN; then
    if [ "${SKIP_RELEASE_TESTS:-0}" = "1" ]; then
        echo "WARNING: SKIP_RELEASE_TESTS=1 — bypassing the pre-flight test gate"
    elif [ -x "$PROJECT_ROOT/tests/release/run.sh" ]; then
        echo "[0/7] Running release gate (tests/release/run.sh)..."
        if ! "$PROJECT_ROOT/tests/release/run.sh"; then
            echo "Error: release gate failed — aborting release. Fix the failing"
            echo "       stage, or re-run with SKIP_RELEASE_TESTS=1 to override."
            exit 1
        fi
    else
        echo "WARNING: tests/release/run.sh not found/executable — skipping test gate"
    fi
fi

# Check tag doesn't already exist
if git -C "$PROJECT_ROOT" tag -l "v$VERSION" | grep -q "v$VERSION"; then
    echo "Error: Tag v$VERSION already exists"
    exit 1
fi

# Get current version
CURRENT_CARGO_VERSION=$(grep '^version' "$PROJECT_ROOT/core/archipelago/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
CURRENT_NPM_VERSION=$(node -p "require('$PROJECT_ROOT/neode-ui/package.json').version")

echo "=== Archipelago Release v${VERSION} ==="
echo "  Current Cargo version: ${CURRENT_CARGO_VERSION}"
echo "  Current npm version:   ${CURRENT_NPM_VERSION}"
echo "  Target version:        ${VERSION}"
echo "  Dry run:               ${DRY_RUN}"
echo ""

if $DRY_RUN; then
    echo "[DRY RUN] Would perform the following:"
    echo "  0. Run pre-flight test gate (tests/release/run.sh) — aborts on failure"
    echo "  1. Update core/archipelago/Cargo.toml version to $VERSION"
    echo "  2. Update neode-ui/package.json version to $VERSION"
    echo "  3. Build backend (cargo build --release -p archipelago)"
    echo "  4. Build frontend (npm run build)"
    echo "  5. Validate the curated changelog"
    echo "  6. Create pending release manifest (the live manifest stays unchanged)"
    echo "  7. Commit: 'chore: prepare release v${VERSION}'"
    echo "  8. Tag: v${VERSION}"
    echo ""
    echo "After this script, publish only with:"
    echo "  scripts/publish-release-assets.sh ${VERSION} gitea-vps2"
    exit 0
fi

echo "[1/7] Bumping version in Cargo.toml..."
# Update archipelago Cargo.toml
sed -i.bak "s/^version = \".*\"/version = \"$VERSION\"/" "$PROJECT_ROOT/core/archipelago/Cargo.toml"
rm -f "$PROJECT_ROOT/core/archipelago/Cargo.toml.bak"

# Also update workspace Cargo.lock if it exists
if [ -f "$PROJECT_ROOT/core/Cargo.lock" ]; then
    # Cargo will update the lock file on next build; touch the toml to trigger
    true
fi

echo "[2/7] Bumping version in package.json..."
cd "$PROJECT_ROOT/neode-ui"
npm version "$VERSION" --no-git-tag-version --allow-same-version 2>/dev/null || true
cd "$PROJECT_ROOT"

echo "[3/8] Building backend..."
cd "$PROJECT_ROOT/core"
cargo build --release -p archipelago
cd "$PROJECT_ROOT"

echo "[4/8] Building frontend..."
cd "$PROJECT_ROOT/neode-ui"
# `| tail -3` discards npm's exit status, so a FAILED build looked identical
# to a successful one and the run marched on to report the far more
# confusing "build no-opped or its output is stale" a few lines later.
# Keep the quiet 3-line output, but surface the real error when it breaks.
build_log=$(mktemp -t neode-build.XXXXXX.log)
if npm run build >"$build_log" 2>&1; then
    tail -3 "$build_log"; rm -f "$build_log"
else
    echo "Error: the frontend build FAILED — full log kept at $build_log" >&2
    tail -40 "$build_log" >&2
    exit 1
fi
cd "$PROJECT_ROOT"

# Build AIUI from the same source as the release. The checked-in demo bundle
# can predate source fixes and must never overwrite the production payload.
bash "$SCRIPT_DIR/build-aiui.sh"
rm -rf "$PROJECT_ROOT/web/dist/neode-ui/aiui"
cp -r "$PROJECT_ROOT/aiui/packages/app/dist" "$PROJECT_ROOT/web/dist/neode-ui/aiui"

# npm run build can silently no-op (vue-tsc EACCES burned us before) — a stale
# dist would ship with a perfectly valid sha256. Require the freshly built
# bundle to embed the version we just bumped to before it gets packaged.
if ! grep -rqo "${VERSION}" "$PROJECT_ROOT"/web/dist/neode-ui/assets/*.js; then
    echo "Error: web/dist/neode-ui does not contain v${VERSION} — the frontend" >&2
    echo "       build no-opped or its output is stale. Aborting release." >&2
    exit 1
fi

echo "[4b/8] Building packaged radio tools (archy-reticulum-daemon, archy-rnodeconf)..."
# These ride the frontend tarball's runtime payload (radio-tools/) and are
# promoted to /usr/local/bin by bootstrap.rs — the ONLY path that updates them
# on OTA-only nodes. Stale-dist releases re-broke fleet mesh once (v1.7.117),
# so always rebuild here; the manifest script hard-fails if they're missing.
(cd "$PROJECT_ROOT/reticulum-daemon" && ./build.sh) || {
    echo "Error: reticulum-daemon/build.sh failed — radio tools are release-critical" >&2
    exit 1
}

echo "[5/8] Validating curated changelog..."

CHANGELOG_FILE="$PROJECT_ROOT/CHANGELOG.md"
RELEASE_DATE=$(date +%Y-%m-%d)
if [ ! -f "$CHANGELOG_FILE" ] || ! grep -q "^## v${VERSION} (" "$CHANGELOG_FILE"; then
    echo "Error: CHANGELOG.md must already contain curated notes for v${VERSION}."
    echo "Add a section like:"
    echo ""
    echo "## v${VERSION} (${RELEASE_DATE})"
    echo ""
    echo "- User/operator-facing change ..."
    echo "- Another concrete change ..."
    echo "- Validation or operational note ..."
    exit 1
fi

echo "[6/8] Creating pending release manifest..."
# Never write the fleet-visible path here. A normal `git push main` must not be
# capable of advertising assets which have not been uploaded yet.
PENDING_DIR="$PROJECT_ROOT/releases/pending/v${VERSION}"
PENDING_MANIFEST="$PENDING_DIR/manifest.json"
mkdir -p "$PENDING_DIR"
"$SCRIPT_DIR/create-release-manifest.sh" --version "$VERSION" --date "$RELEASE_DATE" --output "$PENDING_MANIFEST" 2>&1 | grep -v "^$"

# §A supply-chain: the OTA manifest must carry the release-root signature.
# Nodes refuse to AUTO-apply unsigned manifests, and publish-release-assets.sh
# hard-refuses to ship one. The mnemonic is read interactively (or from
# RELEASE_MASTER_MNEMONIC) — it must never land in files or shell history.
SIGNER="$PROJECT_ROOT/core/target/release/archipelago"
if [ ! -x "$SIGNER" ]; then
    echo "Error: release binary not found at $SIGNER — cannot sign manifest" >&2
    exit 1
fi
if [ -n "${RELEASE_MASTER_MNEMONIC:-}" ] || [ -t 0 ]; then
    echo "[6b/8] Signing release manifest..."
    # The signer reads stdin to EOF, so Enter alone submits NOTHING and a
    # second paste just appends to the first. That is how a 24-word phrase
    # arrived as "invalid word count: 89" on 2026-08-20 — roughly four
    # pastes concatenated, because nothing appeared to happen after Enter.
    echo "════════════════════════════════════════════════════════════════"
    echo "  Paste the release master mnemonic ONCE, press Enter, then"
    echo "  press Ctrl-D on the new line to submit."
    echo "  Enter by itself will NOT submit; pasting twice concatenates"
    echo "  the phrases and fails on word count."
    echo "════════════════════════════════════════════════════════════════"
    "$SIGNER" ceremony sign "$PENDING_MANIFEST"
    "$SIGNER" ceremony verify "$PENDING_MANIFEST"
else
    echo "⚠ WARNING: no TTY and RELEASE_MASTER_MNEMONIC unset — pending manifest left UNSIGNED."
    echo "  This run will ABORT before committing (step 7 refuses an unsigned manifest)."
    echo "  Sign it, then re-run: bash scripts/sign-manifest.sh $PENDING_MANIFEST"
fi
echo "[6c/8] Staging release artifacts for validation..."
VERSION_DIR="$PROJECT_ROOT/releases/v${VERSION}"
FRONTEND_ARCHIVE="/tmp/archipelago-frontend-${VERSION}.tar.gz"
mkdir -p "$VERSION_DIR"
install -m 0755 "$PROJECT_ROOT/core/target/release/archipelago" "$VERSION_DIR/archipelago"
install -m 0644 "$FRONTEND_ARCHIVE" "$VERSION_DIR/archipelago-frontend-${VERSION}.tar.gz"
"$SCRIPT_DIR/check-release-manifest.sh" "$PENDING_MANIFEST"

# §A supply-chain gate, mirroring publish-release-assets.sh. The pending path
# prevents an ordinary main push from exposing the release, but an unsigned
# manifest is still unpublishable and must never be tagged as ready.
# Release root ROTATED 2026-08-05. v1.7.122-alpha was the last release signed
# with the old root (z6Mkkid…q7ur) — it is the release that installed this
# pin on every node. From v1.7.123 onward the new root signs, and nodes
# running .122+ reject anything signed with the old key.
EXPECTED_DID="did:key:z6Mkfu5LT8d4DjETtrkATvHh9Dvcbnr7zBCUwfau8Sw7DLWT"
if ! grep -q '"signature":' "$PENDING_MANIFEST" \
    || ! grep -q "\"signed_by\": \"$EXPECTED_DID\"" "$PENDING_MANIFEST"; then
    echo "" >&2
    echo "Error: the pending manifest is NOT signed by the release root." >&2
    echo "  Refusing to commit an unpublishable release." >&2
    echo "" >&2
    echo "  Sign it, then re-run this script:" >&2
    echo "    bash scripts/sign-manifest.sh $PENDING_MANIFEST" >&2
    echo "" >&2
    echo "  (Signing needs a TTY for the mnemonic prompt, or RELEASE_MASTER_MNEMONIC set.)" >&2
    exit 1
fi
"$SIGNER" ceremony verify "$PENDING_MANIFEST" \
    || { echo "Error: manifest signature failed cryptographic verification — refusing to commit" >&2; exit 1; }

echo "[7/8] Committing release preparation..."
git -C "$PROJECT_ROOT" add \
    core/archipelago/Cargo.toml \
    core/Cargo.lock \
    neode-ui/package.json \
    neode-ui/package-lock.json \
    neode-ui/public/catalog.json \
    CHANGELOG.md \
    2>/dev/null || true
# releases/** is ignored because binaries live in Gitea attachments; force-add
# only this small signed pending manifest.
git -C "$PROJECT_ROOT" add -f "releases/pending/v${VERSION}/manifest.json"
# Cargo.lock (rewritten by the release build after the version bump) and
# neode-ui/public/catalog.json (regenerated by the frontend build) belong in
# THIS commit: leaving them dirty failed build-iso-release.sh's clean-tree
# preflight on three consecutive releases (.127-.129, 2026-08-09/10).

git -C "$PROJECT_ROOT" commit -m "chore: prepare release v${VERSION}"

echo "[8/8] Creating git tag..."
git -C "$PROJECT_ROOT" tag -a "v${VERSION}" -m "Release v${VERSION}"

echo ""
echo "=== Release v${VERSION} Ready ==="
echo ""
echo "Artifacts:"
echo "  - Version bumped in Cargo.toml and package.json"
echo "  - Changelog updated in CHANGELOG.md"
echo "  - Pending manifest: releases/pending/v${VERSION}/manifest.json"
echo "  - Live manifest: unchanged until assets pass publication verification"
echo "  - Staged artifacts: releases/v${VERSION}/"
echo "  - Git tag: v${VERSION}"
echo ""
echo "Next steps:"
echo "  1. Review: git log --oneline -5"
echo "  2. Publish commits, tag, artifacts, and verify download URLs:"
echo "       scripts/publish-release-assets.sh ${VERSION} gitea-vps2"
echo "  3. Verify manifest is live on both mirrors:"
echo "       curl -fsS http://localhost:3000/lfg2025/archy/raw/branch/main/releases/manifest.json"
echo "       curl -fsS https://source.archipelago-foundation.org/lfg2025/archy/raw/branch/main/releases/manifest.json"
