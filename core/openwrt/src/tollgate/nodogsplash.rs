use anyhow::{Context, Result};

use crate::opkg::PkgManager;
use crate::tollgate::TollGateConfig;
use crate::Router;

/// Install NoDogSplash and immediately stop it, before configuring anything.
///
/// The OpenWrt package's postinst auto-enables and starts nodogsplash on
/// install using its stock default config — critically, `gatewayinterface`
/// defaults to `br-lan`. On a fresh install that window is real: NoDogSplash
/// only manages IPv4 iptables, so anything plugged into `br-lan` (e.g. an
/// admin's own management box) silently loses IPv4 connectivity (DHCP still
/// listens, but the gate blocks the client until ndsctl authorizes its MAC)
/// until we get a chance to repoint it — a full network re-scan can take
/// long enough for that to matter. Stopping it right after install, before
/// `configure()` ever runs, closes that window as early as possible.
///
/// `tollgate-wrt` delegates all MAC authorization and gate open/close to
/// NoDogSplash via `ndsctl` — it has no firewall/netfilter code of its own
/// (confirmed: its binary has no `nft`/`ipset`/`iptables` calls at all).
/// Upstream's package therefore hard-depends on `+nodogsplash`, but neither
/// of our install paths (see `tollgate::install`) pull it in automatically.
pub fn install_and_stop(router: &Router, pkg_mgr: PkgManager) -> Result<()> {
    router
        .install_package(pkg_mgr, "nodogsplash")
        .context("install nodogsplash — required by tollgate-wrt for client gating")?;
    router.run_ok("/etc/init.d/nodogsplash stop || true")?;
    Ok(())
}

/// Point NoDogSplash's webroot at TollGate's own splash page instead of the
/// generic stock one NoDogSplash ships with.
///
/// Confirmed live: without this, NoDogSplash serves its own bundled
/// click-to-continue splash page — clicking "Continue" calls NDS's built-in
/// auth handler directly and authorizes the client with zero payment
/// involved. TollGate's actual payment UI (a QR/Cashu-token entry SPA) lives
/// at `/etc/tollgate/tollgate-captive-portal-site` — the .ipk's data payload
/// stages it there (see `packaging/files/tollgate-captive-portal-site/` in
/// the upstream repo), it's just never wired up as NoDogSplash's webroot.
///
/// Mirrors upstream's own `90-tollgate-captive-portal-symlink` uci-defaults
/// script exactly (symlink swap, not a `webroot` UCI override) — confirmed
/// live that setting `option webroot` directly instead causes NoDogSplash to
/// 500 on every request, for reasons not fully understood (worth filing
/// upstream, but the symlink approach is what's actually shipped/tested).
pub fn install_captive_portal_symlink(router: &Router) -> Result<()> {
    let (_, exists) = router.run("test -d /etc/tollgate/tollgate-captive-portal-site")?;
    if exists != 0 {
        anyhow::bail!(
            "/etc/tollgate/tollgate-captive-portal-site missing — expected to be staged \
             by the tollgate-wrt package install"
        );
    }

    router.run_ok(
        "if [ -L /etc/nodogsplash/htdocs ]; then \
             true; \
         else \
             if [ -d /etc/nodogsplash/htdocs ]; then \
                 mv /etc/nodogsplash/htdocs /etc/nodogsplash/htdocs.backup; \
             fi; \
             rm -rf /etc/nodogsplash/htdocs; \
             ln -sf /etc/tollgate/tollgate-captive-portal-site /etc/nodogsplash/htdocs; \
         fi",
    )?;
    Ok(())
}

/// Whether `ip link show` answers for this bridge name.
///
/// The name is validated before interpolation: UCI values are operator input.
fn bridge_exists(router: &Router, bridge: &str) -> bool {
    let bridge = bridge.trim();
    if bridge.is_empty()
        || !bridge
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return false;
    }
    router
        .run(&format!("ip link show {bridge} 2>/dev/null"))
        .map(|(_, code)| code == 0)
        .unwrap_or(false)
}

