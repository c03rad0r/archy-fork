use crate::Router;
use anyhow::Result;
use tracing::info;

pub struct WispConfig {
    pub ssid: String,
    pub password: String,
    /// Raw scan label ("wpa-psk", "WPA2", "SAE", ...). Normalized before it is
    /// written to UCI.
    pub encryption: String,
    /// Preferred upstream band: "2.4", "5", "6", or None to try every radio.
    pub band: Option<String>,
    pub dhcp_start: u32, // first address in DHCP pool (default 100 → .100)
    pub dhcp_limit: u32, // pool size (default 150 → .100–.249)
    pub masq: bool,      // enable NAT on WAN zone (almost always true)
}

/// Map a scan label onto an OpenWrt wifi-iface `encryption` value.
///
/// Writing the raw scan label was why a `wpa-psk` result produced an interface
/// that never associated: netifd expects `psk2` / `sae` / `none`, not the
/// human-readable label.
pub fn normalize_encryption(raw: &str) -> String {
    let value = raw.trim().to_ascii_lowercase();
    if value.is_empty() || value == "none" || value.contains("open") {
        return "none".to_string();
    }
    if value.contains("mixed") {
        return if value.contains("sae") || value.contains("wpa3") {
            "sae-mixed".to_string()
        } else {
            "psk-mixed".to_string()
        };
    }
    if value.contains("sae") || value.contains("wpa3") {
        return "sae".to_string();
    }
    "psk2".to_string()
}

fn normalize_band(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "2g" | "bg" | "11g" | "2.4" | "2.4g" | "2.4ghz" => "2.4".to_string(),
        "5g" | "a" | "11a" | "5" | "5ghz" => "5".to_string(),
        "6g" | "11ax6g" | "6" | "6ghz" => "6".to_string(),
        _ => String::new(),
    }
}

fn radio_band(router: &Router, radio: &str) -> String {
    let mut raw = router
        .uci_get(&format!("wireless.{radio}.band"))
        .unwrap_or_default();
    if raw.trim().is_empty() {
        raw = router
            .uci_get(&format!("wireless.{radio}.hwmode"))
            .unwrap_or_default();
    }
    normalize_band(&raw)
}

/// Every `wifi-device` section, with radios on the requested band first.
fn radio_candidates(router: &Router, band: Option<&str>) -> Result<Vec<String>> {
    let out = router.run_ok("uci -q show wireless 2>/dev/null")?;
    let mut radios = Vec::new();
    for line in out.lines() {
        let Some(rest) = line.strip_prefix("wireless.") else {
            continue;
        };
        let Some(name) = rest.strip_suffix("=wifi-device") else {
            continue;
        };
        if !name.contains('.') {
            radios.push(name.to_string());
        }
    }
    if radios.is_empty() {
        anyhow::bail!("no wifi-device (radio) sections found in /etc/config/wireless");
    }

    let want = band.map(normalize_band).unwrap_or_default();
    if !want.is_empty() {
        let (matching, rest): (Vec<_>, Vec<_>) = radios
            .into_iter()
            .partition(|radio| radio_band(router, radio) == want);
        radios = matching.into_iter().chain(rest).collect();
    }
    Ok(radios)
}

/// Write the STA interface for one radio. `wireless.wwan` is reused for every
/// attempt, so a failed attempt is replaced rather than accumulating STAs.
fn apply_sta(router: &Router, radio: &str, config: &WispConfig) -> Result<()> {
    // A second STA on the same radio fights wpa_supplicant for the single
    // channel. Leave exactly one: our `wwan` section.
    let _ = router.run_ok(&format!(
        "for s in $(uci -q show wireless 2>/dev/null | sed -n \"s/^wireless\\.\\([^.]*\\)\\.device='{radio}'$/\\1/p\"); do \
            [ \"$s\" = 'wwan' ] && continue; \
            [ \"$(uci -q get wireless.$s.mode 2>/dev/null)\" = 'sta' ] && uci -q set wireless.$s.disabled='1'; \
         done; true"
    ));

    // Ensure the target radio is enabled (disabled=1 by default on a fresh flash)
    router.uci_set(&format!("wireless.{radio}.disabled"), "0")?;

    let encryption = normalize_encryption(&config.encryption);
    router.uci_set("wireless.wwan", "wifi-iface")?;
    router.uci_set("wireless.wwan.device", radio)?;
    router.uci_set("wireless.wwan.mode", "sta")?;
    router.uci_set("wireless.wwan.ssid", &config.ssid)?;
    router.uci_set("wireless.wwan.network", "wwan")?;
    router.uci_set("wireless.wwan.disabled", "0")?;
    router.uci_set("wireless.wwan.encryption", &encryption)?;
    if encryption != "none" && !config.password.is_empty() {
        router.uci_set("wireless.wwan.key", &config.password)?;
    } else {
        router.run_ok("uci -q delete wireless.wwan.key 2>/dev/null; true")?;
    }
    router.uci_commit(Some("wireless"))?;
    Ok(())
}

