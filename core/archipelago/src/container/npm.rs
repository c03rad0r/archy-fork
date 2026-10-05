//! Preserve NPM's active persistent mounts across installer and runtime paths.
use anyhow::{bail, Context, Result};
use archipelago_container::AppManifest;
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct Storage {
    pub data: String,
    pub certificates: String,
}

impl Storage {
    pub fn bind_mounts(&self) -> Vec<String> {
        vec![
            format!("{}:/data", self.data),
            format!("{}:/etc/letsencrypt", self.certificates),
        ]
    }

    pub fn apply(&self, manifest: &mut AppManifest) -> Result<()> {
        for (target, source) in [
            ("/data", &self.data),
            ("/etc/letsencrypt", &self.certificates),
        ] {
            let matching: Vec<_> = manifest
                .app
                .volumes
                .iter_mut()
                .filter(|volume| volume.target == target)
                .collect();
            if matching.len() != 1 {
                bail!("NPM requires one persistent mount per storage target");
            }
            let volume = matching.into_iter().next().unwrap();
            if volume.volume_type != "bind" {
                bail!("NPM persistent state requires bind mounts");
            }
            volume.source.clone_from(source);
        }
        Ok(())
    }
}

fn parse_storage(bytes: &[u8]) -> Result<Storage> {
    let storage: Storage =
        serde_json::from_slice(bytes).context("invalid NPM storage resolution")?;
    for value in [&storage.data, &storage.certificates] {
        if !Path::new(value).is_absolute() || value.chars().any(|c| c.is_control() || c == ':') {
            bail!("invalid NPM persistent storage path");
        }
    }
    Ok(storage)
}

pub async fn resolve_storage() -> Result<Storage> {
    // Verify live mounts/database before any caller can stop or recreate NPM.
    // Remember only validated mount paths so later recreation, after the inspect
    // record is removed, still uses the operator's original storage.
    let mut command = tokio::process::Command::new("python3");
    command
        .args([
            "-c",
            include_str!("../../../../scripts/npm-public-bridge.py"),
            "--resolve",
            "--remember",
            "--prepare-realip",
        ])
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(75), command.output())
        .await
        .context("NPM storage resolution timed out; existing state preserved")??;
    if !output.status.success() {
        bail!("NPM storage resolution failed; inspect mount/database ambiguity or permissions before migration");
    }
    parse_storage(&output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_mounts_preserve_custom_and_legacy_paths() {
        for data in [
            "/var/lib/archipelago/nginx-proxy-manager/data",
            "/srv/operator npm",
        ] {
            let bytes = serde_json::to_vec(
                &serde_json::json!({"data": data, "certificates": "/srv/certificates"}),
            )
            .unwrap();
            let storage = parse_storage(&bytes).unwrap();
            assert_eq!(
                storage.bind_mounts(),
                [
                    format!("{data}:/data"),
                    "/srv/certificates:/etc/letsencrypt".into(),
                ]
            );
        }
    }

    #[test]
    fn invalid_resolution_cannot_become_a_container_mount() {
        for data in ["relative", "/srv/data:ro", "/srv/data\nother"] {
            let bytes =
                serde_json::to_vec(&serde_json::json!({"data": data, "certificates": "/certs"}))
                    .unwrap();
            assert!(parse_storage(&bytes).is_err());
        }
        assert!(parse_storage(b"{}").is_err());
    }
}
