//! RPC handlers for streaming ecash payments.
//!
//! Endpoints for managing priced services, processing payments,
//! checking sessions/usage, and publishing service advertisements.

use super::RpcHandler;
use crate::streaming::{advertisement, gate, meter, pricing, session};
use crate::wallet::ecash;
use anyhow::Result;

impl RpcHandler {
    // ── Service pricing management ──

    /// List all configured streaming services and their pricing.
    pub(super) async fn handle_streaming_list_services(&self) -> Result<serde_json::Value> {
        let config = pricing::load_pricing(&self.config.data_dir).await?;
        Ok(serde_json::json!({
            "services": config.services,
        }))
    }

    /// Configure pricing for a streaming service.
    pub(super) async fn handle_streaming_configure_service(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;

        let service_id = params
            .get("service_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing service_id"))?;
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(service_id);
        let metric_str = params
            .get("metric")
            .and_then(|v| v.as_str())
            .unwrap_or("requests");
        let step_size = params
            .get("step_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        let price_per_step = params
            .get("price_per_step")
            .and_then(|v| v.as_u64())
            .unwrap_or(1);
        let min_steps = params
            .get("min_steps")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let enabled = params
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let description = params
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let metric = match metric_str {
            "bytes" => pricing::Metric::Bytes,
            "milliseconds" | "time" => pricing::Metric::Milliseconds,
            "requests" => pricing::Metric::Requests,
            _ => return Err(anyhow::anyhow!("Invalid metric: {}", metric_str)),
        };

        let accepted_mints: Vec<String> = params
            .get("accepted_mints")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let service = pricing::ServicePricing {
            service_id: service_id.to_string(),
            name: name.to_string(),
            metric,
            step_size,
            price_per_step,
            min_steps,
            enabled,
            description: description.to_string(),
            accepted_mints,
        };
        service.validate()?;

        let mut config = pricing::load_pricing(&self.config.data_dir).await?;

        // Update existing or add new
        if let Some(existing) = config
            .services
            .iter_mut()
            .find(|s| s.service_id == service_id)
        {
            *existing = service.clone();
        } else {
            config.services.push(service.clone());
        }

        pricing::save_pricing(&self.config.data_dir, &config).await?;

        Ok(serde_json::json!({
            "service": service,
            "updated": true,
        }))
    }

