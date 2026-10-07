//! Per-app manual update handler.
//!
//! Flow: validate → set Updating state → graceful stop → pull new image(s) →
//! remove old container(s) → recreate (orchestrator-first, legacy fallback) → verify running.
//! Data volumes are preserved (bind mounts, not stored in container).

use super::config::{all_container_names, get_containers_for_app};
use super::install::install_log;
use super::progress::parse_pull_progress;
use super::validation::validate_app_id;
use crate::api::rpc::RpcHandler;
use crate::container::image_versions;
use crate::data_model::{InstallPhase, PackageState};
use anyhow::{Context, Result};
use std::{collections::HashSet, path::Path};
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{error, info, warn};

const PODMAN_UPDATE_PULL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

impl RpcHandler {
    /// Update a package to the freshly verified catalog target.
    /// This is a manual operation — the user clicks "Update" in the UI.
    pub(in crate::api::rpc) async fn handle_package_update(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let package_id = params
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing package id"))?;
        validate_app_id(package_id)?;
        let lifecycle_guard =
            crate::container::update_transaction::Guard::acquire(&self.config.data_dir)?;
        lifecycle_guard.require_clear()?;

        // An Update click must not act on an hourly cache that predates the
        // button. Fetch and verify first; failure leaves running containers alone.
        crate::container::app_catalog::refresh_catalog(&self.config.data_dir)
            .await
            .context(
                "Cannot check the signed app catalog; update cancelled before changing containers",
            )?;
        if let Some(orch) = &self.orchestrator {
            // Reload even when bytes did not change: a previous reload may have
            // failed after the cache was written, or another refresher wrote it.
            orch.reload_manifests()
                .await
                .context("Cannot load current app manifests; update cancelled")?;
        }

        // Resolve the target image. Prefer the remote app catalog (decoupled
        // from the binary OTA), falling back to the image-versions.sh pin. This
        // is OPTIONAL for orchestrator-managed apps: the orchestrator resolves
        // the image itself (manifest + catalog + version_config pin) in its
        // upgrade path, so an app the catalog doesn't carry a primary image for
        // (e.g. bitcoin-core, image lives in the embedded manifest + versions[])
        // still upgrades. Only the legacy/stack path below hard-requires it.
        let pinned = crate::container::app_catalog::catalog_primary_image(package_id)
            .or_else(|| image_versions::pinned_image_for_app(package_id));

        let targets = pinned
            .as_ref()
            .map(|target| self.resolve_images_to_pull(package_id, target));
        // A stopped Quadlet normally removes its --rm container. Absence is
        // not an install decision: only a known single managed app with durable
        // installed AND stopped evidence may enter the recreate path.
        let known_managed =
            if should_try_orchestrator_update(package_id, self.orchestrator.is_some()) {
                self.orchestrator
                    .as_ref()
                    .expect("orchestrator presence checked")
                    .knows_app(orchestrator_update_app_id(package_id))
                    .await
            } else {
                false
            };
        let markers = UpdateMarkers::load(&self.config.data_dir, package_id).await?;
        let installed = inspect_update_images(package_id).await?;
        validate_update_presence(package_id, known_managed, !installed.is_empty(), &markers)?;
        if let Some(targets) = &targets {
            if !update_targets_need_change(targets, &installed)? && !markers.stopped {
                install_log(&format!(
                    "UPDATE SKIP: {} — target versions already installed",
                    package_id
                ))
                .await;
                self.clear_install_progress(package_id).await;
                return Ok(serde_json::json!({"status": "up-to-date", "package_id": package_id}));
            }
        }

        // Note: the `already updating` guard lives in `spawn_package_update`
        // (the async wrapper that dispatch actually routes to). By the time
        // this inner function runs, the wrapper has already flipped state to
        // `Updating`, so duplicating the check here would be a false positive.

        install_log(&format!(
            "UPDATE: {} → {}",
            package_id,
            pinned.as_deref().unwrap_or("(orchestrator-resolved)")
        ))
        .await;

        // Set state to Updating
        {
            let (mut data, _) = self.state_manager.get_snapshot().await;
            if let Some(entry) = data.package_data.get_mut(package_id) {
                entry.state = PackageState::Updating;
                entry.available_update = None;
            }
            self.state_manager.update_data(data).await;
        }

        // Preferred path: for single-container apps managed by manifests, route
        // updates through the orchestrator's upgrade lifecycle instead of the
        // legacy shell/CLI flow. Keep stack-style packages on legacy for now.
        if should_try_orchestrator_update(package_id, self.orchestrator.is_some()) {
            let orchestrator_app_id = orchestrator_update_app_id(package_id);
            self.set_install_phase(package_id, InstallPhase::Preparing)
                .await;
            install_log(&format!(
                "UPDATE ORCH: {} — attempting orchestrator upgrade as {}",
                package_id, orchestrator_app_id
            ))
            .await;

            if let Some(orchestrator) = self.orchestrator.as_ref() {
                match orchestrator.upgrade(orchestrator_app_id).await {
                    Ok(()) => {
                        if let Some(image) = orchestrator
                            .staged_upgrade_image(orchestrator_app_id)
                            .await?
                        {
                            // The orchestrator proved the pinned image exists and
                            // persisted its reviewed manifest. No running container
                            // or healthy service is claimed for a stopped update.
                            self.clear_install_progress(package_id).await;
                            return Ok(serde_json::json!({
                                "status": "staged", "state": "stopped",
                                "package_id": package_id, "image": image,
                            }));
                        }
                        if let Some(targets) = &targets {
                            verify_update_targets(
                                targets,
                                &inspect_update_images(package_id).await?,
                            )?;
                        }
                        self.set_install_phase(package_id, InstallPhase::WaitingHealthy)
                            .await;
                        if let Ok(health) = orchestrator.health(orchestrator_app_id).await {
                            if health != "healthy" {
                                warn!(
                                    "Update {}: orchestrator upgrade completed with health={} (expected healthy)",
                                    package_id, health
                                );
                            }
                        }
                        install_log(&format!(
                            "UPDATE ORCH OK: {} (app={})",
                            package_id, orchestrator_app_id
                        ))
                        .await;
                        self.clear_install_progress(package_id).await;
                        return Ok(serde_json::json!({
                            "status": "updated",
                            "package_id": package_id,
                        }));
                    }
                    Err(e) if is_unknown_app_id_error(&e) => {
                        info!(
                            "Update {}: orchestrator has no manifest mapping yet, falling back to legacy updater",
                            package_id
                        );
                        install_log(&format!(
                            "UPDATE ORCH SKIP: {} — unknown app_id, using legacy flow",
                            package_id
                        ))
                        .await;
                    }
                    Err(e) => {
                        install_log(&format!("UPDATE ORCH FAIL: {} — {}", package_id, e)).await;
                        self.clear_install_progress(package_id).await;
                        self.clear_update_state(package_id).await;
                        return Err(e.context(format!("Orchestrator update {} failed", package_id)));
                    }
                }
            }
        }

        // Legacy/stack path hard-requires a concrete primary image (the
        // orchestrator path above already returned for apps it manages).
        let pinned = match pinned {
            Some(p) => p,
            None => {
                self.clear_update_state(package_id).await;
                return Err(anyhow::anyhow!("No pinned image found for {}", package_id));
            }
        };

        // Resolve images to pull — either a stack or single container
        let images_to_pull =
            targets.unwrap_or_else(|| self.resolve_images_to_pull(package_id, &pinned));

        // Get all containers for this app
        let containers = get_containers_for_app(package_id).await?;
        if containers.is_empty() {
            self.clear_update_state(package_id).await;
            return Err(anyhow::anyhow!("No containers found for {}", package_id));
        }

        // Resolve every image while the old stack is still available. A
        // registry outage or missing private import must not stop the app or
        // enter rollback (which could start a deliberately stopped member).
        self.set_install_phase(package_id, InstallPhase::PullingImage)
            .await;
        match preflighted_stack_update(
            &images_to_pull,
            |image| async move { self.pull_update_image(package_id, &image).await },
            || self.execute_update(package_id, &containers, &images_to_pull, &lifecycle_guard),
        )
        .await
        {
            Ok(()) => {
                install_log(&format!("UPDATE OK: {}", package_id)).await;
                self.clear_install_progress(package_id).await;
                Ok(serde_json::json!({
                    "status": "updated",
                    "package_id": package_id,
                }))
            }
            Err(UpdateFailure::Preparation(error)) => {
                self.clear_install_progress(package_id).await;
                self.clear_update_state(package_id).await;
                Err(error)
            }
            Err(UpdateFailure::Execution(e)) => {
                error!("Update {} failed: {}. Attempting rollback.", package_id, e);
                install_log(&format!(
                    "UPDATE FAIL: {} — {}. Rolling back.",
                    package_id, e
                ))
                .await;
                // Transaction executor already recovered original identities or
                // returned an explicit unresolved state. Never guess/reinstall.
                self.clear_install_progress(package_id).await;
                self.clear_update_state(package_id).await;
                Err(e.context(format!(
                    "Update {} failed; see retained-runtime recovery result",
                    package_id
                )))
            }
        }
    }

