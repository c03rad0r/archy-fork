# Registry-Distributed App Manifests — Design

**Status:** implemented — Phases 1–3 shipped (schema + catalog-wins overlay,
signed publisher generator with embedded manifests for all apps, immich
end-to-end via `install_stack_via_orchestrator`); Phases 4–5 (build-context
apps content-addressed, drop `apps/` from OTA) remain open. Updated 2026-07-08.
**Goal (north-star):** every app installs from a manifest distributed via the
signed app-catalog on the registry — **no OS-level code reliance, no
OTA-shipped disk manifest required**. Rootless, signed, robust, reboot-survivable.

See also: [`docs/dht-distribution-design.md`](dht-distribution-design.md) (this is
its "discovery/authenticity" layer).

---

## 1. Where we started (the pre-Phase-1 baseline)

This section is the problem statement the design was written against, kept for
context. **It no longer describes the running system** — Phases 1–3 shipped, so
see "Where we are now" below.

Two distinct mechanisms, only one of which was registry-distributed:

| Thing | Source | Reaches node via | Carried |
|-------|--------|------------------|---------|
| `apps/*/manifest.yml` | repo working tree | **OTA**: `self-update.sh` rsyncs `apps/ → /opt/archipelago/apps/` | full manifest (the orchestrator's real source of truth) |
| `app-catalog.json` | `releases/app-catalog.json` | **registry HTTP fetch**, hourly, **signed** (`app_catalog::refresh_catalog`) | version + image override only |

### Where we are now

`releases/app-catalog.json` carries 66 entries, and 56 of them embed a full
`manifest` block — one for every `apps/*/manifest.yml` in the tree. So the
"catalog carries an image override only" gap below is closed for image-only
apps; what remains is build-context apps (Phase 4) and dropping `apps/` from the
OTA rsync (Phase 5).

- Orchestrator registry = in-memory `state.manifests: HashMap<app_id, LoadedManifest>`,
  populated by `ProdContainerOrchestrator::load_manifests()` walking the disk dir.
  `install(app_id)` → `loaded(app_id)` → "unknown app_id" if absent.
- `app_catalog.rs` is already: signed (release-root, `trust::verify_detached` over
  the raw JSON), mirror-derived URLs, atomic cache at `<data_dir>/app-catalog.json`,
  **forward-compatible** (no `deny_unknown_fields` — adding fields never breaks old nodes).

**Gap:** the manifest itself is never registry-distributed. Every app — btcpay,
grafana, immich — depends on an OTA-shipped disk file. That is the OS-level
reliance to eliminate.

## 2. Target

The signed catalog entry carries the **full manifest**. The orchestrator loads
manifests from the catalog cache (origin), falling back to disk only during the
migration window. Publishing an app = editing the catalog + signing + push — no
binary OTA, no disk manifest.

```
publisher: apps/*/manifest.yml  ──generate──▶  releases/app-catalog.json (embeds + signs)
node:      refresh_catalog() ──fetch+verify──▶ <data_dir>/app-catalog.json
           load_manifests()  ──merge──▶ state.manifests   (catalog wins; disk = fallback)
           install(app_id)   ──▶ create the rootless container (Quadlet unit when
                                 use_quadlet_backends is on; podman create+start otherwise)
```

## 3. Schema change (`app_catalog::AppCatalogEntry`)

Add one optional, forward-compatible field:

```rust
/// Full app manifest, embedded so the app installs from the registry alone
/// (no OTA-shipped disk file). Carried as the raw value the publisher signed;
/// deserialized into `AppManifest` at load time. Absent during migration =>
/// the node uses the disk manifest fallback.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub manifest: Option<serde_json::Value>,
```

Why `serde_json::Value`, not `AppManifest`:
- keeps the **signed preimage** intact (we verify over the raw JSON bytes; a typed
  round-trip could drop/reorder unknown fields and break the signature),
- decouples catalog schema from manifest schema churn,
- deserialize + `validate()` happens at orchestrator load, exactly like `from_file`.

Authenticity is **free**: `fetch_one` already verifies the release-root signature
over the whole document, so an embedded manifest is covered by the same signature.
A present-but-bad signature is already a hard reject.

## 4. Orchestrator load path (`load_manifests`)

Extend (not replace) the disk walk:

1. Load disk manifests as today → `disk: HashMap<app_id, LoadedManifest>`.
2. Load catalog manifests from the cache: for each entry with `manifest: Some(v)`,
   `serde_json::from_value::<AppManifest>(v)` then `validate()`; on success build a
   `LoadedManifest { manifest, manifest_dir }`.
3. **Merge, catalog-wins**: a catalog manifest overrides the disk one for the same
   `app_id`. Disk remains the fallback for apps the catalog doesn't cover (migration).
   - Rationale: the registry is the authoritative origin; disk is the legacy
     transport we're retiring. This matches `app_catalog`'s "catalog verdict is
     authoritative when it covers the app" posture.
4. A catalog manifest that fails parse/validate is logged and skipped → disk
   fallback used (one bad entry never blocks the fleet, same as the disk walk).

### `manifest_dir` for registry manifests — IMPLEMENTED

`LoadedManifest.manifest_dir` is used **only** in the `ResolvedSource::Build` branch
(relative `container.build.context` resolution — two call sites). Image-only apps
(`ResolvedSource::Pull`) never read it.

**Decision (phase 1, shipped):** keep `manifest_dir: PathBuf` (no `Option` ripple
through the codebase). A catalog manifest with a **build source is skipped** so its
disk manifest stays in effect — build contexts aren't registry-distributed until a
later phase (content-addressed, per the DHT plan). For an accepted (image-only)
catalog manifest, `manifest_dir` = the disk app dir if the app also exists on disk,
else a sentinel `<manifests_dir>/<app_id>` (never read for image-only apps).

This is enforced by `catalog_manifest_to_overlay(app_id, value) -> Option<AppManifest>`
in `prod_orchestrator.rs`, which returns `None` (→ disk fallback) for: unparseable
value, embedded-id ≠ catalog-key, failed `validate()`, or a build source.

## 5. Publishing (publish-side generator)

Add a generator (extend `create-release.sh` / a small `scripts/gen-app-catalog`):
- walk `apps/*/manifest.yml`, parse, embed each as the entry's `manifest` (JSON),
- keep `version`/`image`/`images` derived from the manifest for the badge path,
- write `releases/app-catalog.json`, then **sign** with the existing release-root
  ceremony (`archipelago ceremony` / Phase 0 seed). Unsigned still accepted in the
  migration window.

## 6. Migration & rollback

- **Backward compatible**: old nodes ignore the new `manifest` field (no
  `deny_unknown_fields`) and keep using disk manifests.
- **Forward**: new nodes prefer catalog manifests, disk as fallback. Once the
  catalog covers every app and is verified live, drop `apps/` from the OTA rsync.
- **Rollback**: delete `<data_dir>/app-catalog.json` (or revert the published
  catalog) → nodes fall back to disk manifests. No data touched.

## 7. Phases

1. ✅ **Schema + load merge** (this design): `manifest` field, `load_manifests`
   catalog-wins merge, unit tests (catalog overrides disk; bad catalog
   manifest → disk fallback; absent → disk); `manifest_dir` stayed a plain
   `PathBuf` (see §4). Image-only apps.
2. ✅ **Publisher generator + signing**: `releases/app-catalog.json` embeds a
   full `manifest` block per app (all disk manifests covered) and is verified
   via the release-root detached signature.
3. ✅ **First real app end-to-end**: immich installs via
   `install_stack_via_orchestrator` with `generated_secrets`; the
   `install_immich_stack` name survives only as an orchestrator-first wrapper.
4. ⏳ **Build-context apps**: content-addressed build contexts in the catalog (DHT
   swarm fetch) so companions stop needing disk too.
5. ⏳ **Drop `apps/` from OTA** once coverage + live verification complete.

## 8. Open questions

- Do we embed manifests inline or reference them by content hash (BLAKE3) with a
  separate signed blob? Inline is simplest for Phase 1; hashing aligns with the
  DHT image-by-digest plan and keeps the catalog small. Lean inline now, revisit
  at Phase 4 when build contexts (large) need addressing anyway.
- ~~`generated_files` with inline content (vs. source-dir) — already supported in
  the manifest schema?~~ **Answered: yes.** `app.files[]` takes inline `content`
  (with `{{HOST_IP}}` / `{{NETWORK_GATEWAY}}` / `{{secret:NAME}}` rendering), so
  registry manifests already carry small rendered files inline and that disk
  dependency is gone. See [`app-manifest-spec.md`](app-manifest-spec.md).
