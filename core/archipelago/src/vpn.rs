//! VPN integration: Tailscale and WireGuard management.
//!
//! Manages VPN connections, generates WireGuard configs, and monitors
//! VPN interface status for remote access to the Archipelago node.

use anyhow::{Context, Result};
use nostr_sdk::ToBech32;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

const VPN_CONFIG_FILE: &str = "vpn-config.json";

/// Known locations for the nvpn config file.
pub const NVPN_CONFIG_PATHS: &[&str] = &[
    "/var/lib/archipelago/nostr-vpn/.config/nvpn/config.toml",
    "/home/archipelago/.config/nvpn/config.toml",
    "/home/debian/.config/nvpn/config.toml",
    "/root/.config/nvpn/config.toml",
];

/// Read a value from the nvpn TOML config (e.g. section="node", key="public_key").
pub async fn read_nvpn_config_value(section: &str, key: &str) -> Option<String> {
    for path in NVPN_CONFIG_PATHS {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            let mut in_section = false;
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('[') {
                    in_section =
                        trimmed.trim_start_matches('[').trim_end_matches(']').trim() == section;
                } else if in_section {
                    if let Some(pos) = trimmed.find('=') {
                        let k = trimmed[..pos].trim();
                        if k == key {
                            let v = trimmed[pos + 1..].trim().trim_matches('"');
                            return Some(v.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Read a value from the first entry of a TOML array of tables (e.g. [[networks]]).
pub async fn read_nvpn_config_list_entry(section: &str, key: &str) -> Option<String> {
    for path in NVPN_CONFIG_PATHS {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            if let Ok(table) = content.parse::<toml::Table>() {
                if let Some(arr) = table.get(section).and_then(|v| v.as_array()) {
                    if let Some(first) = arr.first().and_then(|v| v.as_table()) {
                        if let Some(val) = first.get(key).and_then(|v| v.as_str()) {
                            return Some(val.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

/// Get the node's private Nostr relay URLs.
/// Returns (onion_url, direct_url) — onion works behind NAT via Tor, direct needs public IP.
pub async fn get_relay_urls() -> (Option<String>, Option<String>) {
    let mut onion_url = None;
    let mut direct_url = None;

    // Tor hidden service relay URL (works without public IP)
    let onion_paths = [
        "/var/lib/archipelago/tor-hostnames/relay",
        "/var/lib/archipelago/relay-onion-hostname",
        "/var/lib/tor/hidden_service_relay/hostname",
    ];
    for path in &onion_paths {
        if let Ok(hostname) = tokio::fs::read_to_string(path).await {
            let hostname = hostname.trim();
            if !hostname.is_empty() && hostname.ends_with(".onion") {
                onion_url = Some(format!("ws://{}:7777", hostname));
                break;
            }
        }
    }

    // Direct IP relay URL (only if public IP available)
    if let Ok(output) = tokio::process::Command::new("hostname")
        .arg("-I")
        .output()
        .await
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(ip) = stdout.split_whitespace().next() {
            if !ip.starts_with("10.") && !ip.starts_with("192.168.") && !ip.starts_with("172.") {
                direct_url = Some(format!("ws://{}:7777", ip));
            }
        }
    }

    (onion_url, direct_url)
}

/// Read an array of strings from the nvpn TOML config (e.g. relays list).
pub async fn read_nvpn_config_list(section: &str, key: &str) -> Vec<String> {
    for path in NVPN_CONFIG_PATHS {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            if let Ok(table) = content.parse::<toml::Table>() {
                if let Some(sec) = table.get(section).and_then(|v| v.as_table()) {
                    if let Some(arr) = sec.get(key).and_then(|v| v.as_array()) {
                        return arr
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect();
                    }
                }
            }
        }
    }
    Vec::new()
}

/// VPN provider type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VpnProvider {
    Tailscale,
    Wireguard,
    NostrVpn,
}

/// Persisted VPN configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnConfig {
    pub provider: VpnProvider,
    pub enabled: bool,
    #[serde(default)]
    pub tailscale_auth_key: Option<String>,
    #[serde(default)]
    pub wireguard_config: Option<WireGuardConfig>,
    #[serde(default)]
    pub configured_at: Option<String>,
}

/// WireGuard configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardConfig {
    pub private_key: String,
    pub public_key: String,
    pub address: String,
    pub dns: String,
    #[serde(default)]
    pub peers: Vec<WireGuardPeer>,
}

/// A WireGuard peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardPeer {
    pub public_key: String,
    pub endpoint: String,
    pub allowed_ips: String,
    #[serde(default)]
    pub persistent_keepalive: Option<u16>,
}

/// Current VPN status (gathered at runtime, not persisted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VpnStatus {
    pub connected: bool,
    pub provider: Option<String>,
    pub interface: Option<String>,
    pub ip_address: Option<String>,
    pub hostname: Option<String>,
    #[serde(default)]
    pub peers_connected: u32,
    #[serde(default)]
    pub bytes_in: u64,
    #[serde(default)]
    pub bytes_out: u64,
}

impl Default for VpnConfig {
    fn default() -> Self {
        Self {
            provider: VpnProvider::Tailscale,
            enabled: false,
            tailscale_auth_key: None,
            wireguard_config: None,
            configured_at: None,
        }
    }
}

pub async fn load_config(data_dir: &Path) -> Result<VpnConfig> {
    let path = data_dir.join(VPN_CONFIG_FILE);
    if !path.exists() {
        return Ok(VpnConfig::default());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read VPN config")?;
    let config: VpnConfig = serde_json::from_str(&content).unwrap_or_default();
    Ok(config)
}

pub async fn save_config(data_dir: &Path, config: &VpnConfig) -> Result<()> {
    fs::create_dir_all(data_dir)
        .await
        .context("Failed to create data dir")?;
    let content = serde_json::to_string_pretty(config).context("Failed to serialize VPN config")?;
    fs::write(data_dir.join(VPN_CONFIG_FILE), content)
        .await
        .context("Failed to write VPN config")?;
    Ok(())
}

/// Generate a WireGuard keypair using the `wg` command.
pub async fn generate_wireguard_keypair() -> Result<(String, String)> {
    let privkey_output = tokio::process::Command::new("wg")
        .arg("genkey")
        .output()
        .await
        .context("Failed to run wg genkey — is wireguard-tools installed?")?;

    if !privkey_output.status.success() {
        anyhow::bail!(
            "wg genkey failed: {}",
            String::from_utf8_lossy(&privkey_output.stderr)
        );
    }

    let private_key = String::from_utf8(privkey_output.stdout)
        .context("Invalid UTF-8 from wg genkey")?
        .trim()
        .to_string();

    let mut child = tokio::process::Command::new("wg")
        .arg("pubkey")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("Failed to spawn wg pubkey")?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin
            .write_all(private_key.as_bytes())
            .await
            .context("Failed to write private key to wg stdin")?;
    }

    let output = child
        .wait_with_output()
        .await
        .context("wg pubkey process failed")?;

    if !output.status.success() {
        anyhow::bail!(
            "wg pubkey failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let public_key = String::from_utf8(output.stdout)
        .context("wg pubkey output is not valid UTF-8")?
        .trim()
        .to_string();

    Ok((private_key, public_key))
}

/// Generate a WireGuard configuration file content.
pub fn generate_wireguard_conf(config: &WireGuardConfig) -> String {
    let mut conf = format!(
        "[Interface]\nPrivateKey = {}\nAddress = {}\nDNS = {}\n",
        config.private_key, config.address, config.dns
    );

    for peer in &config.peers {
        conf.push_str(&format!(
            "\n[Peer]\nPublicKey = {}\nEndpoint = {}\nAllowedIPs = {}\n",
            peer.public_key, peer.endpoint, peer.allowed_ips
        ));
        if let Some(ka) = peer.persistent_keepalive {
            conf.push_str(&format!("PersistentKeepalive = {}\n", ka));
        }
    }

    conf
}

/// Get the current VPN status by checking network interfaces.
pub async fn get_status() -> VpnStatus {
    // NostrVPN disabled — standalone WireGuard only

    // Check for Tailscale interface
    if let Ok(tailscale) = get_tailscale_status().await {
        return tailscale;
    }

    // Check for WireGuard interface (wg0)
    if let Ok(wg) = get_wireguard_status().await {
        return wg;
    }

    VpnStatus {
        connected: false,
        provider: None,
        interface: None,
        ip_address: None,
        hostname: None,
        peers_connected: 0,
        bytes_in: 0,
        bytes_out: 0,
    }
}

/// Check if NostrVPN system service is running and get its status.
#[allow(dead_code)]
async fn get_nostr_vpn_status() -> Result<VpnStatus> {
    // Fast check: is the service unit enabled/active?
    let svc_state = tokio::process::Command::new("systemctl")
        .args(["is-active", "nostr-vpn"])
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    if svc_state != "active" && svc_state != "activating" {
        anyhow::bail!("nostr-vpn service not running");
    }

    // Get tunnel IP: config file first (instant), fall back to interface
    // NOTE: Do NOT call `nvpn status` — it blocks for 20+ seconds connecting to relays
    let ip = {
        let cfg_ip = read_nvpn_config_value("node", "tunnel_ip").await;
        if cfg_ip.is_some() {
            cfg_ip
        } else {
            // Fallback: read from network interface
            tokio::process::Command::new("ip")
                .args(["-4", "addr", "show", "nvpn0"])
                .output()
                .await
                .ok()
                .and_then(|o| {
                    let out = String::from_utf8_lossy(&o.stdout).to_string();
                    out.lines()
                        .find(|l| l.contains("inet "))
                        .and_then(|l| l.split_whitespace().nth(1))
                        .map(|s| s.to_string())
                })
        }
    };

    Ok(VpnStatus {
        connected: svc_state == "active",
        provider: Some("nostr-vpn".to_string()),
        interface: Some("nvpn0".to_string()),
        ip_address: ip,
        hostname: None,
        peers_connected: 0,
        bytes_in: 0,
        bytes_out: 0,
    })
}

/// Convert a hex public key to npub1... bech32 format.
/// Returns the original string if already npub1 or conversion fails.
pub fn ensure_npub(key: &str) -> String {
    let key = key.trim();
    if key.starts_with("npub1") {
        return key.to_string();
    }
    nostr_sdk::PublicKey::from_hex(key)
        .ok()
        .and_then(|pk| pk.to_bech32().ok())
        .unwrap_or_else(|| key.to_string())
}

/// Convert a hex secret key to nsec1... bech32 format.
/// Returns the original string if already nsec1 or conversion fails.
fn ensure_nsec(key: &str) -> String {
    let key = key.trim();
    if key.starts_with("nsec1") {
        return key.to_string();
    }
    nostr_sdk::SecretKey::from_hex(key)
        .ok()
        .and_then(|sk| sk.to_bech32().ok())
        .unwrap_or_else(|| key.to_string())
}

/// Configure NostrVPN with the node's Nostr identity.
/// Writes both the env file (for systemd) and config.toml (for vpn.invite/status).
pub async fn configure_nostr_vpn(data_dir: &Path) -> Result<()> {
    let nostr_secret_hex = tokio::fs::read_to_string(data_dir.join("identity/nostr_secret"))
        .await
        .context("No Nostr secret key — complete onboarding first")?;

    let nostr_pubkey_hex = tokio::fs::read_to_string(data_dir.join("identity/nostr_pubkey"))
        .await
        .unwrap_or_default();

    let nostr_secret_hex = nostr_secret_hex.trim();
    let nostr_pubkey_hex = nostr_pubkey_hex.trim();

    if nostr_pubkey_hex.is_empty() {
        anyhow::bail!("Empty Nostr public key — identity not ready");
    }

    // Convert hex keys to bech32 (npub1.../nsec1...)
    let npub = ensure_npub(nostr_pubkey_hex);
    let nsec = ensure_nsec(nostr_secret_hex);

    let vpn_dir = data_dir.join("nostr-vpn");
    tokio::fs::create_dir_all(&vpn_dir)
        .await
        .context("Failed to create nostr-vpn dir")?;

    // Write env file for the systemd service
    let env_content = format!(
        "NOSTR_SECRET={}\nNOSTR_PUBKEY={}\n",
        nostr_secret_hex, nostr_pubkey_hex
    );
    tokio::fs::write(vpn_dir.join("env"), &env_content)
        .await
        .context("Failed to write nostr-vpn env")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(vpn_dir.join("env"), std::fs::Permissions::from_mode(0o600)).ok();
    }

    // Write nvpn config.toml so vpn.invite and vpn.status can read the node identity.
    // This is the primary fix: previously only env was written, but all VPN RPC handlers
    // read from config.toml — not env vars.
    let config_dir = vpn_dir.join(".config/nvpn");
    tokio::fs::create_dir_all(&config_dir)
        .await
        .context("Failed to create nvpn config dir")?;

    let config_path = config_dir.join("config.toml");
    // Only write if config doesn't exist or has no public_key
    // (avoid clobbering participants list added by vpn.add-participant)
    let should_write = if let Ok(existing) = tokio::fs::read_to_string(&config_path).await {
        !existing.contains("public_key")
    } else {
        true
    };

    if should_write {
        // Gather relay URLs for the config
        let (relay_onion, relay_direct) = get_relay_urls().await;
        // Always include public relays so peers can discover each other.
        // Local/onion relays are added as extras for direct connectivity.
        let mut relays = vec![
            "\"wss://relay.damus.io\"".to_string(),
            "\"wss://relay.primal.net\"".to_string(),
        ];
        if let Some(ref onion) = relay_onion {
            relays.push(format!("\"{}\"", onion));
        }
        if let Some(ref direct) = relay_direct {
            relays.push(format!("\"{}\"", direct));
        }

        let config_toml = format!(
            "[nostr]\npublic_key = \"{npub}\"\nsecret_key = \"{nsec}\"\nrelays = [{relays}]\n\n[[networks]]\nnetwork_id = \"archipelago\"\nparticipants = []\n",
            npub = npub,
            nsec = nsec,
            relays = relays.join(", "),
        );
        tokio::fs::write(&config_path, &config_toml)
            .await
            .context("Failed to write nvpn config.toml")?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600)).ok();
        }

        tracing::info!("Wrote nvpn config.toml with node npub");
    }

    // Reset any previous failure state (systemd rate-limits restarts before onboarding)
    let _ = tokio::process::Command::new("systemctl")
        .args(["reset-failed", "nostr-vpn"])
        .output()
        .await;

    // Enable and start the service
    tokio::process::Command::new("systemctl")
        .args(["enable", "--now", "nostr-vpn"])
        .output()
        .await
        .context("Failed to enable nostr-vpn service")?;

    let mut config = load_config(data_dir).await?;
    config.provider = VpnProvider::NostrVpn;
    config.enabled = true;
    config.configured_at = Some(chrono::Utc::now().to_rfc3339());
    save_config(data_dir, &config).await?;

    Ok(())
}

async fn get_tailscale_status() -> Result<VpnStatus> {
    // Check if tailscale0 interface exists
    let output = tokio::process::Command::new("ip")
        .args(["addr", "show", "tailscale0"])
        .output()
        .await
        .context("Failed to check tailscale0")?;

    if !output.status.success() {
        anyhow::bail!("No tailscale0 interface");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let ip = stdout
        .lines()
        .find(|l| l.contains("inet ") && !l.contains("inet6"))
        .and_then(|l| {
            l.split_whitespace()
                .nth(1)
                .map(|ip| ip.split('/').next().unwrap_or(ip).to_string())
        });

    // Try to get hostname from tailscale status
    let hostname = tokio::process::Command::new("sh")
        .arg("-c")
        .arg("podman exec tailscale tailscale status --self --json 2>/dev/null | grep -o '\"DNSName\":\"[^\"]*\"' | head -1 | cut -d'\"' -f4")
        .output()
        .await
        .ok()
        .and_then(|o| {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() { None } else { Some(s.trim_end_matches('.').to_string()) }
        });

    // Get peer count
    let peers = tokio::process::Command::new("sh")
        .arg("-c")
        .arg("podman exec tailscale tailscale status 2>/dev/null | grep -c 'active' || echo 0")
        .output()
        .await
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(0);

    Ok(VpnStatus {
        connected: ip.is_some(),
        provider: Some("tailscale".to_string()),
        interface: Some("tailscale0".to_string()),
        ip_address: ip,
        hostname,
        peers_connected: peers,
        bytes_in: 0,
        bytes_out: 0,
    })
}

async fn get_wireguard_status() -> Result<VpnStatus> {
    let output = tokio::process::Command::new("ip")
        .args(["addr", "show", "wg0"])
        .output()
        .await
        .context("Failed to check wg0")?;

    if !output.status.success() {
        anyhow::bail!("No wg0 interface");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let ip = stdout
        .lines()
        .find(|l| l.contains("inet ") && !l.contains("inet6"))
        .and_then(|l| {
            l.split_whitespace()
                .nth(1)
                .map(|ip| ip.split('/').next().unwrap_or(ip).to_string())
        });

    // Get peer count from wg show
    let peers = tokio::process::Command::new("sh")
        .arg("-c")
        .arg("wg show wg0 peers 2>/dev/null | wc -l")
        .output()
        .await
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(0);

    // Get transfer stats
    let (bytes_in, bytes_out) = tokio::process::Command::new("sh")
        .arg("-c")
        .arg("wg show wg0 transfer 2>/dev/null | awk '{i+=$2; o+=$3} END {print i, o}'")
        .output()
        .await
        .ok()
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout);
            let mut parts = s.split_whitespace();
            let i = parts
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            let o = parts
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            (i, o)
        })
        .unwrap_or((0, 0));

    Ok(VpnStatus {
        connected: ip.is_some(),
        provider: Some("wireguard".to_string()),
        interface: Some("wg0".to_string()),
        ip_address: ip,
        hostname: None,
        peers_connected: peers,
        bytes_in,
        bytes_out,
    })
}

/// Configure Tailscale with an auth key (triggers tailscale up).
pub async fn configure_tailscale(auth_key: &str, data_dir: &Path) -> Result<()> {
    let mut config = load_config(data_dir).await?;
    config.provider = VpnProvider::Tailscale;
    config.enabled = true;
    config.tailscale_auth_key = Some(auth_key.to_string());
    config.configured_at = Some(chrono::Utc::now().to_rfc3339());
    save_config(data_dir, &config).await?;

    // Run tailscale up with auth key in the container
    let output = tokio::process::Command::new("podman")
        .args([
            "exec",
            "tailscale",
            "tailscale",
            "up",
            "--authkey",
            auth_key,
            "--accept-routes",
        ])
        .output()
        .await
        .context("Failed to run tailscale up")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("tailscale up failed: {}", stderr);
    }

    Ok(())
}

/// Configure WireGuard with generated keys and optional peer.
pub async fn configure_wireguard(
    data_dir: &Path,
    address: &str,
    dns: &str,
    peer: Option<WireGuardPeer>,
) -> Result<WireGuardConfig> {
    let (private_key, public_key) = generate_wireguard_keypair().await?;

    let wg_config = WireGuardConfig {
        private_key: private_key.clone(),
        public_key: public_key.clone(),
        address: address.to_string(),
        dns: dns.to_string(),
        peers: peer.map_or_else(Vec::new, |p| vec![p]),
    };

    let mut config = load_config(data_dir).await?;
    config.provider = VpnProvider::Wireguard;
    config.enabled = true;
    config.wireguard_config = Some(wg_config.clone());
    config.configured_at = Some(chrono::Utc::now().to_rfc3339());
    save_config(data_dir, &config).await?;

    // Write WireGuard config file
    let conf_content = generate_wireguard_conf(&wg_config);
    let wg_dir = data_dir.join("wireguard");
    fs::create_dir_all(&wg_dir)
        .await
        .context("Failed to create wireguard dir")?;
    fs::write(wg_dir.join("wg0.conf"), &conf_content)
        .await
        .context("Failed to write wg0.conf")?;

    Ok(wg_config)
}

/// Restore WireGuard peers from `data_dir/nostr-vpn/peers/*.json` into the
/// kernel after a reboot.
///
/// Kernel peer state is ephemeral. The add-peer RPC persists each peer to
/// a JSON file but only pushes it into wg0 at creation time — without this
/// restore step, every reboot drops all peers and the user has to re-add
/// them via QR.
pub async fn restore_wg_peers(data_dir: &Path) {
    let peers_dir = data_dir.join("nostr-vpn/peers");
    let mut entries = match fs::read_dir(&peers_dir).await {
        Ok(e) => e,
        Err(_) => return,
    };

    // archipelago-wg.service may race us on boot; wait up to 30s for wg0.
    for _ in 0..30 {
        let up = tokio::process::Command::new("ip")
            .args(["link", "show", "wg0"])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);
        if up {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    let mut restored = 0usize;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        let content = match fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("VPN restore: read {:?} failed: {}", path, e);
                continue;
            }
        };
        let v: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("VPN restore: parse {:?} failed: {}", path, e);
                continue;
            }
        };
        let pubkey = v.get("public_key").and_then(|s| s.as_str());
        let ip = v.get("ip").and_then(|s| s.as_str());
        let (pubkey, ip) = match (pubkey, ip) {
            (Some(p), Some(i)) => (p, i),
            _ => {
                tracing::warn!("VPN restore: {:?} missing public_key or ip", path);
                continue;
            }
        };

        let out = tokio::process::Command::new("sudo")
            .args(["archipelago-wg", "add-peer", pubkey, ip])
            .output()
            .await;
        match out {
            Ok(o) if o.status.success() => restored += 1,
            Ok(o) => tracing::warn!(
                "VPN restore: add-peer failed for {}: {}",
                pubkey,
                String::from_utf8_lossy(&o.stderr).trim()
            ),
            Err(e) => tracing::warn!("VPN restore: add-peer spawn failed: {}", e),
        }
    }

    if restored > 0 {
        tracing::info!(
            "🔐 VPN: restored {} WireGuard peer(s) from {}",
            restored,
            peers_dir.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vpn_config_default() {
        let config = VpnConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.provider, VpnProvider::Tailscale);
        assert!(config.tailscale_auth_key.is_none());
    }

    #[test]
    fn test_vpn_config_serialization() {
        let config = VpnConfig {
            provider: VpnProvider::Wireguard,
            enabled: true,
            tailscale_auth_key: None,
            wireguard_config: Some(WireGuardConfig {
                private_key: "privkey".to_string(),
                public_key: "pubkey".to_string(),
                address: "10.0.0.1/24".to_string(),
                dns: "1.1.1.1".to_string(),
                peers: vec![],
            }),
            configured_at: Some("2026-01-01T00:00:00Z".to_string()),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: VpnConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.provider, VpnProvider::Wireguard);
        assert!(parsed.enabled);
        assert!(parsed.wireguard_config.is_some());
    }

    #[test]
    fn test_generate_wireguard_conf() {
        let config = WireGuardConfig {
            private_key: "test_privkey".to_string(),
            public_key: "test_pubkey".to_string(),
            address: "10.0.0.2/24".to_string(),
            dns: "1.1.1.1, 8.8.8.8".to_string(),
            peers: vec![WireGuardPeer {
                public_key: "peer_pubkey".to_string(),
                endpoint: "vpn.example.com:51820".to_string(),
                allowed_ips: "0.0.0.0/0".to_string(),
                persistent_keepalive: Some(25),
            }],
        };
        let conf = generate_wireguard_conf(&config);
        assert!(conf.contains("[Interface]"));
        assert!(conf.contains("PrivateKey = test_privkey"));
        assert!(conf.contains("Address = 10.0.0.2/24"));
        assert!(conf.contains("[Peer]"));
        assert!(conf.contains("PublicKey = peer_pubkey"));
        assert!(conf.contains("PersistentKeepalive = 25"));
    }

    #[tokio::test]
    async fn test_load_config_default_when_no_file() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_config(dir.path()).await.unwrap();
        assert!(!config.enabled);
    }

    #[tokio::test]
    async fn test_save_and_load_config_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let config = VpnConfig {
            provider: VpnProvider::Tailscale,
            enabled: true,
            tailscale_auth_key: Some("tskey-auth-test".to_string()),
            wireguard_config: None,
            configured_at: Some("2026-03-10T00:00:00Z".to_string()),
        };
        save_config(dir.path(), &config).await.unwrap();
        let loaded = load_config(dir.path()).await.unwrap();
        assert!(loaded.enabled);
        assert_eq!(
            loaded.tailscale_auth_key,
            Some("tskey-auth-test".to_string())
        );
    }

    #[test]
    fn test_vpn_status_default() {
        let status = VpnStatus {
            connected: false,
            provider: None,
            interface: None,
            ip_address: None,
            hostname: None,
            peers_connected: 0,
            bytes_in: 0,
            bytes_out: 0,
        };
        assert!(!status.connected);
    }
}