    /// Manual "check for updates": refresh the remote app catalog now. The
    /// package scanner recomputes each app's `available-update` from the fresh
    /// catalog on its next cycle and pushes it to the UI. When the catalog
    /// bytes changed, the orchestrator's manifest overlay is reloaded in the
    /// same call so catalog-shipped manifest fixes apply without a service
    /// restart. Best-effort — a fetch failure leaves the cached catalog in
    /// place and reports `refreshed: false`.
    pub(in crate::api::rpc) async fn handle_package_check_updates(
        &self,
        _params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        match crate::container::app_catalog::refresh_catalog(&self.config.data_dir).await {
            Ok(refresh) => {
                let mut manifests_reloaded = serde_json::Value::Null;
                if refresh.changed {
                    if let Some(orch) = &self.orchestrator {
                        match orch.reload_manifests().await {
                            Ok(n) => manifests_reloaded = serde_json::json!(n),
                            Err(e) => tracing::warn!(
                                "check-updates: manifest reload after catalog change failed: {e}"
                            ),
                        }
                    }
                }
                Ok(serde_json::json!({
                    "status": "ok",
                    "refreshed": true,
                    "catalog_apps": refresh.apps,
                    "catalog_changed": refresh.changed,
                    "manifests_reloaded": manifests_reloaded,
                }))
            }
            Err(e) => Ok(serde_json::json!({
                "status": "ok",
                "refreshed": false,
                "error": e.to_string(),
            })),
        }
    }

