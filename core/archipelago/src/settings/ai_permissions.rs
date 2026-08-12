//! What the assistant is allowed to read about this node.
//!
//! # Why this lives on the node and not in the browser
//!
//! These grants were stored in `localStorage`, which is scoped to an
//! **origin**. A node answers on several: its LAN address, its Tailscale
//! address, `<host>.local`, and its hostname. Granting "Media" over the LAN
//! and returning over Tailscale showed every switch off again — not reset,
//! simply never set *there*. Operator-reported as "the AI Data Access settings
//! are not persistent through sessions, often turns them all off", and it made
//! a working content path look broken: every scope silently returns nothing
//! without a grant, so an ungranted permission is indistinguishable from an
//! empty library.
//!
//! The grant answers "what may the assistant read **about this node**". That is
//! a property of the node, not of one browser at one address, so the node is
//! where it belongs. Stored here it survives a cache clear, a new device, a
//! different browser, and any change of address.
//!
//! # Why the category list is not validated against a hardcoded set
//!
//! The authoritative list of categories lives in the UI
//! (`AI_PERMISSION_CATEGORIES`) and in the context broker's `fetchAndSanitize`.
//! Duplicating it here would create a third copy that silently drops a new
//! category on upgrade — the grant would round-trip through an older node and
//! come back missing. Unknown strings are stored verbatim and simply never
//! match a fetch, which fails closed. **Storing a category grants nothing on
//! its own**: the broker checks each category before it fetches, and the node
//! re-checks before it answers (T-13-33). This file records intent; it is not
//! the enforcement point.

use serde::{Deserialize, Serialize};
use std::path::Path;

const FILE_PATH: &str = "settings/ai_permissions.json";

/// Defence against a hand-edited or hostile file turning into unbounded
/// memory. Far above any real category count.
const MAX_CATEGORIES: usize = 64;
const MAX_CATEGORY_LEN: usize = 64;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiPermissions {
    /// Categories the operator has granted. Empty means the assistant reads
    /// nothing about this node, which is the default: a fresh node grants
    /// nothing until asked.
    #[serde(default)]
    pub granted: Vec<String>,
}

impl AiPermissions {
    /// Drop anything malformed and de-duplicate. Applied on both read and
    /// write so a hand-edited file cannot produce a state the UI can never
    /// display or undo.
    pub fn sanitized(mut self) -> Self {
        self.granted.retain(|c| {
            !c.is_empty()
                && c.len() <= MAX_CATEGORY_LEN
                // Category ids are lowercase kebab (`ai-local`). Anything else
                // is not something this node will ever match against.
                && c.chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        });
        self.granted.sort();
        self.granted.dedup();
        self.granted.truncate(MAX_CATEGORIES);
        self
    }

    pub fn is_granted(&self, category: &str) -> bool {
        self.granted.iter().any(|c| c == category)
    }
}

pub async fn load(data_dir: &Path) -> AiPermissions {
    let path = data_dir.join(FILE_PATH);
    match tokio::fs::read(&path).await {
        Ok(bytes) => serde_json::from_slice::<AiPermissions>(&bytes)
            .map(AiPermissions::sanitized)
            .unwrap_or_else(|e| {
                // Fail closed. An unreadable grant file must not be treated as
                // "everything allowed" — the assistant simply reads nothing
                // until the operator sets it again.
                tracing::warn!(error = %e, "AI permissions unreadable; granting nothing");
                AiPermissions::default()
            }),
        // Absent file is the ordinary first-run case, not an error.
        Err(_) => AiPermissions::default(),
    }
}

pub async fn save(data_dir: &Path, perms: AiPermissions) -> anyhow::Result<AiPermissions> {
    let perms = perms.sanitized();
    let path = data_dir.join(FILE_PATH);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Temp + rename so a crash mid-write cannot leave a truncated file that
    // reads as "no grants" on the next boot.
    let tmp = path.with_extension("json.tmp");
    tokio::fs::write(&tmp, serde_json::to_vec_pretty(&perms)?).await?;
    tokio::fs::rename(&tmp, &path).await?;
    Ok(perms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_node_grants_nothing() {
        assert!(AiPermissions::default().granted.is_empty());
        assert!(!AiPermissions::default().is_granted("media"));
    }

    #[test]
    fn unknown_categories_survive_a_round_trip() {
        // A newer UI may grant a category this binary has never heard of.
        // Dropping it would silently revoke the grant on downgrade/upgrade.
        let p = AiPermissions {
            granted: vec!["media".into(), "some-future-category".into()],
        }
        .sanitized();
        assert!(p.is_granted("some-future-category"));
    }

    #[test]
    fn malformed_entries_are_dropped_not_stored() {
        let p = AiPermissions {
            granted: vec![
                "media".into(),
                "".into(),
                "UPPER".into(),
                "has space".into(),
                "../../etc/passwd".into(),
                "x".repeat(500),
            ],
        }
        .sanitized();
        assert_eq!(p.granted, vec!["media".to_string()]);
    }

    #[test]
    fn duplicates_collapse() {
        let p = AiPermissions {
            granted: vec!["media".into(), "media".into(), "files".into()],
        }
        .sanitized();
        assert_eq!(p.granted, vec!["files".to_string(), "media".to_string()]);
    }

    #[tokio::test]
    async fn absent_file_reads_as_no_grants_rather_than_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).await.granted.is_empty());
    }

    #[tokio::test]
    async fn a_corrupt_file_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_PATH);
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&path, b"{ not json").await.unwrap();
        // The dangerous failure would be defaulting to "all granted".
        assert!(load(dir.path()).await.granted.is_empty());
    }

    #[tokio::test]
    async fn saved_grants_survive_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        save(
            dir.path(),
            AiPermissions {
                granted: vec!["media".into(), "files".into()],
            },
        )
        .await
        .unwrap();
        let back = load(dir.path()).await;
        assert!(back.is_granted("media"));
        assert!(back.is_granted("files"));
        assert!(!back.is_granted("wallet"));
    }
}
