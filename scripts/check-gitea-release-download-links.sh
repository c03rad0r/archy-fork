#!/usr/bin/env bash
# check-gitea-release-download-links.sh - verify Gitea's public release page
# points users at the canonical HTTPS download URLs, not an internal ROOT_URL.
#
# Usage:
#   scripts/check-gitea-release-download-links.sh VERSION ASSET_NAME...

set -euo pipefail

VERSION="${1:-}"
if [ -z "$VERSION" ] || [ "$#" -lt 2 ]; then
  echo "usage: $0 VERSION ASSET_NAME..." >&2
  exit 2
fi
shift

PUBLIC_BASE="${ARCHY_RELEASE_PUBLIC_BASE:-https://source.archipelago-foundation.org/lfg2025/archy}"
page_url="$PUBLIC_BASE/releases/tag/v$VERSION"

command -v curl >/dev/null 2>&1 || { echo "ERROR: curl required" >&2; exit 2; }
command -v python3 >/dev/null 2>&1 || { echo "ERROR: python3 required" >&2; exit 2; }

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
curl -fsSL "$page_url" -o "$tmp"

python3 - "$tmp" "$PUBLIC_BASE" "$VERSION" "$page_url" "$@" <<'PY'
from html.parser import HTMLParser
from urllib.parse import quote
import sys

html_path, public_base, version, page_url, *assets = sys.argv[1:]
with open(html_path, encoding="utf-8") as f:
    html = f.read()

class LinkParser(HTMLParser):
    def __init__(self):
        super().__init__()
        self.hrefs = []

    def handle_starttag(self, tag, attrs):
        if tag.lower() != "a":
            return
        attrs = dict(attrs)
        href = attrs.get("href")
        if href:
            self.hrefs.append(href)

parser = LinkParser()
parser.feed(html)
hrefs = set(parser.hrefs)

bad_internal = sorted(
    h for h in hrefs
    if "/releases/download/" in h and h.startswith(("http://", "https://"))
    and not h.startswith(public_base + "/releases/download/")
)

failures = []
for asset in assets:
    expected = f"{public_base}/releases/download/v{quote(version)}/{quote(asset)}"
    if expected not in hrefs:
        matches = sorted(h for h in hrefs if h.endswith("/" + quote(asset)))
        if matches:
            failures.append(f"{asset}: expected {expected}, found {matches[0]}")
        else:
            failures.append(f"{asset}: expected {expected}, but no matching release-page link was found")

if bad_internal:
    failures.append("release page contains non-canonical download href(s):")
    failures.extend(f"  {h}" for h in bad_internal[:10])

if failures:
    print(f"FAIL: public release page has broken download links: {page_url}", file=sys.stderr)
    for failure in failures:
        print(f"  {failure}", file=sys.stderr)
    print(
        "Fix the Gitea public URL/proxy configuration so release links are generated "
        "from the canonical HTTPS origin, then re-run the publish check.",
        file=sys.stderr,
    )
    sys.exit(1)

print(f"OK: public release page download links use {public_base}")
PY