    /// Enable or disable a streaming service.
    pub(super) async fn handle_streaming_toggle_service(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let service_id = params
            .get("service_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing service_id"))?;
        let enabled = params
            .get("enabled")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("Missing enabled"))?;

        let mut config = pricing::load_pricing(&self.config.data_dir).await?;
        if let Some(service) = config
            .services
            .iter_mut()
            .find(|s| s.service_id == service_id)
        {
            service.enabled = enabled;
            pricing::save_pricing(&self.config.data_dir, &config).await?;
            Ok(serde_json::json!({
                "service_id": service_id,
                "enabled": enabled,
            }))
        } else {
            Err(anyhow::anyhow!("Service '{}' not found", service_id))
        }
    }

    // ── Payment processing ──

    /// Process a streaming payment — submit a Cashu token for a service.
    /// Returns session details with allotment on success.
    pub(super) async fn handle_streaming_pay(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let service_id = params
            .get("service_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing service_id"))?;
        let token = params
            .get("token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing token (cashuA token string)"))?;
        let peer_id = params
            .get("peer_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing peer_id"))?;

        if token.is_empty() {
            return Err(anyhow::anyhow!("Token cannot be empty"));
        }
        if peer_id.is_empty() {
            return Err(anyhow::anyhow!("Peer ID cannot be empty"));
        }

        let result =
            gate::check_gate(&self.config.data_dir, peer_id, service_id, Some(token), 0).await?;

        match result {
            gate::GateResult::PaidAndAllowed {
                session_id,
                allotment,
                paid_sats,
            } => Ok(serde_json::json!({
                "status": "paid",
                "session_id": session_id,
                "allotment": allotment,
                "paid_sats": paid_sats,
            })),
            gate::GateResult::InsufficientPayment {
                provided_sats,
                minimum_sats,
            } => Ok(serde_json::json!({
                "status": "insufficient",
                "error": { "code": "insufficient_payment", "message": format!("Need {} sats, got {}", minimum_sats, provided_sats) },
                "minimum_sats": minimum_sats,
                "provided_sats": provided_sats,
            })),
            gate::GateResult::PaymentFailed { reason } => Ok(serde_json::json!({
                "status": "failed",
                "error": { "code": "payment_failed", "message": reason },
            })),
            gate::GateResult::ServiceUnavailable => {
                Err(anyhow::anyhow!("Service '{}' not available", service_id))
            }
            _ => Err(anyhow::anyhow!("Unexpected gate result")),
        }
    }

    /// Build a payment token for a remote seeder (payer side, cross-mint aware).
    ///
    /// Given the seeder's advertised `accepted_mints` and `price_sats`, builds a
    /// `cashuA` token denominated in one of those mints — paying directly if we
    /// already hold the right mint, else auto-swapping into a trusted accepted
    /// mint (within `max_fee_sats`). If the price is over `budget_sats`, the
    /// wallet can't cover it, or the swap is too costly, returns `declined` so
    /// the caller falls back to the free origin (origin always wins).
    pub(super) async fn handle_streaming_prepare_payment(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let accepted_mints: Vec<String> = params
            .get("accepted_mints")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let price_sats = params
            .get("price_sats")
            .or_else(|| params.get("amount_sats"))
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing price_sats"))?;
        // Default budget = the asked price (willing to pay exactly what's quoted).
        let budget_sats = params
            .get("budget_sats")
            .and_then(|v| v.as_u64())
            .unwrap_or(price_sats);
        let max_fee_sats = params
            .get("max_fee_sats")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let policy = crate::swarm::payment::PaymentPolicy::with_budget(budget_sats, max_fee_sats);
        match crate::swarm::payment::auto_pay_token(
            &self.config.data_dir,
            &policy,
            &accepted_mints,
            price_sats,
        )
        .await?
        {
            Some(token) => Ok(serde_json::json!({
                "status": "ready",
                "token": token,
                "paid_sats": price_sats,
            })),
            None => Ok(serde_json::json!({
                "status": "declined",
                "message": "payment declined (over budget, unpayable, or swap too costly) — use free origin",
            })),
        }
    }

    /// Discover available streaming services (pricing info).
    /// This is the unauthenticated discovery endpoint.
    pub(super) async fn handle_streaming_discover(&self) -> Result<serde_json::Value> {
        let config = pricing::load_pricing(&self.config.data_dir).await?;
        let accepted_mints = ecash::load_accepted_mints(&self.config.data_dir).await?;

        let services: Vec<serde_json::Value> = config
            .services
            .iter()
            .filter(|s| s.enabled)
            .map(|s| {
                let mints = if s.accepted_mints.is_empty() {
                    &accepted_mints.mints
                } else {
                    &s.accepted_mints
                };
                serde_json::json!({
                    "service_id": s.service_id,
                    "name": s.name,
                    "description": s.description,
                    "metric": s.metric,
                    "step_size": s.step_size,
                    "price_per_step": s.price_per_step,
                    "min_steps": s.min_steps,
                    "minimum_sats": s.minimum_payment(),
                    "accepted_mints": mints,
                })
            })
            .collect();

        Ok(serde_json::json!({
            "services": services,
        }))
    }

    // ── Session management ──

    /// Check usage for a peer's active session.
    pub(super) async fn handle_streaming_usage(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let peer_id = params
            .get("peer_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing peer_id"))?;
        let service_id = params
            .get("service_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing service_id"))?;

        match meter::get_peer_usage(&self.config.data_dir, peer_id, service_id).await? {
            Some(usage) => Ok(serde_json::json!({ "usage": usage })),
            None => Ok(serde_json::json!({
                "usage": null,
                "message": "No active session",
            })),
        }
    }

    /// Get details of a specific session by ID.
    pub(super) async fn handle_streaming_session(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing session_id"))?;

        let store = session::load_sessions(&self.config.data_dir).await?;
        match store.get(session_id) {
            Some(s) => Ok(serde_json::json!({ "session": s })),
            None => Err(anyhow::anyhow!("Session not found")),
        }
    }

    /// List all active streaming sessions (admin view).
    pub(super) async fn handle_streaming_list_sessions(&self) -> Result<serde_json::Value> {
        let store = session::load_sessions(&self.config.data_dir).await?;
        let active = store.active_sessions();
        let revenue = store.total_revenue();
        let by_service = store.revenue_by_service();

        Ok(serde_json::json!({
            "sessions": active,
            "total_active": active.len(),
            "total_revenue_sats": revenue,
            "revenue_by_service": by_service,
        }))
    }

    /// Close a specific session.
    pub(super) async fn handle_streaming_close_session(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let session_id = params
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing session_id"))?;

        let mut store = session::load_sessions(&self.config.data_dir).await?;
        if let Some(s) = store.get_mut(session_id) {
            s.close();
            session::save_sessions(&self.config.data_dir, &store).await?;
            Ok(serde_json::json!({ "closed": true }))
        } else {
            Err(anyhow::anyhow!("Session not found"))
        }
    }

    // ── Advertisement ──

    /// Publish a streaming service advertisement to Nostr relays.
    pub(super) async fn handle_streaming_advertise(&self) -> Result<serde_json::Value> {
        let config = pricing::load_pricing(&self.config.data_dir).await?;
        let accepted_mints = ecash::load_accepted_mints(&self.config.data_dir).await?;

        let enabled_count = config.services.iter().filter(|s| s.enabled).count();
        if enabled_count == 0 {
            return Err(anyhow::anyhow!("No enabled services to advertise"));
        }

        // Get node's onion address for the endpoint tag
        let onion = crate::container::docker_packages::read_tor_address("archipelago").await;

        let tags = advertisement::build_advertisement_tags(
            &config,
            &accepted_mints.mints,
            onion.as_deref(),
        );
        let content = advertisement::build_advertisement_content(&config);

        Ok(serde_json::json!({
            "kind": advertisement::KIND_SERVICE_ADVERTISEMENT,
            "content": content,
            "tags": tags,
            "services_count": enabled_count,
            "ready_to_publish": true,
        }))
    }

    // ── Accepted mints management ──

    /// List accepted mints for streaming payments.
    pub(super) async fn handle_streaming_list_mints(&self) -> Result<serde_json::Value> {
        let mints = ecash::load_accepted_mints(&self.config.data_dir).await?;
        Ok(serde_json::json!({ "mints": mints.mints }))
    }

    /// Add or remove accepted mints.
    pub(super) async fn handle_streaming_configure_mints(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let mints = params
            .get("mints")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing mints array"))?;

        let mint_urls: Vec<String> = mints
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        if mint_urls.is_empty() {
            return Err(anyhow::anyhow!("Must have at least one accepted mint"));
        }

        // Basic validation
        for url in &mint_urls {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(anyhow::anyhow!("Invalid mint URL: {}", url));
            }
        }

        let config = ecash::AcceptedMints {
            mints: mint_urls.clone(),
        };
        ecash::save_accepted_mints(&self.config.data_dir, &config).await?;

        Ok(serde_json::json!({
            "mints": mint_urls,
            "updated": true,
        }))
    }

    // ── Maintenance ──

    /// Run streaming maintenance (close expired sessions, prune old records).
    pub(super) async fn handle_streaming_maintenance(&self) -> Result<serde_json::Value> {
        let closed = meter::maintenance(&self.config.data_dir).await?;
        Ok(serde_json::json!({
            "expired_closed": closed,
        }))
    }
}
