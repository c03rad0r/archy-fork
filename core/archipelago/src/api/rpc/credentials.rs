use super::RpcHandler;
use crate::credentials;
use crate::identity_manager::IdentityManager;
use anyhow::Result;

impl RpcHandler {
    /// Issue a Verifiable Credential from one of the user's identities.
    pub(super) async fn handle_identity_issue_credential(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let issuer_id = params
            .get("issuer_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing issuer_id"))?;
        let subject_did = params
            .get("subject_did")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing subject_did"))?;
        let credential_type = params
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("VerifiableCredential");
        let claims = params
            .get("claims")
            .cloned()
            .unwrap_or(serde_json::json!({}));
        let expires_at = params.get("expires_at").and_then(|v| v.as_str());

        let prefer_dht = params
            .get("prefer_dht_did")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let issuer_record = manager.get(issuer_id).await?;
        // Use did:dht if available and preferred, otherwise did:key
        let issuer_did = if prefer_dht {
            issuer_record
                .dht_did
                .as_deref()
                .unwrap_or(&issuer_record.did)
                .to_string()
        } else {
            issuer_record.did.clone()
        };

        // Capture identity_id for the signing closure
        let data_dir = self.config.data_dir.clone();
        let sign_id = issuer_id.to_string();

        let vc = credentials::issue_credential(
            &self.config.data_dir,
            &issuer_did,
            subject_did,
            credential_type,
            claims,
            expires_at,
            |bytes| {
                // Use block_in_place to avoid deadlocking the tokio runtime
                let hex_msg = hex::encode(bytes);
                tokio::task::block_in_place(|| {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let mgr = IdentityManager::new(&data_dir).await?;
                        mgr.sign(&sign_id, hex_msg.as_bytes()).await
                    })
                })
            },
        )
        .await?;

        let status = if credentials::is_revoked(&vc) {
            "revoked"
        } else {
            "active"
        };

        Ok(serde_json::json!({
            "id": vc.id,
            "issuer": vc.issuer,
            "subject": vc.credential_subject.id,
            "type": vc.credential_type,
            "issued_at": vc.issuance_date,
            "status": status,
        }))
    }

    /// Verify a credential by its ID.
    pub(super) async fn handle_identity_verify_credential(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let credential_id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?;

        let store = credentials::load_credentials(&self.config.data_dir).await?;
        let vc = store
            .credentials
            .iter()
            .find(|c| c.id == credential_id)
            .ok_or_else(|| anyhow::anyhow!("Credential not found"))?;

        let data_dir = self.config.data_dir.clone();
        let valid = credentials::verify_credential(vc, |did, bytes, signature| {
            let hex_msg = hex::encode(bytes);
            tokio::task::block_in_place(|| {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    let mgr = IdentityManager::new(&data_dir).await?;
                    mgr.verify(did, hex_msg.as_bytes(), signature).await
                })
            })
        })?;

        let status = if credentials::is_revoked(vc) {
            "revoked"
        } else {
            "active"
        };

        Ok(serde_json::json!({
            "id": vc.id,
            "valid": valid,
            "status": status,
        }))
    }

    /// List all credentials, optionally filtered by DID.
    pub(super) async fn handle_identity_list_credentials(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let filter_did = params
            .as_ref()
            .and_then(|p| p.get("did"))
            .and_then(|v| v.as_str());

        let creds = credentials::list_credentials(&self.config.data_dir, filter_did).await?;
        let items: Vec<serde_json::Value> = creds
            .into_iter()
            .map(|c| {
                let status = if credentials::is_revoked(&c) {
                    "revoked"
                } else {
                    "active"
                };
                serde_json::json!({
                    "@context": c.context,
                    "id": c.id,
                    "type": c.credential_type,
                    "issuer": c.issuer,
                    "credentialSubject": c.credential_subject,
                    "issuanceDate": c.issuance_date,
                    "expirationDate": c.expiration_date,
                    "proof": c.proof,
                    "status": status,
                })
            })
            .collect();
        Ok(serde_json::json!({ "credentials": items }))
    }

    /// Revoke a credential.
    pub(super) async fn handle_identity_revoke_credential(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?;

        credentials::revoke_credential(&self.config.data_dir, id).await?;
        Ok(serde_json::json!({ "ok": true }))
    }

    /// Create a Verifiable Presentation bundling selected credentials.
    pub(super) async fn handle_identity_create_presentation(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let holder_id = params
            .get("holder_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing holder_id"))?;
        let credential_ids: Vec<&str> = params
            .get("credential_ids")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing credential_ids array"))?
            .iter()
            .filter_map(|v| v.as_str())
            .collect();

        if credential_ids.is_empty() {
            return Err(anyhow::anyhow!("credential_ids must not be empty"));
        }

        let manager = IdentityManager::new(&self.config.data_dir).await?;
        let holder_record = manager.get(holder_id).await?;
        let holder_did = holder_record.did.clone();

        let store = credentials::load_credentials(&self.config.data_dir).await?;

        let data_dir = self.config.data_dir.clone();
        let sign_id = holder_id.to_string();

        let vp = credentials::create_presentation(
            &holder_did,
            &credential_ids,
            &store.credentials,
            |bytes| {
                let hex_msg = hex::encode(bytes);
                tokio::task::block_in_place(|| {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        let mgr = IdentityManager::new(&data_dir).await?;
                        mgr.sign(&sign_id, hex_msg.as_bytes()).await
                    })
                })
            },
        )?;

        Ok(serde_json::to_value(&vp)?)
    }

    /// Verify a Verifiable Presentation: check holder proof and all embedded credentials.
    pub(super) async fn handle_identity_verify_presentation(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let presentation = params
            .get("presentation")
            .ok_or_else(|| anyhow::anyhow!("Missing presentation"))?;

        let vp: credentials::VerifiablePresentation = serde_json::from_value(presentation.clone())?;

        let data_dir = self.config.data_dir.clone();
        let result = credentials::verify_presentation(&vp, |did, bytes, signature| {
            let hex_msg = hex::encode(bytes);
            tokio::task::block_in_place(|| {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    let mgr = IdentityManager::new(&data_dir).await?;
                    mgr.verify(did, hex_msg.as_bytes(), signature).await
                })
            })
        })?;

        Ok(serde_json::json!({
            "valid": result.valid,
            "holder_valid": result.holder_valid,
            "credentials": result.credentials,
        }))
    }
}
