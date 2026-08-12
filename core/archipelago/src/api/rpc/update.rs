use super::RpcHandler;
use crate::update;
use anyhow::{Context, Result};

impl RpcHandler {
    /// Check for available system updates.
    /// Prefer manifest-based OTA so installed nodes with a checked-out repo do
    /// not depend on a potentially stale git remote. Git remains a dev fallback.
    pub(super) async fn handle_update_check(&self) -> Result<serde_json::Value> {
        let state = update::check_for_updates(&self.config.data_dir).await?;

        let update_info = state.available_update.as_ref().map(|u| {
            serde_json::json!({
                "version": u.version,
                "release_date": u.release_date,
                "changelog": u.changelog,
                "components": u.components.len(),
            })
        });

        if update_info.is_some() {
            return Ok(serde_json::json!({
                "current_version": state.current_version,
                "last_check": state.last_check,
                "update_available": true,
                "update": update_info,
                "manifest_mirror": state.manifest_mirror,
            }));
        }

        let repo_dir = std::path::PathBuf::from(
            std::env::var("HOME").unwrap_or_else(|_| "/home/archipelago".to_string()),
        )
        .join("archy");
        if std::env::var("ARCHIPELAGO_GIT_UPDATES").is_ok() && repo_dir.join(".git").exists() {
            if let Ok(git_status) = self.git_check_update(&repo_dir).await {
                return Ok(git_status);
            }
        }

        Ok(serde_json::json!({
            "current_version": state.current_version,
            "last_check": state.last_check,
            "update_available": false,
            "update": update_info,
            "manifest_mirror": state.manifest_mirror,
        }))
    }

    /// Git-based update check: runs `git fetch` and compares HEAD to origin/main.
    async fn git_check_update(&self, repo_dir: &std::path::Path) -> Result<serde_json::Value> {
        let repo_str = repo_dir.to_string_lossy().to_string();

        // git fetch origin main
        let fetch = tokio::process::Command::new("git")
            .args(["fetch", "origin", "main", "--quiet"])
            .current_dir(&repo_str)
            .output()
            .await
            .context("git fetch failed")?;

        if !fetch.status.success() {
            anyhow::bail!(
                "git fetch failed: {}",
                String::from_utf8_lossy(&fetch.stderr)
            );
        }

        // Get local and remote HEADs
        let local = tokio::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(&repo_str)
            .output()
            .await?;
        let local_hash = String::from_utf8_lossy(&local.stdout).trim().to_string();

        let remote = tokio::process::Command::new("git")
            .args(["rev-parse", "--short", "origin/main"])
            .current_dir(&repo_str)
            .output()
            .await?;
        let remote_hash = String::from_utf8_lossy(&remote.stdout).trim().to_string();

        let update_available = local_hash != remote_hash;

        // Get commit count and changelog if update available
        let mut changelog = Vec::new();
        let mut commits_behind = 0u64;
        if update_available {
            let count = tokio::process::Command::new("git")
                .args(["rev-list", "HEAD..origin/main", "--count"])
                .current_dir(&repo_str)
                .output()
                .await?;
            commits_behind = String::from_utf8_lossy(&count.stdout)
                .trim()
                .parse()
                .unwrap_or(0);

            let log = tokio::process::Command::new("git")
                .args([
                    "log",
                    "HEAD..origin/main",
                    "--oneline",
                    "--no-merges",
                    "-20",
                ])
                .current_dir(&repo_str)
                .output()
                .await?;
            changelog = String::from_utf8_lossy(&log.stdout)
                .lines()
                .map(|l| l.to_string())
                .collect();
        }

        let now = chrono::Utc::now().to_rfc3339();

        Ok(serde_json::json!({
            "current_version": local_hash,
            "last_check": now,
            "update_available": update_available,
            "update_method": "git",
            "update": if update_available {
                Some(serde_json::json!({
                    "version": remote_hash,
                    "commits_behind": commits_behind,
                    "changelog": changelog,
                }))
            } else { None },
        }))
    }

    /// Apply git-based update: runs self-update.sh which pulls, builds, and restarts.
    pub(super) async fn handle_update_git_apply(&self) -> Result<serde_json::Value> {
        if std::env::var("ARCHIPELAGO_GIT_UPDATES").is_err() {
            anyhow::bail!("git/self-build updates are disabled; use manifest OTA updates instead");
        }

        let script = std::path::PathBuf::from(
            std::env::var("HOME").unwrap_or_else(|_| "/home/archipelago".to_string()),
        )
        .join("archy/scripts/self-update.sh");

        if !script.exists() {
            anyhow::bail!("self-update.sh not found at {}", script.display());
        }

        // Spawn the update script in the background (it will restart the service)
        let child = tokio::process::Command::new("bash")
            .arg(&script)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .context("Failed to spawn self-update.sh")?;

        tracing::info!(pid = child.id(), "Self-update script spawned");

        Ok(serde_json::json!({
            "started": true,
            "message": "Update started. The service will restart when complete.",
        }))
    }

