use super::RpcHandler;
use crate::network::dns;
use anyhow::{Context, Result};
use tracing::debug;

impl RpcHandler {
    /// network.list-interfaces — list all network interfaces with IP, MAC, status.
    pub(super) async fn handle_network_list_interfaces(&self) -> Result<serde_json::Value> {
        debug!("Listing network interfaces");
        let interfaces = list_interfaces().await?;
        Ok(serde_json::json!({ "interfaces": interfaces }))
    }

    /// network.upstream-wifi — describe the node's own Wi-Fi uplink so the
    /// router WAN step can offer to reuse it.
    ///
    /// Deliberately returns `has_password`, never the PSK: the browser must
    /// not receive a network secret it does not need. The backend resolves the
    /// secret server-side only when the operator opts in.
    pub(super) async fn handle_network_upstream_wifi(&self) -> Result<serde_json::Value> {
        debug!("Describing the node's upstream Wi-Fi");
        upstream_wifi_info().await
    }

    /// network.scan-wifi — scan for available WiFi networks.
    pub(super) async fn handle_network_scan_wifi(&self) -> Result<serde_json::Value> {
        debug!("Scanning WiFi networks");
        let networks = scan_wifi().await?;
        Ok(serde_json::json!({ "networks": networks }))
    }

    /// network.set-wifi-radio — turn the wifi adapter fully on or off (not just
    /// disconnect from a network). Params: `{ "enabled": bool }`.
    pub(super) async fn handle_network_set_wifi_radio(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let enabled = params
            .get("enabled")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: enabled"))?;

        tracing::info!(enabled, "Setting wifi radio state");
        set_wifi_radio(enabled).await?;

        Ok(serde_json::json!({ "ok": true, "enabled": enabled }))
    }

    /// network.configure-wifi — connect to a WiFi network.
    pub(super) async fn handle_network_configure_wifi(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let ssid = params
            .get("ssid")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: ssid"))?;
        let password = params
            .get("password")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Validate SSID (prevent command injection)
        if ssid.len() > 64 || ssid.contains('\0') {
            anyhow::bail!("Invalid SSID");
        }
        // Validate WiFi password
        if password.len() > 63 || password.contains('\0') {
            anyhow::bail!("Invalid WiFi password (max 63 chars, no null bytes)");
        }

        tracing::info!("Connecting to WiFi network: {}", ssid);
        connect_wifi(ssid, password).await?;

        Ok(serde_json::json!({ "ok": true, "ssid": ssid }))
    }

    /// network.configure-ethernet — set DHCP or static IP for an ethernet interface.
    pub(super) async fn handle_network_configure_ethernet(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let interface = params
            .get("interface")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: interface"))?;
        let mode = params
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("dhcp");

        // Validate interface name (alphanumeric + digits only)
        if !interface
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            anyhow::bail!("Invalid interface name");
        }

        match mode {
            "dhcp" => {
                tracing::info!("Setting {} to DHCP", interface);
                configure_ethernet_dhcp(interface).await?;
            }
            "static" => {
                let ip = params.get("ip").and_then(|v| v.as_str()).ok_or_else(|| {
                    anyhow::anyhow!("Missing required parameter: ip for static mode")
                })?;
                let gateway = params.get("gateway").and_then(|v| v.as_str()).unwrap_or("");
                let dns = params
                    .get("dns")
                    .and_then(|v| v.as_str())
                    .unwrap_or("1.1.1.1");

                // Validate IP: must parse as IP or CIDR
                let ip_part = ip.split('/').next().unwrap_or("");
                if ip_part.parse::<std::net::IpAddr>().is_err() {
                    anyhow::bail!("Invalid IP address format");
                }

                // Validate gateway if provided
                if !gateway.is_empty() && gateway.parse::<std::net::IpAddr>().is_err() {
                    anyhow::bail!("Invalid gateway IP address");
                }

                // Validate DNS server IP
                if dns.parse::<std::net::IpAddr>().is_err() {
                    anyhow::bail!("Invalid DNS server IP address");
                }

                tracing::info!("Setting {} to static IP {}", interface, ip);
                configure_ethernet_static(interface, ip, gateway, dns).await?;
            }
            _ => anyhow::bail!("Invalid mode: {}. Use 'dhcp' or 'static'", mode),
        }

