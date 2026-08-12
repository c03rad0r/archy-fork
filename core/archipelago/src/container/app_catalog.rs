//! Remote app version catalog — DECOUPLES per-app updates from the binary OTA.
//!
//! Background: `image_versions.rs` reads the pinned image tags from
//! `image-versions.sh`, which is deployed *with the archipelago binary*. That
//! coupled every app update to a full node release. This module adds a remote
//! catalog (`app-catalog.json`) fetched over HTTP from the same origin as the
//! OTA manifest, refreshed periodically and on demand. Bumping an app's version
//! is then a JSON edit + push — no binary release.
//!
//! Resolution order (origin-always-wins, matching the DHT design's posture):
//!   1. Remote catalog (this module) — the live source of "available update".
//!   2. `image-versions.sh` pin — offline/baseline fallback when the catalog is
//!      missing or doesn't cover the app.
//!
//! ## Forward-compatibility with the DHT distribution plan
//! (`docs/dht-distribution-design.md`)
//! This catalog IS the "discovery / authenticity" layer of that plan. The schema
//! is deliberately extensible so the later phases bolt on WITHOUT a breaking
//! change:
//!   - `signature` / `signed_by` (top level) — Phase 0 seed-derived release-root
//!     signature over the canonical JSON. Absent today; verified when present.
//!   - per-image `digest` / `size` — BLAKE3/SHA-256 content address + length, so
//!     the iroh swarm can fetch images by hash with the registry as origin.
//! Unknown fields are ignored (no `deny_unknown_fields`), so adding fields on the
//! publisher side never breaks older nodes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;
use tracing::{debug, info, warn};

/// Filename for both the published catalog and the on-node cache.
pub const APP_CATALOG_FILE: &str = "app-catalog.json";

/// Cache of the parsed catalog, invalidated when the cache file mtime changes.
static CACHE: Mutex<Option<CacheEntry>> = Mutex::new(None);

struct CacheEntry {
    mtime: SystemTime,
    catalog: AppCatalog,
}

/// Top-level catalog document.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppCatalog {
    /// Schema version. 1 = current. Bump only on incompatible changes.
    #[serde(default)]
    pub schema: u32,
    /// Publish date (RFC 3339 or YYYY-MM-DD). Informational.
    #[serde(default)]
    pub updated: String,
    /// app_id -> entry.
    #[serde(default)]
    pub apps: HashMap<String, AppCatalogEntry>,
    /// DHT-plan forward-compat: detached signature over the canonical JSON,
    /// produced by the seed-derived release-root key. Absent today.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// DHT-plan forward-compat: publisher identity (did:key / npub).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_by: Option<String>,
}

/// Per-app catalog entry.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppCatalogEntry {
    /// User-facing version string (drives the "Update available" badge text).
    pub version: String,
    /// Primary single-container image reference (`registry/repo:tag`). For stack
    /// apps this is the primary container's image (the one whose version the
    /// badge tracks — e.g. the IndeeHub frontend).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Stack apps only: container_name -> image reference. Components omitted here
    /// fall back to the `image-versions.sh` pin during an update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<HashMap<String, String>>,
    /// DHT-plan forward-compat: content address of the primary image (unused now).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    /// DHT-plan forward-compat: size in bytes of the primary image (unused now).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Optional human-readable changelog lines for this version.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changelog: Vec<String>,
    /// Multi-version support (`docs/bitcoin-multi-version-design.md`): the bounded
    /// set of versions a user may install or switch to for this app. Empty for
    /// single-version apps; `version`/`image` above remain the default/latest for
    /// back-compat. Old nodes ignore this field (no `deny_unknown_fields`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<CatalogVersion>,
    /// Full app manifest, embedded so the app installs from the registry alone —
    /// no OTA-shipped `apps/<id>/manifest.yml`. Carried as the raw value the
    /// publisher signed (so it stays part of the verified preimage) and
    /// deserialized into an `AppManifest` by the orchestrator at load time, where
    /// it overrides the disk manifest (origin-wins). Absent during the migration
    /// window => the node falls back to the disk manifest. See
    /// `docs/registry-manifest-design.md`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<serde_json::Value>,
}

