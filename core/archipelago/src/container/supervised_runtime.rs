//! Production systemd/Podman adapter. Application write admission/drain is an
//! explicit dependency: neither process pause nor a filesystem receipt is drain.
use super::{
    supervised_update::{self, PreparedTarget, RecoveryImage, Supervisor, Unit},
    update_transaction::{Observed, Podman, Runtime, Target},
};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    future::Future,
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) trait DrainBarrier: Sync {
    /// Persist ownership before blocking admissions. Return only after API,
    /// direct uploads and workers have drained and the coherent backup finished.
    fn acquire(
        &self,
        operation: &str,
        originals: &[Unit],
    ) -> impl Future<Output = Result<()>> + Send;
    /// Must inspect the live admission fence and same-operation ownership, not
    /// merely trust an old 'backup complete' file. Keep it through snapshot/stop.
    fn verify(&self, operation: &str) -> impl Future<Output = Result<()>> + Send;
    /// Idempotent; an old recovery must never clear a newer operation's fence.
    fn release(&self, operation: &str) -> impl Future<Output = Result<()>> + Send;
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
        ])
        .await?;
        let values: HashMap<_, _> = output
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect();
        anyhow::ensure!(
            values.get("LoadState") == Some(&"loaded")
                && values.get("DropInPaths") == Some(&"")
                && values
                    .get("SourcePath")
                    .is_some_and(|path| Path::new(path) == expected),
            "Service is not owned by the exact original source Quadlet or has external overrides"
        );
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
    async fn begin_barrier(&self, operation: &str, originals: &[Unit]) -> Result<()> {
        self.barrier.acquire(operation, originals).await
    }
    async fn verify_barrier(&self, operation: &str) -> Result<()> {
        self.barrier.verify(operation).await
    }
    async fn release_barrier(&self, operation: &str) -> Result<()> {
        self.barrier.release(operation).await
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
        self.barrier.verify(operation).await?;
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
    struct UnusedBarrier;
    impl DrainBarrier for UnusedBarrier {
        async fn acquire(&self, _: &str, _: &[Unit]) -> Result<()> {
            anyhow::bail!("No application barrier installed")
        }
        async fn verify(&self, _: &str) -> Result<()> {
            anyhow::bail!("No application barrier installed")
        }
        async fn release(&self, _: &str) -> Result<()> {
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
        assert!(adapter.begin_barrier("unused", &[original]).await.is_err());
    }
}
