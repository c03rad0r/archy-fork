//! Retained-container updates. This is runtime recovery, never database rollback.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    future::Future,
    io::Write,
    os::fd::AsRawFd,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct Observed {
    pub id: String,
    pub name: String,
    pub image: String,
    pub running: bool,
    pub config_sha256: String,
    /// False for auto-remove, external supervision, pods, paused/unknown states.
    pub retainable: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Target {
    pub name: String,
    pub reference: String,
    pub image: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Member {
    original: Observed,
    target: Target,
    backup: String,
    replacement_name: String,
    replacement_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Phase {
    Prepared,
    Replacing,
    Verified,
    Committed,
    Restored,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u8,
    operation: String,
    package: String,
    phase: Phase,
    members: Vec<Member>,
}

/// An adapter must address destructive commands by immutable ID. `create` must
/// not start the new member, including when the original was stopped.
pub(crate) trait Runtime: Sync {
    fn inspect(&self, id_or_name: &str) -> impl Future<Output = Result<Option<Observed>>> + Send;
    fn stop(&self, id: &str) -> impl Future<Output = Result<()>> + Send;
    fn start(&self, id: &str) -> impl Future<Output = Result<()>> + Send;
    fn rename(&self, id: &str, name: &str) -> impl Future<Output = Result<()>> + Send;
    fn create(
        &self,
        original: &str,
        name: &str,
        target: &str,
    ) -> impl Future<Output = Result<String>> + Send;
    fn remove(&self, id: &str) -> impl Future<Output = Result<()>> + Send;
    fn healthy(&self, id: &str) -> impl Future<Output = Result<bool>> + Send;
}

pub(crate) struct Guard {
    _file: std::fs::File,
    root: PathBuf,
}
impl Guard {
    pub(crate) fn acquire(data: &Path) -> Result<Self> {
        let root = data.join("update-transactions");
        match std::fs::DirBuilder::new().mode(0o700).create(&root) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        anyhow::ensure!(
            !std::fs::symlink_metadata(&root)?.file_type().is_symlink(),
            "Update journal directory is a symlink"
        );
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(root.join("lock"))?;
        anyhow::ensure!(
            unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
            "An app lifecycle operation is active; retry when it completes"
        );
        Ok(Self { _file: file, root })
    }
    fn path(&self, operation: &str) -> Result<PathBuf> {
        anyhow::ensure!(
            uuid::Uuid::parse_str(operation)?.to_string() == operation,
            "Invalid update operation"
        );
        Ok(self.root.join(format!("{operation}.json")))
    }
    fn save(&self, record: &Record) -> Result<()> {
        record.validate()?;
        let target = self.path(&record.operation)?;
        let temp = self.root.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        let bytes = serde_json::to_vec(record)?;
        anyhow::ensure!(bytes.len() <= 1024 * 1024, "Update journal too large");
        // No await/detached writer after the ownership lock can be released.
        let result = (|| -> Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            std::fs::rename(&temp, &target)?;
            std::fs::File::open(&self.root)?.sync_all()?;
            std::fs::File::open(self.root.parent().unwrap())?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temp);
        }
        result
    }
    fn records(&self) -> Result<Vec<Record>> {
        let mut records = Vec::new();
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.path().extension().and_then(|v| v.to_str()) != Some("json") {
                continue;
            }
            anyhow::ensure!(records.len() < 256, "Too many retained update journals");
            let meta = entry.metadata()?;
            anyhow::ensure!(
                entry.file_type()?.is_file() && meta.len() <= 1024 * 1024,
                "Invalid update journal file"
            );
            let record: Record = serde_json::from_slice(&std::fs::read(entry.path())?)?;
            record.validate()?;
            anyhow::ensure!(
                self.path(&record.operation)? == entry.path(),
                "Update journal name changed"
            );
            records.push(record);
        }
        Ok(records)
    }
    pub(crate) fn require_clear(&self) -> Result<()> {
        anyhow::ensure!(
            self.records()?
                .iter()
                .all(|r| matches!(r.phase, Phase::Committed | Phase::Restored)),
            "An interrupted app update needs recovery before another lifecycle action"
        );
        Ok(())
    }
}
fn name_ok(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}
fn id_ok(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
impl Record {
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.schema == 1
                && name_ok(&self.package)
                && !self.members.is_empty()
                && self.members.len() <= 32,
            "Invalid update journal"
        );
        let operation = uuid::Uuid::parse_str(&self.operation)?;
        anyhow::ensure!(
            operation.to_string() == self.operation,
            "Invalid update UUID"
        );
        let mut names = HashSet::new();
        let mut ids = HashSet::new();
        for (i, m) in self.members.iter().enumerate() {
            anyhow::ensure!(
                name_ok(&m.original.name)
                    && m.target.name == m.original.name
                    && id_ok(&m.original.id)
                    && id_ok(&m.original.config_sha256)
                    && ids.insert(&m.original.id)
                    && names.insert(&m.original.name)
                    && m.original.retainable
                    && m.backup == format!("archy-update-{}-{i}-old", operation.simple())
                    && m.replacement_name == format!("archy-update-{}-{i}-new", operation.simple())
                    && m.replacement_id.as_deref().is_none_or(id_ok),
                "Invalid retained update member"
            );
        }
        Ok(())
    }
}
async fn exact(runtime: &impl Runtime, id: &str) -> Result<Observed> {
    let found = runtime
        .inspect(id)
        .await?
        .context("Retained container is missing; runtime recovery unresolved")?;
    anyhow::ensure!(
        found.id == id,
        "Container identity changed; recovery unresolved"
    );
    Ok(found)
}

