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

pub(crate) struct Snapshot {
    pub file: File,
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
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            ()
        }
        Err(error) => return Err(error),
    }
    let used = storage_bytes(&root)?;
    anyhow::ensure!(
        used.checked_add(size)
            .and_then(|v| v.checked_add(128 * 1024))
            .is_some_and(|total| total <= max_total_bytes),
        "Shared snapshot storage budget is full; no new purchase accepted"
    );
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    anyhow::ensure!(
        unsafe { libc::fstatvfs(root.as_raw_fd(), stat.as_mut_ptr()) } == 0,
        "Could not verify snapshot disk space"
    );
    let stat = unsafe { stat.assume_init() };
    let free = (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64);
    anyhow::ensure!(
        free >= size.saturating_add(minimum_free_bytes),
        "Not enough free storage for a retained purchase snapshot"
    );
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
    Ok(Snapshot {
        file,
        key,
        sha256,
        size,
    })
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
        let version = io::open_at(&root, &key, libc::O_RDONLY | libc::O_DIRECTORY, 0)?;
        let Some(record) = read_manifest(&version)? else {
            continue;
        };
        if record.content_id != content_id || record.sha256 != sha256 || record.size != size {
            continue;
        }
        let file = io::open_at(&version, "media", libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
        anyhow::ensure!(
            record.version == 1
                && SourceStamp::read(&file)? == record.snapshot
                && file.metadata()?.mode() & 0o7777 == 0o400,
            "Accepted snapshot changed; retain purchase for recovery"
        );
        return Ok(Snapshot {
            file,
            key,
            sha256: record.sha256,
            size: record.size,
        });
    }
    anyhow::bail!("Accepted content snapshot is unavailable; retain purchase for recovery")
}

fn storage_bytes(directory: &File) -> Result<u64> {
    let mut total = 0u64;
    for entry in std::fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().context("Invalid snapshot filename")?;
        let metadata = std::fs::symlink_metadata(entry.path())?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "Snapshot storage contains an unexpected symlink"
        );
        let size = if metadata.is_dir() {
            storage_bytes(&io::open_at(
                directory,
                name,
                libc::O_RDONLY | libc::O_DIRECTORY,
                0,
            )?)?
        } else {
            anyhow::ensure!(metadata.is_file(), "Unexpected snapshot storage entry");
            metadata.len()
        };
        total = total
            .checked_add(size)
            .context("Snapshot storage accounting overflow")?;
    }
    Ok(total)
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