/// One selectable version in an app's `versions[]` list. The catalog carries a
/// curated, bounded set (current + a few majors back); see
/// `docs/bitcoin-multi-version-design.md` §3 Phase 1.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CatalogVersion {
    /// User-facing + tag-matching version string (e.g. `31.0`,
    /// `29.3.knots20260508`). Treated as the image tag.
    pub version: String,
    /// Concrete image reference for this version. When omitted the orchestrator
    /// falls back to composing `<default-repo>:<version>` from the entry image.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Marks the default / latest version pre-selected in the install modal.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default: bool,
    /// Deprecated versions are still installable but badged in the UI.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
    /// Optional end-of-life date (YYYY-MM-DD), surfaced in the UI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eol: Option<String>,
}

/// Read-side cache file search order. Mirrors `image_versions.rs`: the running
/// daemon's data dir first (via env for dev), then the canonical runtime path.
fn cache_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(dir) = std::env::var("ARCHIPELAGO_DATA_DIR") {
        paths.push(Path::new(&dir).join(APP_CATALOG_FILE));
    }
    paths.push(Path::new("/var/lib/archipelago").join(APP_CATALOG_FILE));
    paths
}

fn find_cache_file() -> Option<(PathBuf, SystemTime)> {
    for p in cache_paths() {
        if let Ok(meta) = p.metadata() {
            if let Ok(mtime) = meta.modified() {
                return Some((p, mtime));
            }
        }
    }
    None
}

/// Load and cache the on-node catalog. Returns an empty catalog when absent —
/// callers then fall back to `image-versions.sh`.
fn load_catalog() -> AppCatalog {
    let (path, mtime) = match find_cache_file() {
        Some(v) => v,
        None => return AppCatalog::default(),
    };

    {
        let cache = CACHE.lock().unwrap();
        if let Some(ref entry) = *cache {
            if entry.mtime == mtime {
                return entry.catalog.clone();
            }
        }
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            debug!("app-catalog: failed to read {}: {}", path.display(), e);
            return AppCatalog::default();
        }
    };
    let catalog: AppCatalog = match serde_json::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            warn!("app-catalog: invalid JSON at {}: {}", path.display(), e);
            return AppCatalog::default();
        }
    };

    {
        let mut cache = CACHE.lock().unwrap();
        *cache = Some(CacheEntry {
            mtime,
            catalog: catalog.clone(),
        });
    }
    catalog
}

fn entry_for(app_id: &str) -> Option<AppCatalogEntry> {
    load_catalog().apps.get(app_id).cloned()
}

/// Primary image for an app per the remote catalog, if covered.
pub fn catalog_primary_image(app_id: &str) -> Option<String> {
    entry_for(app_id).and_then(|e| e.image)
}

/// Per-container stack image overrides from the catalog (container_name -> image).
pub fn catalog_stack_images(app_id: &str) -> HashMap<String, String> {
    entry_for(app_id).and_then(|e| e.images).unwrap_or_default()
}

/// All `(app_id, manifest-value)` pairs the registry catalog carries. The
/// orchestrator deserializes + validates each into an `AppManifest` and prefers
/// it over the disk manifest (origin-wins); disk remains the migration fallback.
/// Empty when the catalog is absent or no entry embeds a manifest.
pub fn catalog_manifest_values() -> Vec<(String, serde_json::Value)> {
    load_catalog()
        .apps
        .into_iter()
        .filter_map(|(id, e)| e.manifest.map(|m| (id, m)))
        .collect()
}

