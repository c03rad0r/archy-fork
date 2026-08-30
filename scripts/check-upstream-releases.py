#!/usr/bin/env python3
"""Report which app pins have fallen behind their upstream project.

Why this exists
---------------
A node only offers an app update when the signed catalog pins a newer image
than the one running (`container/app_catalog.rs::available_update_for_app`).
That machinery works. What was missing is the step *before* it: nothing told
us when upstream had shipped something new, so a catalog pin could sit at
fedimintd v0.10.0 for months and every node in the fleet would correctly and
confidently report "up to date".

The reason nothing could tell us is that the manifests never recorded where an
app comes from. `container.image` names our *mirror*
(`source.archipelago-foundation.org/lfg2025/fedimintd:v0.10.0`), which says
nothing about the project it was mirrored from. So this script reads a new
optional `app.upstream` block (see docs/app-manifest-spec.md), asks that
source what its latest release is, and prints what is behind.

An app with no `upstream` block is reported as UNTRACKED rather than skipped.
A silent skip is how this gap stayed invisible in the first place.

Usage
-----
    scripts/check-upstream-releases.py                 # check everything
    scripts/check-upstream-releases.py fedimint lnd    # check named apps
    scripts/check-upstream-releases.py --offline       # no network; coverage only
    scripts/check-upstream-releases.py --json          # machine-readable

Exit status is 1 when any tracked app is behind, so CI or the release
checklist can gate on it.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import yaml

REPO_ROOT = Path(__file__).resolve().parent.parent
APPS_DIR = REPO_ROOT / "apps"
CATALOG = REPO_ROOT / "releases" / "app-catalog.json"

USER_AGENT = "archipelago-upstream-check/1"
TIMEOUT = 20


# ── Version handling ───────────────────────────────────────────────────────


def version_parts(tag: str) -> tuple[int, ...] | None:
    """Numeric components of a version tag, for ordering only.

    Mirrors `image_versions::parse_version_parts` on the node: accepts a
    leading `v` and ignores a pre-release suffix. Returns None for opaque tags
    (`RELEASE.2024-11-07T00-52-20Z`), which are reported but never *ordered* —
    guessing an order for those is how you end up advertising a downgrade.
    """
    if not tag:
        return None
    core = tag.strip().lstrip("vV").split("-")[0].split("+")[0]
    if not re.fullmatch(r"\d+(\.\d+)*", core):
        return None
    return tuple(int(p) for p in core.split("."))


def tag_of(image: str) -> str:
    """The tag from an image reference, ignoring a registry port."""
    if "@" in image:  # digest pin — no tag to compare
        return ""
    last = image.rsplit("/", 1)[-1]
    return last.split(":", 1)[1] if ":" in last else "latest"


# ── Upstream sources ───────────────────────────────────────────────────────


class RateLimited(RuntimeError):
    """GitHub refused because we are over the anonymous quota."""


def http_json(url: str, headers: dict[str, str] | None = None) -> Any:
    hdrs = {"User-Agent": USER_AGENT, **(headers or {})}
    # Anonymous GitHub allows 60 requests an hour, and a full sweep needs more
    # than that. A token raises it to 5000 — worth exporting before a release
    # pass, and the failure below says so rather than reporting every app as
    # broken.
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if token and "api.github.com" in url:
        hdrs["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, headers=hdrs)
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as res:  # noqa: S310
            return json.loads(res.read().decode())
    except urllib.error.HTTPError as e:
        if e.code in (403, 429) and "rate limit" in (e.read().decode(errors="replace").lower()):
            raise RateLimited(
                "GitHub rate limit reached — export GITHUB_TOKEN and re-run"
            ) from e
        raise


def latest_github(repo: str, current: str = "") -> str:
    """Newest release tag for `owner/name`.

    Three sources, in descending order of what the project *means*: the marked
    latest release, then the release list (many projects publish only
    pre-releases, or never mark a latest), then plain git tags. The last one
    matters more than it looks — electrumx, strfry and nostr-rs-relay all tag
    releases without creating GitHub Release objects, and stopping at the
    release list reported them as having "no orderable version tags" when they
    were simply tagged instead.
    """
    try:
        return str(http_json(f"https://api.github.com/repos/{repo}/releases/latest")["tag_name"])
    except (urllib.error.HTTPError, KeyError):
        pass
    try:
        releases = http_json(f"https://api.github.com/repos/{repo}/releases?per_page=50")
        best = _highest([r["tag_name"] for r in releases if not r.get("draft")], current)
        if best:
            return best
    except urllib.error.HTTPError:
        pass
    tags = http_json(f"https://api.github.com/repos/{repo}/tags?per_page=100")
    return _highest([t["name"] for t in tags], current)


def latest_dockerhub(repo: str, current: str = "") -> str:
    """Newest version-like tag on Docker Hub (`library/nginx`, `valkey/valkey`)."""
    url = f"https://hub.docker.com/v2/repositories/{repo}/tags?page_size=100&ordering=last_updated"
    results = http_json(url).get("results", [])
    return _highest([t["name"] for t in results], current)


PRERELEASE = re.compile(r"(^|[-.])(rc|alpha|beta|dev|pre|snapshot|nightly|canary|test)([-.\d]|$)", re.I)


def is_prerelease(tag: str) -> bool:
    """Does this tag advertise itself as not-yet-stable?

    Needed because ordering ignores the suffix: `13.0.1-ubi10-rc` outranks
    every stable MariaDB tag numerically, so the first run of this script
    recommended shipping a release candidate to the fleet.
    """
    return bool(PRERELEASE.search(tag.strip().lstrip("vV")))


def _variant(tag: str) -> str:
    """The non-numeric suffix of a tag: `1.27-alpine` → `alpine`."""
    core = tag.strip().lstrip("vV")
    m = re.match(r"\d+(\.\d+)*[-.]?(.*)$", core)
    return (m.group(2) if m else "").lower()


def _highest(tags: list[str], current: str = "") -> str:
    """The highest orderable tag, preferring our own variant.

    Preferring the variant is what makes the answer actionable rather than
    merely true: a node pinned to `postgres:16.13-alpine` is not helped by
    being told the newest tag is `18.6-trixie`. Same version, different base
    image — swapping it is a different decision from bumping a version.
    """
    ranked = [(version_parts(t), t) for t in tags]
    ranked = [(p, t) for p, t in ranked if p is not None]

    # Never propose a pre-release to someone on a stable tag. The exception is
    # a project whose stable line *is* suffixed — LND ships `-beta` and always
    # has, so excluding those outright would report it as permanently current.
    if not is_prerelease(current):
        stable = [(p, t) for p, t in ranked if not is_prerelease(t)]
        if stable:
            ranked = stable
    if not ranked:
        return ""

    want = _variant(current)
    if want:
        same = [(p, t) for p, t in ranked if _variant(t) == want]
        if same:
            return max(same)[1]
    return max(ranked)[1]


def latest_gitlab(project: str, current: str = "") -> str:
    """Newest release tag for a GitLab `group/project`.

    Some projects publish releases only on GitLab with no GitHub mirror
    (bark lives at ark-bitcoin/bark and nowhere else). GitLab release tags
    sometimes carry the project name as a prefix (`bark-0.6.2`); strip it so
    version ordering can see the number.
    """
    esc = urllib.parse.quote(project, safe="")
    releases = http_json(
        f"https://gitlab.com/api/v4/projects/{esc}/releases?per_page=100"
    )
    tags = [str(r["tag_name"]) for r in releases]
    prefix = project.rsplit("/", 1)[-1].lower() + "-"
    tags = [t[len(prefix):] if t.lower().startswith(prefix) else t for t in tags]
    return _highest(tags, current)


def latest_ghcr(repo: str, current: str = "") -> str:
    """Newest version-like tag on GitHub's container registry.

    Some images exist only on ghcr.io (immich-app/postgres publishes there
    and nowhere else), so neither the GitHub-release nor the Docker Hub
    fetcher can see them. Anonymous pull token first, then the tag list —
    the same handshake any `docker pull ghcr.io/...` performs.
    """
    token = http_json(
        f"https://ghcr.io/token?scope=repository:{repo}:pull&service=ghcr.io"
    )["token"]
    req = urllib.request.Request(
        f"https://ghcr.io/v2/{repo}/tags/list",
        headers={"User-Agent": USER_AGENT, "Authorization": f"Bearer {token}"},
    )
    with urllib.request.urlopen(req, timeout=TIMEOUT) as res:  # noqa: S310
        tags = [str(t) for t in json.loads(res.read().decode()).get("tags", [])]
    return _highest(tags, current)


FETCHERS = {
    "github": latest_github,
    "dockerhub": latest_dockerhub,
    "gitlab": latest_gitlab,
    "ghcr": latest_ghcr,
}


# ── Manifest reading ───────────────────────────────────────────────────────


@dataclass
class AppPin:
    app_id: str
    manifest_version: str
    image: str
    upstream: dict[str, Any] = field(default_factory=dict)
    catalog_image: str = ""


def load_apps(only: list[str]) -> list[AppPin]:
    catalog_images: dict[str, str] = {}
    if CATALOG.exists():
        catalog = json.loads(CATALOG.read_text()).get("apps", {})
        for app_id, entry in catalog.items():
            image = entry.get("containers") or entry.get("image") or ""
            catalog_images[app_id] = image if isinstance(image, str) else ""

    pins: list[AppPin] = []
    for path in sorted(APPS_DIR.glob("*/manifest.yml")):
        try:
            doc = yaml.safe_load(path.read_text()) or {}
        except yaml.YAMLError as e:
            print(f"warning: {path} is not valid YAML ({e})", file=sys.stderr)
            continue
        app = doc.get("app") or {}
        app_id = app.get("id") or path.parent.name
        if only and app_id not in only:
            continue
        pins.append(
            AppPin(
                app_id=app_id,
                manifest_version=str(app.get("version") or ""),
                image=str((app.get("container") or {}).get("image") or ""),
                upstream=app.get("upstream") or {},
                catalog_image=catalog_images.get(app_id, ""),
            )
        )
    return pins


# ── Reporting ──────────────────────────────────────────────────────────────


def check(pin: AppPin, offline: bool) -> dict[str, Any]:
    # The catalog pin is what nodes actually act on, so it is the number that
    # matters; the manifest is the fallback for apps the catalog doesn't cover.
    shipped_image = pin.catalog_image or pin.image
    shipped = tag_of(shipped_image) or pin.manifest_version

    row: dict[str, Any] = {
        "app": pin.app_id,
        "shipped": shipped,
        "source": "catalog" if pin.catalog_image else "manifest",
        "upstream_kind": pin.upstream.get("kind", ""),
        "latest": "",
        "status": "",
        "jump": "",
        "note": "",
    }

    kind = pin.upstream.get("kind")
    if not kind:
        row["status"] = "UNTRACKED"
        row["note"] = "no app.upstream block — nothing can tell us when this app moves"
        return row
    if kind == "internal":
        row["status"] = "INTERNAL"
        row["note"] = pin.upstream.get("note", "built by this project — no upstream to track")
        return row
    if kind == "manual":
        row["status"] = "MANUAL"
        row["note"] = pin.upstream.get("url", "check by hand")
        return row
    if kind not in FETCHERS:
        row["status"] = "UNKNOWN-KIND"
        row["note"] = f"unsupported upstream.kind {kind!r}"
        return row
    if offline:
        row["status"] = "SKIPPED"
        row["note"] = "offline"
        return row

    ref = pin.upstream.get("repo") or ""
    if not ref:
        row["status"] = "UNKNOWN-KIND"
        row["note"] = f"upstream.kind {kind} needs a repo"
        return row

    try:
        latest = FETCHERS[kind](ref, shipped)
    except RateLimited as e:
        # Distinct from ERROR: the pin may be perfectly current, we just
        # couldn't ask. Reporting it as a failure would train people to ignore
        # the column that matters.
        row["status"] = "RATE-LIMITED"
        row["note"] = str(e)
        return row
    except Exception as e:  # noqa: BLE001 — any failure is "we couldn't ask"
        row["status"] = "ERROR"
        row["note"] = str(e)
        return row

    row["latest"] = latest
    if not latest:
        row["status"] = "ERROR"
        row["note"] = "upstream published no orderable version tags"
        return row

    ours, theirs = version_parts(shipped), version_parts(latest)
    if ours is None or theirs is None:
        row["status"] = "UNCOMPARABLE"
        row["note"] = "opaque tag — compare by hand"
    elif theirs > ours:
        row["status"] = "BEHIND"
        # How big a jump matters more than the fact of one. A major bump is
        # where data migrations live — Postgres will refuse to start on an
        # older cluster, Nextcloud requires one major at a time — so these are
        # a different piece of work from a patch bump, not a longer version of
        # the same one.
        row["jump"] = "major" if theirs[0] != ours[0] else (
            "minor" if len(theirs) > 1 and len(ours) > 1 and theirs[1] != ours[1] else "patch"
        )
        if row["jump"] == "major":
            row["note"] = "major version — check for a data migration before bumping"
    elif theirs < ours:
        row["status"] = "AHEAD"
        row["note"] = "we ship newer than upstream's latest release"
    else:
        row["status"] = "CURRENT"
    return row


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("apps", nargs="*", help="app ids to check (default: all)")
    ap.add_argument("--offline", action="store_true", help="no network — report coverage only")
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    args = ap.parse_args()

    pins = load_apps(args.apps)
    if not pins:
        print("no manifests matched", file=sys.stderr)
        return 2

    rows = [check(p, args.offline) for p in pins]

    if args.json:
        print(json.dumps(rows, indent=2))
    else:
        width = max(len(r["app"]) for r in rows)
        for r in sorted(rows, key=lambda r: (r["status"] != "BEHIND", r["app"])):
            status = r["status"] + (f"/{r['jump']}" if r.get("jump") else "")
            line = f"{r['app']:<{width}}  {status:<19} {r['shipped'] or '-':<18}"
            if r["latest"]:
                line += f"→ {r['latest']:<18}"
            if r["note"]:
                line += f" {r['note']}"
            print(line.rstrip())

        behind = [r["app"] for r in rows if r["status"] == "BEHIND"]
        untracked = [r["app"] for r in rows if r["status"] == "UNTRACKED"]
        print()
        print(f"{len(rows)} apps · {len(behind)} behind · {len(untracked)} untracked")
        majors = [r["app"] for r in rows if r["status"] == "BEHIND" and r.get("jump") == "major"]
        if behind:
            print("Behind: " + ", ".join(behind))
            if majors:
                print(f"Of those, {len(majors)} are MAJOR jumps that may need a data "
                      f"migration, not just a pin bump: " + ", ".join(majors))
            print("Bump the pin, regenerate and re-sign the catalog, and nodes will offer the update.")
        if untracked:
            print("Untracked apps cannot ever be reported as behind — add an app.upstream block.")

    return 1 if any(r["status"] == "BEHIND" for r in rows) else 0


if __name__ == "__main__":
    sys.exit(main())
