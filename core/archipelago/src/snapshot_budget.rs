//! Shared admission budget for immutable paid-content snapshots.
//! Blocking worker only. Reservations are durable before copying and intentionally
//! survive process death; orphan cleanup needs operation-aware reconciliation.
use crate::media_registration::{self as io, Limits};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    os::unix::io::AsRawFd,
    path::Path,
    time::{Duration, Instant},
};

pub(crate) const DEFAULT_MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024 * 1024;
pub(crate) const DEFAULT_MIN_FREE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    operation: String,
    bytes: u64,
}
pub(crate) struct Reservation {
    _claim: File,
    root: File,
    name: String,
}
fn count(directory: &File) -> Result<u64> {
    let mut bytes = 0u64;
    for item in std::fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))? {
        let item = item?;
        let name = item.file_name();
        let name = name.to_str().context("Invalid snapshot storage filename")?;
        let metadata = std::fs::symlink_metadata(item.path())?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "Unexpected snapshot symlink"
        );
        let size = if metadata.is_dir() {
            count(&io::open_at(
                directory,
                name,
                libc::O_RDONLY | libc::O_DIRECTORY,
                0,
            )?)?
        } else {
            anyhow::ensure!(metadata.is_file(), "Unexpected snapshot storage entry");
            metadata.len()
        };
        bytes = bytes
            .checked_add(size)
            .context("Snapshot accounting overflow")?;
    }
    Ok(bytes)
}
fn reserved(root: &File) -> Result<u64> {
    let mut total = 0u64;
    for item in std::fs::read_dir(format!("/proc/self/fd/{}", root.as_raw_fd()))? {
        let item = item?;
        let name = item.file_name();
        let name = name.to_str().context("Invalid reservation filename")?;
        if name == "claims" {
            continue;
        }
        anyhow::ensure!(
            name.ends_with(".json"),
            "Unknown reservation state; preserve for recovery"
        );
        let record: Record = io::read_record(root, name)?.context("Reservation disappeared")?;
        anyhow::ensure!(
            record.version == 1 && record.bytes > 0,
            "Invalid reservation; preserve for recovery"
        );
        total = total
            .checked_add(record.bytes)
            .context("Reservation accounting overflow")?;
    }
    Ok(total)
}
/// Operation must be a durable journal identifier selected by the authenticated
/// caller. Do not release an orphan reservation solely because its client left.
pub(crate) fn reserve(
    data_dir: &Path,
    operation: &str,
    bytes: u64,
    maximum_total: u64,
    minimum_free: u64,
    limits: &Limits<'_>,
) -> Result<Reservation> {
    reserve_until(
        data_dir,
        operation,
        bytes,
        maximum_total,
        minimum_free,
        limits,
        Instant::now() + Duration::from_secs(30),
    )
}

