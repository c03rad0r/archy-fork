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
        Ok(
            serde_json::json!({"playback_url":format!("/api/rental-playback/{handle}"), "expires_at":expires}),
        )
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
