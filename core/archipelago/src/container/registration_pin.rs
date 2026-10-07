//! Installer-owned public identity bindings for opted-in media-registration apps.
//! No identity generation, app enablement, signing permission or data deletion.
use anyhow::{Context, Result};
use archipelago_container::AppManifest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const ROOT: &str = "app-registration-pins";
const MAX_RECORD: u64 = 16 * 1024;
const ENV_NAMES: [&str; 3] = [
    "ARCHIPELAGO_REGISTRATION_NODE_PUBLIC_KEY",
    "ARCHIPELAGO_REGISTRATION_NODE_DID",
    "ARCHIPELAGO_REGISTRATION_AUDIENCE",
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrationPin {
    version: u8,
    pub app_id: String,
    pub node_public_key: String,
    pub node_did: String,
    pub app_audience: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    version: u8,
    app_id: String,
    pin_sha256: String,
}
fn valid_app_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('-')
        && !id.ends_with('-')
        && !id.contains("--")
        && id
            .bytes()
            .all(|v| v.is_ascii_lowercase() || v.is_ascii_digit() || v == b'-')
}
fn paths(data_dir: &Path, app_id: &str) -> Result<(PathBuf, PathBuf, PathBuf)> {
    anyhow::ensure!(
        valid_app_id(app_id),
        "Invalid application registration scope"
    );
    let root = data_dir.join(ROOT);
    Ok((
        root.clone(),
        root.join(format!("{app_id}.json")),
        root.join(format!("{app_id}.initialized.json")),
    ))
}
fn private_root(path: &Path) -> Result<File> {
    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    let metadata = dir.metadata()?;
    anyhow::ensure!(
        metadata.uid() == unsafe { libc::geteuid() } && metadata.mode() & 0o077 == 0,
        "Application registration pins must remain private to the node service"
    );
    Ok(dir)
}
fn lock(root: &File) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if unsafe { libc::flock(root.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if !matches!(
            error.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
        ) {
            return Err(error.into());
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "Application registration provisioning is busy; retry the same install"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .context("Application registration pin is unavailable; preserve it for recovery")
        }
    };
    let metadata = file.metadata()?;
    anyhow::ensure!(
        metadata.is_file() && metadata.len() <= MAX_RECORD && metadata.mode() & 0o077 == 0,
        "Invalid application registration pin storage"
    );
    let mut bytes = Vec::new();
    file.take(MAX_RECORD + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_RECORD,
        "Application registration pin is too large"
    );
    Ok(Some(serde_json::from_slice(&bytes).context(
        "Damaged application registration pin; do not replace it",
    )?))
}
fn persist<T: Serialize>(root: &Path, path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    let mut pending = tempfile::NamedTempFile::new_in(root)?;
    pending.write_all(&bytes)?;
    pending.as_file().sync_all()?;
    pending.persist_noclobber(path).map_err(|error| {
        anyhow::anyhow!(
            "Could not commit application registration pin: {}",
            error.error
        )
    })?;
    private_root(root)?.sync_all()?;
    Ok(())
}
fn validate(
    pin: &RegistrationPin,
    app_id: &str,
    identity: &crate::identity::NodeIdentity,
) -> Result<()> {
    let audience =
        uuid::Uuid::parse_str(&pin.app_audience).context("Invalid saved application audience")?;
    anyhow::ensure!(pin.version == 1 && pin.app_id == app_id
        && pin.node_public_key == identity.pubkey_hex() && pin.node_did == identity.did_key()?
        && audience.get_version_num() == 4 && audience.get_variant() == uuid::Variant::RFC4122
        && audience.to_string() == pin.app_audience,
        "Application registration identity changed; preserve the existing pin and migrate explicitly");
    Ok(())
}
fn commitment(pin: &RegistrationPin) -> Result<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(pin)?)))
}
fn validate_marker(marker: &Marker, pin: &RegistrationPin) -> Result<()> {
    anyhow::ensure!(
        marker.version == 1 && marker.app_id == pin.app_id && marker.pin_sha256 == commitment(pin)?,
        "Application registration initialization record changed; preserve both records"
    );
    Ok(())
}

