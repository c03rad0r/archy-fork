use anyhow::{Context, Result};
use archipelago_container::{
    AppManifest, BitcoinSimulationMode, BitcoinSimulator,
    ContainerRuntime as ContainerRuntimeTrait, ContainerStatus, PortManager, ResolvedSource,
};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::config::{BitcoinSimulation, Config, ContainerRuntime};
use crate::container::data_manager::DevDataManager;
use crate::container::traits::ContainerOrchestrator;

pub struct DevContainerOrchestrator {
    runtime: Arc<dyn ContainerRuntimeTrait>,
    port_manager: Arc<PortManager>,
    bitcoin_simulator: Arc<BitcoinSimulator>,
    data_manager: Arc<DevDataManager>,
    config: Config,
}

impl DevContainerOrchestrator {
    pub async fn new(config: Config) -> Result<Self> {
        let user = std::env::var("USER").unwrap_or_else(|_| "archipelago".to_string());

        // Create runtime based on config
        let runtime: Arc<dyn ContainerRuntimeTrait> = match &config.container_runtime {
            ContainerRuntime::Podman => {
                Arc::new(archipelago_container::PodmanRuntime::new(user.clone()))
            }
            ContainerRuntime::Docker => {
                Arc::new(archipelago_container::DockerRuntime::new(user.clone()))
            }
            ContainerRuntime::Auto => Arc::new(
                archipelago_container::AutoRuntime::new(user.clone())
                    .await
                    .context("Failed to create auto runtime")?,
            ),
        };

        let port_manager = Arc::new(PortManager::new(config.port_offset));
        let bitcoin_simulator = Arc::new(BitcoinSimulator::new(BitcoinSimulationMode::from(
            match &config.bitcoin_simulation {
                BitcoinSimulation::Mock => "mock",
                BitcoinSimulation::Testnet => "testnet",
                BitcoinSimulation::Mainnet => "mainnet",
                BitcoinSimulation::None => "none",
            },
        )));
        let data_manager = Arc::new(DevDataManager::new(config.dev_data_dir.clone()));

        Ok(Self {
            runtime,
            port_manager,
            bitcoin_simulator,
            data_manager,
            config,
        })
    }

    /// Install a container from a manifest
    pub async fn install_container(
        &self,
        manifest: &AppManifest,
        _manifest_path: &str,
    ) -> Result<String> {
        let app_id = &manifest.app.id;
        let container_name = format!("archipelago-{}-dev", app_id);

        // Check dependencies
        if self.config.dev_mode {
            // In dev mode, check if Bitcoin dependency can be satisfied
            for dep in &manifest.app.dependencies {
                if let archipelago_container::Dependency::App {
                    app_id: dep_id,
                    version: _,
                } = dep
                {
                    if dep_id == "bitcoin-core" && !self.bitcoin_simulator.is_bitcoin_available() {
                        return Err(anyhow::anyhow!(
                            "Bitcoin Core dependency not satisfied (simulation: {:?})",
                            self.bitcoin_simulator.mode()
                        ));
                    }
                }
            }
        }

        // Allocate ports
        let base_ports: Vec<u16> = manifest.app.ports.iter().map(|p| p.host).collect();
        let _dev_ports = self
            .port_manager
            .allocate_ports(app_id, &base_ports)
            .context("Failed to allocate ports")?;

        // Create app data directory
        self.data_manager
            .create_app_data_dir(app_id)
            .await
            .context("Failed to create app data directory")?;

        // Map volumes to dev paths
        let mut dev_manifest = manifest.clone();
        for volume in &mut dev_manifest.app.volumes {
            let dev_path = self.data_manager.map_volume_path(app_id, &volume.source);
            volume.source = dev_path.to_string_lossy().to_string();
        }

        // Resolve pull-or-build. Dev orchestrator currently only supports pull;
        // Build support lands in Step 2 of the rust-orchestrator migration.
        match manifest.app.container.resolve().ok_or_else(|| {
            anyhow::anyhow!("manifest container config invalid (neither image nor build)")
        })? {
            ResolvedSource::Pull {
                image,
                image_signature,
                ..
            } => {
                self.runtime
                    .pull_image(&image, image_signature.as_deref())
                    .await
                    .context("Failed to pull image")?;
            }
            ResolvedSource::Build(_) => {
                anyhow::bail!(
                    "dev orchestrator does not yet support local image builds (see rust-orchestrator-migration.md Step 2)"
                );
            }
        }

        // Create container with port offset
        let port_offset = if self.config.dev_mode {
            self.config.port_offset
        } else {
            0
        };

        self.runtime
            .create_container(&dev_manifest, &container_name, port_offset)
            .await
            .context("Failed to create container")?;

        Ok(container_name)
    }

