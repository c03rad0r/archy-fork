//! Parser for image-versions.sh — single source of truth for pinned container images.
//!
//! Reads the deployed file at /opt/archipelago/scripts/image-versions.sh (the canonical
//! location installed by the image-recipe) with fallbacks for older layouts and the
//! repo-local scripts/image-versions.sh for development runs from the repo root.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;
use tracing::debug;

/// Cached parse result, invalidated when file mtime changes.
static CACHE: Mutex<Option<CacheEntry>> = Mutex::new(None);

struct CacheEntry {
    mtime: SystemTime,
    images: HashMap<String, String>,
}

/// File search order — canonical production path first, older layout second,
/// repo-local for dev last. The canonical deployed path is
/// /opt/archipelago/scripts/image-versions.sh; earlier builds put it directly
/// in /opt/archipelago/, so that path is kept as a fallback for not-yet-updated
/// nodes. The repo-relative entry matches `cargo run` from the repo root.
const PATHS: &[&str] = &[
    "/opt/archipelago/scripts/image-versions.sh",
    "/opt/archipelago/image-versions.sh",
    "/home/archipelago/Projects/archy/scripts/image-versions.sh",
    "scripts/image-versions.sh",
];

/// Parse image-versions.sh and return map of variable names to full image refs.
/// Result is cached and only re-parsed when the file's mtime changes.
fn load_image_versions() -> HashMap<String, String> {
    let (path, mtime) = match find_file() {
        Some(v) => v,
        None => {
            debug!("image-versions.sh not found in any search path");
            return HashMap::new();
        }
    };

    // Check cache
    {
        let cache = CACHE.lock().unwrap();
        if let Some(ref entry) = *cache {
            if entry.mtime == mtime {
                return entry.images.clone();
            }
        }
    }

    // Parse fresh
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            debug!("Failed to read {}: {}", path, e);
            return HashMap::new();
        }
    };

    let images = parse_image_versions(&content);
    debug!("Parsed {} image versions from {}", images.len(), path);

    // Update cache
    {
        let mut cache = CACHE.lock().unwrap();
        *cache = Some(CacheEntry {
            mtime,
            images: images.clone(),
        });
    }

    images
}

fn find_file() -> Option<(String, SystemTime)> {
    for p in PATHS {
        let path = Path::new(p);
        if let Ok(meta) = path.metadata() {
            if let Ok(mtime) = meta.modified() {
                return Some((p.to_string(), mtime));
            }
        }
    }
    None
}

/// Parse shell variable assignments, expanding $ARCHY_REGISTRY.
fn parse_image_versions(content: &str) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    let mut registry = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Match VAR="value" or VAR=value
        if let Some((key, val)) = parse_assignment(line) {
            // Read a self-default assignment without evaluating shell code.
            let default_prefix = format!("${{{key}:-");
            let val = val
                .strip_prefix(&default_prefix)
                .and_then(|v| v.strip_suffix('}'))
                .unwrap_or(val);
            let expanded = val.replace("$ARCHY_REGISTRY", &registry);
            if key == "ARCHY_REGISTRY" {
                registry = expanded.clone();
            }
            vars.insert(key.to_string(), expanded);
        }
    }

    // Keep only *_IMAGE entries whose value looks like a container image
    // reference (contains a `:` tag separator and at least one `/` path
    // component). Rejects placeholder values like "something" so a
    // hand-edit typo in image-versions.sh never gets treated as an image.
    vars.retain(|k, v| k.ends_with("_IMAGE") && v.contains(':') && v.contains('/'));
    vars
}

fn parse_assignment(line: &str) -> Option<(&str, &str)> {
    let eq = line.find('=')?;
    let key = &line[..eq];

    // Validate key is a shell variable name
    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }

    let val = &line[eq + 1..];
    // Strip surrounding quotes
    let val = val
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(val);

    Some((key, val))
}

