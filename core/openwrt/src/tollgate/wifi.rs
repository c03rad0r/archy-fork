use anyhow::{Context, Result};
use tracing::info;

use crate::tollgate::TollGateConfig;
use crate::Router;

/// Create (or update) the dedicated pay-as-you-go WiFi interface for TollGate.
///
/// Uses a fixed named section (`wireless.tollgate`) rather than `uci add`, so
/// re-provisioning (e.g. editing price/mint URL after install) updates the
/// same interface in place instead of piling up a new `wifi-iface` section —
/// and therefore a new duplicate broadcast SSID — on every call.
pub fn provision_ssid(router: &Router, cfg: &TollGateConfig) -> Result<()> {
    let radio = detect_radio(router).context("detect WiFi radio")?;
    info!("[{}] Using radio {} for TollGate SSID", router.host, radio);

    router.uci_apply(
        "wireless",
        &[
            ("wireless.tollgate", "wifi-iface"),
            ("wireless.tollgate.device", &radio),
            ("wireless.tollgate.mode", "ap"),
            ("wireless.tollgate.ssid", &cfg.ssid),
            ("wireless.tollgate.encryption", "none"),
            ("wireless.tollgate.network", "tollgate"),
            // Disable 802.11r/k/v — unnecessary for transient pay-as-you-go clients.
            ("wireless.tollgate.ieee80211r", "0"),
            // Stop broadcasting entirely when disabled, rather than leaving an
            // open SSID up that leads nowhere once the backend is stopped.
            (
                "wireless.tollgate.disabled",
                if cfg.enabled { "0" } else { "1" },
            ),
        ],
    )?;

    provision_network(router)?;
    provision_firewall(router)?;

    Ok(())
}

/// Add a `tollgate` network interface (isolated LAN for TollGate clients).
///
/// Binds to a named bridge device (`br-tollgate`) rather than leaving the
/// wifi-iface as the network's raw device — NoDogSplash's `gatewayinterface`
/// needs a stable, known interface name to gate (see `nodogsplash::provision`),
/// and the driver-assigned name of a bare wifi vif (e.g. `phy0-ap0`) isn't
/// guaranteed across hardware.
fn provision_network(router: &Router) -> Result<()> {
    router.uci_apply(
        "network",
        &[
            ("network.tollgate_bridge", "device"),
            ("network.tollgate_bridge.type", "bridge"),
            ("network.tollgate_bridge.name", "br-tollgate"),
            ("network.tollgate", "interface"),
            ("network.tollgate.device", "br-tollgate"),
            ("network.tollgate.proto", "static"),
            ("network.tollgate.ipaddr", "192.168.99.1"),
            ("network.tollgate.netmask", "255.255.255.0"),
            // NoDogSplash only manages IPv4 iptables rules. If IPv6 RA/DHCPv6
            // stays enabled, clients get routable IPv6 addresses and their OS
            // validates connectivity (and browses freely) over IPv6, bypassing
            // the portal entirely. See OpenTollGate/tollgate-module-basic-go#148.
            ("network.tollgate.ip6assign", "0"),
        ],
    )?;

    // Enable DHCP for the tollgate interface.
    router.uci_apply(
        "dhcp",
        &[
            ("dhcp.tollgate", "dhcp"),
            ("dhcp.tollgate.interface", "tollgate"),
            ("dhcp.tollgate.start", "100"),
            ("dhcp.tollgate.limit", "150"),
            ("dhcp.tollgate.leasetime", "5m"),
            ("dhcp.tollgate.ra", "disabled"),
            ("dhcp.tollgate.dhcpv6", "disabled"),
        ],
    )?;

    Ok(())
}

/// Add firewall zone for the tollgate interface.
///
/// This zone only isolates tollgate clients from other LAN segments and
/// opens the payment port to the router. Per-client forwarding to WAN is
/// actually gated by NoDogSplash's own iptables rules (via `ndsctl`), not by
/// anything in this static firewall config — `tollgate-wrt` has no netfilter
/// code of its own. See `nodogsplash::provision`.
fn provision_firewall(router: &Router) -> Result<()> {
    // Zone
    router.uci_apply(
        "firewall",
        &[
            ("firewall.tollgate_zone", "zone"),
            ("firewall.tollgate_zone.name", "tollgate"),
            ("firewall.tollgate_zone.network", "tollgate"),
            ("firewall.tollgate_zone.input", "ACCEPT"),
            ("firewall.tollgate_zone.output", "ACCEPT"),
            ("firewall.tollgate_zone.forward", "REJECT"),
        ],
    )?;

    // Forwarding rule: tollgate → wan (TollGate manages which clients can forward)
    router.uci_apply(
        "firewall",
        &[
            ("firewall.tollgate_fwd", "forwarding"),
            ("firewall.tollgate_fwd.src", "tollgate"),
            ("firewall.tollgate_fwd.dest", "wan"),
        ],
    )?;

    Ok(())
}

/// Return the first available wireless radio device name (e.g. "radio0").
fn detect_radio(router: &Router) -> Result<String> {
    let out =
        router.run_ok("uci show wireless | grep -o 'wireless\\.radio[0-9]*\\.type' | head -1")?;
    // Extract "radioN" from "wireless.radioN.type"
    let radio = out.trim().split('.').nth(1).unwrap_or("radio0").to_string();
    Ok(radio)
}
