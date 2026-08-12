//! Nostr service advertisements for streaming data payments.
//!
//! Publishes and parses kind 10021 replaceable events (TollGate TIP-01 compatible)
//! that advertise priced services on this node. Peers discover services by
//! querying Nostr relays for these events.

use super::pricing::{PricingConfig, ServicePricing};
use serde::{Deserialize, Serialize};

/// Nostr event kind for service advertisements (TollGate TIP-01).
pub const KIND_SERVICE_ADVERTISEMENT: u16 = 10021;

/// Nostr event kind for session proof (TollGate TIP-01).
pub const KIND_SESSION: u16 = 1022;

/// Nostr event kind for service notice.
pub const KIND_NOTICE: u16 = 21023;

/// A parsed service advertisement from a Nostr event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceAdvertisement {
    /// Publisher's Nostr pubkey (hex).
    pub pubkey: String,
    /// Tor onion address or clearnet address for connecting.
    #[serde(default)]
    pub endpoint: String,
    /// Advertised services with pricing.
    pub services: Vec<AdvertisedService>,
    /// Supported protocol versions/TIPs.
    #[serde(default)]
    pub supported_tips: Vec<String>,
    /// Timestamp of the advertisement.
    pub created_at: String,
}

/// A single service within an advertisement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvertisedService {
    pub service_id: String,
    pub name: String,
    pub metric: String,
    pub step_size: u64,
    pub price_per_step: u64,
    pub unit: String,
    pub mint_urls: Vec<String>,
    pub min_steps: u64,
    pub description: String,
}

impl From<&ServicePricing> for AdvertisedService {
    fn from(p: &ServicePricing) -> Self {
        Self {
            service_id: p.service_id.clone(),
            name: p.name.clone(),
            metric: p.metric.to_string(),
            step_size: p.step_size,
            price_per_step: p.price_per_step,
            unit: "sat".to_string(),
            mint_urls: p.accepted_mints.clone(),
            min_steps: p.min_steps,
            description: p.description.clone(),
        }
    }
}

/// Build Nostr event tags for a service advertisement (TIP-01/TIP-02 format).
///
/// Returns a vector of tag arrays suitable for inclusion in a Nostr event.
pub fn build_advertisement_tags(
    config: &PricingConfig,
    accepted_mints: &[String],
    onion_address: Option<&str>,
) -> Vec<Vec<String>> {
    let mut tags: Vec<Vec<String>> = Vec::new();

    // Endpoint tag (if we have a Tor address)
    if let Some(onion) = onion_address {
        tags.push(vec!["endpoint".to_string(), onion.to_string()]);
    }

    // Supported TIPs
    tags.push(vec![
        "tips".to_string(),
        "TIP-01".to_string(),
        "TIP-02".to_string(),
    ]);

    // One set of tags per enabled service
    for service in config.services.iter().filter(|s| s.enabled) {
        tags.push(vec![
            "service".to_string(),
            service.service_id.clone(),
            service.name.clone(),
        ]);

        tags.push(vec![
            "metric".to_string(),
            service.service_id.clone(),
            service.metric.to_string(),
        ]);

        tags.push(vec![
            "step_size".to_string(),
            service.service_id.clone(),
            service.step_size.to_string(),
        ]);

        // Price tags — one per accepted mint (TIP-02 format)
        let mints = if service.accepted_mints.is_empty() {
            accepted_mints.to_vec()
        } else {
            service.accepted_mints.clone()
        };

        for mint_url in &mints {
            tags.push(vec![
                "price_per_step".to_string(),
                service.service_id.clone(),
                "cashu".to_string(),
                service.price_per_step.to_string(),
                "sat".to_string(),
                mint_url.clone(),
                service.min_steps.to_string(),
            ]);
        }
    }

    tags
}

/// Build the content string for a kind 10021 advertisement event.
pub fn build_advertisement_content(config: &PricingConfig) -> String {
    let enabled: Vec<_> = config.services.iter().filter(|s| s.enabled).collect();
    if enabled.is_empty() {
        return "No streaming services available".to_string();
    }

    let mut lines = vec!["Streaming data services available:".to_string()];
    for service in &enabled {
        lines.push(format!(
            "- {} ({}: {} sats per {} {})",
            service.name,
            service.service_id,
            service.price_per_step,
            service.step_size,
            service.metric,
        ));
    }
    lines.join("\n")
}

