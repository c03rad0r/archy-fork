//! SSH over the FIPS mesh — a first-class settings toggle.
//!
//! `fips0` is default-deny inbound: the hardening baseline (`/etc/fips/
//! fips.nft`) rejects un-allowlisted ports, and the daemon's own drop-ins
//! (`80-web-ui.nft`, `85-app-ports.nft`) do not include 22. That is correct
//! by default — but the user asked to be able to SSH their node from Termux
//! over the phone's FIPS mesh instead of keeping a second VPN around for it,
//! and the mesh path already works end-to-end (verified live: the connect
//! reaches fips0 and gets a RST from the node).
//!
//! This module owns the whole lifecycle of the `90-ssh.nft` drop-in, exactly
//! the way `config.rs` owns `80-web-ui.nft` — a hand-added rule and this
//! feature can never fight over the same slot:
//!
//!   * toggle OFF  → drop-in removed, port 22 refused again
//!   * toggle ON   → drop-in written on every toggle change AND on every
//!                   daemon config install (upgrade, reconnect, self-heal),
//!                   so the on-state survives reinstalls idempotently
//!   * scope       → "any" (every mesh peer — a real exposure, gated in the
//!                   UI behind an explicit confirmation) or an explicit list
//!                   of mesh addresses
//!
//! Nothing else is touched: `80-web-ui.nft` / `85-app-ports.nft` belong to
//! `config.rs`, and the sshd process itself is entirely the operator's.

use std::net::Ipv6Addr;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

/// On-disk state under the archipelago data dir. Absent file = disabled,
/// which is the safe default for every node that never touched the toggle.
const STATE_FILE: &str = "fips-ssh-over-mesh.json";

/// The drop-in slot this module owns. 90 sorts after the daemon's own
/// drop-ins (80/85) so a human reading the directory sees the deliberate
/// order; the include order does not change semantics for plain accepts.
pub const DROPIN_PATH: &str = "/etc/fips/fips.d/90-ssh.nft";

/// The hardening baseline this drop-in hangs off. Same file `config.rs`
/// reloads after its own drop-ins.
const FIPS_NFT: &str = "/etc/fips/fips.nft";

/// Persisted toggle state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SshMeshState {
    /// Whether port 22 is allowed through the fips0 baseline at all.
    #[serde(default)]
    pub enabled: bool,
    /// Mesh addresses (ULAs) the rule is restricted to. Empty = any mesh
    /// peer. Kept as strings as-entered but validated as IPv6 on save.
    #[serde(default)]
    pub sources: Vec<String>,
}

fn state_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(STATE_FILE)
}

/// Load the persisted state. Missing file = disabled, no sources — never an
/// error, so a fresh node and a deleted file both mean "off".
pub async fn load(data_dir: &Path) -> SshMeshState {
    match tokio::fs::read_to_string(state_path(data_dir)).await {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => SshMeshState::default(),
    }
}

/// Validate and normalise an operator-supplied source list. Every entry must
/// be a parseable IPv6 address (mesh addresses are full ULAs, not CIDRs) —
/// anything else is refused with the offending entry named, so a typo can
/// never silently narrow or widen the rule.
pub fn validate_sources(raw: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::with_capacity(raw.len());
    for entry in raw {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let addr: Ipv6Addr = trimmed
            .parse()
            .with_context(|| format!("not a valid mesh (IPv6) address: {trimmed:?}"))?;
        out.push(addr.to_string());
    }
    out.dedup();
    Ok(out)
}

/// Render the nft drop-in for a state. The rule shape mirrors the interim
/// manual unblock from the field notes (`ip6 saddr <ula> tcp dport 22
/// accept`) — an unrestricted rule is the same statement without the saddr.
pub fn render_dropin(state: &SshMeshState) -> String {
    let mut out = String::from(
        "# Written by archipelago — SSH over mesh (Settings → SSH over mesh).\n\
         # Allows sshd (port 22) through the fips0 default-deny inbound\n\
         # baseline. Remove = refused again; never edit 80/85-* by hand.\n",
    );
    if state.sources.is_empty() {
        out.push_str("tcp dport 22 accept\n");
    } else {
        out.push_str(&format!(
            "ip6 saddr {{ {} }} tcp dport 22 accept\n",
            state.sources.join(", ")
        ));
    }
    out
}