/// Networks carried by enabled AP wifi-iface sections, isolated `tollgate`
/// first. A stock install has `lan`; Archy's isolated layout has `tollgate`.
fn ap_networks(router: &Router) -> Vec<String> {
    let (out, _) = router
        .run("uci show wireless 2>/dev/null")
        .unwrap_or_default();
    let mut sections: std::collections::BTreeMap<String, (String, String, String)> =
        std::collections::BTreeMap::new();
    for line in out.lines() {
        let Some((lhs, rhs)) = line.trim().split_once('=') else {
            continue;
        };
        let parts: Vec<&str> = lhs.splitn(3, '.').collect();
        if parts.len() != 3 || parts[0] != "wireless" {
            continue;
        }
        let value = rhs.trim().trim_matches('\'').to_string();
        let entry = sections.entry(parts[1].to_string()).or_default();
        match parts[2] {
            "mode" => entry.0 = value,
            "network" => entry.1 = value,
            "disabled" => entry.2 = value,
            _ => {}
        }
    }
    let mut networks: Vec<String> = Vec::new();
    for (mode, network, disabled) in sections.values() {
        if mode != "ap" || disabled == "1" || network.is_empty() {
            continue;
        }
        if !networks.contains(network) {
            networks.push(network.clone());
        }
    }
    networks.sort_by_key(|n| if n == "tollgate" { 0 } else { 1 });
    networks
}

/// Resolve a UCI network to its bridge device. OpenWrt omits
/// `network.lan.device` when it uses the implicit `br-lan`.
fn network_device(router: &Router, network: &str) -> Option<String> {
    if let Ok(dev) = router.uci_get(&format!("network.{network}.device")) {
        let dev = dev.trim();
        if !dev.is_empty() {
            return Some(dev.to_string());
        }
    }
    let candidate = format!("br-{network}");
    if bridge_exists(router, &candidate) {
        Some(candidate)
    } else {
        None
    }
}

/// The bridge that actually carries paid clients.
///
/// Order:
/// 1. `tollgate.main.paid_bridge`, when it names an existing bridge.
/// 2. `network.tollgate.device` (Archy's isolated layout).
/// 3. The paid AP's `network` resolved to its device (stock installer layout:
///    `lan` -> `br-lan`).
/// 4. The existing NoDogSplash gateway.
/// 5. `br-tollgate`, then `br-lan`, when they exist.
///
/// Empty when nothing resolves. Callers must refuse to rewrite in that case;
/// substituting a literal is what broke gating on 2026-11-07.
pub fn paid_bridge(router: &Router) -> String {
    if let Ok(value) = router.uci_get("tollgate.main.paid_bridge") {
        let value = value.trim();
        if !value.is_empty() && bridge_exists(router, value) {
            return value.to_string();
        }
    }
    if let Ok(value) = router.uci_get("network.tollgate.device") {
        let value = value.trim();
        if !value.is_empty() && bridge_exists(router, value) {
            return value.to_string();
        }
    }
    for network in ap_networks(router) {
        if let Some(dev) = network_device(router, &network) {
            return dev;
        }
    }
    for gateway in enabled_gateways(router) {
        if bridge_exists(router, &gateway) {
            return gateway;
        }
    }
    for candidate in ["br-tollgate", "br-lan"] {
        if bridge_exists(router, candidate) {
            return candidate.to_string();
        }
    }
    String::new()
}

