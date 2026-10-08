//! Production systemd/Podman adapter. Application write admission/drain is an
//! explicit dependency: neither process pause nor a filesystem receipt is drain.
use super::{
    supervised_update::{self, Completion, PreparedTarget, RecoveryImage, Supervisor, Unit},
    update_transaction::{Observed, Podman, Runtime, Target},
};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    future::Future,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

fn owned_directory(path: &Path) -> Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    anyhow::ensure!(
        meta.is_dir()
            && !meta.file_type().is_symlink()
            && meta.uid() == unsafe { libc::geteuid() }
            && meta.mode() & 0o022 == 0,
        "Maintenance ownership directory changed"
    );
    Ok(())
}
fn owned_record(path: &Path, limit: u64, exact_private: bool) -> Result<Vec<u8>> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    anyhow::ensure!(
        meta.is_file()
            && meta.uid() == unsafe { libc::geteuid() }
            && meta.mode() & 0o022 == 0
            && (!exact_private || meta.mode() & 0o777 == 0o600)
            && meta.len() <= limit,
        "Maintenance ownership record changed"
    );
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= limit,
        "Maintenance ownership record exceeds bound"
    );
    Ok(bytes)
}
/// Only the currently held Indee API or relay restart fence is admitted.
/// This never grants ownership to arbitrary operator or historical overrides.
fn validate_maintenance_override(
    data: &Path,
    runtime: &Path,
    name: &str,
    dropins: &str,
    restart: &str,
) -> Result<()> {
    if dropins.is_empty() {
        return Ok(());
    }
    let obligation = match name {
        "indeedhub-api" => "api_restart_override",
        "indeedhub-relay" => "relay_restart_override",
        _ => anyhow::bail!("External service override refused"),
    };
    anyhow::ensure!(restart == "no", "External service override refused");
    owned_directory(data)?;
    for relative in [
        "update-transactions",
        "update-transactions/holds",
        "update-transactions/supervised",
        "update-transactions/indeehub-maintenance",
        "app-maintenance",
    ] {
        owned_directory(&data.join(relative))?;
    }
    let operation = String::from_utf8(owned_record(
        &data.join("update-transactions/holds").join(name),
        128,
        false,
    )?)?;
    anyhow::ensure!(
        uuid::Uuid::parse_str(&operation)?.to_string() == operation,
        "Invalid maintenance hold owner"
    );
    anyhow::ensure!(
        owned_record(&data.join("app-maintenance/indeedhub"), 128, false)? == operation.as_bytes(),
        "Maintenance admission fence owner changed"
    );
    let maintenance_dir = data
        .join("update-transactions/indeehub-maintenance")
        .join(&operation);
    owned_directory(&maintenance_dir)?;
    let journal: serde_json::Value = serde_json::from_slice(&owned_record(
        &maintenance_dir.join("journal.json"),
        16 * 1024 * 1024,
        false,
    )?)?;
    let native: serde_json::Value = serde_json::from_slice(&owned_record(
        &data
            .join("update-transactions/supervised")
            .join(format!("{operation}.json")),
        16 * 1024 * 1024,
        false,
    )?)?;
    anyhow::ensure!(
        native["id"].as_str() == Some(operation.as_str())
            && native["package"] == "indeedhub"
            && matches!(native["schema"].as_u64(), Some(1 | 2))
            && native["cleanup_done"] == false
            && matches!(
                native["phase"].as_str(),
                Some("Editing" | "Starting" | "Restoring" | "Restored" | "Committed")
            ),
        "Native maintenance operation is not active"
    );
    anyhow::ensure!(
        journal["operation_id"].as_str() == Some(operation.as_str())
            && matches!(
                journal["phase"].as_str(),
                Some("Prepared" | "Drained" | "Recovering")
            ),
        "Maintenance journal owner is not active"
    );
    let originals = native["members"]
        .as_array()
        .context("Missing native members")?;
    let matching: Vec<_> = originals
        .iter()
        .filter(|m| m["original"]["name"] == name)
        .collect();
    anyhow::ensure!(matching.len() == 1, "Ambiguous native member identity");
    let original = &matching[0]["original"];
    let members = journal["original_members"]
        .as_array()
        .context("Missing maintenance members")?;
    let matching: Vec<_> = members.iter().filter(|m| m["name"] == name).collect();
    anyhow::ensure!(matching.len() == 1, "Ambiguous maintenance member identity");
    let member = matching[0];
    let body = original["body"]
        .as_str()
        .context("Missing original managed unit")?;
    anyhow::ensure!(
        ["container_id", "image", "config_sha256"]
            .iter()
            .all(|key| original[*key]
                .as_str()
                .is_some_and(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit())))
            && member["container_id"] == original["container_id"]
            && member["image_id"] == original["image"]
            && member["config_sha256"] == original["config_sha256"]
            && original["running"].is_boolean()
            && member["running"] == original["running"]
            && member["unit_sha256"] == hex::encode(Sha256::digest(body.as_bytes())),
        "Maintenance member binding changed"
    );
    let bytes = format!("# Archipelago maintenance operation {operation}\n[Service]\nRestart=no\n");
    let saved = &journal[obligation];
    anyhow::ensure!(
        saved["operation_id"].as_str() == Some(operation.as_str())
            && saved["released"] == false
            && saved["sha256"] == hex::encode(Sha256::digest(bytes.as_bytes())),
        "Maintenance restart override obligation changed"
    );
    owned_directory(runtime)?;
    for relative in ["systemd", "systemd/user"] {
        owned_directory(&runtime.join(relative))?;
    }
    let service_dir = runtime
        .join("systemd/user")
        .join(format!("{name}.service.d"));
    owned_directory(&service_dir)?;
    let expected = service_dir.join(format!("zz-archipelago-maintenance-{operation}.conf"));
    anyhow::ensure!(
        expected.to_str() == Some(dropins),
        "Unexpected or additional service override"
    );
    anyhow::ensure!(
        owned_record(&expected, 512, true)? == bytes.as_bytes(),
        "Maintenance restart override bytes changed"
    );
    Ok(())
}

