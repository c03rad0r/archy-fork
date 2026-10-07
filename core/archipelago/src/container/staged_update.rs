//! Durable, immutable preparation for updates that must remain stopped.
use anyhow::{Context, Result};
use archipelago_container::AppManifest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StagedUpdate {
    pub manifest: AppManifest,
    pub manifest_dir: PathBuf,
    pub ready: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: u32,
    payload: String,
    checksum: String,
}

impl StagedUpdate {
    pub fn image(&self) -> Result<&str> {
        anyhow::ensure!(
            self.manifest.app.container.build.is_none(),
            "Stopped updates of build-based apps require immutable image resolution"
        );
        let image = self
            .manifest
            .app
            .container
            .image
            .as_deref()
            .context("Missing staged image")?;
        let digest = image.rsplit_once("@sha256:").map(|(_, d)| d);
        anyhow::ensure!(
            digest.is_some_and(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit())),
            "Stopped update requires a digest-pinned image; app remains stopped"
        );
        Ok(image)
    }
}

fn record_path(root: &Path, directory: &str, app: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        !app.is_empty()
            && app.len() <= 128
            && app
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')),
        "Invalid staged app id"
    );
    Ok(root.join(directory).join(format!("{app}.json")))
}

fn path(root: &Path, app: &str) -> Result<PathBuf> {
    record_path(root, "staged-updates", app)
}

pub(super) async fn load(root: &Path, app: &str) -> Result<Option<StagedUpdate>> {
    load_at(root, "staged-updates", app).await
}

async fn load_at(root: &Path, directory: &str, app: &str) -> Result<Option<StagedUpdate>> {
    let bytes = match tokio::fs::read(record_path(root, directory, app)?).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let envelope: Envelope = serde_json::from_slice(&bytes)
        .context("Damaged staged update; refusing another version")?;
    anyhow::ensure!(envelope.schema == 1, "Unsupported staged update schema");
    anyhow::ensure!(
        envelope.checksum == hex::encode(Sha256::digest(envelope.payload.as_bytes())),
        "Staged update checksum mismatch"
    );
    let record: StagedUpdate =
        serde_json::from_str(&envelope.payload).context("Damaged staged manifest")?;
    record
        .manifest
        .validate()
        .context("Invalid staged manifest")?;
    anyhow::ensure!(
        record.manifest.app.id == app,
        "Staged update belongs to another app"
    );
    record.image()?;
    Ok(Some(record))
}

pub(super) async fn save(root: &Path, record: &StagedUpdate) -> Result<()> {
    save_at(root, "staged-updates", record).await
}

pub(super) async fn save_installed(root: &Path, record: &StagedUpdate) -> Result<()> {
    anyhow::ensure!(record.ready, "Cannot publish incomplete installed manifest");
    save_at(root, "installed-manifests", record).await
}

pub(super) async fn installed_manifest(
    root: &Path,
    app: &str,
    observed_image: &str,
) -> Result<Option<AppManifest>> {
    let Some(record) = load_at(root, "installed-manifests", app).await? else {
        return Ok(None);
    };
    let expected = record.image()?;
    let matches = matches_image(expected, observed_image);
    Ok((record.ready && matches).then_some(record.manifest))
}

pub(super) fn matches_image(expected: &str, observed: &str) -> bool {
    expected == observed
        || match (
            expected.rsplit_once("@sha256:"),
            observed.rsplit_once("@sha256:"),
        ) {
            (Some((_, a)), Some((_, b))) => a.len() == 64 && a.eq_ignore_ascii_case(b),
            _ => false,
        }
}

async fn save_at(root: &Path, directory: &str, record: &StagedUpdate) -> Result<()> {
    use std::{
        io::Write,
        os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    };
    record.image()?;
    let target = record_path(root, directory, &record.manifest.app.id)?;
    record
        .manifest
        .validate()
        .context("Invalid staged manifest")?;
    let payload = serde_json::to_string(record)?;
    let checksum = hex::encode(Sha256::digest(payload.as_bytes()));
    let bytes = serde_json::to_vec(&Envelope {
        schema: 1,
        payload,
        checksum,
    })?;
    // Bounded metadata commit stays in this poll while the caller owns its
    // lifecycle lock; cancellation cannot leave a late detached writer.
    (|| -> Result<()> {
        let parent = target.parent().context("Missing staged update directory")?;
        match std::fs::DirBuilder::new().mode(0o700).create(parent) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        // Persist the directory entry itself before an acknowledged record can
        // depend on it surviving a crash (syncing the child alone is insufficient).
        std::fs::File::open(parent.parent().context("Missing staged update parent")?)?
            .sync_all()?;
        let tmp = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            std::fs::rename(&tmp, &target)?;
            std::fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(tmp);
        }
        result
    })()
}

