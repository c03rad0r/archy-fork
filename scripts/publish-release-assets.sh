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

fail() { echo "Error: $*" >&2; exit 1; }

[ -f "$PROJECT_ROOT/releases/manifest.json" ] || fail "releases/manifest.json missing"
[ -f "$BACKEND" ] || fail "backend artifact missing: $BACKEND"
[ -f "$FRONTEND" ] || fail "frontend artifact missing: $FRONTEND"

"$SCRIPT_DIR/check-release-manifest.sh"

# §A supply-chain gate: never publish an unsigned OTA manifest. Fleet nodes
# with the pinned release-root anchor refuse to auto-apply unsigned manifests,
# and enforcement will tighten to hard-reject — an unsigned publish would
# strand them. Grep proves presence; ceremony verify proves the crypto.
# Release root ROTATED 2026-08-05; see create-release.sh. New root from
# v1.7.123 onward.
EXPECTED_DID="did:key:z6Mkfu5LT8d4DjETtrkATvHh9Dvcbnr7zBCUwfau8Sw7DLWT"
grep -q '"signature":' "$PROJECT_ROOT/releases/manifest.json" \
    && grep -q "\"signed_by\": \"$EXPECTED_DID\"" "$PROJECT_ROOT/releases/manifest.json" \
    || fail "releases/manifest.json is not signed by the release root — run: bash scripts/sign-manifest.sh"
if [ -x "$PROJECT_ROOT/core/target/release/archipelago" ]; then
    "$PROJECT_ROOT/core/target/release/archipelago" ceremony verify "$PROJECT_ROOT/releases/manifest.json" \
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
upload_asset() {
    local path="$1"
    local name="$2"
    if printf '%s\n' "$asset_names" | grep -Fxq "$name"; then
        echo "Asset $name already exists; leaving it in place."
        return
    fi
    echo "Uploading $name..."
    curl --fail --show-error --silent --http1.1 --connect-timeout 20 --max-time 900 \
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
"$PROJECT_ROOT/scripts/check-release-assets.sh" "$PROJECT_ROOT/releases/manifest.json" \
    || fail "asset verification failed — NOT pushing main. The manifest stays off the branch nodes read, so no node sees a version it cannot fetch. Repair the assets and re-run."

# Assets are proven fetchable — only now does the manifest become live.
echo "Assets verified. Pushing main to $REMOTE (this makes v${VERSION} live)..."
git -C "$PROJECT_ROOT" push "$REMOTE" main

echo "Release v${VERSION} published and verified on $REMOTE."
