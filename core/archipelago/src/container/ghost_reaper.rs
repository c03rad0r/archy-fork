//! Ghost-container reaper.
//!
//! A *ghost* is a container whose process tree (conmon → the app's init → the
//! app) is still running while podman has no record of it — `podman ps -a`
//! does not list it, so every podman-level stop/rm/recreate misses it. They
//! are produced by a cleanup race: the exit-command runs `container cleanup
//! --rm`, the record is deleted, but conmon and the payload survive.
//!
//! A ghost is not merely untidy — it still owns the things the app needs:
//!
//! * the published host port, so the replacement container fails to start with
//!   `rootlessport listen tcp 127.0.0.1:<port>: bind: address already in use`;
//! * file locks inside the app's data dir, so a container that does start dies
//!   at boot (Gitea: `unable to lock level db … resource temporarily
//!   unavailable` → fatal).
//!
//! `Restart=always` then re-runs the app straight back into the same wall —
//! observed at 752 restarts on a fleet node (2026-08-10) and again on the dev
//! box (2026-08-16), where the app finally disappeared from My Apps because no
//! container existed to list. Both were cleared by hand; this module is the
//! automation, because no podman-level release logic can reap a container
//! podman does not know about.
//!
//! Safety rule, and the reason this is id-based rather than name-based: a
//! process is only ever a reap candidate when its container id is **absent**
//! from `podman ps -a --no-trunc -q`. Killing by container *name* would hit
//! the live managed container, which is the opposite of the fix.

use std::collections::HashSet;
use std::time::Duration;

use tracing::{info, warn};

/// A container process tree podman has no record of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ghost {
    /// conmon's pid — killed last, so it cannot re-parent the payload.
    pub conmon_pid: i32,
    /// Full 64-hex container id from conmon's `-c` argument.
    pub container_id: String,
    /// Container name from conmon's `-n` argument, when present. This is what
    /// ties a ghost to an app id for the pre-start reap.
    pub name: Option<String>,
}

/// Read a process's argv from /proc, NUL-separated.
fn proc_argv(pid: i32) -> Option<Vec<String>> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    Some(
        raw.split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect(),
    )
}

/// Every pid currently in /proc.
fn all_pids() -> Vec<i32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|s| s.parse::<i32>().ok()))
        .collect()
}

/// Container ids podman currently knows about (running or stopped).
async fn podman_known_ids() -> Option<HashSet<String>> {
    let out = tokio::process::Command::new("podman")
        .args(["ps", "-a", "--no-trunc", "-q"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        // A failed listing must NEVER be read as "podman knows nothing" —
        // that would make every running container look like a ghost and reap
        // the whole node. Absent knowledge = do nothing.
        warn!("ghost reaper: `podman ps` failed; skipping this pass");
        return None;
    }
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
    )
}

/// Parse a conmon argv into (container_id, name), if it is a conmon at all.
fn parse_conmon(argv: &[String]) -> Option<(String, Option<String>)> {
    let exe = argv.first()?;
    if !exe.ends_with("conmon") {
        return None;
    }
    let mut id = None;
    let mut name = None;
    let mut it = argv.iter().peekable();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-c" => {
                if let Some(v) = it.peek() {
                    // Only a full 64-hex id counts; anything else is not a
                    // container id and must not drive a kill decision.
                    if v.len() == 64 && v.chars().all(|c| c.is_ascii_hexdigit()) {
                        id = Some((*v).clone());
                    }
                }
            }
            "-n" => name = it.peek().map(|v| (*v).clone()),
            _ => {}
        }
    }
    Some((id?, name))
}

/// All ghost process trees on this host. Empty when podman cannot be listed
/// (fail-closed: unknown state reaps nothing).
pub async fn find_ghosts() -> Vec<Ghost> {
    let Some(known) = podman_known_ids().await else {
        return Vec::new();
    };
    let mut ghosts = Vec::new();
    for pid in all_pids() {
        let Some(argv) = proc_argv(pid) else { continue };
        let Some((container_id, name)) = parse_conmon(&argv) else {
            continue;
        };
        if known.contains(&container_id) {
            continue;
        }
        ghosts.push(Ghost {
            conmon_pid: pid,
            container_id,
            name,
        });
    }
    ghosts
}

fn signal(pid: i32, sig: i32) {
    // SAFETY: kill(2) with a pid we read from /proc; a dead pid returns ESRCH,
    // which we ignore. Signals are the only way to reach a process podman has
    // disowned.
    unsafe {
        libc::kill(pid, sig);
    }
}