/// Write or remove the drop-in to match the persisted state, then reload the
/// baseline so the change is live immediately. Returns whether a reload was
/// attempted and succeeded — a node without the hardening baseline has
/// nothing to reload (port 22 is governed by sshd and the host firewall
/// there), which is reported rather than treated as failure.
pub async fn reconcile(data_dir: &Path) -> Result<ReconcileOutcome> {
    let state = load(data_dir).await;

    if !state.enabled {
        let removed = remove_dropin().await?;
        let reloaded = reload_nft().await;
        return Ok(ReconcileOutcome {
            applied: false,
            removed,
            reloaded,
        });
    }

    // Ensure /etc/fips/fips.d exists, exactly like config::install.
    let out = Command::new("sudo")
        .args(["install", "-d", "-m", "0755", "/etc/fips/fips.d"])
        .output()
        .await
        .context("sudo install -d /etc/fips/fips.d")?;
    if !out.status.success() {
        anyhow::bail!(
            "sudo install -d /etc/fips/fips.d failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    let dropin = render_dropin(&state);
    let stage = std::env::temp_dir().join(format!("fips-ssh-{}.nft", std::process::id()));
    tokio::fs::write(&stage, &dropin)
        .await
        .context("stage ssh nft drop-in")?;
    let install = Command::new("sudo")
        .args(["install", "-m", "0644"])
        .arg(&stage)
        .arg(DROPIN_PATH)
        .output()
        .await;
    let _ = tokio::fs::remove_file(&stage).await;
    let install = install?;
    if !install.status.success() {
        anyhow::bail!(
            "install {} failed: {}",
            DROPIN_PATH,
            String::from_utf8_lossy(&install.stderr).trim()
        );
    }

    let reloaded = reload_nft().await;
    Ok(ReconcileOutcome {
        applied: true,
        removed: false,
        reloaded,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconcileOutcome {
    /// The allow rule is in place.
    pub applied: bool,
    /// A previously-written drop-in was removed this call.
    pub removed: bool,
    /// The hardening baseline existed and `nft -f` succeeded.
    pub reloaded: bool,
}

async fn remove_dropin() -> Result<bool> {
    match tokio::fs::try_exists(DROPIN_PATH).await {
        Ok(true) => {}
        _ => return Ok(false),
    }
    let out = Command::new("sudo")
        .args(["rm", "-f", DROPIN_PATH])
        .output()
        .await
        .context("sudo rm 90-ssh.nft")?;
    if !out.status.success() {
        anyhow::bail!(
            "removing {} failed: {}",
            DROPIN_PATH,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    tracing::info!("ssh-over-mesh: drop-in removed — port 22 refused over fips0 again");
    Ok(true)
}

/// Reload the hardening baseline. Best-effort in the same spirit as
/// `config.rs`: absent baseline (nothing to reload) → Ok(false); a failed
/// reload is Ok(false) with a warn, never an error — the drop-in is on disk
/// either way and the next daemon install reloads it.
async fn reload_nft() -> bool {
    match tokio::fs::try_exists(FIPS_NFT).await {
        Ok(true) => {}
        _ => return false,
    }
    match Command::new("sudo")
        .args(["nft", "-f", FIPS_NFT])
        .output()
        .await
    {
        Ok(out) if out.status.success() => true,
        Ok(out) => {
            tracing::warn!(
                "ssh-over-mesh: nft reload failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            false
        }
        Err(e) => {
            tracing::warn!("ssh-over-mesh: nft reload failed: {e}");
            false
        }
    }
}

/// Persist new state and reconcile immediately. Validation happens here so
/// an invalid source list can never reach disk, and reconcile reads back
/// exactly what was saved.
pub async fn set(
    data_dir: &Path,
    enabled: bool,
    sources: &[String],
) -> Result<(SshMeshState, ReconcileOutcome)> {
    let state = SshMeshState {
        enabled,
        sources: validate_sources(sources)?,
    };
    tokio::fs::create_dir_all(data_dir)
        .await
        .with_context(|| format!("mkdir -p {}", data_dir.display()))?;
    tokio::fs::write(state_path(data_dir), serde_json::to_string_pretty(&state)?)
        .await
        .with_context(|| format!("write {}", state_path(data_dir).display()))?;
    let outcome = reconcile(data_dir).await?;
    Ok((state, outcome))
}

/// Preflights surfaced in the settings card. None of these gate the toggle —
/// they explain it: writing the rule on a node whose sshd doesn't listen on
/// IPv6 simply has no effect until sshd does, and the card says so instead of
/// the user discovering it as a silent connection failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SshPreflights {
    /// ssh.service (or sshd.service) is active.
    pub sshd_active: bool,
    /// Something listens on :22 for IPv6 (`[::]:22` or a dual-stack `*:22`).
    /// fips0 is IPv6-only, so a 0.0.0.0-bound sshd is unreachable over it.
    pub sshd_ipv6_listen: bool,
    /// sshd_config's PasswordAuthentication (last directive wins, includes
    /// after the main file). None = not found / unreadable.
    pub password_auth: Option<bool>,
}

pub async fn preflights() -> SshPreflights {
    SshPreflights {
        sshd_active: sshd_active().await,
        sshd_ipv6_listen: sshd_ipv6_listen().await,
        password_auth: password_auth_enabled().await,
    }
}

async fn sshd_active() -> bool {
    for unit in ["ssh", "sshd"] {
        if let Ok(out) = Command::new("systemctl")
            .args(["is-active", "--quiet", unit])
            .output()
            .await
        {
            if out.status.success() {
                return true;
            }
        }
    }
    false
}

async fn sshd_ipv6_listen() -> bool {
    let Ok(out) = Command::new("ss").args(["-H", "-tln"]).output().await else {
        return false;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines().any(|line| {
        let mut cols = line.split_whitespace();
        // -t -l: State Recv-Q Send-Q Local:Port Peer:Port → local is col 4.
        let _state = cols.next();
        let _recv = cols.next();
        let _send = cols.next();
        match cols.next() {
            Some(local) => {
                let port_ok = local.rsplit(':').next() == Some("22");
                let v6 = local.starts_with("[::]") || local.starts_with('*');
                port_ok && v6
            }
            None => false,
        }
    })
}

async fn password_auth_enabled() -> Option<bool> {
    let mut directives: Vec<bool> = Vec::new();
    if let Ok(main) = tokio::fs::read_to_string("/etc/ssh/sshd_config").await {
        collect_password_auth(&main, &mut directives);
    }
    if let Ok(includes) = glob_sorted("/etc/ssh/sshd_config.d/*.conf").await {
        for path in includes {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                collect_password_auth(&content, &mut directives);
            }
        }
    }
    directives.pop()
}

fn collect_password_auth(content: &str, out: &mut Vec<bool>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("PasswordAuthentication") {
            let rest = rest.trim_start();
            let value = rest.split_whitespace().next().unwrap_or("");
            if value.eq_ignore_ascii_case("yes") {
                out.push(true);
            } else if value.eq_ignore_ascii_case("no") {
                out.push(false);
            }
        }
    }
}

async fn glob_sorted(pattern: &str) -> Result<Vec<std::path::PathBuf>> {
    let dir = std::path::Path::new(pattern)
        .parent()
        .unwrap_or_else(|| Path::new("/"));
    let prefix = std::path::Path::new(pattern)
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.split('.').next())
        .unwrap_or("")
        .to_string();
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let mut entries = tokio::fs::read_dir(dir)
        .await
        .context("read sshd_config.d")?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(&prefix) && name.ends_with(".conf") {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_is_the_default_and_missing_file_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let state = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(load(dir.path()));
        assert!(!state.enabled);
        assert!(state.sources.is_empty());
    }

    #[test]
    fn any_peer_dropin_is_an_unrestricted_accept() {
        let state = SshMeshState {
            enabled: true,
            sources: vec![],
        };
        let out = render_dropin(&state);
        assert!(out.contains("tcp dport 22 accept"));
        assert!(!out.contains("ip6 saddr"), "no saddr restriction expected");
    }

    #[test]
    fn source_list_dropin_restricts_to_those_addresses() {
        let state = SshMeshState {
            enabled: true,
            sources: vec![
                "fd68:496d:fe34:a06d:cf1:6e4:b6a4:3586".to_string(),
                "fd79:1aa:b9e9:4c9f:1f80:5376:9385:1824".to_string(),
            ],
        };
        let out = render_dropin(&state);
        assert!(out.contains("ip6 saddr { fd68:496d:fe34:a06d:cf1:6e4:b6a4:3586, fd79:1aa:b9e9:4c9f:1f80:5376:9385:1824 } tcp dport 22 accept"));
    }

    #[test]
    fn sources_must_be_ipv6_and_are_normalised() {
        let bad = validate_sources(&["192.168.1.5".to_string()]).unwrap_err();
        assert!(bad.to_string().contains("192.168.1.5"));

        let bad = validate_sources(&["not-an-address".to_string()]).unwrap_err();
        assert!(bad.to_string().contains("not-an-address"));

        // Uppercase/whitespace entries normalise to canonical lowercase.
        let ok = validate_sources(&[
            " FD68:496D:FE34:A06D:0CF1:06E4:B6A4:3586 ".to_string(),
            "fd68:496d:fe34:a06d:cf1:6e4:b6a4:3586".to_string(),
            String::new(),
        ])
        .unwrap();
        assert_eq!(
            ok,
            vec!["fd68:496d:fe34:a06d:cf1:6e4:b6a4:3586".to_string()]
        );
    }

    #[test]
    fn state_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let state = SshMeshState {
            enabled: true,
            sources: vec!["fd00::1".to_string()],
        };
        std::fs::write(
            dir.path().join(STATE_FILE),
            serde_json::to_string(&state).unwrap(),
        )
        .unwrap();
        let loaded = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(load(dir.path()));
        assert_eq!(loaded, state);
    }

    #[test]
    fn set_validates_before_persisting() {
        let dir = tempfile::tempdir().unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt
            .block_on(set(dir.path(), true, &["bogus".to_string()]))
            .unwrap_err();
        assert!(err.to_string().contains("bogus"));
        // Nothing was persisted.
        let state = rt.block_on(load(dir.path()));
        assert!(!state.enabled);
    }

    #[test]
    fn preflight_parse_helpers_cover_the_directives() {
        let mut directives = Vec::new();
        collect_password_auth(
            "# comment\nPasswordAuthentication yes\nMatch all\n  PasswordAuthentication no\n",
            &mut directives,
        );
        assert_eq!(directives, vec![true, false]);
    }

    #[test]
    fn sshd_ipv6_listen_recognises_dual_stack_and_v6_only() {
        assert!(line_listens("[::]:22"));
        assert!(line_listens("*:22"));
        assert!(!line_listens("0.0.0.0:22"));
        assert!(!line_listens("[::]:80"));
    }

    fn line_listens(local: &str) -> bool {
        let line = format!("LISTEN 0 128 {local} 0.0.0.0:*");
        let mut cols = line.split_whitespace();
        cols.next();
        cols.next();
        cols.next();
        match cols.next() {
            Some(l) => {
                let port_ok = l.rsplit(':').next() == Some("22");
                let v6 = l.starts_with("[::]") || l.starts_with('*');
                port_ok && v6
            }
            None => false,
        }
    }
}
