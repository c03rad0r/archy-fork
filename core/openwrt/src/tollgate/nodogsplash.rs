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

/// Configure NoDogSplash to gate the dedicated `br-tollgate` bridge (see
/// `wifi::provision_network`), not `br-lan` — the paid SSID here lives on its
/// own isolated network/subnet rather than the canonical upstream layout
/// where it's bridged into `lan`.
///
/// Must run after `wifi::provision_ssid` has created `br-tollgate` — pointing
/// `gatewayinterface` at a bridge that doesn't exist yet is at best a no-op
/// and at worst leaves NoDogSplash in a confused state.
pub fn configure(router: &Router, cfg: &TollGateConfig) -> Result<()> {
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
    router.uci_set("nodogsplash.main.gatewayinterface", "br-tollgate")?;
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

/// (Re)start the nodogsplash service so config changes and gate state take effect.
pub fn restart(router: &Router) -> Result<()> {
    router.run_ok("/etc/init.d/nodogsplash enable")?;
    router.run_ok("/etc/init.d/nodogsplash restart || /etc/init.d/nodogsplash start")?;
    Ok(())
}
