#!/usr/bin/env bash
# Publish an Archipelago OTA release to a Gitea remote and verify downloads.

set -euo pipefail

VERSION="${1:-}"
REMOTE="${2:-gitea-vps2}"

if [ -z "$VERSION" ]; then
    echo "Usage: $0 VERSION [remote]"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSION_DIR="$PROJECT_ROOT/releases/v${VERSION}"
BACKEND="$VERSION_DIR/archipelago"
FRONTEND="$VERSION_DIR/archipelago-frontend-${VERSION}.tar.gz"
PENDING_MANIFEST="$PROJECT_ROOT/releases/pending/v${VERSION}/manifest.json"
LIVE_MANIFEST="$PROJECT_ROOT/releases/manifest.json"
if [ -f "$PENDING_MANIFEST" ]; then
    MANIFEST="$PENDING_MANIFEST"
    PROMOTE_MANIFEST=1
else
    # Backward compatibility for releases prepared before pending manifests.
    MANIFEST="$LIVE_MANIFEST"
    PROMOTE_MANIFEST=0
fi

fail() { echo "Error: $*" >&2; exit 1; }

[ -f "$MANIFEST" ] || fail "release manifest missing: $MANIFEST"
MANIFEST_VERSION=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$MANIFEST")
[ "$MANIFEST_VERSION" = "$VERSION" ] || fail "requested v$VERSION but $MANIFEST describes v$MANIFEST_VERSION"
[ -f "$BACKEND" ] || fail "backend artifact missing: $BACKEND"
[ -f "$FRONTEND" ] || fail "frontend artifact missing: $FRONTEND"

"$SCRIPT_DIR/check-release-manifest.sh" "$MANIFEST"

# §A supply-chain gate: never publish an unsigned OTA manifest. Fleet nodes
# with the pinned release-root anchor refuse to auto-apply unsigned manifests,
# and enforcement will tighten to hard-reject — an unsigned publish would
# strand them. Grep proves presence; ceremony verify proves the crypto.
# Release root ROTATED 2026-08-05; see create-release.sh. New root from
# v1.7.123 onward.
EXPECTED_DID="did:key:z6Mkfu5LT8d4DjETtrkATvHh9Dvcbnr7zBCUwfau8Sw7DLWT"
grep -q '"signature":' "$MANIFEST" \
    && grep -q "\"signed_by\": \"$EXPECTED_DID\"" "$MANIFEST" \
    || fail "$MANIFEST is not signed by the release root — run: bash scripts/sign-manifest.sh $MANIFEST"
if [ -x "$PROJECT_ROOT/core/target/release/archipelago" ]; then
    "$PROJECT_ROOT/core/target/release/archipelago" ceremony verify "$MANIFEST" \
        || fail "manifest signature failed cryptographic verification"
fi

remote_url=$(git -C "$PROJECT_ROOT" remote get-url "$REMOTE")
# https is accepted as well as http. Requiring http:// meant the only remote
# whose credential actually works for git push (the https one) was rejected,
# while the http remote it forced you to use had a dead token — so publishing
# failed on auth after the manifest had already passed every check
# (v1.7.121-alpha, 2026-08-04). The scheme is carried through to the API URL
# rather than assumed.
case "$remote_url" in
    http://*@*|https://*@*) ;;
    *) fail "$REMOTE must be an authenticated http(s):// Gitea remote URL for API uploads" ;;
esac