    /// Images are prepared first. The transaction preserves original identities,
    /// creates stopped replacements, and restores the exact old states on failure.
    async fn execute_update(
        &self,
        package_id: &str,
        containers: &[String],
        images_to_pull: &[(String, String)],
        guard: &crate::container::update_transaction::Guard,
    ) -> Result<()> {
        use crate::container::update_transaction::{self, Podman};
        let targets = Podman::targets(images_to_pull).await?;
        let names: HashSet<_> = containers.iter().map(String::as_str).collect();
        anyhow::ensure!(
            targets.len() == names.len()
                && targets
                    .iter()
                    .all(|target| names.contains(target.name.as_str())),
            "Update target membership differs from installed stack; no containers changed"
        );
        self.set_install_phase(package_id, InstallPhase::Preparing)
            .await;
        update_transaction::execute(guard, package_id, &targets, &Podman).await
    }

    async fn recreate_container_for_update(
        &self,
        package_id: &str,
        container_name: &str,
    ) -> Result<()> {
        let Some(orchestrator) = self.orchestrator.as_ref() else {
            return Err(anyhow::anyhow!(
                "Cannot recreate {} during update {}: orchestrator unavailable",
                container_name,
                package_id
            ));
        };

        let mut attempted = Vec::new();
        for app_id in candidate_app_ids_for_container(container_name) {
            attempted.push(app_id.clone());
            match orchestrator.install(&app_id).await {
                Ok(created_name) => {
                    install_log(&format!(
                        "UPDATE ORCH RECREATE OK: {} — container={} app_id={} created={}",
                        package_id, container_name, app_id, created_name
                    ))
                    .await;
                    return Ok(());
                }
                Err(e) if is_unknown_app_id_error(&e) => {
                    continue;
                }
                Err(e) => {
                    return Err(e.context(format!(
                        "orchestrator recreate failed for update {} (container={}, app_id={})",
                        package_id, container_name, app_id
                    )));
                }
            }
        }

        Err(anyhow::anyhow!(
            "No manifest mapping found while recreating {} during update {} (attempted app_ids: {})",
            container_name,
            package_id,
            attempted.join(", ")
        ))
    }

