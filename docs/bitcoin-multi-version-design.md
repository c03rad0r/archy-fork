# Bitcoin Multi-Version Support — Design


**Status:** implemented — all four phases shipped (catalog schema, install-time selection, in-app switch + auto-update toggle, and the verified image build pipeline). Downgrades are guarded: the update path never offers a lower version than what is running.
**Goal:** let a user choose *which* version of Bitcoin Core / Bitcoin Knots to
install (latest pre-selected, older versions in a dropdown), and later switch
versions or opt into auto-update — all manifest/catalog-driven, all served from
**our signed registry**, rootless, with **zero data loss** across version
changes.

See also: [`docs/registry-manifest-design.md`](registry-manifest-design.md)
(catalog distribution + signing this builds on),
and the production test gate (which must be green first).


---

## 1. Where we are today

### Image source / build
| Thing | Today |
|-------|-------|
| `apps/bitcoin-core/Dockerfile` | `FROM bitcoin/bitcoin:24.0` — a **community** image, **stale** (manifest says 28.4), no project-official Docker image exists |
| `apps/bitcoin-knots/` | **no Dockerfile** — `:latest` is built/pushed by hand |
| Registry | `scripts/image-versions.sh` → `ARCHY_REGISTRY="source.archipelago-foundation.org/lfg2025"`; only `BITCOIN_KNOTS_IMAGE=…/bitcoin-knots:latest` pinned, no Core pin |
| Tags in registry | **one tag per image**. No historical versions. |

### Version pinning
- `apps/bitcoin-core/manifest.yml` → `…/bitcoin:28.4` (pinned).
- `apps/bitcoin-knots/manifest.yml` → `…/bitcoin-knots:latest` (**floating** — a
  liability for reproducibility and for "switch back to the version I had").
- `core/archipelago/src/container/app_catalog.rs` + `app-catalog/catalog.json`:
  signed, hourly-fetched, carries `version` (badge text) + `image`.
  `catalog_image_override()` overrides the manifest image **only if same-repo**.
  `available_update_for_app()` already ignores floating tags for update
  detection.

### Install path
- `prod_orchestrator.rs::install_fresh()` resolves the image as
  **manifest image → catalog override → pull**. There is **no per-install
  version parameter** — `orchestrator.install(app_id)` takes only the id.
- RPC `package.install` (`api/rpc/package/install.rs`) *accepts* `dockerImage` /
  `version` params but for orchestrator-managed apps (bitcoin-core / bitcoin-knots
  are allowlisted) it **ignores them** and lets the orchestrator resolve.
- **Conflict guard** (`prod_orchestrator.rs` ~1306–1325): core and knots may not
  run simultaneously. Must be preserved by everything below.

### UI
- Install is **one-click, no modal** (`MarketplaceAppDetails.vue::installApp()`).
- Update badge + "Update to X" already exist (`appDetails/AppHeroSection.vue`,
  RPC `package.update`).
- **No** Bitcoin-specific settings panel; all apps share `AppSidebar.vue`.
- Per-app config persisted **only at install time** as `containerConfig` →
  `/var/lib/archipelago/app-configs/<id>.json`. **No post-install set-config RPC.**

---

## 2. Source-of-truth decision: official upstream → our registry

We use the **official releases** as upstream provenance, but nodes only ever pull
from our registry. Nodes do **not** fetch bitcoin.org / GitHub at install time —
that would break rootless/offline installs and the signed-registry trust model,
and neither project publishes an official Docker image anyway.

**Official sources (verified):**