        Ok(serde_json::json!({ "ok": true, "interface": interface, "mode": mode }))
    }

    /// network.dns-status — get current DNS configuration and status.
    pub(super) async fn handle_network_dns_status(&self) -> Result<serde_json::Value> {
        debug!("Getting DNS status");
        let status = dns::get_status(&self.config.data_dir).await?;
        Ok(serde_json::to_value(status)?)
    }

    /// network.configure-dns — configure DNS servers and provider.
    pub(super) async fn handle_network_configure_dns(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let params = params.ok_or_else(|| anyhow::anyhow!("Missing params"))?;
        let provider_str = params
            .get("provider")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: provider"))?;

        let provider = match provider_str {
            "system" => dns::DnsProvider::System,
            "cloudflare" => dns::DnsProvider::Cloudflare,
            "google" => dns::DnsProvider::Google,
            "quad9" => dns::DnsProvider::Quad9,
            "mullvad" => dns::DnsProvider::Mullvad,
            "custom" => dns::DnsProvider::Custom,
            other => anyhow::bail!(
                "Unknown DNS provider: {}. Use: system, cloudflare, google, quad9, mullvad, custom",
                other
            ),
        };

        let custom_servers: Vec<String> = if provider == dns::DnsProvider::Custom {
            params
                .get("servers")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        if provider == dns::DnsProvider::Custom && custom_servers.is_empty() {
            anyhow::bail!("Custom provider requires at least one DNS server in 'servers' array");
        }

        // Validate custom server IPs
        for s in &custom_servers {
            if s.parse::<std::net::IpAddr>().is_err() {
                anyhow::bail!("Invalid DNS server IP: {}", s);
            }
        }

        tracing::info!(provider = provider_str, "Configuring DNS");
        let config = dns::configure(&self.config.data_dir, provider, custom_servers).await?;

        Ok(serde_json::json!({
            "ok": true,
            "provider": config.provider.to_string(),
            "servers": config.servers,
            "doh_enabled": config.doh_enabled,
            "doh_url": config.doh_url,
        }))
    }
}

/// List network interfaces using `ip -j addr show`.
async fn list_interfaces() -> Result<Vec<serde_json::Value>> {
    let output = tokio::process::Command::new("ip")
        .args(["-j", "addr", "show"])
        .output()
        .await
        .context("Failed to run `ip addr show`")?;

    if !output.status.success() {
        anyhow::bail!(
            "ip addr show failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let raw: Vec<serde_json::Value> =
        serde_json::from_slice(&output.stdout).context("Failed to parse ip JSON output")?;

    let interfaces: Vec<serde_json::Value> = raw
        .into_iter()
        .filter_map(|iface| {
            let name = iface.get("ifname")?.as_str()?;
            // Skip loopback
            if name == "lo" {
                return None;
            }
            let operstate = iface
                .get("operstate")
                .and_then(|v| v.as_str())
                .unwrap_or("UNKNOWN");
            let mac = iface.get("address").and_then(|v| v.as_str()).unwrap_or("");

            // Get IPv4 addresses
            let addrs: Vec<String> = iface
                .get("addr_info")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter(|a| a.get("family").and_then(|f| f.as_str()) == Some("inet"))
                        .filter_map(|a| {
                            let local = a.get("local")?.as_str()?;
                            let prefix = a.get("prefixlen")?.as_u64()?;
                            Some(format!("{}/{}", local, prefix))
                        })
                        .collect()
                })
                .unwrap_or_default();

            let iface_type = if name.starts_with("wl") {
                "wifi"
            } else if name.starts_with("en") || name.starts_with("eth") {
                "ethernet"
            } else if name.starts_with("veth")
                || name.starts_with("br-")
                || name.starts_with("docker")
                || name.starts_with("podman")
            {
                "virtual"
            } else {
                "other"
            };

            Some(serde_json::json!({
                "name": name,
                "type": iface_type,
                "state": operstate.to_lowercase(),
                "mac": mac,
                "ipv4": addrs,
            }))
        })
        .collect();

    Ok(interfaces)
}