fn reload_wifi(router: &Router) {
    // Full cycle: "wifi reload" keeps stale wpa_supplicant state on some
    // drivers, including the MT7981 in the GL-MT3000.
    match router.run("wifi down 2>&1") {
        Ok((out, code)) if code != 0 => info!(
            "[{}] wifi down failed ({}): {}",
            router.host,
            code,
            out.trim()
        ),
        Err(err) => info!("[{}] wifi down failed: {}", router.host, err),
        _ => {}
    }
    match router.run("wifi up 2>&1") {
        Ok((out, code)) if code != 0 => {
            info!(
                "[{}] wifi up failed ({}): {} — falling back to network restart",
                router.host,
                code,
                out.trim()
            );
            let _ = router.run_ok("/etc/init.d/network restart 2>&1");
        }
        Err(err) => {
            info!("[{}] wifi up failed: {}", router.host, err);
            let _ = router.run_ok("/etc/init.d/network restart 2>&1");
        }
        _ => {}
    }
}

fn wwan_is_up(router: &Router) -> bool {
    let out = router
        .run_ok("ubus call network.interface.wwan status 2>/dev/null")
        .unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&out)
        .ok()
        .and_then(|value| value.get("up").and_then(|up| up.as_bool()))
        .unwrap_or(false)
}

/// ~22s: enough for association plus DHCP, matching the installer's budget.
fn wait_for_wwan(router: &Router) -> bool {
    for attempt in 0..15 {
        if wwan_is_up(router) {
            return true;
        }
        if attempt < 14 {
            std::thread::sleep(std::time::Duration::from_millis(1500));
        }
    }
    false
}

/// Configure a Wi-Fi WAN, trying band-compatible radios until one associates.
/// Returns the radio that came up.
pub fn configure_wisp(router: &Router, config: &WispConfig) -> Result<String> {
    info!(
        "[{}] Configuring WISP → ssid={} band={:?}",
        router.host, config.ssid, config.band
    );

    let candidates = radio_candidates(router, config.band.as_deref())?;

    // Network interface + firewall + DHCP are radio-independent: do them once.
    router.uci_set("network.wwan", "interface")?;
    router.uci_set("network.wwan.proto", "dhcp")?;
    router.uci_commit(Some("network"))?;
    ensure_wwan_in_wan_zone(router)?;
    router.uci_set("dhcp.lan.start", &config.dhcp_start.to_string())?;
    router.uci_set("dhcp.lan.limit", &config.dhcp_limit.to_string())?;
    router.uci_commit(Some("dhcp"))?;
    if config.masq {
        ensure_masq_on_wan_zone(router)?;
    }

    let mut tried = Vec::new();
    for radio in &candidates {
        info!(
            "[{}] Trying Wi-Fi WAN on {} (band={})",
            router.host,
            radio,
            radio_band(router, radio)
        );
        apply_sta(router, radio, config)?;
        reload_wifi(router);
        if wait_for_wwan(router) {
            info!(
                "[{}] Wi-Fi WAN associated on {} (ssid={})",
                router.host, radio, config.ssid
            );
            return Ok(radio.clone());
        }
        info!("[{}] Wi-Fi WAN did not associate on {}", router.host, radio);
        tried.push(radio.clone());
    }

    anyhow::bail!(
        "the router could not associate to \"{}\" on any radio ({}). \
         A radio can host an AP and a STA only on the same channel; \
         try the other band or move the AP channel.",
        config.ssid,
        tried.join(", ")
    )
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
    let wwan_radio = router.uci_get("wireless.wwan.device").unwrap_or_default();
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
        "radio":           wwan_radio,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_scan_encryption_labels() {
        assert_eq!(normalize_encryption("wpa-psk"), "psk2");
        assert_eq!(normalize_encryption("WPA2"), "psk2");
        assert_eq!(normalize_encryption("WPA PSK (CCMP)"), "psk2");
        assert_eq!(normalize_encryption("sae"), "sae");
        assert_eq!(normalize_encryption("WPA3"), "sae");
        assert_eq!(normalize_encryption("sae-mixed"), "sae-mixed");
        assert_eq!(normalize_encryption("wpa-psk/wpa2 mixed"), "psk-mixed");
        assert_eq!(normalize_encryption("none"), "none");
        assert_eq!(normalize_encryption(""), "none");
    }

    #[test]
    fn normalizes_uci_band_values() {
        assert_eq!(normalize_band("2g"), "2.4");
        assert_eq!(normalize_band("bg"), "2.4");
        assert_eq!(normalize_band("5g"), "5");
        assert_eq!(normalize_band("11a"), "5");
        assert_eq!(normalize_band("6g"), "6");
        assert_eq!(normalize_band(""), "");
    }
}
