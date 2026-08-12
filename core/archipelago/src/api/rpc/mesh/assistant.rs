//! Mesh-AI assistant RPCs (issue #50): read/update the local assistant config
//! and report whether a local Ollama is available (for the install deep-link).

use super::super::RpcHandler;
use anyhow::Result;
use std::time::Duration;

/// Default model when the node hasn't picked one (kept in sync with the mesh
/// assistant handler's `DEFAULT_MODEL`).
const DEFAULT_MODEL: &str = "qwen2.5-coder";

impl RpcHandler {
    /// mesh.assistant-status — current settings + local Ollama availability.
    pub(in crate::api::rpc) async fn handle_mesh_assistant_status(
        &self,
    ) -> Result<serde_json::Value> {
        let (cfg, denied_askers) = {
            let service = self.mesh_service.read().await;
            let svc = service
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
            (
                svc.assistant_config().await,
                svc.assistant_denied_askers().await,
            )
        };

        let (ollama_detected, models) = detect_ollama().await;
        let claude_available =
            tokio::fs::metadata(self.config.data_dir.join("secrets/claude-api-key"))
                .await
                .is_ok();
        Ok(serde_json::json!({
            "enabled": cfg.enabled,
            "model": cfg.model,
            "trusted_only": cfg.trusted_only,
            "backend": cfg.backend,
            "allowed_contacts": cfg.allowed_contacts,
            "default_model": DEFAULT_MODEL,
            "ollama_detected": ollama_detected,
            "claude_available": claude_available,
            "models": models,
            "denied_askers": denied_askers,
        }))
    }

    /// mesh.assistant-configure — update assistant settings live.
    /// Params: `enabled?: bool`, `trusted_only?: bool`,
    /// `model?: string|null` (string sets, null clears to default, absent leaves).
    pub(in crate::api::rpc) async fn handle_mesh_assistant_configure(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.unwrap_or_default();
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;

        let enabled = params.get("enabled").and_then(|v| v.as_bool());
        let trusted_only = params.get("trusted_only").and_then(|v| v.as_bool());
        let backend = params
            .get("backend")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        // model: key present + string => set; present + null => clear; absent => leave
        let model = if let Some(v) = params.get("model") {
            Some(v.as_str().map(|s| s.to_string()))
        } else {
            None
        };
        // allowed_contacts: present + array => replace the allowlist (pubkey hex
        // strings); absent => leave unchanged.
        let allowed_contacts = params
            .get("allowed_contacts")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| e.as_str().map(|s| s.to_string()))
                    .collect::<Vec<String>>()
            });

        svc.configure_assistant(enabled, model, trusted_only, backend, allowed_contacts)
            .await?;
        let cfg = svc.assistant_config().await;
        Ok(serde_json::json!({
            "enabled": cfg.enabled,
            "model": cfg.model,
            "trusted_only": cfg.trusted_only,
            "backend": cfg.backend,
            "allowed_contacts": cfg.allowed_contacts,
        }))
    }

    /// mesh.schedule-message — queue a message to send at a future time.
    /// Params: `body: string`, `fire_at: i64` (unix secs), and one of
    /// `contact_id: u32` (DM) or `channel: u8` (broadcast).
    pub(in crate::api::rpc) async fn handle_mesh_schedule_message(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let p = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let body = p
            .get("body")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("body is required"))?
            .to_string();
        let fire_at = p
            .get("fire_at")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("fire_at (unix seconds) is required"))?;
        let contact_id = p
            .get("contact_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        let channel = p.get("channel").and_then(|v| v.as_u64()).map(|v| v as u8);
        if contact_id.is_none() && channel.is_none() {
            anyhow::bail!("either contact_id or channel is required");
        }

        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let msg = svc
            .scheduler
            .add(contact_id, channel, body, fire_at)
            .await?;
        Ok(serde_json::to_value(msg)?)
    }

    /// mesh.list-scheduled — list queued messages (sorted by fire time).
    pub(in crate::api::rpc) async fn handle_mesh_list_scheduled(
        &self,
    ) -> Result<serde_json::Value> {
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let messages = svc.scheduler.list().await;
        Ok(serde_json::json!({ "messages": messages }))
    }

    /// mesh.cancel-scheduled — remove a queued message by id.
    pub(in crate::api::rpc) async fn handle_mesh_cancel_scheduled(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let id = params
            .as_ref()
            .and_then(|p| p.get("id"))
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("id is required"))?;
        let service = self.mesh_service.read().await;
        let svc = service
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Mesh service not running"))?;
        let cancelled = svc.scheduler.cancel(id).await?;
        Ok(serde_json::json!({ "cancelled": cancelled }))
    }
}

/// Probe the local Ollama HTTP API; return (detected, model_names).
///
/// `pub(crate)`: reused directly by `crate::assistant::backends::select_backend`
/// (13-10, D-04) rather than a second probe being written there.
pub(crate) async fn detect_ollama() -> (bool, Vec<String>) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return (false, Vec::new()),
    };
    match client.get("http://localhost:11434/api/tags").send().await {
        Ok(resp) if resp.status().is_success() => {
            let json: serde_json::Value = resp.json().await.unwrap_or_default();
            let models = json
                .get("models")
                .and_then(|m| m.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| {
                            m.get("name")
                                .and_then(|n| n.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect()
                })
                .unwrap_or_default();
            (true, models)
        }
        _ => (false, Vec::new()),
    }
}
