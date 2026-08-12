#!/usr/bin/env bash
# Sign an ISO's checksums with the release root (counterpart to sign-manifest.sh).
#
#   Run:   bash scripts/sign-iso-checksums.sh path/to/archipelago-X.Y.Z.iso
#   Then:  paste your 24-word release master mnemonic, press Enter, then Ctrl-D.
#
# Writes <iso>.sha256.json next to the ISO — a JSON document carrying the
# artifact name, size and sha256, signed by the pinned release-root anchor
# (same detached-Ed25519 scheme as the OTA manifest and app catalog).
# Verify anywhere with: archipelago ceremony verify <iso>.sha256.json
#
# The mnemonic is read from the terminal only (never stored, never in shell
# history). The build host never holds the release key: build emits the plain
# <iso>.sha256; the publisher signs with this script during the ceremony.
set -euo pipefail

ISO="${1:-}"
[ -n "$ISO" ] || { echo "Usage: $0 path/to/image.iso"; exit 1; }
[ -f "$ISO" ] || { echo "Error: $ISO not found"; exit 1; }

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

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

ISO_NAME="$(basename "$ISO")"
ISO_DIR="$(cd "$(dirname "$ISO")" && pwd)"
OUT="$ISO_DIR/$ISO_NAME.sha256.json"

echo "Hashing $ISO_NAME (this can take a minute on a large ISO)..."
SHA256="$(sha256sum "$ISO" | awk '{print $1}')"
SIZE="$(wc -c < "$ISO" | tr -d ' ')"

cat > "$OUT" <<EOF
{
  "artifact": "$ISO_NAME",
  "sha256": "$SHA256",
  "size_bytes": $SIZE
}
EOF

echo "════════════════════════════════════════════════════════════════"
echo "  Paste your 24-word release master mnemonic below, press Enter,"
echo "  then press Ctrl-D on a new line."
echo "════════════════════════════════════════════════════════════════"
"$BIN" ceremony sign "$OUT"

echo
if "$BIN" ceremony verify "$OUT"; then
    echo "✅ SUCCESS — $OUT signed by the pinned release root."
    echo "   Publish it next to the ISO together with $ISO_NAME.sha256."
else
    echo "❌ Verification failed — do not publish. Re-run the signing."
    exit 1
fi