pub(super) async fn clear(root: &Path, app: &str) -> Result<()> {
    clear_at(root, "staged-updates", app).await
}

pub(super) async fn clear_installed(root: &Path, app: &str) -> Result<()> {
    clear_at(root, "installed-manifests", app).await
}

async fn clear_at(root: &Path, directory: &str, app: &str) -> Result<()> {
    let target = record_path(root, directory, app)?;
    // Bounded metadata commit stays in this poll while the caller owns its
    // lifecycle lock; cancellation cannot leave a late detached writer.
    (|| -> Result<()> {
        match std::fs::remove_file(&target) {
            Ok(()) => std::fs::File::open(target.parent().unwrap())?.sync_all()?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    })()
}

pub(super) async fn marked(root: &Path, file: &str, app: &str, container: &str) -> Result<bool> {
    let bytes = match tokio::fs::read(root.join(file)).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).with_context(|| format!("Cannot read {file}")),
    };
    let set: HashSet<String> =
        serde_json::from_slice(&bytes).with_context(|| format!("Damaged {file}"))?;
    Ok(set.contains(app) || set.contains(container) || set.contains(&format!("archy-{app}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn envelope_rejects_corruption_schema_and_invalid_manifest_and_is_private() {
        let root = tempfile::tempdir().unwrap();
        let manifest = AppManifest::parse(&format!("app:\n  id: music\n  name: Music\n  version: 1.0.0\n  container:\n    image: docker.io/library/alpine@sha256:{}\n", "a".repeat(64))).unwrap();
        let record = StagedUpdate {
            manifest,
            manifest_dir: root.path().into(),
            ready: true,
        };
        save(root.path(), &record)
            .now_or_never()
            .expect("Journal commit must finish before lifecycle lock cancellation is possible")
            .unwrap();
        let target = path(root.path(), "music").unwrap();
        assert_eq!(
            std::fs::metadata(target.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(load(root.path(), "music").await.unwrap().unwrap().ready);
        save_installed(root.path(), &record).await.unwrap();
        clear(root.path(), "music")
            .now_or_never()
            .expect("Journal clear must not leave a detached late deletion")
            .unwrap();
        assert!(load(root.path(), "music").await.unwrap().is_none());
        assert!(
            installed_manifest(root.path(), "music", record.image().unwrap())
                .await
                .unwrap()
                .is_some()
        );
        assert!(installed_manifest(
            root.path(),
            "music",
            &format!("docker.io/library/alpine@sha256:{}", "b".repeat(64))
        )
        .await
        .unwrap()
        .is_none());
        save(root.path(), &record).await.unwrap();
        let original = tokio::fs::read(&target).await.unwrap();
        let mut envelope: Envelope = serde_json::from_slice(&original).unwrap();
        envelope.payload = envelope.payload.replace("Music", "Changed Music");
        tokio::fs::write(&target, serde_json::to_vec(&envelope).unwrap())
            .await
            .unwrap();
        assert!(load(root.path(), "music")
            .await
            .unwrap_err()
            .to_string()
            .contains("checksum"));
        envelope = serde_json::from_slice(&original).unwrap();
        envelope.schema = 2;
        tokio::fs::write(&target, serde_json::to_vec(&envelope).unwrap())
            .await
            .unwrap();
        assert!(load(root.path(), "music").await.is_err());
        envelope = serde_json::from_slice(&original).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&envelope.payload).unwrap();
        value["manifest"]["app"]["container"]["image"] = serde_json::Value::Null;
        envelope.payload = serde_json::to_string(&value).unwrap();
        envelope.checksum = hex::encode(Sha256::digest(envelope.payload.as_bytes()));
        tokio::fs::write(&target, serde_json::to_vec(&envelope).unwrap())
            .await
            .unwrap();
        assert!(load(root.path(), "music").await.is_err());
    }
}

pub(super) async fn pending_ids(root: &Path) -> Result<Vec<String>> {
    let mut dir = match tokio::fs::read_dir(root.join("staged-updates")).await {
        Ok(dir) => dir,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    let mut ids = Vec::new();
    while let Some(entry) = dir.next_entry().await? {
        if let Some(name) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.strip_suffix(".json"))
        {
            ids.push(name.to_string());
        }
    }
    Ok(ids)
}