/// Configure NoDogSplash to gate the bridge that actually carries the paid AP.
///
/// This is not always `br-tollgate`: the stock/installer layout keeps the paid
/// AP on `lan` (`br-lan`), while Archy's isolated layout creates
/// `br-tollgate`. Resolve the bridge first and refuse to point the portal at a
/// name that does not exist.
pub fn configure(router: &Router, cfg: &TollGateConfig) -> Result<()> {
    let bridge = paid_bridge(router);
    if bridge.is_empty() {
        anyhow::bail!(
            "cannot configure the paid portal: no AP network, NDS gateway or \
             tollgate.main.paid_bridge names an existing bridge"
        );
    }
    router.run_ok("touch /etc/config/nodogsplash")?;

    // The nodogsplash package's own uci-defaults populate an anonymous
    // `@nodogsplash[0]` section on first install, pointed at `br-lan` (its
    // stock default — see `install_and_stop`). NoDogSplash supports multiple
    // simultaneous gateway instances, one per config section, so leaving this
    // in place alongside our own `nodogsplash.main` doesn't get overridden by
    // it — it starts a *second* instance gating br-lan for real. Delete it;
    // `main` is the only instance this project manages.
    let _ = router.uci_delete("nodogsplash.@nodogsplash[0]");

    router.uci_set("nodogsplash.main", "nodogsplash")?;
    router.uci_set("nodogsplash.main.enabled", "1")?;
    router.uci_set("nodogsplash.main.gatewayinterface", &bridge)?;
    router.uci_set(
        "nodogsplash.main.gatewayname",
        &format!("{} Portal", cfg.ssid),
    )?;
    router.uci_set("nodogsplash.main.gatewaydomainname", "TollGate.lan")?;
    router.uci_set("nodogsplash.main.gatewayport", "2050")?;

    // Pre-auth "walled garden": traffic an unauthenticated client must still
    // reach before ndsctl authorizes their MAC. `uci_delete` + rebuild (rather
    // than only adding our own entries) is deliberate — the stock package
    // config ships a `users_to_router` default of its own (DNS, DHCP, plus
    // SSH/Telnet to the router), and `uci_set`/`add_list` on an existing
    // *named* section does not clear an inherited default list, so without
    // an explicit delete first, re-provisioning would silently keep
    // whatever was there before.
    //
    // DNS (53) and DHCP (67, udp) are carried over from that stock default —
    // without them a client can't even get an IP or resolve the portal
    // domain before authenticating (confirmed live: omitting udp/67 here
    // broke DHCP entirely for new clients on the archipelago SSID). 2121
    // (TollGate payment) and 2050 (NDS's own splash portal) are ours.
    // SSH/Telnet (22/23) are deliberately *not* carried over — the stock
    // default exposes router shell access to every unauthenticated device
    // on a public pay-as-you-go network, which is a bad default here.
    let _ = router.uci_delete("nodogsplash.main.users_to_router");
    router.uci_add_list("nodogsplash.main.users_to_router", "allow udp port 53")?;
    router.uci_add_list("nodogsplash.main.users_to_router", "allow tcp port 53")?;
    router.uci_add_list("nodogsplash.main.users_to_router", "allow udp port 67")?;
    router.uci_add_list("nodogsplash.main.users_to_router", "allow tcp port 2121")?;
    router.uci_add_list("nodogsplash.main.users_to_router", "allow tcp port 2050")?;

    // Post-auth (paid) clients get full access — matches the stock package
    // default (`list authenticated_users 'allow all'`), which our from-scratch
    // named section never carried over. Under this router's default-ACCEPT
    // FORWARD policy an empty list happens to behave the same, but that's an
    // accident of this specific setup, not something to depend on.
    let _ = router.uci_delete("nodogsplash.main.authenticated_users");
    router.uci_add_list("nodogsplash.main.authenticated_users", "allow all")?;

    router.uci_commit(Some("nodogsplash"))?;
    Ok(())
}