    /// Get update status without checking remote.
    pub(super) async fn handle_update_status(&self) -> Result<serde_json::Value> {
        let state = update::get_status(&self.config.data_dir).await?;
        // Expose live download progress so the UI can resume the
        // progress bar after navigation instead of showing the fake
        // creep again. An RPC poll every ~1s during download drives a
        // real progress indicator that survives route changes.
        let downloaded = update::DOWNLOAD_BYTES.load(std::sync::atomic::Ordering::Relaxed);
        let total = update::DOWNLOAD_TOTAL.load(std::sync::atomic::Ordering::Relaxed);
        let active = total > 0 && downloaded < total;
        let completed = total > 0 && downloaded >= total;

        // Stall detection: if the progress-at timestamp hasn't advanced
        // for 30+ seconds while active, the download is wedged (usually
        // HTTP stream silently dropped and reqwest is waiting out its
        // read timeout). The UI uses this to surface a Cancel button
        // with explanatory copy.
        let stalled = if active {
            let last_at = update::DOWNLOAD_PROGRESS_AT.load(std::sync::atomic::Ordering::Relaxed);
            if last_at > 0 {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                now.saturating_sub(last_at) > 30_000
            } else {
                false
            }
        } else {
            false
        };

        Ok(serde_json::json!({
            "current_version": state.current_version,
            "last_check": state.last_check,
            "update_available": state.available_update.is_some(),
            "update_in_progress": state.update_in_progress,
            "rollback_available": state.rollback_available,
            "manifest_mirror": state.manifest_mirror,
            "download_progress": if active || completed {
                Some(serde_json::json!({
                    "bytes_downloaded": downloaded,
                    "total_bytes": total,
                    "active": active,
                    "stalled": stalled,
                }))
            } else { None },
        }))
    }

    /// Dismiss the update notification.
    pub(super) async fn handle_update_dismiss(&self) -> Result<serde_json::Value> {
        update::dismiss_update(&self.config.data_dir).await?;
        Ok(serde_json::json!({ "ok": true }))
    }

    /// Download the available update to staging.
    pub(super) async fn handle_update_download(&self) -> Result<serde_json::Value> {
        let progress = update::download_update(&self.config.data_dir).await?;
        Ok(serde_json::json!({
            "total_bytes": progress.total_bytes,
            "downloaded_bytes": progress.downloaded_bytes,
            "components_downloaded": progress.components_downloaded,
        }))
    }

    /// Cancel an in-flight or stuck download. Clears the live counters
    /// and staging dir so the UI returns to the "Download Update" state.
    pub(super) async fn handle_update_cancel_download(&self) -> Result<serde_json::Value> {
        update::cancel_download(&self.config.data_dir).await?;
        Ok(serde_json::json!({ "canceled": true }))
    }

    /// Apply the staged update.
    pub(super) async fn handle_update_apply(&self) -> Result<serde_json::Value> {
        update::apply_update(&self.config.data_dir).await?;
        Ok(serde_json::json!({ "applied": true, "restart_required": true }))
    }

    /// Rollback to the previous version.
    pub(super) async fn handle_update_rollback(&self) -> Result<serde_json::Value> {
        update::rollback_update(&self.config.data_dir).await?;
        Ok(serde_json::json!({ "rolled_back": true, "restart_required": true }))
    }

    /// List configured update mirrors in priority order.
    pub(super) async fn handle_update_list_mirrors(&self) -> Result<serde_json::Value> {
        let list = update::load_mirrors(&self.config.data_dir).await?;
        Ok(serde_json::json!({ "mirrors": list }))
    }