    /// Pull a single image with progress broadcasting (reuses install progress pattern).
    async fn pull_update_image(&self, package_id: &str, image: &str) -> Result<()> {
        self.set_install_progress(package_id, 0, 0).await;

        if immutable_update_image(image) {
            // A digest-addressed lookup asks Podman for these exact bytes,
            // unlike a mutable tag lookup. Private imports need no registry.
            let local = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                tokio::process::Command::new("podman")
                    .args(["image", "exists", image])
                    .kill_on_drop(true)
                    .output(),
            )
            .await
            .context("Local image lookup timed out; existing app remains unchanged")??;
            match local.status.code() {
                Some(0) => {
                    self.set_install_progress(package_id, 100, 100).await;
                    return Ok(());
                }
                Some(1) => {}
                _ => anyhow::bail!("Cannot inspect local image storage; update cancelled"),
            }
        }
        anyhow::ensure!(
            !image.starts_with("localhost/"),
            "The exact private image must be imported before updating this app"
        );

        let mut cmd = tokio::process::Command::new("podman");
        cmd.arg("pull");
        if archipelago_container::image_uses_insecure_registry(image) {
            cmd.arg("--tls-verify=false");
        }
        cmd.kill_on_drop(true);
        let mut child = cmd
            .arg(image)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("Failed to start image pull")?;

        let progress_task = if let Some(stderr) = child.stderr.take() {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            let pkg_id = package_id.to_string();
            let state_mgr = self.state_manager.clone();

            Some(tokio::spawn(async move {
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Some((downloaded, total)) = parse_pull_progress(&line) {
                        Self::update_install_progress(&state_mgr, &pkg_id, downloaded, total).await;
                    }
                }
            }))
        } else {
            None
        };

        let status = match tokio::time::timeout(PODMAN_UPDATE_PULL_TIMEOUT, child.wait()).await {
            Ok(result) => result.context("Failed to wait for image pull")?,
            Err(_) => {
                let _ = child.kill().await;
                return Err(anyhow::anyhow!(
                    "podman pull {} timed out after {}s",
                    image,
                    PODMAN_UPDATE_PULL_TIMEOUT.as_secs()
                ));
            }
        };

        if let Some(task) = progress_task {
            let _ = task.await;
        }
        if !status.success() {
            return Err(anyhow::anyhow!("podman pull {} failed", image));
        }

        self.set_install_progress(package_id, 100, 100).await;
        Ok(())
    }

    /// Determine which images need to be pulled for this update.
    /// For multi-container stacks, pulls all component images.
    /// For single-container apps, pulls just the pinned image.
    fn resolve_images_to_pull(
        &self,
        package_id: &str,
        pinned_primary: &str,
    ) -> Vec<(String, String)> {
        let mut stack_images = image_versions::pinned_images_for_stack(package_id);
        if stack_images.is_empty() {
            // Single container app — pinned_primary already prefers the catalog.
            return vec![(package_id.to_string(), pinned_primary.to_string())];
        }
        // Stack app: override per-container images with the catalog where it
        // provides them; components the catalog omits keep the image-versions.sh
        // pin. This lets a single component (e.g. the IndeeHub frontend) be
        // bumped without touching the rest of the stack.
        let catalog_images = crate::container::app_catalog::catalog_stack_images(package_id);
        if !catalog_images.is_empty() {
            for (name, image) in stack_images.iter_mut() {
                if let Some(catalog_image) = catalog_images.get(name) {
                    *image = catalog_image.clone();
                }
            }
        }
        stack_images
    }

    /// Clear the Updating state (used on failure/rollback).
    async fn clear_update_state(&self, package_id: &str) {
        let (mut data, _) = self.state_manager.get_snapshot().await;
        if let Some(entry) = data.package_data.get_mut(package_id) {
            // Don't overwrite state from scanner — just clear if still Updating
            if entry.state == PackageState::Updating {
                entry.state = PackageState::Stopped;
            }
        }
        self.state_manager.update_data(data).await;
    }
}

#[derive(Default)]
struct UpdateMarkers {
    installed: bool,
    stopped: bool,
    uninstalled: bool,
}

impl UpdateMarkers {
    async fn load(data_dir: &Path, package_id: &str) -> Result<Self> {
        let mut names = all_container_names(package_id);
        names.push(package_id.to_string());
        names.push(orchestrator_update_app_id(package_id).to_string());
        let installed = read_update_markers(data_dir, "installed-apps.json").await?;
        let stopped = read_update_markers(data_dir, "user-stopped.json").await?;
        let uninstalled = read_update_markers(data_dir, "user-uninstalled.json").await?;
        Ok(Self {
            installed: names.iter().any(|name| installed.contains(name)),
            stopped: names.iter().any(|name| stopped.contains(name)),
            uninstalled: names.iter().any(|name| uninstalled.contains(name)),
        })
    }
}

