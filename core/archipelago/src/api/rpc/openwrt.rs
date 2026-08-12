use super::RpcHandler;
use crate::network::router as net_router;
use anyhow::Result;
use archipelago_openwrt::{
    detect,
    router::Router,
    tollgate::{self, TollGateConfig},
    wan, wifi_scan,
};

/// Default port for the local Cashu mint (nutshell / cashu-mint app).
const LOCAL_MINT_PORT: u16 = 3338;

impl RpcHandler {
    /// Scan the local subnet for OpenWrt routers.
    ///
    /// Params: `{ "subnet": "192.168.1.0", "prefix": 24,
    ///             "ssh_user": "root", "ssh_password": "" }`
    pub(super) async fn handle_openwrt_scan(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let p = params.unwrap_or_default();
        let subnet: [u8; 4] = parse_ipv4(
            p.get("subnet")
                .and_then(|v| v.as_str())
                .unwrap_or("192.168.1.0"),
        )?;
        let prefix = p.get("prefix").and_then(|v| v.as_u64()).unwrap_or(24) as u8;
        let ssh_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .unwrap_or("root")
            .to_string();
        let ssh_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let routers = detect::scan_subnet(subnet, prefix, &ssh_user, &ssh_password).await;
        let ips: Vec<String> = routers.iter().map(|ip| ip.to_string()).collect();

        Ok(serde_json::json!({ "routers": ips }))
    }

    /// Read current settings from a saved or ad-hoc OpenWrt router via SSH/UCI.
    ///
    /// Params (all optional): `{ "host": "...", "ssh_user": "root", "ssh_password": "" }`
    /// If params are omitted the saved `router_config.json` credentials are used.
    pub(super) async fn handle_openwrt_get_status(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let p = params.unwrap_or_default();
        let host_from_params = p.get("host").and_then(|v| v.as_str()).is_some();

        let host = p
            .get("host")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                if saved.configured {
                    Some(saved.address.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No router configured — provide host or call router.configure first"
                )
            })?;

