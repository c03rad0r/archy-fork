#!/usr/bin/env bash
# One-step OTA-manifest signer (counterpart to sign-catalog.sh).
#
#   Run:   bash scripts/sign-manifest.sh
#   Then:  paste your 24-word release master mnemonic, press Enter, then Ctrl-D.
#
# Signs releases/manifest.json in place and cryptographically verifies the
# result against the pinned release-root anchor. The mnemonic is read from the
# terminal only (never stored, never in shell history, never passed to Claude).
#
# Normally create-release.sh signs the manifest inline; this script exists for
# re-signing (e.g. a manifest edited after creation) or signing on a box where
# the release run was non-interactive.
#
# The release root was rotated 2026-08-05. From v1.7.123 this signs with the
# NEW mnemonic and the signer's own anchor already pins that key, so no
# ARCHY_RELEASE_ROOT_PUBKEY override is needed (it was, for .122 only).
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$REPO/releases/manifest.json"

# Use ONLY a prebuilt signer — never compile here (compiling caused hangs in
# the earlier catalog ceremony). Prefer the repo's release build.
BIN=""
for candidate in "$REPO/core/target/release/archipelago" /tmp/archy-sign-bin/release/archipelago; do
    if [[ -x "$candidate" ]]; then BIN="$candidate"; break; fi
done
if [[ -z "$BIN" ]]; then
    echo "⏳ No prebuilt signer found. Build one first:"
    echo "   (cd core && cargo build --release -p archipelago)"
    echo "   Nothing was changed."
    exit 0
fi

echo "════════════════════════════════════════════════════════════════"
echo "  Paste your 24-word release master mnemonic below, press Enter,"
echo "  then press Ctrl-D on a new line."
echo "════════════════════════════════════════════════════════════════"
"$BIN" ceremony sign "$MANIFEST"

echo
if "$BIN" ceremony verify "$MANIFEST"; then
    echo "✅ SUCCESS — manifest signed by the pinned release root."
    echo "   Commit + push releases/manifest.json (and release-manifest.json if present)."
    cp "$MANIFEST" "$REPO/release-manifest.json" 2>/dev/null || true
else
    echo "❌ Signature did NOT verify against the pinned release-root anchor."
    echo "   Do NOT commit. Check the mnemonic and re-run."
    exit 1
fi
