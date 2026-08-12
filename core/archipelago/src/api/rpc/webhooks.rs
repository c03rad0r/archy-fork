use super::RpcHandler;
use crate::webhooks;
use anyhow::Result;
use tracing::info;

/// Check if a hostname/IP points to a private or internal address.
/// Handles: IPv4, IPv6 (including mapped IPv4 like ::ffff:127.0.0.1),
/// decimal/octal IP representations, and well-known internal hostnames.
fn is_webhook_host_private(host: &str) -> bool {
    // Strip IPv6 brackets if present
    let h = host.trim_start_matches('[').trim_end_matches(']');

    // Check well-known internal hostnames
    let lower = h.to_lowercase();
    if lower == "localhost"
        || lower == "localhost.localdomain"
        || lower.ends_with(".local")
        || lower.ends_with(".internal")
        || lower == "metadata.google.internal"
        || lower == "169.254.169.254"
    {
        return true;
    }

    // Try to parse as IP address
    if let Ok(ip) = h.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.octets()[0] == 100 && (64..=127).contains(&v4.octets()[1])
                // CGNAT
            }
            std::net::IpAddr::V6(v6) => {
                if v6.is_loopback() || v6.is_unspecified() {
                    return true;
                }
                // Check IPv4-mapped IPv6 (::ffff:x.x.x.x)
                if let Some(v4) = v6.to_ipv4_mapped() {
                    return v4.is_loopback()
                        || v4.is_private()
                        || v4.is_link_local()
                        || v4.is_unspecified();
                }
                // Unique local (fd00::/8, fc00::/7)
                let segments = v6.segments();
                (segments[0] & 0xfe00) == 0xfc00 || (segments[0] & 0xffc0) == 0xfe80
                // link-local
            }
        };
    }

    // Detect decimal IP notation (e.g., "2130706433" = 127.0.0.1)
    if let Ok(decimal) = h.parse::<u32>() {
        let octets = decimal.to_be_bytes();
        let v4 = std::net::Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3]);
        return v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified();
    }

    // Detect octal IP notation (e.g., "0177.0.0.1" = 127.0.0.1)
    if h.contains('.') {
        let parts: Vec<&str> = h.split('.').collect();
        if parts.len() == 4 {
            let mut octets = [0u8; 4];
            let mut all_ok = true;
            for (i, part) in parts.iter().enumerate() {
                let val = if part.starts_with("0x") || part.starts_with("0X") {
                    u64::from_str_radix(part.trim_start_matches("0x").trim_start_matches("0X"), 16)
                        .ok()
                } else if part.starts_with('0') && part.len() > 1 {
                    u64::from_str_radix(part, 8).ok()
                } else {
                    part.parse::<u64>().ok()
                };
                match val {
                    Some(v) if v <= 255 => octets[i] = v as u8,
                    _ => {
                        all_ok = false;
                        break;
                    }
                }
            }
            if all_ok {
                let v4 = std::net::Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3]);
                return v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_unspecified();
            }
        }
    }

    false
}

impl RpcHandler {
    /// webhook.get-config — Get current webhook configuration.
    pub(super) async fn handle_webhook_get_config(&self) -> Result<serde_json::Value> {
        let config = webhooks::load_config(&self.config.data_dir).await?;
        Ok(serde_json::json!({
            "enabled": config.enabled,
            "url": config.url,
            "events": config.events,
            "has_secret": config.secret.is_some(),
        }))
    }

    /// webhook.configure — Update webhook configuration.
    pub(super) async fn handle_webhook_configure(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let mut config = webhooks::load_config(&self.config.data_dir).await?;

        if let Some(enabled) = params.get("enabled").and_then(|v| v.as_bool()) {
            config.enabled = enabled;
        }
        if let Some(url) = params.get("url").and_then(|v| v.as_str()) {
            // Validate webhook URL scheme and reject dangerous targets
            if !url.is_empty() {
                if url.len() > 2048 {
                    anyhow::bail!("Webhook URL too long");
                }
                // Parse URL properly to handle edge cases (IPv6, userinfo, etc.)
                let parsed =
                    reqwest::Url::parse(url).map_err(|_| anyhow::anyhow!("Invalid webhook URL"))?;
                // Require https:// in production
                if !self.config.dev_mode && parsed.scheme() != "https" {
                    anyhow::bail!("Webhook URL must use HTTPS in production");
                }
                if parsed.scheme() != "https" && parsed.scheme() != "http" {
                    anyhow::bail!("Webhook URL must use HTTP(S)");
                }
                // Reject URLs with userinfo (user:pass@host) — can be used for credential smuggling
                if parsed.username() != "" || parsed.password().is_some() {
                    anyhow::bail!("Webhook URL must not contain credentials");
                }
                // Extract and validate the host
                let host = parsed.host_str().unwrap_or("");
                if host.is_empty() {
                    anyhow::bail!("Webhook URL must have a valid host");
                }
                // Reject private/internal addresses (handle IPv4, IPv6, decimal/octal IPs, hostnames)
                let is_private = is_webhook_host_private(host);
                if is_private && !self.config.dev_mode {
                    anyhow::bail!("Webhook URL must not point to private/local addresses");
                }
            }
            config.url = url.to_string();
        }
        if let Some(secret) = params.get("secret").and_then(|v| v.as_str()) {
            config.secret = if secret.is_empty() {
                None
            } else {
                Some(secret.to_string())
            };
        }
        if let Some(events) = params.get("events") {
            if let Ok(parsed) =
                serde_json::from_value::<Vec<webhooks::WebhookEvent>>(events.clone())
            {
                config.events = parsed;
            }
        }

        webhooks::save_config(&self.config.data_dir, &config).await?;
        info!(
            "Webhook config updated: enabled={}, url={}",
            config.enabled, config.url
        );

        Ok(serde_json::json!({
            "configured": true,
            "enabled": config.enabled,
            "url": config.url,
        }))
    }

    /// webhook.test — Send a test webhook notification.
    pub(super) async fn handle_webhook_test(&self) -> Result<serde_json::Value> {
        let config = webhooks::load_config(&self.config.data_dir).await?;
        if !config.enabled || config.url.is_empty() {
            anyhow::bail!("Webhook is not configured. Set a URL and enable it first.");
        }

        let payload = webhooks::WebhookPayload {
            event: webhooks::WebhookEvent::ContainerCrash,
            title: "Test Notification".to_string(),
            message: "This is a test webhook from your Archipelago node.".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            node_id: {
                let (data, _) = self.state_manager.get_snapshot().await;
                data.server_info.id
            },
            details: Some(serde_json::json!({"test": true})),
        };

        webhooks::send_webhook(&self.config.data_dir, payload).await;

        Ok(serde_json::json!({
            "sent": true,
            "url": config.url,
        }))
    }
}
