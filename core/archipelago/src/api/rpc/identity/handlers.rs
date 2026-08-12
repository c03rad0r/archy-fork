use super::*;
use crate::api::rpc::RpcHandler;
use crate::identity_manager::{IdentityManager, IdentityProfile, IdentityPurpose};
use crate::network::did_dht;
use anyhow::{Context, Result};
use nostr_sdk::ToBech32;

impl RpcHandler {
    /// List all identities with their default status.
    pub(in crate::api::rpc) async fn handle_identity_list(
        &self,
        _params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let (identities, default_id) = manager.list().await?;

        // #49: The canonical node Nostr key is the node-level HKDF key
        // (`derive_node_nostr_key`) that Settings and Nostr discovery both use
        // via `node.nostr-pubkey`. The mirrored "Node" identity stores
        // nostr=None, and seed identities use a different BIP-32 NIP-06 key, so
        // the "Node" entry in Web5 > Identities disagreed with Settings. Resolve
        // the node-level key once and override it onto whichever identity record
        // is the node's own (its ed25519 matches `server_info.pubkey`), so both
        // surfaces always show the same npub. Display-only — no key is rewritten.
        let identity_dir = self.config.data_dir.join("identity");
        let node_nostr_hex = crate::nostr_discovery::get_nostr_pubkey(&identity_dir)
            .await
            .ok();
        let node_nostr_npub = node_nostr_hex.as_ref().and_then(|h| {
            nostr_sdk::PublicKey::from_hex(h)
                .ok()
                .and_then(|pk| pk.to_bech32().ok())
        });
        let (snapshot, _) = self.state_manager.get_snapshot().await;
        let node_pubkey_hex = snapshot.server_info.pubkey.clone();

        let items: Vec<serde_json::Value> = identities
            .into_iter()
            .map(|id| {
                let is_default = default_id.as_deref() == Some(&id.id);
                let is_node = !node_pubkey_hex.is_empty() && id.pubkey_hex == node_pubkey_hex;
                let (nostr_pubkey, nostr_npub) = if is_node {
                    (
                        node_nostr_hex.clone().or(id.nostr_pubkey),
                        node_nostr_npub.clone().or(id.nostr_npub),
                    )
                } else {
                    (id.nostr_pubkey, id.nostr_npub)
                };
                serde_json::json!({
                    "id": id.id,
                    "name": id.name,
                    "purpose": id.purpose,
                    "pubkey": id.pubkey_hex,
                    "did": id.did,
                    "created_at": id.created_at,
                    "is_default": is_default,
                    "nostr_pubkey": nostr_pubkey,
                    "nostr_npub": nostr_npub,
                    "profile": id.profile,
                })
            })
            .collect();

        Ok(serde_json::json!({ "identities": items }))
    }

    /// Create a new identity.
    pub(in crate::api::rpc) async fn handle_identity_create(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Personal");
        if name.len() > 100 {
            anyhow::bail!("Identity name must be 100 characters or fewer");
        }
        let name = name.to_string();

        let purpose_str = params
            .get("purpose")
            .and_then(|v| v.as_str())
            .unwrap_or("personal");

        let purpose = match purpose_str {
            "business" => IdentityPurpose::Business,
            "anonymous" => IdentityPurpose::Anonymous,
            _ => IdentityPurpose::Personal,
        };

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let record = manager.create(name, purpose).await?;

        Ok(serde_json::json!({
            "id": record.id,
            "name": record.name,
            "purpose": record.purpose,
            "pubkey": record.pubkey_hex,
            "did": record.did,
            "created_at": record.created_at,
            "nostr_pubkey": record.nostr_pubkey,
            "nostr_npub": record.nostr_npub,
        }))
    }

    /// Get a single identity by ID.
    pub(in crate::api::rpc) async fn handle_identity_get(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: id"))?;
        validate_identity_id(id)?;

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let record = manager.get(id).await?;
        let (_, default_id) = manager.list().await?;
        let is_default = default_id.as_deref() == Some(&record.id);

        Ok(serde_json::json!({
            "id": record.id,
            "name": record.name,
            "purpose": record.purpose,
            "pubkey": record.pubkey_hex,
            "did": record.did,
            "created_at": record.created_at,
            "is_default": is_default,
            "nostr_pubkey": record.nostr_pubkey,
            "nostr_npub": record.nostr_npub,
        }))
    }

    /// Delete an identity.
    pub(in crate::api::rpc) async fn handle_identity_delete(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: id"))?;
        validate_identity_id(id)?;

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        manager.delete(id).await?;

        Ok(serde_json::json!({ "ok": true }))
    }

