#!/usr/bin/env python3
"""Report drift between app-catalog/catalog.json and apps/*/manifest.yml."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

import yaml

INTERNAL_MANIFEST_IDS = {
    "aiui",
    "archy-btcpay-db",
    "archy-mempool-db",
    "archy-mempool-web",
    "archy-nbxplorer",
    "bitcoin-ui",
    "core-lightning",
    "cuprate-ui",
    "electrs-ui",
    "fips-ui",
    "lnd-ui",
    "mempool-api",
    "router",
    "strfry",
    "web5-dwn",
    "immich-postgres",
    "immich-redis",
    "indeedhub-api",
    "indeedhub-ffmpeg",
    "indeedhub-minio",
    "indeedhub-postgres",
    "indeedhub-redis",
    "indeedhub-relay",
    "netbird-dashboard",
    "netbird-server",
    "pine-whisper",
    "pine-piper",
    "pine-openwakeword",
}

LEGACY_STACK_CATALOG_IDS = {
    "btcpay",
    "immich",
    "netbird",
    "tailscale",
}


def load_catalog(path: Path) -> dict[str, dict[str, Any]]:
    """Load either catalog shape into {app_id: app-fields}.

    Two formats exist and only one used to be understood here:

      * app-catalog/catalog.json  — `apps` is a LIST of entries carrying `id`.
      * releases/app-catalog.json — `apps` is a DICT keyed by app id, and each
        entry wraps the app's full manifest under `manifest.app` (the signed
        release catalog; EMBED_MANIFESTS has been on since 2026-06-23).

    The signed release catalog is the one nodes actually resolve apps through,
    so a drift checker that only parsed the list form was checking the file
    that governs nothing and crashing on the file that governs everything.
    """
    with path.open("r", encoding="utf-8") as fh:
        data = json.load(fh)
    apps = data.get("apps", [])

    if isinstance(apps, list):
        return {
            str(app.get("id", "")): app
            for app in apps
            if isinstance(app, dict) and app.get("id")
        }

    if isinstance(apps, dict):
        out: dict[str, dict[str, Any]] = {}
        for app_id, entry in apps.items():
            if not isinstance(entry, dict):
                continue
            manifest = entry.get("manifest")
            for variant in reversed(entry.get("manifest_variants", [])):
                requires = variant.get("requires", [])
                if requires and all(cap == "runtime-migration-backup-v1" for cap in requires):
                    manifest = variant.get("manifest")
                    break
            if isinstance(manifest, dict) and isinstance(manifest.get("app"), dict):
                # Embedded manifest: compare against the same fields the disk
                # manifests expose, plus the entry's own version.
                app = dict(manifest["app"])
            else:
                app = {}
            app.setdefault("id", app_id)
            if entry.get("version") is not None:
                app["version"] = entry["version"]
            out[str(app_id)] = app
        return out

    raise ValueError(f"{path}: expected .apps to be a list or an object")


def load_manifests(apps_dir: Path) -> dict[str, dict[str, Any]]:
    manifests: dict[str, dict[str, Any]] = {}
    for path in sorted(apps_dir.glob("*/manifest.yml")):
        with path.open("r", encoding="utf-8") as fh:
            data = yaml.safe_load(fh)
        if not isinstance(data, dict) or not isinstance(data.get("app"), dict):
            continue
        app = data["app"]
        app_id = app.get("id")
        if app_id:
            manifests[str(app_id)] = {"path": str(path), "app": app}
    return manifests


def metadata(app: dict[str, Any]) -> dict[str, Any]:
    value = app.get("metadata")
    return value if isinstance(value, dict) else {}


def manifest_value(app: dict[str, Any], field: str) -> Any:
    meta = metadata(app)
    container = app.get("container") if isinstance(app.get("container"), dict) else {}
    match field:
        case "title":
            return app.get("name")
        case "version":
            return str(app.get("version", ""))
        case "description":
            return app.get("description")
        case "dockerImage":
            return container.get("image")
        case "category":
            return app.get("category") or meta.get("category")
        case "tier":
            return meta.get("tier")
        case "icon":
            return meta.get("icon")
        case "repoUrl":
            return meta.get("repo") or meta.get("repoUrl")
        case _:
            return None


def normalize(value: Any) -> str:
    if value is None:
        return ""
    return str(value).strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", default="app-catalog/catalog.json")
    parser.add_argument("--apps-dir", default="apps")
    parser.add_argument(
        "--strict",
        action="store_true",
        help="exit non-zero when missing entries or metadata drift are found",
    )
    parser.add_argument(
        "--release",
        action="store_true",
        help="suppress known internal/legacy-stack entries so output is release-actionable",
    )
    args = parser.parse_args()

    catalog_path = Path(args.catalog)
    with catalog_path.open("r", encoding="utf-8") as fh:
        raw_catalog = json.load(fh)
    registry_catalog = isinstance(raw_catalog.get("apps"), dict)

    catalog = load_catalog(catalog_path)
    manifests = load_manifests(Path(args.apps_dir))

    catalog_ids = set(catalog)
    manifest_ids = set(manifests)
    missing_manifests = sorted(catalog_ids - manifest_ids)
    missing_catalog = sorted(manifest_ids - catalog_ids)
    if args.release:
        missing_manifests = [app_id for app_id in missing_manifests if app_id not in LEGACY_STACK_CATALOG_IDS]
        missing_catalog = [app_id for app_id in missing_catalog if app_id not in INTERNAL_MANIFEST_IDS]

    compared_fields = [
        "title",
        "description",
        "category",
        "tier",
        "icon",
        "repoUrl",
    ]
    # The signed registry catalog deliberately overrides versions and images:
    # that is how app updates are shipped independently of an OS release. The
    # public storefront list, on the other hand, should match the manifest's
    # advertised version/image. Treating registry overrides as metadata drift
    # made a freshly generated, otherwise valid release artifact fail on every
    # app whose image tag had advanced since its disk manifest was authored.
    if not registry_catalog:
        compared_fields[1:1] = ["version", "dockerImage"]
    drift: list[str] = []
    for app_id in sorted(catalog_ids & manifest_ids):
        catalog_app = catalog[app_id]
        manifest_app = manifests[app_id]["app"]
        for field in compared_fields:
            catalog_val = normalize(catalog_app.get(field))
            manifest_val = normalize(manifest_value(manifest_app, field))
            if catalog_val and manifest_val and catalog_val != manifest_val:
                drift.append(f"{app_id}: {field}: catalog={catalog_val!r} manifest={manifest_val!r}")

    print(
        json.dumps(
            {
                "catalog_apps": len(catalog),
                "manifest_apps": len(manifests),
                "missing_manifests": len(missing_manifests),
                "missing_catalog": len(missing_catalog),
                "metadata_drift": len(drift),
            },
            sort_keys=True,
        )
    )

    for app_id in missing_manifests:
        print(f"MISSING_MANIFEST {app_id}")
    for app_id in missing_catalog:
        print(f"MISSING_CATALOG {app_id}")
    for item in drift:
        print(f"DRIFT {item}")

    if args.strict and (missing_manifests or missing_catalog or drift):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
