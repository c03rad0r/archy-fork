//! Nostr relay management: configure, monitor, and manage relay connections.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::debug;

const RELAYS_FILE: &str = "nostr_relays.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    pub url: String,
    pub enabled: bool,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayStatus {
    pub url: String,
    pub connected: bool,
    pub enabled: bool,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RelayStore {
    pub relays: Vec<RelayConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayStats {
    pub total_relays: usize,
    pub connected_count: usize,
    pub enabled_count: usize,
}

/// Default relays seeded on first use.
const DEFAULT_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://relay.primal.net",
    "wss://relay.nostr.band",
    "wss://relay.snort.social",
    "wss://nostr.wine",
    "wss://relay.nostr.info",
    "wss://nostr-pub.wellorder.net",
    "wss://relay.current.fyi",
];

/// The union of the boot-config relay list and the user-managed (enabled)
/// relays — the set every nostr-facing operation should use, so senders and
/// receivers always overlap regardless of which list the operator edited.
pub async fn merged_relay_list(data_dir: &Path, config_relays: &[String]) -> Vec<String> {
    let mut relays: Vec<String> = config_relays.to_vec();
    if let Ok(store) = load_relays(data_dir).await {
        for r in store.relays {
            if r.enabled && !relays.contains(&r.url) {
                relays.push(r.url);
            }
        }
    }
    relays
}

pub async fn load_relays(data_dir: &Path) -> Result<RelayStore> {
    let path = data_dir.join(RELAYS_FILE);
    if !path.exists() {
        // Seed with defaults on first load
        let store = seed_defaults();
        save_relays(data_dir, &store).await?;
        return Ok(store);
    }
    let data = fs::read_to_string(&path)
        .await
        .context("Reading relay store")?;
    serde_json::from_str(&data).context("Parsing relay store")
}

pub async fn save_relays(data_dir: &Path, store: &RelayStore) -> Result<()> {
    let path = data_dir.join(RELAYS_FILE);
    let data = serde_json::to_string_pretty(store)?;
    fs::write(&path, data).await.context("Writing relay store")
}

fn seed_defaults() -> RelayStore {
    let now = chrono::Utc::now().to_rfc3339();
    RelayStore {
        relays: DEFAULT_RELAYS
            .iter()
            .map(|url| RelayConfig {
                url: url.to_string(),
                enabled: true,
                added_at: now.clone(),
            })
            .collect(),
    }
}

/// List all relays with connection status.
pub async fn list_relays(data_dir: &Path) -> Result<Vec<RelayStatus>> {
    let store = load_relays(data_dir).await?;
    let statuses: Vec<RelayStatus> = store
        .relays
        .into_iter()
        .map(|r| {
            // Connection check: try a quick TCP probe to the relay
            // For now, report enabled relays as connected (actual connectivity
            // is tested via the Nostr SDK when publishing/subscribing)
            RelayStatus {
                url: r.url,
                connected: r.enabled,
                enabled: r.enabled,
                added_at: r.added_at,
            }
        })
        .collect();
    Ok(statuses)
}

/// Add a new relay.
pub async fn add_relay(data_dir: &Path, url: &str) -> Result<RelayConfig> {
    let mut store = load_relays(data_dir).await?;

    let normalized = normalize_relay_url(url)?;

    if store.relays.iter().any(|r| r.url == normalized) {
        return Err(anyhow::anyhow!("Relay already exists: {}", normalized));
    }

    let config = RelayConfig {
        url: normalized,
        enabled: true,
        added_at: chrono::Utc::now().to_rfc3339(),
    };

    debug!(url = %config.url, "Added relay");
    store.relays.push(config.clone());
    save_relays(data_dir, &store).await?;
    Ok(config)
}

/// Remove a relay.
pub async fn remove_relay(data_dir: &Path, url: &str) -> Result<()> {
    let mut store = load_relays(data_dir).await?;
    let original_len = store.relays.len();
    store.relays.retain(|r| r.url != url);
    if store.relays.len() == original_len {
        return Err(anyhow::anyhow!("Relay not found: {}", url));
    }
    save_relays(data_dir, &store).await
}

/// Toggle relay enabled/disabled.
pub async fn toggle_relay(data_dir: &Path, url: &str, enabled: bool) -> Result<()> {
    let mut store = load_relays(data_dir).await?;
    let relay = store
        .relays
        .iter_mut()
        .find(|r| r.url == url)
        .ok_or_else(|| anyhow::anyhow!("Relay not found: {}", url))?;
    relay.enabled = enabled;
    save_relays(data_dir, &store).await
}

/// Get aggregate stats.
pub async fn get_stats(data_dir: &Path) -> Result<RelayStats> {
    let store = load_relays(data_dir).await?;
    let enabled_count = store.relays.iter().filter(|r| r.enabled).count();
    Ok(RelayStats {
        total_relays: store.relays.len(),
        connected_count: enabled_count,
        enabled_count,
    })
}

/// Normalize and validate a relay URL.
/// Only allows wss:// scheme (not ws://) for security.
/// Rejects URLs pointing to private/internal IPs.
fn normalize_relay_url(url: &str) -> Result<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(anyhow::anyhow!("Relay URL cannot be empty"));
    }
    if trimmed.len() > 2048 {
        return Err(anyhow::anyhow!("Relay URL too long"));
    }

    // Apply wss:// prefix if no scheme
    let with_scheme = if trimmed.starts_with("wss://") || trimmed.starts_with("ws://") {
        trimmed.to_string()
    } else {
        format!("wss://{}", trimmed)
    };

    // Only allow wss:// scheme for security
    if !with_scheme.starts_with("wss://") {
        return Err(anyhow::anyhow!("Relay URL must use wss:// scheme"));
    }

    // Extract host portion for SSRF validation
    let host = with_scheme
        .strip_prefix("wss://")
        .unwrap_or("")
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");

    if host.is_empty() {
        return Err(anyhow::anyhow!("Relay URL must have a valid host"));
    }

    // Reject private/internal addresses
    if is_relay_host_private(host) {
        return Err(anyhow::anyhow!(
            "Relay URL must not point to private/local addresses"
        ));
    }

    Ok(with_scheme)
}