/// All original inventory is proven before the first stop/rename. Unsupported
/// runtimes stay untouched. The caller has already prepared every target image.
pub(crate) async fn execute(
    guard: &Guard,
    package: &str,
    targets: &[Target],
    runtime: &impl Runtime,
) -> Result<()> {
    guard.require_clear()?;
    anyhow::ensure!(
        !targets.is_empty() && targets.len() <= 32,
        "Invalid update members"
    );
    let operation = uuid::Uuid::new_v4();
    let mut members = Vec::new();
    for (i, target) in targets.iter().enumerate() {
        let original = runtime
            .inspect(&target.name)
            .await?
            .context("Original update member is missing")?;
        anyhow::ensure!(
            original.retainable,
            "{} cannot retain its original runtime safely; no containers changed",
            target.name
        );
        members.push(Member {
            original,
            target: target.clone(),
            backup: format!("archy-update-{}-{i}-old", operation.simple()),
            replacement_name: format!("archy-update-{}-{i}-new", operation.simple()),
            replacement_id: None,
        });
    }
    let mut record = Record {
        schema: 1,
        operation: operation.to_string(),
        package: package.into(),
        phase: Phase::Prepared,
        members,
    };
    guard.save(&record)?;
    match replace(&guard,&mut record,runtime).await {
        Ok(())=>Ok(()),
        Err(error)=>match restore(&guard,&mut record,runtime).await {
            Ok(())=>Err(error.context("Update failed; original runtime states restored (persistent data was not rolled back)")),
            Err(recovery)=>Err(error.context(format!("Update failed; runtime recovery remains unresolved: {recovery:#}"))),
        }
    }
}
async fn replace(guard: &Guard, record: &mut Record, runtime: &impl Runtime) -> Result<()> {
    for member in record.members.iter().rev() {
        let current = exact(runtime, &member.original.id).await?;
        anyhow::ensure!(
            current.name == member.original.name
                && current.image == member.original.image
                && current.config_sha256 == member.original.config_sha256
                && current.retainable,
            "Original changed before update"
        );
        if current.running {
            runtime.stop(&current.id).await?;
        }
        anyhow::ensure!(
            !exact(runtime, &current.id).await?.running,
            "Original did not stop"
        );
    }
    record.phase = Phase::Replacing;
    guard.save(record)?;
    for index in 0..record.members.len() {
        let member = &record.members[index];
        anyhow::ensure!(
            runtime.inspect(&member.backup).await?.is_none()
                && runtime.inspect(&member.replacement_name).await?.is_none(),
            "Update backup name collision"
        );
        runtime.rename(&member.original.id, &member.backup).await?;
        let id = runtime
            .create(
                &member.original.id,
                &member.replacement_name,
                &member.target.reference,
            )
            .await?;
        anyhow::ensure!(id_ok(&id), "Invalid replacement identity");
        record.members[index].replacement_id = Some(id.clone());
        guard.save(record)?;
        let member = &record.members[index];
        let created = exact(runtime, &id).await?;
        anyhow::ensure!(
            created.name == member.replacement_name
                && !created.running
                && created.image == member.target.image
                && created.config_sha256 == member.original.config_sha256,
            "Replacement configuration/state mismatch"
        );
        runtime.rename(&id, &member.original.name).await?;
    }
    for member in &record.members {
        let id = member
            .replacement_id
            .as_deref()
            .context("Missing replacement")?;
        if member.original.running {
            runtime.start(id).await?;
        }
        let observed = exact(runtime, id).await?;
        anyhow::ensure!(
            observed.running == member.original.running && observed.image == member.target.image,
            "Replacement state mismatch"
        );
        if member.original.running {
            anyhow::ensure!(
                runtime.healthy(id).await?,
                "Updated member failed health verification"
            );
        }
    }
    record.phase = Phase::Verified;
    guard.save(record)?;
    // Original backups are retained. Committing does not remove volumes or
    // silently discard the only recoverable original after a schema migration.
    record.phase = Phase::Committed;
    guard.save(record)
}
async fn restore(guard: &Guard, record: &mut Record, runtime: &impl Runtime) -> Result<()> {
    // Preflight every original before mutating any replacement. Never infer
    // ownership of a new container from a reused public name.
    for member in &record.members {
        let original = exact(runtime, &member.original.id).await?;
        anyhow::ensure!(
            original.image == member.original.image
                && original.config_sha256 == member.original.config_sha256
                && (original.name == member.original.name || original.name == member.backup),
            "Original rollback identity/config changed"
        );
        if member.replacement_id.is_none() {
            anyhow::ensure!(
                runtime.inspect(&member.replacement_name).await?.is_none(),
                "Unacknowledged replacement exists; preserve it for explicit recovery"
            );
        }
        if let Some(current) = runtime.inspect(&member.original.name).await? {
            anyhow::ensure!(
                current.id == member.original.id
                    || Some(&current.id) == member.replacement_id.as_ref(),
                "Foreign container occupies original name"
            );
        }
    }
    for member in record.members.iter().rev() {
        if let Some(id) = &member.replacement_id {
            if let Some(current) = runtime.inspect(id).await? {
                anyhow::ensure!(
                    current.id == *id && current.image == member.target.image,
                    "Replacement identity changed"
                );
                if current.running {
                    runtime.stop(id).await?;
                }
                runtime.remove(id).await?;
            }
        }
    }
    for member in &record.members {
        let current = exact(runtime, &member.original.id).await?;
        if current.name != member.original.name {
            runtime.rename(&current.id, &member.original.name).await?;
        }
        if member.original.running && !current.running {
            runtime.start(&current.id).await?;
        } else if !member.original.running && current.running {
            runtime.stop(&current.id).await?;
        }
        let observed = exact(runtime, &current.id).await?;
        anyhow::ensure!(
            observed.name == member.original.name
                && observed.image == member.original.image
                && observed.running == member.original.running,
            "Original state was not restored"
        );
    }
    record.phase = Phase::Restored;
    guard.save(record)
}
/// Called before ordinary reconciliation. An unresolved recovery must prevent
/// reconciliation from guessing a replacement or starting stopped originals.
pub(crate) async fn recover(data: &Path, runtime: &impl Runtime) -> Result<Guard> {
    let guard = Guard::acquire(data)?;
    for mut record in guard.records()? {
        if !matches!(record.phase, Phase::Committed | Phase::Restored) {
            restore(&guard, &mut record, runtime).await?;
        }
    }
    Ok(guard)
}

