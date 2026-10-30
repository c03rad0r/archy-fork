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
    let mut candidates = wireless_ifaces(router);
    if candidates.is_empty() {
        candidates.push(create_temp_scan_iface(router)?);
    }

    let mut any_ok = false;
    let mut merged: Vec<ScannedNetwork> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut used_temp = false;

    for iface in candidates {
        if iface == "scan0" {
            used_temp = true;
        }
        match scan_iface(router, &iface) {
            Ok(networks) => {
                any_ok = true;
                for network in networks {
                    if let Some(existing) = merged
                        .iter_mut()
                        .find(|n| !n.bssid.is_empty() && n.bssid == network.bssid)
                    {
                        if network.signal > existing.signal {
                            *existing = network;
                        }
                    } else {
                        merged.push(network);
                    }
                }
            }
            Err(err) => errors.push(format!("{iface}: {err}")),
        }
    }

    if used_temp {
        let _ = router.run("iw dev scan0 del 2>/dev/null");
    }

    if any_ok {
        merged.sort_by_key(|n| std::cmp::Reverse(n.signal));
        Ok(merged)
    } else {
        anyhow::bail!("no wireless interface could scan: {}", errors.join("; "))
    }
}

/// Scan one interface, treating the nl80211 "busy/no event" failure as a
/// reason to try the next radio rather than failing the whole request.
fn scan_iface(router: &Router, iface: &str) -> Result<Vec<ScannedNetwork>> {
    let output = router.run_ok(&format!("iwinfo {} scan 2>&1", iface))?;
    let lower = output.to_lowercase();
    if lower.contains("netlink error") || lower.contains("no event received") {
        anyhow::bail!("{}", output.trim());
    }
    if output.contains("Scanning not possible") {
        // Vendor MediaTek `mt_wifi` driver doesn't support scanning through
        // iwinfo/nl80211 at all. Fall back to its private ioctl site survey.
        return scan_via_mtk_site_survey(router, iface);
    }
    if output.contains("No scan results") || output.trim().is_empty() {
        return Ok(vec![]);
    }
    parse_iwinfo_scan(&output)
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

/// Existing wireless interfaces, APs first.
///
/// The first `iw dev` interface is often the active station (`phy1-sta0`);
/// scanning on an associated station returns `Netlink error while awaiting
/// scan results: No event received` on MediaTek. An AP vif on the other radio
/// can scan normally, so prefer all APs and try each one.
fn wireless_ifaces(router: &Router) -> Vec<String> {
    let (out, _) = router
        .run(
            "iw dev 2>/dev/null | awk '/^[[:space:]]*Interface/ {iface=$2} \
         /^[[:space:]]*type/ {print iface, $2}'",
        )
        .unwrap_or_default();
    let mut entries: Vec<(String, String)> = out
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let iface = parts.next()?.to_string();
            let kind = parts.next().unwrap_or("").to_string();
            Some((iface, kind))
        })
        .collect();
    entries.sort_by_key(|(_, kind)| match kind.as_str() {
        "AP" => 0,
        "managed" => 2,
        _ => 1,
    });
    let mut ifaces: Vec<String> = entries.into_iter().map(|(iface, _)| iface).collect();

    // Vendor drivers (e.g. MediaTek's out-of-tree `mt_wifi` SDK) may not
    // register with cfg80211, so `iw dev` is empty. `iwinfo` still lists them.
    if ifaces.is_empty() {
        let (iwinfo_out, _) = router
            .run("iwinfo 2>/dev/null | awk '/^[A-Za-z]/{print $1; exit}'")
            .unwrap_or_default();
        if !iwinfo_out.trim().is_empty() {
            ifaces.push(iwinfo_out.trim().to_string());
        }
    }
    ifaces
}

/// Create a temporary managed interface on the first PHY. Used only when the
/// router has no existing wireless interfaces (a freshly flashed device).
fn create_temp_scan_iface(router: &Router) -> Result<String> {
    let (phy_out, _) = router.run("ls /sys/class/ieee80211/ 2>/dev/null | head -1")?;
    let phy = phy_out.trim().to_string();
    if phy.is_empty() {
        anyhow::bail!("No wireless radio found on this router");
    }
    tracing::info!(
        "[{}] Creating temporary scan interface on {}",
        router.host,
        phy
    );
    let _ = router.run("iw dev scan0 del 2>/dev/null");
    router.run_ok(&format!(
        "iw phy {} interface add scan0 type managed 2>&1 && ip link set scan0 up 2>&1",
        phy
    ))?;
    Ok("scan0".to_string())
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