/// A catalog-embedded manifest as the node actually applies it: parsed,
/// id-checked, validated, and image-only (build-source manifests defer to
/// disk). `None` = the caller must fall back to the disk manifest.
///
/// Shared between the orchestrator's load overlay and the app gate's port
/// classification so both answer "which manifest governs this app?" from the
/// same origin. They diverged once — the orchestrator published containers
/// from the catalog while the gate classified from stale disk manifests, and
/// the gate externally bound a port the catalog had declared `auth: local`
/// (nbxplorer 32838, a test node 2026-08-04).
pub fn catalog_manifest_overlay(
    app_id: &str,
    value: serde_json::Value,
) -> Option<archipelago_container::manifest::AppManifest> {
    let m: archipelago_container::manifest::AppManifest = match serde_json::from_value(value) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(app = %app_id, error = %e,
                "skipping unparseable catalog manifest; using disk fallback");
            return None;
        }
    };
    if m.app.id != app_id {
        tracing::warn!(catalog_id = %app_id, manifest_id = %m.app.id,
            "skipping catalog manifest: embedded app id mismatches catalog key");
        return None;
    }
    if let Err(e) = m.validate() {
        tracing::warn!(app = %app_id, error = %e,
            "skipping invalid catalog manifest; using disk fallback");
        return None;
    }
    if m.app.container.build.is_some() {
        tracing::debug!(app = %app_id,
            "catalog manifest has a build source; deferring to disk (phase 1 = image-only)");
        return None;
    }
    Some(m)
}

/// Like [`catalog_manifest_overlay`] but WITHOUT the build-source refusal —
/// for PORT CLASSIFICATION only, never for install/orchestration.
///
/// The on-node-built companion UIs (lnd-ui, bitcoin-ui, electrs-ui, fips-ui)
/// are exactly the apps whose port policy (auth/bind/session_passthrough)
/// must reach the gate reliably, yet their build sources made the overlay
/// defer to DISK manifests — whose only delivery paths (frontend runtime
/// payload, per-node repo copies) proved stale or absent across the fleet in
/// the v1.7.125 rollout: nodes served ungated UIs or 401-dead panels until
/// hand-fixed. The signed catalog is fresher and operator-signed; and the
/// gate's address binds fail safely on conflict with a container that
/// publishes differently (logged as CANNOT PROTECT), so classifying from the
/// catalog cannot open anything the running container hasn't already opened.
pub fn catalog_manifest_ports_overlay(
    app_id: &str,
    value: serde_json::Value,
) -> Option<archipelago_container::manifest::AppManifest> {
    let m: archipelago_container::manifest::AppManifest = serde_json::from_value(value).ok()?;
    if m.app.id != app_id {
        return None;
    }
    if m.validate().is_err() {
        return None;
    }
    Some(m)
}

/// The catalog's default/latest version string for an app (the top-level
/// `version` field), if covered. Used to decide whether an install-time
/// selection should pin (older) or track-latest (default).
pub fn catalog_default_version(app_id: &str) -> Option<String> {
    entry_for(app_id)
        .map(|e| e.version)
        .filter(|v| !v.is_empty())
}

/// Curated, selectable versions for an app per the remote catalog. Empty when
/// the catalog is absent or the app is single-version. The default entry (if
/// any) sorts first so callers can pre-select it.
pub fn catalog_versions(app_id: &str) -> Vec<CatalogVersion> {
    let mut versions = entry_for(app_id).map(|e| e.versions).unwrap_or_default();
    versions.sort_by_key(|v| !v.default); // default first, stable otherwise
    versions
}

