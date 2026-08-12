#!/usr/bin/env bash
# Build the Archipelago Debian installer ISO.
#
# The historical ISO builder remains archived because OTA tarballs are the
# normal release path. This wrapper keeps the documented ISO command working
# by running a temporary active-layout copy of that builder with fixed paths.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ARCHIVED_BUILDER="$SCRIPT_DIR/_archived/build-auto-installer-iso.sh"
TMP_DIR="$(mktemp -d -t archipelago-iso-builder.XXXXXX)"

cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

if [ ! -f "$ARCHIVED_BUILDER" ]; then
    echo "Archived ISO builder not found: $ARCHIVED_BUILDER" >&2
    exit 1
fi

TMP_BUILDER="$TMP_DIR/build-auto-installer-iso.sh"
cp "$ARCHIVED_BUILDER" "$TMP_BUILDER"

# The archived builder lived one directory deeper at image-recipe/_archived/.
# Rewrite only path expressions that were relative to that old location.
perl -0pi -e 's#SCRIPT_DIR="\$\(cd "\$\(dirname "\$0"\)" && pwd\)"#SCRIPT_DIR="__ARCHIPELAGO_IMAGE_RECIPE_DIR__"#g' "$TMP_BUILDER"
# Repo-root references: _archived/../../X becomes image-recipe/../X.
perl -0pi -e 's#\$SCRIPT_DIR/\.\./\.\./#\$SCRIPT_DIR/../#g' "$TMP_BUILDER"
perl -0pi -e 's#\$SCRIPT_DIR/\.\./\.\."#\$SCRIPT_DIR/.."#g' "$TMP_BUILDER"
# configs/ lives inside image-recipe/ itself: _archived/../configs becomes image-recipe/configs.
perl -0pi -e 's#\$SCRIPT_DIR/\.\./configs#\$SCRIPT_DIR/configs#g' "$TMP_BUILDER"
perl -0pi -e 's#"\$\(dirname "\$0"\)/\.\./\.\./scripts#"$(dirname "$0")/../scripts#g' "$TMP_BUILDER"

perl -0pi -e "s#__ARCHIPELAGO_IMAGE_RECIPE_DIR__#${SCRIPT_DIR}#g" "$TMP_BUILDER"

chmod +x "$TMP_BUILDER"
exec bash "$TMP_BUILDER" "$@"
