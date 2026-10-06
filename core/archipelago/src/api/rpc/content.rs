use super::RpcHandler;
use crate::content_server::{self, AccessControl, Availability, ContentItem};
use crate::network::dwn_store::DwnStore;
use crate::wallet::ecash;
use anyhow::{Context, Result};
use tracing::debug;

/// Validate a v3 Tor onion address.
/// Must be exactly 62 chars: 56 base32 characters (a-z, 2-7) followed by ".onion".
fn is_valid_v3_onion(addr: &str) -> bool {
    if addr.len() != 62 || !addr.ends_with(".onion") {
        return false;
    }
    let prefix = &addr[..56];
    prefix
        .chars()
        .all(|c| c.is_ascii_lowercase() || ('2'..='7').contains(&c))
}

const FILE_CATALOG_PROTOCOL: &str = "https://archipelago.dev/protocols/file-catalog/v1";

/// Best-effort reclaim of an ecash payment token that was minted but the sale
/// didn't complete (seller unreachable or couldn't redeem it), so the buyer
/// doesn't lose the value. For Fedimint the spender can reissue its own
/// un-redeemed notes; for Cashu the proofs are received back. Report the actual
/// recovered amount, or explicitly say when a refund could not be confirmed.
async fn reclaim_spent_ecash(data_dir: &std::path::Path, token: &str, backend: &str) -> String {
    let res = match backend {
        "fedimint" => crate::wallet::fedimint_client::reissue_into_any(data_dir, token)
            .await
            .map(|(sats, _fed)| sats),
        _ => ecash::receive_token(data_dir, token).await,
    };
    match res {
        Ok(sats) => {
            tracing::info!("paid download: reclaimed {sats} sats after failed sale");
            format!("Refunded {sats} sats to your wallet.")
        }
        Err(e) => {
            tracing::warn!("paid download: refund not confirmed: {e}");
            "Your refund could not be confirmed. The seller may have received the payment. Do not pay again until this is checked.".to_string()
        }
    }
}

/// Only pass through the peer's bounded, printable explanation; refund status
/// is always determined locally and must never come from the peer's wording.
fn seller_error_message(status: reqwest::StatusCode, body: &str) -> String {
    let reason = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_owned));
    match reason {
        Some(reason) if !reason.trim().is_empty() => {
            let clean: String = reason
                .chars()
                .filter(|c| !c.is_control())
                .take(240)
                .collect();
            format!("Seller response ({status}): {clean}")
        }
        _ => format!("Peer returned an error ({status})."),
    }
}

/// Keep first purchases and cached repeats compatible with both existing clients.
fn paid_content_response(bytes: &[u8], mime: &str, paid_sats: u64) -> serde_json::Value {
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    serde_json::json!({
        "data": data, "data_base64": data,
        "size": bytes.len(), "size_bytes": bytes.len(),
        "mime_type": mime, "paid_sats": paid_sats, "owned": true,
    })
}

// Resolve known purchases BEFORE any mint/spend. Missing bytes or an unreadable
// index require recovery; neither is authorization to charge the buyer again.
async fn existing_paid_content(
    data_dir: &std::path::Path,
    onion: &str,
    content_id: &str,
    filename: Option<&str>,
) -> Result<Option<serde_json::Value>> {
    let owned = crate::content_owned::list_owned_checked(data_dir)
        .await
        .context("Could not verify previous purchases; no new payment was sent")?;
    let Some(item) = owned.iter().find(|o| {
        o.onion == onion
            && (o.content_id == content_id
                || filename.is_some_and(|f| {
                    !f.is_empty() && o.filename.trim_start_matches('/') == f.trim_start_matches('/')
                }))
    }) else {
        return Ok(None);
    };
    let (mime, bytes) = crate::content_owned::read_owned(data_dir, &item.onion, &item.content_id)
        .await.context("This purchase is recorded, but its cached file is unavailable. No new payment was sent. Restore the cached file or contact the seller.")?;
    let mut response = paid_content_response(&bytes, &mime, 0);
    response["already_owned"] = serde_json::json!(true);
    response["filename"] = serde_json::json!(item.filename);
    Ok(Some(response))
}

// Updated clients open the persisted file through the Range-capable HTTP
// endpoint. Avoid putting two base64 copies of a large video in a JSON reply.
// Keep older clients compatible until both sides have upgraded.
fn invoice_download_response(bytes: &[u8], mime: &str, cache_only: bool) -> serde_json::Value {
    if cache_only {
        serde_json::json!({ "owned": true, "mime_type": mime, "size_bytes": bytes.len() })
    } else {
        paid_content_response(bytes, mime, 0)
    }
}