        let ssh_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.username.clone())
            .unwrap_or_else(|| "root".to_string());

        let ssh_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.password.clone())
            .unwrap_or_default();

        let router = Router::connect_password(&host, 22, &ssh_user, &ssh_password)?;
        router.verify_openwrt()?;

        // Persist the connection so other views (e.g. the Home dashboard's
        // Network tile) can poll `openwrt.get-status` with no params instead
        // of every caller needing to carry host/credentials around. Only do
        // this when the host actually came from params — otherwise every
        // no-args poll would re-save the same thing it just read.
        if host_from_params {
            let _ = net_router::configure_router(
                &self.config.data_dir,
                net_router::RouterType::OpenWrt,
                &host,
                None,
                Some(&ssh_user),
                Some(&ssh_password),
            )
            .await;
        }

        // System info
        let release = router
            .run_ok("cat /etc/openwrt_release")
            .unwrap_or_default();
        let hostname = router
            .uci_get("system.@system[0].hostname")
            .unwrap_or_else(|_| "unknown".into());
        let uptime_secs: u64 = router
            .run_ok("cat /proc/uptime")
            .unwrap_or_default()
            .split_whitespace()
            .next()
            .and_then(|s| s.split('.').next())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // TollGate — check via opkg (≤24.x) or binary presence (25.x apk-native).
        // The service binary is /usr/bin/tollgate-wrt (per its init.d script),
        // not /usr/bin/tollgate-module-basic-go — that's only the opkg/apk
        // *package* name, never an on-disk filename.
        let tollgate_installed = router
            .run("/usr/bin/opkg list-installed 2>/dev/null | grep -q '^tollgate-module-basic-go ' || \
                  test -f /usr/bin/tollgate-wrt 2>/dev/null")
            .map(|(_, code)| code == 0)
            .unwrap_or(false);

        let tollgate = if tollgate_installed {
            serde_json::json!({
                "installed": true,
                "enabled":       router.uci_get("tollgate.main.enabled").map(|v| v == "1").unwrap_or(false),
                "metric":        router.uci_get("tollgate.main.metric").unwrap_or_default(),
                "step_size_ms":  router.uci_get("tollgate.main.step_size").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0),
                "price_per_step":router.uci_get("tollgate.main.price_per_step").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0),
                "min_steps":     router.uci_get("tollgate.main.min_steps").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(1),
                "currency":      router.uci_get("tollgate.main.currency").unwrap_or_default(),
                "mint_url":      router.uci_get("tollgate.main.mint_url").unwrap_or_default(),
            })
        } else {
            serde_json::json!({ "installed": false })
        };

        // WiFi interfaces
        let wifi_raw = router.run_ok("uci show wireless").unwrap_or_default();
        let wifi_interfaces = parse_wifi_interfaces(&wifi_raw);

        let wan_status = wan::get_wan_status(&router);

        Ok(serde_json::json!({
            "host": host,
            "hostname": hostname,
            "uptime_secs": uptime_secs,
            "release": parse_release(&release),
            "tollgate": tollgate,
            "wifi_interfaces": wifi_interfaces,
            "wan": wan_status,
        }))
    }

    /// Provision TollGate on an OpenWrt router and create the "archipelago" SSID.
    ///
    /// Params: `{ "host": "192.168.1.1", "ssh_user": "root", "ssh_password": "",
    ///             "price_sats": 10, "step_size_ms": 60000, "min_steps": 1,
    ///             "mint_url": "<optional override>" }`
    ///
    /// `mint_url` defaults to `http://<this node's IP>:3338` — the local Cashu
    /// mint that must be running as an Archy app before calling this endpoint.
    pub(super) async fn handle_openwrt_provision_tollgate(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let p = params.unwrap_or_default();

        let host = p
            .get("host")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                if saved.configured {
                    Some(saved.address.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No router configured — provide host or call router.configure first"
                )
            })?;
        let ssh_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.username.clone())
            .unwrap_or_else(|| "root".to_string());
        let ssh_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.password.clone())
            .unwrap_or_default();

        let default_mint_url = format!("http://{}:{}", self.config.host_ip, LOCAL_MINT_PORT);
        let mint_url = p
            .get("mint_url")
            .and_then(|v| v.as_str())
            .unwrap_or(&default_mint_url)
            .to_string();

        let config = TollGateConfig {
            ssid: "archipelago".to_string(),
            mint_url,
            price_sats: p.get("price_sats").and_then(|v| v.as_u64()).unwrap_or(10),
            step_size_ms: p
                .get("step_size_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(60_000),
            min_steps: p.get("min_steps").and_then(|v| v.as_u64()).unwrap_or(1) as u32,
            enabled: p.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        };

        let router = Router::connect_password(&host, 22, &ssh_user, &ssh_password)?;
        router.verify_openwrt()?;
        tollgate::provision(&router, &config).await?;

        Ok(serde_json::json!({
            "ok": true,
            "host": host,
            "ssid": config.ssid,
            "mint_url": config.mint_url,
        }))
    }

    /// Scan for visible WiFi networks from the router's radio.
    ///
    /// Params: same host/credentials as other openwrt methods.
    pub(super) async fn handle_openwrt_scan_wifi(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let p = params.unwrap_or_default();

        let host = p
            .get("host")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                if saved.configured {
                    Some(saved.address.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No router configured — provide host or call router.configure first"
                )
            })?;
        let ssh_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.username.clone())
            .unwrap_or_else(|| "root".to_string());
        let ssh_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.password.clone())
            .unwrap_or_default();

        let router = Router::connect_password(&host, 22, &ssh_user, &ssh_password)?;
        router.verify_openwrt()?;

        let networks = wifi_scan::scan_networks(&router)?;
        let result: Vec<serde_json::Value> = networks
            .iter()
            .map(|n| {
                serde_json::json!({
                    "ssid":       n.ssid,
                    "bssid":      n.bssid,
                    "signal":     n.signal,
                    "channel":    n.channel,
                    "encryption": n.encryption,
                })
            })
            .collect();

        Ok(serde_json::json!({ "networks": result }))
    }

    /// Configure WAN/WISP — connect the router to an upstream WiFi network.
    ///
    /// Params: host/credentials + `{ "ssid": "...", "password": "...", "encryption": "psk2" }`
    pub(super) async fn handle_openwrt_configure_wan(
        &self,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let saved = net_router::load_router_config(&self.config.data_dir).await?;
        let p = params.unwrap_or_default();

        let host = p
            .get("host")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                if saved.configured {
                    Some(saved.address.clone())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No router configured — provide host or call router.configure first"
                )
            })?;
        let ssh_user = p
            .get("ssh_user")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.username.clone())
            .unwrap_or_else(|| "root".to_string());
        let ssh_password = p
            .get("ssh_password")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| saved.password.clone())
            .unwrap_or_default();

        let ssid = p
            .get("ssid")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required field: ssid"))?
            .to_string();
        let password = p
            .get("password")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let encryption = p
            .get("encryption")
            .and_then(|v| v.as_str())
            .unwrap_or("psk2")
            .to_string();
        let dhcp_start = p.get("dhcp_start").and_then(|v| v.as_u64()).unwrap_or(100) as u32;
        let dhcp_limit = p.get("dhcp_limit").and_then(|v| v.as_u64()).unwrap_or(150) as u32;
        let masq = p.get("masq").and_then(|v| v.as_bool()).unwrap_or(true);

        let router = Router::connect_password(&host, 22, &ssh_user, &ssh_password)?;
        router.verify_openwrt()?;

        let config = wan::WispConfig {
            ssid: ssid.clone(),
            password,
            encryption,
            dhcp_start,
            dhcp_limit,
            masq,
        };
        wan::configure_wisp(&router, &config)?;

        Ok(serde_json::json!({ "ok": true, "host": host, "ssid": ssid }))
    }
}

