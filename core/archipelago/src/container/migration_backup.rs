//! Consistent, private snapshots for declaratively opted-in network migrations.
use anyhow::{bail, Context, Result};
use archipelago_container::AppManifest;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub fn enabled(manifest: &AppManifest) -> Result<bool> {
    match manifest.app.extensions.get("backup_on_network_change") {
        None => Ok(false),
        Some(value) => value
            .as_bool()
            .context("backup_on_network_change must be boolean"),
    }
}

fn relative_sources(manifest: &AppManifest, data_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut sources = Vec::new();
    for volume in &manifest.app.volumes {
        if volume.options.iter().any(|v| v == "ro") || volume.volume_type == "tmpfs" {
            continue;
        }
        // A runtime socket is a connection, not application state.
        if volume.source == "/run/user/1000/podman/podman.sock" {
            continue;
        }
        if volume.volume_type != "bind" {
            bail!("network migration backup requires bind-mounted persistent state");
        }
        let path = Path::new(&volume.source);
        let relative = path
            .strip_prefix(data_dir)
            .context("network migration state must be inside the node data directory")?;
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            bail!("invalid network migration state path");
        }
        sources.push(relative.to_path_buf());
    }
    sources.sort();
    sources.dedup();
    let mut roots: Vec<PathBuf> = Vec::new();
    for source in sources {
        if !roots.iter().any(|root| source.starts_with(root)) {
            roots.push(source);
        }
    }
    if roots.is_empty() {
        bail!("network migration backup has no persistent state mounts");
    }
    Ok(roots)
}

/// Caller must gracefully stop the app before this function, and resume the old
/// service if it fails. No source files are changed or deleted by this operation.
pub async fn snapshot(
    manifest: &AppManifest,
    data_dir: &Path,
    previous_unit: Option<&[u8]>,
) -> Result<PathBuf> {
    let mut command = tokio::process::Command::new("podman");
    command.args(["unshare", "tar"]);
    snapshot_with_command(manifest, data_dir, previous_unit, command).await
}

