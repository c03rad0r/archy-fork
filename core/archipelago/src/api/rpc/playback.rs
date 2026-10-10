//! Native broker only: settled purchases become session-bound opaque media URLs.
use super::RpcHandler;
use anyhow::{Context, Result};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Issue {
    purchase_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Status {
    handle: String,
}
impl RpcHandler {
    async fn playback_context(
        &self,
        session: &Option<String>,
    ) -> Result<(
        String,
        crate::container::registration_pin::InstalledAppContext,
    )> {
        let session = session.as_ref().context("Owner session required")?;
        anyhow::ensure!(
            self.session_store.validate(session).await,
            "Owner session expired"
        );
        let identity =
            crate::identity::NodeIdentity::load_existing(&self.config.data_dir.join("identity"))
                .await?;
        let (state, _) = self.state_manager.get_snapshot().await;
        let root = self.config.data_dir.clone();
        let context = tokio::task::spawn_blocking(move || {
            crate::container::registration_pin::installed_context(&root, &identity, &state)
        })
        .await??;
        Ok((session.clone(), context))
    }
    pub(super) async fn handle_playback_handle(
        &self,
        params: Option<serde_json::Value>,
        session: &Option<String>,
    ) -> Result<serde_json::Value> {
        let params: Issue = serde_json::from_value(params.context("Missing purchase identifier")?)?;
        let (session, context) = self.playback_context(session).await?;
        let handle = self
            .playback_handles()
            .issue(
                &self.config.data_dir,
                context.clone(),
                &session,
                &params.purchase_id,
            )
            .await?;
        let expires = self
            .playback_handles()
            .expiry(&handle, &session, &context)?;
        Ok(serde_json::json!({"handle":handle,"expires_at":expires}))
    }
    pub(super) async fn handle_playback_status(
        &self,
        params: Option<serde_json::Value>,
        session: &Option<String>,
    ) -> Result<serde_json::Value> {
        let params: Status = serde_json::from_value(params.context("Missing playback handle")?)?;
        let (session, context) = self.playback_context(session).await?;
        let expires = self
            .playback_handles()
            .expiry(&params.handle, &session, &context)?;
        Ok(serde_json::json!({"expires_at":expires}))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepare {
    handle: String,
    #[serde(default)]
    retry: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    handle: String,
    ready_id: String,
}
impl RpcHandler {
    async fn playback_control(
        &self,
        handle: &str,
        ready_id: Option<&str>,
        retry: bool,
        session: &Option<String>,
    ) -> Result<serde_json::Value> {
        let (session, context) = self.playback_context(session).await?;
        let binding = self.playback_handles().lookup(handle, &session, &context)?;
        let (capability, duration) = {
            let journal = crate::content_purchase::Journal::open(&self.config.data_dir).await?;
            let record = journal
                .buyer(&binding.contract.id)
                .await?
                .context("Original purchase missing")?;
            anyhow::ensure!(
                record.contract == binding.contract,
                "Original purchase changed"
            );
            let capability = record
                .receipt()
                .context("Original payment is not settled")?
                .capability
                .clone();
            let envelope = journal
                .protocol_envelope("buyer", &binding.contract.id)
                .await?
                .context("Original rental terms missing")?;
            (
                capability,
                envelope
                    .offer
                    .viewing_seconds
                    .context("Purchase is not a timed rental")?,
            )
        };
        let peer = crate::federation::load_unique_payment_peer(
            &self.config.data_dir,
            &binding.seller_onion,
        )
        .await?;
        anyhow::ensure!(
            peer.did == binding.contract.seller_did,
            "Purchased seller identity changed"
        );
        let mesh = peer.fips_npub.context("Seller mesh binding unavailable")?;
        let action = if ready_id.is_some() {
            "start"
        } else {
            "prepare"
        };
        let path = format!(
            "/content/{}/rental/{}/{action}",
            binding.contract.content_id, binding.contract.id
        );
        let body = serde_json::json!({"capability":capability,"ready_id":ready_id,"retry":retry});
        let (mut response, _) =
            crate::fips::dial::PeerRequest::new(Some(&mesh), &binding.seller_onion, &path)
                .require_fips()
                .single_delivery()
                .timeout(std::time::Duration::from_secs(20))
                .send_rental_control_json(
                    &self.config.data_dir,
                    &binding.contract.seller_did,
                    &body,
                )
                .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "Original rental is unavailable; retry this purchase without paying again"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(
                bytes
                    .len()
                    .checked_add(chunk.len())
                    .is_some_and(|n| n <= 16 * 1024),
                "Rental control response too large"
            );
            bytes.extend_from_slice(&chunk);
        }
        let remote: serde_json::Value = serde_json::from_slice(&bytes)?;
        let state = remote["state"].as_str().context("Missing rental state")?;
        let window = match (remote["started_at"].as_u64(), remote["expires_at"].as_u64()) {
            (None, None) if remote["started_at"].is_null() && remote["expires_at"].is_null() => {
                None
            }
            (Some(started), Some(expires))
                if started > 0 && started.checked_add(duration) == Some(expires) =>
            {
                Some((started, expires))
            }
            _ => anyhow::bail!("Seller changed the original rental window"),
        };
        let (started_at, expires_at) =
            window.map_or((None, None), |(start, end)| (Some(start), Some(end)));
        let result = match state {
            "preparing" if ready_id.is_none() => {
                let completed = remote["completed_bytes"]
                    .as_u64()
                    .context("Invalid verification progress")?;
                anyhow::ensure!(
                    remote["total_bytes"].as_u64() == Some(binding.contract.content_size)
                        && completed <= binding.contract.content_size
                        && remote["viewing_seconds"].as_u64() == Some(duration),
                    "Rental preparation terms changed"
                );
                serde_json::json!({"state":"preparing","completed_bytes":completed,"total_bytes":binding.contract.content_size,
                    "viewing_seconds":duration,"started_at":started_at,"expires_at":expires_at})
            }
            "ready" if ready_id.is_none() => {
                let id = remote["ready_id"]
                    .as_str()
                    .context("Missing readiness identifier")?;
                let parsed = uuid::Uuid::parse_str(id)?;
                anyhow::ensure!(
                    parsed.to_string() == id
                        && parsed.get_version_num() == 4
                        && remote["total_bytes"].as_u64() == Some(binding.contract.content_size)
                        && remote["viewing_seconds"].as_u64() == Some(duration),
                    "Rental readiness changed"
                );
                serde_json::json!({"state":"ready","ready_id":id,"handle":handle,"viewing_seconds":duration,
                    "started_at":started_at,"expires_at":expires_at})
            }
            "unavailable" if ready_id.is_none() => {
                serde_json::json!({"state":"unavailable","expires_at":expires_at})
            }
            "expired" if window.is_some() => {
                serde_json::json!({"state":"expired","started_at":started_at,"expires_at":expires_at})
            }
            "started" if ready_id.is_some() && window.is_some() => {
                serde_json::json!({"state":"started","started_at":started_at,
                "expires_at":expires_at,"playback_url":format!("/api/rental-playback/{handle}")})
            }
            _ => {
                anyhow::bail!("Unexpected rental state; recover this purchase without paying again")
            }
        };
        if let Some((_, expires)) = window {
            self.playback_handles()
                .note_expiry(handle, &binding, expires)?;
        }
        Ok(result)
    }
    pub(super) async fn handle_playback_prepare(
        &self,
        params: Option<serde_json::Value>,
        session: &Option<String>,
    ) -> Result<serde_json::Value> {
        let input: Prepare = serde_json::from_value(params.context("Missing playback handle")?)?;
        self.playback_control(&input.handle, None, input.retry, session)
            .await
    }
    pub(super) async fn handle_playback_start(
        &self,
        params: Option<serde_json::Value>,
        session: &Option<String>,
    ) -> Result<serde_json::Value> {
        let input: Start = serde_json::from_value(params.context("Missing readiness identifier")?)?;
        self.playback_control(&input.handle, Some(&input.ready_id), false, session)
            .await
    }
}