pub(crate) fn reserve_until(
    data_dir: &Path,
    operation: &str,
    bytes: u64,
    maximum_total: u64,
    minimum_free: u64,
    limits: &Limits<'_>,
    deadline: Instant,
) -> Result<Reservation> {
    anyhow::ensure!(
        !operation.is_empty() && operation.len() <= 256 && bytes > 0 && bytes <= limits.max_bytes,
        "Invalid snapshot admission request"
    );
    let data = io::open_directory(&data_dir.canonicalize()?)?;
    let root = io::private_directory(&data, "snapshot-reservations")?;
    let key = hex::encode(Sha256::digest(operation.as_bytes()));
    let claims = io::private_directory(&root, "claims")?;
    let claim = io::private_directory(&claims, &key)?;
    // Wait for this operation without holding the shared admission lock. The
    // claim survives as a harmless empty directory; its flock ends on process exit.
    io::lock_operation(&claim, limits, deadline)?;
    io::lock_operation(&root, limits, deadline)?;
    let name = format!("{key}.json");
    let existing: Option<Record> = io::read_record(&root, &name)?;
    let additional = bytes
        .checked_add(128 * 1024)
        .context("Reservation overflow")?;
    if let Some(saved) = &existing {
        anyhow::ensure!(
            saved.version == 1 && saved.operation == operation && saved.bytes == additional,
            "Original snapshot reservation terms changed; preserve for recovery"
        );
    }
    let newly_reserved = if existing.is_some() { 0 } else { additional };
    let mut stored = 0u64;
    for name in ["content-snapshots", "media-registration"] {
        match io::open_at(&data, name, libc::O_RDONLY | libc::O_DIRECTORY, 0) {
            Ok(directory) => {
                stored = stored
                    .checked_add(count(&directory)?)
                    .context("Storage accounting overflow")?
            }
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) => {}
            Err(error) => return Err(error),
        }
    }
    let outstanding = reserved(&root)?;
    anyhow::ensure!(
        stored
            .checked_add(outstanding)
            .and_then(|v| v.checked_add(newly_reserved))
            .is_some_and(|total| total <= maximum_total),
        "Immutable media storage budget is full"
    );
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    anyhow::ensure!(
        unsafe { libc::fstatvfs(data.as_raw_fd(), stat.as_mut_ptr()) } == 0,
        "Cannot inspect snapshot storage capacity"
    );
    let stat = unsafe { stat.assume_init() };
    let free = (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64);
    let required = outstanding
        .checked_add(newly_reserved)
        .and_then(|v| v.checked_add(minimum_free))
        .context("Snapshot free-space requirement overflow")?;
    anyhow::ensure!(
        free >= required,
        "Not enough free storage for immutable media"
    );
    if existing.is_none() {
        io::save_record(
            &root,
            &name,
            &Record {
                version: 1,
                operation: operation.into(),
                bytes: additional,
            },
        )?;
    }
    // flock is on this open description; release before expensive media copying.
    anyhow::ensure!(
        unsafe { libc::flock(root.as_raw_fd(), libc::LOCK_UN) } == 0,
        "Cannot release admission lock"
    );
    Ok(Reservation {
        _claim: claim,
        root,
        name,
    })
}
/// Call only after the original operation's immutable bytes and durable record
/// have been verified. This permits recovery/cleanup even when current admission
/// capacity is exhausted; it authorizes no new copy and touches no media bytes.
pub(crate) fn finish_completed(
    data_dir: &Path,
    operation: &str,
    bytes: u64,
    limits: &Limits<'_>,
) -> Result<()> {
    let data = io::open_directory(&data_dir.canonicalize()?)?;
    let root = match io::open_at(
        &data,
        "snapshot-reservations",
        libc::O_RDONLY | libc::O_DIRECTORY,
        0,
    ) {
        Ok(root) => root,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(())
        }
        Err(error) => return Err(error),
    };
    let key = hex::encode(Sha256::digest(operation.as_bytes()));
    let claims = io::private_directory(&root, "claims")?;
    let claim = io::private_directory(&claims, &key)?;
    io::lock_operation(&claim, limits, Instant::now() + Duration::from_secs(30))?;
    io::lock_operation(&root, limits, Instant::now() + Duration::from_secs(30))?;
    let name = format!("{key}.json");
    if let Some(record) = io::read_record::<Record>(&root, &name)? {
        anyhow::ensure!(
            record.version == 1
                && record.operation == operation
                && bytes.checked_add(128 * 1024) == Some(record.bytes),
            "Completed snapshot reservation terms changed; preserve for recovery"
        );
        let name = std::ffi::CString::new(name)?;
        anyhow::ensure!(
            unsafe { libc::unlinkat(root.as_raw_fd(), name.as_ptr(), 0) } == 0,
            "Cannot release completed snapshot reservation"
        );
        root.sync_all()?;
    }
    Ok(())
}

