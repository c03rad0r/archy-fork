//! bitcoin-ui nginx.conf renderer.
//!
//! Step 7 of the rust-orchestrator migration. Replaces the old
//! `sed -i __BITCOIN_RPC_AUTH__` approach from `first-boot-containers.sh`
//! (which destructively overwrote its own template, broke on rotation,
//! and had no story for dual Knots/Core UIs) with a binary-embedded
//! template rendered at install/reconcile time and atomic-written to
//! disk.
//!
//! The manifest bind-mounts the rendered file read-only into the
//! container at `/etc/nginx/conf.d/default.conf`. On every reconcile
//! pass we re-render and compare — if the rendered bytes would differ
//! from what's on disk (password rotated, template changed via OTA),
//! we rewrite atomically and the reconciler restarts the container.
//!
//! Source of truth:
//!   * RPC user: hardcoded `archipelago` (matches the image's `bitcoin.conf`).
//!   * RPC password: `/var/lib/archipelago/secrets/bitcoin-rpc-password`,
//!     plaintext, written by the seed-derived credential setup.
//!
//! Both Knots and Core back-ends expose RPC on 127.0.0.1:8332 with the
//! same auth shape, so one template serves both.

use anyhow::{Context, Result};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;

/// The nginx.conf template. Embedded at compile time so it can never
/// drift from the code that renders it, and ships atomically with OTA.
///
/// `{{BITCOIN_RPC_AUTH}}` is the only placeholder — replaced with a
/// `base64(user:password)` blob at render time.
pub(crate) const TEMPLATE: &str = include_str!("bitcoin_ui_nginx.conf.template");

/// The single placeholder in `TEMPLATE`.
const PLACEHOLDER: &str = "{{BITCOIN_RPC_AUTH}}";

/// Hardcoded RPC user. Matches the user written into `bitcoin.conf` by
/// the bitcoin-core/bitcoin-knots bootstrap, and the legacy
/// `BITCOIN_RPC_USER="archipelago"` from `first-boot-containers.sh`.
const RPC_USER: &str = "archipelago";

/// Default path to the plaintext RPC password secret.
///
/// Written by the seed-derived credential flow; same file the bash
/// scripts read today at `first-boot-containers.sh:277` and `:1225`.
pub const DEFAULT_SECRET_PATH: &str = "/var/lib/archipelago/secrets/bitcoin-rpc-password";

/// Default output path for the rendered nginx.conf.
///
/// The manifest bind-mounts this file read-only into the bitcoin-ui
/// container at `/etc/nginx/conf.d/default.conf`.
pub const DEFAULT_RENDERED_PATH: &str = "/var/lib/archipelago/bitcoin-ui/nginx.conf";

/// Parameters for rendering. Injectable so tests can hit a tmpdir
/// instead of `/var/lib/archipelago`.
#[derive(Debug, Clone)]
pub struct RenderPaths {
    /// Path to read the plaintext RPC password from.
    pub secret_path: PathBuf,
    /// Path to write the rendered nginx.conf to.
    pub rendered_path: PathBuf,
}

impl Default for RenderPaths {
    fn default() -> Self {
        Self {
            secret_path: PathBuf::from(DEFAULT_SECRET_PATH),
            rendered_path: PathBuf::from(DEFAULT_RENDERED_PATH),
        }
    }
}

/// Outcome of a render pass. `Written` if the rendered bytes differed
/// from the current on-disk contents and we rewrote; `Unchanged` if
/// they matched and we left the file alone.
///
/// The caller (reconciler / install path) decides whether to restart
/// the bitcoin-ui container based on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderOutcome {
    Written,
    Unchanged,
}

/// Render the bitcoin-ui nginx.conf and atomic-write it to disk if it
/// differs from what's already there.
///
/// Idempotent: safe to call on every reconcile pass. Does a byte
/// comparison before writing so an unchanged password + template is a
/// no-op (no inode churn, no container restart cascade).
///
/// Errors if the secret file is missing or empty. Upstream callers
/// treat that as "bitcoin-ui isn't installable yet" rather than fatal
/// — the RPC password comes into being during bitcoin-core's own
/// bootstrap, which may not have happened yet on a fresh node.
pub async fn render(paths: &RenderPaths) -> Result<RenderOutcome> {
    let password = read_password(&paths.secret_path).await?;
    let auth_b64 = encode_basic_auth(RPC_USER, &password);
    let rendered = TEMPLATE.replace(PLACEHOLDER, &auth_b64);

    // Compare against existing. read-to-string fails on ENOENT (first
    // install) — treat as "different".
    let existing = fs::read_to_string(&paths.rendered_path).await.ok();
    if existing.as_deref() == Some(rendered.as_str()) {
        return Ok(RenderOutcome::Unchanged);
    }

    // Atomic write: write to sibling tmp + rename. Keeps the bind-
    // mounted file pointing at a fully-formed config at all times.
    let parent = paths
        .rendered_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("rendered_path has no parent directory"))?;
    fs::create_dir_all(parent)
        .await
        .with_context(|| format!("creating {}", parent.display()))?;

    let tmp = unique_tmp_path(&paths.rendered_path);
    fs::write(&tmp, &rendered)
        .await
        .with_context(|| format!("writing tmp {}", tmp.display()))?;
    fs::rename(&tmp, &paths.rendered_path)
        .await
        .with_context(|| {
            format!(
                "renaming {} -> {}",
                tmp.display(),
                paths.rendered_path.display()
            )
        })?;

    tracing::info!(
        path = %paths.rendered_path.display(),
        auth_hash = %short_hash(&auth_b64),
        "bitcoin-ui nginx.conf rendered"
    );

    Ok(RenderOutcome::Written)
}

