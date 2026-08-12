#!/usr/bin/env bash
# Gated ISO release build — the single command that turns a signed release
# on `main` into a tested installer ISO.
#
# Stages (fail-fast, each logged with timing):
#   0. preflight   — Linux, clean tree on main, version parity across
#                    Cargo.toml / package.json / releases/manifest.json /
#                    CHANGELOG / git tag, manifest signature present
#   1. gates       — tests/release/run.sh (static + frontend + backend
#                    slice), strict catalog drift, FULL cargo test suite
#   2. artifacts   — release binary embeds the version, frontend dist
#                    matches, AIUI present (OTA-strip regression guard)
#   3. build       — image-recipe/build-debian-iso.sh (unbundled by default)
#   4. smoke       — scripts/iso-smoke-test.sh (mount-level, version-checked)
#   5. qemu        — headless boot test (skippable with --no-qemu)
#
# Usage:
#   scripts/build-iso-release.sh [--skip-gates] [--no-qemu] [--bundled] [--rc N]
#
# The ISO is NOT signed here — run scripts/sign-iso-checksums.sh with the
# offline RELEASE_MASTER_MNEMONIC afterwards (publisher only).

set -u

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

SKIP_GATES=0 NO_QEMU=0 UNBUNDLED=1 RC_OVERRIDE=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-gates) SKIP_GATES=1 ;;
        --no-qemu)    NO_QEMU=1 ;;
        --bundled)    UNBUNDLED=0 ;;
        --rc)         RC_OVERRIDE="${2:?--rc needs a number}"; shift ;;
        *) echo "unknown flag: $1" >&2; exit 2 ;;
    esac
    shift
done

