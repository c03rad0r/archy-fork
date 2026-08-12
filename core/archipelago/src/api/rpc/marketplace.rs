use super::RpcHandler;
use crate::federation;
use crate::marketplace;
use crate::nostr_relays;
use anyhow::Result;
use tracing::info;

impl RpcHandler {
    /// marketplace.discover — Query Nostr relays for community app manifests.
    pub(super) async fn handle_marketplace_discover(&self) -> Result<serde_json::Value> {
        // Load enabled relays
        let relay_store = nostr_relays::load_relays(&self.config.data_dir).await?;
        let relay_urls: Vec<String> = relay_store
            .relays
            .iter()
            .filter(|r| r.enabled)
            .map(|r| r.url.clone())
            .collect();

        // Load federated DIDs for trust scoring
        let fed_nodes = federation::load_nodes(&self.config.data_dir)
            .await
            .unwrap_or_default();
        let federated_dids: Vec<String> = fed_nodes.iter().map(|n| n.did.clone()).collect();

        let tor_proxy = std::env::var("ARCHIPELAGO_NOSTR_TOR_PROXY").ok();
        let apps = marketplace::discover(
            &self.config.data_dir,
            &relay_urls,
            tor_proxy.as_deref(),
            &federated_dids,
        )
        .await?;

        Ok(serde_json::json!({
            "apps": apps,
            "relay_count": relay_urls.len(),
        }))
    }

    /// marketplace.publish — Publish an app manifest to Nostr relays.
    pub(super) async fn handle_marketplace_publish(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let manifest: marketplace::AppManifest = serde_json::from_value(params)
            .map_err(|e| anyhow::anyhow!("Invalid manifest: {}", e))?;

        // Validate before publishing
        let issues = marketplace::validate_manifest(&manifest);
        if !issues.is_empty() {
            return Ok(serde_json::json!({
                "ok": false,
                "errors": issues,
            }));
        }

        let relay_store = nostr_relays::load_relays(&self.config.data_dir).await?;
        let relay_urls: Vec<String> = relay_store
            .relays
            .iter()
            .filter(|r| r.enabled)
            .map(|r| r.url.clone())
            .collect();

        let tor_proxy = std::env::var("ARCHIPELAGO_NOSTR_TOR_PROXY").ok();
        let event_id = marketplace::publish(
            &self.config.data_dir,
            &manifest,
            &relay_urls,
            tor_proxy.as_deref(),
        )
        .await?;

        info!(app_id = %manifest.app_id, "Published app manifest");
        Ok(serde_json::json!({
            "ok": true,
            "event_id": event_id,
            "app_id": manifest.app_id,
            "relays": relay_urls.len(),
        }))
    }

    /// marketplace.get-manifest — Get cached manifest for a specific app.
    pub(super) async fn handle_marketplace_get_manifest(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let app_id = params
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: app_id"))?;

        let cache = marketplace::load_cache(&self.config.data_dir).await?;
        let app = cache.apps.iter().find(|a| a.manifest.app_id == app_id);

        match app {
            Some(discovered) => Ok(serde_json::to_value(discovered)?),
            None => Ok(serde_json::json!({ "error": "App not found in cache", "app_id": app_id })),
        }
    }

    /// marketplace.list-published — List manifests published by this node.
    pub(super) async fn handle_marketplace_list_published(&self) -> Result<serde_json::Value> {
        let manifests = marketplace::list_published(&self.config.data_dir).await?;
        Ok(serde_json::json!({ "manifests": manifests }))
    }

    /// marketplace.verify — Verify a manifest's security compliance.
    pub(super) async fn handle_marketplace_verify(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let manifest: marketplace::AppManifest = serde_json::from_value(params)
            .map_err(|e| anyhow::anyhow!("Invalid manifest: {}", e))?;

        let issues = marketplace::validate_manifest(&manifest);
        let signature = marketplace::verify_manifest_signature(&manifest);
        let (trust_score, trust_tier) =
            marketplace::calculate_trust_score(&manifest, 0, &[], &signature);

        Ok(serde_json::json!({
            // Security-policy compliance (advisory — these are score inputs).
            "valid": issues.is_empty(),
            "issues": issues,
            // Cryptographic authorship: valid | missing | invalid(+reason).
            // This is the one that says whether author.did is proven.
            "signature": signature,
            "signature_valid": signature.is_valid(),
            "trust_score": trust_score,
            "trust_tier": trust_tier,
        }))
    }

    /// marketplace.create-invoice — Generate a Lightning invoice for app purchase.
    /// Returns BOLT11 invoice string and payment hash.
    pub(super) async fn handle_marketplace_create_invoice(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let app_id = params
            .get("app_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing app_id"))?;
        let amount_sats = params
            .get("amount_sats")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing amount_sats"))?;

        // Create LND invoice via the existing wallet integration
        let invoice_params = serde_json::json!({
            "amount": amount_sats,
            "memo": format!("Archipelago app: {}", app_id),
        });
        let invoice_result = self.handle_lnd_createinvoice(Some(invoice_params)).await?;

        let payment_request = invoice_result
            .get("payment_request")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let r_hash = invoice_result
            .get("r_hash")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        Ok(serde_json::json!({
            "app_id": app_id,
            "amount_sats": amount_sats,
            "payment_request": payment_request,
            "r_hash": r_hash,
        }))
    }

    /// marketplace.check-payment — Check if a Lightning payment has been received.
    pub(super) async fn handle_marketplace_check_payment(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let r_hash = params
            .get("r_hash")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing r_hash"))?;

        // Validate r_hash is hex-encoded (LND payment hashes are 32 bytes = 64 hex chars)
        if r_hash.len() != 64 || !r_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(anyhow::anyhow!(
                "Invalid r_hash: must be 64-character hex string"
            ));
        }

        let (client, macaroon_hex) = self.lnd_client().await?;

        let url = format!("{}/v1/invoice/{r_hash}", super::lnd::LND_REST_BASE_URL);
        let paid = match client
            .get(&url)
            .header("Grpc-Metadata-macaroon", &macaroon_hex)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => {
                let body: serde_json::Value = r.json().await.unwrap_or_default();
                body.get("settled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            }
            _ => false,
        };

        Ok(serde_json::json!({
            "r_hash": r_hash,
            "paid": paid,
        }))
    }
}
