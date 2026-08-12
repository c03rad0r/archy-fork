#!/usr/bin/env python3
"""Refuse to publish a catalog naming registry hosts the fleet cannot pull from.

The signed app catalog is authoritative for deployed nodes: `catalog_image_override`
makes its image reference win over the on-disk manifest. So a catalog that names a
registry host the *deployed* binaries do not trust turns every install into
"not from a trusted registry" — fleet-wide, at publish time, with no local signal.

The subtlety this guard exists for: TRUSTED_REGISTRIES in the working tree
describes a binary being built today. Nodes run what was shipped to them. Those
two lists diverge for exactly as long as it takes an OTA to reach the fleet, and
that window is when a catalog regeneration silently breaks everything.

So the floor is tracked explicitly in releases/registry-trust-floor.json and the
catalog is checked against that, never against the source tree.

Usage:
  scripts/check-catalog-registry-trust.py                        # check the release catalog
  scripts/check-catalog-registry-trust.py --catalog path.json
  scripts/check-catalog-registry-trust.py --show                 # print current state
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any, Iterator

DEFAULT_CATALOG = "releases/app-catalog.json"
DEFAULT_FLOOR = "releases/registry-trust-floor.json"
IMAGE_POLICY = "core/archipelago/src/container/image_policy.rs"


def iter_images(node: Any) -> Iterator[str]:
    """Yield every value stored under an `image` key, at any depth."""
    if isinstance(node, dict):
        for key, value in node.items():
            if key == "image" and isinstance(value, str) and value:
                yield value
            else:
                yield from iter_images(value)
    elif isinstance(node, list):
        for item in node:
            yield from iter_images(item)


def registry_host(image: str) -> str | None:
    """Registry host of an image ref, or None for Docker Hub shorthand.

    A ref's first segment is a registry only if it contains a '.' or ':'
    (docker.io, host:3000). Otherwise it is a Docker Hub namespace — `nginx`,
    `btcpayserver/btcpayserver` — which resolves via registries.conf, not an
    attacker-controlled host. This mirrors is_valid_docker_image() in
    image_policy.rs; keep the two in step.
    """
    head = image.split("/", 1)[0]
    if "/" not in image:
        return None
    if "." in head or ":" in head:
        return head
    return None


def source_trusted_registries(repo: Path) -> list[str]:
    """TRUSTED_REGISTRIES as the working tree currently defines it (advisory)."""
    path = repo / IMAGE_POLICY
    try:
        text = path.read_text(encoding="utf-8")
    except OSError:
        return []
    match = re.search(r"TRUSTED_REGISTRIES:\s*&\[&str\]\s*=\s*&\[(.*?)\];", text, re.S)
    if not match:
        return []
    body = match.group(1)
    hosts = re.findall(r'"([^"]+)"', body)
    # Entries may be consts (LEGACY_REGISTRY_HOST); resolve those too.
    for const in re.findall(r"\b([A-Z][A-Z0-9_]+)\b", body):
        const_match = re.search(rf'{const}:\s*&str\s*=\s*"([^"]+)"', text)
        if const_match:
            hosts.append(const_match.group(1))
    return sorted(set(hosts))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--catalog", default=DEFAULT_CATALOG)
    parser.add_argument("--floor", default=DEFAULT_FLOOR)
    parser.add_argument("--repo", default=".")
    parser.add_argument("--show", action="store_true",
                        help="print the floor, the source list and the catalog's hosts")
    args = parser.parse_args()

    repo = Path(args.repo)
    catalog_path = repo / args.catalog
    floor_path = repo / args.floor

    try:
        catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
    except OSError as exc:
        print(f"ERROR: cannot read catalog: {exc}", file=sys.stderr)
        return 2
    try:
        floor_doc = json.loads(floor_path.read_text(encoding="utf-8"))
    except OSError as exc:
        print(f"ERROR: cannot read trust floor: {exc}", file=sys.stderr)
        return 2

    floor = set(floor_doc.get("hosts") or [])
    if not floor:
        print(f"ERROR: {args.floor} lists no hosts; refusing to pass vacuously.",
              file=sys.stderr)
        return 2

    hosts: dict[str, list[str]] = {}
    for image in iter_images(catalog):
        host = registry_host(image)
        if host:
            hosts.setdefault(host, []).append(image)

    if args.show:
        print("trust floor (deployed binaries):")
        for h in sorted(floor):
            print(f"  {h}")
        pending = floor_doc.get("pending") or {}
        if pending:
            print("pending (not yet in the fleet):")
            for h, meta in pending.items():
                print(f"  {h}  — trusted_from_binary={meta.get('trusted_from_binary')}")
        print("working-tree TRUSTED_REGISTRIES (advisory):")
        for h in source_trusted_registries(repo) or ["(could not parse)"]:
            print(f"  {h}")
        print(f"catalog hosts ({catalog_path}):")
        for h in sorted(hosts):
            print(f"  {h}  ({len(hosts[h])} image refs)")

    violations = sorted(set(hosts) - floor)
    if violations:
        print("")
        print("REFUSING: the catalog names registry hosts the deployed fleet does not trust.")
        for host in violations:
            examples = hosts[host][:3]
            print(f"\n  {host}  — {len(hosts[host])} image refs, e.g.")
            for ref in examples:
                print(f"      {ref}")
        print("")
        print("Publishing this would make every install fail with")
        print('"not from a trusted registry" on every node in the field.')
        print("")
        print(f"Fix by ordering the migration — see the _comment in {args.floor}:")
        print("  ship a binary that trusts the host, confirm the fleet is on it,")
        print("  promote the host in the floor file, and only then regenerate.")
        return 1

    print(f"OK: all {len(hosts)} registry host(s) in {args.catalog} are trusted by the deployed fleet.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