/// Map app ID (as seen by the container scanner) to image variable name.
fn image_var_for_app(app_id: &str) -> Option<&'static str> {
    match app_id {
        // Bitcoin stack
        "bitcoin-knots" | "bitcoin" | "bitcoin-core" => Some("BITCOIN_KNOTS_IMAGE"),
        "lnd" => Some("LND_IMAGE"),
        "electrumx" => Some("ELECTRUMX_IMAGE"),
        "electrs" | "mempool-electrs" => Some("ELECTRUMX_IMAGE"),
        "bitcoin-ui" | "archy-bitcoin-ui" => Some("BITCOIN_UI_IMAGE"),
        "lnd-ui" | "archy-lnd-ui" => Some("LND_UI_IMAGE"),
        "electrs-ui" | "archy-electrs-ui" => Some("ELECTRS_UI_IMAGE"),
        "cuprate-ui" | "archy-cuprate-ui" => Some("CUPRATE_UI_IMAGE"),

        // Mempool stack (primary = web)
        "mempool" | "mempool-web" | "archy-mempool-web" => Some("MEMPOOL_WEB_IMAGE"),

        // BTCPay stack (primary = server)
        "btcpay" | "btcpay-server" | "btcpayserver" | "archy-btcpay-ui" => Some("BTCPAY_IMAGE"),

        // Apps
        "homeassistant" | "home-assistant" => Some("HOMEASSISTANT_IMAGE"),
        "grafana" => Some("GRAFANA_IMAGE"),
        "uptime-kuma" => Some("UPTIME_KUMA_IMAGE"),
        "jellyfin" => Some("JELLYFIN_IMAGE"),
        "photoprism" => Some("PHOTOPRISM_IMAGE"),
        "ollama" => Some("OLLAMA_IMAGE"),
        "vaultwarden" => Some("VAULTWARDEN_IMAGE"),
        "nextcloud" => Some("NEXTCLOUD_IMAGE"),
        "searxng" => Some("SEARXNG_IMAGE"),
        "filebrowser" => Some("FILEBROWSER_IMAGE"),
        "nginx-proxy-manager" => Some("NPM_IMAGE"),
        "portainer" => Some("PORTAINER_IMAGE"),
        "tailscale" => Some("TAILSCALE_IMAGE"),
        "netbird" => Some("NETBIRD_DASHBOARD_IMAGE"),
        "netbird-dashboard" => Some("NETBIRD_DASHBOARD_IMAGE"),
        "netbird-server" => Some("NETBIRD_SERVER_IMAGE"),

        // Fedimint
        "fedimint" | "fedimintd" => Some("FEDIMINT_IMAGE"),
        "fedimint-gateway" => Some("FEDIMINT_GATEWAY_IMAGE"),

        // Nostr / VPN
        "nostr-rs-relay" => Some("NOSTR_RS_RELAY_IMAGE"),

        // Immich (primary = server)
        "immich" | "immich_server" => Some("IMMICH_SERVER_IMAGE"),

        // Networking
        "tor" | "archy-tor" => Some("ALPINE_TOR_IMAGE"),

        _ => None,
    }
}

/// Get the full pinned image reference for an app ID.
pub fn pinned_image_for_app(app_id: &str) -> Option<String> {
    let var = image_var_for_app(app_id)?;
    let images = load_image_versions();
    images.get(var).cloned()
}

/// Return the pinned tag only when the running image is genuinely behind.
/// Registry host changes alone are not app updates, and floating tags are not
/// explicit versions we should advertise to users as available updates.
pub fn available_update_for_app(app_id: &str, running_image: &str) -> Option<String> {
    let pinned = pinned_image_for_app(app_id)?;
    available_update_for_images(&pinned, running_image)
}

pub fn available_update_for_images(pinned: &str, running_image: &str) -> Option<String> {
    if image_without_registry_or_tag(pinned) != image_without_registry_or_tag(running_image) {
        return None;
    }
    available_catalog_update_for_images(pinned, running_image)
}

/// A signed catalog binds the image to an app id, so a publisher namespace
/// migration must not hide a real upgrade. Baseline pins still require the
/// same repository via `available_update_for_images` above.
pub fn available_catalog_update_for_images(pinned: &str, running_image: &str) -> Option<String> {
    let pinned_version = extract_version_from_image(pinned);
    if is_floating_tag(&pinned_version) {
        return None;
    }
    if matches!(
        compare_image_versions(pinned, running_image),
        Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
    ) {
        return None;
    }
    Some(pinned_version)
}

