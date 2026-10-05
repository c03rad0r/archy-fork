#!/usr/bin/env bash
# Generate releases/app-catalog.json — the REMOTE per-app version catalog that
# decouples app updates from the binary OTA (see
# core/.../container/app_catalog.rs and docs/dht-distribution-design.md).
#
# Nodes fetch this file over HTTP from the OVH origin (same host as the OTA
# manifest), compare each app's catalog version against the running container
# tag, and light up the per-app "Update" button — no node release required.
#
# The app_id -> image-variable mapping below MIRRORS
# core/archipelago/src/container/image_versions.rs (image_var_for_app +
# containers_for_stack). image_versions.rs is the canonical mapping; keep this in
# sync when you add an app there.
#
# Usage:
#   scripts/generate-app-catalog.sh [output-path]
#   EMBED_MANIFESTS=0 scripts/generate-app-catalog.sh   # version/image only (legacy)
#   # then publish: push releases/app-catalog.json to the OVH gitea (raw URL).
#
# EMBED_MANIFESTS (default ON, 2026-06-23): embed each app's full
# apps/<id>/manifest.yml into its catalog entry's `manifest` field, so nodes
# install from the signed registry alone (no OTA-shipped disk manifest). Consumed
# by container::app_catalog + the orchestrator's load_manifests overlay
# (origin-wins, disk = fallback). See docs/registry-manifest-design.md. The
# migration window is over — every regen now embeds; set EMBED_MANIFESTS=0 only
# to reproduce the old version/image-only catalog.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-$ROOT/releases/app-catalog.json}"

# Export every *_IMAGE var (and ARCHY_REGISTRY) so python can read them.
set -a
# shellcheck disable=SC1091
source "$ROOT/scripts/image-versions.sh"
set +a

UPDATED="$(date -u +%Y-%m-%d)" OUT="$OUT" APPS_DIR="$ROOT/apps" \
BASE_CATALOG="${BASE_CATALOG:-$ROOT/releases/app-catalog.json}" \
PUBLIC_CATALOG="$ROOT/app-catalog/catalog.json" \
EMBED_MANIFESTS="${EMBED_MANIFESTS:-1}" python3 - <<'PY'
import glob
import json, os

with open(os.environ["BASE_CATALOG"], encoding="utf-8") as baseline_file:
    baseline_entries = json.load(baseline_file).get("apps", {})

try:
    import yaml
except ImportError:
    yaml = None

def img(var):
    v = os.environ.get(var)
    return v if v else None

def tag(image):
    # version = tag after the LAST colon that follows the last slash
    if not image:
        return None
    tail = image.rsplit('/', 1)[-1]
    return tail.rsplit(':', 1)[1] if ':' in tail else 'latest'

# Single-container apps: app_id -> primary image variable.
SINGLE = {
    "bitcoin-knots": "BITCOIN_KNOTS_IMAGE",
    "lnd": "LND_IMAGE",
    "electrumx": "ELECTRUMX_IMAGE",
    "bitcoin-ui": "BITCOIN_UI_IMAGE",
    "lnd-ui": "LND_UI_IMAGE",
    "electrs-ui": "ELECTRS_UI_IMAGE",
    "cuprate-ui": "CUPRATE_UI_IMAGE",
    "homeassistant": "HOMEASSISTANT_IMAGE",
    "grafana": "GRAFANA_IMAGE",
    "uptime-kuma": "UPTIME_KUMA_IMAGE",
    "jellyfin": "JELLYFIN_IMAGE",
    "photoprism": "PHOTOPRISM_IMAGE",
    "ollama": "OLLAMA_IMAGE",
    "vaultwarden": "VAULTWARDEN_IMAGE",
    "nextcloud": "NEXTCLOUD_IMAGE",
    "searxng": "SEARXNG_IMAGE",
    "filebrowser": "FILEBROWSER_IMAGE",
    "nginx-proxy-manager": "NPM_IMAGE",
    "portainer": "PORTAINER_IMAGE",
    "tailscale": "TAILSCALE_IMAGE",
    "fedimint": "FEDIMINT_IMAGE",
    "fedimint-gateway": "FEDIMINT_GATEWAY_IMAGE",
    "nostr-rs-relay": "NOSTR_RS_RELAY_IMAGE",
}

# Stack apps: app_id -> {container_name: image variable}. The FIRST entry is the
# primary (its version drives the badge); it is also emitted as `image`.
STACK = {
    "indeedhub": {
        "indeedhub": "INDEEDHUB_IMAGE",
        "indeedhub-api": "INDEEDHUB_API_IMAGE",
        "indeedhub-ffmpeg": "INDEEDHUB_FFMPEG_IMAGE",
    },
    "immich": {
        "immich_server": "IMMICH_SERVER_IMAGE",
        "immich_postgres": "IMMICH_POSTGRES_IMAGE",
        "immich_redis": "REDIS_IMAGE",
    },
    "mempool": {
        "archy-mempool-web": "MEMPOOL_WEB_IMAGE",
        "mempool-api": "MEMPOOL_BACKEND_IMAGE",
        "archy-mempool-db": "MARIADB_IMAGE",
    },
    "btcpay": {
        "btcpay-server": "BTCPAY_IMAGE",
        "archy-nbxplorer": "NBXPLORER_IMAGE",
        "archy-btcpay-db": "BTCPAY_POSTGRES_IMAGE",
    },
}

