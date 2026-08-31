#!/usr/bin/env bash
# create-release-manifest.sh — Build a release manifest for the Archipelago update system.
#
# Generates a JSON manifest with version info, changelog, and SHA256 hashes for
# each component, matching the format expected by core/archipelago/src/update.rs.
#
# Usage:
#   ./scripts/create-release-manifest.sh --version 0.2.0 --date 2026-04-01
#
# The script reads built artifacts from the build output directories and produces
# a manifest.json file suitable for hosting at the UPDATE_MANIFEST_URL.

set -euo pipefail

# Defaults
VERSION=""
RELEASE_DATE=""
OUTPUT_FILE="manifest.json"
BACKEND_BINARY=""
FRONTEND_ARCHIVE=""
BASE_URL="https://source.archipelago-foundation.org/lfg2025/archy/releases/download"

usage() {
    echo "Usage: $0 --version VERSION [--date DATE] [--output FILE]"
    echo ""
    echo "Options:"
    echo "  --version VERSION    Release version (e.g., 0.2.0) [required]"
    echo "  --date DATE          Release date (YYYY-MM-DD) [default: today]"
    echo "  --output FILE        Output manifest path [default: manifest.json]"
    echo "  --backend PATH       Path to backend binary [default: auto-detect]"
    echo "  --frontend PATH      Path to frontend archive [default: auto-detect]"
    echo "  --base-url URL       Base download URL [default: Gitea release attachments]"
    exit 1
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --version) VERSION="$2"; shift 2 ;;
        --date) RELEASE_DATE="$2"; shift 2 ;;
        --output) OUTPUT_FILE="$2"; shift 2 ;;
        --backend) BACKEND_BINARY="$2"; shift 2 ;;
        --frontend) FRONTEND_ARCHIVE="$2"; shift 2 ;;
        --base-url) BASE_URL="$2"; shift 2 ;;
        -h|--help) usage ;;
        *) echo "Unknown option: $1"; usage ;;
    esac
done

if [ -z "$VERSION" ]; then
    echo "Error: --version is required"
    usage
fi

if [ -z "$RELEASE_DATE" ]; then
    RELEASE_DATE=$(date +%Y-%m-%d)
fi

# Find project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Auto-detect backend binary
if [ -z "$BACKEND_BINARY" ]; then
    BACKEND_BINARY="$PROJECT_ROOT/core/target/release/archipelago"
fi