// The recovery loaders intentionally return empty on malformed state. An update
// must instead distinguish unavailable evidence from a proven absence of an
// uninstall decision before it can recreate a removed Quadlet container.
async fn read_update_markers(data_dir: &Path, filename: &str) -> Result<HashSet<String>> {
    match tokio::fs::read(data_dir.join(filename)).await {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("Cannot read {filename}; update cancelled")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HashSet::new()),
        Err(error) => {
            Err(error).with_context(|| format!("Cannot read {filename}; update cancelled"))
        }
    }
}

fn validate_update_presence(
    package_id: &str,
    known_managed: bool,
    has_containers: bool,
    markers: &UpdateMarkers,
) -> Result<()> {
    anyhow::ensure!(
        !markers.uninstalled,
        "{} was uninstalled; use an explicit install instead of update",
        package_id
    );
    anyhow::ensure!(
        has_containers || (known_managed && markers.installed && markers.stopped),
        "No containers or durable stopped installation found for {}",
        package_id
    );
    Ok(())
}

async fn inspect_update_images(package_id: &str) -> Result<Vec<(String, String)>> {
    let containers = get_containers_for_app(package_id).await?;
    if containers.is_empty() {
        // The caller decides whether durable installation evidence authorizes
        // a missing runtime. Never invoke bare `podman inspect` here.
        return Ok(Vec::new());
    }
    let mut command = tokio::process::Command::new("podman");
    command.arg("inspect").args(&containers).kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(30), command.output())
        .await
        .context("Timed out checking installed images")??;
    anyhow::ensure!(
        output.status.success(),
        "Cannot inspect installed images; update cancelled"
    );
    let inspected: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)?;
    inspected
        .iter()
        .map(|entry| {
            let name = entry
                .get("Name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Container inspection omitted Name"))?;
            let image = entry
                .get("ImageName")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Container inspection omitted ImageName"))?;
            Ok((name.trim_start_matches('/').to_string(), image.to_string()))
        })
        .collect()
}

fn installed_image_for_target<'a>(
    app_id: &str,
    installed: &'a [(String, String)],
) -> Option<&'a str> {
    installed
        .iter()
        .find(|(name, _)| {
            candidate_app_ids_for_container(name)
                .iter()
                .any(|id| id == app_id)
        })
        .map(|(_, image)| image.as_str())
}

/// A successful recreate is not proof that it used the downloaded image.
fn verify_update_targets(
    targets: &[(String, String)],
    installed: &[(String, String)],
) -> Result<()> {
    anyhow::ensure!(!targets.is_empty(), "No update targets resolved");
    for (app_id, target) in targets {
        let running = installed_image_for_target(app_id, installed).ok_or_else(|| {
            anyhow::anyhow!("Update {}: target container missing after recreate", app_id)
        })?;
        anyhow::ensure!(
            image_versions::extract_version_from_image(target)
                == image_versions::extract_version_from_image(running)
                || image_versions::compare_image_versions(target, running)
                    == Some(std::cmp::Ordering::Equal),
            "Update {}: recreated container did not reach target version {}",
            app_id,
            image_versions::extract_version_from_image(target)
        );
    }
    Ok(())
}

/// Check every stack component, not just the version shown on its tile. A
/// newer backend must still update when its frontend version is unchanged.
/// A stale target for any component cancels before pulling or stopping anything.
fn update_targets_need_change(
    targets: &[(String, String)],
    installed: &[(String, String)],
) -> Result<bool> {
    use std::cmp::Ordering;
    anyhow::ensure!(!targets.is_empty(), "No update targets resolved");
    let mut changed = false;
    for (app_id, target) in targets {
        let running = installed_image_for_target(app_id, installed);
        match running.and_then(|image| image_versions::compare_image_versions(target, image)) {
            Some(Ordering::Less) => anyhow::bail!(
                "Catalog target for {} is older than the installed image; refusing downgrade",
                app_id
            ),
            Some(Ordering::Equal) => {}
            Some(Ordering::Greater) | None => changed = true,
        }
    }
    Ok(changed)
}

fn should_try_orchestrator_update(package_id: &str, orchestrator_available: bool) -> bool {
    orchestrator_available && !uses_legacy_update_flow(package_id)
}

