//! Webhook notification system.
//!
//! Sends HTTP POST notifications to a configured webhook URL when
//! system events occur (container crashes, updates, disk warnings, etc).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::{debug, warn};

const WEBHOOK_CONFIG_FILE: &str = "webhook-config.json";

/// Events that can trigger webhook notifications.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEvent {
    ContainerCrash,
    UpdateAvailable,
    DiskWarning,
    BackupComplete,
}

/// Persisted webhook configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub enabled: bool,
    pub url: String,
    pub events: Vec<WebhookEvent>,
    #[serde(default)]
    pub secret: Option<String>,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: String::new(),
            events: vec![
                WebhookEvent::ContainerCrash,
                WebhookEvent::UpdateAvailable,
                WebhookEvent::DiskWarning,
                WebhookEvent::BackupComplete,
            ],
            secret: None,
        }
    }
}

/// Payload sent to the webhook URL.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
    pub event: WebhookEvent,
    pub title: String,
    pub message: String,
    pub timestamp: String,
    pub node_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

pub async fn load_config(data_dir: &Path) -> Result<WebhookConfig> {
    let path = data_dir.join(WEBHOOK_CONFIG_FILE);
    if !path.exists() {
        return Ok(WebhookConfig::default());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read webhook config")?;
    let config: WebhookConfig = serde_json::from_str(&content).unwrap_or_default();
    Ok(config)
}

pub async fn save_config(data_dir: &Path, config: &WebhookConfig) -> Result<()> {
    fs::create_dir_all(data_dir)
        .await
        .context("Failed to create data dir")?;
    let content =
        serde_json::to_string_pretty(config).context("Failed to serialize webhook config")?;
    fs::write(data_dir.join(WEBHOOK_CONFIG_FILE), content)
        .await
        .context("Failed to write webhook config")?;
    Ok(())
}

/// Send a webhook notification (non-blocking, fire-and-forget).
pub async fn send_webhook(data_dir: &Path, payload: WebhookPayload) {
    let config = match load_config(data_dir).await {
        Ok(c) => c,
        Err(e) => {
            debug!("Webhook config not loaded: {}", e);
            return;
        }
    };

    if !config.enabled || config.url.is_empty() {
        return;
    }

    // Check if this event type is subscribed
    if !config.events.contains(&payload.event) {
        debug!("Webhook event {:?} not subscribed", payload.event);
        return;
    }

    let url = config.url.clone();
    let secret = config.secret.clone();

    tokio::spawn(async move {
        if let Err(e) = send_http_webhook(&url, &payload, secret.as_deref()).await {
            warn!("Webhook delivery failed: {}", e);
        } else {
            debug!("Webhook delivered to {}", url);
        }
    });
}

async fn send_http_webhook(
    url: &str,
    payload: &WebhookPayload,
    secret: Option<&str>,
) -> Result<()> {
    let body = serde_json::to_string(payload).context("Failed to serialize webhook payload")?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("Failed to create HTTP client")?;

    let mut request = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "Archipelago-Webhook/1.0");

    // Add HMAC signature if secret is configured
    if let Some(secret) = secret {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).context("Invalid HMAC key")?;
        mac.update(body.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());
        request = request.header("X-Webhook-Signature", format!("sha256={}", signature));
    }

    let response = request
        .body(body)
        .send()
        .await
        .context("Failed to send webhook")?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("Webhook returned {}: {}", status, text);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WebhookConfig::default();
        assert!(!config.enabled);
        assert!(config.url.is_empty());
        assert_eq!(config.events.len(), 4);
    }

    #[test]
    fn test_config_serialization() {
        let config = WebhookConfig {
            enabled: true,
            url: "https://hooks.example.com/notify".to_string(),
            events: vec![WebhookEvent::ContainerCrash, WebhookEvent::DiskWarning],
            secret: Some("my-secret".to_string()),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: WebhookConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.events.len(), 2);
        assert_eq!(parsed.secret, Some("my-secret".to_string()));
    }

    #[test]
    fn test_payload_serialization() {
        let payload = WebhookPayload {
            event: WebhookEvent::ContainerCrash,
            title: "Bitcoin stopped".to_string(),
            message: "Container bitcoin-knots has stopped unexpectedly".to_string(),
            timestamp: "2026-03-11T12:00:00Z".to_string(),
            node_id: "test-node-id".to_string(),
            details: Some(serde_json::json!({"container": "bitcoin-knots"})),
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("container_crash"));
        assert!(json.contains("bitcoin-knots"));
    }

    #[tokio::test]
    async fn test_load_config_default() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_config(dir.path()).await.unwrap();
        assert!(!config.enabled);
    }

    #[tokio::test]
    async fn test_save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let config = WebhookConfig {
            enabled: true,
            url: "https://example.com/hook".to_string(),
            events: vec![WebhookEvent::BackupComplete],
            secret: None,
        };
        save_config(dir.path(), &config).await.unwrap();
        let loaded = load_config(dir.path()).await.unwrap();
        assert!(loaded.enabled);
        assert_eq!(loaded.url, "https://example.com/hook");
        assert_eq!(loaded.events.len(), 1);
    }
}
