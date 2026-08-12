use crate::Router;
use anyhow::Result;
use tracing::info;

pub struct WispConfig {
    pub ssid: String,
    pub password: String,
    pub encryption: String, // psk2 | psk | sae | none
    pub dhcp_start: u32,    // first address in DHCP pool (default 100 → .100)
    pub dhcp_limit: u32,    // pool size (default 150 → .100–.249)
    pub masq: bool,         // enable NAT on WAN zone (almost always true)
}

pub fn configure_wisp(router: &Router, config: &WispConfig) -> Result<()> {
    info!("[{}] Configuring WISP → ssid={}", router.host, config.ssid);

    let radio = detect_radio(router)?;

    // Ensure the radio is enabled (disabled=1 by default on fresh flash)
    router.uci_set("wireless.radio0.disabled", "0")?;

    // Create/update named sta wifi-iface "wwan" (idempotent: uci set creates if absent)
    router.uci_set("wireless.wwan", "wifi-iface")?;
    router.uci_set("wireless.wwan.device", &radio)?;
    router.uci_set("wireless.wwan.mode", "sta")?;
    router.uci_set("wireless.wwan.ssid", &config.ssid)?;
    router.uci_set("wireless.wwan.network", "wwan")?;
    router.uci_set("wireless.wwan.disabled", "0")?;
    router.uci_set("wireless.wwan.encryption", &config.encryption)?;
    if config.encryption != "none" && !config.password.is_empty() {
        router.uci_set("wireless.wwan.key", &config.password)?;
    }
    router.uci_commit(Some("wireless"))?;

    // Create/update wwan network interface (DHCP)
    router.uci_set("network.wwan", "interface")?;
    router.uci_set("network.wwan.proto", "dhcp")?;
    router.uci_commit(Some("network"))?;

    // Add wwan to the WAN firewall zone (walk zones by name)
    ensure_wwan_in_wan_zone(router)?;

    // Configure LAN DHCP pool
    router.uci_set("dhcp.lan.start", &config.dhcp_start.to_string())?;
    router.uci_set("dhcp.lan.limit", &config.dhcp_limit.to_string())?;
    router.uci_commit(Some("dhcp"))?;

    // Ensure masquerade on WAN zone so LAN clients reach the internet
    if config.masq {
        ensure_masq_on_wan_zone(router)?;
    }

    // Full wifi cycle so wpa_supplicant restarts cleanly with the new config.
    // "wifi reload" is not enough on some drivers — it keeps stale state.
    let (down_out, down_code) = router.run("wifi down 2>&1")?;
    if down_code != 0 {
        info!(
            "[{}] wifi down failed ({}): {}",
            router.host,
            down_code,
            down_out.trim()
        );
    }
    let (up_out, up_code) = router.run("wifi up 2>&1")?;
    if up_code != 0 {
        info!(
            "[{}] wifi up failed ({}): {} — falling back to network restart",
            router.host,
            up_code,
            up_out.trim()
        );
        router.run_ok("/etc/init.d/network restart 2>&1")?;
    }

    Ok(())
}

