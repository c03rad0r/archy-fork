//! UPnP port forwarding and network router integration.
//! Discovers UPnP-capable routers and manages port forwards for exposed services.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::debug;

const FORWARDS_FILE: &str = "port_forwards.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForward {
    pub id: String,
    pub service_name: String,
    pub internal_port: u16,
    pub external_port: u16,
    pub protocol: String,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ForwardStore {
    pub forwards: Vec<PortForward>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterInfo {
    pub discovered: bool,
    pub device_name: Option<String>,
    pub wan_ip: Option<String>,
    pub upnp_available: bool,
}

pub async fn load_forwards(data_dir: &Path) -> Result<ForwardStore> {
    let path = data_dir.join(FORWARDS_FILE);
    if !path.exists() {
        return Ok(ForwardStore::default());
    }
    let data = fs::read_to_string(&path)
        .await
        .context("Reading forwards")?;
    serde_json::from_str(&data).context("Parsing forwards")
}

pub async fn save_forwards(data_dir: &Path, store: &ForwardStore) -> Result<()> {
    let path = data_dir.join(FORWARDS_FILE);
    let data = serde_json::to_string_pretty(store)?;
    fs::write(&path, data).await.context("Writing forwards")
}

/// Discover UPnP gateway on the local network.
/// Uses a simple SSDP M-SEARCH to find IGD (Internet Gateway Device).
pub async fn discover_router() -> Result<RouterInfo> {
    // Attempt UPnP discovery via SSDP
    let wan_ip = get_wan_ip().await;

    // Try to find a UPnP gateway by sending SSDP M-SEARCH
    let upnp_available = check_upnp_available().await;

    Ok(RouterInfo {
        discovered: upnp_available,
        device_name: if upnp_available {
            Some("UPnP Gateway".to_string())
        } else {
            None
        },
        wan_ip,
        upnp_available,
    })
}

/// Get WAN IP address via external service.
async fn get_wan_ip() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .ok()?;

    // Try multiple services for redundancy
    for url in &[
        "https://api.ipify.org",
        "https://ifconfig.me/ip",
        "https://icanhazip.com",
    ] {
        if let Ok(resp) = client.get(*url).send().await {
            if let Ok(ip) = resp.text().await {
                let ip = ip.trim().to_string();
                if !ip.is_empty() && ip.len() < 50 {
                    return Some(ip);
                }
            }
        }
    }
    None
}

/// Check if UPnP is available by attempting SSDP discovery.
async fn check_upnp_available() -> bool {
    use std::net::UdpSocket;

    let ssdp_request = "M-SEARCH * HTTP/1.1\r\n\
        HOST: 239.255.255.250:1900\r\n\
        MAN: \"ssdp:discover\"\r\n\
        MX: 2\r\n\
        ST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\n\r\n";

    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return false,
    };

    if socket
        .set_read_timeout(Some(std::time::Duration::from_secs(3)))
        .is_err()
    {
        return false;
    }

    if socket
        .send_to(ssdp_request.as_bytes(), "239.255.255.250:1900")
        .is_err()
    {
        return false;
    }

    let mut buf = [0u8; 2048];
    match socket.recv_from(&mut buf) {
        Ok((len, _)) => {
            let response = String::from_utf8_lossy(&buf[..len]);
            response.contains("InternetGatewayDevice") || response.contains("200 OK")
        }
        Err(_) => false,
    }
}

