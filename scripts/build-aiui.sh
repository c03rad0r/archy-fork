#!/usr/bin/env bash
#
# build-aiui.sh — the ONE supported way to build AIUI for a node.
#
# D-19 (2026-08-03): AIUI is no longer a second repository at
# git.tx1138.com/lfg2025/AIUI. It was migrated in-repo to aiui/ via
# `git subtree`, full history intact. There is no second checkout to clone,
# no separate per-repo pin file to read or write, and no dirty-second-tree
# refusal to perform — this repo's own ordinary commit discipline is what
# keeps its history honest now, not a second-repo-specific check. D-15's
# *delivery* half still stands and is what this script enforces:
#
#   - VITE_BASE_PATH must be exactly /aiui/. A wrong value produces a BLACK
#     PAGE when embedded — the router base breaks, not the assets (this has
#     burned this project before). Enforced here, not remembered by whoever
#     runs the build.
#   - A fresh checkout of this repo has no aiui/node_modules (unlike the old
#     world, where a developer's separate AIUI clone was assumed already
#     `pnpm install`ed) — this script installs from aiui/pnpm-lock.yaml
#     with --frozen-lockfile before building, and treats a lockfile/
#     package.json mismatch as a hard failure, not something to silently
#     resolve.
#   - The build runs AIUI's own real command (vue-tsc --noEmit && vite
#     build via `pnpm build`), so a type error fails the build loudly
#     instead of shipping a stale dist.
#   - Before anything is copied anywhere, the emitted dist is verified:
#     every local asset href carries the AIUI mount path, and this repo's
#     own current commit (there is no second repo's SHA to pin — D-19 made
#     them the same thing) is discoverable in the output, so a deployed
#     node is attributable to a commit of THIS repo.
#
# Usage:
#   bash scripts/build-aiui.sh
#
# On success, aiui/packages/app/dist/ is a fresh, verified AIUI build ready
# to be rsynced/tar'd to a node by scripts/deploy-to-target.sh or
# scripts/setup-aiui-server.sh.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
AIUI_ROOT="$PROJECT_DIR/aiui"
AIUI_APP_DIR="$AIUI_ROOT/packages/app"
AIUI_DIST="$AIUI_APP_DIR/dist"
AIUI_MOUNT_PATH="/aiui/"

# Source archives/container contexts omit .git. Require their caller to supply
# the exact checkout revision; normal node builds always use the real checkout.
if git -C "$PROJECT_DIR" rev-parse --verify HEAD >/dev/null 2>&1; then
    SOURCE_REVISION="$(git -C "$PROJECT_DIR" rev-parse HEAD)"
else
    SOURCE_REVISION="${ARCHY_SOURCE_REVISION:-}"
fi
if [[ ! "$SOURCE_REVISION" =~ ^[0-9a-f]{40}$ ]]; then
    echo "FATAL: a full source commit is required (ARCHY_SOURCE_REVISION for archives)." >&2
    exit 1
fi

timestamp() { echo "[$(date +%H:%M:%S)]"; }

# ── require_base_path ──────────────────────────────────────────────────
# Fails loudly, not silently, when VITE_BASE_PATH is unset or wrong. The
# normal case never reaches the "unset" branch below: the caller sets a
# default via `: "${VITE_BASE_PATH:=$AIUI_MOUNT_PATH}"` before this runs.
# This function's job is to catch an operator override with the WRONG
# value (T-13-57: a wrong base path ships a black page to every node).
require_base_path() {
    if [ -z "${VITE_BASE_PATH:-}" ]; then
        echo "FATAL: VITE_BASE_PATH is unset." >&2
        echo "       AIUI must be built with VITE_BASE_PATH=${AIUI_MOUNT_PATH}" >&2
        echo "       or every asset href in the built index.html will be" >&2
        echo "       wrong and AIUI will render as a BLACK PAGE when" >&2
        echo "       embedded at ${AIUI_MOUNT_PATH} on a node." >&2
        return 1
    fi
    if [ "$VITE_BASE_PATH" != "$AIUI_MOUNT_PATH" ]; then
        echo "FATAL: VITE_BASE_PATH='$VITE_BASE_PATH' is wrong." >&2
        echo "       AIUI must be built with VITE_BASE_PATH=${AIUI_MOUNT_PATH}" >&2
        echo "       exactly. A wrong base path breaks the router base (not" >&2
        echo "       the assets) and ships a BLACK PAGE to every node this" >&2
        echo "       dist reaches." >&2
        return 1
    fi
    return 0
}