    /// Start a container
    pub async fn start_container(&self, app_id: &str) -> Result<()> {
        let container_name = format!("archipelago-{}-dev", app_id);
        self.runtime
            .start_container(&container_name)
            .await
            .context("Failed to start container")?;
        Ok(())
    }

    /// Stop a container
    pub async fn stop_container(&self, app_id: &str) -> Result<()> {
        let container_name = format!("archipelago-{}-dev", app_id);
        self.runtime
            .stop_container(&container_name)
            .await
            .context("Failed to stop container")?;
        Ok(())
    }

    /// Remove a container
    pub async fn remove_container(&self, app_id: &str, preserve_data: bool) -> Result<()> {
        let container_name = format!("archipelago-{}-dev", app_id);

        // Stop container first
        let _ = self.runtime.stop_container(&container_name).await;

        // Remove container
        self.runtime
            .remove_container(&container_name)
            .await
            .context("Failed to remove container")?;

        // Release ports
        let _ = self.port_manager.release_ports(app_id);

        // Clean up or preserve data
        if preserve_data {
            self.data_manager.preserve_app_data(app_id).await?;
        } else {
            self.data_manager.cleanup_app_data(app_id).await?;
        }

        Ok(())
    }

    /// Get container status with dev port info
    pub async fn get_container_status(&self, app_id: &str) -> Result<ContainerStatus> {
        let container_name = format!("archipelago-{}-dev", app_id);
        let mut status = self
            .runtime
            .get_container_status(&container_name)
            .await
            .context("Failed to get container status")?;

        // Add dev port information
        if let Ok(Some(ports)) = self.port_manager.get_port_mapping(app_id) {
            status.ports = ports.iter().map(|p| p.to_string()).collect();
        }

        Ok(status)
    }