# Auto-detect frontend archive.
# Layout: flat tarball (`./index.html`, `./assets/…`, `./aiui/…`) so the
# Rust updater can unpack it directly into /opt/archipelago/web-ui/.
# Using `-C web/dist neode-ui` would produce a `neode-ui/` prefix which
# breaks the installer and returns 403 on every fleet UI — see
# feedback_release_tarball_layout.md.
if [ -z "$FRONTEND_ARCHIVE" ]; then
    FRONTEND_DIST="$PROJECT_ROOT/web/dist/neode-ui"
    if [ -d "$FRONTEND_DIST" ]; then
        FRONTEND_ARCHIVE="/tmp/archipelago-frontend-${VERSION}.tar.gz"
        STAGING_DIR=$(mktemp -d -t archipelago-frontend.XXXXXX)
        echo "Staging frontend archive in $STAGING_DIR..."
        cp -r "$FRONTEND_DIST/." "$STAGING_DIR/"
        # Bake AIUI in so fresh installs pick it up. OTA already
        # carries-forward the existing aiui/ if the tarball lacks one
        # (update.rs:922), but including it here makes the tarball
        # the single source of truth instead of relying on a side-
        # effect of the in-place swap.
        if [ -d "$PROJECT_ROOT/demo/aiui" ] && [ -f "$PROJECT_ROOT/demo/aiui/index.html" ]; then
            echo "  Including AIUI from demo/aiui/"
            cp -r "$PROJECT_ROOT/demo/aiui" "$STAGING_DIR/aiui"
        fi
        # OTA bridge for nodes running older updaters: they only know how to
        # apply the backend binary and frontend archive. Carry host runtime
        # assets inside the frontend tarball; the new backend promotes them
        # from /opt/archipelago/web-ui/archipelago-runtime on first startup.
        RUNTIME_DIR="$STAGING_DIR/archipelago-runtime"
        mkdir -p "$RUNTIME_DIR"
        for runtime_path in apps scripts docker; do
            if [ -d "$PROJECT_ROOT/$runtime_path" ]; then
                echo "  Including runtime $runtime_path/"
                cp -r "$PROJECT_ROOT/$runtime_path" "$RUNTIME_DIR/$runtime_path"
            fi
        done
        # KEEP IN SYNC with the `for unit in [...]` array in
        # core/archipelago/src/bootstrap.rs (run_runtime_assets). A unit that
        # bootstrap installs but this list does not ship simply never reaches a
        # node: bootstrap looks for it in the runtime payload, does not find
        # it, and silently installs nothing. There is no error to notice.
        mkdir -p "$RUNTIME_DIR/image-recipe/configs"
        for unit in archipelago-doctor.service archipelago-doctor.timer \
                    archipelago-host-secrets-audit.service; do
            if [ -f "$PROJECT_ROOT/image-recipe/configs/$unit" ]; then
                echo "  Including runtime unit $unit"
                cp "$PROJECT_ROOT/image-recipe/configs/$unit" "$RUNTIME_DIR/image-recipe/configs/$unit"
            fi
        done
        if [ -f "$PROJECT_ROOT/image-recipe/configs/nginx-archipelago.conf" ]; then
            mkdir -p "$RUNTIME_DIR/image-recipe/configs"
            echo "  Including runtime nginx-archipelago.conf"
            cp "$PROJECT_ROOT/image-recipe/configs/nginx-archipelago.conf" \
                "$RUNTIME_DIR/image-recipe/configs/nginx-archipelago.conf"
        fi
        # Packaged radio tools ride the runtime payload: OTA-only nodes never
        # get them any other way — the v1.7.117 rollout left fleet nodes with
        # a stale archy-reticulum-daemon (exits on the new --enable-transport
        # flag → mesh dead) and no archy-rnodeconf at all (Flash LoRa fails
        # with "No such file or directory"). bootstrap.rs promotes these to
        # /usr/local/bin on first startup after the update.
        for tool in archy-reticulum-daemon archy-rnodeconf; do
            if [ -f "$PROJECT_ROOT/reticulum-daemon/dist/$tool" ]; then
                mkdir -p "$RUNTIME_DIR/radio-tools"
                echo "  Including radio tool $tool"
                cp "$PROJECT_ROOT/reticulum-daemon/dist/$tool" "$RUNTIME_DIR/radio-tools/$tool"
            else
                echo "  ERROR: reticulum-daemon/dist/$tool missing — run reticulum-daemon/build.sh first" >&2
                rm -rf "$STAGING_DIR"
                exit 1
            fi
        done
        rm -rf "$RUNTIME_DIR/scripts/resilience/reports"
        find "$RUNTIME_DIR" -type d -name '__pycache__' -prune -exec rm -rf {} +
        find "$RUNTIME_DIR" -type f \( -name '*.bak' -o -name '*.bak-*' -o -name '._*' -o -name '*.log' -o -name '*.pyc' \) -delete
        # Force world-readable perms on every entry BEFORE tar, so the
        # archive's internal mode bits are 755/644 regardless of what
        # the staging dir's umask gave us. Without this, mktemp -d
        # creates the staging dir at 700, that 700 gets baked into the
        # tarball's root `./` entry, and every node that extracts the
        # archive ends up with /opt/archipelago/web-ui at 700 — which
        # causes nginx (www-data) to return 500 "permission denied" on
        # every page. Bit us on the v1.7.38 + v1.7.39 rollouts.
        chmod 755 "$STAGING_DIR"
        find "$STAGING_DIR" -type d -exec chmod 755 {} +
        find "$STAGING_DIR" -type f -exec chmod 644 {} +
        echo "Creating frontend archive $FRONTEND_ARCHIVE..."
        # --mode is a belt-and-braces in case a file's on-disk perms
        # drift again; forces 755 dir / 644 file in the archive too.
        tar --owner=0 --group=0 \
            --mode='u=rwX,go=rX' \
            -czf "$FRONTEND_ARCHIVE" \
            -C "$STAGING_DIR" .
        # Verify the archive root entry is world-readable before we
        # declare success — catches regressions in tar-flag handling
        # (BSD tar, busybox tar) that might silently drop --mode.
        # SIGPIPE-safe: use awk to read only the first line and exit,
        # then terminate the tar pipeline explicitly so `pipefail`+SIGPIPE
        # don't kill the whole `set -euo pipefail` script.
        root_mode=$({ tar tvzf "$FRONTEND_ARCHIVE" 2>/dev/null || true; } | awk 'NR==1{print $1; exit}')
        case "$root_mode" in
            drwxr-xr-x|drwxr-x*x*)
                echo "  Tarball root perms OK: $root_mode"
                ;;
            *)
                echo "  ERROR: tarball root perms are $root_mode (want drwxr-xr-x) — aborting release"
                rm -f "$FRONTEND_ARCHIVE"
                rm -rf "$STAGING_DIR"
                exit 1
                ;;
        esac
        rm -rf "$STAGING_DIR"
    fi
