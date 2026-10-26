//! Retained versioned snapshots for explicitly shared Cloud files. Reuses the
//! confined descriptor and durable-copy primitives of media registration.
//! Call from spawn_blocking; ownership/visibility is authenticated by the caller.
use crate::media_registration::{self as io, Limits, SourceStamp};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        io::AsRawFd,
    },
    path::Path,
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    content_id: String,
    source: SourceStamp,
    snapshot: SourceStamp,
    sha256: String,
    size: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    record: Record,
    checksum: String,
}
fn read_manifest(version: &File) -> Result<Option<Record>> {
    let Some(saved) = io::read_record::<Envelope>(version, "snapshot.json")? else {
        return Ok(None);
    };
    anyhow::ensure!(
        saved.checksum == hash(&serde_json::to_vec(&saved.record)?),
        "Snapshot metadata is damaged; preserve accepted purchases"
    );
    Ok(Some(saved.record))
}

/// A snapshot handle. `key` is the snapshot directory name.
///
/// `key` is read by tests and by the index (which records where a snapshot was found), but
/// not on the serving path — `open_verified` returns it for callers that need to address the
/// directory. Marked rather than removed: it is part of the type's contract and the
/// alternative is re-deriving the directory name from the file descriptor.
pub(crate) struct Snapshot {
    pub file: File,
    #[allow(dead_code)]
    pub key: String,
    pub sha256: String,
    pub size: u64,
}

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Only callers holding a catalog item selected from this node's owner-managed
/// catalog may call this. Paths are confined to the supplied configured root.
/// max_total_bytes counts incomplete files too; failed quotes cannot fill disk.
pub(crate) fn prepare(
    data_dir: &Path,
    cloud_root: &Path,
    content_id: &str,
    relative: &Path,
    limits: &Limits<'_>,
    max_total_bytes: u64,
    minimum_free_bytes: u64,
    mut progress: impl FnMut(u64) -> Result<()>,
) -> Result<Snapshot> {
    anyhow::ensure!(
        !content_id.is_empty()
            && content_id.len() <= 256
            && content_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
        "Invalid shared content identity"
    );
    let data = io::open_directory(&data_dir.canonicalize()?)?;
    let root = io::private_directory(&data, "content-snapshots")?;
    // One copy admission decision at a time also makes quota accounting exact.
    io::lock_operation(&root, limits, Instant::now() + Duration::from_secs(30))?;
    let cloud = io::open_directory(&cloud_root.canonicalize()?)?;
    let mut source = io::open_cloud_file(&cloud, relative)?;
    let source_stamp = SourceStamp::read(&source)?;
    let size = source.metadata()?.len();
    anyhow::ensure!(
        size > 0 && size <= limits.max_bytes,
        "Shared file exceeds snapshot limits"
    );
    let key = hash(&serde_json::to_vec(&(
        "shared-content-snapshot-v1",
        content_id,
        relative,
        &source_stamp,
    ))?);
    let mut versions = 0usize;
    let mut existing_version = false;
    for entry in std::fs::read_dir(format!("/proc/self/fd/{}", root.as_raw_fd()))? {
        let entry = entry?;
        versions += 1;
        existing_version |= entry.file_name().to_str() == Some(&key);
    }
    anyhow::ensure!(
        existing_version || versions < 4096,
        "Shared snapshot version budget is full; retain existing purchases"
    );
    let version = io::private_directory(&root, &key)?;
    let budget_operation = format!("shared:{key}");
    if let Some(record) = read_manifest(&version)? {
        anyhow::ensure!(
            record.version == 1 && record.content_id == content_id && record.source == source_stamp,
            "Shared snapshot identity changed; preserve it for recovery"
        );
        let file = io::open_at(&version, "media", libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
        anyhow::ensure!(
            SourceStamp::read(&file)? == record.snapshot
                && file.metadata()?.mode() & 0o7777 == 0o400
                && file.metadata()?.len() == record.size,
            "Retained shared snapshot changed; preserve accepted purchases"
        );
        crate::snapshot_budget::finish_completed(data_dir, &budget_operation, record.size, limits)?;
        return Ok(Snapshot {
            file,
            key,
            sha256: record.sha256,
            size: record.size,
        });
    }
    // A crash can publish immutable bytes before the small manifest commit.
    // Recover those exact bytes by checking both held descriptors; never replace
    // an existing media file or manufacture a hash from a path/size alone.
    match io::open_at(&version, "media", libc::O_RDONLY | libc::O_NONBLOCK, 0) {
        Ok(mut file) => {
            let stamp = SourceStamp::read(&file)?;
            anyhow::ensure!(
                file.metadata()?.mode() & 0o7777 == 0o400 && file.metadata()?.len() == size,
                "Incomplete retained snapshot; preserve it for recovery"
            );
            let (snapshot_hash, snapshot_size) =
                io::hash_file(&mut file, None, limits, &mut progress)?;
            let (source_hash, source_size) =
                io::hash_file(&mut source, None, limits, &mut progress)?;
            anyhow::ensure!(
                snapshot_size == size
                    && source_size == size
                    && snapshot_hash == source_hash
                    && SourceStamp::read(&source)? == source_stamp
                    && SourceStamp::read(&file)? == stamp,
                "Interrupted snapshot does not match the selected shared version"
            );
            let record = Record {
                version: 1,
                content_id: content_id.into(),
                source: source_stamp,
                snapshot: stamp,
                sha256: snapshot_hash.clone(),
                size,
            };
            let checksum = hash(&serde_json::to_vec(&record)?);
            io::save_record(&version, "snapshot.json", &Envelope { record, checksum })?;
            use std::io::{Seek, SeekFrom};
            file.seek(SeekFrom::Start(0))?;
            crate::snapshot_budget::finish_completed(data_dir, &budget_operation, size, limits)?;
            return Ok(Snapshot {
                file,
                key,
                sha256: snapshot_hash,
                size,
            });
        }
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) => {}
        Err(error) => return Err(error),
    }
    let reservation = crate::snapshot_budget::reserve(
        data_dir,
        &budget_operation,
        size,
        max_total_bytes,
        minimum_free_bytes,
        limits,
    )?;
    let (name, mut destination) = io::temporary(&version)?;
    let mut temporary = Temporary {
        directory: &version,
        name: name.clone(),
    };
    let (sha256, copied) =
        io::hash_file(&mut source, Some(&mut destination), limits, &mut progress)?;
    anyhow::ensure!(
        copied == size && SourceStamp::read(&source)? == source_stamp,
        "Shared file changed during snapshot creation; no purchase accepted"
    );
    destination.set_permissions(std::fs::Permissions::from_mode(0o400))?;
    destination.sync_all()?;
    io::publish_file(&version, &name, "media")?;
    temporary.name.clear();
    let file = io::open_at(&version, "media", libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
    let record = Record {
        version: 1,
        content_id: content_id.into(),
        source: source_stamp,
        snapshot: SourceStamp::read(&file)?,
        sha256: sha256.clone(),
        size,
    };
    let checksum = hash(&serde_json::to_vec(&record)?);
    io::save_record(&version, "snapshot.json", &Envelope { record, checksum })?;
    reservation.finish(limits)?;
    Ok(Snapshot {
        file,
        key,
        sha256,
        size,
    })
}

/// How many snapshot opens may run in the blocking pool at once.
///
/// Deliberately small: each open may `read_dir` up to 10,000 directories on the miss path, and
/// the pool is shared with every other blocking call in the node. 16 keeps a purchase burst
/// from monopolising it while still allowing real concurrency on a many-core node.
pub(crate) const SNAPSHOT_OPEN_CONCURRENCY: usize = 16;

static OPEN_GATE: std::sync::OnceLock<OpenGate> = std::sync::OnceLock::new();

/// The process-wide gate guarding `spawn_blocking(open_matching)`.
pub(crate) fn open_gate() -> &'static OpenGate {
    OPEN_GATE.get_or_init(|| OpenGate::new(SNAPSHOT_OPEN_CONCURRENCY))
}