| Impl | Index | Per-version asset pattern |
|------|-------|---------------------------|
| Bitcoin Core | [bitcoincore.org/en/releases](https://bitcoincore.org/en/releases/) · [github bitcoin/bitcoin](https://github.com/bitcoin/bitcoin/releases) | `https://bitcoincore.org/bin/bitcoin-core-<ver>/bitcoin-<ver>-x86_64-linux-gnu.tar.gz` + `SHA256SUMS` + `SHA256SUMS.asc` |
| Bitcoin Knots | [github bitcoinknots/bitcoin](https://github.com/bitcoinknots/bitcoin/releases) · [bitcoinknots.org/files](https://bitcoinknots.org/) | `https://bitcoinknots.org/files/<maj>.x/<ver>/bitcoin-<ver>-x86_64-linux-gnu.tar.gz` (`<ver>` e.g. `29.3.knots20260508`) |

Both ship **signed binary tarballs** with multi-builder Guix attestations
(`SHA256SUMS.asc`). The build pipeline verifies these **once, at build**; our DHT
Phase 0 registry signature then carries provenance to the fleet.

> Knots version strings embed a build date (`29.3.knots20260508`). Treat the full
> string as the tag; surface a friendly `29.3` + date in the UI.

---

## 3. Design

### Phase 0 — Reproducible, verified image pipeline *(prerequisite)*

New `scripts/build-bitcoin-image.sh <impl> <version>` that, per version:

1. Downloads the official tarball + `SHA256SUMS(.asc)` (GitHub release assets are
   an identical mirror → fallback).
2. Verifies SHA256 **and** the Guix/builder GPG signatures. **Fail closed.**
3. Builds a minimal **rootless** image: pin a small base, unpack
   `bitcoind`/`bitcoin-cli`. Keep the existing entrypoint probe
   (`command -v bitcoind || find /opt -path '*/bin/bitcoind'`) so per-version
   layout differences don't break startup.
4. Tags + pushes `:<version>` **and** updates the default pin (`:latest` /
   `:28.4`-style) to the registry.

**Curate, don't mirror everything.** Publish a bounded set (proposal: current +
last ~3 majors), e.g. Core `31.0, 30.0, 29.3, 28.4, 27.2` and Knots
`29.3.knots…, 28.1.knots…, 27.1.knots…`. **`log` / document dropped versions** —
silent truncation reads as "all versions supported" when it isn't.

Also fixes existing debt: replaces the stale community `FROM bitcoin/bitcoin:24.0`
and gives Knots a real Dockerfile + non-floating tags.

### Phase 1 — Version catalog (signed, registry-distributed)

Extend `AppCatalogEntry` (forward-compatible — no `deny_unknown_fields`, old nodes
ignore it):

```jsonc
"bitcoin-core": {
  "version": "31.0",                 // default / latest (existing field)
  "image": "…/bitcoin:31.0",         // existing
  "versions": [                      // NEW
    { "version": "31.0", "image": "…/bitcoin:31.0", "default": true },
    { "version": "30.0", "image": "…/bitcoin:30.0" },
    { "version": "28.4", "image": "…/bitcoin:28.4", "deprecated": true, "eol": "2026-...." }
  ]
}
```

Published to `releases/app-catalog.json`, signed by the existing release-root
mechanism. This is the **single source of truth** the UI reads for "what can I
install / switch to," and third-party-registry apps inherit the capability for
free. `version`/`image` stay as the default for back-compat.

### Phase 2 — Install-time version selection

- **Orchestrator:** add `install_with_image(app_id, Option<image_tag>)` (or an
  optional arg on `install`). When a tag is supplied, **validate same-repo**
  against the manifest (reuse `image_without_registry_or_tag()`), then override in
  `install_fresh()`. Default path unchanged. Preserve the core/knots conflict
  guard.
- **RPC:** thread the selected version/image from `package.install` into the
  orchestrator for the allowlisted apps (the param is already received — just not
  forwarded).
- **UI:** the first **install modal** in the app — latest pre-selected, dropdown
  of `versions[]`, deprecated/EOL badges on old entries. On confirm, pass the
  chosen version to `package.install`.

### Phase 3 — In-app version switch + auto-update toggle

- **UI:** a Bitcoin **"Version & Updates"** card (conditional in `AppSidebar.vue`
  for `bitcoin-core` / `bitcoin-knots`): current version, a switch dropdown, and
  an **auto-update-to-latest** toggle.
- **Switch = controlled re-pull/recreate** reusing the `package.update`
  machinery but targeting an arbitrary (incl. older) tag → effectively
  `package.set-version`.
- **Persistence:** new `package.set-config` RPC writing the existing
  `app-configs/<id>.json` (`{ pinnedVersion, autoUpdate }`).
- **Auto-update:** the existing hourly catalog check, when `autoUpdate:true`,
  triggers `package.update` to the catalog default. A pinned version **suppresses
  the update badge**.

---

## 4. Invariants & safety rails

- **Rootless only.** Pipeline images and run path stay rootless; no Docker-socket,
  no privileged.
- **No data loss across version change.** Preserve `/var/lib/archipelago/bitcoin`,
  secrets (`bitcoin-rpc-password`, `…-rpcauth`), ports, and the adoption container
  name on every install / switch / update.
- **⚠️ Downgrade vs. chainstate (highest risk).** Bitcoin Core refuses to start on
  a chainstate written by a *newer* version unless reindexed (expensive, or data
  loss on a pruned node). The UI **must** warn loudly on downgrade; the
  orchestrator should gate/confirm it and never silently wipe. Pruned nodes can't
  simply `-reindex`.
- **Core ⇄ Knots switch** stays governed by the existing conflict guard; treat an
  impl switch as distinct from a version switch.
- **Floating tags** (`latest`) are never advertised as a selectable "version" and
  never counted as an available update (already handled by
  `available_update_for_app`).
- **Verify on a real node** and pass the lifecycle gate before any
  tag.

---

## 5. Files / seams (no code yet)

| Concern | File |
|---------|------|
| Image build/push | new `scripts/build-bitcoin-image.sh`; `apps/bitcoin-core/Dockerfile`; new `apps/bitcoin-knots/Dockerfile`; `scripts/image-versions.sh` |
| Catalog schema | `core/archipelago/src/container/app_catalog.rs`; `releases/app-catalog.json` (+ `app-catalog/catalog.json`) |
| Install override | `core/archipelago/src/container/prod_orchestrator.rs` (`install` / `install_fresh`); `api/rpc/package/install.rs`; `api/rpc/dispatcher.rs` |
| Switch / set-config RPC | `api/rpc/package/update.rs`; new `package.set-config` handler; `app-configs/<id>.json` |
| Install modal | `neode-ui/src/views/MarketplaceAppDetails.vue`; new `…/marketplace/AppInstallModal.vue` |
| Version & Updates card | `neode-ui/src/views/appDetails/AppSidebar.vue`; `neode-ui/src/api/rpc-client.ts`; `neode-ui/src/types/api.ts` |

---

## 6. Open questions

1. **Curated version set** — how many majors back do we host, and storage budget
   on the registry?
2. **Multi-arch** — fleet is x86_64 today; do any nodes need arm64 images?
3. **Pruned-node downgrade policy** — block outright, or allow with an explicit
   "this will require re-sync / may lose pruned data" confirmation?
4. **Auto-update default** — off (opt-in) for a consensus-critical app like
   Bitcoin? (Recommended: **off**, explicit opt-in.)
5. **Knots date-suffix UX** — how to display `29.3.knots20260508` cleanly.

---

## Sources

- [Bitcoin Core releases](https://bitcoincore.org/en/releases/)
- [bitcoin/bitcoin releases](https://github.com/bitcoin/bitcoin/releases)
- [bitcoinknots/bitcoin releases](https://github.com/bitcoinknots/bitcoin/releases)
- [Bitcoin Knots](https://bitcoinknots.org/)
- [bitcoin.org version history](https://bitcoin.org/en/version-history)