/// Scan WiFi networks using `nmcli -t -f SSID,SIGNAL,SECURITY device wifi list`.
/// The node's active Wi-Fi uplink, or a `configured: false` result when the
/// node is wired (or has no active Wi-Fi profile).
async fn upstream_wifi_info() -> Result<serde_json::Value> {
    let Some((profile, device)) = active_wifi_profile().await else {
        return Ok(serde_json::json!({
            "configured": false,
            "reason": "no active Wi-Fi uplink",
        }));
    };
    let ssid = nmcli_get(&profile, "802-11-wireless.ssid")
        .await
        .unwrap_or_default();
    let security = nmcli_get(&profile, "802-11-wireless-security.key-mgmt")
        .await
        .unwrap_or_default();
    let has_password = nmcli_get_secret(&profile, "802-11-wireless-security.psk")
        .await
        .map(|psk| !psk.is_empty())
        .unwrap_or(false);

    Ok(serde_json::json!({
        "configured": true,
        "source": "active",
        "profile": profile,
        "device": device,
        "ssid": ssid,
        "band": wifi_band(&device).await.unwrap_or_default(),
        "security": security,
        "has_password": has_password,
    }))
}

/// Resolve the active Wi-Fi profile's SSID and PSK, server-side. Used only by
/// the router WAN setup when the operator chooses "use this node's
/// credentials"; the PSK never leaves the backend.
pub(super) async fn upstream_wifi_psk() -> Result<Option<(String, String)>> {
    let Some((profile, _device)) = active_wifi_profile().await else {
        return Ok(None);
    };
    let ssid = nmcli_get(&profile, "802-11-wireless.ssid")
        .await
        .unwrap_or_default();
    let psk = nmcli_get_secret(&profile, "802-11-wireless-security.psk")
        .await
        .unwrap_or_default();
    if ssid.is_empty() || psk.is_empty() {
        return Ok(None);
    }
    Ok(Some((ssid, psk)))
}

/// Find the active 802-11-wireless NetworkManager profile: `(profile, device)`.
async fn active_wifi_profile() -> Option<(String, String)> {
    let out = tokio::process::Command::new("nmcli")
        .args(["-t", "-f", "NAME,TYPE,DEVICE,ACTIVE", "connection", "show"])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_active_wifi_connection(&String::from_utf8_lossy(&out.stdout))
}

/// Pick the active 802-11-wireless row out of
/// `nmcli -t -f NAME,TYPE,DEVICE,ACTIVE connection show` output. Pure, so the
/// escaped-colon shape is testable without NetworkManager.
fn parse_active_wifi_connection(text: &str) -> Option<(String, String)> {
    for line in text.lines() {
        let fields = split_nmcli_escaped(line, 4);
        if fields.len() < 4 {
            continue;
        }
        if fields[1] == "802-11-wireless" && fields[3].eq_ignore_ascii_case("yes") {
            return Some((fields[0].clone(), fields[2].clone()));
        }
    }
    None
}

/// One non-secret NM property. Absent/unreadable values are `None`, never a
/// hard failure: a node without NetworkManager must still render the manual
/// WAN form.
async fn nmcli_get(profile: &str, key: &str) -> Option<String> {
    let out = tokio::process::Command::new("nmcli")
        .args(["-g", key, "connection", "show", profile])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// A secret NM property (`--show-secrets`). Requires the same NetworkManager
/// polkit grant the node already uses for Wi-Fi setup; a refusal is `None`,
/// and the caller falls back to manual entry rather than failing the RPC.
async fn nmcli_get_secret(profile: &str, key: &str) -> Option<String> {
    let out = tokio::process::Command::new("nmcli")
        .args(["-s", "-g", key, "connection", "show", profile])
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The band of the AP the device is associated with, from `iw link`. Empty
/// when unknown; the caller treats that as "let the router's scan decide".
async fn wifi_band(device: &str) -> Option<String> {
    let out = tokio::process::Command::new("iw")
        .args(["dev", device, "link"])
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let freq = text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("freq:")
            .and_then(|v| v.trim().parse::<u32>().ok())
    })?;
    Some(
        match freq {
            2400..=2500 => "2.4",
            5000..=5900 => "5",
            5925..=7125 => "6",
            _ => return None,
        }
        .to_string(),
    )
}