/// Compare explicit image tags, ignoring registry and namespace. `None` means
/// unknown ordering (including floating tags), never permission to downgrade.
/// Archipelago's `-archyN` is a downstream patch revision ABOVE the upstream
/// release, not a SemVer prerelease below it.
pub fn compare_image_versions(target: &str, running: &str) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    let target = extract_version_from_image(target);
    let running = extract_version_from_image(running);
    if is_floating_tag(&target) || is_floating_tag(&running) {
        return None;
    }
    let target = target.strip_prefix('v').unwrap_or(&target);
    let running = running.strip_prefix('v').unwrap_or(&running);
    if target == running {
        return Some(Ordering::Equal);
    }
    let mut target_core = parse_version_parts(target)?;
    let mut running_core = parse_version_parts(running)?;
    while target_core.last() == Some(&0) {
        target_core.pop();
    }
    while running_core.last() == Some(&0) {
        running_core.pop();
    }
    match target_core.cmp(&running_core) {
        Ordering::Equal => {
            fn patch_revision(tag: &str) -> Option<u64> {
                if let Some((base, revision)) = tag.rsplit_once("-archy") {
                    if base.chars().all(|c| c.is_ascii_digit() || c == '.') {
                        return revision.parse().ok();
                    }
                }
                tag.chars()
                    .all(|c| c.is_ascii_digit() || c == '.')
                    .then_some(0)
            }
            Some(patch_revision(target)?.cmp(&patch_revision(running)?))
        }
        order => Some(order),
    }
}

/// Numeric components of a version tag, for ordering comparisons only.
///
/// Accepts a leading `v` and a trailing pre-release suffix (`v0.18.4-beta`),
/// comparing on the dotted numbers alone. Returns None when the tag is not a
/// recognisable dotted-numeric version, which the caller treats as "cannot
/// order these" rather than as equality.
fn parse_version_parts(tag: &str) -> Option<Vec<u64>> {
    let core = tag.strip_prefix('v').unwrap_or(tag);
    // Drop a pre-release/build suffix: 0.18.4-beta -> 0.18.4
    let core = core.split(['-', '+', '_']).next().unwrap_or(core);
    if core.is_empty() {
        return None;
    }
    let parts: Vec<&str> = core.split('.').collect();
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        // Any non-numeric component makes the whole tag unorderable.
        out.push(part.parse::<u64>().ok()?);
    }
    Some(out)
}

/// Extract version tag from a full image reference.
/// e.g. "source.archipelago-foundation.org/lfg2025/lnd:v0.18.4-beta" → "v0.18.4-beta"
/// Returns "latest" if no tag or tag is empty.
pub fn extract_version_from_image(image: &str) -> String {
    // Split off the tag after the last colon, but only if it comes after the last slash
    // (to avoid splitting on registry port like "registry.example.com:3000")
    if let Some(slash_pos) = image.rfind('/') {
        let after_slash = &image[slash_pos..];
        if let Some(colon_pos) = after_slash.rfind(':') {
            let tag = &after_slash[colon_pos + 1..];
            if !tag.is_empty() {
                return tag.to_string();
            }
        }
    }
    "latest".to_string()
}

fn is_floating_tag(tag: &str) -> bool {
    matches!(tag, "latest" | "stable" | "release" | "main")
}

pub fn image_without_registry_or_tag(image: &str) -> &str {
    let without_tag = strip_tag(image);
    match without_tag.split_once('/') {
        Some((first, rest))
            if first.contains('.') || first.contains(':') || first == "localhost" =>
        {
            rest
        }
        _ => without_tag,
    }
}

fn strip_tag(image: &str) -> &str {
    if let Some(slash_pos) = image.rfind('/') {
        let after_slash = &image[slash_pos..];
        if let Some(colon_pos) = after_slash.rfind(':') {
            return &image[..slash_pos + colon_pos];
        }
    }
    image
}

/// Container names and their image variable names for multi-container stacks.
/// Returns empty vec for single-container apps.
pub fn containers_for_stack(app_id: &str) -> Vec<(&'static str, &'static str)> {
    match app_id {
        "mempool" | "mempool-web" => vec![
            ("archy-mempool-db", "MARIADB_IMAGE"),
            ("mempool-api", "MEMPOOL_BACKEND_IMAGE"),
            ("archy-mempool-web", "MEMPOOL_WEB_IMAGE"),
        ],
        "btcpay" | "btcpay-server" | "btcpayserver" => vec![
            ("archy-btcpay-db", "BTCPAY_POSTGRES_IMAGE"),
            ("archy-nbxplorer", "NBXPLORER_IMAGE"),
            ("btcpay-server", "BTCPAY_IMAGE"),
        ],
        "immich" | "immich_server" => vec![
            ("immich_postgres", "IMMICH_POSTGRES_IMAGE"),
            ("immich_redis", "REDIS_IMAGE"),
            ("immich_server", "IMMICH_SERVER_IMAGE"),
        ],
        "netbird" => vec![
            ("netbird", "NETBIRD_PROXY_IMAGE"),
            ("netbird-dashboard", "NETBIRD_DASHBOARD_IMAGE"),
            ("netbird-server", "NETBIRD_SERVER_IMAGE"),
        ],
        _ => vec![],
    }
}