fn unique_tmp_path(dest: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    dest.with_extension(format!("tmp.{ts}.{n}"))
}

/// Read the plaintext RPC password from disk. Trims trailing newlines
/// (common from `echo "$PASS" > file`) but rejects an empty result.
async fn read_password(path: &Path) -> Result<String> {
    let raw = fs::read_to_string(path)
        .await
        .with_context(|| format!("reading bitcoin RPC password from {}", path.display()))?;
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        anyhow::bail!(
            "bitcoin RPC password file {} is empty — bitcoin-core bootstrap hasn't written it yet",
            path.display()
        );
    }
    Ok(trimmed)
}

/// `base64("user:password")` — the value nginx puts after `Basic ` in
/// the upstream `Authorization` header.
fn encode_basic_auth(user: &str, password: &str) -> String {
    let raw = format!("{user}:{password}");
    base64::engine::general_purpose::STANDARD.encode(raw.as_bytes())
}

/// Short hash of the auth value for logging — we never want the
/// plaintext or full base64 in logs (it's a credential), but a stable
/// fingerprint helps correlate rotations.
fn short_hash(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let digest = hasher.finalize();
    hex::encode(&digest[..4])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn paths_in(dir: &Path, password: &str) -> RenderPaths {
        let secret = dir.join("bitcoin-rpc-password");
        std::fs::write(&secret, password).unwrap();
        RenderPaths {
            secret_path: secret,
            rendered_path: dir.join("nginx.conf"),
        }
    }

    #[tokio::test]
    async fn render_writes_file_with_substitution() {
        let tmp = TempDir::new().unwrap();
        let paths = paths_in(tmp.path(), "hunter2");
        let outcome = render(&paths).await.unwrap();
        assert_eq!(outcome, RenderOutcome::Written);
        let contents = std::fs::read_to_string(&paths.rendered_path).unwrap();
        // archipelago:hunter2 -> "YXJjaGlwZWxhZ286aHVudGVyMg=="
        assert!(
            contents.contains("YXJjaGlwZWxhZ286aHVudGVyMg=="),
            "base64 auth not found in rendered config:\n{contents}"
        );
        assert!(
            !contents.contains(PLACEHOLDER),
            "placeholder left in output"
        );
    }

    #[tokio::test]
    async fn render_is_idempotent_when_password_unchanged() {
        let tmp = TempDir::new().unwrap();
        let paths = paths_in(tmp.path(), "hunter2");
        let first = render(&paths).await.unwrap();
        assert_eq!(first, RenderOutcome::Written);
        let second = render(&paths).await.unwrap();
        assert_eq!(second, RenderOutcome::Unchanged);
    }

    #[tokio::test]
    async fn render_rewrites_on_password_rotation() {
        let tmp = TempDir::new().unwrap();
        let paths = paths_in(tmp.path(), "old-pass");
        render(&paths).await.unwrap();
        // Rotate.
        std::fs::write(&paths.secret_path, "new-pass").unwrap();
        let outcome = render(&paths).await.unwrap();
        assert_eq!(outcome, RenderOutcome::Written);
        let contents = std::fs::read_to_string(&paths.rendered_path).unwrap();
        // archipelago:new-pass -> "YXJjaGlwZWxhZ286bmV3LXBhc3M="
        assert!(contents.contains("YXJjaGlwZWxhZ286bmV3LXBhc3M="));
    }

    #[tokio::test]
    async fn render_trims_trailing_newline_from_secret() {
        // Matches `echo "$PASS" > file` behaviour.
        let tmp = TempDir::new().unwrap();
        let paths = paths_in(tmp.path(), "hunter2\n");
        render(&paths).await.unwrap();
        let contents = std::fs::read_to_string(&paths.rendered_path).unwrap();
        assert!(
            contents.contains("YXJjaGlwZWxhZ286aHVudGVyMg=="),
            "trailing newline should be stripped before encoding"
        );
    }

    #[tokio::test]
    async fn render_errors_on_empty_password() {
        let tmp = TempDir::new().unwrap();
        let paths = paths_in(tmp.path(), "");
        let err = render(&paths).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("empty"), "unexpected error: {msg}");
    }

    #[tokio::test]
    async fn render_errors_when_secret_missing() {
        let tmp = TempDir::new().unwrap();
        let paths = RenderPaths {
            secret_path: tmp.path().join("does-not-exist"),
            rendered_path: tmp.path().join("nginx.conf"),
        };
        let err = render(&paths).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("reading bitcoin RPC password"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn template_contains_exactly_one_placeholder() {
        // Safety net: if someone adds a second placeholder to the
        // template without updating the renderer, we want a test to
        // fail loudly rather than ship a half-substituted config.
        let count = TEMPLATE.matches(PLACEHOLDER).count();
        assert_eq!(count, 1, "template must contain exactly one {PLACEHOLDER}");
    }

    #[test]
    fn template_proxies_bitcoin_rpc_on_8332() {
        // Lock in the core shape so a bad template edit doesn't ship.
        assert!(TEMPLATE.contains("proxy_pass http://127.0.0.1:8332/"));
        assert!(TEMPLATE.contains("location /bitcoin-rpc/"));
        assert!(TEMPLATE.contains("listen 127.0.0.1:8334"));
    }
}