pub fn get_wan_status(router: &Router) -> serde_json::Value {
    let configured = router
        .uci_get("network.wwan.proto")
        .map(|v| v == "dhcp")
        .unwrap_or(false);

    let ssid = router.uci_get("wireless.wwan.ssid").unwrap_or_default();
    let encryption = router
        .uci_get("wireless.wwan.encryption")
        .unwrap_or_default();
    let radio0_disabled = router
        .uci_get("wireless.radio0.disabled")
        .map(|v| v == "1")
        .unwrap_or(false);

    // Find the active sta-mode interface and its association state
    let iw_out = router.run_ok("iw dev 2>/dev/null").unwrap_or_default();
    let (sta_iface, assoc_ssid) = parse_sta_iface(&iw_out);

    // Interface operstate (up / down / absent)
    let sta_state = if !sta_iface.is_empty() {
        router
            .run_ok(&format!(
                "cat /sys/class/net/{}/operstate 2>/dev/null",
                sta_iface
            ))
            .unwrap_or_else(|_| "unknown".into())
            .trim()
            .to_string()
    } else {
        "absent".to_string()
    };

    // Source IP for reaching 8.8.8.8 — empty if no default route yet
    let ip = router
        .run_ok("ip -4 route get 8.8.8.8 2>/dev/null | awk '{for(i=1;i<=NF;i++) if($i==\"src\"){print $(i+1); exit}}'")
        .unwrap_or_default()
        .trim()
        .to_string();

    // Recent wifi-related kernel/syslog lines for quick diagnosis
    let wifi_log = router
        .run_ok("logread 2>/dev/null | grep -iE 'wlan|wwan|wifi|assoc|deauth|auth fail|CTRL-EVENT|wpa_supplicant' | tail -8 2>/dev/null")
        .unwrap_or_default()
        .trim()
        .to_string();

    // LAN info for the DHCP setup display
    let lan_ip = router
        .uci_get("network.lan.ipaddr")
        .unwrap_or_else(|_| "192.168.1.1".into());
    let lan_netmask = router
        .uci_get("network.lan.netmask")
        .unwrap_or_else(|_| "255.255.255.0".into());
    let dhcp_start = router
        .uci_get("dhcp.lan.start")
        .unwrap_or_else(|_| "100".into());
    let dhcp_limit = router
        .uci_get("dhcp.lan.limit")
        .unwrap_or_else(|_| "150".into());

    // Masquerade: check WAN zone
    let masq = {
        let script = "for i in $(seq 0 9); do \
            n=$(uci get firewall.@zone[$i].name 2>/dev/null) || break; \
            if [ \"$n\" = \"wan\" ]; then \
                uci get firewall.@zone[$i].masq 2>/dev/null; break; \
            fi; done";
        router.run_ok(script).unwrap_or_default().trim() == "1"
    };

    info!("[{}] WAN status: configured={} ssid={:?} assoc={:?} sta_iface={:?} sta_state={:?} ip={:?} lan={} masq={}",
        router.host, configured, ssid, assoc_ssid, sta_iface, sta_state, ip, lan_ip, masq);
    if !wifi_log.is_empty() {
        info!(
            "[{}] wifi_log: {}",
            router.host,
            wifi_log.replace('\n', " | ")
        );
    }

    serde_json::json!({
        "configured":      configured,
        "ssid":            ssid,
        "assoc_ssid":      assoc_ssid,
        "encryption":      encryption,
        "ip":              ip,
        "internet":        !ip.is_empty(),
        "radio0_disabled": radio0_disabled,
        "sta_iface":       sta_iface,
        "sta_state":       sta_state,
        "wifi_log":        wifi_log,
        "lan_ip":          lan_ip,
        "lan_netmask":     lan_netmask,
        "dhcp_start":      dhcp_start,
        "dhcp_limit":      dhcp_limit,
        "masq":            masq,
    })
}

fn parse_sta_iface(iw_out: &str) -> (String, String) {
    let mut result_iface = String::new();
    let mut result_ssid = String::new();
    let mut current_iface = String::new();
    let mut current_type = String::new();
    let mut current_ssid = String::new();

    for line in iw_out.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("Interface ") {
            // Save previous interface if it was a sta
            if current_type == "managed" && result_iface.is_empty() {
                result_iface = current_iface.clone();
                result_ssid = current_ssid.clone();
            }
            current_iface = name.trim().to_string();
            current_type.clear();
            current_ssid.clear();
        } else if let Some(t) = line.strip_prefix("type ") {
            current_type = t.trim().to_string();
        } else if let Some(s) = line.strip_prefix("ssid ") {
            current_ssid = s.trim().to_string();
        }
    }
    // Handle last block
    if current_type == "managed" && result_iface.is_empty() {
        result_iface = current_iface;
        result_ssid = current_ssid;
    }

    (result_iface, result_ssid)
}

fn detect_radio(router: &Router) -> Result<String> {
    // radio0 is universal; verify it exists
    let out = router.uci_get("wireless.radio0").unwrap_or_default();
    if !out.is_empty() {
        return Ok("radio0".to_string());
    }
    anyhow::bail!("No wireless radio (radio0) found in UCI config")
}

fn ensure_masq_on_wan_zone(router: &Router) -> Result<()> {
    let script = "for i in $(seq 0 9); do \
        name=$(uci get firewall.@zone[$i].name 2>/dev/null) || break; \
        if [ \"$name\" = \"wan\" ]; then \
            uci set firewall.@zone[$i].masq=1 2>/dev/null; \
            uci commit firewall; \
            break; \
        fi; \
    done; echo ok";
    router.run_ok(script)?;
    Ok(())
}

fn ensure_wwan_in_wan_zone(router: &Router) -> Result<()> {
    // Walk zones 0-9, find the one named "wan", add wwan to its network list
    let script = "for i in $(seq 0 9); do \
        name=$(uci get firewall.@zone[$i].name 2>/dev/null) || break; \
        if [ \"$name\" = \"wan\" ]; then \
            uci add_list firewall.@zone[$i].network=wwan 2>/dev/null; \
            uci commit firewall; \
            break; \
        fi; \
    done; echo ok";
    router.run_ok(script)?;
    Ok(())
}