# ── verify_dist ─────────────────────────────────────────────────────────
# Asserts the build is safe to ship, BEFORE anything is copied anywhere.
verify_dist() {
    local index="$AIUI_DIST/index.html"
    if [ ! -f "$index" ]; then
        echo "FATAL: $index does not exist — the build did not produce a dist." >&2
        return 1
    fi

    # Every local (leading-"/") src=/href= must carry the AIUI mount path.
    # A hand-built bundle with the wrong base path gives a black page, and
    # the router base is what actually breaks, not the assets.
    local bad_refs
    bad_refs=$(grep -oE '(src|href)="/[^"]*"' "$index" \
        | grep -v -F "=\"${AIUI_MOUNT_PATH}" || true)
    if [ -n "$bad_refs" ]; then
        echo "FATAL: $index references local assets outside ${AIUI_MOUNT_PATH}:" >&2
        echo "$bad_refs" | sed 's/^/       /' >&2
        return 1
    fi

    # Mock quarantine (operator decision 2026-08-07, enforced 8329b826):
    # production bundles must carry NO mock content hosts. The demo site's
    # content pack builds with VITE_DEMO_CONTENT=true and legitimately
    # contains them — skip this check for that build.
    if [ "${VITE_DEMO_CONTENT:-false}" != "true" ]; then
        local mock_hits
        mock_hits=$(grep -rl -e 'spotify\.com/track/example' -e 'cloud\.example\.com' \
            -e 'plex://play' -e 'image\.tmdb\.org' "$AIUI_DIST" 2>/dev/null || true)
        if [ -n "$mock_hits" ]; then
            echo "FATAL: production bundle contains mock content hosts" >&2
            echo "       (mocks are demo-site-only per the 2026-08-07 operator decision):" >&2
            echo "$mock_hits" | sed 's/^/       /' >&2
            return 1
        fi
    fi

    # Attribute this build to THIS repo's own current commit (D-19: no
    # second-repo pin file — this repo's own commit IS the answer now).
    local commit_sha
    commit_sha="$SOURCE_REVISION"
    {
        echo "commit=$commit_sha"
        echo "built_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        echo "base_path=$VITE_BASE_PATH"
    } > "$AIUI_DIST/BUILD-INFO"
    if ! grep -rq "$commit_sha" "$AIUI_DIST/"; then
        echo "FATAL: this repo's current commit ($commit_sha) is not" >&2
        echo "       discoverable anywhere under $AIUI_DIST — a deployed" >&2
        echo "       node would be unattributable to a source commit." >&2
        return 1
    fi

    # Best-effort staleness check: if the source tree changed since the
    # last recorded build but the emitted asset filenames are IDENTICAL,
    # something didn't actually rebuild — Vite's content hashes should
    # differ whenever the content they hash differs. State lives in a
    # gitignored marker beside dist/ (dist/ itself gets wiped by every
    # `vite build`, so it can't hold its own prior-run history).
    local src_hash_file="$AIUI_APP_DIR/.build-aiui-last-src-hash"
    local prev_asset_list="$AIUI_APP_DIR/.build-aiui-last-assets"
    local cur_src_hash cur_assets
    cur_src_hash=$(find "$AIUI_APP_DIR/src" -type f -print0 2>/dev/null \
        | sort -z | xargs -0 sha256sum 2>/dev/null | sha256sum | awk '{print $1}')
    cur_assets=$(ls "$AIUI_DIST/assets" 2>/dev/null | sort)
    if [ -f "$src_hash_file" ] && [ -f "$prev_asset_list" ]; then
        local prev_src_hash prev_assets
        prev_src_hash=$(cat "$src_hash_file")
        prev_assets=$(cat "$prev_asset_list")
        if [ "$prev_src_hash" != "$cur_src_hash" ] && [ "$prev_assets" = "$cur_assets" ] && [ -n "$cur_assets" ]; then
            echo "FATAL: source changed since the last build but the" >&2
            echo "       emitted asset filenames are IDENTICAL to the" >&2
            echo "       previous build — this looks like a stale/cached" >&2
            echo "       dist, not a fresh build." >&2
            return 1
        fi
    fi
    echo "$cur_src_hash" > "$src_hash_file"
    echo "$cur_assets" > "$prev_asset_list"

    return 0
}

echo "$(timestamp) build-aiui.sh: building AIUI from $AIUI_ROOT (in-repo, D-19)"

if [ ! -d "$AIUI_ROOT" ]; then
    echo "FATAL: $AIUI_ROOT does not exist." >&2
    echo "       AIUI is expected in-repo at aiui/ — it is no longer a" >&2
    echo "       sibling checkout at ../AIUI (D-19)." >&2
    exit 1
fi

: "${VITE_BASE_PATH:=$AIUI_MOUNT_PATH}"
export VITE_BASE_PATH
require_base_path

echo "$(timestamp) Installing aiui/ workspace from its committed lockfile..."
(cd "$AIUI_ROOT" && pnpm install --frozen-lockfile)

echo "$(timestamp) Building AIUI (vue-tsc --noEmit && vite build)..."
(cd "$AIUI_APP_DIR" && VITE_BASE_PATH="$VITE_BASE_PATH" pnpm build)

echo "$(timestamp) Verifying dist..."
verify_dist

echo "$(timestamp) AIUI build OK — $AIUI_DIST attributable to ${SOURCE_REVISION:0:8}"
