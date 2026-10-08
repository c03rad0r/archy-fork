//! Owns only the FIPS website drop-in, never container, wallet or management
//! rules. nft applies a complete transaction atomically; failed reload restores
//! the previous drop-in for the next boot. Existing non-owned files are refused.
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::path::Path;
use tokio::process::Command;

const DROPIN: &str = "/etc/fips/fips.d/86-websites.nft";
const BASELINE: &str = "/etc/fips/fips.nft";
const MARKER: &str = "# Owned by Archipelago website publishing.\n";

pub fn render(ports: &BTreeSet<u16>) -> Result<String> {
    if ports.iter().any(|p| !(32000..32032).contains(p)) {
        bail!("Invalid website port");
    }
    let mut output = MARKER.to_owned();
    for port in ports {
        output.push_str(&format!("iifname \"fips0\" tcp dport {port} accept\n"));
    }
    Ok(output)
}

async fn command(args: &[&str]) -> Result<()> {
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        Command::new("sudo").arg("-n").args(args).output(),
    )
    .await
    .context("Website firewall operation timed out")??;
    if !out.status.success() {
        bail!(
            "Website firewall operation failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

async fn install(root: &Path, contents: &str) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let dir = root.join("publishing");
    tokio::fs::create_dir_all(&dir).await?;
    let stage = dir.join(format!("firewall-{}.tmp", uuid::Uuid::new_v4()));
    let mut opts = tokio::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    opts.mode(0o600);
    let mut f = opts.open(&stage).await?;
    f.write_all(contents.as_bytes()).await?;
    f.sync_all().await?;
    let result = command(&[
        "install",
        "-m",
        "0644",
        stage.to_str().context("Invalid data directory")?,
        DROPIN,
    ])
    .await;
    let _ = tokio::fs::remove_file(stage).await;
    result
}

pub async fn reconcile(root: &Path, ports: &BTreeSet<u16>) -> Result<()> {
    let next = render(ports)?;
    let previous = match tokio::fs::read_to_string(DROPIN).await {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    if previous.is_none() && ports.is_empty() {
        return Ok(());
    }
    if previous.as_ref().is_some_and(|s| !s.starts_with(MARKER)) {
        bail!("Website firewall slot is already owned by another configuration; no changes made");
    }
    let baseline = tokio::fs::read_to_string(BASELINE)
        .await
        .context("FIPS firewall baseline is missing; publication remains unavailable")?;
    if !baseline.contains("/etc/fips/fips.d/*.nft") || !baseline.contains("table inet fips") {
        bail!("FIPS firewall layout is unsupported; existing rules were preserved");
    }
    if previous.as_deref() == Some(&next) {
        return Ok(());
    }
    install(root, &next).await?;
    let applied = async {
        command(&["nft", "--check", "--file", BASELINE]).await?;
        command(&["nft", "--file", BASELINE]).await
    }
    .await;
    if let Err(error) = applied {
        let rollback = match previous {
            Some(old) => install(root, &old).await,
            None => command(&["rm", "-f", DROPIN]).await,
        };
        if let Err(rollback) = rollback {
            bail!("{error}; restoring website firewall file also failed: {rollback}");
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn firewall_scope_is_only_selected_website_ports_on_fips() {
        let rules = render(&[32000, 32002].into_iter().collect()).unwrap();
        assert_eq!(rules, format!("{MARKER}iifname \"fips0\" tcp dport 32000 accept\niifname \"fips0\" tcp dport 32002 accept\n"));
        assert_eq!(render(&BTreeSet::new()).unwrap(), MARKER);
        for port in [22, 80, 443, 8332, 31999, 32032] {
            assert!(render(&[port].into_iter().collect()).is_err());
        }
    }
}