/// Parse /etc/openwrt_release key=value pairs into a JSON object.
fn parse_release(raw: &str) -> serde_json::Value {
    let mut m = serde_json::Map::new();
    for line in raw.lines() {
        if let Some((k, v)) = line.split_once('=') {
            m.insert(
                k.to_lowercase(),
                serde_json::Value::String(v.trim_matches('"').to_string()),
            );
        }
    }
    serde_json::Value::Object(m)
}

/// Extract AP wifi-iface sections from `uci show wireless` output.
fn parse_wifi_interfaces(raw: &str) -> Vec<serde_json::Value> {
    use std::collections::HashMap;
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();

    for line in raw.lines() {
        if let Some((lhs, rhs)) = line.trim().split_once('=') {
            let parts: Vec<&str> = lhs.splitn(3, '.').collect();
            if parts.len() == 3 && parts[0] == "wireless" {
                sections
                    .entry(parts[1].to_string())
                    .or_default()
                    .insert(parts[2].to_string(), rhs.trim_matches('\'').to_string());
            }
        }
    }

    let mut ifaces: Vec<serde_json::Value> = sections
        .into_iter()
        .filter(|(_, f)| f.get("mode").map(|m| m == "ap").unwrap_or(false))
        .map(|(name, f)| {
            serde_json::json!({
                "section":    name,
                "ssid":       f.get("ssid").cloned().unwrap_or_default(),
                "device":     f.get("device").cloned().unwrap_or_default(),
                "encryption": f.get("encryption").cloned().unwrap_or_else(|| "none".into()),
                "network":    f.get("network").cloned().unwrap_or_default(),
                "disabled":   f.get("disabled").map(|v| v == "1").unwrap_or(false),
            })
        })
        .collect();

    ifaces.sort_by_key(|v| v["section"].as_str().unwrap_or("").to_string());
    ifaces
}

fn parse_ipv4(s: &str) -> Result<[u8; 4]> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        anyhow::bail!("Invalid IPv4: {}", s);
    }
    Ok([
        parts[0].parse()?,
        parts[1].parse()?,
        parts[2].parse()?,
        parts[3].parse()?,
    ])
}