apps = {}
for app_id, var in SINGLE.items():
    image = img(var)
    if image:
        apps[app_id] = {"version": tag(image), "image": image}

for app_id, comps in STACK.items():
    images = {name: img(var) for name, var in comps.items() if img(var)}
    if not images:
        continue
    primary_name = next(iter(comps))          # first listed = primary
    primary_image = img(comps[primary_name])
    entry = {"version": tag(primary_image)}
    if primary_image:
        entry["image"] = primary_image
    entry["images"] = images
    apps[app_id] = entry

# Opt-in (EMBED_MANIFESTS): embed each app's full manifest so nodes install from
# the registry alone. The whole manifest document is embedded under `manifest`
# (top-level `app:` preserved) — that is exactly what the Rust side deserializes
# into an AppManifest. Apps not already in SINGLE/STACK get a new entry whose
# version comes from the manifest. A bad embed is harmless: the node validates and
# falls back to its disk manifest.
# Embedded manifests must name a registry the DEPLOYED fleet trusts, which is
# not necessarily the one the repo names. apps/*/manifest.yml moved to the public
# domain in 8e814ca0, but releases/registry-trust-floor.json still lists only the
# OVH host — the migration is ship-binary -> confirm-fleet -> promote-floor ->
# regenerate, and the later steps have not happened. Embedding the repo's host
# verbatim produced a catalog naming 78 untrusted refs, which would have made
# every install in the field fail with "not from a trusted registry". The signer
# refused it, which is how this was caught.
#
# So rewrite OUR registry host to whatever REGISTRY is generating against, and
# leave every other host (docker.io, ghcr.io, ...) untouched. When the floor is
# promoted, generating against the domain becomes a no-op here.
REGISTRY = os.environ.get("ARCHY_REGISTRY", "source.archipelago-foundation.org/lfg2025")

_KNOWN_ARCHY_REGISTRY_HOSTS = (
    "source.archipelago-foundation.org/lfg2025",
    "146.59.87.168:3000/lfg2025",
)


def _retarget_registry(node):
    if isinstance(node, dict):
        return {k: _retarget_registry(v) for k, v in node.items()}
    if isinstance(node, list):
        return [_retarget_registry(v) for v in node]
    if isinstance(node, str):
        for host in _KNOWN_ARCHY_REGISTRY_HOSTS:
            if host != REGISTRY and node.startswith(host + "/"):
                return REGISTRY + node[len(host):]
        return node
    return node


embedded = 0
apps_dir = os.environ.get("APPS_DIR")
if os.environ.get("EMBED_MANIFESTS") and apps_dir:
    if yaml is None:
        raise SystemExit("EMBED_MANIFESTS set but PyYAML is not available")
    for path in sorted(glob.glob(os.path.join(apps_dir, "*", "manifest.yml"))):
        with open(path) as fh:
            data = yaml.safe_load(fh)
        if not isinstance(data, dict) or not isinstance(data.get("app"), dict):
            continue
        app = data["app"]
        app_id = app.get("id")
        if not app_id:
            continue
        entry = apps.setdefault(str(app_id), {})
        entry.setdefault("version", str(app.get("version", "")) or "0")
        rendered = _retarget_registry(data)
        if data["app"].get("backup_before_runtime_change"):
            baseline = baseline_entries.get(app_id, {}).get("manifest")
            if not baseline or baseline.get("app", {}).get("backup_before_runtime_change"):
                raise SystemExit(f"{app_id}: a pre-migration BASE_CATALOG manifest is required for old-node compatibility")
            entry["manifest"] = baseline
            requires = ["runtime-migration-backup-v1"]
            if app_id == "nginx-proxy-manager":
                requires.append("npm-legacy-host-gateway-v1")
            entry["manifest_variants"] = [{
                "requires": requires, "manifest": rendered,
            }]
        else:
            entry["manifest"] = rendered
        embedded += 1

