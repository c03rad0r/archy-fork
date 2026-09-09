#!/usr/bin/env python3
"""Sync public app catalog metadata from apps/*/manifest.yml.

Manifests are the source of truth for fields the runtime already needs
(`name`, `version`, `description`, container image, category, tier, icon,
repo URL). The catalog still owns presentation-only fields that manifests do
not carry yet, such as `author`, `requires`, `featured`, and rich
`containerConfig` notes.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

import yaml


SYNC_FIELDS = ("title", "version", "description", "dockerImage", "category", "tier", "icon", "repoUrl", "maintainerNpub")


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
            manifests[str(app_id)] = app
    return manifests


def metadata(app: dict[str, Any]) -> dict[str, Any]:
    value = app.get("metadata")
    return value if isinstance(value, dict) else {}


def manifest_catalog_values(app: dict[str, Any]) -> dict[str, str]:
    meta = metadata(app)
    container = app.get("container") if isinstance(app.get("container"), dict) else {}
    build = container.get("build") if isinstance(container.get("build"), dict) else {}
    values = {
        "title": app.get("name"),
        "version": app.get("version"),
        "description": app.get("description"),
        # The install RPC still requires an image-shaped identifier before it
        # dispatches to the manifest orchestrator. For build-source apps this
        # is the local tag the orchestrator creates, not a registry image.
        "dockerImage": container.get("image") or build.get("tag"),
        "category": app.get("category") or meta.get("category"),
        "tier": meta.get("tier"),
        "icon": meta.get("icon"),
        "repoUrl": meta.get("repo") or meta.get("repoUrl") or meta.get("source"),
        "maintainerNpub": meta.get("maintainer_npub"),
    }
    return {key: str(value) for key, value in values.items() if value is not None and str(value).strip()}


def manifest_launch_port(app: dict[str, Any]) -> int | None:
    """Return the manifest-owned public UI port, when it is unambiguous."""
    interfaces = app.get("interfaces")
    if isinstance(interfaces, dict):
        main = interfaces.get("main")
        if isinstance(main, dict) and main.get("type") == "ui":
            port = main.get("port")
            if isinstance(port, int):
                return port
            if isinstance(port, str) and port.isdigit():
                return int(port)

    health_check = app.get("health_check")
    if not isinstance(health_check, dict) or str(health_check.get("type", "")).lower() != "http":
        return None

    ports = app.get("ports")
    if not isinstance(ports, list):
        return None
    tcp_ports = [
        item.get("host")
        for item in ports
        if isinstance(item, dict) and str(item.get("protocol", "tcp")).lower() == "tcp"
    ]
    if len(tcp_ports) != 1:
        return None
    port = tcp_ports[0]
    if isinstance(port, int):
        return port
    if isinstance(port, str) and port.isdigit():
        return int(port)
    return None


def manifest_opens_in_new_tab(app: dict[str, Any]) -> bool:
    """Return whether manifest launch metadata opts the app out of iframe launch."""
    launch = metadata(app).get("launch")
    if not isinstance(launch, dict):
        return False
    return launch.get("open_in_new_tab") is True


def manifest_requires_host_frame(app: dict[str, Any]) -> bool:
    """Return whether an app must remain inside the dashboard document.

    The Android companion normally promotes app sessions to its native browser
    overlay. Apps that consume a parent-frame integration (for example the
    NIP-07 signer bridge) must stay embedded instead.
    """
    launch = metadata(app).get("launch")
    if not isinstance(launch, dict):
        return False
    return launch.get("requires_host_frame") is True


def ts_string(value: str) -> str:
    return json.dumps(value, ensure_ascii=True)


def render_app_session_config(manifests: dict[str, dict[str, Any]]) -> str:
    ports: dict[str, int] = {}
    titles: dict[str, str] = {}
    new_tab_apps: list[str] = []
    host_frame_apps: list[str] = []
    for app_id, app in sorted(manifests.items()):
        name = app.get("name")
        if isinstance(name, str) and name.strip():
            titles[app_id] = name.strip()
        port = manifest_launch_port(app)
        if port:
            ports[app_id] = port
        opens_in_new_tab = manifest_opens_in_new_tab(app)
        requires_host_frame = manifest_requires_host_frame(app)
        if opens_in_new_tab and requires_host_frame:
            raise ValueError(
                f"{app_id}: metadata.launch.open_in_new_tab and "
                "requires_host_frame cannot both be true"
            )
        if opens_in_new_tab:
            new_tab_apps.append(app_id)
        if requires_host_frame:
            host_frame_apps.append(app_id)

    lines = [
        "/** Generated by scripts/generate-app-catalog.py. Do not edit manually. */",
        "",
        "export const GENERATED_APP_PORTS: Record<string, number> = {",
    ]
    for app_id, port in ports.items():
        lines.append(f"  {ts_string(app_id)}: {port},")
    lines.extend([
        "}",
        "",
        "export const GENERATED_APP_TITLES: Record<string, string> = {",
    ])
    for app_id, title in titles.items():
        lines.append(f"  {ts_string(app_id)}: {ts_string(title)},")
    lines.extend([
        "}",
        "",
        "export const GENERATED_NEW_TAB_APPS = new Set<string>([",
    ])
    for app_id in new_tab_apps:
        lines.append(f"  {ts_string(app_id)},")
    lines.extend([
        "])",
        "",
        "export const GENERATED_HOST_FRAME_APPS = new Set<string>([",
    ])
    for app_id in host_frame_apps:
        lines.append(f"  {ts_string(app_id)},")
    lines.extend(["])", ""])
    return "\n".join(lines)


def render_rust_ports(ports: dict[str, int], extra_ports: list[int]) -> str:
    """Rust constant of catalog launch ports for the fips0 firewall drop-in
    (core/archipelago/src/fips/app_ports.rs). Extra ports cover the frontend's
    APP_PORTS overrides (companions/aliases) that have no manifest of their own.
    """
    distinct = sorted(set(list(ports.values()) + extra_ports))
    lines = [
        "//! Generated by scripts/generate-app-catalog.py. Do not edit manually.",
        "//!",
        "//! Catalog app launch ports (the web UIs the companion opens by direct",
        "//! port). Used to write the fips0 firewall allowance drop-in so app UIs",
        "//! are reachable over the mesh; ports of apps that aren\'t installed have",
        "//! no listener, so allowing them is inert.",
        "",
        "pub const APP_LAUNCH_PORTS: &[u16] = &[",
    ]
    lines.extend(f"    {port}," for port in distinct)
    lines.extend(["];", ""])
    return "\n".join(lines)


# Keep in lockstep with APP_PORTS overrides in
# neode-ui/src/views/appSession/appSessionConfig.ts.
RUST_EXTRA_PORTS = [8334, 50002, 18083, 11434, 8081, 8240, 8175, 8176, 8080]


def sync_catalog(path: Path, manifests: dict[str, dict[str, Any]]) -> int:
    with path.open("r", encoding="utf-8") as fh:
        catalog = json.load(fh)
    apps = catalog.get("apps")
    if not isinstance(apps, list):
        raise ValueError(f"{path}: expected .apps to be a list")

    changed = 0
    for catalog_app in apps:
        if not isinstance(catalog_app, dict):
            continue
        app_id = catalog_app.get("id")
        if not app_id or str(app_id) not in manifests:
            continue
        values = manifest_catalog_values(manifests[str(app_id)])
        for field in SYNC_FIELDS:
            if field not in values:
                continue
            old = catalog_app.get(field)
            new = values[field]
            if old != new:
                catalog_app[field] = new
                changed += 1

    path.write_text(json.dumps(catalog, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return changed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apps-dir", default="apps")
    parser.add_argument(
        "--catalog",
        action="append",
        default=[],
        help="Catalog JSON path to update. May be passed multiple times.",
    )
    parser.add_argument(
        "--rust-app-ports",
        default="core/archipelago/src/fips/app_ports.rs",
        help="Generated Rust launch-port list for the fips0 firewall drop-in. Empty string to skip.",
    )
    parser.add_argument(
        "--app-session-config",
        default="neode-ui/src/views/appSession/generatedAppSessionConfig.ts",
        help="Generated TypeScript app-session metadata path. Pass an empty string to skip.",
    )
    args = parser.parse_args()

    catalogs = args.catalog or ["app-catalog/catalog.json", "neode-ui/public/catalog.json"]
    manifests = load_manifests(Path(args.apps_dir))
    total = 0
    for catalog in catalogs:
        changed = sync_catalog(Path(catalog), manifests)
        total += changed
        print(f"{catalog}: updated {changed} fields")
    if args.app_session_config:
        path = Path(args.app_session_config)
        content = render_app_session_config(manifests)
        old = path.read_text(encoding="utf-8") if path.exists() else ""
        if old != content:
            path.write_text(content, encoding="utf-8")
            print(f"{path}: updated")
        else:
            print(f"{path}: updated 0 fields")
    if args.rust_app_ports:
        ports = {
            app_id: port
            for app_id, app in manifests.items()
            if (port := manifest_launch_port(app))
        }
        rust_path = Path(args.rust_app_ports)
        rust_content = render_rust_ports(ports, RUST_EXTRA_PORTS)
        rust_old = rust_path.read_text(encoding="utf-8") if rust_path.exists() else ""
        if rust_old != rust_content:
            rust_path.write_text(rust_content, encoding="utf-8")
            print(f"{rust_path}: updated")
        else:
            print(f"{rust_path}: updated 0 fields")
    print(f"total_updated={total}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