/// Check if a relay host points to a private or internal address.
fn is_relay_host_private(host: &str) -> bool {
    let lower = host.to_lowercase();
    if lower == "localhost" || lower == "localhost.localdomain" || lower.ends_with(".local") {
        return true;
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
            }
            std::net::IpAddr::V6(v6) => {
                if v6.is_loopback() || v6.is_unspecified() {
                    return true;
                }
                if let Some(v4) = v6.to_ipv4_mapped() {
                    return v4.is_loopback()
                        || v4.is_private()
                        || v4.is_link_local()
                        || v4.is_unspecified();
                }
                let segments = v6.segments();
                (segments[0] & 0xfe00) == 0xfc00 || (segments[0] & 0xffc0) == 0xfe80
            }
        };
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_normalize_relay_url_with_wss() {
        let result = normalize_relay_url("wss://relay.damus.io").unwrap();
        assert_eq!(result, "wss://relay.damus.io");
    }

    #[test]
    fn test_normalize_relay_url_rejects_ws() {
        let result = normalize_relay_url("ws://relay.example.com");
        assert!(
            result.is_err(),
            "ws:// scheme should be rejected — only wss:// is allowed"
        );
    }

    #[test]
    fn test_normalize_relay_url_without_scheme() {
        let result = normalize_relay_url("relay.example.com").unwrap();
        assert_eq!(result, "wss://relay.example.com");
    }

    #[test]
    fn test_normalize_relay_url_trims_whitespace() {
        let result = normalize_relay_url("  wss://relay.example.com  ").unwrap();
        assert_eq!(result, "wss://relay.example.com");
    }

    #[test]
    fn test_normalize_relay_url_empty_errors() {
        let result = normalize_relay_url("");
        assert!(result.is_err());

        let result = normalize_relay_url("   ");
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_relay_url_rejects_private_ips() {
        assert!(normalize_relay_url("wss://127.0.0.1").is_err());
        assert!(normalize_relay_url("wss://localhost").is_err());
        assert!(normalize_relay_url("wss://192.168.1.1").is_err());
        assert!(normalize_relay_url("wss://10.0.0.1").is_err());
    }

    #[test]
    fn test_seed_defaults_has_expected_count() {
        let store = seed_defaults();
        assert_eq!(store.relays.len(), DEFAULT_RELAYS.len());
    }

    #[test]
    fn test_seed_defaults_all_enabled() {
        let store = seed_defaults();
        assert!(store.relays.iter().all(|r| r.enabled));
    }

    #[test]
    fn test_seed_defaults_urls_match() {
        let store = seed_defaults();
        let urls: Vec<&str> = store.relays.iter().map(|r| r.url.as_str()).collect();
        for expected in DEFAULT_RELAYS {
            assert!(
                urls.contains(expected),
                "Missing default relay: {}",
                expected
            );
        }
    }

    #[test]
    fn test_seed_defaults_all_have_timestamps() {
        let store = seed_defaults();
        for relay in &store.relays {
            assert!(!relay.added_at.is_empty());
            // Should be a valid RFC3339 timestamp
            assert!(chrono::DateTime::parse_from_rfc3339(&relay.added_at).is_ok());
        }
    }

    #[tokio::test]
    async fn test_load_relays_seeds_defaults_on_first_load() {
        let tmp = TempDir::new().unwrap();
        let store = load_relays(tmp.path()).await.unwrap();
        assert_eq!(store.relays.len(), DEFAULT_RELAYS.len());

        // The file should now exist after seeding
        assert!(tmp.path().join(RELAYS_FILE).exists());
    }

    #[tokio::test]
    async fn test_save_and_load_relays_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = RelayStore {
            relays: vec![
                RelayConfig {
                    url: "wss://test.relay.one".to_string(),
                    enabled: true,
                    added_at: "2025-01-01T00:00:00Z".to_string(),
                },
                RelayConfig {
                    url: "wss://test.relay.two".to_string(),
                    enabled: false,
                    added_at: "2025-01-02T00:00:00Z".to_string(),
                },
            ],
        };

        save_relays(tmp.path(), &store).await.unwrap();
        let loaded = load_relays(tmp.path()).await.unwrap();
        assert_eq!(loaded.relays.len(), 2);
        assert_eq!(loaded.relays[0].url, "wss://test.relay.one");
        assert!(loaded.relays[0].enabled);
        assert_eq!(loaded.relays[1].url, "wss://test.relay.two");
        assert!(!loaded.relays[1].enabled);
    }

    #[tokio::test]
    async fn test_add_relay() {
        let tmp = TempDir::new().unwrap();
        // Seed defaults first
        let _ = load_relays(tmp.path()).await.unwrap();

        let config = add_relay(tmp.path(), "wss://new.relay.test").await.unwrap();
        assert_eq!(config.url, "wss://new.relay.test");
        assert!(config.enabled);

        let store = load_relays(tmp.path()).await.unwrap();
        assert_eq!(store.relays.len(), DEFAULT_RELAYS.len() + 1);
    }

    #[tokio::test]
    async fn test_add_relay_normalizes_url() {
        let tmp = TempDir::new().unwrap();
        let _ = load_relays(tmp.path()).await.unwrap();

        let config = add_relay(tmp.path(), "bare.relay.test").await.unwrap();
        assert_eq!(config.url, "wss://bare.relay.test");
    }

    #[tokio::test]
    async fn test_add_duplicate_relay_errors() {
        let tmp = TempDir::new().unwrap();
        let _ = load_relays(tmp.path()).await.unwrap();

        // Adding a default relay again should fail
        let result = add_relay(tmp.path(), "wss://relay.damus.io").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_remove_relay() {
        let tmp = TempDir::new().unwrap();
        let _ = load_relays(tmp.path()).await.unwrap();
        let initial_count = DEFAULT_RELAYS.len();

        remove_relay(tmp.path(), "wss://relay.damus.io")
            .await
            .unwrap();
        let store = load_relays(tmp.path()).await.unwrap();
        assert_eq!(store.relays.len(), initial_count - 1);
        assert!(!store.relays.iter().any(|r| r.url == "wss://relay.damus.io"));
    }

    #[tokio::test]
    async fn test_remove_nonexistent_relay_errors() {
        let tmp = TempDir::new().unwrap();
        let _ = load_relays(tmp.path()).await.unwrap();

        let result = remove_relay(tmp.path(), "wss://nonexistent.relay").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_toggle_relay_disable() {
        let tmp = TempDir::new().unwrap();
        let _ = load_relays(tmp.path()).await.unwrap();

        toggle_relay(tmp.path(), "wss://relay.damus.io", false)
            .await
            .unwrap();

        let store = load_relays(tmp.path()).await.unwrap();
        let relay = store
            .relays
            .iter()
            .find(|r| r.url == "wss://relay.damus.io")
            .unwrap();
        assert!(!relay.enabled);
    }

    #[tokio::test]
    async fn test_toggle_relay_enable() {
        let tmp = TempDir::new().unwrap();
        let _ = load_relays(tmp.path()).await.unwrap();

        // Disable first, then re-enable
        toggle_relay(tmp.path(), "wss://relay.damus.io", false)
            .await
            .unwrap();
        toggle_relay(tmp.path(), "wss://relay.damus.io", true)
            .await
            .unwrap();

        let store = load_relays(tmp.path()).await.unwrap();
        let relay = store
            .relays
            .iter()
            .find(|r| r.url == "wss://relay.damus.io")
            .unwrap();
        assert!(relay.enabled);
    }

    #[tokio::test]
    async fn test_toggle_nonexistent_relay_errors() {
        let tmp = TempDir::new().unwrap();
        let _ = load_relays(tmp.path()).await.unwrap();

        let result = toggle_relay(tmp.path(), "wss://no.such.relay", true).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_stats() {
        let tmp = TempDir::new().unwrap();
        let _ = load_relays(tmp.path()).await.unwrap();

        let stats = get_stats(tmp.path()).await.unwrap();
        assert_eq!(stats.total_relays, DEFAULT_RELAYS.len());
        assert_eq!(stats.enabled_count, DEFAULT_RELAYS.len());
        assert_eq!(stats.connected_count, stats.enabled_count);
    }

    #[tokio::test]
    async fn test_get_stats_after_disable() {
        let tmp = TempDir::new().unwrap();
        let _ = load_relays(tmp.path()).await.unwrap();

        toggle_relay(tmp.path(), "wss://relay.damus.io", false)
            .await
            .unwrap();
        toggle_relay(tmp.path(), "wss://relay.primal.net", false)
            .await
            .unwrap();

        let stats = get_stats(tmp.path()).await.unwrap();
        assert_eq!(stats.total_relays, DEFAULT_RELAYS.len());
        assert_eq!(stats.enabled_count, DEFAULT_RELAYS.len() - 2);
    }

    #[tokio::test]
    async fn test_list_relays() {
        let tmp = TempDir::new().unwrap();
        let statuses = list_relays(tmp.path()).await.unwrap();
        assert_eq!(statuses.len(), DEFAULT_RELAYS.len());

        // All default relays should be enabled and "connected"
        for status in &statuses {
            assert!(status.enabled);
            assert!(status.connected);
            assert!(status.url.starts_with("wss://"));
        }
    }

    #[test]
    fn test_relay_store_default_is_empty() {
        let store = RelayStore::default();
        assert!(store.relays.is_empty());
    }

    #[test]
    fn test_relay_config_serialization() {
        let config = RelayConfig {
            url: "wss://test.relay".to_string(),
            enabled: true,
            added_at: "2025-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: RelayConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.url, config.url);
        assert_eq!(parsed.enabled, config.enabled);
        assert_eq!(parsed.added_at, config.added_at);
    }
}
