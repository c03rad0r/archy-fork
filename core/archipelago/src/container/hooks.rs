//! Manifest-driven lifecycle hook executor (Task #20).
//!
//! Runs an app's declarative `post_install` hooks against its **own** running
//! container. Hooks are an allowlisted, reviewed escape hatch — NOT arbitrary
//! host scripts:
//!
//! - `exec` runs *inside the container* (`podman exec`), never on the host, and
//!   inherits the container's (already dropped) capabilities.
//! - `copy_from_host.src` is resolved against an allowlist root, canonicalised,
//!   and rejected on any escape; only then is it `podman cp`'d into the container.
//! - Execution is **best-effort + idempotent**: each step is logged, a failure is
//!   warned and the remaining steps still run, so a transient hook error never
//!   bricks an install. Authors must make steps safe to re-run (e.g. `grep -q … ||`).
//!
//! See `docs/manifest-hooks-design.md`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Result};
use archipelago_container::{AppManifest, HookStep};

/// Upper bound on a single hook command. Generous — config rewrites + nginx
/// reloads are fast, but an image with a hung entrypoint shouldn't wedge install.
const HOOK_TIMEOUT: Duration = Duration::from_secs(60);

/// Roots a `copy_from_host.src` may resolve within. A src is joined onto each
/// root, canonicalised, and accepted only if it stays inside that root:
/// - the app's own data dir (`<data_dir>/<app_id>`), and
/// - `/opt/archipelago` (covers the orchestrator's bundled `web-ui/` assets,
///   e.g. indeedhub's `web-ui/nostr-provider.js`).
fn allowlist_roots(app_id: &str, data_dir: &Path) -> Vec<PathBuf> {
    vec![data_dir.join(app_id), PathBuf::from("/opt/archipelago")]
}

/// Resolve a hook copy source against the allowlist. Returns the canonical
/// absolute path iff it exists and lies within an allowlist root. Defence in
/// depth: `AppManifest::validate` already rejects absolute / `..` srcs, but we
/// re-check here and canonicalise so a symlink inside a root can't escape it.
fn resolve_copy_src(src: &str, app_id: &str, data_dir: &Path) -> Result<PathBuf> {
    if src.is_empty() || src.starts_with('/') || src.contains("..") {
        bail!("hook copy src '{src}' is not an allowlisted relative path");
    }
    for root in allowlist_roots(app_id, data_dir) {
        let Ok(root_canon) = root.canonicalize() else {
            continue;
        };
        let Ok(canon) = root.join(src).canonicalize() else {
            continue;
        };
        if canon.starts_with(&root_canon) {
            return Ok(canon);
        }
    }
    bail!("hook copy src '{src}' did not resolve inside an allowlist root")
}

/// Run an app's declarative `post_install` hooks against its running container.
/// Best-effort: never returns an error — a failed step is warned and skipped.
/// Called from the install path after the container is created + running, and
/// only when a fresh container was created (see `install_fresh`).
pub async fn run_post_install(manifest: &AppManifest, container_name: &str, data_dir: &Path) {
    let steps = &manifest.app.hooks.post_install;
    if steps.is_empty() {
        return;
    }
    let app_id = &manifest.app.id;
    tracing::info!(
        app_id = %app_id,
        container = %container_name,
        steps = steps.len(),
        "running manifest post_install hooks"
    );
    for (i, step) in steps.iter().enumerate() {
        match run_step(step, container_name, app_id, data_dir).await {
            Ok(()) => tracing::debug!(app_id = %app_id, step = i, "post_install hook step ok"),
            Err(err) => tracing::warn!(
                app_id = %app_id,
                container = %container_name,
                step = i,
                error = %err,
                "post_install hook step failed (continuing best-effort)"
            ),
        }
    }
}

/// Strict completion for an explicitly staged update. Keep the durable pin on failure.
pub(super) async fn run_post_install_strict(
    manifest: &AppManifest,
    container: &str,
    data_dir: &Path,
) -> Result<()> {
    for step in &manifest.app.hooks.post_install {
        run_step(step, container, &manifest.app.id, data_dir).await?;
    }
    Ok(())
}