    /// List all containers with dev info
    pub async fn list_containers(&self) -> Result<Vec<ContainerStatus>> {
        let containers = self
            .runtime
            .list_containers()
            .await
            .context("Failed to list containers")?;

        // Filter to only archipelago containers and add port info
        let mut result = Vec::new();
        for container in containers {
            if container.name.contains("archipelago-") {
                // Extract app_id from container name
                if let Some(app_id) = container.name.strip_prefix("archipelago-") {
                    if let Some(app_id) = app_id.strip_suffix("-dev") {
                        if let Ok(Some(ports)) = self.port_manager.get_port_mapping(app_id) {
                            let mut container_with_ports = container.clone();
                            container_with_ports.ports =
                                ports.iter().map(|p| p.to_string()).collect();
                            result.push(container_with_ports);
                        } else {
                            result.push(container);
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Get container logs
    pub async fn get_container_logs(&self, app_id: &str, lines: u32) -> Result<Vec<String>> {
        let container_name = format!("archipelago-{}-dev", app_id);
        self.runtime
            .get_container_logs(&container_name, lines)
            .await
            .context("Failed to get container logs")
    }

    /// Get health status
    pub async fn get_health_status(&self, app_id: &str) -> Result<String> {
        let status = self.get_container_status(app_id).await?;
        match status.state {
            archipelago_container::ContainerState::Running => Ok("healthy".to_string()),
            archipelago_container::ContainerState::Stopped
            | archipelago_container::ContainerState::Exited
            | archipelago_container::ContainerState::Stopping => Ok("unhealthy".to_string()),
            archipelago_container::ContainerState::Created => Ok("starting".to_string()),
            archipelago_container::ContainerState::Paused => Ok("paused".to_string()),
            archipelago_container::ContainerState::Unknown(_) => Ok("unknown".to_string()),
        }
    }

    /// Load a manifest for `app_id` from the dev-mode apps directory.
    ///
    /// Used by the trait-level `install(app_id)` entry point.
    ///
    /// Search order intentionally mirrors production/operator reality:
    /// 1) `$ARCHIPELAGO_APPS_DIR` (explicit override)
    /// 2) `/opt/archipelago/apps` (image-recipe canonical path)
    /// 3) `/home/archipelago/Projects/archy/apps` (repo-local fallback on dev nodes)
    /// 4) `<data_dir>/apps` (legacy dev layout)
    async fn load_manifest_for(&self, app_id: &str) -> Result<AppManifest> {
        let candidates = candidate_manifest_paths(app_id, &self.config.data_dir);
        let mut last_err: Option<anyhow::Error> = None;

        for path in candidates {
            let content = match tokio::fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(e) => {
                    last_err = Some(e.into());
                    continue;
                }
            };

            let manifest: AppManifest = serde_yaml::from_str(&content)
                .with_context(|| format!("parsing manifest {}", path.display()))?;
            return Ok(manifest);
        }

        let msg = format!(
            "manifest for {} not found in any search path (set ARCHIPELAGO_APPS_DIR or install /opt/archipelago/apps)",
            app_id
        );
        Err(match last_err {
            Some(e) => e.context(msg),
            None => anyhow::anyhow!(msg),
        })
    }
}

fn candidate_manifest_paths(app_id: &str, data_dir: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    if let Ok(v) = std::env::var("ARCHIPELAGO_APPS_DIR") {
        let v = v.trim();
        if !v.is_empty() {
            roots.push(PathBuf::from(v));
        }
    }

    roots.push(PathBuf::from("/opt/archipelago/apps"));
    roots.push(PathBuf::from("/home/archipelago/Projects/archy/apps"));
    roots.push(data_dir.join("apps"));

    let mut deduped: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !deduped.iter().any(|p| p == &root) {
            deduped.push(root);
        }
    }

    deduped
        .into_iter()
        .map(|root| root.join(app_id).join("manifest.yml"))
        .collect()
}

// ---------------------------------------------------------------------------
// Trait impl (Step 4): expose the shared ContainerOrchestrator surface.
// Forwards to the inherent methods, which internally apply the `-dev` suffix
// and the port offset. The trait keeps the RPC layer mode-agnostic; Dev's
// install_container (manifest_path-based) stays as an inherent method for the
// ad-hoc dev-mode RPC and is not exposed on the trait.
// ---------------------------------------------------------------------------

#[async_trait]
impl ContainerOrchestrator for DevContainerOrchestrator {
    async fn install(&self, app_id: &str) -> Result<String> {
        let manifest = self.load_manifest_for(app_id).await?;
        let name = self.install_container(&manifest, "").await?;
        Ok(name)
    }

    async fn start(&self, app_id: &str) -> Result<()> {
        self.start_container(app_id).await
    }

    async fn stop(&self, app_id: &str) -> Result<()> {
        self.stop_container(app_id).await
    }

    async fn restart(&self, app_id: &str) -> Result<()> {
        let _ = self.stop_container(app_id).await;
        self.start_container(app_id).await
    }

    async fn remove(&self, app_id: &str, preserve_data: bool) -> Result<()> {
        self.remove_container(app_id, preserve_data).await
    }

    async fn upgrade(&self, app_id: &str) -> Result<()> {
        // Dev upgrade: stop, remove (preserving data), re-install from the loaded manifest.
        let _ = self.stop_container(app_id).await;
        let _ = self.remove_container(app_id, true).await;
        let manifest = self.load_manifest_for(app_id).await?;
        self.install_container(&manifest, "").await?;
        self.start_container(app_id).await
    }

    async fn status(&self, app_id: &str) -> Result<ContainerStatus> {
        self.get_container_status(app_id).await
    }

    async fn list(&self) -> Result<Vec<ContainerStatus>> {
        self.list_containers().await
    }

    async fn logs(&self, app_id: &str, lines: u32) -> Result<Vec<String>> {
        self.get_container_logs(app_id, lines).await
    }

    async fn health(&self, app_id: &str) -> Result<String> {
        self.get_health_status(app_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::candidate_manifest_paths;
    use std::path::PathBuf;

    #[test]
    fn candidate_manifest_paths_include_expected_fallbacks() {
        let app_id = "bitcoin-ui";
        let paths = candidate_manifest_paths(app_id, &PathBuf::from("/var/lib/archipelago"));
        let as_strings: Vec<String> = paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();

        assert!(as_strings
            .iter()
            .any(|p| p == "/opt/archipelago/apps/bitcoin-ui/manifest.yml"));
        assert!(as_strings
            .iter()
            .any(|p| p == "/home/archipelago/Projects/archy/apps/bitcoin-ui/manifest.yml"));
        assert!(as_strings
            .iter()
            .any(|p| p == "/var/lib/archipelago/apps/bitcoin-ui/manifest.yml"));
    }
}