impl Reservation {
    /// After success OR a stopped copy, retained/partial files count against the
    /// stored-byte budget. Caller must stop writing before returning reservation.
    pub(crate) fn finish(self, limits: &Limits<'_>) -> Result<()> {
        io::lock_operation(&self.root, limits, Instant::now() + Duration::from_secs(30))?;
        let name = std::ffi::CString::new(self.name)?;
        anyhow::ensure!(
            unsafe { libc::unlinkat(self.root.as_raw_fd(), name.as_ptr(), 0) } == 0,
            "Cannot release snapshot reservation; preserve operation for recovery"
        );
        self.root.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    #[test]
    fn reservations_share_both_storage_roots_and_do_not_hold_copy_lock() {
        let temp = tempfile::tempdir().unwrap();
        let cancelled = AtomicBool::new(false);
        let limits = Limits {
            max_bytes: 8 * 1024 * 1024,
            cancelled: &cancelled,
        };
        for name in ["content-snapshots", "media-registration"] {
            std::fs::create_dir(temp.path().join(name)).unwrap();
            File::create(temp.path().join(name).join("retained-media"))
                .unwrap()
                .set_len(1024 * 1024)
                .unwrap();
        }
        // Two stores already occupy 2MiB. Both guards coexist, proving the
        // admission lock does not serialize the subsequent expensive copies.
        let first = reserve(
            temp.path(),
            "first",
            1024 * 1024,
            5 * 1024 * 1024,
            0,
            &limits,
        )
        .unwrap();
        let second = reserve(
            temp.path(),
            "second",
            1024 * 1024,
            5 * 1024 * 1024,
            0,
            &limits,
        )
        .unwrap();
        assert!(reserve(
            temp.path(),
            "third",
            1024 * 1024,
            5 * 1024 * 1024,
            0,
            &limits
        )
        .is_err());
        first.finish(&limits).unwrap();
        let third = reserve(
            temp.path(),
            "third",
            1024 * 1024,
            5 * 1024 * 1024,
            0,
            &limits,
        )
        .unwrap();
        second.finish(&limits).unwrap();
        third.finish(&limits).unwrap();
    }
    #[test]
    fn interrupted_reservation_stays_counted_and_symlink_storage_rejects() {
        let temp = tempfile::tempdir().unwrap();
        let cancelled = AtomicBool::new(false);
        let limits = Limits {
            max_bytes: 8 * 1024 * 1024,
            cancelled: &cancelled,
        };
        let held = reserve(
            temp.path(),
            "interrupted",
            2 * 1024 * 1024,
            3 * 1024 * 1024,
            0,
            &limits,
        )
        .unwrap();
        drop(held); // process death does not erase a durable outstanding liability
        let resumed = reserve(
            temp.path(),
            "interrupted",
            2 * 1024 * 1024,
            3 * 1024 * 1024,
            0,
            &limits,
        )
        .unwrap();
        drop(resumed);
        assert!(reserve(
            temp.path(),
            "interrupted",
            1024 * 1024,
            3 * 1024 * 1024,
            0,
            &limits
        )
        .is_err());
        assert!(reserve(
            temp.path(),
            "other",
            1024 * 1024,
            3 * 1024 * 1024,
            0,
            &limits
        )
        .is_err());
        std::os::unix::fs::symlink(temp.path(), temp.path().join("media-registration")).unwrap();
        assert!(reserve(temp.path(), "symlink", 1, 16 * 1024 * 1024, 0, &limits).is_err());
    }
    #[test]
    fn completed_cleanup_works_without_new_admission_and_checks_original_size() {
        let temp = tempfile::tempdir().unwrap();
        let cancelled = AtomicBool::new(false);
        let limits = Limits {
            max_bytes: 8 * 1024 * 1024,
            cancelled: &cancelled,
        };
        let original = reserve(
            temp.path(),
            "complete",
            1024 * 1024,
            2 * 1024 * 1024,
            0,
            &limits,
        )
        .unwrap();
        drop(original);
        assert!(finish_completed(temp.path(), "complete", 2 * 1024 * 1024, &limits).is_err());
        finish_completed(temp.path(), "complete", 1024 * 1024, &limits).unwrap();
        finish_completed(temp.path(), "complete", 1024 * 1024, &limits).unwrap();
        let root = io::open_directory(&temp.path().join("snapshot-reservations")).unwrap();
        assert_eq!(reserved(&root).unwrap(), 0);
    }
    #[test]
    fn expired_admission_deadline_never_records_a_new_reservation() {
        let temp = tempfile::tempdir().unwrap();
        let cancelled = AtomicBool::new(false);
        let limits = Limits {
            max_bytes: 1024,
            cancelled: &cancelled,
        };
        assert!(reserve_until(
            temp.path(),
            "expired",
            10,
            1024 * 1024,
            0,
            &limits,
            Instant::now()
        )
        .is_err());
        let root = io::open_directory(&temp.path().join("snapshot-reservations")).unwrap();
        assert_eq!(reserved(&root).unwrap(), 0);
    }
}
