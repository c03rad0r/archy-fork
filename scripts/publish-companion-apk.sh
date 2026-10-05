#!/usr/bin/env bash
# Build the Archipelago companion debug APK and stage it as the served download
# at neode-ui/public/packages/archipelago-companion.apk (a plain APK, so a phone
# can install it straight from the link — no unzip step).
#
# Run manually, or automatically via the pre-push hook (.githooks/pre-push).
#
# Hardened (2026-06-26) so a broken APK can never ship again:
#   1. Aborts on stray resource dirs whose names contain spaces (these break a
#      clean build with "Invalid resource directory name"). Empty ones — junk
#      left by some icon-export tools — are auto-removed; non-empty ones error.
#   2. Always a CLEAN build (incremental builds masked the bad resource dirs).
#   3. Forces v1 + v2 + v3 signing with zipalign + apksigner. AGP's
#      `enableV1Signing = true` flag is silently ignored for minSdk>=24, which
#      shipped a v2-only APK that some OEM installers reject ("App not installed").
#   4. VERIFIES all three schemes and ABORTS if any is missing — no silent ship.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

JAVA="${JAVA_HOME:-/opt/homebrew/opt/openjdk@17}"
SDK="${ANDROID_HOME:-$HOME/Library/Android/sdk}"

if [ ! -x "$JAVA/bin/java" ] || [ ! -d "$SDK" ]; then
  echo "publish-companion-apk: JDK or Android SDK not found — skipping." >&2
  echo "  (set JAVA_HOME and ANDROID_HOME to build the companion APK)" >&2
  exit 0
fi
export JAVA_HOME="$JAVA"
export PATH="$JAVA/bin:$PATH"

RES="Android/app/src/main/res"
APK="Android/app/build/outputs/apk/debug/app-debug.apk"
SIGNED="Android/app/build/outputs/apk/debug/app-debug-signed.apk"
DEST="neode-ui/public/packages/archipelago-companion.apk"
OLD_ZIP="neode-ui/public/packages/archipelago-companion.apk.zip"
KS="Android/app/debug.keystore"

# 1. Guard against resource dirs with spaces (Android forbids them; a clean
#    build aborts on them). Empty ones are removed; non-empty ones are fatal.
while IFS= read -r d; do
  [ -n "$d" ] || continue
  if [ -n "$(ls -A "$d" 2>/dev/null)" ]; then
    echo "publish-companion-apk: ERROR — resource dir with a space is not empty:" >&2
    echo "    $d" >&2
    echo "  Rename it (Android resource dir names cannot contain spaces)." >&2
    exit 1
  fi
  rmdir "$d" && echo "publish-companion-apk: removed stray empty resource dir: $d" >&2
done < <(find "$RES" -type d -name '* *' 2>/dev/null)

# 2. Clean build.
echo "publish-companion-apk: clean build of debug APK…" >&2
( cd Android && ./gradlew -q --console=plain :app:clean :app:assembleDebug )
[ -f "$APK" ] || { echo "publish-companion-apk: ERROR — APK not produced at $APK" >&2; exit 1; }

# 3. Force v1 + v2 + v3 signing (AGP's enableV1Signing flag is ignored here).
BT="$(ls -d "$SDK"/build-tools/*/ | sort -V | tail -1)"
ZIPALIGN="${BT}zipalign"; APKSIGNER="${BT}apksigner"
[ -x "$ZIPALIGN" ] && [ -x "$APKSIGNER" ] || {
  echo "publish-companion-apk: ERROR — zipalign/apksigner not found under $BT" >&2; exit 1; }
[ -f "$KS" ] || { echo "publish-companion-apk: ERROR — keystore missing at $KS" >&2; exit 1; }

echo "publish-companion-apk: zipalign + sign (v1+v2+v3)…" >&2
"$ZIPALIGN" -p -f 4 "$APK" "$SIGNED"
"$APKSIGNER" sign \
  --ks "$KS" --ks-pass pass:android \
  --ks-key-alias androiddebugkey --key-pass pass:android \
  --v1-signing-enabled true --v2-signing-enabled true --v3-signing-enabled true \
  "$SIGNED"

# 4. Verify all three schemes (min-sdk 21 forces the v1 path to be exercised).
VERIFY="$("$APKSIGNER" verify -v --min-sdk-version 21 --print-certs "$SIGNED" 2>&1)"
for scheme in "v1 scheme" "v2 scheme" "v3 scheme"; do
  if ! grep -iq "$scheme.*: true" <<< "$VERIFY"; then
    echo "publish-companion-apk: ERROR — $scheme NOT present after signing. Aborting." >&2
    printf '%s\n' "$VERIFY" | grep -iE "scheme" >&2
    exit 1
  fi
done
# The existing shared companion identity must survive updates. Verifying a new
# certificate's mathematical validity alone does not establish upgrade compatibility.
EXPECTED_SIGNER_SHA256="d622e07e7f474246e8e2bcee828ca9bf6c6cdf9434c065c58fe89bb4eec2664d"
if ! grep -iq "Signer #1 certificate SHA-256 digest: $EXPECTED_SIGNER_SHA256" <<< "$VERIFY"; then
  echo "publish-companion-apk: ERROR — signing identity differs from the existing companion. Aborting." >&2
  exit 1
fi
echo "publish-companion-apk: verified v1 + v2 + v3 signatures and existing signer." >&2

# 5. Publish.
mkdir -p "$(dirname "$DEST")"
cp "$SIGNED" "$DEST"

# Version metadata beside the APK: the dashboard's companion overlay reads
# this to show which version the Download button installs. Extracted from
# the gradle config that just built the APK, so it can never drift from it.
META="${DEST%.apk}.json"
V_NAME="$(sed -n 's/^[[:space:]]*versionName = "\(.*\)"/\1/p' Android/app/build.gradle.kts | head -1)"
V_CODE="$(sed -n 's/^[[:space:]]*versionCode = \([0-9]*\).*/\1/p' Android/app/build.gradle.kts | head -1)"
if [ -n "$V_NAME" ] && [ -n "$V_CODE" ]; then
  printf '{\n  "versionName": "%s",\n  "versionCode": %s\n}\n' "$V_NAME" "$V_CODE" > "$META"
  git add "$META"
  echo "publish-companion-apk: staged $META (v$V_NAME, build $V_CODE)" >&2
else
  echo "publish-companion-apk: WARNING could not extract version from build.gradle.kts — $META not updated" >&2
fi

# Drop the legacy zipped artifact so the served download is the raw APK only.
if [ -f "$OLD_ZIP" ]; then
  git rm -q --ignore-unmatch "$OLD_ZIP" 2>/dev/null || rm -f "$OLD_ZIP"
fi

git add "$DEST"
echo "publish-companion-apk: staged $DEST" >&2