    /// Set the default identity.
    pub(in crate::api::rpc) async fn handle_identity_set_default(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: id"))?;
        validate_identity_id(id)?;

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        manager.set_default(id).await?;

        Ok(serde_json::json!({ "ok": true }))
    }

    /// Sign a message with a specific identity.
    pub(in crate::api::rpc) async fn handle_identity_sign(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: id"))?;
        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: message"))?;

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let signature = manager.sign(id, message.as_bytes()).await?;
        let record = manager.get(id).await?;

        Ok(serde_json::json!({
            "did": record.did,
            "message": message,
            "signature": signature,
        }))
    }

    /// Verify a signature against a DID.
    pub(in crate::api::rpc) async fn handle_identity_verify(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let did = params
            .get("did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: did"))?;
        let message = params
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: message"))?;
        let signature = params
            .get("signature")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: signature"))?;

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let valid = manager.verify(did, message.as_bytes(), signature).await?;

        Ok(serde_json::json!({ "valid": valid }))
    }

    /// Resolve a DID to its W3C DID Document.
    /// If no DID is provided, returns the node's own DID Document.
    pub(in crate::api::rpc) async fn handle_identity_resolve_did(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();

        // If a DID is provided, resolve it; otherwise use the node's DID
        let is_local = params.get("did").and_then(|v| v.as_str()).is_none();
        let pubkey_hex = if let Some(did) = params.get("did").and_then(|v| v.as_str()) {
            let pubkey_bytes = crate::identity::pubkey_bytes_from_did_key(did)?;
            hex::encode(pubkey_bytes)
        } else {
            let (data, _) = self.state_manager.get_snapshot().await;
            data.server_info.pubkey.clone()
        };

        // For local node, include Nostr secp256k1 key in DID Document (paired identity)
        let document = if is_local {
            let identity_dir = self.config.data_dir.join("identity");
            match crate::nostr_discovery::get_nostr_pubkey(&identity_dir).await {
                Ok(nostr_pubkey) => {
                    crate::identity::did_document_with_nostr(&pubkey_hex, &nostr_pubkey)?
                }
                Err(_) => crate::identity::did_document_from_pubkey_hex(&pubkey_hex)?,
            }
        } else {
            crate::identity::did_document_from_pubkey_hex(&pubkey_hex)?
        };
        Ok(document)
    }

    /// Verify a DID Document: validate structure, check key material matches DID.
    pub(in crate::api::rpc) async fn handle_identity_verify_did_document(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let document = params
            .get("document")
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: document"))?;

        // Validate required fields
        let did = document["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("DID Document missing 'id' field"))?;

        let context = document["@context"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("DID Document missing '@context' array"))?;

        let has_did_context = context
            .iter()
            .any(|c| c.as_str() == Some("https://www.w3.org/ns/did/v1"));
        if !has_did_context {
            return Ok(serde_json::json!({
                "valid": false,
                "errors": ["Missing required @context: https://www.w3.org/ns/did/v1"]
            }));
        }

        let verification_methods = document["verificationMethod"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("DID Document missing 'verificationMethod' array"))?;

        if verification_methods.is_empty() {
            return Ok(serde_json::json!({
                "valid": false,
                "errors": ["verificationMethod array is empty"]
            }));
        }

        // Verify the DID matches the key material (for did:key method)
        let mut errors: Vec<String> = Vec::new();

        if did.starts_with("did:key:") {
            match crate::identity::pubkey_bytes_from_did_key(did) {
                Ok(pubkey_bytes) => {
                    // Check that at least one verification method has matching key
                    let pubkey_multibase =
                        format!("z{}", bs58::encode(&pubkey_bytes).into_string());
                    let has_matching_key = verification_methods
                        .iter()
                        .any(|vm| vm["publicKeyMultibase"].as_str() == Some(&pubkey_multibase));
                    if !has_matching_key {
                        errors
                            .push("No verificationMethod matches the DID's public key".to_string());
                    }
                }
                Err(e) => {
                    errors.push(format!("Failed to extract pubkey from DID: {}", e));
                }
            }
        }

        // Check authentication is present
        if document["authentication"]
            .as_array()
            .is_none_or(|a| a.is_empty())
        {
            errors.push("Missing or empty 'authentication' field".to_string());
        }

        Ok(serde_json::json!({
            "valid": errors.is_empty(),
            "did": did,
            "errors": errors,
            "verification_methods": verification_methods.len(),
        }))
    }

    /// Create a Nostr keypair linked to an identity.
    pub(in crate::api::rpc) async fn handle_identity_create_nostr_key(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: id"))?;

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let pubkey_hex = manager.create_nostr_key(id).await?;

        // Derive npub (bech32 NIP-19) from hex
        let npub = nostr_sdk::PublicKey::from_hex(&pubkey_hex)
            .ok()
            .and_then(|pk| pk.to_bech32().ok());

        Ok(serde_json::json!({
            "nostr_pubkey": pubkey_hex,
            "nostr_npub": npub,
        }))
    }

    /// Sign a Nostr event with an identity's Nostr key.
    ///
    /// Accepts either:
    ///   - `event_hash` (hex) + `id` — sign a pre-computed hash
    ///   - `event` (full event object) — compute NIP-01 hash, fill pubkey, sign
    /// If `id` is omitted, uses the default identity.
    pub(in crate::api::rpc) async fn handle_identity_nostr_sign(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let (records, _) = manager.list().await?;

        // Resolve identity: prefer explicit id, then default, then any with Nostr key
        let id = if let Some(id) = params.get("id").and_then(|v| v.as_str()) {
            id.to_string()
        } else {
            // Prefer an identity with a Nostr key
            records
                .iter()
                .find(|r| r.nostr_pubkey.is_some())
                .map(|r| r.id.clone())
                .ok_or_else(|| anyhow::anyhow!("No identity with Nostr key found"))?
        };

        let identity = records
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| anyhow::anyhow!("Identity not found: {}", id))?;
        let pubkey_hex = identity
            .nostr_pubkey
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Identity has no Nostr key"))?;

        if let Some(event_hash) = params.get("event_hash").and_then(|v| v.as_str()) {
            // Direct hash signing
            let signature = manager.nostr_sign(&id, event_hash).await?;
            return Ok(serde_json::json!({ "signature": signature }));
        }

        // Full event signing: compute NIP-01 event hash
        let event = params
            .get("event")
            .ok_or_else(|| anyhow::anyhow!("Missing 'event' or 'event_hash' parameter"))?;

        let kind = event.get("kind").and_then(|v| v.as_u64()).unwrap_or(1);
        let content = event.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let created_at = event
            .get("created_at")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            });
        let tags = event
            .get("tags")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));

        // NIP-01 serialization: [0, pubkey, created_at, kind, tags, content]
        let serialized = serde_json::json!([0, pubkey_hex, created_at, kind, tags, content]);
        let serialized_str = serde_json::to_string(&serialized)?;

        // SHA-256 hash
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(serialized_str.as_bytes());
        let event_hash_hex = hex::encode(hash);

        let signature = manager.nostr_sign(&id, &event_hash_hex).await?;

        // Return the complete signed event
        Ok(serde_json::json!({
            "id": event_hash_hex,
            "pubkey": pubkey_hex,
            "created_at": created_at,
            "kind": kind,
            "tags": tags,
            "content": content,
            "sig": signature,
        }))
    }

    /// Resolve the identity ID from params, falling back to the default identity.
    async fn resolve_identity_id(&self, params: &serde_json::Value) -> Result<String> {
        if let Some(id) = params.get("id").and_then(|v| v.as_str()) {
            return Ok(id.to_string());
        }
        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let (records, default_id) = manager.list().await?;
        // Prefer the default identity
        if let Some(default_id) = default_id {
            return Ok(default_id);
        }
        // Fall back to first identity with a Nostr key, or just the first identity
        records
            .iter()
            .find(|i| i.nostr_pubkey.is_some())
            .or(records.first())
            .map(|i| i.id.clone())
            .ok_or_else(|| anyhow::anyhow!("No identity found"))
    }

    /// NIP-04 encrypt plaintext for a peer.
    pub(in crate::api::rpc) async fn handle_identity_nostr_encrypt_nip04(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let id = self.resolve_identity_id(&params).await?;
        let pubkey = params
            .get("pubkey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: pubkey"))?;
        let plaintext = params
            .get("plaintext")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: plaintext"))?;

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let ciphertext = manager.nostr_encrypt_nip04(&id, pubkey, plaintext).await?;

        Ok(serde_json::json!({ "ciphertext": ciphertext }))
    }

    /// NIP-04 decrypt ciphertext from a peer.
    pub(in crate::api::rpc) async fn handle_identity_nostr_decrypt_nip04(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let id = self.resolve_identity_id(&params).await?;
        let pubkey = params
            .get("pubkey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: pubkey"))?;
        let ciphertext = params
            .get("ciphertext")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: ciphertext"))?;

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let plaintext = manager.nostr_decrypt_nip04(&id, pubkey, ciphertext).await?;

        Ok(serde_json::json!({ "plaintext": plaintext }))
    }

    /// NIP-44 encrypt plaintext for a peer.
    pub(in crate::api::rpc) async fn handle_identity_nostr_encrypt_nip44(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let id = self.resolve_identity_id(&params).await?;
        let pubkey = params
            .get("pubkey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: pubkey"))?;
        let plaintext = params
            .get("plaintext")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: plaintext"))?;

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let ciphertext = manager.nostr_encrypt_nip44(&id, pubkey, plaintext).await?;

        Ok(serde_json::json!({ "ciphertext": ciphertext }))
    }

    /// NIP-44 decrypt ciphertext from a peer.
    pub(in crate::api::rpc) async fn handle_identity_nostr_decrypt_nip44(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let id = self.resolve_identity_id(&params).await?;
        let pubkey = params
            .get("pubkey")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: pubkey"))?;
        let ciphertext = params
            .get("ciphertext")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: ciphertext"))?;

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let plaintext = manager.nostr_decrypt_nip44(&id, pubkey, ciphertext).await?;

        Ok(serde_json::json!({ "plaintext": plaintext }))
    }

    /// Resolve a remote peer's DID Document over Tor.
    /// Queries the peer's /rpc/ endpoint for identity.resolve-did.
    pub(in crate::api::rpc) async fn handle_identity_resolve_remote_did(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let onion = params
            .get("onion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: onion"))?;

        // Build URL for peer's RPC endpoint over Tor
        let host = if onion.ends_with(".onion") {
            onion.to_string()
        } else {
            format!("{}.onion", onion)
        };
        let url = format!("http://{}/rpc/", host);

        // Use SOCKS5 proxy to reach .onion address
        let proxy = reqwest::Proxy::all(crate::constants::TOR_SOCKS_PROXY)
            .context("Failed to create Tor proxy")?;
        let client = reqwest::Client::builder()
            .proxy(proxy)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to build HTTP client")?;

        let rpc_body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "identity.resolve-did",
            "params": {}
        });

        let resp = client
            .post(&url)
            .json(&rpc_body)
            .send()
            .await
            .context("Failed to connect to peer over Tor")?;

        let body: serde_json::Value = resp.json().await.context("Failed to parse peer response")?;

        // Extract the DID Document from the RPC response
        let document = body
            .get("result")
            .ok_or_else(|| anyhow::anyhow!("Peer returned error or missing result"))?;

        // Cache the resolved DID locally
        let did = document["id"].as_str().unwrap_or("unknown");
        let cache_dir = self.config.data_dir.join("did-cache");
        tokio::fs::create_dir_all(&cache_dir).await.ok();
        let cache_file = cache_dir.join(format!("{}.json", onion.replace('.', "_")));
        let cache_entry = serde_json::json!({
            "document": document,
            "resolved_at": chrono::Utc::now().to_rfc3339(),
            "onion": onion,
        });
        tokio::fs::write(
            &cache_file,
            serde_json::to_string_pretty(&cache_entry).unwrap_or_default(),
        )
        .await
        .ok();

        Ok(serde_json::json!({
            "document": document,
            "did": did,
            "resolved_from": onion,
            "cached": true,
        }))
    }

    /// identity.create-dht-did — Publish an identity's DID to the Mainline DHT.
    pub(in crate::api::rpc) async fn handle_identity_create_dht_did(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let identity_id = params
            .get("identity_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing identity_id"))?;
        validate_identity_id(identity_id)?;

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let signing_key = manager.get_signing_key(identity_id).await?;

        let dht_did = did_dht::create_and_publish(&signing_key, &[]).await?;

        // Save the dht_did back to the identity record
        did_dht::save_dht_did(&self.config.data_dir, identity_id, &dht_did).await?;

        Ok(serde_json::json!({
            "dht_did": dht_did,
            "published": true,
        }))
    }

    /// identity.resolve-dht-did — Resolve a did:dht from the DHT.
    pub(in crate::api::rpc) async fn handle_identity_resolve_dht_did(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let did = params
            .get("did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing did"))?;

        if !did.starts_with("did:dht:") {
            anyhow::bail!("Not a did:dht identifier");
        }

        let doc = did_dht::resolve(did, None).await?;

        Ok(serde_json::json!({
            "did": did,
            "document": doc,
        }))
    }

    /// identity.refresh-dht-did — Re-publish an identity's did:dht to keep it alive in the DHT.
    pub(in crate::api::rpc) async fn handle_identity_refresh_dht_did(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let identity_id = params
            .get("identity_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing identity_id"))?;
        validate_identity_id(identity_id)?;

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let record = manager.get(identity_id).await?;

        if record.dht_did.is_none() {
            anyhow::bail!(
                "Identity has no did:dht — create one first with identity.create-dht-did"
            );
        }

        let signing_key = manager.get_signing_key(identity_id).await?;
        let dht_did = did_dht::create_and_publish(&signing_key, &[]).await?;

        Ok(serde_json::json!({
            "dht_did": dht_did,
            "refreshed": true,
        }))
    }

    /// Update profile metadata for an identity.
    pub(in crate::api::rpc) async fn handle_identity_update_profile(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: id"))?;
        validate_identity_id(id)?;

        let profile = IdentityProfile {
            display_name: params
                .get("display_name")
                .and_then(|v| v.as_str())
                .map(String::from),
            about: params
                .get("about")
                .and_then(|v| v.as_str())
                .map(String::from),
            picture: params
                .get("picture")
                .and_then(|v| v.as_str())
                .map(String::from),
            banner: params
                .get("banner")
                .and_then(|v| v.as_str())
                .map(String::from),
            website: params
                .get("website")
                .and_then(|v| v.as_str())
                .map(String::from),
            nip05: params
                .get("nip05")
                .and_then(|v| v.as_str())
                .map(String::from),
            lud16: params
                .get("lud16")
                .and_then(|v| v.as_str())
                .map(String::from),
        };

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        manager.update_profile(id, profile).await?;

        Ok(serde_json::json!({ "ok": true }))
    }

    /// Publish kind 0 (metadata) profile to every enabled Nostr relay
    /// configured in Manage Relays. Callers can override the default
    /// list by passing `relays: [..]` in params (e.g. to publish to a
    /// single relay for testing).
    pub(in crate::api::rpc) async fn handle_identity_publish_profile(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: id"))?;
        validate_identity_id(id)?;

        let relay_urls: Vec<String> =
            if let Some(arr) = params.get("relays").and_then(|v| v.as_array()) {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect()
            } else if let Some(single) = params.get("relay").and_then(|v| v.as_str()) {
                vec![single.to_string()]
            } else {
                // Default: every enabled relay in the user's Manage Relays list.
                let statuses = crate::nostr_relays::list_relays(&self.config.data_dir)
                    .await
                    .unwrap_or_default();
                statuses
                    .into_iter()
                    .filter(|s| s.enabled)
                    .map(|s| s.url)
                    .collect()
            };

        if relay_urls.is_empty() {
            anyhow::bail!("No enabled relays configured; add one under Manage Relays");
        }

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let outcome = manager.publish_profile(id, &relay_urls).await?;

        Ok(serde_json::json!({
            "event_id": outcome.event_id,
            "accepted": outcome.accepted,
            "rejected": outcome.rejected,
            "relays_attempted": relay_urls.len(),
            "published": !outcome.accepted.is_empty(),
        }))
    }

    /// Export private keys for an identity — REQUIRES password verification.
    pub(in crate::api::rpc) async fn handle_identity_export_keys(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: id"))?;
        let password = params
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: password"))?;
        validate_identity_id(id)?;

        // Verify password against auth system
        if !self.auth_manager.verify_password(password).await? {
            anyhow::bail!("Invalid password");
        }

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let keys = manager.export_keys(id).await?;
        let record = manager.get(id).await?;

        Ok(serde_json::json!({
            "id": record.id,
            "name": record.name,
            "pubkey": record.pubkey_hex,
            "did": record.did,
            "nostr_pubkey": record.nostr_pubkey,
            "nostr_npub": record.nostr_npub,
            "ed25519_secret_hex": keys["ed25519_secret_hex"],
            "nostr_secret_hex": keys["nostr_secret_hex"],
            "nostr_nsec": keys["nostr_nsec"],
        }))
    }

    /// identity.dht-status — Check if an identity's did:dht is published and resolvable.
    pub(in crate::api::rpc) async fn handle_identity_dht_status(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let identity_id = params
            .get("identity_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing identity_id"))?;
        validate_identity_id(identity_id)?;

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let record = manager.get(identity_id).await?;

        let (published, resolvable) = match &record.dht_did {
            Some(dht_did) => {
                let resolvable = did_dht::resolve(dht_did, None).await.is_ok();
                (true, resolvable)
            }
            None => (false, false),
        };

        Ok(serde_json::json!({
            "identity_id": identity_id,
            "did_key": record.did,
            "dht_did": record.dht_did,
            "published": published,
            "resolvable": resolvable,
        }))
    }
}