fn immutable_update_image(image: &str) -> bool {
    image.rsplit_once("@sha256:").is_some_and(|(name, digest)| {
        !name.is_empty()
            && !name.contains('@')
            && digest.len() == 64
            && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

#[derive(Debug)]
enum UpdateFailure {
    Preparation(anyhow::Error),
    Execution(anyhow::Error),
}

/// A preparation failure never enters the lifecycle/rollback path. Keep this
/// sequencing injectable so failed second-image pulls are tested without apps.
async fn preflighted_stack_update<P, PF, E, EF>(
    images: &[(String, String)],
    mut prepare: P,
    execute: E,
) -> std::result::Result<(), UpdateFailure>
where
    P: FnMut(String) -> PF,
    PF: std::future::Future<Output = Result<()>>,
    E: FnOnce() -> EF,
    EF: std::future::Future<Output = Result<()>>,
{
    for (name, image) in images {
        prepare(image.clone()).await.map_err(|error| {
            UpdateFailure::Preparation(error.context(format!(
                "Cannot prepare image for {name}; existing containers were left unchanged"
            )))
        })?;
    }
    execute().await.map_err(UpdateFailure::Execution)
}

fn orchestrator_update_app_id(package_id: &str) -> &str {
    match package_id {
        "electrs" | "mempool-electrs" => "electrumx",
        _ => package_id,
    }
}

fn uses_legacy_update_flow(package_id: &str) -> bool {
    // A primary container already at its target does not mean its backend or
    // database is current. Route every mapped stack through the component flow.
    !image_versions::containers_for_stack(package_id).is_empty()
        || matches!(
            package_id,
            // Multi-container stacks still updated via the stack-aware path.
            "immich" | "penpot" | "penpot-frontend" | "indeedhub"
        )
}

fn is_unknown_app_id_error(err: &anyhow::Error) -> bool {
    err.chain()
        .any(|cause| cause.to_string().contains("unknown app_id"))
}

fn candidate_app_ids_for_container(container_name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |s: &str| {
        if !out.iter().any(|e: &String| e == s) {
            out.push(s.to_string());
        }
    };

    match container_name {
        "bitcoin-knots" | "bitcoin-core" => {
            push("bitcoin-knots");
            push("bitcoin-core");
        }
        "archy-bitcoin-ui" => push("bitcoin-ui"),
        "archy-lnd-ui" => push("lnd-ui"),
        "archy-electrs-ui" => push("electrs-ui"),
        "mysql-mempool" => push("archy-mempool-db"),
        "btcpay" | "btcpayserver" | "archy-btcpay" => push("btcpay-server"),
        "homeassistant" | "archy-homeassistant" => push("home-assistant"),
        "fedimintd" => push("fedimint"),
        "electrs" | "mempool-electrs" => push("electrumx"),
        "mempool" | "mempool-web" => {
            push("archy-mempool-web");
            push("mempool");
        }
        _ => {}
    }

    push(container_name);
    if let Some(stripped) = container_name.strip_prefix("archy-") {
        push(stripped);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        candidate_app_ids_for_container, immutable_update_image, orchestrator_update_app_id,
        should_try_orchestrator_update, update_targets_need_change, uses_legacy_update_flow,
        validate_update_presence, verify_update_targets, UpdateMarkers,
    };

    #[tokio::test]
    async fn stopped_installed_managed_app_updates_after_quadlet_container_disappears() {
        let root = tempfile::tempdir().unwrap();
        crate::crash_recovery::mark_installed(root.path(), "node-demo-music").await;
        crate::crash_recovery::mark_user_stopped(root.path(), "node-demo-music").await;
        let markers = UpdateMarkers::load(root.path(), "node-demo-music")
            .await
            .unwrap();
        validate_update_presence("node-demo-music", true, false, &markers).unwrap();
        let target = vec![("node-demo-music".into(), "localhost/music:2".into())];
        assert!(super::update_targets_need_change(&target, &[]).unwrap());
        // Success must still prove that upgrade actually created the target.
        assert!(verify_update_targets(&target, &[]).is_err());
        assert!(verify_update_targets(&target, &target).is_ok());
        assert!(crate::crash_recovery::load_user_stopped(root.path())
            .await
            .contains("node-demo-music"));
    }

    #[test]
    fn absent_catalog_app_or_unmanaged_runtime_is_not_an_update_installation() {
        for (managed, installed, stopped) in [
            (true, false, false),
            (true, false, true),
            (true, true, false),
            (false, true, true),
        ] {
            let markers = UpdateMarkers {
                installed,
                stopped,
                uninstalled: false,
            };
            assert!(validate_update_presence("optional", managed, false, &markers).is_err());
        }
        // Existing legacy containers remain updateable without modern markers.
        assert!(validate_update_presence("legacy", false, true, &UpdateMarkers::default()).is_ok());
        assert!(super::update_targets_need_change(&[], &[]).is_err());
        assert!(verify_update_targets(&[], &[]).is_err());
    }

    #[tokio::test]
    async fn uninstall_tombstone_beats_stale_installed_and_stopped_markers() {
        let root = tempfile::tempdir().unwrap();
        crate::crash_recovery::mark_installed(root.path(), "node-demo-music").await;
        crate::crash_recovery::mark_user_stopped(root.path(), "node-demo-music").await;
        crate::crash_recovery::mark_user_uninstalled(root.path(), "archy-node-demo-music").await;
        let markers = UpdateMarkers::load(root.path(), "node-demo-music")
            .await
            .unwrap();
        for present in [false, true] {
            assert!(validate_update_presence("node-demo-music", true, present, &markers).is_err());
        }
    }

    #[tokio::test]
    async fn damaged_lifecycle_markers_cannot_authorize_recreation() {
        let root = tempfile::tempdir().unwrap();
        let absent = UpdateMarkers::load(root.path(), "optional").await.unwrap();
        assert!(validate_update_presence("optional", true, false, &absent).is_err());
        for name in [
            "installed-apps.json",
            "user-stopped.json",
            "user-uninstalled.json",
        ] {
            tokio::fs::write(root.path().join(name), b"not-json")
                .await
                .unwrap();
            assert!(UpdateMarkers::load(root.path(), "optional").await.is_err());
            tokio::fs::remove_file(root.path().join(name))
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn stack_image_failure_precedes_every_lifecycle_action() {
        use std::sync::{Arc, Mutex};
        for failed_second in [true, false] {
            let calls = Arc::new(Mutex::new(Vec::new()));
            let preparing = calls.clone();
            let executing = calls.clone();
            let images = vec![
                ("web".into(), "web-image".into()),
                ("api".into(), "api-image".into()),
            ];
            let result = super::preflighted_stack_update(
                &images,
                move |image| {
                    let preparing = preparing.clone();
                    async move {
                        preparing.lock().unwrap().push(format!("prepare:{image}"));
                        anyhow::ensure!(!(failed_second && image == "api-image"), "import missing");
                        Ok(())
                    }
                },
                move || async move {
                    executing.lock().unwrap().extend([
                        "stop".into(),
                        "remove".into(),
                        "start".into(),
                    ]);
                    Ok(())
                },
            )
            .await;
            if failed_second {
                assert!(matches!(result, Err(super::UpdateFailure::Preparation(_))));
                assert_eq!(
                    *calls.lock().unwrap(),
                    ["prepare:web-image", "prepare:api-image"]
                );
            } else {
                assert!(result.is_ok());
                assert_eq!(
                    *calls.lock().unwrap(),
                    [
                        "prepare:web-image",
                        "prepare:api-image",
                        "stop",
                        "remove",
                        "start"
                    ]
                );
            }
        }
    }

    #[test]
    fn only_exact_digest_refs_may_skip_update_registry_pull() {
        let digest = "ab".repeat(32);
        assert!(immutable_update_image(&format!(
            "localhost/lfg2025/indeedhub:1.0.1@sha256:{digest}"
        )));
        assert!(immutable_update_image(&format!(
            "registry.example/app@sha256:{digest}"
        )));
        for mutable_or_invalid in [
            "localhost/lfg2025/indeedhub:1.0.1".to_string(),
            "registry.example/app:latest".to_string(),
            format!("registry.example/app@sha256:{}", "g".repeat(64)),
            "registry.example/app@sha256:abcd".to_string(),
            format!("@sha256:{digest}"),
            format!("registry.example/app@other@sha256:{digest}"),
        ] {
            assert!(!immutable_update_image(&mutable_or_invalid));
        }
    }

    #[test]
    fn mempool_update_preflight_rejects_stale_catalog_without_reinstalling() {
        let installed = vec![(
            "mempool".into(),
            "r.test/lfg2025/mempool-frontend:v3.3.1-archy1".into(),
        )];
        let stale = vec![(
            "archy-mempool-web".into(),
            "r.test/lfg2025/mempool-frontend:v3.3.1".into(),
        )];
        assert!(update_targets_need_change(&stale, &installed).is_err());
        let current = vec![(
            "archy-mempool-web".into(),
            "r.test/chaum/mempool-frontend:v3.3.1-archy1".into(),
        )];
        assert!(!update_targets_need_change(&current, &installed).unwrap());
        let legacy = vec![(
            "mempool-web".into(),
            "r.test/old/mempool-frontend:v3.3.1-archy1".into(),
        )];
        assert!(!update_targets_need_change(&current, &legacy).unwrap());
        let newer = vec![(
            "archy-mempool-web".into(),
            "r.test/chaum/mempool-frontend:v3.3.1-archy2".into(),
        )];
        assert!(update_targets_need_change(&newer, &installed).unwrap());
    }

    #[test]
    fn stack_update_checks_backend_even_when_frontend_matches() {
        let installed = vec![
            ("mempool".into(), "r.test/team/web:3.3.1-archy1".into()),
            ("mempool-api".into(), "r.test/team/api:3.3.1".into()),
        ];
        let mut targets = vec![
            (
                "archy-mempool-web".into(),
                "r.test/team/web:3.3.1-archy1".into(),
            ),
            ("mempool-api".into(), "r.test/team/api:3.3.2".into()),
        ];
        assert!(update_targets_need_change(&targets, &installed).unwrap());
        targets[0].1 = "r.test/team/web:3.3.1".into();
        assert!(update_targets_need_change(&targets, &installed).is_err());
    }

    #[test]
    fn update_completion_requires_the_target_version_to_be_installed() {
        let targets = vec![(
            "archy-mempool-web".into(),
            "r.test/chaum/mempool-frontend:v3.3.1-archy1".into(),
        )];
        let mut installed = vec![(
            "mempool".into(),
            "r.test/lfg2025/mempool-frontend:v3.3.1".into(),
        )];
        assert!(verify_update_targets(&targets, &installed).is_err());
        assert!(verify_update_targets(&targets, &[]).is_err());
        installed[0].1 = "r.test/lfg2025/mempool-frontend:v3.3.1-archy1".into();
        assert!(verify_update_targets(&targets, &installed).is_ok());
    }

    #[test]
    fn legacy_flow_for_stack_apps() {
        for app in [
            "immich",
            "penpot",
            "indeedhub",
            "mempool",
            "btcpay-server",
            "netbird",
        ] {
            assert!(uses_legacy_update_flow(app), "{app} should stay legacy");
        }
    }

    #[test]
    fn orchestrator_flow_for_single_apps() {
        for app in ["lnd", "bitcoin-core", "searxng", "grafana", "fedimint"] {
            assert!(
                !uses_legacy_update_flow(app),
                "{app} should be orchestrator-first"
            );
            assert!(
                should_try_orchestrator_update(app, true),
                "{app} should use orchestrator when available"
            );
        }
    }

    #[test]
    fn no_orchestrator_means_no_orchestrator_flow() {
        assert!(!should_try_orchestrator_update("lnd", false));
        assert!(!should_try_orchestrator_update("btcpay-server", false));
    }

    #[test]
    fn container_name_candidates_cover_common_aliases() {
        assert_eq!(
            candidate_app_ids_for_container("bitcoin-knots"),
            vec!["bitcoin-knots", "bitcoin-core"]
        );
        assert_eq!(
            candidate_app_ids_for_container("archy-bitcoin-ui"),
            vec!["bitcoin-ui", "archy-bitcoin-ui"]
        );
        assert_eq!(
            candidate_app_ids_for_container("mempool"),
            vec!["archy-mempool-web", "mempool"]
        );
        assert_eq!(
            candidate_app_ids_for_container("archy-mempool-db"),
            vec!["archy-mempool-db", "mempool-db"]
        );
    }

    #[test]
    fn update_aliases_map_to_manifest_app_ids() {
        assert_eq!(orchestrator_update_app_id("bitcoin-knots"), "bitcoin-knots");
        assert_eq!(orchestrator_update_app_id("bitcoin-core"), "bitcoin-core");
        assert_eq!(orchestrator_update_app_id("electrs"), "electrumx");
        assert_eq!(orchestrator_update_app_id("mempool-electrs"), "electrumx");
        assert_eq!(orchestrator_update_app_id("fedimint"), "fedimint");
    }
}