# Multi-version support (docs/bitcoin-multi-version-design.md §3 Phase 1):
# curated, bounded `versions[]` a runner may install or switch to. The entry
# marked default:true MUST equal the app's top-level catalog `version` (the
# manifest version for embedded apps) so selecting it un-pins / tracks latest.
#
# ONLY list versions whose tagged image is actually published to the registry —
# an unbuilt tag 404s on install. Extend each list as scripts/build-bitcoin-
# image.sh (Phase 0) publishes more tagged images, e.g.:
#   {"version": "30.0", "image": f"{REGISTRY}/bitcoin:30.0"},
#   {"version": "27.2", "image": f"{REGISTRY}/bitcoin:27.2", "deprecated": True, "eol": "2026-12-31"},
VERSIONS = {
    # Curated Core set (latest patch per major, current → 25). Images built +
    # verified (SHA-256 + OpenPGP, fail-closed) and pushed by
    # scripts/build-bitcoin-image.sh. `28.4.0` is the default (== the manifest's
    # top-level version) so existing/new installs are undisturbed; runners switch
    # up to 31.0 (e.g. for BIP-110 signalling) or down to 25.2 from the app's
    # "Version & Updates" card. Add the next release by building its image then
    # prepending it here.
    "bitcoin-core": [
        {"version": "latest", "image": f"{REGISTRY}/bitcoin:latest", "default": True},
        {"version": "31.0", "image": f"{REGISTRY}/bitcoin:31.0"},
        {"version": "30.2", "image": f"{REGISTRY}/bitcoin:30.2"},
        {"version": "29.3", "image": f"{REGISTRY}/bitcoin:29.3"},
        {"version": "29.2", "image": f"{REGISTRY}/bitcoin:29.2"},
        {"version": "28.4.0", "image": f"{REGISTRY}/bitcoin:28.4"},
        {"version": "27.2", "image": f"{REGISTRY}/bitcoin:27.2"},
        {"version": "26.2", "image": f"{REGISTRY}/bitcoin:26.2", "deprecated": True},
        {"version": "25.2", "image": f"{REGISTRY}/bitcoin:25.2", "deprecated": True},
    ],
    # Knots: a real tagged build is now published, so it's selectable + pinnable
    # in the Knots app interface. `latest` is the default (== the manifest's
    # floating tag) so selecting it un-pins / tracks latest — this MUST match the
    # top-level catalog version (L167-168) or the card can't reach "latest" and
    # selecting the highlighted default would instead pin+recreate. Pinning
    # 29.3.knots20260508 moves a runner off the floating tag.
    # `latest` is the default and points at the NEWEST published dated image
    # (not the bare :latest tag) so "Always use the latest version" installs the
    # newest build on fixed-binary nodes, while UNPINNED nodes still resolve via
    # the manifest's floating :latest tag (kept on the legacy image until the
    # entrypoint-render fix is fleet-deployed — see
    # the bitcoin multi-version design).
    # NO "latest" pseudo-version here, and the default is pinned. The entry
    # marked default:true used to be {"version": "latest"} pointing at
    # 29.3.knots20260508 — so a fresh install, or anyone picking "latest",
    # silently got the BIP110/RDTS build. That build HALTS until an operator
    # sets consensusrules=rdts: node 100.64.204.114 runs it and is frozen at
    # block 961,692 (blocks AND headers static, 11 peers, unpruned) while
    # reporting itself synced, whereas the nodes on 20260210 sit at the tip.
    # A default that can move across a consensus boundary is a fleet-wide
    # stall waiting to happen, so the default is an explicit, non-RDTS build
    # and moving it is a deliberate consensus decision.
    "bitcoin-knots": [
        {"version": "29.3.knots20260210",
         "image": f"{REGISTRY}/bitcoin-knots:29.3.knots20260210", "default": True},
        {"version": "29.3.knots20260508",
         "image": f"{REGISTRY}/bitcoin-knots:29.3.knots20260508"},
        {"version": "29.3.knots20260507",
         "image": f"{REGISTRY}/bitcoin-knots:29.3.knots20260507"},
        {"version": "29.2.knots20251110",
         "image": f"{REGISTRY}/bitcoin-knots:29.2.knots20251110"},
    ],
}
for app_id, versions in VERSIONS.items():
    if app_id in apps and versions:
        apps[app_id]["versions"] = versions
        # The default/latest entry MUST equal the app's top-level catalog
        # `version` (commit 169ff2e2) so selecting the highlighted default
        # un-pins / tracks latest instead of pinning+recreating. Enforce it here
        # rather than relying on the manifest version matching.
        default_entry = next((v for v in versions if v.get("default")), None)
        if default_entry:
            apps[app_id]["version"] = default_entry["version"]

catalog = {
    "schema": 1,
    "updated": os.environ["UPDATED"],
    "apps": dict(sorted(apps.items())),
}

# Storefront composition is release data, not node-OS UI code. Copy it from
# the public catalog into the signed registry artifact so popular ordering and
# promotions can change independently after the fleet supports this schema.
# The legacy `featured` block remains for older dashboards.
public_catalog_path = os.environ.get("PUBLIC_CATALOG")
if public_catalog_path:
    with open(public_catalog_path, encoding="utf-8") as fh:
        public_catalog = json.load(fh)
    for key in ("featured", "storefront"):
        if key in public_catalog:
            catalog[key] = public_catalog[key]

with open(os.environ["OUT"], "w") as f:
    json.dump(catalog, f, indent=2)
    f.write("\n")
suffix = f" (embedded {embedded} manifests)" if embedded else ""
print(f"Wrote {os.environ['OUT']} with {len(apps)} apps{suffix}")
PY