/// Parse a Nostr event's tags into a ServiceAdvertisement.
pub fn parse_advertisement_tags(
    pubkey: &str,
    tags: &[Vec<String>],
    created_at: &str,
) -> ServiceAdvertisement {
    let mut ad = ServiceAdvertisement {
        pubkey: pubkey.to_string(),
        endpoint: String::new(),
        services: Vec::new(),
        supported_tips: Vec::new(),
        created_at: created_at.to_string(),
    };

    // Collect service IDs first
    let mut service_ids: Vec<String> = Vec::new();
    let mut service_names: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut service_metrics: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut service_step_sizes: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    let mut service_prices: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();
    let mut service_mints: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut service_min_steps: std::collections::HashMap<String, u64> =
        std::collections::HashMap::new();

    for tag in tags {
        if tag.is_empty() {
            continue;
        }
        match tag[0].as_str() {
            "endpoint" if tag.len() >= 2 => {
                ad.endpoint = tag[1].clone();
            }
            "tips" => {
                ad.supported_tips = tag[1..].to_vec();
            }
            "service" if tag.len() >= 3 => {
                let sid = tag[1].clone();
                service_names.insert(sid.clone(), tag[2].clone());
                if !service_ids.contains(&sid) {
                    service_ids.push(sid);
                }
            }
            "metric" if tag.len() >= 3 => {
                service_metrics.insert(tag[1].clone(), tag[2].clone());
            }
            "step_size" if tag.len() >= 3 => {
                if let Ok(v) = tag[2].parse() {
                    service_step_sizes.insert(tag[1].clone(), v);
                }
            }
            "price_per_step" if tag.len() >= 5 => {
                // ["price_per_step", service_id, "cashu", price, "sat", mint_url, min_steps]
                let sid = tag[1].clone();
                if let Ok(price) = tag[3].parse::<u64>() {
                    service_prices.insert(sid.clone(), price);
                }
                if tag.len() >= 6 {
                    service_mints
                        .entry(sid.clone())
                        .or_default()
                        .push(tag[5].clone());
                }
                if tag.len() >= 7 {
                    if let Ok(ms) = tag[6].parse::<u64>() {
                        service_min_steps.insert(sid, ms);
                    }
                }
            }
            _ => {}
        }
    }

    // Assemble services
    for sid in &service_ids {
        ad.services.push(AdvertisedService {
            service_id: sid.clone(),
            name: service_names.get(sid).cloned().unwrap_or_default(),
            metric: service_metrics.get(sid).cloned().unwrap_or_default(),
            step_size: service_step_sizes.get(sid).copied().unwrap_or(0),
            price_per_step: service_prices.get(sid).copied().unwrap_or(0),
            unit: "sat".to_string(),
            mint_urls: service_mints.get(sid).cloned().unwrap_or_default(),
            min_steps: service_min_steps.get(sid).copied().unwrap_or(0),
            description: String::new(),
        });
    }

    ad
}

/// Build a kind 1022 session event content (proof of access grant).
pub fn build_session_event_content(
    session_id: &str,
    peer_pubkey: &str,
    service_id: &str,
    allotment: u64,
    metric: &str,
    paid_sats: u64,
) -> serde_json::Value {
    serde_json::json!({
        "session_id": session_id,
        "customer": peer_pubkey,
        "service": service_id,
        "allotment": allotment,
        "metric": metric,
        "paid_sats": paid_sats,
        "granted_at": chrono::Utc::now().to_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streaming::pricing::Metric;

    fn test_config() -> PricingConfig {
        PricingConfig {
            services: vec![
                ServicePricing {
                    service_id: "content-download".into(),
                    name: "Content Downloads".into(),
                    metric: Metric::Bytes,
                    step_size: 1_048_576,
                    price_per_step: 1,
                    min_steps: 0,
                    enabled: true,
                    description: "test".into(),
                    accepted_mints: vec![],
                },
                ServicePricing {
                    service_id: "disabled-service".into(),
                    name: "Disabled".into(),
                    metric: Metric::Requests,
                    step_size: 1,
                    price_per_step: 1,
                    min_steps: 0,
                    enabled: false,
                    description: "disabled".into(),
                    accepted_mints: vec![],
                },
            ],
        }
    }

    #[test]
    fn test_build_advertisement_tags() {
        let config = test_config();
        let mints = vec!["http://mint.example.com".to_string()];
        let tags = build_advertisement_tags(&config, &mints, Some("abc123.onion"));

        // Should have endpoint, tips, service, metric, step_size, price_per_step
        assert!(tags.iter().any(|t| t[0] == "endpoint"));
        assert!(tags.iter().any(|t| t[0] == "tips"));
        assert!(tags
            .iter()
            .any(|t| t[0] == "service" && t[1] == "content-download"));
        assert!(tags
            .iter()
            .any(|t| t[0] == "metric" && t[1] == "content-download"));
        assert!(tags
            .iter()
            .any(|t| t[0] == "price_per_step" && t[1] == "content-download"));

        // Disabled service should NOT appear
        assert!(!tags
            .iter()
            .any(|t| t.len() > 1 && t[1] == "disabled-service"));
    }

    #[test]
    fn test_build_advertisement_content() {
        let config = test_config();
        let content = build_advertisement_content(&config);
        assert!(content.contains("Content Downloads"));
        assert!(!content.contains("Disabled"));
    }

    #[test]
    fn test_parse_advertisement_tags_roundtrip() {
        let config = test_config();
        let mints = vec!["http://mint.example.com".to_string()];
        let tags = build_advertisement_tags(&config, &mints, Some("abc123.onion"));

        let ad = parse_advertisement_tags("deadbeef", &tags, "2025-01-01T00:00:00Z");
        assert_eq!(ad.pubkey, "deadbeef");
        assert_eq!(ad.endpoint, "abc123.onion");
        assert_eq!(ad.services.len(), 1);
        assert_eq!(ad.services[0].service_id, "content-download");
        assert_eq!(ad.services[0].price_per_step, 1);
        assert_eq!(ad.services[0].step_size, 1_048_576);
        assert_eq!(ad.services[0].metric, "bytes");
    }

    #[test]
    fn test_build_session_event_content() {
        let content = build_session_event_content(
            "session-123",
            "peer-pubkey",
            "content-download",
            10_485_760,
            "bytes",
            10,
        );
        assert_eq!(content["session_id"], "session-123");
        assert_eq!(content["paid_sats"], 10);
    }
}