/// Enabled NoDogSplash gateway interfaces, in UCI section order.
fn enabled_gateways(router: &Router) -> Vec<String> {
    use std::collections::BTreeMap;
    let (out, _) = router
        .run("uci show nodogsplash 2>/dev/null")
        .unwrap_or_default();
    let mut sections: BTreeMap<String, (Option<String>, bool)> = BTreeMap::new();
    for line in out.lines() {
        let Some((lhs, rhs)) = line.trim().split_once('=') else {
            continue;
        };
        let parts: Vec<&str> = lhs.splitn(3, '.').collect();
        if parts.len() != 3 || parts[0] != "nodogsplash" {
            continue;
        }
        let section = parts[1]
            .trim_start_matches("@nodogsplash[")
            .trim_end_matches(']')
            .to_string();
        let value = rhs.trim().trim_matches('\'');
        let entry = sections.entry(section).or_insert((None, false));
        match parts[2] {
            "gatewayinterface" => entry.0 = Some(value.to_string()),
            "enabled" => entry.1 = value == "1",
            _ => {}
        }
    }
    sections
        .values()
        .filter(|(_, enabled)| *enabled)
        .filter_map(|(iface, _)| iface.clone())
        .collect()
}

/// The active gateway interface and whether it matches the derived paid bridge.
pub fn summary(router: &Router) -> (String, bool) {
    let expected = paid_bridge(router);
    let gateways = enabled_gateways(router);
    let gateway = gateways
        .iter()
        .find(|g| g.as_str() == expected)
        .cloned()
        .or_else(|| gateways.first().cloned())
        .unwrap_or_default();
    let ok = !expected.is_empty()
        && gateways.len() == 1
        && gateway == expected
        && gateways.iter().any(|g| g == &expected)
        && bridge_exists(router, &expected);
    (gateway, ok)
}

/// Refuse a provisioned router whose NoDogSplash is not gating the bridge that
/// carries the paid AP. Accepts both the stock `br-lan` layout and Archy's
/// isolated `br-tollgate` layout; rejects extra enabled gateways and any
/// derived bridge that does not exist.
pub fn verify(router: &Router) -> Result<()> {
    let expected = paid_bridge(router);
    if expected.is_empty() {
        anyhow::bail!(
            "cannot determine the paid bridge: no AP network, NDS gateway or \
             tollgate.main.paid_bridge names an existing bridge"
        );
    }
    let gateways = enabled_gateways(router);
    if gateways.is_empty() {
        anyhow::bail!("NoDogSplash has no enabled gateway section");
    }
    if !gateways.iter().any(|g| g == &expected) {
        anyhow::bail!(
            "NoDogSplash is not gating {expected} (enabled interfaces: {:?})",
            gateways
        );
    }
    if let Some(extra) = gateways.iter().find(|g| g.as_str() != expected) {
        anyhow::bail!(
            "NoDogSplash is gating an extra interface {extra} (expected {expected}; \
             enabled interfaces: {:?})",
            gateways
        );
    }
    if !bridge_exists(router, &expected) {
        anyhow::bail!("{expected} does not exist");
    }
    Ok(())
}