/// Resolve the image for a specific selectable `version` of `app_id`, validated
/// same-repo against `manifest_image` (the same guard `catalog_image_override`
/// applies). The version's explicit `image` is used when present; otherwise the
/// repo of `manifest_image` is retagged with `version`. Returns `None` when the
/// version is unknown or would point at a different repository — the caller then
/// keeps the default resolution and the switch is refused upstream.
pub fn catalog_image_for_version(
    app_id: &str,
    version: &str,
    manifest_image: &str,
) -> Option<String> {
    let entry = catalog_versions(app_id)
        .into_iter()
        .find(|v| v.version == version)?;
    let manifest_repo =
        crate::container::image_versions::image_without_registry_or_tag(manifest_image);
    let candidate = match entry.image {
        Some(img) => img,
        None => {
            // Retag the manifest's full registry/repo with the requested version.
            let repo = manifest_image
                .rsplit_once(':')
                // keep registry:port colons intact: only strip a tag after the last '/'
                .filter(|(left, _)| left.contains('/'))
                .map(|(left, _)| left)
                .unwrap_or(manifest_image);
            format!("{repo}:{version}")
        }
    };
    let same_repo = crate::container::image_versions::image_without_registry_or_tag(&candidate)
        == manifest_repo;
    if same_repo {
        Some(candidate)
    } else {
        warn!(
            "app-catalog: ignoring version {} for {} — repo mismatch (candidate={}, manifest={})",
            version, app_id, candidate, manifest_image
        );
        None
    }
}

/// Image override for the orchestrator's install/upgrade path. Returns the
/// catalog's primary image for `app_id` ONLY when it refers to the same
/// repository as the manifest's current image — a guard so a catalog typo can
/// never redirect an app to an unrelated image. `None` means "use the manifest
/// image as-is" (catalog absent, app uncovered, or repo mismatch).
pub fn catalog_image_override(app_id: &str, manifest_image: &str) -> Option<String> {
    let candidate = catalog_primary_image(app_id)?;
    let same_repo = crate::container::image_versions::image_without_registry_or_tag(&candidate)
        == crate::container::image_versions::image_without_registry_or_tag(manifest_image);
    if same_repo {
        Some(candidate)
    } else {
        warn!(
            "app-catalog: ignoring image for {} — repo mismatch (catalog={}, manifest={})",
            app_id, candidate, manifest_image
        );
        None
    }
}

/// Decoupled "available update" check for ALL apps.
///
/// Prefers the remote catalog; when the catalog covers the app, its verdict is
/// authoritative (so we never advertise a stale `image-versions.sh` pin over a
/// newer catalog, nor vice-versa). Falls back to the deployed pin only when the
/// catalog is missing or doesn't cover the app.
pub fn available_update_for_app(app_id: &str, running_image: &str) -> Option<String> {
    // A runner-pinned version is an explicit "stay here" choice — never advertise
    // an update over it (design §3 Phase 3). Auto-update, when enabled, ignores
    // the pin and is driven by the catalog tick, not this badge.
    if crate::container::version_config::pinned_version(app_id).is_some() {
        return None;
    }
    if let Some(catalog_image) = catalog_primary_image(app_id) {
        // Catalog covers this app with a concrete image -> authoritative.
        return crate::container::image_versions::available_update_for_images(
            &catalog_image,
            running_image,
        );
    }
    // Not covered by the catalog -> baseline pin from image-versions.sh.
    crate::container::image_versions::available_update_for_app(app_id, running_image)
}

/// Derive candidate catalog URLs from the OTA mirror list by swapping the
/// manifest filename for the catalog filename. Falls back to the default
/// manifest origin when no mirrors are configured.
fn catalog_urls_from_mirrors(mirrors: &[crate::update::UpdateMirror]) -> Vec<String> {
    let mut urls: Vec<String> = mirrors
        .iter()
        .filter_map(|m| {
            // mirror.url ends with ".../releases/manifest.json"
            if m.url.ends_with("manifest.json") {
                Some(m.url.replace("manifest.json", APP_CATALOG_FILE))
            } else {
                None
            }
        })
        .collect();
    urls.dedup();
    urls
}