pub(crate) struct Podman;
impl Podman {
    async fn output(args: &[&str]) -> Result<std::process::Output> {
        tokio::time::timeout(
            std::time::Duration::from_secs(120),
            tokio::process::Command::new("podman")
                .args(args)
                .kill_on_drop(true)
                .output(),
        )
        .await
        .context("Container operation timed out; inspect original operation before retry")?
        .context("Container runtime unavailable")
    }
    async fn command(args: &[&str]) -> Result<String> {
        let output = Self::output(args).await?;
        anyhow::ensure!(
            output.status.success(),
            "Container runtime rejected {} (exit {:?})",
            args.first().unwrap_or(&"operation"),
            output.status.code()
        );
        Ok(std::str::from_utf8(&output.stdout)?.trim().to_string())
    }
    pub(crate) async fn targets(images: &[(String, String)]) -> Result<Vec<Target>> {
        // Confirm clone support before any stop. Never silently switch to a
        // latest-catalog install if this runtime cannot create without running.
        Self::command(&["container", "clone", "--help"]).await?;
        let mut targets = Vec::new();
        for (name, reference) in images {
            let raw = Self::command(&["image", "inspect", reference]).await?;
            let rows: Vec<serde_json::Value> = serde_json::from_str(&raw)?;
            let image = rows
                .first()
                .and_then(|v| v.get("Id").or_else(|| v.get("ID")))
                .and_then(|v| v.as_str())
                .context("Prepared image identity unavailable")?;
            targets.push(Target {
                name: name.clone(),
                reference: reference.clone(),
                image: image.strip_prefix("sha256:").unwrap_or(image).into(),
            });
        }
        Ok(targets)
    }
}
impl Runtime for Podman {
    async fn inspect(&self, value: &str) -> Result<Option<Observed>> {
        let exists = Self::output(&["container", "exists", value]).await?;
        match exists.status.code() {
            Some(0) => {}
            Some(1) => return Ok(None),
            _ => anyhow::bail!("Container inventory unavailable"),
        }
        let raw = Self::command(&["inspect", value]).await?;
        let rows: Vec<serde_json::Value> = serde_json::from_str(&raw)?;
        let row = rows.first().context("Empty container inspection")?;
        let string = |key: &str| {
            row.get(key)
                .and_then(|v| v.as_str())
                .context("Incomplete container identity")
        };
        let state = row
            .pointer("/State/Status")
            .and_then(|v| v.as_str())
            .context("Missing runtime state")?;
        let labels = row.pointer("/Config/Labels");
        let supervised = labels.and_then(|v| v.get("PODMAN_SYSTEMD_UNIT")).is_some()
            || labels
                .and_then(|v| v.get("io.containers.systemd.unit"))
                .is_some();
        let supervised = supervised
            || row
                .pointer("/Config/Env")
                .and_then(|v| v.as_array())
                .is_some_and(|env| {
                    env.iter().any(|v| {
                        v.as_str()
                            .is_some_and(|v| v.starts_with("PODMAN_SYSTEMD_UNIT="))
                    })
                });
        let restart = row
            .pointer("/HostConfig/RestartPolicy/Name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let retainable = row
            .pointer("/HostConfig/AutoRemove")
            .and_then(|v| v.as_bool())
            == Some(false)
            && !supervised
            && matches!(restart, "" | "no")
            && row
                .get("Pod")
                .and_then(|v| v.as_str())
                .is_none_or(str::is_empty)
            && row
                .get("Dependencies")
                .and_then(|v| v.as_array())
                .is_none_or(Vec::is_empty)
            && matches!(state, "running" | "stopped" | "exited" | "created");
        let config = serde_json::json!({
            "mounts":row.get("Mounts"),"env":row.pointer("/Config/Env"),
            "command":row.pointer("/Config/Cmd"),"entrypoint":row.pointer("/Config/Entrypoint"),
            "user":row.pointer("/Config/User"),"directory":row.pointer("/Config/WorkingDir"),
            "ports":row.pointer("/HostConfig/PortBindings"),"network":row.pointer("/HostConfig/NetworkMode"),
            "privileged":row.pointer("/HostConfig/Privileged"),"devices":row.pointer("/HostConfig/Devices"),
            "security":row.pointer("/HostConfig/SecurityOpt"),"cap_add":row.pointer("/HostConfig/CapAdd"),"cap_drop":row.pointer("/HostConfig/CapDrop")
        });
        use sha2::Digest;
        Ok(Some(Observed {
            id: string("Id")?.into(),
            name: string("Name")?.trim_start_matches('/').into(),
            image: string("Image")?
                .strip_prefix("sha256:")
                .unwrap_or(string("Image")?)
                .into(),
            running: state == "running",
            retainable,
            config_sha256: hex::encode(sha2::Sha256::digest(serde_json::to_vec(&config)?)),
        }))
    }
    async fn stop(&self, id: &str) -> Result<()> {
        let original = exact(self, id).await?;
        let grace = archipelago_container::runtime::stop_grace_secs_for(&original.name);
        let timeout = grace.to_string();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(grace + 30),
            tokio::process::Command::new("podman")
                .args(["stop", "--time", &timeout, id])
                .kill_on_drop(true)
                .output(),
        )
        .await
        .context("Graceful stop is unresolved; original runtime retained")??;
        anyhow::ensure!(result.status.success(), "Graceful container stop failed");
        Ok(())
    }
    async fn start(&self, id: &str) -> Result<()> {
        Self::command(&["start", id]).await?;
        Ok(())
    }
    async fn rename(&self, id: &str, name: &str) -> Result<()> {
        Self::command(&["rename", id, name]).await?;
        Ok(())
    }
    async fn create(&self, id: &str, name: &str, target: &str) -> Result<String> {
        Self::command(&["container", "clone", id, name, target]).await
    }
    async fn remove(&self, id: &str) -> Result<()> {
        Self::command(&["rm", id]).await?;
        Ok(())
    }
    async fn healthy(&self, id: &str) -> Result<bool> {
        // Missing healthcheck is not fabricated health. Running-only containers
        // can pass runtime readiness; healthchecked members must report healthy.
        for _ in 0..30 {
            let raw = Self::command(&["inspect", id]).await?;
            let rows: Vec<serde_json::Value> = serde_json::from_str(&raw)?;
            let row = rows.first().context("Missing updated container")?;
            if row.pointer("/State/Status").and_then(|v| v.as_str()) != Some("running") {
                return Ok(false);
            }
            match row.pointer("/State/Health/Status").and_then(|v| v.as_str()) {
                None | Some("") | Some("healthy") => return Ok(true),
                Some("unhealthy") => return Ok(false),
                _ => {}
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    };
    struct Mock {
        rows: Mutex<Vec<Observed>>,
        calls: Mutex<Vec<String>>,
        fail_at: AtomicUsize,
        sequence: AtomicUsize,
        lost_create: AtomicBool,
        fail_health: AtomicBool,
    }
    impl Mock {
        fn new() -> Self {
            Self {
                rows: Mutex::new(vec![
                    Observed {
                        id: format!("{:064x}", 1),
                        name: "db".into(),
                        image: "old-db".into(),
                        running: true,
                        retainable: true,
                        config_sha256: "a".repeat(64),
                    },
                    Observed {
                        id: format!("{:064x}", 2),
                        name: "web".into(),
                        image: "old-web".into(),
                        running: false,
                        retainable: true,
                        config_sha256: "b".repeat(64),
                    },
                ]),
                calls: Default::default(),
                fail_at: AtomicUsize::new(0),
                sequence: AtomicUsize::new(0),
                lost_create: AtomicBool::new(false),
                fail_health: AtomicBool::new(false),
            }
        }
        fn event(&self, action: String) -> Result<()> {
            self.calls.lock().unwrap().push(action);
            let step = self.sequence.fetch_add(1, Ordering::SeqCst) + 1;
            anyhow::ensure!(
                step != self.fail_at.load(Ordering::SeqCst),
                "Injected lifecycle failure"
            );
            Ok(())
        }
        fn targets() -> Vec<Target> {
            vec![
                Target {
                    name: "db".into(),
                    reference: "new-db".into(),
                    image: "new-db".into(),
                },
                Target {
                    name: "web".into(),
                    reference: "new-web".into(),
                    image: "new-web".into(),
                },
            ]
        }
        fn originals_restored(&self) {
            let rows = self.rows.lock().unwrap();
            assert_eq!(rows.len(), 2);
            for (i, name, running) in [(1, "db", true), (2, "web", false)] {
                let row = rows.iter().find(|r| r.id == format!("{i:064x}")).unwrap();
                assert_eq!(row.name, name);
                assert_eq!(row.running, running);
                assert_eq!(row.image, format!("old-{name}"));
            }
        }
    }
    impl Runtime for Mock {
        async fn inspect(&self, value: &str) -> Result<Option<Observed>> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.id == value || r.name == value)
                .cloned())
        }
        async fn stop(&self, id: &str) -> Result<()> {
            self.event(format!("stop:{id}"))?;
            self.rows
                .lock()
                .unwrap()
                .iter_mut()
                .find(|r| r.id == id)
                .unwrap()
                .running = false;
            Ok(())
        }
        async fn start(&self, id: &str) -> Result<()> {
            self.event(format!("start:{id}"))?;
            self.rows
                .lock()
                .unwrap()
                .iter_mut()
                .find(|r| r.id == id)
                .unwrap()
                .running = true;
            Ok(())
        }
        async fn rename(&self, id: &str, name: &str) -> Result<()> {
            self.event(format!("rename:{id}:{name}"))?;
            let mut rows = self.rows.lock().unwrap();
            anyhow::ensure!(!rows.iter().any(|r| r.name == name), "Name occupied");
            rows.iter_mut().find(|r| r.id == id).unwrap().name = name.into();
            Ok(())
        }
        async fn create(&self, original: &str, name: &str, target: &str) -> Result<String> {
            self.event(format!("create-stopped:{name}"))?;
            let mut rows = self.rows.lock().unwrap();
            let mut row = rows.iter().find(|r| r.id == original).unwrap().clone();
            row.id = format!("{:064x}", 100 + rows.len());
            row.name = name.into();
            row.image = target.into();
            row.running = false;
            let id = row.id.clone();
            rows.push(row);
            anyhow::ensure!(
                !self.lost_create.swap(false, Ordering::SeqCst),
                "Lost create acknowledgement"
            );
            Ok(id)
        }
        async fn remove(&self, id: &str) -> Result<()> {
            self.event(format!("remove:{id}"))?;
            self.rows.lock().unwrap().retain(|r| r.id != id);
            Ok(())
        }
        async fn healthy(&self, _id: &str) -> Result<bool> {
            Ok(!self.fail_health.load(Ordering::SeqCst))
        }
    }
    #[tokio::test]
    async fn mixed_stack_update_never_starts_stopped_member_and_retains_original_ids() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Mock::new();
        let guard = Guard::acquire(root.path()).unwrap();
        execute(&guard, "stack", &Mock::targets(), &runtime)
            .await
            .unwrap();
        let record = guard.records().unwrap().pop().unwrap();
        assert_eq!(record.phase, Phase::Committed);
        let rows = runtime.rows.lock().unwrap();
        assert_eq!(rows.len(), 4);
        for member in &record.members {
            let original = rows.iter().find(|r| r.id == member.original.id).unwrap();
            assert_eq!(original.name, member.backup);
            assert!(!original.running);
            let new = rows
                .iter()
                .find(|r| Some(&r.id) == member.replacement_id.as_ref())
                .unwrap();
            assert_eq!(new.running, member.original.running);
        }
        assert!(!runtime.calls.lock().unwrap().iter().any(|call| call
            == &format!(
                "start:{}",
                record.members[1].replacement_id.as_ref().unwrap()
            )));
        assert!(!runtime
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| call.starts_with("remove:")));
    }
    #[tokio::test]
    async fn every_mutation_failure_restores_exact_originals_without_catalog_recreate() {
        // Eight mutating calls in this two-member update, including starting db.
        for fail in 1..=8 {
            let root = tempfile::tempdir().unwrap();
            let runtime = Mock::new();
            runtime.fail_at.store(fail, Ordering::SeqCst);
            let guard = Guard::acquire(root.path()).unwrap();
            let error = execute(&guard, "stack", &Mock::targets(), &runtime)
                .await
                .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("original runtime states restored"),
                "failure {fail}: {error:#}"
            );
            runtime.originals_restored();
            assert_eq!(guard.records().unwrap()[0].phase, Phase::Restored);
        }
    }
    #[tokio::test]
    async fn unsupported_member_refuses_before_any_runtime_mutation() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Mock::new();
        runtime.rows.lock().unwrap()[1].retainable = false;
        let guard = Guard::acquire(root.path()).unwrap();
        assert!(execute(&guard, "stack", &Mock::targets(), &runtime)
            .await
            .is_err());
        assert!(runtime.calls.lock().unwrap().is_empty());
        assert!(guard.records().unwrap().is_empty());
        runtime.originals_restored();
    }
    #[tokio::test]
    async fn failed_health_restores_running_and_stopped_states() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Mock::new();
        runtime.fail_health.store(true, Ordering::SeqCst);
        let guard = Guard::acquire(root.path()).unwrap();
        assert!(execute(&guard, "stack", &Mock::targets(), &runtime)
            .await
            .is_err());
        runtime.originals_restored();
    }
    #[tokio::test]
    async fn lost_create_reply_is_unresolved_and_never_deletes_unowned_container() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Mock::new();
        runtime.lost_create.store(true, Ordering::SeqCst);
        let guard = Guard::acquire(root.path()).unwrap();
        let error = execute(&guard, "stack", &Mock::targets(), &runtime)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("unresolved"));
        assert!(guard.require_clear().is_err());
        assert!(!runtime
            .calls
            .lock()
            .unwrap()
            .iter()
            .any(|call| call.starts_with("remove:")));
        drop(guard);
        assert!(recover(root.path(), &runtime).await.is_err());
        assert_eq!(runtime.rows.lock().unwrap().len(), 3);
    }
    #[tokio::test]
    async fn daemon_restart_restores_interrupted_rename_before_normal_reconciliation() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Mock::new();
        let guard = Guard::acquire(root.path()).unwrap();
        execute(&guard, "stack", &Mock::targets(), &runtime)
            .await
            .unwrap();
        let mut record = guard.records().unwrap().pop().unwrap();
        // Simulate interruption after replacements existed but before durable commit.
        record.phase = Phase::Replacing;
        guard.save(&record).unwrap();
        drop(guard);
        let recovered = recover(root.path(), &runtime).await.unwrap();
        runtime.originals_restored();
        assert_eq!(recovered.records().unwrap()[0].phase, Phase::Restored);
    }
    #[tokio::test]
    async fn missing_original_does_not_fabricate_rollback_or_start_other_members() {
        let root = tempfile::tempdir().unwrap();
        let runtime = Mock::new();
        let guard = Guard::acquire(root.path()).unwrap();
        execute(&guard, "stack", &Mock::targets(), &runtime)
            .await
            .unwrap();
        let mut record = guard.records().unwrap().pop().unwrap();
        record.phase = Phase::Replacing;
        guard.save(&record).unwrap();
        runtime
            .rows
            .lock()
            .unwrap()
            .retain(|r| r.id != format!("{:064x}", 1));
        runtime.calls.lock().unwrap().clear();
        drop(guard);
        assert!(recover(root.path(), &runtime).await.is_err());
        assert!(runtime.calls.lock().unwrap().is_empty());
    }
    #[test]
    fn lifecycle_lock_excludes_competing_commands() {
        let root = tempfile::tempdir().unwrap();
        let guard = Guard::acquire(root.path()).unwrap();
        assert!(Guard::acquire(root.path()).is_err());
        drop(guard);
        assert!(Guard::acquire(root.path()).is_ok());
    }
}