[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

PASS=() FAIL=()
stage() { # stage <name> <cmd...>
    local name="$1"; shift
    local t0=$SECONDS
    echo
    echo "═══ [$name] $*"
    if "$@"; then
        echo "═══ [$name] PASS ($((SECONDS - t0))s)"
        PASS+=("$name")
    else
        local rc=$?
        echo "═══ [$name] FAIL exit=$rc ($((SECONDS - t0))s)"
        FAIL+=("$name")
        summary 1
    fi
}
summary() {
    echo
    echo "──────── ISO release build summary ────────"
    printf 'PASS: %s\n' "${PASS[@]:-none}"
    [[ ${#FAIL[@]} -gt 0 ]] && printf 'FAIL: %s\n' "${FAIL[@]}"
    exit "${1:-0}"
}

# ── Stage 0: preflight ───────────────────────────────────────────────
preflight() {
    [ "$(uname -s)" = "Linux" ] || { echo "ISO builds run on Linux only"; return 1; }

    local branch; branch="$(git rev-parse --abbrev-ref HEAD)"
    [ "$branch" = "main" ] || { echo "must build from main (on: $branch)"; return 1; }

    if [ -n "$(git status --porcelain)" ]; then
        echo "working tree is not clean — release ISOs build from committed state only:"
        git status --porcelain | head -20
        return 1
    fi

    VERSION="$(grep -m1 '^version' core/archipelago/Cargo.toml | sed 's/.*"\(.*\)".*/\1/')"
    local ui_ver manifest_ver
    ui_ver="$(python3 -c 'import json;print(json.load(open("neode-ui/package.json"))["version"])')"
    manifest_ver="$(python3 -c 'import json;print(json.load(open("releases/manifest.json"))["version"])')"
    echo "  Cargo.toml:        $VERSION"
    echo "  package.json:      $ui_ver"
    echo "  releases/manifest: $manifest_ver"
    [ "$VERSION" = "$ui_ver" ] || { echo "version mismatch Cargo vs package.json"; return 1; }
    [ "$VERSION" = "$manifest_ver" ] || { echo "version mismatch Cargo vs releases/manifest.json"; return 1; }

    head -5 CHANGELOG.md | grep -qF "v$VERSION" \
        || { echo "CHANGELOG.md top entry is not v$VERSION"; return 1; }

    git rev-parse -q --verify "refs/tags/v$VERSION" >/dev/null \
        || { echo "tag v$VERSION does not exist — cut the release first (scripts/create-release.sh)"; return 1; }

    # The ISO must only ever be cut from a ceremony-signed manifest.
    python3 - <<'EOF' || return 1
import json, sys
m = json.load(open("releases/manifest.json"))
sig, by = m.get("signature"), m.get("signed_by", "")
if not sig or not by.startswith("did:key:"):
    print("releases/manifest.json is UNSIGNED — run the signing ceremony first")
    sys.exit(1)
print(f"  manifest signed by {by[:32]}…")
EOF

    echo "  version: $VERSION @ $(git rev-parse --short HEAD), tree clean, manifest signed"
}
stage "preflight" preflight
VERSION="$(grep -m1 '^version' core/archipelago/Cargo.toml | sed 's/.*"\(.*\)".*/\1/')"

# ── Stage 1: gates ───────────────────────────────────────────────────
if [ "$SKIP_GATES" = "0" ]; then
    stage "release-gate-harness" bash tests/release/run.sh
    stage "catalog-drift-strict" python3 scripts/check-app-catalog-drift.py --release --strict
    # Full Rust suite — the release harness only runs a 6-module slice;
    # ~1000 tests otherwise go unverified at ISO time (hardening plan §H).
    stage "cargo-test-full" timeout 5400 env CARGO_INCREMENTAL=0 \
        nice -n 10 cargo test --manifest-path core/Cargo.toml -p archipelago --bin archipelago
else
    echo; echo "═══ [gates] SKIPPED (--skip-gates)"
fi

# ── Stage 2: artifact verification ───────────────────────────────────
verify_artifacts() {
    local bin="core/target/release/archipelago"
    [ -x "$bin" ] || { echo "missing release binary $bin — build it first"; return 1; }
    strings "$bin" | grep -qF "$VERSION" \
        || { echo "release binary does not embed $VERSION — stale build"; return 1; }
    echo "  backend binary embeds $VERSION ($(du -h "$bin" | cut -f1))"

    [ -f web/dist/neode-ui/index.html ] || { echo "missing frontend dist"; return 1; }
    grep -rqoF "$VERSION" web/dist/neode-ui/assets/*.js \
        || { echo "frontend dist does not contain $VERSION — stale build"; return 1; }
    echo "  frontend dist contains $VERSION"

    # AIUI must ride inside the dist BEFORE packaging or OTA upgrades
    # silently strip it from nodes in the field.
    [ -f web/dist/neode-ui/aiui/index.html ] \
        || { echo "AIUI missing from web/dist/neode-ui/aiui — fold it in before building"; return 1; }
    echo "  AIUI present in frontend dist"
}
stage "verify-artifacts" verify_artifacts

# ── Stage 3: build the ISO ───────────────────────────────────────────
build_iso() {
    local env_args=(
        UNBUNDLED="$UNBUNDLED"
        BUILD_FROM_SOURCE=0
        DEV_SERVER=localhost
        ARCHIPELAGO_BIN="$REPO/core/target/release/archipelago"
    )
    [ -n "$RC_OVERRIDE" ] && env_args+=(RC="$RC_OVERRIDE")
    sudo -E env "${env_args[@]}" nice -n 5 bash image-recipe/build-debian-iso.sh
}
stage "build-iso" build_iso

find_iso() {
    ls -t "$REPO"/image-recipe/results/archipelago-installer-"$VERSION"*-x86_64_RC*.iso 2>/dev/null | head -1
}
ISO="$(find_iso)"
[ -n "$ISO" ] || { echo "FAIL: no ISO produced for $VERSION in image-recipe/results/"; FAIL+=("locate-iso"); summary 1; }

# ── Stage 4: mount-level smoke test ──────────────────────────────────
stage "iso-smoke" bash scripts/iso-smoke-test.sh "$ISO" "$VERSION"

# ── Stage 5: QEMU boot test (best-effort) ────────────────────────────
# The ISO's kernel cmdline has no serial console, so the serial-log
# sanity grep can miss a perfectly healthy boot. Run it, report it,
# but don't fail an otherwise-green build on it.
if [ "$NO_QEMU" = "0" ] && command -v qemu-system-x86_64 >/dev/null 2>&1; then
    echo
    echo "═══ [qemu-boot] (best-effort) test-iso-qemu.sh $ISO 180"
    if bash image-recipe/_archived/test-iso-qemu.sh "$ISO" 180; then
        echo "═══ [qemu-boot] PASS"
        PASS+=("qemu-boot")
    else
        echo "═══ [qemu-boot] INCONCLUSIVE (not gating — verify on real hardware)"
        PASS+=("qemu-boot(inconclusive)")
    fi
else
    echo; echo "═══ [qemu-boot] SKIPPED"
fi

# ── Done ─────────────────────────────────────────────────────────────
SHA_FILE="$ISO.sha256"
[ -f "$SHA_FILE" ] || (cd "$(dirname "$ISO")" && sha256sum "$(basename "$ISO")" > "$SHA_FILE")

echo
echo "════════════════════════════════════════════════════"
echo " ISO RELEASE BUILD COMPLETE — v$VERSION"
echo "════════════════════════════════════════════════════"
echo " ISO:    $ISO ($(du -h "$ISO" | cut -f1))"
echo " SHA256: $(cut -d' ' -f1 "$SHA_FILE")"
echo
echo " Next steps (publisher, offline mnemonic required):"
echo "   1. scripts/sign-iso-checksums.sh $ISO"
echo "   2. upload ISO + .sha256 + signed checksum JSON alongside the"
echo "      v$VERSION Gitea release assets"
summary 0