/// A sized gate around the blocking snapshot opens.
///
/// Backpressure, not rejection: a waiter blocks until a slot frees. Refusing here would fail a
/// delivery the buyer has already paid for, which is a worse outcome than waiting a few
/// milliseconds for a directory scan.
#[derive(Clone)]
pub(crate) struct OpenGate {
    sem: std::sync::Arc<tokio::sync::Semaphore>,
}

/// One held slot; released on drop, including on unwind.
pub(crate) struct OpenSlot {
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl OpenGate {
    pub(crate) fn new(permits: usize) -> Self {
        Self {
            sem: std::sync::Arc::new(tokio::sync::Semaphore::new(permits)),
        }
    }

    /// Wait for a slot. `Err` only if the gate is closed, which nothing in this process does.
    pub(crate) async fn acquire(&self) -> Result<OpenSlot> {
        Ok(OpenSlot {
            _permit: self
                .sem
                .clone()
                .acquire_owned()
                .await
                .context("Snapshot open gate closed")?,
        })
    }

    #[cfg(test)]
    pub(crate) fn available(&self) -> usize {
        self.sem.available_permits()
    }
}

/// Run `open_matching` on the blocking pool, bounded by [`open_gate`].
///
/// This is the ONLY sanctioned way to reach `open_matching` from async code: calling
/// `spawn_blocking(open_matching)` directly is unbounded and lets a media burst starve every
/// other blocking user of the node.
pub(crate) async fn open_matching_bounded(
    data_dir: std::path::PathBuf,
    content_id: String,
    sha256: String,
    size: u64,
) -> Result<Snapshot> {
    let slot = open_gate().acquire().await?;
    tokio::task::spawn_blocking(move || {
        // Held across the blocking call — this is what bounds the pool.
        let _slot = slot;
        open_matching(&data_dir, &content_id, &sha256, size)
    })
    .await
    .context("Snapshot open task failed")?
}

/// Open retained accepted bytes even after the original Cloud source vanishes.
/// Caller supplies the durable contract's identity/hash/size, not a browser path.
pub(crate) fn open_matching(
    data_dir: &Path,
    content_id: &str,
    sha256: &str,
    size: u64,
) -> Result<Snapshot> {
    let data = io::open_directory(&data_dir.canonicalize()?)?;
    let root = io::open_at(
        &data,
        "content-snapshots",
        libc::O_RDONLY | libc::O_DIRECTORY,
        0,
    )?;

    // D1: try the resident index first. This is the whole win — previously EVERY request
    // walked up to 10,000 directory entries and parsed a manifest per entry until a match.
    //
    // A hit does NOT skip verification: it only supplies the directory name, and the
    // manifest/stamp/mode checks below still run through the same confined-descriptor path.
    // So the index can never make an unverified file readable — a wrong or stale entry
    // yields a miss (or a failed verify that falls through to the scan), never authority.
    let index_key = crate::snapshot_index::SnapshotKey::new(content_id, sha256, size);
    if let Some(cached) = crate::snapshot_index::index().lookup(&index_key) {
        if let Some(snapshot) = open_verified(&root, &cached, content_id, sha256, size)? {
            return Ok(snapshot);
        }
        // Stale: the directory is gone or no longer matches. Drop it and fall through to the
        // scan, which is exactly the old behaviour.
        crate::snapshot_index::index().invalidate(&index_key);
    }

    let mut count = 0usize;
    for entry in std::fs::read_dir(format!("/proc/self/fd/{}", root.as_raw_fd()))? {
        let entry = entry?;
        count += 1;
        anyhow::ensure!(count <= 10000, "Snapshot index requires maintenance");
        let key = entry
            .file_name()
            .to_str()
            .context("Invalid snapshot version")?
            .to_owned();
        anyhow::ensure!(
            key.len() == 64 && key.bytes().all(|v| v.is_ascii_hexdigit()),
            "Unexpected snapshot version"
        );
        if let Some(snapshot) = open_verified(&root, &key, content_id, sha256, size)? {
            // Remember where it was, so the next request for this object skips the scan.
            crate::snapshot_index::index().insert(index_key, key);
            return Ok(snapshot);
        }
    }
    anyhow::bail!("Accepted content snapshot is unavailable; retain purchase for recovery")
}

/// Open a specific snapshot directory and verify it matches. `None` = not this one.
///
/// Split out of `open_matching` so the indexed path and the scanning path run the SAME
/// verification. Duplicating the checks for the cached path is how a cache silently becomes
/// an authorization bypass.
fn open_verified(
    root: &File,
    key: &str,
    content_id: &str,
    sha256: &str,
    size: u64,
) -> Result<Option<Snapshot>> {
    let version = match io::open_at(root, key, libc::O_RDONLY | libc::O_DIRECTORY, 0) {
        Ok(v) => v,
        // A cached directory that has since been removed is a miss, not an error: the
        // caller falls back to the scan.
        Err(_) => return Ok(None),
    };
    let Some(record) = read_manifest(&version)? else {
        return Ok(None);
    };
    if record.content_id != content_id || record.sha256 != sha256 || record.size != size {
        return Ok(None);
    }
    let file = io::open_at(&version, "media", libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
    anyhow::ensure!(
        record.version == 1
            && SourceStamp::read(&file)? == record.snapshot
            && file.metadata()?.mode() & 0o7777 == 0o400,
        "Accepted snapshot changed; retain purchase for recovery"
    );
    Ok(Some(Snapshot {
        file,
        key: key.to_string(),
        sha256: record.sha256,
        size: record.size,
    }))
}

struct Temporary<'a> {
    directory: &'a File,
    name: String,
}
impl Drop for Temporary<'_> {
    fn drop(&mut self) {
        if let Ok(name) = std::ffi::CString::new(self.name.as_str()) {
            if !self.name.is_empty() {
                unsafe {
                    libc::unlinkat(self.directory.as_raw_fd(), name.as_ptr(), 0);
                }
            }
        }
    }
}