pub(crate) trait DrainBarrier: Sync {
    /// Called after the node persisted original writable recovery images.
    /// Persist ownership before blocking admissions or stopping writers. Return
    /// only after API/direct uploads/workers drained and coherent backup finished.
    fn acquire(
        &self,
        operation: &str,
        originals: &[Unit],
        recovery: bool,
    ) -> impl Future<Output = Result<()>> + Send;
    /// Must inspect the live admission fence and same-operation ownership, not
    /// merely trust an old 'backup complete' file. Keep it through snapshot/stop.
    fn verify(&self, operation: &str) -> impl Future<Output = Result<()>> + Send;
    /// Idempotent; an old recovery must never clear a newer operation's fence.
    fn release(
        &self,
        operation: &str,
        outcome: Completion,
    ) -> impl Future<Output = Result<()>> + Send;
}

/// The controller is deployed from reviewed platform source. Its durable
/// journal owns legacy ingress/drain/backup across API/container shutdown.
pub(crate) struct LegacyIndeeMaintenance {
    lock: std::sync::Arc<std::fs::File>,
}
impl LegacyIndeeMaintenance {
    pub(crate) fn new(guard: &super::update_transaction::Guard) -> Result<Self> {
        Self::controller_path()?;
        Ok(Self {
            lock: std::sync::Arc::new(guard.clone_lifecycle_lock()?),
        })
    }
    fn controller_path() -> Result<&'static Path> {
        let path = Path::new("/opt/archipelago/scripts/indeehub-maintenance-controller.py");
        let metadata = std::fs::symlink_metadata(path)
            .context("Legacy maintenance controller is not installed; original runtime retained")?;
        anyhow::ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && (metadata.uid() == 0 || metadata.uid() == unsafe { libc::geteuid() }),
            "Maintenance controller ownership changed"
        );
        anyhow::ensure!(
            metadata.mode() & 0o022 == 0,
            "Maintenance controller is writable by another user"
        );
        anyhow::ensure!(metadata.len() <= 128*1024
            && Sha256::digest(std::fs::read(path)?) == Sha256::digest(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../scripts/indeehub-maintenance-controller.py"))),
            "Maintenance controller does not match this qualified backend; install the exact companion script first");
        Ok(path)
    }
    async fn invoke(&self, action: &'static str, request: serde_json::Value) -> Result<()> {
        use std::os::{fd::AsRawFd, unix::process::CommandExt};
        use tokio::io::AsyncWriteExt;
        let path = Self::controller_path()?;
        let operation = request["operation_id"]
            .as_str()
            .context("Missing maintenance operation")?
            .to_owned();
        let recovery = request.get("recovery").and_then(|v| v.as_bool()) == Some(true);
        let bytes = serde_json::to_vec(&request)?;
        let lock = self.lock.try_clone()?;
        // Detaching the RPC future must not detach mutation from its lifecycle
        // lock. This task and the child retain the same flock open description.
        tokio::spawn(async move {
            let fd = lock.as_raw_fd();
            // The manager's ProtectSystem=strict mount namespace prevents rootless
            // Podman exec from entering retained user-unit cgroups. A user scope
            // executes the controller in that unit context while preserving pipes
            // and the inherited flock open description (unlike a detached service).
            let mut command = tokio::process::Command::new("/usr/bin/systemd-run");
            command
                .args([
                    "--user",
                    "--scope",
                    "--quiet",
                    "--collect",
                    "--",
                    "/usr/bin/python3",
                ])
                .arg(path)
                .arg(action)
                .env("ARCHY_UPDATE_LOCK_FD", fd.to_string())
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(false);
            unsafe {
                command.as_std_mut().pre_exec(move || {
                    if libc::fcntl(fd, libc::F_SETFD, 0) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let mut child = command
                .spawn()
                .context("Could not start legacy maintenance controller")?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(&bytes).await?;
                stdin.shutdown().await?;
            }
            let result = child.wait_with_output().await?;
            anyhow::ensure!(
                result.status.success() && result.stdout.len() <= 4096,
                "Legacy maintenance remains unresolved; inspect its original operation journal"
            );
            let reply: serde_json::Value = serde_json::from_slice(&result.stdout)?;
            let wanted = match action {
                "acquire" if recovery => "recovering",
                "acquire" => "drained",
                "verify" => "held",
                "release" => "released",
                _ => unreachable!(),
            };
            anyhow::ensure!(
                reply["operation_id"].as_str() == Some(operation.as_str())
                    && reply["state"].as_str() == Some(wanted),
                "Maintenance controller did not acknowledge this operation"
            );
            drop(lock);
            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("Maintenance completion task interrupted")?
    }
}
impl DrainBarrier for LegacyIndeeMaintenance {
    async fn acquire(&self, operation: &str, originals: &[Unit], recovery: bool) -> Result<()> {
        let members: Vec<_> = originals
            .iter()
            .map(|unit| {
                serde_json::json!({
            "name":unit.name,"container_id":unit.container_id,"image_id":unit.image,
            "unit_sha256":hex::encode(Sha256::digest(unit.body.as_bytes())),
            "config_sha256":unit.config_sha256,"running":unit.running})
            })
            .collect();
        self.invoke(
            "acquire",
            serde_json::json!({"operation_id":operation,"original_members":members,"recovery":recovery}),
        )
        .await
    }
    async fn verify(&self, operation: &str) -> Result<()> {
        self.invoke("verify", serde_json::json!({"operation_id":operation}))
            .await
    }
    async fn release(&self, operation: &str, outcome: Completion) -> Result<()> {
        self.invoke(
            "release",
            serde_json::json!({"operation_id":operation,"outcome":outcome}),
        )
        .await
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MigrationPlan {
    original_sha256: String,
    prepared: PreparedTarget,
}
impl MigrationPlan {
    /// The old render must be reproducible from an image-bound retained original
    /// manifest. A current target catalog is not an installed-version receipt.
    pub(crate) fn from_installed_render(
        original: &str,
        old_render: &str,
        target_render: &str,
        manifest: archipelago_container::AppManifest,
    ) -> Result<Self> {
        manifest.validate()?;
        let body = supervised_update::merge_reviewed_unit(original, old_render, target_render)?;
        Ok(Self {
            original_sha256: hex::encode(Sha256::digest(original.as_bytes())),
            prepared: PreparedTarget { body, manifest },
        })
    }
    /// Legacy overrides whose provenance cannot be reconstructed need a reviewed
    /// exact-original-hash-bound migration, prepared by node administration code.
    /// No browser RPC accepts unit bodies or this type.
    pub(crate) fn from_reviewed_legacy(
        original_sha256: &str,
        body: String,
        manifest: archipelago_container::AppManifest,
    ) -> Result<Self> {
        anyhow::ensure!(
            original_sha256.len() == 64 && original_sha256.bytes().all(|v| v.is_ascii_hexdigit()),
            "Invalid original unit commitment"
        );
        manifest.validate()?;
        Ok(Self {
            original_sha256: original_sha256.into(),
            prepared: PreparedTarget { body, manifest },
        })
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanFile {
    schema: u8,
    package: String,
    plans: HashMap<String, MigrationPlan>,
}
/// Administrative preparation is separate from the Update RPC. The RPC never
/// accepts unit bodies, hook commands, or a browser-chosen preparation path.
pub(crate) fn load_reviewed_plans(
    data_dir: &Path,
    package: &str,
) -> Result<HashMap<String, MigrationPlan>> {
    anyhow::ensure!(
        !package.is_empty()
            && package.len() <= 128
            && package
                .bytes()
                .all(|v| v.is_ascii_alphanumeric() || matches!(v, b'-' | b'_')),
        "Invalid managed package"
    );
    let dir = data_dir.join("managed-update-plans");
    let directory = std::fs::symlink_metadata(&dir)
        .context("No original-bound managed update plan; existing app remains unchanged")?;
    anyhow::ensure!(
        directory.is_dir()
            && !directory.file_type().is_symlink()
            && directory.uid() == unsafe { libc::geteuid() }
            && directory.mode() & 0o077 == 0,
        "Managed update plan directory must be private and node-owned"
    );
    let path = dir.join(format!("{package}.json"));
    let metadata = std::fs::symlink_metadata(&path)?;
    anyhow::ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o077 == 0
            && metadata.len() <= 4 * 1024 * 1024,
        "Managed update plan must be private and node-owned"
    );
    let record: PlanFile = serde_json::from_slice(&std::fs::read(path)?)?;
    anyhow::ensure!(
        record.schema == 1
            && record.package == package
            && !record.plans.is_empty()
            && record.plans.len() <= 32,
        "Managed update plan does not match this package"
    );
    for plan in record.plans.values() {
        anyhow::ensure!(
            plan.original_sha256.len() == 64
                && plan.original_sha256.bytes().all(|v| v.is_ascii_hexdigit()),
            "Invalid original unit commitment"
        );
        plan.prepared.manifest.validate()?;
    }
    Ok(record.plans)
}

/// Called by the explicit offline administration command before catalog
/// selection. It validates the complete reviewed plan against current originals
/// and installer pins, then preserves their exact recipes under the lifecycle lock.
pub(crate) async fn preserve_reviewed_indeehub_originals(data: &Path) -> Result<()> {
    let guard = super::update_transaction::Guard::acquire(data)?;
    guard.require_clear()?;
    anyhow::ensure!(
        guard.held_names()?.is_empty(),
        "Existing lifecycle holds require recovery"
    );
    let plans = load_reviewed_plans(data, "indeedhub")?;
    let names = [
        "indeedhub-postgres",
        "indeedhub-redis",
        "indeedhub-minio",
        "indeedhub-relay",
        "indeedhub-api",
        "indeedhub-ffmpeg",
        "indeedhub",
    ];
    anyhow::ensure!(
        plans.len() == names.len() && names.iter().all(|name| plans.contains_key(*name)),
        "Preparation requires the exact seven-member IndeeHub plan"
    );
    let refs: Vec<_> = names
        .iter()
        .map(|name| {
            Ok((
                name.to_string(),
                plans[*name]
                    .prepared
                    .manifest
                    .app
                    .container
                    .image
                    .clone()
                    .context("Reviewed target image is missing")?,
            ))
        })
        .collect::<Result<_>>()?;
    let targets = Podman::targets(&refs, false).await?;
    let adapter = SystemdSupervisor::new(
        data.to_path_buf(),
        plans,
        LegacyIndeeMaintenance::new(&guard)?,
    )
    .await?;
    let targets = adapter.reviewed_targets(&targets).await?;
    let mut originals = Vec::new();
    for target in targets {
        let original = adapter.capture(&target.name).await?;
        adapter.prepare_target(&target, &original).await?;
        originals.push(original);
    }
    supervised_update::preserve_observed_originals(&guard, &originals)
}

pub(crate) async fn recover_before_reconcile(
    data_dir: &Path,
    guard: &super::update_transaction::Guard,
) -> Result<()> {
    if !supervised_update::needs_recovery(guard)? {
        return Ok(());
    }
    let adapter = SystemdSupervisor::new(
        data_dir.into(),
        HashMap::new(),
        LegacyIndeeMaintenance::new(guard)?,
    )
    .await?;
    supervised_update::recover(guard, &adapter).await
}

pub(crate) struct SystemdSupervisor<B> {
    data_dir: PathBuf,
    unit_dir: PathBuf,
    plans: HashMap<String, MigrationPlan>,
    barrier: B,
}
impl<B: DrainBarrier> SystemdSupervisor<B> {
    pub(crate) async fn new(
        data_dir: PathBuf,
        plans: HashMap<String, MigrationPlan>,
        barrier: B,
    ) -> Result<Self> {
        let unit_dir = super::quadlet::unit_dir().await?;
        let meta = std::fs::symlink_metadata(&unit_dir)?;
        anyhow::ensure!(
            meta.is_dir()
                && !meta.file_type().is_symlink()
                && meta.uid() == unsafe { libc::geteuid() }
                && meta.mode() & 0o022 == 0,
            "Quadlet directory ownership changed"
        );
        Ok(Self {
            data_dir,
            unit_dir,
            plans,
            barrier,
        })
    }
    /// A reviewed plan may replace a mutable catalog spelling with its exact
    /// locally verified digest, but never select different image bytes.
    pub(crate) async fn reviewed_targets(&self, targets: &[Target]) -> Result<Vec<Target>> {
        let mut refs = Vec::new();
        for target in targets {
            let plan = self
                .plans
                .get(&target.name)
                .context("Missing reviewed managed target")?;
            let reference = plan
                .prepared
                .manifest
                .app
                .container
                .image
                .as_ref()
                .context("Reviewed image missing")?;
            anyhow::ensure!(
                reference.rsplit_once("@sha256:").is_some_and(
                    |(_, hash)| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())
                ),
                "Managed target must use an immutable digest"
            );
            refs.push((target.name.clone(), reference.clone()));
        }
        let resolved = Podman::targets(&refs, false).await?;
        anyhow::ensure!(
            resolved
                .iter()
                .zip(targets)
                .all(|(reviewed, catalog)| reviewed.name == catalog.name
                    && reviewed.image == catalog.image),
            "Reviewed managed image differs from prepared catalog image"
        );
        Ok(resolved)
    }
    fn path(&self, name: &str) -> Result<PathBuf> {
        anyhow::ensure!(
            !name.is_empty()
                && name.len() <= 128
                && name
                    .bytes()
                    .all(|v| v.is_ascii_alphanumeric() || matches!(v, b'-' | b'_')),
            "Invalid supervised unit name"
        );
        Ok(self.unit_dir.join(format!("{name}.container")))
    }
    async fn manager(args: &[&str]) -> Result<String> {
        let output = tokio::time::timeout(
            Duration::from_secs(120),
            tokio::process::Command::new("systemctl")
                .arg("--user")
                .args(args)
                .kill_on_drop(true)
                .output(),
        )
        .await
        .context("User service manager timed out")??;
        anyhow::ensure!(
            output.status.success(),
            "User service manager rejected operation"
        );
        Ok(String::from_utf8(output.stdout)?)
    }
    async fn owned_file(&self, name: &str) -> Result<(PathBuf, std::fs::Metadata)> {
        let expected = self.path(name)?;
        let service = format!("{name}.service");
        let output = Self::manager(&[
            "show",
            &service,
            "--property=SourcePath",
            "--property=DropInPaths",
            "--property=LoadState",
            "--property=Restart",
        ])
        .await?;
        let values: HashMap<_, _> = output
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect();
        anyhow::ensure!(
            values.get("LoadState") == Some(&"loaded")
                && values
                    .get("SourcePath")
                    .is_some_and(|path| Path::new(path) == expected),
            "Service is not owned by the exact original source Quadlet or has external overrides"
        );
        validate_maintenance_override(
            &self.data_dir,
            &PathBuf::from(format!("/run/user/{}", unsafe { libc::geteuid() })),
            name,
            values
                .get("DropInPaths")
                .context("Missing service override inventory")?,
            values
                .get("Restart")
                .context("Missing service restart policy")?,
        )?;
        let meta = std::fs::symlink_metadata(&expected)?;
        anyhow::ensure!(
            meta.is_file()
                && !meta.file_type().is_symlink()
                && meta.uid() == unsafe { libc::geteuid() }
                && meta.mode() & 0o022 == 0
                && meta.len() <= 1024 * 1024,
            "Original Quadlet ownership changed"
        );
        Ok((expected, meta))
    }
}
impl<B: DrainBarrier> Supervisor for SystemdSupervisor<B> {
    async fn begin_barrier(
        &self,
        operation: &str,
        originals: &[Unit],
        recovery: bool,
    ) -> Result<()> {
        self.barrier.acquire(operation, originals, recovery).await
    }
    async fn verify_barrier(&self, operation: &str) -> Result<()> {
        self.barrier.verify(operation).await
    }
    async fn release_barrier(&self, operation: &str, outcome: Completion) -> Result<()> {
        self.barrier.release(operation, outcome).await
    }
    async fn prepare_target(&self, target: &Target, original: &Unit) -> Result<PreparedTarget> {
        let plan = self
            .plans
            .get(&target.name)
            .context("No reviewed original-bound managed migration; app unchanged")?;
        anyhow::ensure!(
            hex::encode(Sha256::digest(original.body.as_bytes())) == plan.original_sha256
                && plan.prepared.manifest.app.container.image.as_deref()
                    == Some(target.reference.as_str())
                && super::prod_orchestrator::compute_container_name(&plan.prepared.manifest)
                    == target.name,
            "Original unit or reviewed immutable target changed before update"
        );
        if plan
            .prepared
            .manifest
            .app
            .container
            .media_registration_identity
        {
            let identity =
                crate::identity::NodeIdentity::load_existing(&self.data_dir.join("identity"))
                    .await?;
            let pin = super::registration_pin::load_existing(
                &self.data_dir,
                &plan.prepared.manifest.app.id,
                &identity,
            )?;
            let mut expected = plan.prepared.manifest.clone();
            super::registration_pin::apply_environment(&mut expected, &pin)?;
            for entry in expected
                .app
                .environment
                .iter()
                .filter(|entry| entry.starts_with("ARCHIPELAGO_REGISTRATION_"))
            {
                let key = entry
                    .split_once('=')
                    .context("Malformed registration binding")?
                    .0;
                let in_manifest: Vec<_> = plan
                    .prepared
                    .manifest
                    .app
                    .environment
                    .iter()
                    .filter(|value| value.split_once('=').is_some_and(|(name, _)| name == key))
                    .collect();
                let in_unit: Vec<_> = plan
                    .prepared
                    .body
                    .lines()
                    .filter_map(|line| line.trim().strip_prefix("Environment="))
                    .map(|value| value.trim_matches('"'))
                    .filter(|value| value.split_once('=').is_some_and(|(name, _)| name == key))
                    .collect();
                anyhow::ensure!(
                    in_manifest.len() == 1
                        && in_manifest[0] == entry
                        && in_unit.len() == 1
                        && in_unit[0] == entry,
                    "Reviewed registration identity does not match the existing installer pin"
                );
            }
        }
        Ok(plan.prepared.clone())
    }
    async fn target_hooks(
        &self,
        name: &str,
        manifest: &archipelago_container::AppManifest,
    ) -> Result<()> {
        super::hooks::run_post_install_strict(manifest, name, &self.data_dir).await
    }
    async fn capture(&self, name: &str) -> Result<Unit> {
        let (path, meta) = self.owned_file(name).await?;
        let body = std::fs::read_to_string(path)?;
        let observed = Podman
            .inspect(name)
            .await?
            .context("Original supervised runtime missing")?;
        Ok(Unit {
            name: name.into(),
            body,
            image: observed.image,
            container_id: observed.id,
            file_mode: meta.mode() & 0o777,
            running: observed.running,
            config_sha256: observed.config_sha256,
            launch_config_sha256_v2: Some(
                observed
                    .launch_config_sha256_v2
                    .context("Original launch configuration is ambiguous or incomplete")?,
            ),
        })
    }
    async fn validate_original_file(&self, original: &Unit) -> Result<()> {
        let (_, meta) = self.owned_file(&original.name).await?;
        anyhow::ensure!(
            meta.mode() & 0o777 == original.file_mode,
            "Original Quadlet mode changed"
        );
        Ok(())
    }
    async fn read(&self, name: &str) -> Result<String> {
        let (path, _) = self.owned_file(name).await?;
        Ok(std::fs::read_to_string(path)?)
    }
    async fn write(&self, original: &Unit, expected: &[String], body: &str) -> Result<()> {
        let (path, meta) = self.owned_file(&original.name).await?;
        anyhow::ensure!(
            meta.mode() & 0o777 == original.file_mode
                && expected.contains(&std::fs::read_to_string(&path)?),
            "Original Quadlet changed; refusing to replace a foreign edit"
        );
        let temporary = self
            .unit_dir
            .join(format!(".archy-update-{}.tmp", uuid::Uuid::new_v4()));
        // Commit point is synchronous while the transaction guard is held;
        // cancelled futures cannot rename over a later recovery after unlock.
        let result = (|| -> Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(original.file_mode)
                .open(&temporary)?;
            file.write_all(body.as_bytes())?;
            file.set_permissions(std::fs::Permissions::from_mode(original.file_mode))?;
            file.sync_all()?;
            std::fs::rename(&temporary, &path)?;
            std::fs::File::open(&self.unit_dir)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
    async fn snapshot(&self, original: &Unit, operation: &str, tag: &str) -> Result<RecoveryImage> {
        // Pre-maintenance image capture preserves code and hook mutations;
        // mounted-data consistency is established separately by acquire().
        supervised_update::capture_local_recovery_image(original, operation, tag).await
    }
    async fn pin(&self, image: &str, tag: &str) -> Result<()> {
        anyhow::ensure!(
            image.len() == 64
                && image.bytes().all(|v| v.is_ascii_hexdigit())
                && tag.starts_with("localhost/archy-update-recovery:"),
            "Invalid recovery image pin"
        );
        let output = tokio::time::timeout(
            Duration::from_secs(30),
            tokio::process::Command::new("podman")
                .args(["tag", &format!("sha256:{image}"), tag])
                .kill_on_drop(true)
                .output(),
        )
        .await??;
        anyhow::ensure!(
            output.status.success(),
            "Original recovery image unavailable"
        );
        Ok(())
    }
    async fn stop(&self, name: &str) -> Result<()> {
        self.owned_file(name).await?;
        super::quadlet::stop_service_with_timeout(
            &format!("{name}.service"),
            Duration::from_secs(archipelago_container::runtime::stop_grace_secs_for(name) + 30),
        )
        .await
    }
    async fn reload(&self) -> Result<()> {
        super::quadlet::daemon_reload_user().await
    }
    async fn start(&self, name: &str) -> Result<()> {
        self.owned_file(name).await?;
        super::quadlet::enable_now(&format!("{name}.service")).await
    }
    async fn observed(&self, name: &str) -> Result<Option<Observed>> {
        Podman.inspect(name).await
    }
    async fn healthy(&self, name: &str) -> Result<bool> {
        Podman.healthy(name).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct OverrideFixture {
        name: &'static str,
        _root: tempfile::TempDir,
        data: PathBuf,
        runtime: PathBuf,
        override_path: PathBuf,
        native_path: PathBuf,
        journal_path: PathBuf,
    }
    fn private_write(path: &Path, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    impl OverrideFixture {
        fn new() -> Self {
            Self::for_role("indeedhub-api", "api_restart_override")
        }
        fn for_role(name: &'static str, obligation: &str) -> Self {
            let root = tempfile::tempdir().unwrap();
            let data = root.path().join("data");
            let runtime = root.path().join("runtime");
            let operation = uuid::Uuid::new_v4().to_string();
            let maintenance = data
                .join("update-transactions/indeehub-maintenance")
                .join(&operation);
            for path in [
                data.join("update-transactions/holds"),
                data.join("update-transactions/supervised"),
                maintenance.clone(),
                data.join("app-maintenance"),
                runtime
                    .join("systemd/user")
                    .join(format!("{name}.service.d")),
            ] {
                std::fs::create_dir_all(path).unwrap();
            }
            private_write(
                &data.join("update-transactions/holds").join(name),
                operation.as_bytes(),
            );
            private_write(
                &data.join("app-maintenance/indeedhub"),
                operation.as_bytes(),
            );
            let body = format!("[Container]\nContainerName={name}\n");
            let original = serde_json::json!({"name":name,"container_id":"a".repeat(64),"image":"b".repeat(64),"config_sha256":"c".repeat(64),"body":body,"running":true});
            let native = serde_json::json!({"schema":2,"id":operation,"package":"indeedhub","phase":"Restoring","cleanup_done":false,"members":[{"original":original}]});
            let bytes =
                format!("# Archipelago maintenance operation {operation}\n[Service]\nRestart=no\n");
            let journal = serde_json::json!({"operation_id":operation,"phase":"Recovering","original_members":[{"name":name,"container_id":original["container_id"],"image_id":original["image"],"config_sha256":original["config_sha256"],"unit_sha256":hex::encode(Sha256::digest(body.as_bytes())),"running":true}],(obligation):{"operation_id":operation,"released":false,"sha256":hex::encode(Sha256::digest(bytes.as_bytes()))}});
            let override_path = runtime
                .join("systemd/user")
                .join(format!("{name}.service.d"))
                .join(format!("zz-archipelago-maintenance-{operation}.conf"));
            let native_path = data
                .join("update-transactions/supervised")
                .join(format!("{operation}.json"));
            let journal_path = maintenance.join("journal.json");
            private_write(&override_path, bytes.as_bytes());
            private_write(&native_path, &serde_json::to_vec(&native).unwrap());
            private_write(&journal_path, &serde_json::to_vec(&journal).unwrap());
            Self {
                name,
                _root: root,
                data,
                runtime,
                override_path,
                native_path,
                journal_path,
            }
        }
        fn validate(&self) -> Result<()> {
            validate_maintenance_override(
                &self.data,
                &self.runtime,
                self.name,
                self.override_path.to_str().unwrap(),
                "no",
            )
        }
    }
    #[test]
    fn only_exact_active_api_restart_override_is_owned() {
        let f = OverrideFixture::new();
        f.validate().unwrap();
        for (name, paths, restart) in [
            (
                "indeedhub-ffmpeg",
                f.override_path.display().to_string(),
                "no",
            ),
            (
                "indeedhub-api",
                format!("{} /other.conf", f.override_path.display()),
                "no",
            ),
            (
                "indeedhub-api",
                f.override_path.display().to_string(),
                "always",
            ),
        ] {
            assert!(
                validate_maintenance_override(&f.data, &f.runtime, name, &paths, restart).is_err()
            );
        }
        // Services without overrides preserve the existing ordinary ownership path.
        validate_maintenance_override(
            Path::new("/absent"),
            Path::new("/absent"),
            "other",
            "",
            "always",
        )
        .unwrap();
    }
    #[test]
    fn api_and_relay_overrides_cannot_borrow_each_others_obligation() {
        for (name, own, other) in [
            (
                "indeedhub-api",
                "api_restart_override",
                "relay_restart_override",
            ),
            (
                "indeedhub-relay",
                "relay_restart_override",
                "api_restart_override",
            ),
        ] {
            let f = OverrideFixture::for_role(name, own);
            f.validate().unwrap();
            let bytes = std::fs::read(&f.journal_path).unwrap();
            let mut journal: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let value = journal.as_object_mut().unwrap().remove(own).unwrap();
            journal[other] = value;
            private_write(&f.journal_path, &serde_json::to_vec(&journal).unwrap());
            assert!(f.validate().is_err());
            private_write(&f.journal_path, &bytes);
            let borrowed = f.override_path.to_str().unwrap().replace(
                name,
                if name == "indeedhub-api" {
                    "indeedhub-relay"
                } else {
                    "indeedhub-api"
                },
            );
            assert!(
                validate_maintenance_override(&f.data, &f.runtime, name, &borrowed, "no").is_err()
            );
        }
    }
    #[test]
    fn restart_override_requires_matching_live_native_and_controller_obligations() {
        for (native, key, value) in [
            (
                true,
                "/id",
                serde_json::json!(uuid::Uuid::new_v4().to_string()),
            ),
            (true, "/cleanup_done", serde_json::json!(true)),
            (true, "/phase", serde_json::json!("Aborted")),
            (
                true,
                "/members/0/original/container_id",
                serde_json::json!("d".repeat(64)),
            ),
            (
                false,
                "/operation_id",
                serde_json::json!(uuid::Uuid::new_v4().to_string()),
            ),
            (false, "/phase", serde_json::json!("Released")),
            (
                false,
                "/api_restart_override/released",
                serde_json::json!(true),
            ),
            (
                false,
                "/api_restart_override/sha256",
                serde_json::json!("d".repeat(64)),
            ),
        ] {
            let f = OverrideFixture::new();
            let path = if native {
                &f.native_path
            } else {
                &f.journal_path
            };
            let mut record: serde_json::Value =
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            *record.pointer_mut(key).unwrap() = value;
            private_write(path, &serde_json::to_vec(&record).unwrap());
            assert!(f.validate().is_err(), "Accepted changed binding: {key}");
        }
        let f = OverrideFixture::new();
        private_write(
            &f.data.join("app-maintenance/indeedhub"),
            uuid::Uuid::new_v4().to_string().as_bytes(),
        );
        assert!(f.validate().is_err());
    }
    #[test]
    fn restart_override_refuses_tamper_symlink_and_writable_ownership_paths() {
        let f = OverrideFixture::new();
        let original = std::fs::read(&f.override_path).unwrap();
        private_write(&f.override_path, b"[Service]\nRestart=always\n");
        assert!(f.validate().is_err());
        private_write(&f.override_path, &original);
        std::fs::set_permissions(&f.override_path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(f.validate().is_err());
        private_write(&f.override_path, &original);
        let foreign = f.runtime.join("foreign");
        private_write(&foreign, &original);
        std::fs::remove_file(&f.override_path).unwrap();
        std::os::unix::fs::symlink(&foreign, &f.override_path).unwrap();
        assert!(f.validate().is_err());
        assert_eq!(std::fs::read(&foreign).unwrap(), original);
        std::fs::remove_file(&f.override_path).unwrap();
        private_write(&f.override_path, &original);
        std::fs::set_permissions(
            f.override_path.parent().unwrap(),
            std::fs::Permissions::from_mode(0o777),
        )
        .unwrap();
        assert!(f.validate().is_err());
        std::fs::set_permissions(
            f.override_path.parent().unwrap(),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        f.validate().unwrap();
        let real = f.journal_path.with_extension("owned");
        std::fs::rename(&f.journal_path, &real).unwrap();
        std::os::unix::fs::symlink(&real, &f.journal_path).unwrap();
        assert!(f.validate().is_err());
    }
    struct UnusedBarrier;
    impl DrainBarrier for UnusedBarrier {
        async fn acquire(&self, _: &str, _: &[Unit], _: bool) -> Result<()> {
            anyhow::bail!("No application barrier installed")
        }
        async fn verify(&self, _: &str) -> Result<()> {
            anyhow::bail!("No application barrier installed")
        }
        async fn release(&self, _: &str, _: Completion) -> Result<()> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn original_bound_plan_rejects_changed_unit_or_image_without_service_calls() {
        let image = format!("localhost/movie@sha256:{}", "b".repeat(64));
        let manifest = archipelago_container::AppManifest::parse(&format!(
            "app:\n  id: movie\n  name: Movie\n  version: 2.0.0\n  container:\n    image: {image}\n")).unwrap();
        let original_body = "[Container]\nContainerName=movie\nImage=old\n";
        let next_body = format!("[Container]\nContainerName=movie\nImage={image}\n");
        let plan = MigrationPlan::from_reviewed_legacy(
            &hex::encode(Sha256::digest(original_body)),
            next_body.clone(),
            manifest,
        )
        .unwrap();
        let adapter = SystemdSupervisor {
            data_dir: PathBuf::from("/unused"),
            unit_dir: PathBuf::from("/unused"),
            plans: HashMap::from([("movie".into(), plan)]),
            barrier: UnusedBarrier,
        };
        let mut original = Unit {
            name: "movie".into(),
            body: original_body.into(),
            image: "a".repeat(64),
            container_id: "c".repeat(64),
            file_mode: 0o600,
            running: true,
            config_sha256: "d".repeat(64),
            launch_config_sha256_v2: None,
        };
        let mut target = Target {
            name: "movie".into(),
            reference: image,
            image: "b".repeat(64),
        };
        assert_eq!(
            adapter
                .prepare_target(&target, &original)
                .await
                .unwrap()
                .body,
            next_body
        );
        original.body.push_str("Environment=OPERATOR=changed\n");
        assert!(adapter.prepare_target(&target, &original).await.is_err());
        original.body = original_body.into();
        target.reference = format!("localhost/movie@sha256:{}", "e".repeat(64));
        assert!(adapter.prepare_target(&target, &original).await.is_err());
        assert!(adapter
            .begin_barrier("unused", &[original], false)
            .await
            .is_err());
    }
}
