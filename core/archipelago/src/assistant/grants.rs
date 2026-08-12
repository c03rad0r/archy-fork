//! D-16: default-closed permission-category grants, persisted under
//! `data_dir`. All ten `PermissionCategory` variants are closed on a fresh
//! node — nothing is shared with the model until the operator deliberately
//! opens a category. A missing or unreadable grants file is
//! `default_closed()`, never an error and never a permissive default: the
//! assistant looking unconfigured on a fresh node is an accepted cost
//! (13-CONTEXT.md D-16), not a bug to work around by defaulting open.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::PermissionCategory;

const GRANTS_FILE: &str = "assistant/grants.json";

/// The set of currently-open permission categories. Construct via
/// [`Grants::default_closed`] or [`Grants::load`] — never via a `Default`
/// impl that could silently be permissive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grants {
    categories: BTreeSet<PermissionCategory>,
}

impl Grants {
    /// D-16: a fresh node grants nothing. Every one of the ten categories is
    /// closed until the operator explicitly opens it.
    pub fn default_closed() -> Grants {
        Grants {
            categories: BTreeSet::new(),
        }
    }

    /// Whether `category` is currently open.
    pub fn allows(&self, category: PermissionCategory) -> bool {
        self.categories.contains(&category)
    }

    /// The full set of currently-open categories.
    pub fn categories(&self) -> &BTreeSet<PermissionCategory> {
        &self.categories
    }

    /// Open or close a single category. Callers must still call
    /// [`Grants::save`] to persist the change.
    pub fn set(&mut self, category: PermissionCategory, granted: bool) {
        if granted {
            self.categories.insert(category);
        } else {
            self.categories.remove(&category);
        }
    }

    /// Load the persisted grants for this node. A missing file, or one that
    /// fails to parse, is `default_closed()` — never an error, and never
    /// anything other than empty. This is the one place D-16's "nothing is
    /// shared with the model until deliberately granted" is enforced at the
    /// data layer; `CallerScope::granted_categories` has no other source of
    /// authority to fall back to.
    pub async fn load(data_dir: &Path) -> Grants {
        let path = data_dir.join(GRANTS_FILE);
        let Ok(content) = tokio::fs::read_to_string(&path).await else {
            return Grants::default_closed();
        };
        serde_json::from_str(&content).unwrap_or_else(|_| Grants::default_closed())
    }

    /// Whether a grants file exists on disk at all. `load` cannot say this —
    /// it maps "absent" and "present but empty" to the same value, and the
    /// unified `ai.permissions.get` reader needs the distinction: an existing
    /// file is authoritative, while an absent one triggers the one-time
    /// legacy migration.
    pub(crate) async fn exists(data_dir: &Path) -> bool {
        tokio::fs::metadata(data_dir.join(GRANTS_FILE))
            .await
            .is_ok()
    }

    /// Persist the grants for this node, 0600 (following
    /// `streaming/session.rs`'s `data_dir`-scoped persisted-state
    /// convention, and this codebase's convention of keeping
    /// non-world-readable anything that shapes what a model or a remote
    /// peer can reach on this node).
    pub async fn save(&self, data_dir: &Path) -> Result<()> {
        let dir = data_dir.join("assistant");
        tokio::fs::create_dir_all(&dir)
            .await
            .context("Failed to create assistant dir")?;
        let path = data_dir.join(GRANTS_FILE);
        let content = serde_json::to_string_pretty(self).context("Failed to serialize grants")?;
        tokio::fs::write(&path, &content)
            .await
            .context("Failed to write grants file")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).ok();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fresh_node_grants_are_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let grants = Grants::load(tmp.path()).await;
        assert!(
            grants.categories().is_empty(),
            "a fresh node with no grants file must grant nothing"
        );
        for category in PermissionCategory::ALL {
            assert!(
                !grants.allows(category),
                "{category:?} must be closed by default"
            );
        }
    }

    #[tokio::test]
    async fn grant_persists_across_load() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut grants = Grants::load(tmp.path()).await;
        grants.set(PermissionCategory::System, true);
        grants.save(tmp.path()).await.expect("save");

        let reloaded = Grants::load(tmp.path()).await;
        assert!(reloaded.allows(PermissionCategory::System));
        assert!(!reloaded.allows(PermissionCategory::Wallet));
    }

    #[tokio::test]
    async fn revoke_removes_the_category() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut grants = Grants::load(tmp.path()).await;
        grants.set(PermissionCategory::Network, true);
        grants.save(tmp.path()).await.expect("save");

        let mut grants = Grants::load(tmp.path()).await;
        grants.set(PermissionCategory::Network, false);
        grants.save(tmp.path()).await.expect("save");

        let reloaded = Grants::load(tmp.path()).await;
        assert!(!reloaded.allows(PermissionCategory::Network));
    }
}
