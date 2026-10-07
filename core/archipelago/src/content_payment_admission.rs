//! Outermost cross-rail admission, before wallet or payment journals.
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};
pub(crate) struct Guard {
    _file: fs::File,
}
pub(crate) async fn lock(data: &Path, buyer: &str, seller: &str, content: &str) -> Result<Guard> {
    let root = data.join("content-payment-admission");
    let key = hex::encode(Sha256::digest(serde_json::to_vec(&(
        buyer, seller, content,
    ))?));
    tokio::task::spawn_blocking(move || {
        use std::os::{
            fd::AsRawFd,
            unix::fs::{OpenOptionsExt, PermissionsExt},
        };
        fs::create_dir_all(&root)?;
        anyhow::ensure!(
            fs::symlink_metadata(&root)?.is_dir(),
            "Invalid payment admission directory"
        );
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(root.join(key))?;
        anyhow::ensure!(file.metadata()?.is_file(), "Invalid payment admission lock");
        loop {
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0 {
                break;
            }
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                return Err(error.into());
            }
        }
        Ok(Guard { _file: file })
    })
    .await?
}
