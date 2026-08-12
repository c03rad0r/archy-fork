#!/usr/bin/env bash
# check-release-assets.sh — prove a release's artifacts are actually fetchable
# BEFORE its manifest goes live on main.
#
# The manifest is the trigger: nodes read releases/manifest.json from branch
# main, and the moment it names a new version they try to download it. So the
# assets must resolve before the manifest lands, not after. On 2026-08-07 the
# order was reversed — the v1.7.126-alpha manifest went live while its binary
# 500'd and its frontend tarball had never uploaded — and every polling node
# would have advertised an update it could not fetch.
#
# For each component in the manifest this checks:
#   1. the download URL returns HTTP 200
#   2. the downloaded bytes match the manifest's sha256 and size
#
# It downloads each asset in full, because a HEAD 200 is not proof the body is
# intact — the corrupt binary that day passed HEAD-shaped checks and still
# served a broken stream. Slower, but this is the last gate before publish.
#
# Usage:
#   scripts/check-release-assets.sh                       # check releases/manifest.json
#   scripts/check-release-assets.sh path/to/manifest.json
#
# Exit 0 = every asset is downloadable and matches. Non-zero = do NOT publish.
set -euo pipefail

MANIFEST="${1:-releases/manifest.json}"
if [[ ! -f "$MANIFEST" ]]; then
  echo "ERROR: manifest not found: $MANIFEST" >&2
  exit 2
fi

command -v python3 >/dev/null 2>&1 || { echo "ERROR: python3 required" >&2; exit 2; }
command -v sha256sum >/dev/null 2>&1 || { echo "ERROR: sha256sum required" >&2; exit 2; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Emit "url<TAB>sha256<TAB>size<TAB>name" per component, tolerating the field
# name variations the manifest has used (download_url/url, size_bytes/size).
rows="$(python3 - "$MANIFEST" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
comps = d.get("components") or []
if not comps:
    sys.exit("manifest has no components")
for c in comps:
    url = c.get("download_url") or c.get("url") or ""
    sha = c.get("sha256") or ""
    size = c.get("size_bytes") or c.get("size") or ""
    name = c.get("name") or "(unnamed)"
    if not url or not sha:
        sys.exit(f"component {name!r} missing url or sha256")
    print(f"{url}\t{sha}\t{size}\t{name}")
PY
)"

version="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("version","?"))' "$MANIFEST")"
echo "Checking release assets for v${version} ($MANIFEST)"
echo ""

fail=0
n=0
while IFS=$'\t' read -r url sha size name; do
  [ -z "$url" ] && continue
  n=$((n + 1))
  out="$TMP/asset.$n"
  echo "  [$name]"
  echo "    $url"

  code="$(curl -sL -o "$out" -w '%{http_code}' "$url" || echo "000")"
  if [ "$code" != "200" ]; then
    echo "    FAIL: HTTP $code (asset not served)"
    fail=1
    continue
  fi

  got_size="$(stat -c%s "$out")"
  if [ -n "$size" ] && [ "$size" != "$got_size" ]; then
    echo "    FAIL: size $got_size, manifest says $size"
    fail=1
    continue
  fi

  got_sha="$(sha256sum "$out" | awk '{print $1}')"
  if [ "$got_sha" != "$sha" ]; then
    echo "    FAIL: sha256 mismatch"
    echo "      served:   $got_sha"
    echo "      manifest: $sha"
    fail=1
    continue
  fi

  echo "    OK: HTTP 200, ${got_size} bytes, sha256 matches"
done <<< "$rows"

echo ""
if [ "$fail" -ne 0 ]; then
  echo "REFUSING: one or more assets are not fetchable or do not match the manifest."
  echo "Do NOT publish the manifest — nodes would advertise an update they cannot"
  echo "apply. Upload/repair the assets, re-run this, and only then flip the"
  echo "manifest live on main."
  exit 1
fi
echo "OK: all $n asset(s) for v${version} download and match the manifest."