/// Bounded entry points for snapshot opens — the last half of P2.
///
/// FINDING THAT MOTIVATED THIS: every media delivery called
/// `tokio::task::spawn_blocking(open_matching)` directly, with no bound. Tokio's blocking
/// pool is capped (512 threads by default) but a burst of purchases pins those threads in
/// `read_dir` over up to 10,000 snapshot directories each. The pool is shared with every
/// other blocking call in the node — including the ones the UI needs — so an unbounded
/// media burst can starve unrelated blocking work while the async runtime looks healthy.
/// `PEER_CONNECTION_BUDGET` and the per-peer cap bound *connections*; neither bounds the
/// blocking work one connection can start.
#[cfg(test)]
mod open_bounds_tests {
    use super::*;

    /// Backpressure semantics, not rejection: N opens run, the N+1th WAITS for a slot
    /// instead of failing. A busy node should slow a delivery, not refuse a purchase the
    /// buyer already paid for.
    #[tokio::test]
    async fn opens_are_bounded_and_wait_rather_than_fail() {
        let gate = OpenGate::new(2);
        let a = gate.acquire().await.expect("first slot");
        let b = gate.acquire().await.expect("second slot");

        // Third must not complete while both slots are held.
        let third = tokio::spawn({
            let gate = gate.clone();
            async move { gate.acquire().await.map(|_g| ()) }
        });
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(
            !third.is_finished(),
            "a third open must wait for a slot; completing here means the bound is not enforced"
        );

        drop(a);
        let third = tokio::time::timeout(std::time::Duration::from_secs(5), third)
            .await
            .expect("released slot must wake the waiter")
            .expect("task joins");
        assert!(
            third.is_ok(),
            "the waiter must be granted a slot, not an error"
        );
        drop(b);
    }

