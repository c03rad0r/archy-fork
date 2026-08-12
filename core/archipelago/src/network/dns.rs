use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::{debug, info};

const DNS_CONFIG_FILE: &str = "dns_config.json";

/// DNS provider presets with server addresses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DnsProvider {
    /// Use system default (DHCP-assigned DNS)
    System,
    /// Cloudflare DNS-over-HTTPS (1.1.1.1)
    Cloudflare,
    /// Google DNS-over-HTTPS (8.8.8.8)
    Google,
    /// Quad9 DNS-over-HTTPS (9.9.9.9)
    Quad9,
    /// Mullvad DNS (no logging)
    Mullvad,
    /// Custom user-specified servers
    Custom,
}

impl std::fmt::Display for DnsProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::System => write!(f, "system"),
            Self::Cloudflare => write!(f, "cloudflare"),
            Self::Google => write!(f, "google"),
            Self::Quad9 => write!(f, "quad9"),
            Self::Mullvad => write!(f, "mullvad"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

/// Persisted DNS configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    pub provider: DnsProvider,
    pub servers: Vec<String>,
    pub doh_enabled: bool,
    pub doh_url: Option<String>,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            provider: DnsProvider::System,
            servers: Vec::new(),
            doh_enabled: false,
            doh_url: None,
        }
    }
}

/// Current DNS status read from the system.
#[derive(Debug, Serialize)]
pub struct DnsStatus {
    pub provider: String,
    pub servers: Vec<String>,
    pub doh_enabled: bool,
    pub doh_url: Option<String>,
    pub resolv_conf_servers: Vec<String>,
}

/// Load persisted DNS config from disk.
pub async fn load_config(data_dir: &Path) -> Result<DnsConfig> {
    let path = data_dir.join(DNS_CONFIG_FILE);
    if !path.exists() {
        return Ok(DnsConfig::default());
    }
    let data = fs::read_to_string(&path)
        .await
        .context("Reading DNS config")?;
    serde_json::from_str(&data).context("Parsing DNS config")
}

/// Save DNS config to disk.
pub async fn save_config(data_dir: &Path, config: &DnsConfig) -> Result<()> {
    let path = data_dir.join(DNS_CONFIG_FILE);
    let data = serde_json::to_string_pretty(config)?;
    fs::write(&path, data).await.context("Writing DNS config")?;
    Ok(())
}

/// Get the DNS servers for a given provider preset.
pub fn provider_servers(provider: &DnsProvider) -> (Vec<String>, Option<String>) {
    match provider {
        DnsProvider::System => (Vec::new(), None),
        DnsProvider::Cloudflare => (
            vec!["1.1.1.1".into(), "1.0.0.1".into()],
            Some("https://cloudflare-dns.com/dns-query".into()),
        ),
        DnsProvider::Google => (
            vec!["8.8.8.8".into(), "8.8.4.4".into()],
            Some("https://dns.google/dns-query".into()),
        ),
        DnsProvider::Quad9 => (
            vec!["9.9.9.9".into(), "149.112.112.112".into()],
            Some("https://dns.quad9.net/dns-query".into()),
        ),
        DnsProvider::Mullvad => (
            vec!["194.242.2.2".into()],
            Some("https://dns.mullvad.net/dns-query".into()),
        ),
        DnsProvider::Custom => (Vec::new(), None),
    }
}

/// Read current DNS servers from /etc/resolv.conf.
pub async fn read_resolv_conf() -> Result<Vec<String>> {
    let content = fs::read_to_string("/etc/resolv.conf")
        .await
        .unwrap_or_default();
    let servers: Vec<String> = content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("nameserver") {
                trimmed.split_whitespace().nth(1).map(String::from)
            } else {
                None
            }
        })
        .collect();
    Ok(servers)
}

/// Get current DNS status combining config + system state.
pub async fn get_status(data_dir: &Path) -> Result<DnsStatus> {
    let config = load_config(data_dir).await?;
    let resolv_servers = read_resolv_conf().await.unwrap_or_default();

    Ok(DnsStatus {
        provider: config.provider.to_string(),
        servers: if config.servers.is_empty() {
            resolv_servers.clone()
        } else {
            config.servers.clone()
        },
        doh_enabled: config.doh_enabled,
        doh_url: config.doh_url.clone(),
        resolv_conf_servers: resolv_servers,
    })
}

/// Apply DNS configuration to the system via nmcli.
///
/// Sets DNS servers on the active NetworkManager connection(s).
pub async fn apply_dns(config: &DnsConfig) -> Result<()> {
    if config.provider == DnsProvider::System {
        // Revert to DHCP-assigned DNS
        info!("Reverting to system (DHCP) DNS");
        apply_dns_via_nmcli(&[]).await?;
        return Ok(());
    }

    let servers = &config.servers;
    if servers.is_empty() {
        anyhow::bail!("No DNS servers specified");
    }

    // Validate all server IPs
    for s in servers {
        if s.parse::<std::net::IpAddr>().is_err() {
            anyhow::bail!("Invalid DNS server IP: {}", s);
        }
    }

    info!(provider = %config.provider, servers = ?servers, "Applying DNS configuration");
    apply_dns_via_nmcli(servers).await?;

    Ok(())
}

