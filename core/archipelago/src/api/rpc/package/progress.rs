//! Install progress tracking and podman pull output parsing.

use crate::api::rpc::RpcHandler;
use crate::data_model::{
    Description, InstallPhase, InstallProgress, Manifest, PackageDataEntry, PackageState,
    StaticFiles,
};

impl RpcHandler {
    /// Set install progress for a package and broadcast the update.
    /// Creates a minimal package entry if one doesn't exist yet.
    ///
    /// Prefer `set_install_phase` — this byte-counter API is kept for
    /// the rare case where the pull stream actually parses, but podman
    /// almost never emits parseable progress on a piped stderr.
    pub(super) async fn set_install_progress(&self, package_id: &str, downloaded: u64, size: u64) {
        self.state_manager
            .mutate_data(|data| {
                let entry = data
                    .package_data
                    .entry(package_id.to_string())
                    .or_insert_with(|| create_installing_entry(package_id));
                // Image pulls also report byte progress during updates, including
                // the local immutable-image shortcut. Preserve the RPC owner so
                // update failure cleanup can release its transitional state.
                if entry.state != PackageState::Updating {
                    entry.ui_ready = Some(false);
                    entry.state = PackageState::Installing;
                }
                let existing_phase = entry.install_progress.as_ref().and_then(|p| p.phase);
                entry.install_progress = Some(InstallProgress {
                    size,
                    downloaded,
                    phase: existing_phase,
                    message: None,
                });
            })
            .await;
    }

    /// Set the install pipeline phase and broadcast. This is the
    /// primary progress signal — the UI maps each phase to a
    /// percentage and a user-facing label. Byte counters are retained
    /// for the rare case podman emits parseable progress.
    pub(super) async fn set_install_phase(&self, package_id: &str, phase: InstallPhase) {
        self.state_manager
            .mutate_data(|data| {
                let entry = data
                    .package_data
                    .entry(package_id.to_string())
                    .or_insert_with(|| create_installing_entry(package_id));
                // Preparing / PullingImage / CreatingContainer / StartingContainer /
                // WaitingHealthy / PostInstall all map to the Installing state.
                // Updates use Updating state — the wrapper has already flipped
                // state to Updating, so don't clobber it.
                if entry.state != PackageState::Updating {
                    entry.ui_ready = Some(false);
                    entry.state = PackageState::Installing;
                }
                let (size, downloaded) = entry
                    .install_progress
                    .as_ref()
                    .map(|p| (p.size, p.downloaded))
                    .unwrap_or((0, 0));
                entry.install_progress = Some(InstallProgress {
                    size,
                    downloaded,
                    phase: Some(phase),
                    message: None,
                });
            })
            .await;
    }

    /// Set a user-facing install status message (e.g. "Waiting for Bitcoin
    /// to start…") without disturbing the current phase/byte counters.
    pub(super) async fn set_install_message(&self, package_id: &str, message: &str) {
        self.state_manager
            .mutate_data(|data| {
                let entry = data
                    .package_data
                    .entry(package_id.to_string())
                    .or_insert_with(|| create_installing_entry(package_id));
                if entry.state != PackageState::Updating {
                    entry.ui_ready = Some(false);
                    entry.state = PackageState::Installing;
                }
                let (size, downloaded, phase) = entry
                    .install_progress
                    .as_ref()
                    .map(|p| (p.size, p.downloaded, p.phase))
                    .unwrap_or((0, 0, None));
                entry.install_progress = Some(InstallProgress {
                    size,
                    downloaded,
                    phase,
                    message: Some(message.to_string()),
                });
            })
            .await;
    }

    /// Clear install progress after pull completes or fails.
    pub(super) async fn clear_install_progress(&self, package_id: &str) {
        self.state_manager
            .mutate_data(|data| {
                if let Some(entry) = data.package_data.get_mut(package_id) {
                    entry.install_progress = None;
                }
            })
            .await;
    }

