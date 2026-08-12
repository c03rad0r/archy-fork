//! Dynamic container registry configuration.
//!
//! Manages a list of container registries that the node uses to pull app images.
//! Registries are tried in order — if the first fails, the next is attempted.
//! Configuration is persisted to disk and editable via RPC.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

const REGISTRY_FILE: &str = "config/registries.json";
const OVH_REGISTRY_URL: &str = "source.archipelago-foundation.org/lfg2025";
/// Retired registry host (release server retired 2026-06-13; the registry
/// frontend was fully dead by 2026-07-10 — 500 on every /v2 manifest read).
/// Never a default, never force-enabled; stripped from saved configs on
/// load. The literal exists ONLY so the strip can match — nothing may pull
/// through this host.
const RETIRED_TX1138_HOST: &str = "git.tx1138.com";

/// A single container registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    /// Registry URL (e.g., "source.archipelago-foundation.org/lfg2025").
    pub url: String,
    /// Human-readable name.
    pub name: String,
    /// Whether TLS verification is required (false for HTTP registries).
    pub tls_verify: bool,
    /// Whether this registry is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Priority (lower = tried first).
    #[serde(default)]
    pub priority: u32,
}

fn default_true() -> bool {
    true
}

/// Registry configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    pub registries: Vec<Registry>,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            registries: vec![Registry {
                url: OVH_REGISTRY_URL.to_string(),
                name: "Server 1 (OVH)".to_string(),
                tls_verify: false,
                enabled: true,
                priority: 0,
            }],
        }
    }
}

impl RegistryConfig {
    /// Get enabled registries sorted by priority.
    pub fn active_registries(&self) -> Vec<&Registry> {
        let mut regs: Vec<&Registry> = self.registries.iter().filter(|r| r.enabled).collect();
        regs.sort_by_key(|r| r.priority);
        regs
    }

    /// Rewrite an image reference to use a specific registry.
    /// E.g., "docker.io/lfg2025/bitcoin-knots:latest" with registry "source.archipelago-foundation.org/lfg2025"
    /// becomes "source.archipelago-foundation.org/lfg2025/bitcoin-knots:latest".
    pub fn rewrite_image(&self, image: &str, registry: &Registry) -> String {
        // Extract the image name (last component after the org/namespace)
        // Handles: "registry/org/image:tag" -> "image:tag"
        let image_name = extract_image_name(image);
        format!("{}/{}", registry.url, image_name)
    }
}

/// Extract the image name from a full image reference.
/// "source.archipelago-foundation.org/lfg2025/bitcoin-knots:latest" -> "bitcoin-knots:latest"
/// "docker.io/gitea/gitea:1.23" -> "gitea:1.23"
fn extract_image_name(image: &str) -> &str {
    // Split by '/' and take the last segment (image:tag)
    image.rsplit('/').next().unwrap_or(image)
}