/// Installer-only provisioning. Run on a blocking worker. Existing data is never
/// replaced or repaired by generating another audience. Does not load/create keys.
pub fn ensure_for_installation(
    data_dir: &Path,
    app_id: &str,
    identity: &crate::identity::NodeIdentity,
) -> Result<RegistrationPin> {
    let (root, path, marker_path) = paths(data_dir, app_id)?;
    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700);
    if let Err(error) = builder.create(&root) {
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(error.into());
        }
    }
    let held = private_root(&root)?;
    File::open(data_dir)?.sync_all()?;
    lock(&held)?;
    let marker: Option<Marker> = read(&marker_path)?;
    let pin = match read::<RegistrationPin>(&path)? {
        Some(pin) => {
            validate(&pin, app_id, identity)?;
            pin
        }
        None => {
            anyhow::ensure!(marker.is_none(), "Previously provisioned application pin is missing; recover it instead of creating another audience");
            let pin = RegistrationPin {
                version: 1,
                app_id: app_id.into(),
                node_public_key: identity.pubkey_hex(),
                node_did: identity.did_key()?,
                app_audience: uuid::Uuid::new_v4().to_string(),
            };
            persist(&root, &path, &pin)?;
            pin
        }
    };
    if let Some(marker) = marker {
        validate_marker(&marker, &pin)?;
    } else {
        persist(
            &root,
            &marker_path,
            &Marker {
                version: 1,
                app_id: app_id.into(),
                pin_sha256: commitment(&pin)?,
            },
        )?;
    }
    held.sync_all()?;
    Ok(pin)
}

/// Caller-side lookup never creates installation state. Require this before
/// accepting a native registration request for the installed application scope.
pub fn load_existing(
    data_dir: &Path,
    app_id: &str,
    identity: &crate::identity::NodeIdentity,
) -> Result<RegistrationPin> {
    let (root, path, marker_path) = paths(data_dir, app_id)?;
    let held = private_root(&root)?;
    lock(&held)?;
    let pin: RegistrationPin =
        read(&path)?.context("Application registration identity is not provisioned")?;
    validate(&pin, app_id, identity)?;
    let marker: Marker =
        read(&marker_path)?.context("Application registration provisioning is incomplete")?;
    validate_marker(&marker, &pin)?;
    Ok(pin)
}