fi

# Compute SHA256 hash
sha256_of() {
    if command -v sha256sum &>/dev/null; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

# File size in bytes
size_of() {
    if [[ "$(uname)" == "Darwin" ]]; then
        stat -f%z "$1"
    else
        stat -c%s "$1"
    fi
}

# Get current version from Cargo.toml
CURRENT_VERSION=$(grep '^version' "$PROJECT_ROOT/core/archipelago/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')

echo "Building release manifest v${VERSION}"
echo "  Current version: ${CURRENT_VERSION}"
echo "  Release date: ${RELEASE_DATE}"
echo "  Output: ${OUTPUT_FILE}"

# Build components array
COMPONENTS="[]"

if [ -f "$BACKEND_BINARY" ]; then
    HASH=$(sha256_of "$BACKEND_BINARY")
    SIZE=$(size_of "$BACKEND_BINARY")
    echo "  Backend binary: ${BACKEND_BINARY} (${SIZE} bytes, sha256: ${HASH})"
    COMPONENTS=$(echo "$COMPONENTS" | python3 -c "
import sys, json
c = json.load(sys.stdin)
c.append({
    'name': 'archipelago',
    'current_version': '$CURRENT_VERSION',
    'new_version': '$VERSION',
    'download_url': '$BASE_URL/v$VERSION/archipelago',
    'sha256': '$HASH',
    'size_bytes': $SIZE
})
print(json.dumps(c))
")
else
    echo "  Warning: Backend binary not found at $BACKEND_BINARY"
fi

if [ -n "$FRONTEND_ARCHIVE" ] && [ -f "$FRONTEND_ARCHIVE" ]; then
    HASH=$(sha256_of "$FRONTEND_ARCHIVE")
    SIZE=$(size_of "$FRONTEND_ARCHIVE")
    ARCHIVE_NAME=$(basename "$FRONTEND_ARCHIVE")
    echo "  Frontend archive: ${FRONTEND_ARCHIVE} (${SIZE} bytes, sha256: ${HASH})"
    COMPONENTS=$(echo "$COMPONENTS" | python3 -c "
import sys, json
c = json.load(sys.stdin)
c.append({
    'name': '$ARCHIVE_NAME',
    'current_version': '$CURRENT_VERSION',
    'new_version': '$VERSION',
    'download_url': '$BASE_URL/v$VERSION/$ARCHIVE_NAME',
    'sha256': '$HASH',
    'size_bytes': $SIZE
})
print(json.dumps(c))
")
else
    echo "  Warning: Frontend archive not found"
fi

# Read changelog from CHANGELOG.md if available
CHANGELOG="[]"
CHANGELOG_FILE="$PROJECT_ROOT/CHANGELOG.md"
if [ -f "$CHANGELOG_FILE" ]; then
    # Extract entries for this version (lines between ## vVERSION and next ##)
    ENTRIES=$(python3 -c "
import re, sys
content = open('$CHANGELOG_FILE').read()
pattern = r'## .*?${VERSION}.*?\n(.*?)(?=\n## |\Z)'
m = re.search(pattern, content, re.DOTALL)
if m:
    for line in m.group(1).strip().split('\n')[:10]:
        line = line.strip()
        if line:
            print(line)
" 2>/dev/null || echo "")
    if [ -n "$ENTRIES" ]; then
        CHANGELOG=$(echo "$ENTRIES" | python3 -c "
import sys, json
lines = [l.strip().lstrip('- ') for l in sys.stdin if l.strip()]
print(json.dumps(lines))
")
    fi
fi

# If no changelog entries found, add a default
if [ "$CHANGELOG" = "[]" ]; then
    CHANGELOG="[\"Update to version ${VERSION}\"]"
fi

# Generate manifest
python3 -c "
import json
manifest = {
    'version': '$VERSION',
    'release_date': '$RELEASE_DATE',
    'changelog': $CHANGELOG,
    'components': $COMPONENTS
}
print(json.dumps(manifest, indent=2))
" > "$OUTPUT_FILE"

echo ""
echo "Manifest written to: $OUTPUT_FILE"
echo ""
cat "$OUTPUT_FILE"
echo ""
echo "Next steps:"
echo "  1. Review and sign the manifest above"
echo "  2. Keep it under releases/pending/v$VERSION/ — do NOT replace the live manifest"
echo "  3. Run scripts/publish-release-assets.sh $VERSION gitea-vps2"
echo "     (it uploads + verifies assets before atomically promoting the manifest)"