async fn scan_wifi() -> Result<Vec<serde_json::Value>> {
    // Trigger a rescan first
    let _ = tokio::process::Command::new("nmcli")
        .args(["device", "wifi", "rescan"])
        .output()
        .await;

    // Short delay for scan to complete
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let output = tokio::process::Command::new("nmcli")
        .args(["-t", "-f", "SSID,SIGNAL,SECURITY", "device", "wifi", "list"])
        .output()
        .await
        .context("Failed to run nmcli wifi list")?;

    if !output.status.success() {
        anyhow::bail!(
            "nmcli wifi list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8(output.stdout).context("nmcli output not utf8")?;
    let mut seen = std::collections::HashSet::new();
    let networks: Vec<serde_json::Value> = stdout
        .lines()
        .filter_map(|line| {
            let parts = split_nmcli_escaped(line, 3);
            if parts.len() < 3 {
                return None;
            }
            let ssid = parts[0].trim();
            if ssid.is_empty() || !seen.insert(ssid.to_string()) {
                return None;
            }
            let signal: u32 = parts[1].parse().unwrap_or(0);
            let security = parts[2].trim();
            Some(serde_json::json!({
                "ssid": ssid,
                "signal": signal,
                "security": security,
            }))
        })
        .collect();

    Ok(networks)
}

fn split_nmcli_escaped(line: &str, limit: usize) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                current.push(next);
            }
        } else if ch == ':' && fields.len() + 1 < limit {
            fields.push(current);
            current = String::new();
        } else {
            current.push(ch);
        }
    }

    fields.push(current);
    fields
}

