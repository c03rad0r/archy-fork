//! Quadlet recovery preserves exact original launch configuration and image, not
//! ephemeral --rm container IDs. Persistent application data is never rolled back.
use super::update_transaction::{Guard, Observed, Target};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct Unit {
    pub name: String,
    pub body: String,
    pub image: String,
    pub container_id: String,
    pub running: bool,
    pub config_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PreparedTarget {
    pub body: String,
    pub manifest: archipelago_container::AppManifest,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RecoveryImage {
    pub image: String,
    pub source_container_id: String,
    pub operation_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Member {
    original: Unit,
    target: Target,
    target_body: String,
    target_manifest: archipelago_container::AppManifest,
    pinned_original_body: String,
    original_tag: String,
    recovery_image: Option<RecoveryImage>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Phase {
    Prepared,
    Aborted,
    Editing,
    Starting,
    Committed,
    Restored,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u8,
    id: String,
    package: String,
    phase: Phase,
    members: Vec<Member>,
}

pub(crate) trait Supervisor: Sync {
    /// Only an internal reviewed signed-manifest planner may supply this value;
    /// browser parameters must never become a unit body or hook recipe.
    fn prepare_target(
        &self,
        target: &Target,
        original: &Unit,
    ) -> impl Future<Output = Result<PreparedTarget>> + Send;
    fn target_hooks(
        &self,
        name: &str,
        manifest: &archipelago_container::AppManifest,
    ) -> impl Future<Output = Result<()>> + Send;

    /// Commit the exact original writable layer to a local-only operation-owned
    /// image, explicitly pausing and excluding mounted volumes. A retry must
    /// recover its matching image rather than overwrite an unrelated tag.
    /// This does not establish application-level write quiescence or DB backup.
    fn snapshot(
        &self,
        original: &Unit,
        operation_id: &str,
        tag: &str,
    ) -> impl Future<Output = Result<RecoveryImage>> + Send;
    fn capture(&self, name: &str) -> impl Future<Output = Result<Unit>> + Send;
    fn read(&self, name: &str) -> impl Future<Output = Result<String>> + Send;
    fn write(
        &self,
        name: &str,
        expected: &[String],
        body: &str,
    ) -> impl Future<Output = Result<()>> + Send;
    fn pin(&self, image: &str, tag: &str) -> impl Future<Output = Result<()>> + Send;
    fn stop(&self, name: &str) -> impl Future<Output = Result<()>> + Send;
    fn reload(&self) -> impl Future<Output = Result<()>> + Send;
    fn start(&self, name: &str) -> impl Future<Output = Result<()>> + Send;
    fn observed(&self, name: &str) -> impl Future<Output = Result<Option<Observed>>> + Send;
    fn healthy(&self, name: &str) -> impl Future<Output = Result<bool>> + Send;
}
fn simple(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
fn digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
/// Parse only the renderer-owned unit shape. Ambiguous images, includes and
/// external environment files cannot become a guessed recovery recipe.
pub(crate) fn pin_body(body: &str, name: &str, image: &str) -> Result<String> {
    anyhow::ensure!(
        body.len() <= 1024 * 1024 && simple(name),
        "Invalid original unit"
    );
    let mut section = "";
    let mut images = 0;
    let mut names = 0;
    let mut output = String::new();
    for line in body.lines() {
        let trimmed = line.trim();
        anyhow::ensure!(
            !trimmed.ends_with('\\')
                && !trimmed.starts_with(".include")
                && !trimmed.starts_with("EnvironmentFile=")
                && !trimmed.starts_with("EnvFile="),
            "Unit has external/continued configuration; exact recovery is not supported yet"
        );
        if trimmed.starts_with('[') {
            section = trimmed;
        }
        if section == "[Container]" && trimmed.starts_with("Image=") {
            images += 1;
            output.push_str(&format!("Image={image}\n"));
        } else {
            if section == "[Container]" && trimmed.starts_with("ContainerName=") {
                names += 1;
                anyhow::ensure!(
                    trimmed == format!("ContainerName={name}"),
                    "Unit container ownership mismatch"
                );
            }
            output.push_str(line);
            output.push('\n');
        }
    }
    anyhow::ensure!(
        images == 1 && names == 1,
        "Original Quadlet must bind one container and image"
    );
    Ok(output)
}
fn root(guard: &Guard) -> Result<PathBuf> {
    let path = guard.directory().join("supervised");
    match std::fs::DirBuilder::new().mode(0o700).create(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    anyhow::ensure!(
        !std::fs::symlink_metadata(&path)?.file_type().is_symlink(),
        "Invalid supervised journal directory"
    );
    Ok(path)
}
fn validate(record: &Journal) -> Result<()> {
    anyhow::ensure!(
        record.schema == 1
            && simple(&record.package)
            && uuid::Uuid::parse_str(&record.id)?.to_string() == record.id
            && !record.members.is_empty()
            && record.members.len() <= 32,
        "Invalid supervised update journal"
    );
    let mut names = std::collections::HashSet::new();
    for (index, member) in record.members.iter().enumerate() {
        anyhow::ensure!(
            simple(&member.original.name)
                && names.insert(&member.original.name)
                && member.target.name == member.original.name
                && digest(&member.target.image)
                && digest(&member.original.image)
                && digest(&member.original.container_id)
                && digest(&member.original.config_sha256),
            "Invalid original supervised identity"
        );
        anyhow::ensure!(
            member.original_tag == format!("localhost/archy-update-recovery:{}-{index}", record.id),
            "Recovery image pin changed"
        );
        if let Some(image) = &member.recovery_image {
            anyhow::ensure!(
                digest(&image.image)
                    && image.source_container_id == member.original.container_id
                    && image.operation_id == record.id,
                "Recovery image ownership changed"
            );
        }
        anyhow::ensure!(
            matches!(record.phase, Phase::Prepared | Phase::Aborted)
                || member.recovery_image.is_some(),
            "Destructive update lacks a durable writable-layer recovery image"
        );
        let restore_image = member
            .recovery_image
            .as_ref()
            .map(|value| value.image.as_str())
            .unwrap_or(&member.original.image);
        anyhow::ensure!(
            member.pinned_original_body
                == pin_body(
                    &member.original.body,
                    &member.original.name,
                    &format!("sha256:{restore_image}")
                )?
                && member.target_body
                    == pin_body(
                        &member.target_body,
                        &member.original.name,
                        &member.target.reference
                    )?
                && member.target_manifest.app.container.image.as_deref()
                    == Some(member.target.reference.as_str()),
            "Saved unit recipe changed"
        );
    }
    Ok(())
}
fn save(guard: &Guard, record: &Journal) -> Result<()> {
    validate(record)?;
    let dir = root(guard)?;
    let bytes = serde_json::to_vec(record)?;
    anyhow::ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "Supervised recovery journal too large"
    );
    let temporary = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, dir.join(format!("{}.json", record.id)))?;
        std::fs::File::open(&dir)?.sync_all()?;
        std::fs::File::open(guard.directory())?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}
fn records(guard: &Guard) -> Result<Vec<Journal>> {
    let dir = root(guard)?;
    let mut records = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.path().extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        anyhow::ensure!(
            records.len() < 128
                && entry.file_type()?.is_file()
                && entry.metadata()?.len() <= 4 * 1024 * 1024,
            "Invalid supervised recovery inventory"
        );
        let record: Journal = serde_json::from_slice(&std::fs::read(entry.path())?)?;
        validate(&record)?;
        anyhow::ensure!(
            entry.file_name() == format!("{}.json", record.id).as_str(),
            "Supervised journal name changed"
        );
        records.push(record);
    }
    Ok(records)
}
pub(crate) fn require_clear(guard: &Guard) -> Result<()> {
    anyhow::ensure!(
        records(guard)?
            .iter()
            .all(|r| matches!(r.phase, Phase::Committed | Phase::Restored | Phase::Aborted)),
        "A supervised update needs recovery first"
    );
    Ok(())
}
pub(crate) async fn execute(
    guard: &Guard,
    package: &str,
    targets: &[Target],
    supervisor: &impl Supervisor,
) -> Result<()> {
    guard.require_clear()?;
    anyhow::ensure!(
        !targets.is_empty() && targets.len() <= 32,
        "Invalid supervised stack"
    );
    let id = uuid::Uuid::new_v4().to_string();
    let mut members = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        let original = supervisor.capture(&target.name).await?;
        // Inactive units require a durable explicit-start staging path; never
        // implement this by starting and then stopping a user's stopped member.
        anyhow::ensure!(original.running,"Stopped supervised member requires staged update; all original services remain unchanged");
        let prepared = supervisor.prepare_target(target, &original).await?;
        anyhow::ensure!(
            prepared.manifest.app.container.image.as_deref() == Some(target.reference.as_str()),
            "Reviewed target manifest image changed"
        );
        let target_body = pin_body(&prepared.body, &original.name, &target.reference)?;
        anyhow::ensure!(
            target_body == prepared.body,
            "Reviewed target unit image changed"
        );
        let pinned_original_body = pin_body(
            &original.body,
            &original.name,
            &format!("sha256:{}", original.image),
        )?;
        members.push(Member {
            original,
            target: target.clone(),
            target_body,
            target_manifest: prepared.manifest,
            pinned_original_body,
            original_tag: format!("localhost/archy-update-recovery:{id}-{index}"),
            recovery_image: None,
        });
    }
    let mut record = Journal {
        schema: 1,
        id,
        package: package.into(),
        phase: Phase::Prepared,
        members,
    };
    save(guard, &record)?;
    for member in &record.members {
        guard.hold(&member.original.name, &record.id)?;
    }
    let result = apply(guard, &mut record, supervisor).await;
    if let Err(error) = result {
        return match restore(guard,&mut record,supervisor).await {
            Ok(())=>Err(error.context("Original supervised image/configuration and running intent restored; container IDs may change and data was not rolled back")),
            Err(recovery)=>Err(error.context(format!("Supervised runtime recovery remains unresolved: {recovery:#}"))),
        };
    }
    Ok(())
}
async fn apply(guard: &Guard, record: &mut Journal, supervisor: &impl Supervisor) -> Result<()> {
    for index in 0..record.members.len() {
        let member = &record.members[index];
        anyhow::ensure!(
            supervisor.read(&member.original.name).await? == member.original.body,
            "Unit edited before update; originals retained"
        );
        let image = supervisor
            .snapshot(&member.original, &record.id, &member.original_tag)
            .await?;
        anyhow::ensure!(
            digest(&image.image)
                && image.source_container_id == member.original.container_id
                && image.operation_id == record.id,
            "Writable-layer recovery ownership mismatch"
        );
        let member = &mut record.members[index];
        member.pinned_original_body = pin_body(
            &member.original.body,
            &member.original.name,
            &format!("sha256:{}", image.image),
        )?;
        member.recovery_image = Some(image);
        // The image acknowledgement becomes durable before any original stop.
        save(guard, record)?;
    }
    record.phase = Phase::Editing;
    save(guard, record)?;
    for member in record.members.iter().rev() {
        supervisor.stop(&member.original.name).await?;
    }
    for member in &record.members {
        supervisor
            .write(
                &member.original.name,
                &[member.original.body.clone()],
                &member.target_body,
            )
            .await?;
    }
    supervisor.reload().await?;
    record.phase = Phase::Starting;
    save(guard, record)?;
    for member in &record.members {
        supervisor.start(&member.original.name).await?;
        supervisor
            .target_hooks(&member.original.name, &member.target_manifest)
            .await?;
        let observed = supervisor
            .observed(&member.original.name)
            .await?
            .context("Updated supervised member missing")?;
        anyhow::ensure!(
            observed.running
                && observed.image == member.target.image
                && supervisor.healthy(&member.original.name).await?,
            "Updated supervised member failed verification"
        );
    }
    record.phase = Phase::Committed;
    save(guard, record)?;
    for member in &record.members {
        guard.release_hold(&member.original.name, &record.id)?;
    }
    Ok(())
}
async fn restore(guard: &Guard, record: &mut Journal, supervisor: &impl Supervisor) -> Result<()> {
    if record.phase == Phase::Prepared {
        // Only image snapshots may have happened. Never stop/recreate an intact
        // app just because preflight or snapshotting failed on another member.
        for member in &record.members {
            let original = supervisor
                .observed(&member.original.name)
                .await?
                .context("Original service disappeared during preparation")?;
            anyhow::ensure!(
                supervisor.read(&member.original.name).await? == member.original.body
                    && original.id == member.original.container_id
                    && original.image == member.original.image
                    && original.running == member.original.running
                    && original.config_sha256 == member.original.config_sha256,
                "Original service changed during preparation; recovery requires inspection"
            );
        }
        record.phase = Phase::Aborted;
        save(guard, record)?;
        for member in &record.members {
            guard.release_hold(&member.original.name, &record.id)?;
        }
        return Ok(());
    }
    // Refuse to overwrite a foreign edit before stopping any surviving member.
    for member in &record.members {
        let body = supervisor.read(&member.original.name).await?;
        anyhow::ensure!(
            [
                &member.original.body,
                &member.target_body,
                &member.pinned_original_body
            ]
            .contains(&&body),
            "Foreign unit edit requires explicit recovery"
        );
        supervisor
            .pin(
                &member
                    .recovery_image
                    .as_ref()
                    .context("Missing recovery image")?
                    .image,
                &member.original_tag,
            )
            .await?;
    }
    for member in record.members.iter().rev() {
        supervisor.stop(&member.original.name).await?;
    }
    for member in &record.members {
        supervisor
            .write(
                &member.original.name,
                &[
                    member.original.body.clone(),
                    member.target_body.clone(),
                    member.pinned_original_body.clone(),
                ],
                &member.pinned_original_body,
            )
            .await?;
    }
    supervisor.reload().await?;
    for member in &record.members {
        if member.original.running {
            supervisor.start(&member.original.name).await?;
        }
        let observed = supervisor.observed(&member.original.name).await?;
        if member.original.running {
            let current = observed.context("Original supervised service did not return")?;
            anyhow::ensure!(
                current.running
                    && current.image
                        == member
                            .recovery_image
                            .as_ref()
                            .context("Missing recovery image")?
                            .image
                    && current.config_sha256 == member.original.config_sha256,
                "Original launch configuration did not recover"
            );
        } else {
            anyhow::ensure!(
                observed.is_none_or(|v| !v.running),
                "Originally stopped service unexpectedly running"
            );
        }
    }
    for member in &record.members {
        guard.hold(&member.original.name, &record.id)?;
    }
    record.phase = Phase::Restored;
    save(guard, record)
}
pub(crate) async fn recover(guard: &Guard, supervisor: &impl Supervisor) -> Result<()> {
    for mut record in records(guard)? {
        match record.phase {
            Phase::Committed | Phase::Aborted => {
                for member in &record.members {
                    guard.release_hold(&member.original.name, &record.id)?;
                }
            }
            Phase::Restored => {} // A later retry may own the current hold.
            _ => restore(guard, &mut record, supervisor).await?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    };
    struct Mock {
        original: Unit,
        body: Mutex<String>,
        running: AtomicBool,
        calls: Mutex<Vec<String>>,
        fail_new_hooks: AtomicBool,
        fail_snapshot: AtomicBool,
        generation: AtomicUsize,
    }
    impl Mock {
        fn new() -> Self {
            let body=format!("[Container]\nContainerName=movie\nImage=sha256:{}\nEnvironment=OPERATOR_VALUE=retained\nPull=never\n[Service]\nRestart=always\n", "a".repeat(64));
            Self {
                original: Unit {
                    name: "movie".into(),
                    body: body.clone(),
                    image: "a".repeat(64),
                    container_id: format!("{:064x}", 1),
                    running: true,
                    config_sha256: "c".repeat(64),
                },
                body: Mutex::new(body),
                running: AtomicBool::new(true),
                calls: Default::default(),
                fail_new_hooks: AtomicBool::new(false),
                fail_snapshot: AtomicBool::new(false),
                generation: AtomicUsize::new(1),
            }
        }
        fn target() -> Target {
            Target {
                name: "movie".into(),
                reference: format!("localhost/new@sha256:{}", "b".repeat(64)),
                image: "b".repeat(64),
            }
        }
    }
    impl Supervisor for Mock {
        async fn prepare_target(
            &self,
            target: &Target,
            _original: &Unit,
        ) -> Result<PreparedTarget> {
            let manifest = archipelago_container::AppManifest::parse(&format!(
                "app:\n  id: movie\n  name: Movie\n  version: 2.0.0\n  container:\n    image: {}\n",
                target.reference
            ))?;
            let body = pin_body(&self.original.body, "movie", &target.reference)?.replace(
                "Pull=never",
                "Environment=NEW_FEATURE=enabled\nVolume=/identity:/run/identity:ro\nPull=never",
            );
            Ok(PreparedTarget { body, manifest })
        }
        async fn target_hooks(
            &self,
            _name: &str,
            _manifest: &archipelago_container::AppManifest,
        ) -> Result<()> {
            self.calls.lock().unwrap().push("new-hooks".into());
            anyhow::ensure!(
                !self.fail_new_hooks.load(Ordering::SeqCst),
                "New provider hook failed"
            );
            Ok(())
        }
        async fn snapshot(
            &self,
            original: &Unit,
            operation_id: &str,
            _tag: &str,
        ) -> Result<RecoveryImage> {
            self.calls.lock().unwrap().push("snapshot-original".into());
            anyhow::ensure!(
                !self.fail_snapshot.load(Ordering::SeqCst),
                "Snapshot failed"
            );
            Ok(RecoveryImage {
                image: "e".repeat(64),
                source_container_id: original.container_id.clone(),
                operation_id: operation_id.into(),
            })
        }
        async fn capture(&self, _name: &str) -> Result<Unit> {
            Ok(self.original.clone())
        }
        async fn read(&self, _name: &str) -> Result<String> {
            Ok(self.body.lock().unwrap().clone())
        }
        async fn write(&self, _name: &str, expected: &[String], body: &str) -> Result<()> {
            let mut current = self.body.lock().unwrap();
            anyhow::ensure!(expected.contains(&*current), "Foreign unit edit");
            *current = body.into();
            self.calls.lock().unwrap().push("write".into());
            Ok(())
        }
        async fn pin(&self, _image: &str, _tag: &str) -> Result<()> {
            self.calls.lock().unwrap().push("pin-original".into());
            Ok(())
        }
        async fn stop(&self, _name: &str) -> Result<()> {
            self.calls.lock().unwrap().push("stop-unit".into());
            self.running.store(false, Ordering::SeqCst);
            Ok(())
        }
        async fn reload(&self) -> Result<()> {
            self.calls.lock().unwrap().push("reload".into());
            Ok(())
        }
        async fn start(&self, _name: &str) -> Result<()> {
            self.calls.lock().unwrap().push("start-unit".into());
            self.running.store(true, Ordering::SeqCst);
            self.generation.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn observed(&self, _name: &str) -> Result<Option<Observed>> {
            if !self.running.load(Ordering::SeqCst) {
                return Ok(None);
            }
            let new = self.body.lock().unwrap().contains("NEW_FEATURE=enabled");
            Ok(Some(Observed {
                id: format!("{:064x}", self.generation.load(Ordering::SeqCst)),
                name: "movie".into(),
                image: if new {
                    "b".repeat(64)
                } else if self.body.lock().unwrap().contains(&"e".repeat(64)) {
                    "e".repeat(64)
                } else {
                    "a".repeat(64)
                },
                running: true,
                retainable: false,
                config_sha256: if new { "d".repeat(64) } else { "c".repeat(64) },
            }))
        }
        async fn healthy(&self, _name: &str) -> Result<bool> {
            Ok(true)
        }
    }
    #[tokio::test]
    async fn snapshot_failure_never_stops_or_recreates_original_runtime() {
        let root = tempfile::tempdir().unwrap();
        let guard = Guard::acquire(root.path()).unwrap();
        let runtime = Mock::new();
        runtime.fail_snapshot.store(true, Ordering::SeqCst);
        assert!(execute(&guard, "movie", &[Mock::target()], &runtime)
            .await
            .is_err());
        assert_eq!(*runtime.calls.lock().unwrap(), ["snapshot-original"]);
        let original = runtime.observed("movie").await.unwrap().unwrap();
        assert_eq!(original.id, runtime.original.container_id);
        assert_eq!(original.image, runtime.original.image);
        assert_eq!(*runtime.body.lock().unwrap(), runtime.original.body);
        assert_eq!(records(&guard).unwrap()[0].phase, Phase::Aborted);
        assert!(!super::super::update_transaction::is_held(root.path(), "movie").unwrap());
        recover(&guard, &runtime).await.unwrap();
        assert_eq!(*runtime.calls.lock().unwrap(), ["snapshot-original"]);
    }
    #[tokio::test]
    async fn committed_restart_releases_only_its_own_hold_without_runtime_mutation() {
        let root = tempfile::tempdir().unwrap();
        let guard = Guard::acquire(root.path()).unwrap();
        let runtime = Mock::new();
        execute(&guard, "movie", &[Mock::target()], &runtime)
            .await
            .unwrap();
        let record = records(&guard).unwrap().pop().unwrap();
        guard.hold("movie", &record.id).unwrap();
        runtime.calls.lock().unwrap().clear();
        recover(&guard, &runtime).await.unwrap();
        assert!(runtime.calls.lock().unwrap().is_empty());
        assert!(!super::super::update_transaction::is_held(root.path(), "movie").unwrap());
        let next = uuid::Uuid::new_v4().to_string();
        guard.hold("movie", &next).unwrap();
        recover(&guard, &runtime).await.unwrap();
        assert!(super::super::update_transaction::is_held(root.path(), "movie").unwrap());
    }
    #[tokio::test]
    async fn forward_applies_reviewed_new_configuration_and_hooks_not_only_image() {
        let root = tempfile::tempdir().unwrap();
        let guard = Guard::acquire(root.path()).unwrap();
        let runtime = Mock::new();
        execute(&guard, "movie", &[Mock::target()], &runtime)
            .await
            .unwrap();
        let body = runtime.body.lock().unwrap();
        assert!(body.contains("OPERATOR_VALUE=retained"));
        assert!(body.contains("NEW_FEATURE=enabled"));
        assert!(body.contains("Volume=/identity:/run/identity:ro"));
        assert!(runtime.calls.lock().unwrap().contains(&"new-hooks".into()));
        assert_eq!(records(&guard).unwrap()[0].phase, Phase::Committed);
        assert!(!super::super::update_transaction::is_held(root.path(), "movie").unwrap());
    }
    #[tokio::test]
    async fn auto_remove_recovery_restores_old_configuration_without_claiming_original_id() {
        let root = tempfile::tempdir().unwrap();
        let guard = Guard::acquire(root.path()).unwrap();
        let runtime = Mock::new();
        runtime.fail_new_hooks.store(true, Ordering::SeqCst);
        let error = execute(&guard, "movie", &[Mock::target()], &runtime)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("container IDs may change"));
        assert_eq!(
            *runtime.body.lock().unwrap(),
            pin_body(
                &runtime.original.body,
                "movie",
                &format!("sha256:{}", "e".repeat(64))
            )
            .unwrap()
        );
        let restored = runtime.observed("movie").await.unwrap().unwrap();
        assert_eq!(restored.image, "e".repeat(64));
        assert_eq!(restored.config_sha256, runtime.original.config_sha256);
        assert_ne!(restored.id, format!("{:064x}", 1));
        assert!(super::super::update_transaction::is_held(root.path(), "movie").unwrap());
        assert_eq!(records(&guard).unwrap()[0].phase, Phase::Restored);
    }
    #[tokio::test]
    async fn interrupted_restart_uses_saved_old_unit_even_after_catalog_plan_changes() {
        let root = tempfile::tempdir().unwrap();
        let guard = Guard::acquire(root.path()).unwrap();
        let runtime = Mock::new();
        execute(&guard, "movie", &[Mock::target()], &runtime)
            .await
            .unwrap();
        let mut record = records(&guard).unwrap().pop().unwrap();
        record.phase = Phase::Starting;
        save(&guard, &record).unwrap();
        runtime.calls.lock().unwrap().clear();
        recover(&guard, &runtime).await.unwrap();
        assert_eq!(
            *runtime.body.lock().unwrap(),
            pin_body(
                &runtime.original.body,
                "movie",
                &format!("sha256:{}", "e".repeat(64))
            )
            .unwrap()
        );
        assert!(!runtime.calls.lock().unwrap().contains(&"new-hooks".into()));
    }
    #[tokio::test]
    async fn foreign_unit_edit_blocks_recovery_before_stopping_other_services() {
        let root = tempfile::tempdir().unwrap();
        let guard = Guard::acquire(root.path()).unwrap();
        let runtime = Mock::new();
        execute(&guard, "movie", &[Mock::target()], &runtime)
            .await
            .unwrap();
        let mut record = records(&guard).unwrap().pop().unwrap();
        record.phase = Phase::Starting;
        save(&guard, &record).unwrap();
        *runtime.body.lock().unwrap() = "operator replaced unit".into();
        runtime.calls.lock().unwrap().clear();
        assert!(recover(&guard, &runtime).await.is_err());
        assert!(runtime.calls.lock().unwrap().is_empty());
        assert_eq!(*runtime.body.lock().unwrap(), "operator replaced unit");
    }
    #[tokio::test]
    async fn unsupported_stopped_supervised_member_is_never_started_as_a_workaround() {
        let root = tempfile::tempdir().unwrap();
        let guard = Guard::acquire(root.path()).unwrap();
        let mut runtime = Mock::new();
        runtime.original.running = false;
        runtime.running.store(false, Ordering::SeqCst);
        assert!(execute(&guard, "movie", &[Mock::target()], &runtime)
            .await
            .is_err());
        assert!(runtime.calls.lock().unwrap().is_empty());
        assert!(records(&guard).unwrap().is_empty());
    }
}
