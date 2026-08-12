#!/usr/bin/env bash
# One-step release-catalog signer.
#
#   Run:   bash scripts/sign-catalog.sh
#   Then:  paste your 24-word release master mnemonic, press Enter, then Ctrl-D.
#
# It signs releases/app-catalog.json in place and checks the signature was made
# by the expected release-root key. Your mnemonic is read from the terminal only
# (never stored, never in shell history, never passed to Claude).
set -euo pipefail

REPO="/home/archipelago/Projects/archy"
CATALOG="$REPO/releases/app-catalog.json"
EXPECTED_DID="did:key:z6Mkfu5LT8d4DjETtrkATvHh9Dvcbnr7zBCUwfau8Sw7DLWT"

# Use ONLY the prebuilt signer. If it isn't ready, stop cleanly — never compile
# here (compiling caused the earlier hangs). Claude builds it in the background.
BIN="/tmp/archy-sign-bin/release/archipelago"
if [[ ! -x "$BIN" ]]; then
  echo "⏳ The signer isn't ready yet — Claude is still building it."
  echo "   Wait until Claude says 'READY', then run this again. Nothing was changed."
  exit 0
fi
SIGN=("$BIN" ceremony sign "$CATALOG")

# Preflight BEFORE asking for the mnemonic. Signing is the point of no return:
# a signed catalog is authoritative for every node, and its image refs override
# the on-disk manifests. If it names a registry host the deployed fleet does not
# trust, every install fails "not from a trusted registry" — so catch that here
# rather than after publication.
if ! python3 "$REPO/scripts/check-catalog-registry-trust.py" --repo "$REPO"; then
  echo
  echo "✋ Refusing to sign. Nothing was changed and your mnemonic was not requested."
  exit 1
fi
echo

echo "════════════════════════════════════════════════════════════════"
echo "  Paste your 24-word release master mnemonic below, press Enter,"
echo "  then press Ctrl-D on a new line."
echo "════════════════════════════════════════════════════════════════"
"${SIGN[@]}"

# Verify the signature is present and made by the expected key.
echo
if grep -q "\"signed_by\": \"$EXPECTED_DID\"" "$CATALOG" \
   && grep -q '"signature":' "$CATALOG"; then
  echo "✅ SUCCESS — catalog signed by the correct release-root key."
  echo "   Tell Claude \"signed\" and it will commit + push for you."
else
  echo "❌ Something is off — the catalog is NOT signed by the expected key."
  echo "   Expected signer: $EXPECTED_DID"
  echo "   Do NOT commit. Check the mnemonic and re-run, or ask Claude."
  exit 1
fi
