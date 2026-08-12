//! Streaming data pricing configuration.
//!
//! Follows TollGate TIP-02 pricing model:
//! - step_size: granularity of purchase (bytes, milliseconds, or request count)
//! - price_per_step: cost in sats for one step
//! - min_steps: minimum purchase requirement
//! - metric: what is being metered (bytes, time, requests)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

const PRICING_FILE: &str = "streaming/pricing.json";

/// What resource is being metered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Metric {
    /// Bytes transferred.
    Bytes,
    /// Time in milliseconds.
    Milliseconds,
    /// Number of API requests.
    Requests,
}

impl std::fmt::Display for Metric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Metric::Bytes => write!(f, "bytes"),
            Metric::Milliseconds => write!(f, "milliseconds"),
            Metric::Requests => write!(f, "requests"),
        }
    }
}

/// Pricing configuration for a specific service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicePricing {
    /// Unique service identifier.
    pub service_id: String,
    /// Human-readable service name.
    pub name: String,
    /// What is being metered.
    pub metric: Metric,
    /// Size of one step in the metric's unit.
    /// e.g., 1_048_576 for 1MB steps, 60_000 for 1-minute steps, 1 for per-request.
    pub step_size: u64,
    /// Price in sats for one step.
    pub price_per_step: u64,
    /// Minimum number of steps per purchase (0 = no minimum).
    #[serde(default)]
    pub min_steps: u64,
    /// Whether this service is currently active/accepting payments.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Description of what this service provides.
    #[serde(default)]
    pub description: String,
    /// Accepted mint URLs (empty = use wallet defaults).
    #[serde(default)]
    pub accepted_mints: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl ServicePricing {
    /// Calculate allotment (in metric units) for a given payment amount.
    pub fn calculate_allotment(&self, paid_sats: u64) -> u64 {
        if self.price_per_step == 0 {
            return 0;
        }
        let steps = paid_sats / self.price_per_step;
        steps * self.step_size
    }

    /// Calculate the minimum payment required.
    pub fn minimum_payment(&self) -> u64 {
        if self.min_steps == 0 {
            self.price_per_step // At least one step
        } else {
            self.min_steps * self.price_per_step
        }
    }

    /// Calculate how many sats are needed for a specific allotment.
    pub fn cost_for_allotment(&self, allotment: u64) -> u64 {
        if self.step_size == 0 {
            return 0;
        }
        let steps = allotment.div_ceil(self.step_size); // ceiling division
        steps * self.price_per_step
    }

    /// Validate that this pricing config is sensible.
    pub fn validate(&self) -> Result<()> {
        if self.service_id.is_empty() {
            anyhow::bail!("Service ID cannot be empty");
        }
        if self.step_size == 0 {
            anyhow::bail!("Step size must be > 0");
        }
        if self.price_per_step == 0 {
            anyhow::bail!("Price per step must be > 0");
        }
        if self.name.is_empty() {
            anyhow::bail!("Service name cannot be empty");
        }
        Ok(())
    }
}

/// All pricing configurations for this node.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PricingConfig {
    pub services: Vec<ServicePricing>,
}

impl PricingConfig {
    /// Find pricing for a service by ID.
    pub fn get_service(&self, service_id: &str) -> Option<&ServicePricing> {
        self.services.iter().find(|s| s.service_id == service_id)
    }

    /// Find enabled pricing for a service by ID.
    pub fn get_active_service(&self, service_id: &str) -> Option<&ServicePricing> {
        self.services
            .iter()
            .find(|s| s.service_id == service_id && s.enabled)
    }
}

