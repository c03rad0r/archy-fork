use crate::Router;
use anyhow::Result;

pub struct ScannedNetwork {
    pub ssid: String,
    pub bssid: String,
    pub signal: i32,
    pub channel: u8,
    pub encryption: String,
}

pub fn scan_networks(router: &Router) -> Result<Vec<ScannedNetwork>> {
    let (iface, temp) = find_wireless_iface(router)?;
    let output = router.run_ok(&format!("iwinfo {} scan 2>&1", iface))?;
    let result = if output.contains("Scanning not possible") {
        // Vendor MediaTek `mt_wifi` driver (see find_wireless_iface) doesn't
        // support scanning through iwinfo/nl80211 at all. Fall back to its own
        // private ioctl site-survey, which works on the same interface.
        scan_via_mtk_site_survey(router, &iface)
    } else if output.contains("No scan results") || output.trim().is_empty() {
        Ok(vec![])
    } else {
        parse_iwinfo_scan(&output)
    };
    if temp {
        let _ = router.run(&format!("iw dev {} del 2>/dev/null", iface));
    }
    result
}

fn scan_via_mtk_site_survey(router: &Router, iface: &str) -> Result<Vec<ScannedNetwork>> {
    let _ = router.run(&format!("iwpriv {} set SiteSurvey=1 2>/dev/null", iface));
    std::thread::sleep(std::time::Duration::from_secs(4));
    let output = router.run_ok(&format!("iwpriv {} get_site_survey 2>&1", iface))?;
    parse_mtk_site_survey(&output)
}

/// Parses MediaTek's `iwpriv <iface> get_site_survey` fixed-width table.
/// Column offsets come from the header row layout, which is part of the
/// vendor SDK's ioctl response format shared across OEMs (GL.iNet, etc.),
/// not something set per-device.
fn parse_mtk_site_survey(output: &str) -> Result<Vec<ScannedNetwork>> {
    let mut networks = Vec::new();
    for line in output.lines() {
        if !line
            .trim_start()
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_digit)
        {
            continue; // skip header/summary lines; data rows start with an index
        }
        let ssid = line.get(8..41).unwrap_or("").trim().to_string();
        if ssid.is_empty() {
            continue;
        }
        let bssid = line.get(41..61).unwrap_or("").trim().to_string();
        let security = line.get(61..84).unwrap_or("");
        let channel: u8 = line
            .get(4..8)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let signal: i32 = line
            .get(84..92)
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(-100);
        networks.push(ScannedNetwork {
            ssid,
            bssid,
            signal,
            channel,
            encryption: normalize_encryption(security),
        });
    }
    // Strongest signal first. `Reverse` keeps this a stable descending sort,
    // identical in ordering (including ties) to the previous `sort_by` comparator.
    networks.sort_by_key(|n| std::cmp::Reverse(n.signal));
    Ok(networks)
}

/// Returns `(interface_name, is_temporary)`.
/// If no interface exists, creates a temporary managed one directly on the PHY
/// so we can scan without needing any UCI wifi-iface sections.
fn find_wireless_iface(router: &Router) -> Result<(String, bool)> {
    // Fast path: an interface already exists (radio was enabled previously)
    let (out, _) = router.run("iw dev 2>/dev/null | awk '/Interface/{print $2}' | head -1")?;
    if !out.trim().is_empty() {
        return Ok((out.trim().to_string(), false));
    }

    // Some vendor wifi drivers (e.g. MediaTek's out-of-tree `mt_wifi`/`mtk` SDK
    // driver used by GL.iNet and others) never register with cfg80211/mac80211,
    // so they have no `iw dev` entry and no /sys/class/ieee80211 phy even though
    // the radio is real and already up. `iwinfo` abstracts over those vendor
    // backends too, so fall back to its device listing before concluding there's
    // no radio at all.
    let (iwinfo_out, _) = router.run("iwinfo 2>/dev/null | awk '/^[A-Za-z]/{print $1; exit}'")?;
    if !iwinfo_out.trim().is_empty() {
        return Ok((iwinfo_out.trim().to_string(), false));
    }

    // Find the phy — if this is empty the device has no WiFi hardware at all
    let (phy_out, _) = router.run("ls /sys/class/ieee80211/ 2>/dev/null | head -1")?;
    let phy = phy_out.trim().to_string();
    if phy.is_empty() {
        anyhow::bail!("No wireless radio found on this router");
    }

    // Create a temporary managed interface directly on the PHY.  This bypasses
    // netifd entirely so it works even when there are no wifi-iface sections in
    // UCI (common on a freshly-flashed device).
    tracing::info!(
        "[{}] Creating temporary scan interface on {}",
        router.host,
        phy
    );
    // Remove any stale scan0 from a previous attempt, then add fresh
    let _ = router.run("iw dev scan0 del 2>/dev/null");
    router.run_ok(&format!(
        "iw phy {} interface add scan0 type managed 2>&1 && ip link set scan0 up 2>&1",
        phy
    ))?;

    Ok(("scan0".to_string(), true))
}

fn parse_iwinfo_scan(output: &str) -> Result<Vec<ScannedNetwork>> {
    let mut networks: Vec<ScannedNetwork> = Vec::new();
    let mut current: Option<ScannedNetwork> = None;

    for line in output.lines() {
        let line = line.trim();
        if line.starts_with("Cell ") {
            if let Some(n) = current.take() {
                if !n.ssid.is_empty() {
                    networks.push(n);
                }
            }
            let bssid = line
                .split("Address:")
                .nth(1)
                .unwrap_or("")
                .trim()
                .to_string();
            current = Some(ScannedNetwork {
                ssid: String::new(),
                bssid,
                signal: -100,
                channel: 0,
                encryption: "none".to_string(),
            });
        } else if let Some(ref mut n) = current {
            if let Some(rest) = line.strip_prefix("ESSID:") {
                n.ssid = rest.trim().trim_matches('"').to_string();
            } else if line.contains("Channel:") && !line.starts_with("Encryption") {
                if let Some(ch_part) = line.split("Channel:").nth(1) {
                    n.channel = ch_part
                        .split_whitespace()
                        .next()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                }
            } else if line.starts_with("Signal:") {
                if let Some(dbm_str) = line.split_whitespace().nth(1) {
                    n.signal = dbm_str.parse().unwrap_or(-100);
                }
            } else if let Some(rest) = line.strip_prefix("Encryption:") {
                n.encryption = normalize_encryption(rest.trim());
            }
        }
    }
    if let Some(n) = current {
        if !n.ssid.is_empty() {
            networks.push(n);
        }
    }

    // Strongest signal first. `Reverse` keeps this a stable descending sort,
    // identical in ordering (including ties) to the previous `sort_by` comparator.
    networks.sort_by_key(|n| std::cmp::Reverse(n.signal));
    Ok(networks)
}

fn normalize_encryption(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("wpa3") || lower.contains("sae") {
        "sae".to_string()
    } else if lower.contains("wpa2") || lower.contains("psk2") {
        "psk2".to_string()
    } else if lower.contains("wpa") {
        // CCMP/AES is WPA2's cipher suite — even if iwinfo labels it "WPA PSK (CCMP)"
        // it's a WPA2 network and we must use psk2 to associate correctly.
        if lower.contains("ccmp") || lower.contains("aes") {
            "psk2".to_string()
        } else {
            "psk".to_string()
        }
    } else if lower.contains("none") || lower.contains("open") || lower.is_empty() {
        "none".to_string()
    } else {
        lower
    }
}
