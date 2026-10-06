//! Persistent signing material must never fall back to an ephemeral key.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub(super) fn read_existing(path: &Path) -> io::Result<Vec<u8>> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() != 32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Session key must be a regular 32-byte file; existing data was preserved",
        ));
    }
    // Tighten legacy modes on the opened inode. Permission failures are errors,
    // never permission to replace a valid key or start with a temporary one.
    if metadata.permissions().mode() & 0o777 != 0o600 {
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.sync_all()?;
    }
    let mut bytes = Vec::with_capacity(32);
    file.take(33).read_to_end(&mut bytes)?;
    if bytes.len() != 32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Session key changed while reading",
        ));
    }
    Ok(bytes)
}

struct TemporaryKey(PathBuf);
impl Drop for TemporaryKey {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(super) fn load_or_create(path: &Path) -> io::Result<Vec<u8>> {
    match read_existing(path) {
        Ok(key) => return Ok(key),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Session key has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    let mut key = [0u8; 32];
    crate::entropy::draw_key_bytes(&mut rand::rngs::OsRng, &mut key)
        .map_err(|_| io::Error::other("Session key entropy unavailable"))?;
    // Publish only a fully written, synced private inode. A hard link provides
    // no-replace semantics: concurrent creators all read the one winning key.
    // The temporary filename is independent of the secret.
    let temporary_path = parent.join(format!(".session-key-{}", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary_path)?;
    let temporary = TemporaryKey(temporary_path);
    file.write_all(&key)?;
    file.sync_all()?;
    match fs::hard_link(&temporary.0, path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    drop(temporary);
    File::open(parent)?.sync_all()?;
    read_existing(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn durable_private_key_survives_reload_without_rotation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");
        let key = load_or_create(&path).unwrap();
        assert_eq!(key.len(), 32);
        assert_eq!(key, load_or_create(&path).unwrap());
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn malformed_existing_key_is_preserved_and_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");
        fs::write(&path, b"partial key").unwrap();
        assert_eq!(
            load_or_create(&path).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(fs::read(path).unwrap(), b"partial key");
    }

    #[test]
    fn legacy_mode_is_tightened_without_changing_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");
        fs::write(&path, [42; 32]).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(load_or_create(&path).unwrap(), vec![42; 32]);
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn symlinks_and_non_files_are_rejected_without_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        fs::write(&target, [42; 32]).unwrap();
        let link = dir.path().join("link");
        symlink(&target, &link).unwrap();
        assert!(load_or_create(&link).is_err());
        assert!(load_or_create(dir.path()).is_err());
        assert_eq!(fs::read(target).unwrap(), vec![42; 32]);
        assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
    }

    #[test]
    fn failed_storage_never_returns_an_ephemeral_key() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("not-a-directory");
        fs::write(&parent, b"preserve").unwrap();
        assert!(load_or_create(&parent.join("key")).is_err());
        assert_eq!(fs::read(parent).unwrap(), b"preserve");
    }

    #[test]
    fn unreadable_existing_key_is_not_replaced() {
        use std::os::fd::AsRawFd;
        use std::os::unix::process::CommandExt;
        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        let path = dir.path().join("key");
        fs::write(&path, [42; 32]).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        if unsafe { libc::geteuid() } == 0 {
            // The isolated runner is root. Probe as an unprivileged child so
            // DAC_OVERRIDE cannot hide the exact production failure.
            // Execute an already-open inode: the test checkout may live under
            // a private home directory which the probe must not traverse.
            let executable = File::open(std::env::current_exe().unwrap()).unwrap();
            let status =
                std::process::Command::new(format!("/proc/self/fd/{}", executable.as_raw_fd()))
                    .args([
                        "--ignored",
                        "--exact",
                        "session::secret_file::tests::permission_denied_child_probe",
                    ])
                    .env("ARCHY_SESSION_KEY_PERMISSION_PROBE", &path)
                    .uid(65534)
                    .gid(65534)
                    .status()
                    .unwrap();
            assert!(status.success());
        } else {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
            assert_eq!(
                load_or_create(&path).unwrap_err().kind(),
                io::ErrorKind::PermissionDenied
            );
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert_eq!(fs::read(path).unwrap(), vec![42; 32]);
    }

    #[test]
    #[ignore = "Executed by unreadable_existing_key_is_not_replaced as an unprivileged child"]
    fn permission_denied_child_probe() {
        let path = PathBuf::from(
            std::env::var_os("ARCHY_SESSION_KEY_PERMISSION_PROBE")
                .expect("parent provides private fixture path"),
        );
        assert_eq!(
            load_or_create(&path).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn concurrent_first_boot_creators_agree_on_one_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    load_or_create(&path).unwrap()
                })
            })
            .collect();
        let keys: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        for key in &keys {
            assert_eq!(key, &keys[0]);
        }
        assert_eq!(fs::read(path).unwrap(), keys[0]);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