/// Load registry config from disk, merging in any default registries
/// that the operator hasn't explicitly removed. This lets us roll out
/// new default mirrors (e.g. a new Server 3) to existing nodes without
/// them having to edit their saved config. Explicit removals stick —
/// if the URL is absent from disk AND absent from current defaults, it
/// stays gone.
pub async fn load_registries(data_dir: &Path) -> Result<RegistryConfig> {
    let path = data_dir.join(REGISTRY_FILE);
    if !path.exists() {
        return Ok(RegistryConfig::default());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read registry config")?;
    let mut config: RegistryConfig =
        serde_json::from_str(&content).unwrap_or_else(|_| RegistryConfig::default());

    // One-time migration: the Hetzner VPS at 23.182.128.160 was
    // decommissioned 2026-04-23. Existing nodes have it baked into
    // their saved registry list (was the original Server 1). Strip it
    // on load so every container pull doesn't pay a connection-refused
    // timeout against a dead host. Exception to the usual "explicit
    // removals stick" rule: the user never chose to add this — it
    // was a default.
    let before = config.registries.len();
    config
        .registries
        .retain(|r| !r.url.contains("23.182.128.160"));
    // Same treatment for the retired tx1138 registry (was Server 2 in older
    // defaults): strip it on load so nothing ever pulls through the dead
    // host again.
    config
        .registries
        .retain(|r| !r.url.contains(RETIRED_TX1138_HOST));
    // And the release server's own bare-IP twin (146.59.87.168:3000 — the
    // same host as source.archipelago-foundation.org): older defaults listed
    // both, so the registry UI showed one server twice. Bare-IP origins were
    // retired 2026-08-11; the named entry stays and covers the same pulls.
    config
        .registries
        .retain(|r| !r.url.contains("146.59.87.168"));
    let mut changed = config.registries.len() != before;

    // Migrate: any default registry URL that isn't already in the
    // saved list gets appended at the end (so existing priority order
    // is preserved for anything the operator already configured).
    let defaults = RegistryConfig::default();
    let known: std::collections::HashSet<String> =
        config.registries.iter().map(|r| r.url.clone()).collect();
    let max_priority = config
        .registries
        .iter()
        .map(|r| r.priority)
        .max()
        .unwrap_or(0);
    for (i, def) in defaults.registries.iter().enumerate() {
        if !known.contains(&def.url) {
            let mut cloned = def.clone();
            cloned.priority = max_priority.saturating_add(10 + i as u32);
            config.registries.push(cloned);
            changed = true;
        }
    }
    let before_order: Vec<(String, bool, u32)> = config
        .registries
        .iter()
        .map(|r| (r.url.clone(), r.enabled, r.priority))
        .collect();
    force_ovh_registry_primary(&mut config);
    changed = changed
        || before_order
            != config
                .registries
                .iter()
                .map(|r| (r.url.clone(), r.enabled, r.priority))
                .collect::<Vec<_>>();
    if changed {
        // Persist so the next load doesn't have to re-merge.
        if let Err(e) = save_registries(data_dir, &config).await {
            tracing::warn!("Failed to persist migrated registry config: {e:#}");
        }
    }
    Ok(config)
}

fn force_ovh_registry_primary(config: &mut RegistryConfig) {
    let defaults = RegistryConfig::default();
    for def in defaults.registries {
        if !config.registries.iter().any(|r| r.url == def.url) {
            config.registries.push(def);
        }
    }

    for registry in config.registries.iter_mut() {
        match registry.url.as_str() {
            OVH_REGISTRY_URL => {
                registry.name = "Server 1 (OVH)".to_string();
                registry.tls_verify = false;
                registry.enabled = true;
                registry.priority = 0;
            }
            _ => {
                if registry.priority <= 10 {
                    registry.priority = registry.priority.saturating_add(20);
                }
            }
        }
    }
}

/// Save registry config to disk.
pub async fn save_registries(data_dir: &Path, config: &RegistryConfig) -> Result<()> {
    let dir = data_dir.join("config");
    fs::create_dir_all(&dir)
        .await
        .context("Failed to create config dir")?;
    let path = data_dir.join(REGISTRY_FILE);
    let content =
        serde_json::to_string_pretty(config).context("Failed to serialize registry config")?;
    fs::write(&path, content)
        .await
        .context("Failed to write registry config")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_extract_image_name() {
        assert_eq!(
            extract_image_name("source.archipelago-foundation.org/lfg2025/bitcoin-knots:latest"),
            "bitcoin-knots:latest"
        );
        assert_eq!(
            extract_image_name("docker.io/gitea/gitea:1.23"),
            "gitea:1.23"
        );
        assert_eq!(extract_image_name("localhost/myimage:v1"), "myimage:v1");
    }

    #[test]
    fn test_rewrite_image() {
        let config = RegistryConfig::default();
        // An image hardcoded to some other registry rewrites to OVH when
        // asked for the primary mirror.
        let primary = &config.registries[0];
        assert_eq!(
            config.rewrite_image("docker.io/lfg2025/bitcoin-knots:latest", primary),
            "source.archipelago-foundation.org/lfg2025/bitcoin-knots:latest"
        );
    }

    #[test]
    fn test_active_registries_sorted() {
        let config = RegistryConfig::default();
        let active = config.active_registries();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].url, OVH_REGISTRY_URL);
    }

    #[tokio::test]
    async fn test_load_default() {
        let tmp = TempDir::new().unwrap();
        let config = load_registries(tmp.path()).await.unwrap();
        assert_eq!(config.registries.len(), 1);
    }

    #[tokio::test]
    async fn test_load_strips_retired_tx1138_registry() {
        // Nodes provisioned before the retirement have the tx1138 registry
        // baked into their saved config (was Server 2). It must be stripped
        // on load and never re-added by the defaults merge.
        let tmp = TempDir::new().unwrap();
        let config = RegistryConfig {
            registries: vec![
                Registry {
                    url: format!("{RETIRED_TX1138_HOST}/lfg2025"),
                    name: "Server 2 (tx1138)".into(),
                    tls_verify: true,
                    enabled: true,
                    priority: 10,
                },
                Registry {
                    url: OVH_REGISTRY_URL.into(),
                    name: "Server 1 (OVH)".into(),
                    tls_verify: false,
                    enabled: true,
                    priority: 0,
                },
            ],
        };
        save_registries(tmp.path(), &config).await.unwrap();
        let loaded = load_registries(tmp.path()).await.unwrap();
        assert!(
            !loaded
                .registries
                .iter()
                .any(|r| r.url.contains(RETIRED_TX1138_HOST)),
            "retired tx1138 registry must be stripped on load; got {:?}",
            loaded.registries
        );
    }

    #[tokio::test]
    async fn test_save_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut config = RegistryConfig::default();
        config.registries.push(Registry {
            url: "myregistry.com/apps".into(),
            name: "Custom".into(),
            tls_verify: true,
            enabled: true,
            priority: 5,
        });
        save_registries(tmp.path(), &config).await.unwrap();
        let loaded = load_registries(tmp.path()).await.unwrap();
        assert_eq!(loaded.registries.len(), 2);
    }
}
