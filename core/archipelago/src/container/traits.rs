//! Orchestrator trait — the shared surface the RPC layer talks to.
//!
//! Step 4 of the rust-orchestrator migration. Unifies the container lifecycle
//! surface of `DevContainerOrchestrator` and `ProdContainerOrchestrator` so
//! `RpcHandler` can hold `Arc<dyn ContainerOrchestrator>` and stop caring
//! which mode it is in.
//!
//! The trait takes `app_id: &str` everywhere (never a manifest path). Dev and
//! Prod both resolve app_id → manifest internally. The legacy
//! `container-install { manifest_path }` RPC shape is preserved as a concrete
//! `install_container_from_path` method on `DevContainerOrchestrator` only,
//! since that ad-hoc workflow is a dev convenience and has no prod meaning.
//!
//! See `docs/rust-orchestrator-migration.md`.

use anyhow::Result;
use archipelago_container::ContainerStatus;
use async_trait::async_trait;

/// Lifecycle + query operations every orchestrator exposes to the RPC layer.
#[async_trait]
pub trait ContainerOrchestrator: Send + Sync {
    /// Build-or-pull the image, create the container, and start it. Returns the
    /// podman container name that was created. Assumes the app_id corresponds
    /// to a manifest the orchestrator already knows about.
    async fn install(&self, app_id: &str) -> Result<String>;

    /// True when this orchestrator holds a manifest for `app_id` (disk or
    /// signed-catalog overlay) — i.e. `install(app_id)` would not fail with
    /// "unknown app_id". Lets the RPC layer route any manifest-driven app
    /// through the orchestrator without a per-app allowlist. Defaults to
    /// `false` so orchestrators without a manifest registry keep routing
    /// through the legacy install flow.
    async fn knows_app(&self, _app_id: &str) -> bool {
        false
    }

    /// Rebuild the in-memory manifest map (disk + signed-catalog overlay).
    /// Called after a runtime catalog refresh detects changed bytes so catalog
    /// manifest changes take effect without a service restart — without this,
    /// `load_manifests` only runs at startup and a freshly published manifest
    /// sits dormant until the next restart. Returns the merged manifest count.
    /// Defaults to a no-op for orchestrators without a manifest registry.
    async fn reload_manifests(&self) -> Result<usize> {
        Ok(0)
    }

    /// Start an already-created container.
    async fn start(&self, app_id: &str) -> Result<()>;

    /// Stop a running container. No-op on Prod if already stopped.
    async fn stop(&self, app_id: &str) -> Result<()>;

    /// Stop-then-start. Best-effort: ignores stop failure.
    async fn restart(&self, app_id: &str) -> Result<()>;

    /// Remove the container. `preserve_data = true` keeps the volumes; `false`
    /// is honored on a best-effort basis (Dev cleans, Prod leaves the volume
    /// management to the data layer).
    async fn remove(&self, app_id: &str, preserve_data: bool) -> Result<()>;

    /// Pull/rebuild the image and recreate the container from scratch.
    async fn upgrade(&self, app_id: &str) -> Result<()>;

    /// Current state of a single container.
    async fn status(&self, app_id: &str) -> Result<ContainerStatus>;

    /// All containers this orchestrator knows about.
    async fn list(&self) -> Result<Vec<ContainerStatus>>;

    /// Tail the container's stdout+stderr.
    async fn logs(&self, app_id: &str, lines: u32) -> Result<Vec<String>>;

    /// Coarse health summary: "healthy", "unhealthy", "starting", "paused", "unknown".
    async fn health(&self, app_id: &str) -> Result<String>;

    /// Declare that a credential this app consumes has just been rotated, so
    /// the running container is now holding an invalid one.
    ///
    /// Restart-sensitivity normally protects apps like `btcpay-server` from
    /// being recreated on drift — correct when the running container is
    /// working, and exactly wrong when it is working only in appearance. After
    /// an LND macaroon rotation, BTCPay is up and healthy while every Lightning
    /// operation it attempts fails against a credential LND no longer honours;
    /// leaving it untouched perpetuates the breakage rather than protecting
    /// anything. This is the same carve-out FED-07 uses for the Fedimint
    /// gateway, reached from the RPC layer instead of from inside a reconcile.
    ///
    /// Consumed by the next drift check, which recreates the container around
    /// its unchanged data directory, ports and volumes. Default no-op: an
    /// orchestrator without restart-sensitivity has nothing to override.
    async fn mark_credential_rotated(&self, _app_id: &str) {}
}
