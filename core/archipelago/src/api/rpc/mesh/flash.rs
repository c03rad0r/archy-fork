use super::super::RpcHandler;
use crate::mesh;
use crate::mesh::flash::{self, FlashBoard, FlashJobStatus};
use crate::mesh::types::DeviceType;
use anyhow::Result;

fn parse_family(s: &str) -> Result<DeviceType> {
    match s.trim().to_lowercase().as_str() {
        "meshcore" => Ok(DeviceType::Meshcore),
        "meshtastic" => Ok(DeviceType::Meshtastic),
        "reticulum" | "rnode" => Ok(DeviceType::Reticulum),
        other => anyhow::bail!(
            "Unknown firmware family: {other} (expected meshcore|meshtastic|reticulum)"
        ),
    }
}

fn parse_board(s: &str) -> Result<FlashBoard> {
    match s.trim().to_lowercase().as_str() {
        "heltec-v3" | "heltec_v3" | "heltecv3" => Ok(FlashBoard::HeltecV3),
        "heltec-v4" | "heltec_v4" | "heltecv4" => Ok(FlashBoard::HeltecV4),
        other => anyhow::bail!("Unknown board: {other} (expected heltec-v3|heltec-v4)"),
    }
}

impl RpcHandler {
    /// mesh.flash-list-firmware — resolve the available firmware version(s)
    /// for a given family. v1 only ever surfaces "latest".
    pub(in crate::api::rpc) async fn handle_mesh_flash_list_firmware(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let family = params
            .as_ref()
            .and_then(|p| p.get("family"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing family"))?;
        let family = parse_family(family)?;
        let versions = flash::list_firmware(family).await?;
        Ok(serde_json::json!({ "versions": versions }))
    }

    /// mesh.flash-device — erase and reflash a detected LoRa radio with the
    /// latest firmware for the given family, defaulting to a full chip
    /// erase before write. `board` is optional: if the port's USB vid:pid
    /// unambiguously resolves to a known board, that's used; otherwise the
    /// caller must supply it explicitly (see `flash::resolve_flash_board`'s
    /// doc comment on why we refuse to guess).
    pub(in crate::api::rpc) async fn handle_mesh_flash_device(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let path = params
            .as_ref()
            .and_then(|p| p.get("path"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing path"))?
            .to_string();
        let family = params
            .as_ref()
            .and_then(|p| p.get("family"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing family"))?;
        let family = parse_family(family)?;

        let detected = mesh::detect_devices().await;
        anyhow::ensure!(
            detected.iter().any(|d| d == &path),
            "{path} is not a detected mesh-radio candidate port"
        );

        let board = match params
            .as_ref()
            .and_then(|p| p.get("board"))
            .and_then(|v| v.as_str())
        {
            Some(explicit) => parse_board(explicit)?,
            None => {
                let info = mesh::detect_devices_info()
                    .await
                    .into_iter()
                    .find(|d| d.path == path);
                info.as_ref()
                    .and_then(flash::resolve_flash_board)
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Could not auto-detect the board on {path} — specify board explicitly"
                        )
                    })?
            }
        };

        flash::start_flash_job(
            &self.flash_job,
            &self.mesh_service_arc(),
            self.config.data_dir.clone(),
            path,
            board,
            family,
        )
        .await?;

        Ok(serde_json::json!({ "started": true }))
    }

    /// mesh.flash-status — poll the current (or most recent) flash job.
    pub(in crate::api::rpc) async fn handle_mesh_flash_status(&self) -> Result<serde_json::Value> {
        let job = self.flash_job.read().await;
        match job.as_ref() {
            Some(j) => {
                let status: FlashJobStatus = j.snapshot().await;
                let mut value = serde_json::to_value(&status)?;
                if let Some(obj) = value.as_object_mut() {
                    obj.insert("active".into(), (!status.done).into());
                }
                Ok(value)
            }
            None => Ok(serde_json::json!({ "active": false })),
        }
    }

    /// mesh.flash-cancel — best-effort; only honored before erase/write has
    /// started (see `FlashJob::cancel`'s doc comment).
    pub(in crate::api::rpc) async fn handle_mesh_flash_cancel(&self) -> Result<serde_json::Value> {
        let job = self.flash_job.read().await;
        match job.as_ref() {
            Some(j) => {
                j.cancel().await?;
                Ok(serde_json::json!({ "cancelled": true }))
            }
            None => anyhow::bail!("No flash job in progress"),
        }
    }
}