async fn snapshot_with_command(
    manifest: &AppManifest,
    data_dir: &Path,
    previous_unit: Option<&[u8]>,
    mut command: tokio::process::Command,
) -> Result<PathBuf> {
    let sources = relative_sources(manifest, data_dir)?;
    let canonical_root = tokio::fs::canonicalize(data_dir).await?;
    for source in &sources {
        let path = data_dir.join(source);
        if tokio::fs::symlink_metadata(&path)
            .await?
            .file_type()
            .is_symlink()
        {
            bail!("network migration state mount is a symlink; explicit backup required");
        }
        let canonical = tokio::fs::canonicalize(&path).await?;
        if !canonical.starts_with(&canonical_root) {
            bail!("network migration state path resolves outside node data directory");
        }
    }
    let root = data_dir.join("migration-backups");
    tokio::fs::create_dir_all(&root).await?;
    tokio::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).await?;
    let dir = root.join(uuid::Uuid::new_v4().to_string());
    tokio::fs::create_dir(&dir).await?;
    tokio::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).await?;
    if let Some(unit) = previous_unit {
        let path = dir.join("previous.container");
        tokio::fs::write(&path, unit).await?;
        tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).await?;
        tokio::fs::File::open(&path).await?.sync_all().await?;
    }
    let partial = dir.join("state.tar.partial");
    let archive = dir.join("state.tar");
    let output = command
        .args([
            "--create",
            "--numeric-owner",
            "--acls",
            "--xattrs",
            "--file",
        ])
        .arg(&partial)
        .arg("--directory")
        .arg(data_dir)
        .arg("--")
        .args(&sources)
        .output()
        .await
        .context("start rootless migration snapshot")?;
    if !output.status.success() {
        // No tar stderr in public logs: it can contain private filenames.
        let _ = tokio::fs::remove_file(&partial).await;
        bail!("persistent-state snapshot failed; original state was left intact");
    }
    tokio::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o600)).await?;
    tokio::fs::File::open(&partial).await?.sync_all().await?;
    tokio::fs::rename(&partial, &archive).await?;
    let metadata = serde_json::json!({"app": manifest.app.id, "version": manifest.app.version,
        "network": manifest.app.container.network, "sources": sources});
    tokio::fs::write(
        dir.join("metadata.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )
    .await?;
    tokio::fs::File::open(&dir).await?.sync_all().await?;
    Ok(archive)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn portainer() -> AppManifest {
        AppManifest::parse(include_str!("../../../../apps/portainer/manifest.yml")).unwrap()
    }
    #[tokio::test]
    async fn stopped_state_archive_round_trips_database_compose_and_old_unit() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("portainer");
        tokio::fs::create_dir_all(state.join("compose"))
            .await
            .unwrap();
        tokio::fs::write(state.join("portainer.db"), b"fixture database")
            .await
            .unwrap();
        tokio::fs::write(state.join("compose/stack.yml"), b"services: {}\n")
            .await
            .unwrap();
        let mut m = portainer();
        m.app.volumes[0].source = state.display().to_string();
        m.app.volumes[1].source = state.join("compose").display().to_string();
        let archive = snapshot_with_command(
            &m,
            dir.path(),
            Some(b"old unit"),
            tokio::process::Command::new("tar"),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::metadata(&archive).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            tokio::fs::read(archive.parent().unwrap().join("previous.container"))
                .await
                .unwrap(),
            b"old unit"
        );
        let restored = tempfile::tempdir().unwrap();
        assert!(tokio::process::Command::new("tar")
            .arg("-xf")
            .arg(archive)
            .arg("-C")
            .arg(restored.path())
            .status()
            .await
            .unwrap()
            .success());
        assert_eq!(
            tokio::fs::read(restored.path().join("portainer/portainer.db"))
                .await
                .unwrap(),
            b"fixture database"
        );
        assert_eq!(
            tokio::fs::read(restored.path().join("portainer/compose/stack.yml"))
                .await
                .unwrap(),
            b"services: {}\n"
        );
        assert_eq!(
            tokio::fs::read(state.join("portainer.db")).await.unwrap(),
            b"fixture database"
        );
    }

    #[tokio::test]
    async fn failed_snapshot_never_publishes_archive_or_changes_original_state() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("portainer");
        tokio::fs::create_dir_all(state.join("compose"))
            .await
            .unwrap();
        tokio::fs::write(state.join("portainer.db"), b"unchanged")
            .await
            .unwrap();
        let mut m = portainer();
        m.app.volumes[0].source = state.display().to_string();
        m.app.volumes[1].source = state.join("compose").display().to_string();
        assert!(
            snapshot_with_command(&m, dir.path(), None, tokio::process::Command::new("false"))
                .await
                .is_err()
        );
        assert_eq!(
            tokio::fs::read(state.join("portainer.db")).await.unwrap(),
            b"unchanged"
        );
        for entry in std::fs::read_dir(dir.path().join("migration-backups")).unwrap() {
            assert!(!entry.unwrap().path().join("state.tar").exists());
        }
    }

    #[test]
    fn backup_covers_all_portainer_state_once_and_excludes_runtime_socket() {
        let m = portainer();
        assert!(enabled(&m).unwrap());
        assert_eq!(
            relative_sources(&m, Path::new("/var/lib/archipelago")).unwrap(),
            vec![PathBuf::from("portainer")]
        );
    }
    #[test]
    fn backup_refuses_unknown_state_locations_instead_of_silently_omitting_them() {
        let mut m = portainer();
        m.app.volumes[0].source = "/other/operator/state".into();
        assert!(relative_sources(&m, Path::new("/var/lib/archipelago")).is_err());
        m.app.volumes[0].source = "/var/lib/archipelago/../secret".into();
        assert!(relative_sources(&m, Path::new("/var/lib/archipelago")).is_err());
    }
}