    /// Report the node's swarm prefs (fetch source + whether it provides to the
    /// swarm) plus swarm capability, so the UI can show whether DHT mode is
    /// actually usable on this build.
    pub(super) async fn handle_update_get_source(&self) -> Result<serde_json::Value> {
        let source = update::load_update_source(&self.config.data_dir).await;
        let provide_dht = update::load_provide_dht(&self.config.data_dir).await;
        let source_str = match source {
            update::UpdateSource::Origin => "origin",
            update::UpdateSource::Swarm => "swarm",
        };
        Ok(serde_json::json!({
            "source": source_str,
            // Whether this node seeds/serves blobs to peers (default true).
            "provide_dht": provide_dht,
            // Compiled with the iroh swarm engine? If false, "swarm" mode has no
            // peers and silently behaves like origin.
            "swarm_available": cfg!(feature = "iroh-swarm"),
            // Runtime swarm-assist gate from config (ARCHIPELAGO_SWARM_ENABLED).
            "swarm_enabled": self.config.swarm_enabled,
        }))
    }

    /// Update the node's swarm prefs. Params (both optional, at least one):
    /// `{ source?: "origin" | "swarm", provide?: bool }`.
    pub(super) async fn handle_update_set_source(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let mut touched = false;
        if let Some(s) = params.get("source").and_then(|v| v.as_str()) {
            let source = match s {
                "origin" => update::UpdateSource::Origin,
                "swarm" => update::UpdateSource::Swarm,
                _ => anyhow::bail!("source must be \"origin\" or \"swarm\""),
            };
            update::save_update_source(&self.config.data_dir, source).await?;
            touched = true;
        }
        if let Some(provide) = params.get("provide").and_then(|v| v.as_bool()) {
            update::save_provide_dht(&self.config.data_dir, provide).await?;
            touched = true;
        }
        if !touched {
            anyhow::bail!("expected \"source\" and/or \"provide\"");
        }
        self.handle_update_get_source().await
    }

    /// Add a mirror to the end of the list. Params: `{ url, label? }`.
    /// Duplicates (same URL) are replaced rather than added twice.
    pub(super) async fn handle_update_add_mirror(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing url"))?
            .trim()
            .to_string();
        if !url.starts_with("http://") && !url.starts_with("https://") {
            anyhow::bail!("url must start with http:// or https://");
        }
        let label = params
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let mut list = update::load_mirrors(&self.config.data_dir).await?;
        list.retain(|m| m.url != url);
        list.push(update::UpdateMirror { url, label });
        update::save_mirrors(&self.config.data_dir, &list).await?;
        Ok(serde_json::json!({ "mirrors": list }))
    }

    /// Remove a mirror by URL. Params: `{ url }`.
    pub(super) async fn handle_update_remove_mirror(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing url"))?;
        let mut list = update::load_mirrors(&self.config.data_dir).await?;
        list.retain(|m| m.url != url);
        update::save_mirrors(&self.config.data_dir, &list).await?;
        Ok(serde_json::json!({ "mirrors": list }))
    }

    /// Ping a mirror's manifest URL. Returns reachability, wall-clock
    /// latency, and HTTP status. Params: `{ url }`.
    pub(super) async fn handle_update_test_mirror(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing url"))?;
        let result = update::test_mirror(url).await;
        Ok(serde_json::to_value(result)?)
    }

    /// Move a mirror to the top of the list so it's tried first.
    /// Params: `{ url }`.
    pub(super) async fn handle_update_set_primary_mirror(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let url = params
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing url"))?;
        let mut list = update::load_mirrors(&self.config.data_dir).await?;
        let Some(idx) = list.iter().position(|m| m.url == url) else {
            anyhow::bail!("mirror not in list");
        };
        let entry = list.remove(idx);
        list.insert(0, entry);
        update::save_mirrors(&self.config.data_dir, &list).await?;
        Ok(serde_json::json!({ "mirrors": list }))
    }

    /// Get the current update schedule.
    pub(super) async fn handle_update_get_schedule(&self) -> Result<serde_json::Value> {
        let schedule = update::get_schedule(&self.config.data_dir).await?;
        Ok(serde_json::json!({ "schedule": schedule }))
    }

    /// Set the update schedule. Params: { schedule: "manual" | "daily_check" | "auto_apply" }
    pub(super) async fn handle_update_set_schedule(
        &self,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let schedule_str = params["schedule"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'schedule' parameter"))?;

        let schedule = match schedule_str {
            "manual" => update::UpdateSchedule::Manual,
            "daily_check" => update::UpdateSchedule::DailyCheck,
            "auto_apply" => update::UpdateSchedule::AutoApply,
            _ => anyhow::bail!(
                "Invalid schedule: '{}'. Use manual, daily_check, or auto_apply",
                schedule_str
            ),
        };

        update::set_schedule(&self.config.data_dir, schedule).await?;
        Ok(serde_json::json!({ "schedule": schedule }))
    }
}