fn alive(pid: i32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// Kill one ghost's process tree: the payload's process group first (so the
/// app's own init can shut its children down), then conmon.
///
/// SIGTERM first with a short grace, then SIGKILL — a ghost has already
/// out-lived its supervisor, and the Gitea case ignored SIGTERM outright.
async fn kill_ghost(ghost: &Ghost) {
    // Children of conmon = the container's init (s6, tini, the app itself).
    let children: Vec<i32> = all_pids()
        .into_iter()
        .filter(|pid| {
            std::fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .and_then(|s| {
                    // ppid is field 4, after the comm field which may itself
                    // contain spaces/parens — split on the last ')'.
                    let tail = s.rsplit_once(')')?.1;
                    tail.split_whitespace().nth(1)?.parse::<i32>().ok()
                })
                .is_some_and(|ppid| ppid == ghost.conmon_pid)
        })
        .collect();

    for pid in children.iter().copied() {
        signal(pid, libc::SIGTERM);
    }
    signal(ghost.conmon_pid, libc::SIGTERM);
    tokio::time::sleep(Duration::from_secs(5)).await;

    for pid in children.iter().copied() {
        if alive(pid) {
            signal(pid, libc::SIGKILL);
        }
    }
    if alive(ghost.conmon_pid) {
        signal(ghost.conmon_pid, libc::SIGKILL);
    }
    tokio::time::sleep(Duration::from_millis(500)).await;

    let survivors = children.iter().filter(|p| alive(**p)).count();
    if survivors > 0 || alive(ghost.conmon_pid) {
        warn!(
            container_id = %&ghost.container_id[..12],
            name = ?ghost.name,
            survivors,
            "ghost reaper: some processes survived SIGKILL"
        );
    }
}

/// Reap every ghost on the host. Returns how many trees were killed.
///
/// Call before a start/restart (so the replacement is not racing a dead
/// twin for its port and locks) and from the periodic reconcile.
pub async fn reap_all() -> usize {
    reap_matching(|_| true).await
}

/// Reap only ghosts belonging to `app_id` — matched on the container name,
/// which the orchestrator sets to the app id (companions carry it as a
/// prefix, e.g. `archy-btcpay-db`).
pub async fn reap_for_app(app_id: &str) -> usize {
    let app_id = app_id.to_string();
    reap_matching(move |g| {
        g.name.as_deref().is_some_and(|n| {
            n == app_id
                || n.starts_with(&format!("{app_id}-"))
                || n.ends_with(&format!("-{app_id}"))
        })
    })
    .await
}

async fn reap_matching(pred: impl Fn(&Ghost) -> bool) -> usize {
    let ghosts: Vec<Ghost> = find_ghosts()
        .await
        .into_iter()
        .filter(|g| pred(g))
        .collect();
    if ghosts.is_empty() {
        return 0;
    }
    for ghost in &ghosts {
        warn!(
            container_id = %&ghost.container_id[..12],
            name = ?ghost.name,
            conmon_pid = ghost.conmon_pid,
            "ghost container found — podman has no record of it but its processes still \
             hold the app's ports and data locks; reaping"
        );
        kill_ghost(ghost).await;
    }
    info!(
        count = ghosts.len(),
        "ghost reaper: reaped ghost containers"
    );
    ghosts.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    const ID: &str = "8ea2fc65603a6db8d48701e26da1a18f1651e5f8b0e2dd4ec931356f4fab0081";

    #[test]
    fn parses_a_real_conmon_invocation() {
        let a = argv(&[
            "/usr/bin/conmon",
            "--api-version",
            "1",
            "-c",
            ID,
            "-u",
            ID,
            "-n",
            "gitea",
            "--full-attach",
        ]);
        let (id, name) = parse_conmon(&a).expect("conmon parsed");
        assert_eq!(id, ID);
        assert_eq!(name.as_deref(), Some("gitea"));
    }

    #[test]
    fn ignores_processes_that_are_not_conmon() {
        assert!(parse_conmon(&argv(&["/usr/local/bin/gitea", "web"])).is_none());
        // A shell whose *arguments* mention conmon must never be parsed as one
        // — the grep-based detection used by hand did exactly this.
        assert!(parse_conmon(&argv(&["/bin/bash", "-c", "pgrep -af conmon"])).is_none());
    }

    #[test]
    fn a_truncated_or_missing_id_is_not_reapable() {
        assert!(
            parse_conmon(&argv(&["/usr/bin/conmon", "-c", "8ea2fc65", "-n", "gitea"])).is_none()
        );
        assert!(parse_conmon(&argv(&["/usr/bin/conmon", "--api-version", "1"])).is_none());
    }

    #[test]
    fn app_matching_covers_companions_but_not_unrelated_apps() {
        let g = |n: &str| Ghost {
            conmon_pid: 1,
            container_id: ID.to_string(),
            name: Some(n.to_string()),
        };
        let matches = |app: &str, name: &str| {
            let app = app.to_string();
            let gh = g(name);
            gh.name.as_deref().is_some_and(|n| {
                n == app || n.starts_with(&format!("{app}-")) || n.ends_with(&format!("-{app}"))
            })
        };
        assert!(matches("gitea", "gitea"));
        assert!(matches("btcpay-server", "btcpay-server"));
        assert!(matches("immich", "immich-postgres"));
        assert!(matches("btcpay", "archy-btcpay"));
        // Substring coincidences must not match.
        assert!(!matches("pay", "btcpay-server"));
        assert!(!matches("gitea", "gitea2"));
    }
}