    /// A permit must be returned when the work finishes OR unwinds, and the in-flight
    /// counter must return to zero — otherwise a burst permanently shrinks capacity.
    #[tokio::test]
    async fn slots_are_returned_and_do_not_drift() {
        let gate = OpenGate::new(4);
        for _ in 0..50 {
            let g = gate.acquire().await.expect("cap is 4, sequential use");
            drop(g);
        }
        assert_eq!(
            gate.available(),
            4,
            "sequential use must not consume capacity"
        );

        // A panicking task must not leak its slot either: `Drop` runs on unwind.
        let gate2 = gate.clone();
        let _ = tokio::spawn(async move {
            let _g = gate2.acquire().await.expect("slot");
            panic!("deliberate: does the permit survive an unwind?");
        })
        .await;
        assert_eq!(
            gate.available(),
            4,
            "a panic inside the guarded section must not leak a permit"
        );
    }

    /// Wiring guard: no delivery path may reach `open_matching` through a bare
    /// `spawn_blocking`. A new handler that copies the old pattern would silently reintroduce
    /// the unbounded behaviour with every unit test still green, so this reads the OTHER
    /// modules' sources and fails on a direct call.
    #[test]
    fn no_call_site_bypasses_the_bounded_path() {
        // `include_str!` per file: the offender is in another module, so the needle cannot
        // match this file's own text.
        let sites = [
            (
                "cloud_purchase",
                include_str!("api/handler/cloud_purchase.rs"),
            ),
            (
                "onchain_purchase",
                include_str!("api/handler/onchain_purchase.rs"),
            ),
            (
                "lightning_purchase",
                include_str!("api/handler/lightning_purchase.rs"),
            ),
            ("content_server", include_str!("content_server.rs")),
        ];
        for (module, src) in sites {
            assert!(
                !src.contains("crate::content_snapshot::open_matching("),
                "{module} calls open_matching directly; it must use open_matching_bounded so                  the blocking pool stays bounded"
            );
            assert!(
                src.contains("open_matching_bounded"),
                "{module} should go through open_matching_bounded"
            );
        }
    }

