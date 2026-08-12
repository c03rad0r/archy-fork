# Rust Orchestrator Migration — Design Doc

Status: **DRAFT — pending user approval**
Author: OpenCode session, 2026-04-22
Supersedes planning in `docs/bulletproof-containers.md` v1.7.43 slot

## Problem statement

Today, the archipelago backend has **no production container orchestrator**. Production containers (bitcoin-knots, lnd, electrumx, btcpay, filebrowser, and the three custom UIs archy-bitcoin-ui / archy-electrs-ui / archy-lnd-ui) are installed by **bash scripts** at first boot (`scripts/first-boot-containers.sh`) and optionally reconciled by another bash script (`scripts/reconcile-containers.sh`) that is **not enabled by default**. The existing `DevContainerOrchestrator` (`core/archipelago/src/container/dev_orchestrator.rs`) is hardcoded to append `-dev` suffixes and gated behind `config.dev_mode`, so it has never managed a production container.

This design migrates production container management into Rust, under a single orchestrator that owns install, start, stop, restart, upgrade, uninstall, health, and self-healing for every container. The three custom UI containers are the first-class test fixture: they exercise the "build image from local Dockerfile" path (which today doesn't exist in the manifest schema) and their lifecycle was the original failure class the user asked to fix.

## Non-goals

- Backwards compatibility with `first-boot-containers.sh`: we **delete** it and its systemd unit after verifying Rust parity.
- Backwards compatibility with the existing `package-install` RPC’s podman shell-outs: those get rewritten to call the orchestrator.
- Registry signature verification: `image_signature` stays optional. Sigstore/cosign integration is out of scope.
- Network isolation improvements: existing SecurityPolicy fields stay as-is.
- Dev mode removal: `DevContainerOrchestrator` keeps existing behavior for local development; prod code path is separate.

## Scope of this migration

In scope:
1. Extend `ContainerConfig` schema with a `source:` variant supporting `{type: build, context, dockerfile, tag}` alongside `{type: pull, image, pull_policy}`.
2. Extend `ContainerRuntime` trait + `PodmanRuntime` impl with `build_image(...)` and `image_exists(...)`.
3. Introduce `ProdContainerOrchestrator` (new type) with identical public surface to `DevContainerOrchestrator` but **no `-dev` suffix**, **no port offset**, **no data-path rewriting**, **no bitcoin_simulator gate**. It is wired into `RpcHandler::orchestrator` in prod (currently `None`).
4. Add `AdoptionScan` at orchestrator startup: enumerate `podman ps -a`, match by container name against declared manifests, adopt into orchestrator state without recreating.
5. Add `BootReconciler` task spawned from `main.rs` (replacing the commented-out `run_boot_reconciliation` hook). Walks the manifest set on startup and periodically, ensures each is present-and-running, builds/pulls/creates anything missing, logs failures non-silently.
6. Ship three manifests in the repo: `apps/bitcoin-ui/manifest.yml`, `apps/electrs-ui/manifest.yml`, `apps/lnd-ui/manifest.yml`. They use the new `source: build` variant pointing at `/opt/archipelago/docker/<name>/`.
7. Delete `scripts/first-boot-containers.sh`, `scripts/reconcile-containers.sh`, `scripts/container-specs.sh`, `image-recipe/configs/archipelago-first-boot-containers.service`, `image-recipe/configs/archipelago-reconcile.service`. Remove enablement from ISO builder.

Out of scope this migration (tracked separately):
- Migrating btcpay / mempool / fedimint multi-container stacks to manifests (they currently live in `core/archipelago/src/api/rpc/package/stacks.rs`). They keep working via `package-install` RPC. Phase 2.
- Rewriting the 26 existing `apps/*/manifest.yml` files to use the new `source:` schema. They stay on `image:` for now; the schema is **additive and backwards-compatible**.
- Re-enabling signature verification; stays todo.

## Data model changes

### 1. `ContainerConfig` gets a `source` enum

File: `core/container/src/manifest.rs:58`

**Before:**
```rust
pub struct ContainerConfig {
    pub image: String,
    pub image_signature: Option<String>,
    pub pull_policy: String,
}
```

**After:**
```rust
pub struct ContainerConfig {
    // Legacy shorthand (backwards compatible with all 26 existing manifests):
    // if `source` is absent, `image` + `pull_policy` are interpreted as
    // `source: { type: pull, image, pull_policy }`.
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub image_signature: Option<String>,
    #[serde(default = "default_pull_policy")]
    pub pull_policy: String,

    // New: explicit source. If present, overrides the legacy shorthand.
    #[serde(default)]
    pub source: Option<ContainerSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ContainerSource {
    /// Pull an image from a registry.
    Pull {
        image: String,
        #[serde(default)]
        image_signature: Option<String>,
        #[serde(default = "default_pull_policy")]
        pull_policy: String,
    },
    /// Build an image from a local Dockerfile.
    Build {
        /// Filesystem path to build context, absolute or relative to manifest dir.
        context: String,
        /// Dockerfile path relative to context. Defaults to "Dockerfile".
        #[serde(default = "default_dockerfile")]
        dockerfile: String,
        /// Tag to assign to the built image, e.g. "localhost/bitcoin-ui:local".
        tag: String,
        /// `--build-arg` key=value pairs.
        #[serde(default)]
        build_args: HashMap<String, String>,
        /// If true, rebuild on every reconcile. If false, only build when tag is missing.
        #[serde(default)]
        always_rebuild: bool,
    },
}
```

Validation in `AppManifest::validate`:
- If `source` is absent AND `image` is empty → error (unchanged rule just rephrased).
- If `source` is present, legacy `image` field is ignored with a warning.
- `Build::context` must resolve to an existing directory that contains `dockerfile`.

Tests to add:
- Parse a legacy manifest → works, produces `ContainerSource::Pull` at resolution time.
- Parse a `source: { type: build, ... }` manifest → works.
- Parse a manifest with both legacy `image:` and `source:` → warning logged, `source:` wins.
- Parse a manifest with neither → rejected.

### 2. `ContainerRuntime` trait gets `build_image` + `image_exists`

File: `core/container/src/runtime.rs:10`

```rust
#[async_trait]
pub trait ContainerRuntime: Send + Sync {
    // existing methods unchanged...
    async fn pull_image(&self, image: &str, signature: Option<&str>) -> Result<()>;
    async fn create_container(...) -> Result<()>;
    // ...

    // NEW:
    /// Build an image from a local Dockerfile. Returns Ok(()) if the image now
    /// exists under the given tag (whether newly built or already present and
    /// `force=false`). Returns Err if the build failed.
    async fn build_image(
        &self,
        context: &Path,
        dockerfile: &str,
        tag: &str,
        build_args: &HashMap<String, String>,
        force: bool,
    ) -> Result<()>;

    /// Check if an image exists in the local image store.
    async fn image_exists(&self, tag: &str) -> Result<bool>;
}
```

`PodmanRuntime::build_image` shells out:
```
podman build --tag <tag> \
    --file <context>/<dockerfile> \
    --build-arg KEY=VALUE ... \
    <context>
```

Force-rebuild semantics: if `force=false`, skip when `image_exists(tag) == true`. If `force=true`, always build (podman's own layer cache handles the fast path).

Tests:
- `build_image` happy path on a minimal Dockerfile (using a throwaway context in tmpdir).
- `build_image` failure path (nonsense Dockerfile) → Err.
- `image_exists` returns false for nonexistent tag.
- `image_exists` returns true after `build_image`.

### 3. Manifest resolution: `ContainerSource::resolve(manifest_dir) -> ResolvedSource`

New method that turns the raw manifest into something the orchestrator can act on:

```rust
pub enum ResolvedSource {
    Pull { image: String, signature: Option<String>, pull_policy: PullPolicy },
    Build { context: PathBuf, dockerfile: String, tag: String, build_args: HashMap<String,String>, always_rebuild: bool },
}

impl ContainerConfig {
    pub fn resolve(&self, manifest_dir: &Path) -> Result<ResolvedSource> {
        match &self.source {
            Some(ContainerSource::Pull { image, image_signature, pull_policy }) => Ok(ResolvedSource::Pull { ... }),
            Some(ContainerSource::Build { context, dockerfile, tag, build_args, always_rebuild }) => {
                let abs_context = if Path::new(context).is_absolute() {
                    PathBuf::from(context)
                } else {
                    manifest_dir.join(context)
                };
                Ok(ResolvedSource::Build { context: abs_context, ... })
            }
            None => {
                // Legacy shorthand
                if self.image.is_empty() {
                    return Err(...);
                }
                Ok(ResolvedSource::Pull { image: self.image.clone(), ... })
            }
        }
    }
}
```

## Runtime architecture

### `ProdContainerOrchestrator`

New file: `core/archipelago/src/container/prod_orchestrator.rs`

```rust
pub struct ProdContainerOrchestrator {
    runtime: Arc<dyn ContainerRuntimeTrait>,
    manifests_dir: PathBuf,   // e.g. /opt/archipelago/apps
    data_dir: PathBuf,        // e.g. /var/lib/archipelago
    state: Arc<RwLock<OrchestratorState>>,
    config: Config,
}

struct OrchestratorState {
    /// app_id → known manifest (loaded from disk at startup, refreshed on reconcile)
    manifests: HashMap<String, AppManifest>,
    /// app_id → current known state (from adoption scan or our own ops)
    containers: HashMap<String, ContainerState>,
    /// app_id → last install/health/build timestamp
    last_reconciled: HashMap<String, Instant>,
}
```

Public surface mirrors `DevContainerOrchestrator` but **container name = `archy-<app_id>` for UI apps, `<app_id>` for backends, matching existing .116 naming**:

```rust
impl ProdContainerOrchestrator {
    pub async fn new(config: Config) -> Result<Self> { ... }
    pub async fn load_manifests(&self) -> Result<()> { /* walks manifests_dir */ }
    pub async fn adopt_existing(&self) -> Result<AdoptionReport> { /* scans podman ps -a */ }
    pub async fn reconcile_all(&self) -> Result<ReconcileReport> { /* ensures every manifest has a running container */ }
    pub async fn install(&self, app_id: &str) -> Result<()> { /* build-or-pull + create + start */ }
    pub async fn start(&self, app_id: &str) -> Result<()> { ... }
    pub async fn stop(&self, app_id: &str) -> Result<()> { ... }
    pub async fn restart(&self, app_id: &str) -> Result<()> { ... }
    pub async fn remove(&self, app_id: &str, preserve_data: bool) -> Result<()> { ... }
    pub async fn upgrade(&self, app_id: &str) -> Result<()> { /* re-read manifest, rebuild/pull, recreate */ }
    pub async fn status(&self, app_id: &str) -> Result<ContainerStatus> { ... }
    pub async fn list(&self) -> Result<Vec<ContainerStatus>> { ... }
    pub async fn logs(&self, app_id: &str, lines: u32) -> Result<Vec<String>> { ... }
    pub async fn health(&self, app_id: &str) -> Result<String> { ... }
}
```

**Container naming rule** (matches `.116` existing fixture so adoption works):
- If the manifest has `extensions["container_name"]` → use that verbatim.
- Else if the app_id starts with `bitcoin-ui` / `electrs-ui` / `lnd-ui` → `archy-<app_id>`.
- Else → `<app_id>`.

This is codified and tested; no ad-hoc naming in the codebase.

### `AdoptionScan`

On orchestrator startup, before any reconcile:

```rust
async fn adopt_existing(&self) -> Result<AdoptionReport> {
    let all = self.runtime.list_containers().await?;  // podman ps -a
    let mut report = AdoptionReport::default();
    for c in all {
        // For each manifest we have loaded, check if the expected container name matches
        for (app_id, manifest) in self.state.read().await.manifests.iter() {
            let expected_name = compute_container_name(manifest);
            if c.name == expected_name {
                // This container is ours. Record its state.
                self.state.write().await.containers.insert(app_id.clone(), c.state.clone());
                report.adopted.push(app_id.clone());
            }
        }
    }
    Ok(report)
}
```

No recreate. No touching data volumes. Just "we now know this container belongs to app X and its current state is Y".

### `BootReconciler`

New file: `core/archipelago/src/container/boot_reconciler.rs`

```rust
pub struct BootReconciler {
    orchestrator: Arc<ProdContainerOrchestrator>,
    interval: Duration,  // e.g. 5 minutes
    shutdown: CancellationToken,
}

impl BootReconciler {
    pub async fn run_forever(self) {
        // Initial reconcile immediately (after adoption).
        let _ = self.orchestrator.reconcile_all().await;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(self.interval) => {
                    let _ = self.orchestrator.reconcile_all().await;
                }
                _ = self.shutdown.cancelled() => break,
            }
        }
    }
}
```

`reconcile_all`:
```rust
async fn reconcile_all(&self) -> Result<ReconcileReport> {
    let manifests: Vec<_> = self.state.read().await.manifests.values().cloned().collect();
    let mut report = ReconcileReport::default();
    for manifest in manifests {
        let app_id = &manifest.app.id;
        match self.ensure_running(&manifest).await {
            Ok(action) => report.record(app_id, action),
            Err(e) => {
                tracing::error!(app_id, error = %e, "Reconcile failed for app");
                report.failures.push((app_id.clone(), e.to_string()));
            }
        }
    }
    if !report.failures.is_empty() {
        // Surface via WebSocket so the UI can show a banner.
        self.notify_failures(&report).await;
    }
    Ok(report)
}

async fn ensure_running(&self, manifest: &AppManifest) -> Result<ReconcileAction> {
    let name = compute_container_name(manifest);
    match self.runtime.get_container_status(&name).await {
        Ok(status) if matches!(status.state, ContainerState::Running) => Ok(ReconcileAction::NoOp),
        Ok(status) if matches!(status.state, ContainerState::Exited | ContainerState::Stopped) => {
            self.runtime.start_container(&name).await?;
            Ok(ReconcileAction::Started)
        }
        Ok(_) => Ok(ReconcileAction::NoOp),  // Created / Paused — leave alone
        Err(_) => {
            // Container doesn't exist. Install it.
            self.install_fresh(manifest).await?;
            Ok(ReconcileAction::Installed)
        }
    }
}

async fn install_fresh(&self, manifest: &AppManifest) -> Result<()> {
    let manifest_dir = ...;  // directory of manifest.yml
    let resolved = manifest.app.container.resolve(manifest_dir)?;
    match resolved {
        ResolvedSource::Pull { image, signature, .. } => {
            self.runtime.pull_image(&image, signature.as_deref()).await?;
        }
        ResolvedSource::Build { context, dockerfile, tag, build_args, always_rebuild } => {
            if always_rebuild || !self.runtime.image_exists(&tag).await? {
                self.runtime.build_image(&context, &dockerfile, &tag, &build_args, always_rebuild).await?;
            }
        }
    }
    self.runtime.create_container(manifest, &compute_container_name(manifest), 0).await?;
    self.runtime.start_container(&compute_container_name(manifest)).await?;
    Ok(())
}
```

### Wire-up in `main.rs`

File: `core/archipelago/src/main.rs`

Replace the commented-out `run_boot_reconciliation` block (`main.rs:107-111`) with:

```rust
// Load manifests + adopt existing + start reconciler loop.
let orchestrator = Arc::new(ProdContainerOrchestrator::new(config.clone()).await?);
orchestrator.load_manifests().await?;
let adoption = orchestrator.adopt_existing().await?;
tracing::info!(adopted = adoption.adopted.len(), "Container adoption complete");
let reconciler = BootReconciler::new(orchestrator.clone(), Duration::from_secs(300), shutdown_token.clone());
tokio::spawn(reconciler.run_forever());
```

`RpcHandler` gets the orchestrator regardless of `dev_mode`:
```rust
// core/archipelago/src/api/rpc/mod.rs:83
let orchestrator: Option<Arc<dyn ContainerOrchestrator>> = if config.dev_mode {
    Some(Arc::new(DevContainerOrchestrator::new(config.clone()).await?))
} else {
    Some(Arc::new(prod_orch.clone()))
};
```

Where `ContainerOrchestrator` becomes a trait implemented by both `DevContainerOrchestrator` and `ProdContainerOrchestrator`.

### First-boot replacement

There is no separate first-boot code. The reconciler handles it: when the archipelago service starts on a fresh node, `adopt_existing` finds nothing, `reconcile_all` sees no running container for any manifest, and installs each one in dependency order (bitcoin-core first, then everything else). On subsequent boots, adoption finds existing containers and reconcile mostly no-ops.

**Removes completely**:
- `/var/lib/archipelago/.first-boot-containers-done` marker (no longer needed)
- `/var/lib/archipelago/.unbundled` handling in first-boot script (becomes a config flag in archipelago.conf if we still need it)
- `scripts/first-boot-containers.sh` (1392 lines)
- `scripts/reconcile-containers.sh`
- `scripts/container-specs.sh`
- `image-recipe/configs/archipelago-first-boot-containers.service`
- `image-recipe/configs/archipelago-reconcile.service`
- Related enable/disable in ISO builder

## The three UI manifests

Example: `apps/bitcoin-ui/manifest.yml`

```yaml
app:
  id: bitcoin-ui
  name: Bitcoin Knots UI
  version: 1.0.0
  description: Custom Archipelago UI for Bitcoin Knots
  container:
    source:
      type: build
      context: /opt/archipelago/docker/bitcoin-ui
      dockerfile: Dockerfile
      tag: localhost/bitcoin-ui:local
      build_args:
        BITCOIN_RPC_AUTH: ${BITCOIN_RPC_AUTH}  # injected from host-ip.env or secrets
      always_rebuild: false
  dependencies:
    - app_id: bitcoin-core
  resources:
    memory_limit: 128Mi
  security:
    network_policy: host
    readonly_root: false
  ports: []  # host networking
  volumes: []
  environment: []
  health_check:
    type: http
    endpoint: http://127.0.0.1:8334
    path: /
    interval: 30s
  extensions:
    container_name: archy-bitcoin-ui
```

The `extensions.container_name` is how we match the existing running container on .116 for adoption. Same pattern for `electrs-ui` (container_name: `archy-electrs-ui`, port probe 50002) and `lnd-ui` (container_name: `archy-lnd-ui`, port probe 8081).

**BITCOIN_RPC_AUTH injection**: today `first-boot-containers.sh` `sed`s this value into `nginx.conf` (destructively). In the new world, it's a `--build-arg` — the Dockerfile gets `ARG BITCOIN_RPC_AUTH` and templates `nginx.conf` from a template file. Fixes the "sed destroys the source" bug from the mapping.

## Migration path (.116 and .228 specifically)

### .116 (all 3 UIs currently running, adopted from bash install)
1. Ship the new archipelago binary with the prod orchestrator.
2. On archipelago restart, `adopt_existing` scans `podman ps -a`, sees `archy-bitcoin-ui`, `archy-electrs-ui`, `archy-lnd-ui` already running.
3. Matches them against the new manifests by `extensions.container_name`.
4. Records state. Reconciler sees them Running → NoOp.
5. Manual test: `podman stop archy-bitcoin-ui` → within 5 minutes, reconciler starts it again. `podman rm -f archy-bitcoin-ui` → reconciler rebuilds from `/opt/archipelago/docker/bitcoin-ui/Dockerfile` and re-creates.

### .228 (no bitcoin-ui, no lnd-ui, has electrs-ui from bash first-boot)
1. Ship same binary.
2. Adoption finds only `archy-electrs-ui`.
3. Reconciler sees `bitcoin-ui` and `lnd-ui` missing → triggers `install_fresh` for each.
4. For `bitcoin-ui`: `image_exists("localhost/bitcoin-ui:local")` → false. `build_image(/opt/archipelago/docker/bitcoin-ui, Dockerfile, localhost/bitcoin-ui:local, {BITCOIN_RPC_AUTH: ...}, force=false)`. Then create + start.
5. Same for `lnd-ui`.
6. Manual test: HTTP probe ports 8334 and 8081 return 200 within ~5 minutes of service restart.

## Test plan

Unit tests (Rust, in-process):
- `manifest::tests::legacy_image_parses_as_pull_source`
- `manifest::tests::explicit_pull_source_parses`
- `manifest::tests::explicit_build_source_parses`
- `manifest::tests::source_build_requires_tag`
- `runtime::tests::build_image_happy_path` (uses a minimal Dockerfile in `tempfile::TempDir`)
- `runtime::tests::build_image_failure`
- `runtime::tests::image_exists_roundtrip`
- `prod_orchestrator::tests::install_fresh_pull`
- `prod_orchestrator::tests::install_fresh_build`
- `prod_orchestrator::tests::adopt_existing_matches_by_name`
- `prod_orchestrator::tests::reconcile_starts_exited_container` (with a mock runtime)
- `prod_orchestrator::tests::reconcile_installs_missing_container`
- `prod_orchestrator::tests::compute_container_name_ui_apps_prefixed`
- `prod_orchestrator::tests::compute_container_name_backend_apps_bare`

Integration tests (require real podman, run on archy node):
- Fresh-install path: wipe containers + images, start archipelago, verify all 3 UIs up within 60s.
- Adoption path: containers pre-running, start archipelago, verify no recreate (compare container IDs before/after).
- Reconcile-start path: `podman stop archy-bitcoin-ui`, wait, verify restart.
- Reconcile-recreate path: `podman rm -f archy-bitcoin-ui`, wait, verify rebuild+recreate.
- Rebuild-on-Dockerfile-change path: edit Dockerfile, call `upgrade` RPC, verify image rebuilt and container recreated.

Chaos matrix (bash + Playwright, the original goal):
- For each UI (bitcoin-ui, electrs-ui, lnd-ui) × each event (stop, start, restart, remove+reconcile, SIGKILL, archipelago-service-restart, host-reboot) × each node (.116, .228): assert HTTP 200 + page-title marker returns within 60s of event.

## Risks + mitigations

| Risk | Mitigation |
|------|------------|
| Adoption mismatches and re-creates a container we already had, losing its data | Adoption matches by exact name; `install_fresh` only runs when `get_container_status` returns Err (container doesn't exist), not when it returns Stopped/Exited. Unit tested. |
| Build loop: reconciler rebuilds on every tick | `always_rebuild: false` + `image_exists` check. Only rebuilds when image tag is missing OR `upgrade` RPC is called. |
| Reconciler runs while user is mid-install via the UI | Orchestrator state has per-app mutex; reconcile waits. Install path takes the same mutex. |
| Auto-rollback (v1.7.41) fires during testing | `reconcile_all` is spawned AFTER server is healthy and responding; if it fails, archipelago the service still passes verification. Individual container failures are logged, not fatal. |
| Dependency ordering: bitcoin-ui needs BITCOIN_RPC_AUTH which is generated at first boot | Reconciler handles dependency order by reading `manifest.app.dependencies` and installing in topological order. If the dep doesn't exist yet, skip and retry next tick. |
| Moving `/opt/archipelago/docker/<name>` content breaks the build context | That path is stable per the ISO builder at `image-recipe/build-auto-installer-iso.sh:1671-1685`. Manifests reference it absolutely. |
| Dropping bash scripts breaks existing ISOs in the field | Target release cycle is disposable alpha nodes. For existing alpha nodes (.116, .228) we hot-swap the binary and let the reconciler take over, then the next reboot doesn't need the systemd units; we mask them manually. |
| User wants to downgrade to v1.7.42 | Auto-rollback mechanism already handles that; binary swap is reversible. The removed bash scripts are still in git history. |

## Implementation order

1. **Schema first**: extend `ContainerConfig` + `ContainerSource` + `resolve()` + validation + unit tests. ~100 LOC Rust + ~80 LOC tests.
2. **Runtime**: `build_image` + `image_exists` in trait, `PodmanRuntime`, `DockerRuntime` (can stub), `AutoRuntime`. ~150 LOC + tests with throwaway tempdir Dockerfile.
3. **ProdContainerOrchestrator**: new type with `install/start/stop/restart/remove/status/list/logs/health/adopt_existing/reconcile_all/ensure_running/install_fresh`. ~400 LOC + unit tests with mocked runtime.
4. **ContainerOrchestrator trait**: abstract over Dev and Prod so `RpcHandler` is polymorphic. ~50 LOC refactor.
5. **BootReconciler**: task spawner with loop + cancellation. ~80 LOC + unit tests.
6. **main.rs wire-up**: adopt + spawn reconciler. ~20 LOC.
7. **3 UI manifests + Dockerfile BITCOIN_RPC_AUTH refactor** (use ARG + template file, not sed). ~60 lines of YAML + ~20 lines of Dockerfile.
8. **Remove bash scripts + services**: split into sub-steps because `first-boot-containers.sh` creates 25+ containers (only 3 ported in Step 7) AND does non-container setup (secret gen, UID-mapping chowns, Tor hostnames, WireGuard, firewall, nostr-relay dir):
   - **8a** (cheap, safe): delete `image-recipe/configs/archipelago-reconcile.{service,timer}` + their ISO-builder touchpoints (the systemd enablement + `cp` into `$WORK_DIR`). `BootReconciler` fully replaces the timer-driven path — no more periodic bash invocation. **Keep** `scripts/reconcile-containers.sh` + `scripts/container-specs.sh` because `core/archipelago/src/api/rpc/package/update.rs` still shells out to reconcile-containers.sh during OTA updates; porting that call site requires manifests for every container it touches (which is Step 8b's scope). Atomic commit, low risk.
   - **8b** (large, deferred): port the remaining ~25 container creations from `first-boot-containers.sh` into `apps/<id>/manifest.yml` files. One manifest per commit, validated against current bash behavior (ports, volumes, env, deps, health checks, post-create wallet/db bootstrap). Probably 1-2 days of careful porting. Includes `apps/filebrowser/manifest.yml`. Then port `update.rs`'s two `reconcile-containers.sh` call sites to the `ContainerOrchestrator` trait (`upgrade(app_id)`).
   - **8c** (final, one-way door): rename `first-boot-containers.sh` → `first-boot-setup.sh`, strip out all `$DOCKER run/pull/exec` calls, keep only secret generation + dir prep + Tor/WG/firewall/nostr setup. Rename `archipelago-first-boot-containers.service` → `archipelago-first-boot-setup.service`. Delete `scripts/reconcile-containers.sh` + `scripts/container-specs.sh` (update.rs no longer needs them). Add ISO builder lines to copy `apps/*/manifest.yml` → `/opt/archipelago/apps/`. Full ISO build test on .116 required before commit.
9. **Live test on .228**: hot-swap binary, expect 3 UIs to come up within 60s of service restart.
10. **Live test on .116**: hot-swap binary, expect zero container recreation + adoption-confirmed log lines.
11. **Chaos matrix** on both nodes.

Each step is a separate commit. Steps 1–6 are independent-enough that they can each have their own test gate.

## Estimated total

~1000 LOC Rust added, ~1500 lines bash deleted, ~50 LOC Rust deleted. 8–12 hours of focused work across multiple sessions. No release pressure per user decision.

## Open questions for user

1. **Container naming**: I propose `archy-<app_id>` for UIs, `<app_id>` for backends (matches current .116 fixture). Alternative: unify on `archy-<app_id>` for everything and migrate existing backends by renaming at adoption. Which?
2. **BITCOIN_RPC_AUTH injection**: the build-arg approach rebuilds the UI image when the auth value changes. Fine during normal operation (rare). Alternative: mount the nginx.conf at runtime as a volume, never bake auth into the image. Which?
3. **Reconciler interval**: 5 minutes. Too slow for a dropped container (user sees a broken UI for up to 5 min). Alternative: 30 seconds + more expensive `podman ps` calls. Which?
4. **Concurrent reconcile + user install**: per-app mutex is the simple answer. Alternative: a single orchestrator-wide mutex (simpler, slower). Which?
5. **Delete bash scripts in this migration, or keep them around as fallback?** I recommend delete (single source of truth), but deleting `first-boot-containers.sh` is a one-way door in terms of field recovery.
