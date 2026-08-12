//! systemctl + dpkg-query helpers for the FIPS daemon.
//!
//! Read-only queries (`is-active`, `--version`, `dpkg-query`) run as the
//! archipelago service user. Write operations (`unmask`, `start`, `stop`,
//! `restart`) go through `sudo`, matching the pattern established in
//! `src/vpn.rs` and `src/api/rpc/vpn.rs`. The sudoers rule shipped in the
//! ISO whitelists exactly these invocations.

use anyhow::{Context, Result};
use nostr_sdk::ToBech32;
use tokio::process::Command;

use super::DAEMON_PUB_PATH;

/// `systemctl is-active <unit>` → "active" / "inactive" / "failed" / "masked"
/// / "unknown". Never errors; returns "unknown" on any failure.
pub async fn unit_state(unit: &str) -> String {
    match Command::new("systemctl")
        .args(["is-active", unit])
        .output()
        .await
    {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() {
                "unknown".to_string()
            } else {
                s
            }
        }
        Err(_) => "unknown".to_string(),
    }
}

/// Whether systemd knows about `unit`.
pub async fn unit_exists(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["cat", unit])
        .output()
        .await
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Whether the `fips` debian package is installed on the host.
pub async fn package_installed() -> bool {
    // dpkg-query -W -f='${Status}' fips → "install ok installed" when present.
    let out = Command::new("dpkg-query")
        .args(["-W", "-f=${Status}", "fips"])
        .output()
        .await;
    match out {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).contains("install ok installed")
        }
        _ => false,
    }
}

/// `fipsctl --version` output stripped of the "fipsctl " prefix if present.
pub async fn daemon_version() -> Result<String> {
    let out = Command::new("fipsctl")
        .arg("--version")
        .output()
        .await
        .context("fipsctl --version failed to launch")?;
    if !out.status.success() {
        anyhow::bail!("fipsctl exited with non-zero status");
    }
    let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(raw
        .strip_prefix("fipsctl ")
        .map(|s| s.to_string())
        .unwrap_or(raw))
}

/// `sudo systemctl <verb> <unit>` — returns stderr on non-zero exit.
async fn sudo_systemctl(verb: &str, unit: &str) -> Result<()> {
    let out = Command::new("sudo")
        .args(["systemctl", verb, unit])
        .output()
        .await
        .with_context(|| format!("sudo systemctl {} {} failed to launch", verb, unit))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        anyhow::bail!("systemctl {} {}: {}", verb, unit, stderr);
    }
    Ok(())
}

/// Unmask + start + enable the FIPS service. Idempotent — safe to call
/// on every backend startup once the key is on disk.
pub async fn activate(unit: &str) -> Result<()> {
    kill_stale_daemons().await?;
    // Order matters: unmask before enable/start, otherwise enable fails
    // on a masked unit.
    sudo_systemctl("unmask", unit).await?;
    sudo_systemctl("enable", unit).await?;
    sudo_systemctl("start", unit).await?;
    Ok(())
}

pub async fn stop(unit: &str) -> Result<()> {
    sudo_systemctl("stop", unit).await
}

pub async fn restart(unit: &str) -> Result<()> {
    kill_stale_daemons().await?;
    sudo_systemctl("restart", unit).await
}

/// Kill orphaned `fips` processes not owned by either known systemd unit.
///
/// Field failure, 2026-07-24: a stale daemon survived outside systemd and kept
/// `0.0.0.0:8443` bound. The supervised daemon then started UDP-only, so every
/// TCP seed-anchor connect failed with "no operational transport" and phones
/// on 5G could not discover the node. We keep the cleanup narrow: preserve the
/// MainPID of both units and terminate only extra `pgrep -x fips` matches.
pub async fn kill_stale_daemons() -> Result<()> {
    let script = format!(
        r#"keep="$(systemctl show -p MainPID --value {managed} 2>/dev/null; systemctl show -p MainPID --value {upstream} 2>/dev/null)"
for pid in $(pgrep -x fips 2>/dev/null || true); do
  case " $keep " in
    *" $pid "*) ;;
    *) kill "$pid" 2>/dev/null || true ;;
  esac
done
"#,
        managed = super::SERVICE_UNIT,
        upstream = super::UPSTREAM_SERVICE_UNIT,
    );
    let out = Command::new("sudo")
        .args(["sh", "-c", &script])
        .output()
        .await
        .context("sudo stale fips cleanup failed to launch")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        anyhow::bail!("stale fips cleanup failed: {}", stderr);
    }
    Ok(())
}

/// Resolve which systemd unit should be started when FIPS is inactive.
/// Newer Archipelago images may ship `archipelago-fips.service`; nodes with
/// the upstream Debian package may only have `fips.service`. Activation must
/// choose a unit systemd can actually load, otherwise the dashboard repeatedly
/// offers an "Activate" action that can never succeed.
pub async fn activation_unit() -> &'static str {
    if unit_exists(super::SERVICE_UNIT).await {
        return super::SERVICE_UNIT;
    }
    if unit_exists(super::UPSTREAM_SERVICE_UNIT).await {
        return super::UPSTREAM_SERVICE_UNIT;
    }
    super::SERVICE_UNIT
}

/// Resolve which systemd unit is actually supervising the fips daemon on this
/// host. Restart/Reconnect must operate on whichever one is running, otherwise
/// the UI button is a silent no-op.
///
/// Returns the archipelago-managed unit name if it's active,
/// else the upstream unit name if that's active,
/// else a startable activation unit.
pub async fn active_unit() -> &'static str {
    if unit_state(super::SERVICE_UNIT).await == "active" {
        return super::SERVICE_UNIT;
    }
    if unit_state(super::UPSTREAM_SERVICE_UNIT).await == "active" {
        return super::UPSTREAM_SERVICE_UNIT;
    }
    activation_unit().await
}