    /// Set the uninstall stage label so the UI can show what's happening
    /// instead of a generic spinner. Each call broadcasts a state change
    /// — call sparingly (one per pipeline phase, not per container).
    pub(super) async fn set_uninstall_stage(&self, package_id: &str, stage: &str) {
        self.state_manager
            .mutate_data(|data| {
                if let Some(entry) = data.package_data.get_mut(package_id) {
                    entry.uninstall_stage = Some(stage.to_string());
                    entry.state = crate::data_model::PackageState::Removing;
                }
            })
            .await;
    }

    /// Update install progress (static method for use in async closures).
    pub(super) async fn update_install_progress(
        state_manager: &crate::state::StateManager,
        package_id: &str,
        downloaded: u64,
        total: u64,
    ) {
        state_manager
            .mutate_data(|data| {
                let entry = data
                    .package_data
                    .entry(package_id.to_string())
                    .or_insert_with(|| create_installing_entry(package_id));
                let existing_phase = entry.install_progress.as_ref().and_then(|p| p.phase);
                entry.install_progress = Some(InstallProgress {
                    size: total,
                    downloaded,
                    phase: existing_phase,
                    message: None,
                });
            })
            .await;
    }
}

/// Seed an Installing entry from its manifest before any container exists.
pub(super) fn create_installing_entry(package_id: &str) -> PackageDataEntry {
    let mut entry = PackageDataEntry {
        ui_ready: None,
        state: PackageState::Installing,
        health: None,
        exit_code: None,
        static_files: StaticFiles {
            license: String::new(),
            instructions: String::new(),
            // Filled below from the manifest, without guessing the extension.
            // Unmanifested apps can still use the frontend catalog fallback.
            icon: String::new(),
        },
        manifest: Manifest {
            id: package_id.to_string(),
            title: package_id.to_string(),
            version: String::new(),
            description: Description {
                short: "Installing...".to_string(),
                long: String::new(),
            },
            release_notes: String::new(),
            license: String::new(),
            wrapper_repo: String::new(),
            upstream_repo: String::new(),
            support_site: String::new(),
            marketing_site: String::new(),
            donation_url: None,
            author: None,
            website: None,
            interfaces: None,
            tier: None,
        },
        installed: None,
        install_progress: None,
        uninstall_stage: None,
        available_update: None,
    };
    crate::container::docker_packages::apply_manifest_presentation(package_id, &mut entry);
    entry
}

/// Parse podman pull progress output.
/// Podman outputs lines like: "Copying blob sha256:abc done  |  50.0MiB / 100.0MiB"
/// Returns (downloaded_bytes, total_bytes) if parseable.
pub(super) fn parse_pull_progress(line: &str) -> Option<(u64, u64)> {
    let line = line.trim();
    let parts: Vec<&str> = line.split('/').collect();
    if parts.len() != 2 {
        return None;
    }

    let downloaded = parse_size_value(parts[0].trim())?;
    let total = parse_size_value(parts[1].trim())?;

    if total > 0 {
        Some((downloaded, total))
    } else {
        None
    }
}

/// Parse a size value like "50.0MiB", "1.2GiB", "500KiB" into bytes.
fn parse_size_value(s: &str) -> Option<u64> {
    let s = s.trim();

    let (num_str, multiplier) = if let Some(pos) = s.rfind("GiB") {
        (s[..pos].split_whitespace().last()?, 1024 * 1024 * 1024)
    } else if let Some(pos) = s.rfind("MiB") {
        (s[..pos].split_whitespace().last()?, 1024 * 1024)
    } else if let Some(pos) = s.rfind("KiB") {
        (s[..pos].split_whitespace().last()?, 1024)
    } else if let Some(pos) = s.rfind("GB") {
        (s[..pos].split_whitespace().last()?, 1_000_000_000)
    } else if let Some(pos) = s.rfind("MB") {
        (s[..pos].split_whitespace().last()?, 1_000_000)
    } else if let Some(pos) = s.rfind("KB") {
        (s[..pos].split_whitespace().last()?, 1_000)
    } else if let Some(pos) = s.rfind('B') {
        (s[..pos].split_whitespace().last()?, 1)
    } else {
        return None;
    };

    let num: f64 = num_str.parse().ok()?;
    Some((num * multiplier as f64) as u64)
}