/// Add a port forward (stored locally; actual UPnP mapping done on request).
pub async fn add_forward(
    data_dir: &Path,
    service_name: &str,
    internal_port: u16,
    external_port: u16,
    protocol: &str,
) -> Result<PortForward> {
    let mut store = load_forwards(data_dir).await?;

    if store
        .forwards
        .iter()
        .any(|f| f.external_port == external_port && f.protocol == protocol)
    {
        return Err(anyhow::anyhow!(
            "Port {} ({}) is already forwarded",
            external_port,
            protocol
        ));
    }

    let forward = PortForward {
        id: uuid::Uuid::new_v4().to_string(),
        service_name: service_name.to_string(),
        internal_port,
        external_port,
        protocol: protocol.to_uppercase(),
        enabled: true,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    debug!(
        service = %service_name,
        port = external_port,
        "Added port forward"
    );
    store.forwards.push(forward.clone());
    save_forwards(data_dir, &store).await?;
    Ok(forward)
}

/// Remove a port forward.
pub async fn remove_forward(data_dir: &Path, forward_id: &str) -> Result<()> {
    let mut store = load_forwards(data_dir).await?;
    let original_len = store.forwards.len();
    store.forwards.retain(|f| f.id != forward_id);
    if store.forwards.len() == original_len {
        return Err(anyhow::anyhow!("Forward not found: {}", forward_id));
    }
    save_forwards(data_dir, &store).await
}

/// List all port forwards.
pub async fn list_forwards(data_dir: &Path) -> Result<Vec<PortForward>> {
    let store = load_forwards(data_dir).await?;
    Ok(store.forwards)
}

/// Network diagnostics result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkDiagnostics {
    pub wan_ip: Option<String>,
    pub nat_type: String,
    pub upnp_available: bool,
    pub tor_connected: bool,
    pub dns_working: bool,
    pub recommendations: Vec<String>,
    /// SSID of the currently-active WiFi connection, or None if the node is on
    /// wired / no WiFi adapter / NetworkManager isn't around.
    pub wifi_ssid: Option<String>,
}

/// Ask NetworkManager for the active WiFi SSID. Returns None silently if
/// nmcli is unavailable or no WiFi device is connected.
async fn active_wifi_ssid() -> Option<String> {
    let out = tokio::process::Command::new("nmcli")
        .args(["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout.lines() {
        // DEVICE:TYPE:STATE:CONNECTION — colons inside fields are escaped by nmcli -t
        let mut parts = line.split(':');
        let _dev = parts.next()?;
        let typ = parts.next().unwrap_or("");
        let state = parts.next().unwrap_or("");
        let conn = parts.next().unwrap_or("");
        if typ == "wifi" && state == "connected" && !conn.is_empty() {
            return Some(conn.to_string());
        }
    }
    None
}

/// Run a comprehensive network diagnostic check.
pub async fn run_diagnostics() -> Result<NetworkDiagnostics> {
    let wan_ip = get_wan_ip().await;
    let upnp_available = check_upnp_available().await;
    let tor_connected = check_tor_connectivity().await;
    let dns_working = check_dns().await;
    let wifi_ssid = active_wifi_ssid().await;

    let nat_type = if wan_ip.is_some() {
        if upnp_available {
            "Open (UPnP)".to_string()
        } else {
            "Restricted".to_string()
        }
    } else {
        "Unknown".to_string()
    };

    let mut recommendations = Vec::new();
    if !upnp_available {
        recommendations
            .push("Enable UPnP on your router for automatic port forwarding".to_string());
    }
    if !tor_connected {
        recommendations
            .push("Tor is not connected — check the Tor container is running".to_string());
    }
    if !dns_working {
        recommendations.push("DNS resolution failed — check your network connection".to_string());
    }
    if wan_ip.is_none() {
        recommendations
            .push("Could not determine WAN IP — you may be behind a firewall".to_string());
    }

    Ok(NetworkDiagnostics {
        wan_ip,
        nat_type,
        upnp_available,
        tor_connected,
        dns_working,
        recommendations,
        wifi_ssid,
    })
}

/// Check if Tor SOCKS proxy is reachable.
async fn check_tor_connectivity() -> bool {
    use std::net::TcpStream;
    TcpStream::connect_timeout(
        &"127.0.0.1:9050".parse().unwrap(),
        std::time::Duration::from_secs(2),
    )
    .is_ok()
}

/// Check DNS resolution works.
async fn check_dns() -> bool {
    use std::net::ToSocketAddrs;
    "cloudflare.com:443".to_socket_addrs().is_ok()
}

// --- Router Compatibility Abstraction ---

/// Detected router type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RouterType {
    UPnP,
    OpenWrt,
    PfSense,
    OPNsense,
    Unknown,
}

impl std::fmt::Display for RouterType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouterType::UPnP => write!(f, "UPnP"),
            RouterType::OpenWrt => write!(f, "OpenWrt"),
            RouterType::PfSense => write!(f, "pfSense"),
            RouterType::OPNsense => write!(f, "OPNsense"),
            RouterType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Router configuration stored for API access.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterConfig {
    pub router_type: RouterType,
    pub address: String,
    pub api_key: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub configured: bool,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            router_type: RouterType::Unknown,
            address: String::new(),
            api_key: None,
            username: None,
            password: None,
            configured: false,
        }
    }
}