pub async fn mask(unit: &str) -> Result<()> {
    let _ = sudo_systemctl("stop", unit).await;
    let _ = sudo_systemctl("disable", unit).await;
    sudo_systemctl("mask", unit).await
}

/// Known public anchor npub (fips.v0l.io as of 2026-04). Used to decide
/// whether the `anchor_connected` badge in the dashboard lights up.
pub const PUBLIC_ANCHOR_NPUB: &str =
    "npub1zv58cn7v83mxvttl70w5fwjwuclfmntv9cnmv5wmz2nzz88u5urqvdx96n";

/// Summarise peer connectivity from `fipsctl show peers`. Returns
/// `(authenticated_peer_count, anchor_connected)`.
///
/// `anchor_candidates` is the operator-controlled list of npubs this
/// node considers a valid mesh anchor — always includes the hard-coded
/// public anchor, plus any entries from `seed-anchors.json`. A node is
/// "anchor connected" when at least one currently-authenticated peer
/// matches one of these npubs. We used to check the identity cache
/// (which includes transient hearsay from other peers), but a cache
/// hit on `fips.v0l.io` didn't mean we could actually route through
/// it, and the card lied to users whose mesh was federated through
/// their own seed anchors instead.
pub async fn peer_connectivity_summary(anchor_candidates: &[String]) -> (u32, bool) {
    let peers_json = match Command::new("sudo")
        .args(["-n", "fipsctl", "show", "peers"])
        .output()
        .await
    {
        Ok(o) if o.status.success() => o.stdout,
        _ => return (0, false),
    };
    let parsed: serde_json::Value = match serde_json::from_slice(&peers_json) {
        Ok(v) => v,
        Err(_) => return (0, false),
    };
    let peers = parsed
        .get("peers")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();
    let authenticated_peer_count = peers.len() as u32;
    let anchor_connected = peers.iter().any(|p| {
        let npub = p.get("npub").and_then(|n| n.as_str()).unwrap_or_default();
        let connected = p
            .get("connectivity")
            .and_then(|c| c.as_str())
            .map(|s| s == "connected")
            .unwrap_or(true);
        connected && anchor_candidates.iter().any(|a| a == npub)
    });
    (authenticated_peer_count, anchor_connected)
}

/// Currently-connected peers with their live endpoints, from
/// `fipsctl show peers` (`transport_addr`/`transport_type`). Feeds the
/// last-known-good endpoint store (A3.10); empty on any failure.
pub async fn connected_peer_endpoints() -> Vec<crate::fips::endpoints::ConnectedPeer> {
    let peers_json = match Command::new("sudo")
        .args(["-n", "fipsctl", "show", "peers"])
        .output()
        .await
    {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };
    let parsed: serde_json::Value = match serde_json::from_slice(&peers_json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    parsed
        .get("peers")
        .and_then(|p| p.as_array())
        .map(|peers| {
            peers
                .iter()
                .filter(|p| {
                    p.get("connectivity")
                        .and_then(|c| c.as_str())
                        .map(|s| s == "connected")
                        .unwrap_or(false)
                })
                .filter_map(|p| {
                    let npub = p.get("npub").and_then(|n| n.as_str())?;
                    let address = p.get("transport_addr").and_then(|a| a.as_str())?;
                    let transport = p
                        .get("transport_type")
                        .and_then(|t| t.as_str())
                        .unwrap_or("udp");
                    Some(crate::fips::endpoints::ConnectedPeer {
                        npub: npub.to_string(),
                        address: address.to_string(),
                        transport: transport.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Read the upstream daemon's public key at `/etc/fips/fips.pub` and return
/// it as a bech32 npub. Returns `Ok(None)` if the file doesn't exist — used
/// as a fallback on legacy/dev nodes where no seed-derived key exists.
///
/// Upstream writes the key as a bech32 string (`npub1…`); older builds may
/// have written 32 raw bytes, so we accept either form.
pub async fn read_upstream_npub() -> Result<Option<String>> {
    let bytes = match tokio::fs::read(DAEMON_PUB_PATH).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).context("read /etc/fips/fips.pub"),
    };
    if let Ok(s) = std::str::from_utf8(&bytes) {
        let trimmed = s.trim();
        if trimmed.starts_with("npub1") {
            if let Ok(pk) = nostr_sdk::PublicKey::parse(trimmed) {
                return Ok(pk.to_bech32().ok());
            }
        }
    }
    let pk = nostr_sdk::PublicKey::from_slice(&bytes)
        .context("parse /etc/fips/fips.pub as secp256k1 public key")?;
    Ok(pk.to_bech32().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_unit_state_returns_string_for_bogus_unit() {
        // Nonexistent unit: systemctl returns "inactive" or "unknown" — we
        // just care that the helper doesn't panic and returns *something*.
        let s = unit_state("archipelago-bogus-test.service").await;
        assert!(!s.is_empty());
    }

    #[tokio::test]
    async fn test_package_installed_is_bool() {
        // Must not panic regardless of host state.
        let _ = package_installed().await;
    }

    #[tokio::test]
    async fn test_unit_exists_is_bool() {
        // Must not panic regardless of host state.
        let _ = unit_exists("archipelago-bogus-test.service").await;
    }
}