/// File purchases through an atomic no-clobber write in Files' own namespace.
async fn file_purchase_in_files(
    data_dir: &std::path::Path,
    filename: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<String> {
    let folder = if mime.starts_with("image/") || mime.starts_with("video/") {
        "Photos"
    } else if mime.starts_with("audio/") {
        "Music"
    } else {
        "Documents"
    };
    let root = data_dir.join("filebrowser");
    anyhow::ensure!(
        tokio::fs::metadata(&root).await?.is_dir(),
        "Files storage is unavailable"
    );
    let name = std::path::Path::new(filename)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("download");
    let path =
        crate::container::filebrowser::save_new_file(&root.join(folder), name, bytes).await?;
    Ok(format!(
        "{folder}/{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .context("Invalid Files name")?
    ))
}

impl RpcHandler {
    /// List content I'm sharing.
    pub(super) async fn handle_content_list_mine(&self) -> Result<serde_json::Value> {
        let catalog = content_server::load_catalog(&self.config.data_dir).await?;
        Ok(serde_json::json!({ "items": catalog.items }))
    }

    /// Add content to my catalog.
    pub(super) async fn handle_content_add(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let filename = params
            .get("filename")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing filename"))?;
        // Validate filename: prevent path traversal and null bytes
        // Allow forward slashes for subdirectories (e.g., "Music/song.mp3")
        if filename.contains("..") || filename.contains('\0') || filename.contains('\\') {
            anyhow::bail!("Invalid filename: path traversal not allowed");
        }
        // Reject paths starting with / (absolute) or . (hidden)
        if filename.starts_with('/') || filename.starts_with('.') {
            anyhow::bail!("Invalid filename: absolute paths and hidden files not allowed");
        }
        // Reject any path segment starting with . (hidden dirs)
        if filename
            .split('/')
            .any(|seg| seg.starts_with('.') || seg.is_empty())
        {
            anyhow::bail!("Invalid filename: hidden files/dirs or empty segments not allowed");
        }
        if filename.is_empty() || filename.len() > 512 {
            anyhow::bail!("Invalid filename: must be 1-512 characters");
        }
        let mime_type = params
            .get("mime_type")
            .and_then(|v| v.as_str())
            .unwrap_or("application/octet-stream");
        let description = params
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let mut item = ContentItem {
            id: uuid::Uuid::new_v4().to_string(),
            filename: filename.to_string(),
            mime_type: mime_type.to_string(),
            size_bytes: 0,
            description: description.to_string(),
            access: AccessControl::Free,
            availability: Availability::default(),
            added_at: chrono::Utc::now().to_rfc3339(),
        };

        // Resolve actual file size from disk
        let file_path = content_server::content_file_path(&self.config.data_dir, &item);
        if let Ok(metadata) = tokio::fs::metadata(&file_path).await {
            item.size_bytes = metadata.len();
        }

        content_server::add_item(&self.config.data_dir, item.clone()).await?;

        // Also store as DWN message for interoperable file catalog
        if let Ok(store) = DwnStore::new(&self.config.data_dir).await {
            let did = crate::identity::did_key_from_pubkey_hex(
                &self.state_manager.get_snapshot().await.0.server_info.pubkey,
            )
            .unwrap_or_default();
            let dwn_data = serde_json::json!({
                "id": item.id,
                "title": item.filename,
                "description": item.description,
                "content_type": item.mime_type,
                "size_bytes": item.size_bytes,
                "access": format!("{:?}", item.access).to_lowercase(),
                "created_at": item.added_at,
            });
            if let Err(e) = store
                .write_message(
                    &did,
                    Some(FILE_CATALOG_PROTOCOL),
                    Some("https://archipelago.dev/schemas/file-entry/v1"),
                    Some("application/json"),
                    Some(dwn_data),
                )
                .await
            {
                debug!("DWN file catalog write (non-fatal): {}", e);
            }
        }

        Ok(serde_json::json!({ "item": item }))
    }

    /// Remove content from my catalog.
    pub(super) async fn handle_content_remove(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?;

        content_server::remove_item(&self.config.data_dir, id).await?;
        Ok(serde_json::json!({ "removed": true }))
    }

    /// Set pricing for a content item.
    pub(super) async fn handle_content_set_pricing(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?;
        let access_type = params
            .get("access")
            .and_then(|v| v.as_str())
            .unwrap_or("free");

        let access = match access_type {
            "free" => AccessControl::Free,
            "peers_only" => AccessControl::PeersOnly,
            "paid" => {
                let price = params
                    .get("price_sats")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                if price == 0 {
                    return Err(anyhow::anyhow!("Paid content requires price_sats > 0"));
                }
                // Optional list of payment methods the sharer accepts.
                // Absent/empty = all methods (backward compatible).
                const KNOWN_METHODS: [&str; 4] = ["lightning", "onchain", "ecash", "fedimint"];
                let accepted: Vec<String> = params
                    .get("accepted_methods")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|m| m.as_str())
                            .filter(|m| KNOWN_METHODS.contains(m))
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                AccessControl::Paid {
                    price_sats: price,
                    accepted,
                }
            }
            _ => return Err(anyhow::anyhow!("Invalid access type: {}", access_type)),
        };

        content_server::set_access(&self.config.data_dir, id, access).await?;
        Ok(serde_json::json!({ "updated": true }))
    }

    /// Set availability for a content item.
    pub(super) async fn handle_content_set_availability(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?;
        let availability_type = params
            .get("availability")
            .and_then(|v| v.as_str())
            .unwrap_or("all_peers");

        let availability = match availability_type {
            "nobody" => Availability::Nobody,
            "all_peers" => Availability::AllPeers,
            "specific" => {
                let peers = params
                    .get("peers")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Availability::Specific { peers }
            }
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid availability: {}",
                    availability_type
                ))
            }
        };

        content_server::set_availability(&self.config.data_dir, id, availability).await?;
        Ok(serde_json::json!({ "updated": true }))
    }

    /// Download content from a peer. Prefers FIPS when the peer is known
    /// in our federation and has advertised a FIPS npub; falls back to
    /// Tor on any network failure.
    pub(super) async fn handle_content_download_peer(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing onion address"))?;
        let content_id = params
            .get("content_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing content_id"))?;

        // Validate v3 onion address: 56 base32 chars + ".onion" = 62 chars total
        if !is_valid_v3_onion(onion) {
            return Err(anyhow::anyhow!("Invalid v3 onion address"));
        }

        let (data, _) = self.state_manager.get_snapshot().await;
        let local_did = crate::identity::did_key_from_pubkey_hex(&data.server_info.pubkey)?;
        let fips_npub = crate::federation::fips_npub_for_onion(&self.config.data_dir, onion).await;

        let path = format!("/content/{}", content_id);
        let (response, transport) =
            crate::fips::dial::PeerRequest::new(fips_npub.as_deref(), onion, &path)
                .service(crate::settings::transport::PeerService::PeerFiles)
                .require_fips()
                .header("X-Federation-DID", local_did)
                .timeout(std::time::Duration::from_secs(120))
                .fips_timeout(std::time::Duration::from_secs(8))
                .send_get()
                .await
                .context("Failed to connect to peer")?;
        // Record which transport actually reached the peer (B14) so the UI
        // reflects FIPS vs Tor truthfully instead of always showing Tor/none.
        if let Err(e) = crate::federation::record_peer_transport(
            &self.config.data_dir,
            None,
            Some(onion),
            &transport.to_string(),
        )
        .await
        {
            tracing::warn!("Failed to persist peer transport badge: {e:#}");
        }

        if response.status() == reqwest::StatusCode::PAYMENT_REQUIRED {
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            return Ok(serde_json::json!({
                "error": "payment_required",
                "price_sats": body.get("price_sats").and_then(|v| v.as_u64()).unwrap_or(0),
            }));
        }

        // A 403 carries an actionable reason in its JSON body (e.g. "shared with
        // the host's federation peers only — federate first"). Surface that to
        // the user instead of a bare "Peer returned: 403 Forbidden".
        if response.status() == reqwest::StatusCode::FORBIDDEN {
            let status = response.status();
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            let msg = body
                .get("error")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Peer returned: {status}"));
            return Err(anyhow::anyhow!(msg));
        }

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Peer returned: {}", response.status()));
        }

        let bytes = response
            .bytes()
            .await
            .context("Failed to read response body")?;

        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);

        Ok(serde_json::json!({
            "data": encoded,
            "size": bytes.len(),
        }))
    }

    /// Browse a peer's content catalog. FIPS if the peer is federated,
    /// otherwise Tor.
    pub(super) async fn handle_content_browse_peer(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing onion address"))?;

        // Validate v3 onion address: 56 base32 chars + ".onion" = 62 chars total
        if !is_valid_v3_onion(onion) {
            return Err(anyhow::anyhow!("Invalid v3 onion address"));
        }

        let fips_npub = crate::federation::fips_npub_for_onion(&self.config.data_dir, onion).await;

        debug!(
            "Browsing peer content at {} (fips={})",
            onion,
            fips_npub.is_some()
        );

        let (response, transport) =
            crate::fips::dial::PeerRequest::new(fips_npub.as_deref(), onion, "/content")
                .service(crate::settings::transport::PeerService::PeerFiles)
                .timeout(std::time::Duration::from_secs(30))
                // The Cloud page's hottest call: without a fast-fail cap a
                // cold FIPS path burned ~16.6s before Tor even started,
                // against the UI's 30s deadline — users saw errors, not
                // fallback.
                .fips_timeout(std::time::Duration::from_secs(6))
                .send_get()
                .await
                .context("Failed to connect to peer")?;
        // Record which transport actually reached the peer (B14).
        if let Err(e) = crate::federation::record_peer_transport(
            &self.config.data_dir,
            None,
            Some(onion),
            &transport.to_string(),
        )
        .await
        {
            tracing::warn!("Failed to persist peer transport badge: {e:#}");
        }

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Peer returned error: {}",
                response.status()
            ));
        }

        let mut body: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse peer catalog")?;

        // Surface the transport that actually reached the peer so the cloud
        // browse UI can show a FIPS/Tor pill instead of always assuming Tor (B21).
        if let Some(obj) = body.as_object_mut() {
            obj.insert(
                "transport".to_string(),
                serde_json::Value::String(transport.to_string()),
            );
        }

        Ok(body)
    }

    /// Download paid content from a peer: mint ecash token, send with request.
    pub(super) async fn handle_content_download_peer_paid(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing onion address"))?;
        let content_id = params
            .get("content_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing content_id"))?;
        let price_sats = params
            .get("price_sats")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing price_sats"))?;

        if price_sats == 0 {
            return Err(anyhow::anyhow!("price_sats must be > 0"));
        }
        if !is_valid_v3_onion(onion) {
            return Err(anyhow::anyhow!("Invalid v3 onion address"));
        }

        // NEVER pay twice for content we already own (2026-07-22: a file
        // shared twice produced two catalog ids for the same bytes and the
        // buyer paid both). Guard BEFORE any ecash is minted, matching both
        // by exact (onion, content_id) and by (onion, filename) — the latter
        // catches duplicate ids pointing at the same file on the same
        // seller. The owned copy is served from the local cache instead.
        if let Some(cached) = existing_paid_content(
            &self.config.data_dir,
            onion,
            content_id,
            params.get("filename").and_then(|v| v.as_str()),
        )
        .await?
        {
            return Ok(cached);
        }

        let fips_npub = crate::federation::fips_npub_for_onion(&self.config.data_dir, onion).await;
        if fips_npub.is_none() {
            return Ok(
                serde_json::json!({ "error": "Connect with this node over FIPS before buying its files. No payment was made." }),
            );
        }

        // `method` pins the backend the user confirmed in the UI ("cashu" |
        // "fedimint"); absent = auto (Cashu first, then Fedimint). The seller's
        // verify_payment_token accepts either, so a node whose balance lives in
        // one system can still pay (#3).
        let method = params.get("method").and_then(|v| v.as_str());

        let mint_cashu = || ecash::send_token(&self.config.data_dir, price_sats);
        let mint_fedimint =
            || crate::wallet::fedimint_client::spend_from_any(&self.config.data_dir, price_sats);

        let (token_str, used_backend) = match method {
            Some("cashu") => match mint_cashu().await {
                Ok(t) => (t, "cashu"),
                Err(e) => {
                    tracing::warn!("paid download: cashu mint failed for {price_sats} sats: {e:#}");
                    return Ok(serde_json::json!({ "error": format!(
                        "Couldn't pay {price_sats} sats from your Cashu wallet: {e}. \
                         Fund it, or choose Fedimint."
                    ) }));
                }
            },
            Some("fedimint") => match mint_fedimint().await {
                Ok((notes, fed)) => {
                    tracing::info!(
                        "paid download: spending {price_sats} sats Fedimint notes from {fed}"
                    );
                    (notes, "fedimint")
                }
                Err(e) => {
                    tracing::warn!(
                        "paid download: fedimint spend failed for {price_sats} sats: {e:#}"
                    );
                    return Ok(serde_json::json!({ "error": format!(
                        "Couldn't pay {price_sats} sats from your Fedimint wallet: {e}. \
                         Fund it, or choose Cashu."
                    ) }));
                }
            },
            _ => match mint_cashu().await {
                Ok(t) => (t, "cashu"),
                Err(cashu_err) => match mint_fedimint().await {
                    Ok((notes, _fed)) => (notes, "fedimint"),
                    Err(fedi_err) => {
                        tracing::warn!(
                            "paid download: no ecash backend could pay {price_sats} sats \
                             (cashu: {cashu_err:#}; fedimint: {fedi_err:#})"
                        );
                        return Ok(serde_json::json!({ "error": format!(
                            "Couldn't pay {price_sats} sats from your ecash wallet \
                             (Cashu or Fedimint). Fund either wallet and try again."
                        ) }));
                    }
                },
            },
        };
        tracing::info!(
            "paid download: paying {price_sats} sats to {onion} via {used_backend} ecash"
        );

        let (data, _) = self.state_manager.get_snapshot().await;
        let local_did = crate::identity::did_key_from_pubkey_hex(&data.server_info.pubkey)?;

        let path = format!("/content/{}", content_id);
        // Surface a real reason instead of the generic sanitized error (#30):
        // A bearer token must not be replayed after an ambiguous delivery.
        // A transport error can mean the seller received it without replying.
        let (response, transport) =
            match crate::fips::dial::PeerRequest::new(fips_npub.as_deref(), onion, &path)
                .service(crate::settings::transport::PeerService::PeerFiles)
                .require_fips()
                .header("X-Federation-DID", local_did)
                .header("X-Payment-Token", token_str.clone())
                .single_delivery()
                .timeout(std::time::Duration::from_secs(900))
                .send_get()
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("paid peer download dial failed for {}: {:#}", onion, e);
                    // The token was already minted/spent — reclaim it so the buyer
                    // doesn't lose the value when the seller was simply unreachable.
                    let refund =
                        reclaim_spent_ecash(&self.config.data_dir, &token_str, used_backend).await;
                    return Ok(serde_json::json!({
                        "error": format!("The purchase could not be completed. {refund}")
                    }));
                }
            };
        // Record which transport actually reached the peer (B14).
        if let Err(e) = crate::federation::record_peer_transport(
            &self.config.data_dir,
            None,
            Some(onion),
            &transport.to_string(),
        )
        .await
        {
            tracing::warn!("Failed to persist peer transport badge: {e:#}");
        }

        if response.status() == reqwest::StatusCode::PAYMENT_REQUIRED {
            // A 402 can mean mint validation, network failure, underpayment,
            // or an unaccepted mint. Do not invent a mint-mismatch diagnosis.
            let body = response.text().await.unwrap_or_default();
            tracing::warn!(
                "paid download: seller {onion} rejected {used_backend} payment of {price_sats} sats: {body}"
            );
            // Seller couldn't redeem the token — reclaim it so the buyer keeps
            // their funds (the spent-but-unredeemed-notes case the user hit).
            let refund = reclaim_spent_ecash(&self.config.data_dir, &token_str, used_backend).await;
            return Ok(serde_json::json!({
                "error": format!("The seller could not verify the payment. {refund}")
            }));
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            tracing::warn!("paid download: seller {onion} returned {status}: {body}");
            let refund = reclaim_spent_ecash(&self.config.data_dir, &token_str, used_backend).await;
            return Ok(serde_json::json!({
                "error": format!("{} {refund}", seller_error_message(status, &body))
            }));
        }

        // Capture the content type BEFORE consuming the body so the local cache
        // can render the right viewer (image vs video) later.
        let mime_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!("paid download: response body failed: {error}");
                let refund =
                    reclaim_spent_ecash(&self.config.data_dir, &token_str, used_backend).await;
                return Ok(serde_json::json!({
                    "error": format!("The file transfer was interrupted after payment was sent. {refund}")
                }));
            }
        };

        // Persist the purchase so it "stays unlocked" for this buyer: cache the
        // bytes + metadata keyed by (onion, content_id). The gallery then renders
        // it unblurred and views it in-app from this cache — no re-payment and no
        // reliance on a browser download (which silently fails on the mobile
        // companion, the original "paid but never unlocked" report). Best-effort:
        // a cache-write failure must not fail an already-paid download.
        let filename = params
            .get("filename")
            .and_then(|v| v.as_str())
            .unwrap_or(content_id)
            .to_string();
        let purchased_at = chrono::Utc::now().to_rfc3339();
        if let Err(e) = crate::content_owned::record_purchase(
            &self.config.data_dir,
            onion,
            content_id,
            &filename,
            &mime_type,
            &bytes,
            price_sats,
            used_backend,
            &purchased_at,
        )
        .await
        {
            tracing::warn!("paid download: failed to cache purchased content (non-fatal): {e:#}");
        }

        // The durable purchased-content cache above is primary. A Files copy
        // remains optional: a stopped FileBrowser must not undo a paid download.
        let filed =
            file_purchase_in_files(&self.config.data_dir, &filename, &mime_type, &bytes).await;
        match filed {
            Ok(path) => tracing::info!("paid download: filed into Files/{path}"),
            Err(error) => tracing::warn!(
                "paid download: optional Files copy failed; purchase cache retained: {error}"
            ),
        }

        tracing::info!("paid download: received {} bytes from {onion} (paid {price_sats} sats via {used_backend})", bytes.len());
        let mut result = paid_content_response(&bytes, &mime_type, price_sats);
        result["ecash_backend"] = serde_json::json!(used_backend);
        Ok(result)
    }

    /// Buyer side (#46): ask the selling node to mint a Lightning invoice for a
    /// paid item so the buyer can pay from any external wallet. Returns the
    /// bolt11 invoice + payment hash to render as a QR and poll for settlement.
    pub(super) async fn handle_content_request_invoice(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing onion address"))?;
        let content_id = params
            .get("content_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing content_id"))?;
        if !is_valid_v3_onion(onion) {
            return Err(anyhow::anyhow!("Invalid v3 onion address"));
        }

        let (data, _) = self.state_manager.get_snapshot().await;
        let local_did = crate::identity::did_key_from_pubkey_hex(&data.server_info.pubkey)?;
        let fips_npub = crate::federation::fips_npub_for_onion(&self.config.data_dir, onion).await;

        // Minting a bolt11 is a tiny request/response — keep it snappy. Cap the
        // FIPS attempt hard so a cold overlay can't burn the whole budget, and
        // give Tor a short-but-real window (onion circuits need a few seconds).
        let path = format!("/content/{}/invoice", content_id);
        let (response, _transport) =
            match crate::fips::dial::PeerRequest::new(fips_npub.as_deref(), onion, &path)
                .service(crate::settings::transport::PeerService::PeerFiles)
                .header("X-Federation-DID", local_did)
                .timeout(std::time::Duration::from_secs(25))
                .fips_timeout(std::time::Duration::from_secs(6))
                .send_get()
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("request-invoice dial failed for {}: {:#}", onion, e);
                    return Ok(serde_json::json!({
                        "error": "Could not reach the peer over mesh or Tor — it may be offline."
                    }));
                }
            };

        if !response.status().is_success() {
            return Ok(serde_json::json!({
                "error": format!("Seller could not create an invoice ({}).", response.status())
            }));
        }
        let body: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse invoice response")?;
        Ok(body)
    }

    /// Buyer side (#46): poll the selling node for invoice settlement.
    pub(super) async fn handle_content_invoice_status(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing onion address"))?;
        let content_id = params
            .get("content_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing content_id"))?;
        let payment_hash = params
            .get("payment_hash")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing payment_hash"))?;
        if !is_valid_v3_onion(onion) {
            return Err(anyhow::anyhow!("Invalid v3 onion address"));
        }
        // Payment hash is hex from the seller; keep it strictly hex so it's safe
        // to interpolate into the request path.
        if payment_hash.is_empty()
            || payment_hash.len() > 128
            || !payment_hash.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err(anyhow::anyhow!("Invalid payment_hash"));
        }

        let fips_npub = crate::federation::fips_npub_for_onion(&self.config.data_dir, onion).await;
        // Settlement poll — runs repeatedly, so each call must be quick. Fast-fail
        // FIPS and keep a short Tor window; an unreachable peer just reads as
        // "not yet paid" and the UI polls again.
        let path = format!("/content/{}/invoice-status/{}", content_id, payment_hash);
        let (response, _transport) =
            match crate::fips::dial::PeerRequest::new(fips_npub.as_deref(), onion, &path)
                .service(crate::settings::transport::PeerService::PeerFiles)
                .timeout(std::time::Duration::from_secs(15))
                .fips_timeout(std::time::Duration::from_secs(6))
                .send_get()
                .await
            {
                Ok(v) => v,
                Err(_) => {
                    // Treat an unreachable peer as "not yet paid" so the UI keeps polling.
                    return Ok(serde_json::json!({ "paid": false, "unreachable": true }));
                }
            };
        if !response.status().is_success() {
            return Ok(serde_json::json!({ "paid": false }));
        }
        let body: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse invoice-status response")?;
        Ok(body)
    }

    /// Buyer side (#46): download a paid item after the invoice settled, passing
    /// the payment hash so the seller's content gate releases the file.
    pub(super) async fn handle_content_download_peer_invoice(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing onion address"))?;
        let content_id = params
            .get("content_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing content_id"))?;
        let payment_hash = params
            .get("payment_hash")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing payment_hash"))?;
        if !is_valid_v3_onion(onion) {
            return Err(anyhow::anyhow!("Invalid v3 onion address"));
        }
        if payment_hash.len() != 64 || !payment_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(anyhow::anyhow!("Invalid payment_hash"));
        }

        let cache_only = params
            .get("cache_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if let Some((mime, bytes)) =
            crate::content_owned::read_owned(&self.config.data_dir, onion, content_id).await
        {
            return Ok(invoice_download_response(&bytes, &mime, cache_only));
        }
        // Older sellers only mark settlement during status polling. Always
        // perform that handshake before requesting bytes; retries never pay.
        // The download gate remains authoritative: a file may have become
        // free, and newer sellers verify directly if status polling fails.
        let _ = self
            .handle_content_invoice_status(Some(serde_json::json!({
                "onion": onion, "content_id": content_id, "payment_hash": payment_hash,
            })))
            .await;

        let (data, _) = self.state_manager.get_snapshot().await;
        let local_did = crate::identity::did_key_from_pubkey_hex(&data.server_info.pubkey)?;
        let fips_npub = crate::federation::fips_npub_for_onion(&self.config.data_dir, onion).await;

        let path = format!("/content/{}", content_id);
        let (response, transport) = match crate::fips::dial::PeerRequest::new(
            fips_npub.as_deref(),
            onion,
            &path,
        )
        .service(crate::settings::transport::PeerService::PeerFiles)
        .require_fips()
        .header("X-Federation-DID", local_did)
        .header("X-Invoice-Hash", payment_hash.to_string())
        .timeout(std::time::Duration::from_secs(900))
        .send_get()
        .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("invoice download dial failed for {}: {:#}", onion, e);
                return Ok(serde_json::json!({
                    "error": "The peer’s FIPS connection is unavailable. Retry when it reconnects; do not pay again."
                }));
            }
        };
        if let Err(e) = crate::federation::record_peer_transport(
            &self.config.data_dir,
            None,
            Some(onion),
            &transport.to_string(),
        )
        .await
        {
            tracing::warn!("Failed to persist peer transport badge: {e:#}");
        }

        if response.status() == reqwest::StatusCode::PAYMENT_REQUIRED {
            return Ok(serde_json::json!({
                "error": "The seller has not confirmed access yet. Retry the download without paying again."
            }));
        }
        if !response.status().is_success() {
            return Ok(serde_json::json!({
                "error": format!("Peer returned an error ({}).", response.status())
            }));
        }

        let mime = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .split(';')
            .next()
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = response
            .bytes()
            .await
            .context("Paid file transfer interrupted; retry the download without paying again")?;
        let filename = params
            .get("filename")
            .and_then(|v| v.as_str())
            .unwrap_or(content_id);
        crate::content_owned::record_purchase(
            &self.config.data_dir,
            onion,
            content_id,
            filename,
            &mime,
            &bytes,
            params
                .get("price_sats")
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            "lightning",
            &chrono::Utc::now().to_rfc3339(),
        )
        .await
        .context("Paid file could not be saved; retry the download without paying again")?;
        if let Err(error) =
            file_purchase_in_files(&self.config.data_dir, filename, &mime, &bytes).await
        {
            tracing::warn!("Lightning purchase cached; optional Files copy failed: {error:#}");
        }
        Ok(invoice_download_response(&bytes, &mime, cache_only))
    }

    /// Buyer side (#46): ask the seller for a fresh on-chain address to pay.
    pub(super) async fn handle_content_request_onchain(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing onion address"))?;
        let content_id = params
            .get("content_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing content_id"))?;
        if !is_valid_v3_onion(onion) {
            return Err(anyhow::anyhow!("Invalid v3 onion address"));
        }

        let (data, _) = self.state_manager.get_snapshot().await;
        let local_did = crate::identity::did_key_from_pubkey_hex(&data.server_info.pubkey)?;
        let fips_npub = crate::federation::fips_npub_for_onion(&self.config.data_dir, onion).await;

        // Issuing an address is a tiny request/response — fast-fail FIPS, short
        // Tor window (same budget shape as the invoice path, #6).
        let path = format!("/content/{}/onchain", content_id);
        let (response, _transport) =
            match crate::fips::dial::PeerRequest::new(fips_npub.as_deref(), onion, &path)
                .service(crate::settings::transport::PeerService::PeerFiles)
                .header("X-Federation-DID", local_did)
                .timeout(std::time::Duration::from_secs(25))
                .fips_timeout(std::time::Duration::from_secs(6))
                .send_get()
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("request-onchain dial failed for {}: {:#}", onion, e);
                    return Ok(serde_json::json!({
                        "error": "Could not reach the peer over mesh or Tor — it may be offline."
                    }));
                }
            };
        if !response.status().is_success() {
            return Ok(serde_json::json!({
                "error": format!("Seller could not provide an address ({}).", response.status())
            }));
        }
        let body: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse onchain response")?;
        Ok(body)
    }

    /// Buyer side (#46): poll the selling node for on-chain payment detection.
    pub(super) async fn handle_content_onchain_status(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing onion address"))?;
        let content_id = params
            .get("content_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing content_id"))?;
        let address = params
            .get("address")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing address"))?;
        if !is_valid_v3_onion(onion) {
            return Err(anyhow::anyhow!("Invalid v3 onion address"));
        }
        // Bitcoin addresses are alphanumeric; keep strictly so for safe path use.
        if address.is_empty()
            || address.len() > 100
            || !address.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return Err(anyhow::anyhow!("Invalid address"));
        }

        let fips_npub = crate::federation::fips_npub_for_onion(&self.config.data_dir, onion).await;
        let path = format!("/content/{}/onchain-status/{}", content_id, address);
        let (response, _transport) =
            match crate::fips::dial::PeerRequest::new(fips_npub.as_deref(), onion, &path)
                .service(crate::settings::transport::PeerService::PeerFiles)
                .timeout(std::time::Duration::from_secs(15))
                .fips_timeout(std::time::Duration::from_secs(6))
                .send_get()
                .await
            {
                Ok(v) => v,
                Err(_) => return Ok(serde_json::json!({ "paid": false, "unreachable": true })),
            };
        if !response.status().is_success() {
            return Ok(serde_json::json!({ "paid": false }));
        }
        let body: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse onchain-status response")?;
        Ok(body)
    }

    /// Buyer side (#46): download a paid item after the on-chain payment was
    /// detected, passing the address so the seller's content gate releases it.
    pub(super) async fn handle_content_download_peer_onchain(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing onion address"))?;
        let content_id = params
            .get("content_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing content_id"))?;
        let address = params
            .get("address")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing address"))?;
        if !is_valid_v3_onion(onion) {
            return Err(anyhow::anyhow!("Invalid v3 onion address"));
        }
        if address.is_empty() || !address.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(anyhow::anyhow!("Invalid address"));
        }

        let (data, _) = self.state_manager.get_snapshot().await;
        let local_did = crate::identity::did_key_from_pubkey_hex(&data.server_info.pubkey)?;
        let fips_npub = crate::federation::fips_npub_for_onion(&self.config.data_dir, onion).await;

        let path = format!("/content/{}", content_id);
        let (response, transport) = match crate::fips::dial::PeerRequest::new(
            fips_npub.as_deref(),
            onion,
            &path,
        )
        .service(crate::settings::transport::PeerService::PeerFiles)
        .require_fips()
        .header("X-Federation-DID", local_did)
        .header("X-Onchain-Address", address.to_string())
        .timeout(std::time::Duration::from_secs(900))
        .send_get()
        .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("onchain download dial failed for {}: {:#}", onion, e);
                return Ok(serde_json::json!({
                    "error": "The peer’s FIPS connection is unavailable. Retry when it reconnects; do not pay again."
                }));
            }
        };
        if let Err(e) = crate::federation::record_peer_transport(
            &self.config.data_dir,
            None,
            Some(onion),
            &transport.to_string(),
        )
        .await
        {
            tracing::warn!("Failed to persist peer transport badge: {e:#}");
        }

        if response.status() == reqwest::StatusCode::PAYMENT_REQUIRED {
            return Ok(serde_json::json!({
                "error": "Seller has not registered this payment yet — wait for confirmation and retry."
            }));
        }
        if !response.status().is_success() {
            return Ok(serde_json::json!({
                "error": format!("Peer returned an error ({}).", response.status())
            }));
        }

        let bytes = response
            .bytes()
            .await
            .context("Failed to read response body")?;
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(serde_json::json!({
            "data": encoded,
            "size": bytes.len(),
        }))
    }

    /// Fetch a preview of paid content from a peer (no payment required).
    pub(super) async fn handle_content_preview_peer(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing onion address"))?;
        let content_id = params
            .get("content_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing content_id"))?;

        if !is_valid_v3_onion(onion) {
            return Err(anyhow::anyhow!("Invalid v3 onion address"));
        }

        let fips_npub = crate::federation::fips_npub_for_onion(&self.config.data_dir, onion).await;

        let path = format!("/content/{}/preview", content_id);
        debug!(
            "Fetching content preview from {}{} (fips={})",
            onion,
            path,
            fips_npub.is_some()
        );

        let (response, transport) =
            crate::fips::dial::PeerRequest::new(fips_npub.as_deref(), onion, &path)
                .service(crate::settings::transport::PeerService::PeerFiles)
                .require_fips()
                .timeout(std::time::Duration::from_secs(30))
                .fips_timeout(std::time::Duration::from_secs(6))
                .send_get()
                .await
                .context("Failed to connect to peer for preview")?;
        // Record which transport actually reached the peer (B14).
        if let Err(e) = crate::federation::record_peer_transport(
            &self.config.data_dir,
            None,
            Some(onion),
            &transport.to_string(),
        )
        .await
        {
            tracing::warn!("Failed to persist peer transport badge: {e:#}");
        }

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Peer returned error for preview: {}",
                response.status()
            ));
        }

        let is_preview = response
            .headers()
            .get("X-Content-Preview")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();

        let bytes = response
            .bytes()
            .await
            .context("Failed to read preview response")?;

        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);

        Ok(serde_json::json!({
            "data": encoded,
            "size": bytes.len(),
            "content_type": content_type,
            "preview_mode": is_preview,
        }))
    }

    /// `content.owned-list` — every paid item this node has purchased, so the
    /// gallery can render owned items unblurred/viewable without re-payment.
    pub(super) async fn handle_content_owned_list(&self) -> Result<serde_json::Value> {
        let items = crate::content_owned::list_owned(&self.config.data_dir).await;
        Ok(serde_json::json!({ "items": items }))
    }

    /// `content.indeehub-projects` — films from the IndeeHub app.
    ///
    /// Node-side because the interesting half needs a Nostr session, and
    /// signing that in the browser would put identity material next to the
    /// model. Returns titles only.
    pub(super) async fn handle_content_indeehub_projects(&self) -> Result<serde_json::Value> {
        let projects = crate::content_indeehub::list_projects(&self.config.data_dir).await;
        let items: Vec<serde_json::Value> = projects
            .iter()
            .filter_map(|p| {
                let title = p.title.as_deref()?.trim();
                if title.is_empty() {
                    return None;
                }
                Some(serde_json::json!({
                    "id": p.id.clone().unwrap_or_else(|| title.to_string()),
                    "title": title,
                    "synopsis": p.synopsis.clone().unwrap_or_default(),
                    "poster": p.poster.clone().unwrap_or_default(),
                    "year": p.year_num(),
                    // Films are video by definition. The UI adapter buckets
                    // purely on mime/extension, so an item carrying neither
                    // silently classified 'excluded' and this scope's
                    // surface could never render a card.
                    "mime_type": "video/mp4",
                }))
            })
            .collect();
        Ok(serde_json::json!({ "count": items.len(), "items": items }))
    }

    /// `content.browse-all-peers` — every federated peer's catalogue in one
    /// call.
    ///
    /// The dashboard fans this out client-side, but the assistant needs a
    /// SINGLE tool call to answer "what films do my peers have" — asking a
    /// model to enumerate peers and loop is how it ends up saying it has no
    /// tool for this at all.
    ///
    /// One peer failing (offline, Tor timeout) contributes nothing rather than
    /// failing the whole call: with a dozen peers, any of them being down is
    /// the normal case, not an error.
    pub(super) async fn handle_content_browse_all_peers(&self) -> Result<serde_json::Value> {
        let nodes = crate::federation::load_nodes(&self.config.data_dir)
            .await
            .unwrap_or_default();

        let onions: Vec<String> = nodes
            .iter()
            .filter_map(|n| {
                let o = n.onion.clone();
                if o.trim().is_empty() {
                    None
                } else {
                    Some(o)
                }
            })
            .collect();

        // CONCURRENT with a cap, mirroring Cloud.vue's peer-files fan-out
        // (BROWSE_PEER_CONCURRENCY = 3, 10s per peer, one attempt). That is
        // the implementation the operator already trusts, and it is why the
        // Cloud tab answers while a sequential version here did not: with 16
        // peers at 8s each, going one at a time reached only one or two inside
        // any sane budget.
        //
        // 02-08 is the reason for the CAP rather than an unbounded fan-out —
        // 13 of 14 simultaneous browse-peer calls never settled and starved
        // the connection pool. Three at a time keeps a dead peer from costing
        // anything but its own slot.
        // Cloud.vue uses 3, but that cap exists because CHROMIUM's connection
        // pool was being starved (02-08) — a browser constraint the daemon does
        // not share. Measured here: at 3, a 20s budget only got through 2
        // batches of 16 peers and reached none. At 8 every peer is attempted
        // inside the budget, which is the point.
        const BROWSE_PEER_CONCURRENCY: usize = 8;
        const PER_PEER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
        // Headroom matters: 16 peers at concurrency 8 is two batches, and a
        // batch only finishes when its SLOWEST peer does. At a 20s budget
        // one slow peer in batch 1 left batch 2 no time at all.
        let overall = std::time::Duration::from_secs(45);
        let deadline = tokio::time::Instant::now() + overall;

        let mut items = Vec::new();
        let mut reached = 0usize;
        let mut unreachable = 0usize;

        // Accumulate per batch rather than wrapping the whole loop in one
        // `timeout(..).unwrap_or_default()`. That construction DISCARDED
        // every completed batch the moment the budget expired, so a single
        // slow peer turned a partly-successful fan-out into "0 reached, 16
        // unreachable" — indistinguishable, downstream, from the peers
        // having no content at all. Observed live on archi-dev-box: back to
        // back calls returned real peer items and then nothing.
        let mut results: Vec<(String, Option<serde_json::Value>)> = Vec::new();
        for chunk in onions.chunks(BROWSE_PEER_CONCURRENCY) {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            let mut set = Vec::new();
            for onion in chunk {
                let params = Some(serde_json::json!({ "onion": onion }));
                set.push(async move {
                    let v = tokio::time::timeout(
                        PER_PEER_TIMEOUT,
                        self.handle_content_browse_peer(params),
                    )
                    .await
                    .ok()
                    .and_then(|r| r.ok());
                    (onion.clone(), v)
                });
            }
            // No batch-level timeout: every future in `set` is ALREADY
            // bounded by PER_PEER_TIMEOUT, so this join can't outrun it, and
            // adding an outer timeout here would reintroduce exactly the
            // discard-on-expiry bug above. The deadline check at the top of
            // the loop is what stops a long peer list from running forever.
            results.extend(futures_util::future::join_all(set).await);
        }

        for (onion, v) in &results {
            match v {
                Some(v) => {
                    reached += 1;
                    if let Some(arr) = v.get("items").and_then(|i| i.as_array()) {
                        for it in arr {
                            let mut it = it.clone();
                            if let Some(obj) = it.as_object_mut() {
                                obj.insert("peer".into(), serde_json::json!(onion));
                            }
                            items.push(it);
                        }
                    }
                }
                None => unreachable += 1,
            }
        }
        // Peers the overall budget never got to are unreachable for this call,
        // not silently absent.
        unreachable += onions.len().saturating_sub(results.len());

        Ok(serde_json::json!({
            "items": items,
            "peers_reached": reached,
            "peers_unreachable": unreachable,
            "peers_total": onions.len(),
            "partial": unreachable > 0,
        }))
    }

    /// `content.owned-get` — return a purchased item's bytes (base64) from the
    /// local cache for in-app viewing/saving. No network, no re-payment.
    pub(super) async fn handle_content_owned_get(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing onion address"))?;
        let content_id = params
            .get("content_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing content_id"))?;

        match crate::content_owned::read_owned(&self.config.data_dir, onion, content_id).await {
            Some((mime_type, bytes)) => {
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
                Ok(serde_json::json!({
                    "data": encoded,
                    "size": bytes.len(),
                    "mime_type": mime_type,
                }))
            }
            None => Ok(serde_json::json!({
                "error": "You don't own this item yet, or its cached copy is missing."
            })),
        }
    }
}

#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;

#[cfg(test)]
mod invoice_delivery_response_tests {
    use super::*;
    #[test]
    fn cached_delivery_avoids_base64_but_keeps_old_clients_compatible() {
        let cached = invoice_download_response(b"paid bytes", "video/mp4", true);
        assert_eq!(cached["owned"], true);
        assert_eq!(cached["size_bytes"], 10);
        assert!(cached.get("data").is_none());
        assert!(cached.get("data_base64").is_none());
        let legacy = invoice_download_response(b"paid bytes", "video/mp4", false);
        assert_eq!(legacy["data"], "cGFpZCBieXRlcw==");
        assert_eq!(legacy["data"], legacy["data_base64"]);
    }
}