const ROUTER_CONFIG_FILE: &str = "router_config.json";

pub async fn load_router_config(data_dir: &Path) -> Result<RouterConfig> {
    let path = data_dir.join(ROUTER_CONFIG_FILE);
    if !path.exists() {
        return Ok(RouterConfig::default());
    }
    let data = fs::read_to_string(&path)
        .await
        .context("Reading router config")?;
    serde_json::from_str(&data).context("Parsing router config")
}

pub async fn save_router_config(data_dir: &Path, config: &RouterConfig) -> Result<()> {
    let path = data_dir.join(ROUTER_CONFIG_FILE);
    let data = serde_json::to_string_pretty(config)?;
    fs::write(&path, data)
        .await
        .context("Writing router config")
}

/// Validate that an IP string is a private/LAN address (not public, not localhost).
fn is_valid_private_ip(ip_str: &str) -> bool {
    let ip: std::net::IpAddr = match ip_str.parse() {
        Ok(ip) => ip,
        Err(_) => return false, // Reject hostnames
    };
    match ip {
        std::net::IpAddr::V4(v4) => {
            // Allow only RFC1918 private ranges, reject localhost and public
            let octets = v4.octets();
            let is_10 = octets[0] == 10;
            let is_172_private = octets[0] == 172 && (16..=31).contains(&octets[1]);
            let is_192_168 = octets[0] == 192 && octets[1] == 168;
            is_10 || is_172_private || is_192_168
        }
        std::net::IpAddr::V6(_) => false, // Reject IPv6 for gateway detection
    }
}

/// Detect router type by probing common endpoints on the gateway.
pub async fn detect_router_type(gateway_ip: &str) -> RouterType {
    // Validate that gateway is a private IP — prevent SSRF to arbitrary hosts
    if !is_valid_private_ip(gateway_ip) {
        tracing::warn!(gateway = gateway_ip, "Rejected non-private gateway IP");
        return RouterType::Unknown;
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap_or_default();

    // Check for OpenWrt (LuCI)
    if let Ok(resp) = client
        .get(format!("http://{}/cgi-bin/luci", gateway_ip))
        .send()
        .await
    {
        if resp.status().is_success() || resp.status().is_redirection() {
            return RouterType::OpenWrt;
        }
    }

    // Check for pfSense
    if let Ok(resp) = client.get(format!("https://{}/", gateway_ip)).send().await {
        if let Ok(body) = resp.text().await {
            if body.contains("pfSense") {
                return RouterType::PfSense;
            }
            if body.contains("OPNsense") {
                return RouterType::OPNsense;
            }
        }
    }

    // Fallback: check UPnP
    if check_upnp_available().await {
        return RouterType::UPnP;
    }

    RouterType::Unknown
}

/// Configure router API access.
pub async fn configure_router(
    data_dir: &Path,
    router_type: RouterType,
    address: &str,
    api_key: Option<&str>,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<RouterConfig> {
    let config = RouterConfig {
        router_type,
        address: address.to_string(),
        api_key: api_key.map(|s| s.to_string()),
        username: username.map(|s| s.to_string()),
        password: password.map(|s| s.to_string()),
        configured: true,
    };
    save_router_config(data_dir, &config).await?;
    Ok(config)
}

/// Get router info including detected type and capabilities.
pub async fn get_router_info(data_dir: &Path) -> Result<serde_json::Value> {
    let config = load_router_config(data_dir).await?;
    let upnp = check_upnp_available().await;
    Ok(serde_json::json!({
        "configured": config.configured,
        "router_type": config.router_type,
        "address": config.address,
        "upnp_available": upnp,
        "capabilities": match config.router_type {
            RouterType::OpenWrt => vec!["port_forwarding", "firewall_rules", "dns", "dhcp"],
            RouterType::PfSense | RouterType::OPNsense => vec!["port_forwarding", "firewall_rules", "dns", "vpn"],
            RouterType::UPnP => vec!["port_forwarding"],
            RouterType::Unknown => vec![],
        },
    }))
}