/// Get all pinned images for a stack update. Returns vec of (container_name, full_image_ref).
pub fn pinned_images_for_stack(app_id: &str) -> Vec<(String, String)> {
    let images = load_image_versions();
    containers_for_stack(app_id)
        .into_iter()
        .filter_map(|(name, var)| images.get(var).map(|img| (name.to_string(), img.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_version() {
        assert_eq!(
            extract_version_from_image(
                "source.archipelago-foundation.org/lfg2025/lnd:v0.18.4-beta"
            ),
            "v0.18.4-beta"
        );
        assert_eq!(
            extract_version_from_image("source.archipelago-foundation.org/lfg2025/grafana:10.2.0"),
            "10.2.0"
        );
        assert_eq!(
            extract_version_from_image("localhost/myapp:latest"),
            "latest"
        );
        assert_eq!(
            extract_version_from_image(
                "source.archipelago-foundation.org/lfg2025/bitcoin-knots:latest"
            ),
            "latest"
        );
    }

    #[test]
    fn strips_registry_and_tag_for_image_identity() {
        assert_eq!(
            image_without_registry_or_tag(
                "source.archipelago-foundation.org/lfg2025/lnd:v0.18.4-beta"
            ),
            "lfg2025/lnd"
        );
        assert_eq!(
            image_without_registry_or_tag(
                "source.archipelago-foundation.org/lfg2025/lnd:v0.18.4-beta"
            ),
            "lfg2025/lnd"
        );
    }

    #[test]
    fn floating_tags_are_not_explicit_updates() {
        assert!(is_floating_tag("latest"));
        assert!(is_floating_tag("stable"));
        assert!(!is_floating_tag("v0.18.4-beta"));
    }

    #[test]
    fn available_update_ignores_registry_only_changes() {
        assert_eq!(
            available_update_for_images(
                "source.archipelago-foundation.org/lfg2025/nextcloud:29",
                "source.archipelago-foundation.org/lfg2025/nextcloud:29",
            ),
            None
        );
    }

    #[test]
    fn available_update_returns_pinned_version_for_same_repo_newer_tag() {
        assert_eq!(
            available_update_for_images(
                "source.archipelago-foundation.org/lfg2025/nextcloud:29",
                "source.archipelago-foundation.org/lfg2025/nextcloud:28",
            ),
            Some("29".to_string())
        );
    }

    #[test]
    fn downstream_patch_is_newer_than_upstream_and_orders_revisions() {
        let upstream = "registry.test/team/mempool-frontend:v3.3.1";
        let patch1 = "registry.test/team/mempool-frontend:v3.3.1-archy1";
        let patch2 = "registry.test/team/mempool-frontend:v3.3.1-archy2";
        assert_eq!(available_update_for_images(upstream, patch1), None);
        assert_eq!(available_update_for_images(patch1, patch2), None);
        assert_eq!(
            available_update_for_images(patch1, upstream),
            Some("v3.3.1-archy1".into())
        );
        assert_eq!(
            available_update_for_images(patch2, patch1),
            Some("v3.3.1-archy2".into())
        );
    }

    #[test]
    fn catalog_namespace_migration_does_not_hide_patch_or_offer_reinstall() {
        let old = "registry.test/lfg2025/mempool-frontend:v3.3.1";
        let patched = "registry.test/chaum/mempool-frontend:v3.3.1-archy1";
        assert_eq!(
            available_catalog_update_for_images(patched, old),
            Some("v3.3.1-archy1".into())
        );
        assert_eq!(
            available_catalog_update_for_images(
                patched,
                "registry.test/lfg2025/mempool-frontend:v3.3.1-archy1"
            ),
            None
        );
        assert_eq!(available_update_for_images(patched, old), None);
    }

    #[test]
    fn equivalent_version_spelling_does_not_offer_update() {
        assert_eq!(
            available_update_for_images("r.test/team/app:v3.3.1", "r.test/team/app:3.3.1"),
            None
        );
        assert_eq!(
            available_update_for_images("r.test/team/app:3.3.0", "r.test/team/app:3.3"),
            None
        );
        assert_eq!(
            compare_image_versions("r.test/team/app:latest", "r.test/team/app:latest"),
            None
        );
    }

    #[test]
    fn test_parse_image_versions() {
        let content = r#"
ARCHY_REGISTRY="source.archipelago-foundation.org/lfg2025"
LND_IMAGE="$ARCHY_REGISTRY/lnd:v0.18.4-beta"
GRAFANA_IMAGE="$ARCHY_REGISTRY/grafana:10.2.0"
# comment
NOT_AN_IMAGE="something"
"#;
        let parsed = parse_image_versions(content);
        assert_eq!(
            parsed.get("LND_IMAGE"),
            Some(&"source.archipelago-foundation.org/lfg2025/lnd:v0.18.4-beta".to_string())
        );
        assert_eq!(
            parsed.get("GRAFANA_IMAGE"),
            Some(&"source.archipelago-foundation.org/lfg2025/grafana:10.2.0".to_string())
        );
        assert!(!parsed.contains_key("NOT_AN_IMAGE"));
        assert!(!parsed.contains_key("ARCHY_REGISTRY"));
    }

    #[test]
    fn shipped_image_pins_expand_shell_defaults_to_concrete_refs() {
        let images = parse_image_versions(include_str!("../../../../scripts/image-versions.sh"));
        assert_eq!(
            images["MEMPOOL_WEB_IMAGE"],
            "source.archipelago-foundation.org/chaum/mempool-frontend:v3.3.1-archy1"
        );
        assert_eq!(
            images["MEMPOOL_BACKEND_IMAGE"],
            "source.archipelago-foundation.org/lfg2025/mempool-backend:v3.3.1"
        );
        assert!(images
            .values()
            .all(|v| !v.contains('$') && !v.contains('}')));
    }

    #[test]
    fn test_image_var_mapping() {
        assert_eq!(image_var_for_app("lnd"), Some("LND_IMAGE"));
        assert_eq!(
            image_var_for_app("bitcoin-knots"),
            Some("BITCOIN_KNOTS_IMAGE")
        );
        assert_eq!(image_var_for_app("unknown-app"), None);
    }

    /// The BTCPay case that prompted the guard: 2.4.2 shipped for an actively
    /// exploited 2FA bypass, a stale 2.3.9 pin left in a legacy catalog entry,
    /// and the UI offering the withdrawn release as an "update".
    #[test]
    fn never_advertises_a_downgrade_as_an_update() {
        let stale = "docker.io/btcpayserver/btcpayserver:2.3.9";
        let running = "docker.io/btcpayserver/btcpayserver:2.4.2";
        assert_eq!(available_update_for_images(stale, running), None);
    }

    #[test]
    fn still_advertises_a_genuine_upgrade() {
        let pinned = "docker.io/btcpayserver/btcpayserver:2.4.2";
        let running = "docker.io/btcpayserver/btcpayserver:2.3.9";
        assert_eq!(
            available_update_for_images(pinned, running),
            Some("2.4.2".to_string())
        );
    }

    #[test]
    fn equal_versions_offer_nothing() {
        let same = "docker.io/btcpayserver/btcpayserver:2.4.2";
        assert_eq!(available_update_for_images(same, same), None);
    }

    #[test]
    fn prerelease_suffixes_compare_on_their_numbers() {
        let older = "example.test/lfg2025/lnd:v0.18.3-beta";
        let newer = "example.test/lfg2025/lnd:v0.18.4-beta";
        assert_eq!(available_update_for_images(older, newer), None);
        assert_eq!(
            available_update_for_images(newer, older),
            Some("v0.18.4-beta".to_string())
        );
    }

    /// Opaque tags stay on the old behaviour: we cannot order them, so a
    /// difference is still reported rather than silently swallowed.
    #[test]
    fn unorderable_tags_keep_previous_behaviour() {
        let a = "example.test/lfg2025/minio:RELEASE.2024-11-07T00-52-20Z";
        let b = "example.test/lfg2025/minio:RELEASE.2024-10-01T00-00-00Z";
        assert_eq!(
            available_update_for_images(a, b),
            Some("RELEASE.2024-11-07T00-52-20Z".to_string())
        );
    }

    #[test]
    fn parse_version_parts_rejects_non_numeric() {
        assert_eq!(parse_version_parts("2.4.2"), Some(vec![2, 4, 2]));
        assert_eq!(parse_version_parts("v0.18.4-beta"), Some(vec![0, 18, 4]));
        assert_eq!(parse_version_parts("28.4"), Some(vec![28, 4]));
        assert_eq!(parse_version_parts("RELEASE.2024-11-07T00-52-20Z"), None);
        assert_eq!(parse_version_parts("14-vectorchord0.4.3"), Some(vec![14]));
        assert_eq!(parse_version_parts("latest"), None);
    }
}