    /// The bound must be a real, named, finite value and small enough to matter: a bound at
    /// or above Tokio's blocking-pool size would never constrain anything.
    #[test]
    fn the_bound_is_meaningful() {
        assert!(SNAPSHOT_OPEN_CONCURRENCY > 0);
        assert!(
            SNAPSHOT_OPEN_CONCURRENCY < 512,
            "a bound at or above Tokio's default blocking-pool size cannot bound the pool"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, sync::atomic::AtomicBool};
    #[test]
    fn quotes_reuse_one_version_and_accepted_bytes_survive_source_replacement() {
        let data = tempfile::tempdir().unwrap();
        let cloud = tempfile::tempdir().unwrap();
        std::fs::write(cloud.path().join("film.mp4"), b"old film").unwrap();
        let cancelled = AtomicBool::new(false);
        let limits = Limits {
            max_bytes: 1024,
            cancelled: &cancelled,
        };
        let first = prepare(
            data.path(),
            cloud.path(),
            "film",
            Path::new("film.mp4"),
            &limits,
            1024 * 1024,
            0,
            |_| Ok(()),
        )
        .unwrap();
        let mut copied = 0;
        let replay = prepare(
            data.path(),
            cloud.path(),
            "film",
            Path::new("film.mp4"),
            &limits,
            1024 * 1024,
            0,
            |bytes| {
                copied = bytes;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(replay.key, first.key);
        assert_eq!(copied, 0, "a new buyer quote must not recopy the file");
        std::fs::write(cloud.path().join("film.mp4"), b"new movie").unwrap();
        let next = prepare(
            data.path(),
            cloud.path(),
            "film",
            Path::new("film.mp4"),
            &limits,
            1024 * 1024,
            0,
            |_| Ok(()),
        )
        .unwrap();
        assert_ne!(first.key, next.key);
        std::fs::remove_file(cloud.path().join("film.mp4")).unwrap();
        let mut retained = open_matching(data.path(), "film", &first.sha256, first.size).unwrap();
        let mut bytes = Vec::new();
        retained.file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"old film");
        assert!(open_matching(data.path(), "other", &first.sha256, first.size).is_err());
    }

    /// D1's judged property, at the level `open_matching` actually runs: after the first
    /// successful lookup of an object, subsequent lookups of the SAME object must be served
    /// from the resident index instead of rescanning `content-snapshots/`.
    ///
    /// Counters are read as deltas because the index is process-global and other tests in
    /// this binary legitimately insert into it.
    #[test]
    fn repeated_lookups_of_one_object_hit_the_index_instead_of_rescanning() {
        let data = tempfile::tempdir().unwrap();
        let cloud = tempfile::tempdir().unwrap();
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let limits = Limits {
            max_bytes: 1024 * 1024,
            cancelled: &cancelled,
        };
        std::fs::write(cloud.path().join("film.mp4"), b"old film").unwrap();
        let prepared = prepare(
            data.path(),
            cloud.path(),
            "d1-film",
            Path::new("film.mp4"),
            &limits,
            1024 * 1024,
            0,
            |_| Ok(()),
        )
        .unwrap();

        // First lookup of this object: whatever the index already held, this key is new, so
        // this call must scan and then populate.
        let before_hits = crate::snapshot_index::index().hits();
        let before_misses = crate::snapshot_index::index().misses();
        let first = open_matching(data.path(), "d1-film", &prepared.sha256, prepared.size).unwrap();
        assert_eq!(
            crate::snapshot_index::index().misses(),
            before_misses + 1,
            "the first lookup must be a miss (it has to find the snapshot somehow)"
        );
        let hits_after_first = crate::snapshot_index::index().hits();

        // Second and third lookups: must hit, and must NOT add misses.
        for _ in 0..2 {
            let again =
                open_matching(data.path(), "d1-film", &prepared.sha256, prepared.size).unwrap();
            assert_eq!(again.key, first.key, "same object, same snapshot directory");
        }
        assert_eq!(
            crate::snapshot_index::index().hits(),
            hits_after_first + 2,
            "both repeat lookups must be served from the index, not by rescanning"
        );
        assert_eq!(
            crate::snapshot_index::index().misses(),
            before_misses + 1,
            "repeat lookups must not fall back to the scan"
        );
    }
    #[test]
    fn quotas_cancellation_and_source_mutation_never_issue_a_snapshot() {
        let data = tempfile::tempdir().unwrap();
        let cloud = tempfile::tempdir().unwrap();
        let path = cloud.path().join("film");
        std::fs::write(&path, vec![7u8; 100000]).unwrap();
        let cancelled = AtomicBool::new(false);
        let limits = Limits {
            max_bytes: 200000,
            cancelled: &cancelled,
        };
        assert!(prepare(
            data.path(),
            cloud.path(),
            "film",
            Path::new("film"),
            &limits,
            50000,
            0,
            |_| Ok(())
        )
        .is_err());
        assert!(prepare(
            data.path(),
            cloud.path(),
            "film",
            Path::new("film"),
            &limits,
            1024 * 1024,
            0,
            |_| {
                std::fs::write(&path, b"changed")?;
                Ok(())
            }
        )
        .is_err());
        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(prepare(
            data.path(),
            cloud.path(),
            "film",
            Path::new("film"),
            &limits,
            1024 * 1024,
            0,
            |_| Ok(())
        )
        .is_err());
    }
    #[test]
    fn interrupted_manifest_commit_recovers_same_snapshot_without_replacing_bytes() {
        let data = tempfile::tempdir().unwrap();
        let cloud = tempfile::tempdir().unwrap();
        std::fs::write(cloud.path().join("film"), b"film").unwrap();
        let cancelled = AtomicBool::new(false);
        let limits = Limits {
            max_bytes: 1024,
            cancelled: &cancelled,
        };
        let first = prepare(
            data.path(),
            cloud.path(),
            "film",
            Path::new("film"),
            &limits,
            1024 * 1024,
            0,
            |_| Ok(()),
        )
        .unwrap();
        let inode = first.file.metadata().unwrap().ino();
        std::fs::remove_file(
            data.path()
                .join("content-snapshots")
                .join(&first.key)
                .join("snapshot.json"),
        )
        .unwrap();
        let restored = prepare(
            data.path(),
            cloud.path(),
            "film",
            Path::new("film"),
            &limits,
            1024 * 1024,
            0,
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(restored.file.metadata().unwrap().ino(), inode);
        assert_eq!(restored.sha256, first.sha256);
    }
}