/// Outcome of [`refresh_catalog`]: the app count of the fetched catalog and
/// whether the cached bytes actually changed. `changed` drives the manifest-
/// overlay reload — catalog manifests only take effect once `load_manifests`
/// re-runs, and reloading on every unchanged hourly poll would be pure churn.
#[derive(Debug, Clone, Copy)]
pub struct CatalogRefresh {
    pub apps: usize,
    pub changed: bool,
}

/// Fetch the catalog from the first reachable mirror and atomically write it to
/// `<data_dir>/app-catalog.json`. Returns the app count and whether the cache
/// changed. Best-effort: a fetch failure leaves the existing cache untouched
/// (origin-always-wins; updates simply aren't refreshed this cycle).
pub async fn refresh_catalog(data_dir: &Path) -> anyhow::Result<CatalogRefresh> {
    let mirrors = crate::update::load_mirrors(data_dir)
        .await
        .unwrap_or_default();
    let urls = catalog_urls_from_mirrors(&mirrors);
    if urls.is_empty() {
        debug!("app-catalog: no mirror-derived URLs to fetch from");
        return Ok(CatalogRefresh {
            apps: 0,
            changed: false,
        });
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()?;

    let mut last_err: Option<anyhow::Error> = None;
    for url in &urls {
        match fetch_one(&client, url).await {
            Ok((catalog, body)) => {
                let count = catalog.apps.len();
                let changed = write_cache(data_dir, &body)?;
                if changed {
                    // Invalidate the in-process cache so the next read re-parses.
                    *CACHE.lock().unwrap() = None;
                }
                info!(
                    "app-catalog: refreshed from {} ({} apps{})",
                    url,
                    count,
                    if changed { ", changed" } else { ", unchanged" }
                );
                return Ok(CatalogRefresh {
                    apps: count,
                    changed,
                });
            }
            Err(e) => {
                debug!("app-catalog: fetch {} failed: {}", url, e);
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no catalog mirrors reachable")))
}

async fn fetch_one(client: &reqwest::Client, url: &str) -> anyhow::Result<(AppCatalog, String)> {
    let resp = client.get(url).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }
    let body = resp.text().await?;
    let catalog: AppCatalog = serde_json::from_str(&body)?;

    // DHT Phase 0 authenticity: verify the release-root signature when present.
    // We verify against the raw JSON (the exact bytes the publisher signed),
    // not a re-serialization of the typed struct, so unknown forward-compat
    // fields stay part of the signed preimage. Unsigned catalogs are still
    // accepted during the migration window — same trust level as today's
    // manifest — but a *present* signature that fails is a hard reject so a
    // tampering mirror cannot pass off altered bytes.
    let raw: serde_json::Value = serde_json::from_str(&body)?;
    match crate::trust::verify_detached(&raw)? {
        crate::trust::SignatureStatus::Unsigned => {
            debug!("app-catalog: unsigned (accepted during migration window)");
        }
        crate::trust::SignatureStatus::Verified {
            signer_did,
            anchored,
        } => {
            if anchored {
                info!(
                    "app-catalog: release-root signature verified ({})",
                    signer_did
                );
            } else {
                warn!(
                    "app-catalog: signature self-consistent but release-root anchor \
                     not pinned ({}); cannot confirm signer identity",
                    signer_did
                );
            }
        }
    }

    Ok((catalog, body))
}

/// Atomically write the catalog cache. Caches the RAW fetched bytes — not a
/// re-serialization of the typed struct — for two reasons: the struct's
/// `apps` HashMap serializes in nondeterministic order (a re-serialized
/// comparison would report "changed" on every poll), and the raw bytes are
/// the signed preimage, so the cache stays signature-verifiable. Returns
/// `false` (skipping the write) when the bytes are identical to what's
/// already cached, so callers can tell a genuine catalog change from an
/// unchanged poll.
fn write_cache(data_dir: &Path, body: &str) -> anyhow::Result<bool> {
    let dest = data_dir.join(APP_CATALOG_FILE);
    if std::fs::read_to_string(&dest)
        .map(|current| current == body)
        .unwrap_or(false)
    {
        return Ok(false);
    }
    let tmp = data_dir.join(format!("{}.tmp", APP_CATALOG_FILE));
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, &dest)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_ignores_unknown_fields() {
        let json = r#"{
            "schema": 1,
            "updated": "2026-06-16",
            "future_field": "ignored",
            "signature": "sig123",
            "signed_by": "did:key:zABC",
            "apps": {
                "indeedhub": {
                    "version": "1.0.1",
                    "image": "source.archipelago-foundation.org/lfg2025/indeedhub:1.0.1",
                    "digest": "blake3:deadbeef",
                    "size": 12345,
                    "another_future_field": true
                }
            }
        }"#;
        let cat: AppCatalog = serde_json::from_str(json).unwrap();
        assert_eq!(cat.schema, 1);
        assert_eq!(cat.signature.as_deref(), Some("sig123"));
        let e = cat.apps.get("indeedhub").unwrap();
        assert_eq!(e.version, "1.0.1");
        assert_eq!(
            e.image.as_deref(),
            Some("source.archipelago-foundation.org/lfg2025/indeedhub:1.0.1")
        );
        assert_eq!(e.digest.as_deref(), Some("blake3:deadbeef"));
    }

    #[test]
    fn write_cache_reports_changed_only_on_new_bytes() {
        // The changed flag gates the runtime manifest-overlay reload: an
        // unchanged hourly poll must NOT report changed (or every tick would
        // rebuild the manifest map), while a genuinely new catalog must. Raw
        // fetched bytes are compared — a re-serialized comparison would flap
        // on the apps HashMap's nondeterministic key order (seen live on .228).
        let dir = tempfile::tempdir().unwrap();
        let body = r#"{"schema":1,"apps":{"demo":{"version":"1.0.0"}}}"#;
        assert!(
            write_cache(dir.path(), body).unwrap(),
            "first write is a change"
        );
        assert!(
            !write_cache(dir.path(), body).unwrap(),
            "identical rewrite is not a change"
        );
        let body2 = r#"{"schema":1,"apps":{"demo":{"version":"1.0.1"}}}"#;
        assert!(
            write_cache(dir.path(), body2).unwrap(),
            "new catalog bytes are a change"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join(APP_CATALOG_FILE)).unwrap(),
            body2,
            "cache holds the raw signed preimage bytes"
        );
    }

    #[test]
    fn entry_carries_embedded_manifest() {
        let json = r#"{
            "schema": 1,
            "apps": {
                "demo": {
                    "version": "1.0.0",
                    "manifest": {
                        "app": {
                            "id": "demo",
                            "name": "Demo",
                            "version": "1.0.0",
                            "container": { "image": "registry/demo:1.0.0" }
                        }
                    }
                }
            }
        }"#;
        let cat: AppCatalog = serde_json::from_str(json).unwrap();
        let e = cat.apps.get("demo").unwrap();
        let m = e.manifest.as_ref().expect("manifest present");
        assert_eq!(m["app"]["id"], "demo");
    }

    #[test]
    fn empty_catalog_when_absent_is_default() {
        let cat = AppCatalog::default();
        assert!(cat.apps.is_empty());
        assert!(cat.signature.is_none());
    }

    #[test]
    fn catalog_url_derived_from_mirror() {
        let mirrors = vec![crate::update::UpdateMirror {
            url: "https://source.archipelago-foundation.org/lfg2025/archy/raw/branch/main/releases/manifest.json"
                .to_string(),
            label: "Server 1".to_string(),
        }];
        let urls = catalog_urls_from_mirrors(&mirrors);
        assert_eq!(
            urls,
            vec![
                "https://source.archipelago-foundation.org/lfg2025/archy/raw/branch/main/releases/app-catalog.json"
                    .to_string()
            ]
        );
    }
}