/// Load pricing config from disk.
pub async fn load_pricing(data_dir: &Path) -> Result<PricingConfig> {
    let path = data_dir.join(PRICING_FILE);
    if !path.exists() {
        return Ok(default_pricing());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read pricing config")?;
    let config: PricingConfig =
        serde_json::from_str(&content).unwrap_or_else(|_| default_pricing());
    Ok(config)
}

/// Save pricing config to disk.
pub async fn save_pricing(data_dir: &Path, config: &PricingConfig) -> Result<()> {
    let dir = data_dir.join("streaming");
    fs::create_dir_all(&dir)
        .await
        .context("Failed to create streaming dir")?;
    let path = data_dir.join(PRICING_FILE);
    let content =
        serde_json::to_string_pretty(config).context("Failed to serialize pricing config")?;
    fs::write(&path, content)
        .await
        .context("Failed to write pricing config")?;
    Ok(())
}

/// Default pricing config with common services pre-configured (disabled).
fn default_pricing() -> PricingConfig {
    PricingConfig {
        services: vec![
            ServicePricing {
                service_id: "content-download".to_string(),
                name: "Content Downloads".to_string(),
                metric: Metric::Bytes,
                step_size: 1_048_576, // 1 MB
                price_per_step: 1,    // 1 sat per MB
                min_steps: 0,
                enabled: false,
                description: "Pay-per-byte content downloads from this node".to_string(),
                accepted_mints: vec![],
            },
            ServicePricing {
                service_id: "federation-sync".to_string(),
                name: "Federation Sync Access".to_string(),
                metric: Metric::Milliseconds,
                step_size: 60_000, // 1 minute
                price_per_step: 1, // 1 sat per minute
                min_steps: 5,      // 5 minute minimum
                enabled: false,
                description: "Timed access to federation sync endpoint".to_string(),
                accepted_mints: vec![],
            },
            ServicePricing {
                service_id: "api-access".to_string(),
                name: "API Access".to_string(),
                metric: Metric::Requests,
                step_size: 1,      // Per request
                price_per_step: 1, // 1 sat per request
                min_steps: 10,     // 10 request minimum
                enabled: false,
                description: "Per-request API access for external consumers".to_string(),
                accepted_mints: vec![],
            },
            ServicePricing {
                service_id: "nostr-relay".to_string(),
                name: "Nostr Relay Access".to_string(),
                metric: Metric::Milliseconds,
                step_size: 3_600_000, // 1 hour
                price_per_step: 10,   // 10 sats per hour
                min_steps: 1,
                enabled: false,
                description: "Timed access to the local Nostr relay".to_string(),
                accepted_mints: vec![],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_calculate_allotment_bytes() {
        let pricing = ServicePricing {
            service_id: "test".into(),
            name: "Test".into(),
            metric: Metric::Bytes,
            step_size: 1_048_576, // 1 MB
            price_per_step: 1,
            min_steps: 0,
            enabled: true,
            description: String::new(),
            accepted_mints: vec![],
        };

        // 10 sats = 10 MB
        assert_eq!(pricing.calculate_allotment(10), 10_485_760);
    }

    #[test]
    fn test_calculate_allotment_time() {
        let pricing = ServicePricing {
            service_id: "test".into(),
            name: "Test".into(),
            metric: Metric::Milliseconds,
            step_size: 60_000, // 1 minute
            price_per_step: 2,
            min_steps: 0,
            enabled: true,
            description: String::new(),
            accepted_mints: vec![],
        };

        // 10 sats at 2 sats/min = 5 minutes = 300,000 ms
        assert_eq!(pricing.calculate_allotment(10), 300_000);
    }

    #[test]
    fn test_minimum_payment() {
        let pricing = ServicePricing {
            service_id: "test".into(),
            name: "Test".into(),
            metric: Metric::Requests,
            step_size: 1,
            price_per_step: 1,
            min_steps: 10,
            enabled: true,
            description: String::new(),
            accepted_mints: vec![],
        };

        assert_eq!(pricing.minimum_payment(), 10);
    }

    #[test]
    fn test_cost_for_allotment() {
        let pricing = ServicePricing {
            service_id: "test".into(),
            name: "Test".into(),
            metric: Metric::Bytes,
            step_size: 1_048_576,
            price_per_step: 1,
            min_steps: 0,
            enabled: true,
            description: String::new(),
            accepted_mints: vec![],
        };

        // 5 MB costs 5 sats
        assert_eq!(pricing.cost_for_allotment(5_242_880), 5);
        // 1.5 MB rounds up to 2 sats
        assert_eq!(pricing.cost_for_allotment(1_572_864), 2);
    }

    #[test]
    fn test_validate_pricing() {
        let good = ServicePricing {
            service_id: "test".into(),
            name: "Test".into(),
            metric: Metric::Bytes,
            step_size: 1024,
            price_per_step: 1,
            min_steps: 0,
            enabled: true,
            description: String::new(),
            accepted_mints: vec![],
        };
        assert!(good.validate().is_ok());

        let bad_step = ServicePricing {
            step_size: 0,
            ..good.clone()
        };
        assert!(bad_step.validate().is_err());

        let bad_price = ServicePricing {
            price_per_step: 0,
            ..good.clone()
        };
        assert!(bad_price.validate().is_err());
    }

    #[tokio::test]
    async fn test_load_default_pricing() {
        let tmp = TempDir::new().unwrap();
        let config = load_pricing(tmp.path()).await.unwrap();
        assert_eq!(config.services.len(), 4);
        // All disabled by default
        assert!(config.services.iter().all(|s| !s.enabled));
    }

    #[tokio::test]
    async fn test_save_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let config = PricingConfig {
            services: vec![ServicePricing {
                service_id: "custom".into(),
                name: "Custom Service".into(),
                metric: Metric::Requests,
                step_size: 1,
                price_per_step: 5,
                min_steps: 1,
                enabled: true,
                description: "custom".into(),
                accepted_mints: vec!["http://mint".into()],
            }],
        };
        save_pricing(tmp.path(), &config).await.unwrap();
        let loaded = load_pricing(tmp.path()).await.unwrap();
        assert_eq!(loaded.services.len(), 1);
        assert_eq!(loaded.services[0].price_per_step, 5);
    }

    #[test]
    fn test_get_service() {
        let config = default_pricing();
        assert!(config.get_service("content-download").is_some());
        assert!(config.get_service("nonexistent").is_none());
        // All disabled by default
        assert!(config.get_active_service("content-download").is_none());
    }

    #[test]
    fn test_metric_display() {
        assert_eq!(format!("{}", Metric::Bytes), "bytes");
        assert_eq!(format!("{}", Metric::Milliseconds), "milliseconds");
        assert_eq!(format!("{}", Metric::Requests), "requests");
    }
}