scheme=${remote_url%%://*}
rest=${remote_url#*://}
auth=${rest%%@*}
host_path=${rest#*@}
host=${host_path%%/*}
repo_path=${host_path#*/}
repo_path=${repo_path%.git}
api="$scheme://$host/api/v1/repos/$repo_path"
release_url="$api/releases/tags/v${VERSION}"

# ORDER MATTERS. The manifest is the trigger — nodes read releases/manifest.json
# from branch main and try to download the named version the moment it appears.
# So main (which carries the live manifest) must be pushed LAST, only after the
# assets are uploaded and their bytes verified against the manifest. The tag is
# pushed first because the Gitea release and its asset download URLs hang off it,
# but the tag alone changes nothing for nodes.
#
# This used to push main and the tag together, up front, then upload assets. That
# left the manifest live for the entire upload+verify window — and on 2026-08-07
# an upload failed inside that window, so every polling node briefly advertised a
# v1.7.126-alpha update whose binary 500'd and whose tarball did not exist.
echo "Pushing tag v${VERSION} to $REMOTE (not main yet)..."
git -C "$PROJECT_ROOT" push "$REMOTE" "refs/tags/v${VERSION}"

release_json=$(curl -fsS -u "$auth" "$release_url" || true)
if [ -z "$release_json" ]; then
    echo "Creating Gitea release v${VERSION}..."
    release_body=$(python3 - "$VERSION" <<'PY'
import json
import sys

version = sys.argv[1]
print(json.dumps({
    "tag_name": f"v{version}",
    "target_commitish": "main",
    "name": f"v{version}",
    "body": f"Archipelago v{version} release artifacts for OTA updates.",
    "draft": False,
    "prerelease": True,
}))
PY
)
    release_json=$(curl -fsS -u "$auth" -H 'Content-Type: application/json' -d "$release_body" "$api/releases")
fi

release_id=$(printf '%s' "$release_json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')

asset_names=$(curl -fsS -u "$auth" "$api/releases/$release_id/assets" | python3 -c 'import json,sys; print("\n".join(a["name"] for a in json.load(sys.stdin)))')
# upload_asset <path> <name> [max_seconds]
# The 900s default is ample for the ~98MB frontend tarball but nowhere near
# enough for a multi-GB ISO, which also deserves a visible progress bar
# rather than sitting mute for the better part of an hour.
upload_asset() {
    local path="$1"
    local name="$2"
    local max_time="${3:-900}"
    if printf '%s\n' "$asset_names" | grep -Fxq "$name"; then
        echo "Asset $name already exists; leaving it in place."
        return
    fi
    local noise=(--silent)
    if [ "$max_time" -gt 900 ]; then noise=(--progress-bar); fi
    echo "Uploading $name ($(du -h "$path" | cut -f1))..."
    curl --fail --show-error "${noise[@]}" --http1.1 --connect-timeout 20 --max-time "$max_time" \
        -u "$auth" \
        -F "attachment=@$path" \
        "$api/releases/$release_id/assets?name=$name" >/dev/null
    asset_names=$(printf '%s\n%s\n' "$asset_names" "$name")
}

upload_asset "$BACKEND" "archipelago"
upload_asset "$FRONTEND" "archipelago-frontend-${VERSION}.tar.gz"

echo "Verifying public download URLs (full GET + size + sha256)..."
# Delegated to check-release-assets.sh so the same verifier is used here and by
# hand during recovery. It fails hard on the first bad asset — the previous
# inline `while read` ran in a pipe subshell, where a `fail` (exit) killed only
# the subshell and let this script march on to "published and verified".
"$PROJECT_ROOT/scripts/check-release-assets.sh" "$MANIFEST" \
    || fail "asset verification failed — NOT pushing main. The manifest stays off the branch nodes read, so no node sees a version it cannot fetch. Repair the assets and re-run."

# Assets are proven fetchable — only now may the manifest become live. First
# incorporate concurrent work, then promote in a dedicated commit. Until the
# final push succeeds the remote still serves the previous manifest.
echo "Assets verified. Synchronizing main before manifest promotion..."
git -C "$PROJECT_ROOT" fetch "$REMOTE" main
git -C "$PROJECT_ROOT" merge --no-edit "$REMOTE/main"

if [ "$PROMOTE_MANIFEST" = "1" ]; then
    cp "$MANIFEST" "$LIVE_MANIFEST"
    cp "$MANIFEST" "$PROJECT_ROOT/release-manifest.json"
    git -C "$PROJECT_ROOT" add releases/manifest.json release-manifest.json
    git -C "$PROJECT_ROOT" rm -f -- "releases/pending/v${VERSION}/manifest.json"
    git -C "$PROJECT_ROOT" commit -m "chore: publish release v${VERSION}"
fi

echo "Publishing verified manifest to main (this makes v${VERSION} live)..."
# A concurrent push can race the fetch above. Merge and retry without ever
# force-pushing; the remote remains on its old, working manifest meanwhile.
for attempt in 1 2 3; do
    if git -C "$PROJECT_ROOT" push "$REMOTE" HEAD:main; then
        break
    fi
    [ "$attempt" -lt 3 ] || fail "main advanced repeatedly; assets are safe but manifest was not promoted"
    echo "main advanced during publication; merging and retrying..."
    git -C "$PROJECT_ROOT" fetch "$REMOTE" main
    git -C "$PROJECT_ROOT" merge --no-edit "$REMOTE/main"
done

echo "Release v${VERSION} published and verified on $REMOTE."

# ── ISO publication (optional) ───────────────────────────────────────
# Deliberately AFTER main is pushed. The ISO is not referenced by
# releases/manifest.json, so no node's OTA path depends on it — running it
# last means a slow or failed multi-GB upload can never delay, or strand,
# an OTA release that has already been verified.
#
# Skipped cleanly when this version has no ISO yet: create-release.sh runs
# long before scripts/build-iso-release.sh, which needs the very tag this
# script pushes. Re-run this script after building the ISO to attach it.
# SKIP_ISO=1 bypasses the stage entirely.
if [ "${SKIP_ISO:-0}" = "1" ]; then
    echo "SKIP_ISO=1 — not publishing an ISO."
    exit 0
fi

ISO=$(ls -t "$PROJECT_ROOT"/image-recipe/results/archipelago-installer-"$VERSION"*-x86_64_RC*.iso 2>/dev/null | head -1 || true)
if [ -z "$ISO" ]; then
    echo
    echo "No ISO built for v${VERSION} — OTA published without one."
    echo "  Build it:   bash scripts/build-iso-release.sh"
    echo "  Sign it:    bash scripts/sign-iso-checksums.sh <iso>"
    echo "  Attach it:  bash scripts/publish-release-assets.sh $VERSION $REMOTE"
    exit 0
fi

echo
echo "Publishing ISO: $(basename "$ISO")"
ISO_SHA_FILE="$ISO.sha256"
ISO_SIG_FILE="$ISO.sha256.json"
[ -f "$ISO_SHA_FILE" ] || fail "missing $(basename "$ISO_SHA_FILE") — re-run scripts/build-iso-release.sh"
[ -f "$ISO_SIG_FILE" ] || fail "the ISO checksum is unsigned. Run: bash scripts/sign-iso-checksums.sh $ISO"

# Same supply-chain rule as the OTA manifest: anything published must be
# signed by the pinned release root, and the crypto must actually verify —
# a present-but-bogus signature is the failure mode worth catching.
grep -q '"signature":' "$ISO_SIG_FILE" \
    && grep -q "\"signed_by\": \"$EXPECTED_DID\"" "$ISO_SIG_FILE" \
    || fail "$(basename "$ISO_SIG_FILE") is not signed by the release root — run: bash scripts/sign-iso-checksums.sh $ISO"
if [ -x "$PROJECT_ROOT/core/target/release/archipelago" ]; then
    "$PROJECT_ROOT/core/target/release/archipelago" ceremony verify "$ISO_SIG_FILE" \
        || fail "the ISO checksum signature failed cryptographic verification"
fi

# Never upload an image that no longer matches its own checksum. A truncated
# or half-copied ISO is exactly what a signed checksum exists to expose, and
# catching it here is far cheaper than on someone's flashed USB stick.
echo "Checking the ISO against its recorded sha256 (reads the whole image)..."
(cd "$(dirname "$ISO")" && sha256sum --check --status "$(basename "$ISO_SHA_FILE")") \
    || fail "$(basename "$ISO") does not match its .sha256 — rebuild it; do not publish this image"

ISO_NAME=$(basename "$ISO")
# 4h ceiling: a multi-GB image over a domestic uplink is not a 15-minute job.
upload_asset "$ISO"          "$ISO_NAME"             14400
upload_asset "$ISO_SHA_FILE" "$ISO_NAME.sha256"
upload_asset "$ISO_SIG_FILE" "$ISO_NAME.sha256.json"

# Verify what actually landed. Re-downloading a multi-GB ISO would cost far
# more than it proves — the signed .sha256.json already lets anyone verify
# the bytes independently — so confirm each asset exists and that Gitea's
# stored size matches the local file exactly.
echo "Verifying uploaded ISO assets..."
assets_json=$(curl -fsS -u "$auth" "$api/releases/$release_id/assets")
python3 - "$assets_json" \
    "$ISO_NAME" "$(stat -c%s "$ISO")" \
    "$ISO_NAME.sha256" "$(stat -c%s "$ISO_SHA_FILE")" \
    "$ISO_NAME.sha256.json" "$(stat -c%s "$ISO_SIG_FILE")" <<'PY' \
    || fail "ISO asset verification failed — the release is missing or has a truncated ISO"
import json
import sys

assets = {a["name"]: a for a in json.loads(sys.argv[1])}
args = sys.argv[2:]
bad = []
for name, size in zip(args[0::2], args[1::2]):
    asset = assets.get(name)
    if asset is None:
        bad.append(f"{name}: missing from the release")
    elif int(asset["size"]) != int(size):
        bad.append(f"{name}: uploaded {asset['size']} bytes, local file is {size}")
    else:
        print(f"  OK {name} ({asset['size']} bytes)")
for b in bad:
    print("  FAIL " + b, file=sys.stderr)
sys.exit(1 if bad else 0)
PY

echo "ISO for v${VERSION} published and verified on $REMOTE."