async fn run_step(step: &HookStep, container: &str, app_id: &str, data_dir: &Path) -> Result<()> {
    match step {
        HookStep::Exec { exec } => {
            let mut args: Vec<&str> = Vec::with_capacity(exec.len() + 2);
            args.push("exec");
            args.push(container);
            args.extend(exec.iter().map(String::as_str));
            // `exec` spawns a process INSIDE the container's cgroup. When the
            // container was started by archipelago.service, that cgroup is under
            // the service's slice and a bare `podman exec` from the service can't
            // write its `cgroup.procs` ("crun: ... Permission denied / OCI
            // permission denied"). Run it in a transient user scope (its own
            // delegated cgroup) — mirrors `podman_user_scope` for pasta starts.
            run_podman(&args, /* scoped */ true).await
        }
        HookStep::CopyFromHost { copy_from_host } => {
            let abs = resolve_copy_src(&copy_from_host.src, app_id, data_dir)?;
            let abs = abs.to_string_lossy().into_owned();
            let dest = format!("{container}:{}", copy_from_host.dest);
            // `cp` is a host-side copy (no in-container process), so no scope needed.
            run_podman(&["cp", &abs, &dest], /* scoped */ false).await
        }
    }
}

/// Run a podman command, optionally inside a transient systemd user scope. The
/// scope gives the invocation its own delegated cgroup so `podman exec` can
/// place its child process — without it, an exec launched from the service's
/// own cgroup is denied write to the container's `cgroup.procs`.
async fn run_podman(args: &[&str], scoped: bool) -> Result<()> {
    let rendered = args.join(" ");
    let mut cmd = if scoped {
        let mut c = tokio::process::Command::new("systemd-run");
        c.args(["--user", "--scope", "--quiet", "--collect", "podman"]);
        c.args(args);
        c
    } else {
        let mut c = tokio::process::Command::new("podman");
        c.args(args);
        c
    };
    let out = tokio::time::timeout(HOOK_TIMEOUT, cmd.output())
        .await
        .map_err(|_| anyhow::anyhow!("podman {rendered} timed out after {:?}", HOOK_TIMEOUT))?
        .map_err(|e| anyhow::anyhow!("podman {rendered}: {e}"))?;

    if !out.status.success() {
        bail!(
            "podman {rendered} exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_copy_src_accepts_file_in_app_data_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path();
        let app_dir = data_dir.join("myapp/web-ui");
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::write(app_dir.join("provider.js"), b"x").unwrap();

        let got = resolve_copy_src("web-ui/provider.js", "myapp", data_dir).unwrap();
        assert!(got.ends_with("myapp/web-ui/provider.js"));
        assert!(got.is_absolute());
    }

    #[test]
    fn resolve_copy_src_rejects_absolute() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(resolve_copy_src("/etc/passwd", "myapp", tmp.path()).is_err());
    }

    #[test]
    fn resolve_copy_src_rejects_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(resolve_copy_src("web-ui/../../etc/shadow", "myapp", tmp.path()).is_err());
    }

    #[test]
    fn resolve_copy_src_rejects_missing_file() {
        // Inside the allowlist shape but the file doesn't exist → canonicalize fails.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("myapp")).unwrap();
        assert!(resolve_copy_src("nope.js", "myapp", tmp.path()).is_err());
    }

    #[test]
    fn resolve_copy_src_rejects_symlink_escape() {
        // A symlink inside the app dir pointing outside it must be rejected by
        // the post-canonicalisation prefix check.
        let tmp = tempfile::tempdir().unwrap();
        let app_dir = tmp.path().join("myapp");
        std::fs::create_dir_all(&app_dir).unwrap();
        let secret = tmp.path().join("secret.txt");
        std::fs::write(&secret, b"s").unwrap();
        let link = app_dir.join("link.js");
        if std::os::unix::fs::symlink(&secret, &link).is_ok() {
            // `secret.txt` lives in the tmp root, NOT under <data_dir>/myapp, so
            // the canonical target escapes the app-data root. It also isn't under
            // /opt/archipelago. Must be rejected.
            assert!(resolve_copy_src("link.js", "myapp", tmp.path()).is_err());
        }
    }
}