/// Turn the wifi radio fully on or off using nmcli (a rfkill-level toggle, not
/// just disconnecting from the current network — the adapter stops scanning/
/// associating entirely until switched back on).
async fn set_wifi_radio(enabled: bool) -> Result<()> {
    let state = if enabled { "on" } else { "off" };
    let output = tokio::process::Command::new("nmcli")
        .args(["radio", "wifi", state])
        .output()
        .await
        .context("Failed to run nmcli radio wifi")?;

    if !output.status.success() {
        anyhow::bail!(
            "nmcli radio wifi {} failed: {}",
            state,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

/// Connect to a WiFi network using nmcli.
async fn connect_wifi(ssid: &str, password: &str) -> Result<()> {
    let conn_name = format!("archipelago-wifi-{ssid}");

    // Delete prior profiles for this SSID/name first. Failed attempts can leave
    // a profile with key-mgmt but no saved PSK, causing future retries to fail
    // with "no secrets" before the supplied password is used.
    let _ = tokio::process::Command::new("nmcli")
        .args(["connection", "delete", &conn_name])
        .output()
        .await;
    let _ = tokio::process::Command::new("nmcli")
        .args(["connection", "delete", ssid])
        .output()
        .await;

    let mut args = vec![
        "connection",
        "add",
        "type",
        "wifi",
        "con-name",
        &conn_name,
        "ifname",
        "*",
        "ssid",
        ssid,
        "ipv4.method",
        "auto",
        "ipv6.method",
        "auto",
    ];
    if !password.is_empty() {
        args.extend(["wifi-sec.key-mgmt", "wpa-psk", "wifi-sec.psk", password]);
    }

    let output = tokio::process::Command::new("nmcli")
        .args(args)
        .output()
        .await
        .context("Failed to run nmcli wifi profile create")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("WiFi profile create failed: {}", stderr);
    }

    let activate = tokio::process::Command::new("nmcli")
        .args(["connection", "up", &conn_name])
        .output()
        .await
        .context("Failed to run nmcli wifi connect")?;

    if !activate.status.success() {
        let stderr = String::from_utf8_lossy(&activate.stderr);
        let _ = tokio::process::Command::new("nmcli")
            .args(["connection", "delete", &conn_name])
            .output()
            .await;
        anyhow::bail!("WiFi connection failed: {}", stderr);
    }

    Ok(())
}

/// Configure ethernet interface for DHCP using nmcli.
async fn configure_ethernet_dhcp(interface: &str) -> Result<()> {
    // Find or create a connection for this interface
    let conn_name = format!("archipelago-{}", interface);

    // Delete existing connection if any
    let _ = tokio::process::Command::new("nmcli")
        .args(["connection", "delete", &conn_name])
        .output()
        .await;

    // Create new DHCP connection
    let output = tokio::process::Command::new("nmcli")
        .args([
            "connection",
            "add",
            "type",
            "ethernet",
            "con-name",
            &conn_name,
            "ifname",
            interface,
            "ipv4.method",
            "auto",
        ])
        .output()
        .await
        .context("Failed to create DHCP connection")?;

    if !output.status.success() {
        anyhow::bail!(
            "nmcli connection add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Activate the connection
    let activate = tokio::process::Command::new("nmcli")
        .args(["connection", "up", &conn_name])
        .output()
        .await
        .context("Failed to activate connection")?;

    if !activate.status.success() {
        anyhow::bail!(
            "nmcli connection up failed: {}",
            String::from_utf8_lossy(&activate.stderr)
        );
    }

    Ok(())
}

/// Configure ethernet interface with a static IP.
async fn configure_ethernet_static(
    interface: &str,
    ip: &str,
    gateway: &str,
    dns: &str,
) -> Result<()> {
    let conn_name = format!("archipelago-{}", interface);

    // Delete existing connection if any
    let _ = tokio::process::Command::new("nmcli")
        .args(["connection", "delete", &conn_name])
        .output()
        .await;

    let mut args = vec![
        "connection",
        "add",
        "type",
        "ethernet",
        "con-name",
        &conn_name,
        "ifname",
        interface,
        "ipv4.method",
        "manual",
        "ipv4.addresses",
        ip,
    ];

    if !gateway.is_empty() {
        args.push("ipv4.gateway");
        args.push(gateway);
    }

    args.push("ipv4.dns");
    args.push(dns);

    let output = tokio::process::Command::new("nmcli")
        .args(&args)
        .output()
        .await
        .context("Failed to create static connection")?;

    if !output.status.success() {
        anyhow::bail!(
            "nmcli connection add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let activate = tokio::process::Command::new("nmcli")
        .args(["connection", "up", &conn_name])
        .output()
        .await
        .context("Failed to activate connection")?;

    if !activate.status.success() {
        anyhow::bail!(
            "nmcli connection up failed: {}",
            String::from_utf8_lossy(&activate.stderr)
        );
    }

    Ok(())
}

#[cfg(test)]
mod upstream_wifi_tests {
    use super::parse_active_wifi_connection;

    #[test]
    fn picks_the_active_wifi_row() {
        let out = "Wired connection 1:802-3-ethernet:eth0:yes\n\
                   archipelago-wifi-venue:802-11-wireless:wlp3s0:yes\n";
        assert_eq!(
            parse_active_wifi_connection(out),
            Some(("archipelago-wifi-venue".to_string(), "wlp3s0".to_string()))
        );
    }

    #[test]
    fn ignores_inactive_wifi_and_wired_only_nodes() {
        assert_eq!(
            parse_active_wifi_connection(
                "home:802-11-wireless:wlp3s0:no\nWired:802-3-ethernet:eth0:yes\n"
            ),
            None
        );
    }

    #[test]
    fn handles_escaped_colons_in_the_profile_name() {
        let out = "archipelago-wifi-venue\\:guest:802-11-wireless:wlp3s0:yes\n";
        assert_eq!(
            parse_active_wifi_connection(out),
            Some((
                "archipelago-wifi-venue:guest".to_string(),
                "wlp3s0".to_string()
            ))
        );
    }

    #[test]
    fn accepts_an_uppercase_active_flag() {
        let out = "home:802-11-wireless:wlp3s0:YES\n";
        assert!(parse_active_wifi_connection(out).is_some());
    }
}
