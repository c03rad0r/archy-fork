#!/usr/bin/env python3
"""Fail when a hardcoded installer image tag disagrees with the app manifest.

The legacy stack installers in core/archipelago/src/api/rpc/package/stacks.rs
carry image references as string literals. Those literals are a second source of
truth for a version, sitting behind the manifest and the signed catalog, and
nothing keeps them in step.

That is not cosmetic. BTCPay shipped 2.4.2 for an actively exploited 2FA bypass
on 2026-08-07 while the legacy installer still named 2.3.9, so the fallback
install path would have deployed the withdrawn release. The same shape applies
to any app whose installer literal is left behind.

The rule enforced here: if an installer literal names the same image repository
as an app manifest, the tags must match. Repositories with no manifest are
ignored, and so are floating tags, which carry no version claim.

Usage:
  scripts/check-installer-image-pins.py
  scripts/check-installer-image-pins.py --show
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

import yaml

# Files that pin images as literals on an install path. Test modules inside them
# are stripped before scanning: fixtures deliberately name old versions.
INSTALLER_SOURCES = [
    "core/archipelago/src/api/rpc/package/stacks.rs",
]

FLOATING_TAGS = {"latest", "stable", "release", "main", "edge"}

IMAGE_RE = re.compile(r'"([a-z0-9][a-z0-9._-]*(?:\.[a-z]+|:[0-9]+)?/[a-z0-9._/-]+:[A-Za-z0-9._-]+)"')


def strip_test_modules(text: str) -> str:
    """Remove #[cfg(test)] modules so fixture literals are not treated as pins."""
    marker = "#[cfg(test)]"
    idx = text.find(marker)
    return text if idx == -1 else text[:idx]


def repo_of(image: str) -> str:
    """Image repository without registry host or tag."""
    without_tag = image.rsplit(":", 1)[0] if ":" in image.rsplit("/", 1)[-1] else image
    head, _, rest = without_tag.partition("/")
    if "." in head or ":" in head or head == "localhost":
        return rest
    return without_tag


def tag_of(image: str) -> str:
    last = image.rsplit("/", 1)[-1]
    return last.rsplit(":", 1)[1] if ":" in last else "latest"


def manifest_images(repo_root: Path) -> dict[str, tuple[str, str]]:
    """{image repo: (tag, manifest path)} across apps/*/manifest.yml."""
    out: dict[str, tuple[str, str]] = {}
    for path in sorted((repo_root / "apps").glob("*/manifest.yml")):
        try:
            data = yaml.safe_load(path.read_text(encoding="utf-8"))
        except Exception:
            continue
        app = (data or {}).get("app")
        if not isinstance(app, dict):
            continue
        image = (app.get("container") or {}).get("image")
        if isinstance(image, str) and image:
            out[repo_of(image)] = (tag_of(image), str(path.relative_to(repo_root)))
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repo", default=".")
    parser.add_argument("--show", action="store_true")
    args = parser.parse_args()

    repo_root = Path(args.repo)
    manifests = manifest_images(repo_root)
    problems: list[str] = []
    checked = 0

    for rel in INSTALLER_SOURCES:
        path = repo_root / rel
        if not path.exists():
            continue
        text = strip_test_modules(path.read_text(encoding="utf-8"))
        for line_no, line in enumerate(text.splitlines(), start=1):
            for image in IMAGE_RE.findall(line):
                repo = repo_of(image)
                if repo not in manifests:
                    continue
                tag = tag_of(image)
                manifest_tag, manifest_path = manifests[repo]
                checked += 1
                if args.show:
                    print(f"  {rel}:{line_no}  {repo}:{tag}  (manifest {manifest_tag})")
                if tag in FLOATING_TAGS or manifest_tag in FLOATING_TAGS:
                    continue
                if tag != manifest_tag:
                    problems.append(
                        f"{rel}:{line_no}\n"
                        f"    installer pins {repo}:{tag}\n"
                        f"    manifest wants {repo}:{manifest_tag}  ({manifest_path})"
                    )

    if problems:
        print("")
        print("Installer image pins disagree with their app manifests:")
        for problem in problems:
            print(f"\n  {problem}")
        print("")
        print("An installer literal left behind deploys the older image on the")
        print("fallback install path — which is how a withdrawn, vulnerable")
        print("release gets installed after it has supposedly been replaced.")
        print("Update the literal to match the manifest.")
        return 1

    print(f"OK: {checked} installer image pin(s) agree with their app manifests.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