/// (Re)start the nodogsplash service so config changes and gate state take effect.
pub fn restart(router: &Router) -> Result<()> {
    router.run_ok("/etc/init.d/nodogsplash enable")?;
    router.run_ok("/etc/init.d/nodogsplash restart || /etc/init.d/nodogsplash start")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        bridges: Vec<String>,
        wireless: String,
        tollgate_dev: Option<String>,
        explicit: Option<String>,
        nds: String,
    }

    fn router(f: Fixture) -> Router {
        let bridges = f.bridges.clone();
        Router::scripted(move |cmd| {
            if cmd == "uci show wireless 2>/dev/null" {
                return Ok((f.wireless.clone(), 0));
            }
            if cmd == "uci show nodogsplash 2>/dev/null" {
                return Ok((f.nds.clone(), 0));
            }
            if let Some(key) = cmd.strip_prefix("uci get ") {
                let value = match key {
                    "tollgate.main.paid_bridge" => f.explicit.clone(),
                    "network.tollgate.device" => f.tollgate_dev.clone(),
                    other if other.starts_with("network.") && other.ends_with(".device") => None,
                    _ => None,
                };
                return value
                    .map(|v| (v, 0))
                    .ok_or_else(|| anyhow::anyhow!("uci: entry not found"));
            }
            if let Some(rest) = cmd.strip_prefix("ip link show ") {
                let name = rest.split_whitespace().next().unwrap_or("");
                let code = if bridges.iter().any(|b| b == name) {
                    0
                } else {
                    1
                };
                return Ok((String::new(), code));
            }
            Ok((String::new(), 0))
        })
    }

    fn ap(network: &str) -> String {
        format!(
            "wireless.default_radio0=wifi-iface\n\
             wireless.default_radio0.mode='ap'\n\
             wireless.default_radio0.network='{network}'\n\
             wireless.default_radio0.disabled='0'\n"
        )
    }

    #[test]
    fn stock_layout_resolves_br_lan() {
        let r = router(Fixture {
            bridges: vec!["br-lan".into(), "br-private".into()],
            wireless: ap("lan"),
            tollgate_dev: None,
            explicit: None,
            nds: "nodogsplash.main=nodogsplash\n\
                  nodogsplash.main.enabled='1'\n\
                  nodogsplash.main.gatewayinterface='br-lan'\n"
                .into(),
        });
        assert_eq!(paid_bridge(&r), "br-lan");
        assert_eq!(summary(&r), ("br-lan".into(), true));
        assert!(verify(&r).is_ok());
    }

    #[test]
    fn isolated_layout_resolves_br_tollgate() {
        let r = router(Fixture {
            bridges: vec!["br-lan".into(), "br-tollgate".into()],
            wireless: ap("tollgate"),
            tollgate_dev: Some("br-tollgate".into()),
            explicit: None,
            nds: "nodogsplash.main=nodogsplash\n\
                  nodogsplash.main.enabled='1'\n\
                  nodogsplash.main.gatewayinterface='br-tollgate'\n"
                .into(),
        });
        assert_eq!(paid_bridge(&r), "br-tollgate");
        assert_eq!(summary(&r), ("br-tollgate".into(), true));
        assert!(verify(&r).is_ok());
    }

    #[test]
    fn explicit_paid_bridge_wins_when_it_exists() {
        let r = router(Fixture {
            bridges: vec!["br-lan".into(), "br-guest".into()],
            wireless: ap("lan"),
            tollgate_dev: None,
            explicit: Some("br-guest".into()),
            nds: "nodogsplash.main.enabled='1'\n\
                  nodogsplash.main.gatewayinterface='br-lan'\n"
                .into(),
        });
        assert_eq!(paid_bridge(&r), "br-guest");
        // The enforcement still says br-lan, so the check must fail, not lie.
        assert!(!summary(&r).1);
        assert!(verify(&r).is_err());
    }

    #[test]
    fn a_nonexistent_explicit_bridge_is_ignored() {
        let r = router(Fixture {
            bridges: vec!["br-lan".into()],
            wireless: ap("lan"),
            tollgate_dev: None,
            explicit: Some("br-missing".into()),
            nds: "nodogsplash.main.enabled='1'\n\
                  nodogsplash.main.gatewayinterface='br-lan'\n"
                .into(),
        });
        assert_eq!(paid_bridge(&r), "br-lan");
    }

    #[test]
    fn no_resolvable_bridge_refuses_to_guess() {
        let r = router(Fixture {
            bridges: vec![],
            wireless: String::new(),
            tollgate_dev: None,
            explicit: None,
            nds: String::new(),
        });
        assert_eq!(paid_bridge(&r), "");
        assert!(verify(&r).is_err());
    }

    #[test]
    fn an_extra_enabled_gateway_is_rejected() {
        let r = router(Fixture {
            bridges: vec!["br-lan".into(), "br-tollgate".into()],
            wireless: ap("lan"),
            tollgate_dev: None,
            explicit: None,
            nds: "nodogsplash.main.enabled='1'\n\
                  nodogsplash.main.gatewayinterface='br-lan'\n\
                  nodogsplash.@nodogsplash[0].enabled='1'\n\
                  nodogsplash.@nodogsplash[0].gatewayinterface='br-tollgate'\n"
                .into(),
        });
        assert_eq!(summary(&r), ("br-lan".into(), false));
        assert!(verify(&r).is_err());
    }
}