/// Installer identity pins are authoritative. No flag enabling registration or
/// publication is set, and non-opted-in apps are not modified.
pub fn apply_environment(manifest: &mut AppManifest, pin: &RegistrationPin) -> Result<()> {
    anyhow::ensure!(
        manifest.app.container.media_registration_identity && manifest.app.id == pin.app_id,
        "Registration pin does not belong to this opted-in application"
    );
    anyhow::ensure!(!manifest.app.container.derived_env.iter().any(|entry| ENV_NAMES.contains(&entry.key.as_str()))
        && !manifest.app.container.secret_env.iter().any(|entry| ENV_NAMES.contains(&entry.key.as_str()))
        && !manifest.app.container.secret_env_refs.iter().any(|entry| ENV_NAMES.contains(&entry.env_key.as_str())),
        "Registration identity variables cannot be replaced by derived or secret environment entries");
    manifest.app.environment.retain(|entry| {
        !entry
            .split_once('=')
            .is_some_and(|(key, _)| ENV_NAMES.contains(&key))
    });
    manifest.app.environment.extend([
        format!("{}={}", ENV_NAMES[0], pin.node_public_key),
        format!("{}={}", ENV_NAMES[1], pin.node_did),
        format!("{}={}", ENV_NAMES[2], pin.app_audience),
    ]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    async fn fixture() -> (tempfile::TempDir, crate::identity::NodeIdentity) {
        let root = tempfile::tempdir().unwrap();
        let identity = crate::identity::NodeIdentity::load_or_create(&root.path().join("identity"))
            .await
            .unwrap();
        (root, identity)
    }
    #[tokio::test]
    async fn audience_is_stable_across_retries_reconstruction_and_other_apps_are_distinct() {
        let (root, identity) = fixture().await;
        assert!(load_existing(root.path(), "indeedhub-api", &identity).is_err());
        assert!(!root.path().join(ROOT).exists());
        let original_key = std::fs::read(root.path().join("identity/node_key")).unwrap();
        let first = ensure_for_installation(root.path(), "indeedhub-api", &identity).unwrap();
        let reloaded = crate::identity::NodeIdentity::load_existing(&root.path().join("identity"))
            .await
            .unwrap();
        assert_eq!(
            ensure_for_installation(root.path(), "indeedhub-api", &reloaded).unwrap(),
            first
        );
        assert_eq!(
            load_existing(root.path(), "indeedhub-api", &reloaded).unwrap(),
            first
        );
        assert_ne!(
            ensure_for_installation(root.path(), "another-app", &reloaded)
                .unwrap()
                .app_audience,
            first.app_audience
        );
        assert_eq!(
            std::fs::read(root.path().join("identity/node_key")).unwrap(),
            original_key
        );
    }
    #[tokio::test]
    async fn concurrent_installers_persist_one_audience() {
        let (root, identity) = fixture().await;
        let identity = Arc::new(identity);
        let workers: Vec<_> = (0..4)
            .map(|_| {
                let identity = identity.clone();
                let path = root.path().to_owned();
                std::thread::spawn(move || {
                    ensure_for_installation(&path, "indeedhub-api", &identity).unwrap()
                })
            })
            .collect();
        let results: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect();
        assert!(results.iter().all(|pin| pin == &results[0]));
    }
    #[tokio::test]
    async fn corrupt_missing_or_foreign_pin_is_preserved_without_rotation() {
        let (root, identity) = fixture().await;
        let first = ensure_for_installation(root.path(), "indeedhub-api", &identity).unwrap();
        let (_, path, marker) = paths(root.path(), "indeedhub-api").unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let marker_bytes = std::fs::read(&marker).unwrap();
        let (_other_root, other_identity) = fixture().await;
        assert!(ensure_for_installation(root.path(), "indeedhub-api", &other_identity).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        std::fs::write(&path, b"damaged").unwrap();
        assert!(ensure_for_installation(root.path(), "indeedhub-api", &identity).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"damaged");
        std::fs::remove_file(&path).unwrap();
        assert!(ensure_for_installation(root.path(), "indeedhub-api", &identity).is_err());
        assert!(!path.exists());
        assert_eq!(std::fs::read(&marker).unwrap(), marker_bytes);
        std::fs::write(&path, &bytes).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            load_existing(root.path(), "indeedhub-api", &identity).unwrap(),
            first
        );
    }
    #[tokio::test]
    async fn interrupted_marker_commit_finishes_with_the_same_pin() {
        let (root, identity) = fixture().await;
        let first = ensure_for_installation(root.path(), "indeedhub-api", &identity).unwrap();
        let (_, path, marker) = paths(root.path(), "indeedhub-api").unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(marker).unwrap();
        assert!(load_existing(root.path(), "indeedhub-api", &identity).is_err());
        assert_eq!(
            ensure_for_installation(root.path(), "indeedhub-api", &identity).unwrap(),
            first
        );
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    #[tokio::test]
    async fn no_pin_symlink_or_manifest_override_is_followed() {
        let (root, identity) = fixture().await;
        let pin = ensure_for_installation(root.path(), "indeedhub-api", &identity).unwrap();
        let (_, path, _) = paths(root.path(), "indeedhub-api").unwrap();
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(root.path().join("identity/node_key"), &path).unwrap();
        assert!(ensure_for_installation(root.path(), "indeedhub-api", &identity).is_err());
        let mut manifest = AppManifest::parse("app:\n  id: indeedhub-api\n  name: Fixture\n  version: '1'\n  container:\n    image: fixture:1\n    media_registration_identity: true\n  environment:\n    - ARCHIPELAGO_REGISTRATION_AUDIENCE=body-value\n").unwrap();
        apply_environment(&mut manifest, &pin).unwrap();
        let first = manifest.app.environment.clone();
        apply_environment(&mut manifest, &pin).unwrap();
        assert_eq!(manifest.app.environment, first);
        assert!(first.contains(&format!(
            "ARCHIPELAGO_REGISTRATION_AUDIENCE={}",
            pin.app_audience
        )));
        assert!(!first.iter().any(|entry| entry.contains("_ENABLED=")));
        manifest.app.container.media_registration_identity = false;
        assert!(apply_environment(&mut manifest, &pin).is_err());
    }
}