/// Apply DNS servers to all active NetworkManager connections.
async fn apply_dns_via_nmcli(servers: &[String]) -> Result<()> {
    // Get active connections
    let output = tokio::process::Command::new("nmcli")
        .args([
            "-t",
            "-f",
            "NAME,DEVICE,TYPE",
            "connection",
            "show",
            "--active",
        ])
        .output()
        .await
        .context("Failed to list nmcli connections")?;

    if !output.status.success() {
        anyhow::bail!(
            "nmcli connection show failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8(output.stdout).context("nmcli output not utf8")?;
    let connections: Vec<&str> = stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(3, ':').collect();
            if parts.len() >= 3 {
                let conn_type = parts[2];
                // Only modify ethernet and wifi connections
                if conn_type.contains("ethernet")
                    || conn_type.contains("wireless")
                    || conn_type.contains("wifi")
                {
                    return Some(parts[0]);
                }
            }
            None
        })
        .collect();

    if connections.is_empty() {
        debug!("No active ethernet/wifi connections found, skipping DNS apply");
        return Ok(());
    }

    let dns_value = if servers.is_empty() {
        String::new() // Empty clears custom DNS, reverts to DHCP
    } else {
        servers.join(" ")
    };

    for conn_name in &connections {
        // Set DNS servers
        let dns_args = if dns_value.is_empty() {
            vec![
                "connection".to_string(),
                "modify".to_string(),
                conn_name.to_string(),
                "ipv4.dns".to_string(),
                String::new(),
                "ipv4.ignore-auto-dns".to_string(),
                "no".to_string(),
            ]
        } else {
            vec![
                "connection".to_string(),
                "modify".to_string(),
                conn_name.to_string(),
                "ipv4.dns".to_string(),
                dns_value.clone(),
                "ipv4.ignore-auto-dns".to_string(),
                "yes".to_string(),
            ]
        };

        let modify = tokio::process::Command::new("nmcli")
            .args(&dns_args)
            .output()
            .await
            .context("Failed to modify DNS via nmcli")?;

        if !modify.status.success() {
            let stderr = String::from_utf8_lossy(&modify.stderr);
            tracing::warn!(conn = conn_name, err = %stderr, "Failed to set DNS on connection");
            continue;
        }

        // Reapply the connection to pick up changes
        let reapply = tokio::process::Command::new("nmcli")
            .args(["connection", "up", conn_name])
            .output()
            .await;

        match reapply {
            Ok(out) if out.status.success() => {
                info!(conn = conn_name, "DNS updated successfully");
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                tracing::warn!(conn = conn_name, err = %stderr, "Failed to reapply connection");
            }
            Err(e) => {
                tracing::warn!(conn = conn_name, err = %e, "Failed to reapply connection");
            }
        }
    }

    Ok(())
}

/// Configure DNS with a specific provider.
pub async fn configure(
    data_dir: &Path,
    provider: DnsProvider,
    custom_servers: Vec<String>,
) -> Result<DnsConfig> {
    let (servers, doh_url) = if provider == DnsProvider::Custom {
        (custom_servers, None)
    } else {
        let (preset_servers, preset_doh) = provider_servers(&provider);
        (preset_servers, preset_doh)
    };

    let doh_enabled = doh_url.is_some();

    let config = DnsConfig {
        provider,
        servers,
        doh_enabled,
        doh_url,
    };

    // Apply to system
    apply_dns(&config).await?;

    // Persist config
    save_config(data_dir, &config).await?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_config() {
        let config = DnsConfig::default();
        assert_eq!(config.provider, DnsProvider::System);
        assert!(config.servers.is_empty());
        assert!(!config.doh_enabled);
    }

    #[test]
    fn test_provider_servers() {
        let (servers, doh) = provider_servers(&DnsProvider::Cloudflare);
        assert_eq!(servers, vec!["1.1.1.1", "1.0.0.1"]);
        assert!(doh.unwrap().contains("cloudflare"));

        let (servers, doh) = provider_servers(&DnsProvider::System);
        assert!(servers.is_empty());
        assert!(doh.is_none());

        let (servers, doh) = provider_servers(&DnsProvider::Custom);
        assert!(servers.is_empty());
        assert!(doh.is_none());
    }

    #[test]
    fn test_provider_display() {
        assert_eq!(DnsProvider::Cloudflare.to_string(), "cloudflare");
        assert_eq!(DnsProvider::System.to_string(), "system");
        assert_eq!(DnsProvider::Quad9.to_string(), "quad9");
    }

    #[tokio::test]
    async fn test_config_persistence() {
        let dir = tempdir().unwrap();
        let config = DnsConfig {
            provider: DnsProvider::Cloudflare,
            servers: vec!["1.1.1.1".into(), "1.0.0.1".into()],
            doh_enabled: true,
            doh_url: Some("https://cloudflare-dns.com/dns-query".into()),
        };
        save_config(dir.path(), &config).await.unwrap();
        let loaded = load_config(dir.path()).await.unwrap();
        assert_eq!(loaded.provider, DnsProvider::Cloudflare);
        assert_eq!(loaded.servers.len(), 2);
        assert!(loaded.doh_enabled);
    }

    #[tokio::test]
    async fn test_load_missing_config_returns_default() {
        let dir = tempdir().unwrap();
        let config = load_config(dir.path()).await.unwrap();
        assert_eq!(config.provider, DnsProvider::System);
    }

    #[test]
    fn test_config_serialization() {
        let config = DnsConfig {
            provider: DnsProvider::Google,
            servers: vec!["8.8.8.8".into()],
            doh_enabled: true,
            doh_url: Some("https://dns.google/dns-query".into()),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: DnsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider, DnsProvider::Google);
        assert_eq!(parsed.servers, vec!["8.8.8.8"]);
    }
}
