pub mod assets;
pub mod config;
pub mod install;
pub mod nodogsplash;
pub mod verify;
pub mod version;
pub mod wifi;

pub use assets::{AssetShape, PackageTarget, FEED_REPO, LEGACY_REPO};
pub use config::TollGateConfig;
pub use install::{
    install_runtime_deps, install_tollgate, installed_tollgate_version,
    installed_tollgate_version_any, read_package_target, TOLLGATE_VERSION,
};
pub use verify::{manifest_url, verify_download, VerifyOutcome};
pub use version::{package_version_from_pin, TOLLGATE_PACKAGE, TOLLGATE_SERVICE_BIN};
pub use wifi::provision_ssid;

use anyhow::{Context, Result};
use tracing::info;

use crate::{opkg::PkgManager, Router};

/// Full TollGate provisioning sequence:
/// 1. Install `tollgate-wrt` at the pinned release
/// 2. Install NoDogSplash and immediately stop it (its postinst auto-starts
///    it against `br-lan` by default — see `nodogsplash::install_and_stop`)
/// 3. Write TollGate config: UCI (status/detection only) + the JSON file the
///    daemon actually reads
/// 4. Create the pay-as-you-go WiFi SSID and its dedicated bridge/network
/// 5. Configure NoDogSplash to gate that bridge (now that it exists) —
///    client gating; tollgate-wrt has no enforcement code of its own
/// 6. Restart affected services
pub async fn provision(router: &Router, config: &TollGateConfig) -> Result<()> {
    info!("[{}] Starting TollGate provisioning", router.host);

    let pkg_mgr = router.opkg_check()?;
    match pkg_mgr {
        PkgManager::Opkg => {
            router.opkg_update()?;
            install_tollgate(router)?;
        }
        PkgManager::ApkNative => {
            install::install_tollgate_apk_native(router)?;
        }
    }

    // Runtime dependencies the package declares but that no install path here
    // resolves: jq decides which UI answers the entry ports, and ca-bundle is
    // what lets the gateway verify a mint over TLS. Installed after the
    // package so a failure names the dependency rather than the package.
    install::install_runtime_deps(router, pkg_mgr)?;

    // NoDogSplash is a hard runtime dependency of tollgate-wrt (upstream's
    // package declares `+nodogsplash`), but neither install path above pulls
    // it in: both install paths install a single verified local asset with no
    // feed solver behind it (opkg/apk resolve deps only for packages they
    // fetch themselves), so nothing pulls the declared DEPENDS in.
    // Without it, tollgate-wrt runs and accepts payments but never actually
    // blocks unpaid clients. Install + stop happens before anything else so
    // its auto-started default config (gating br-lan) is live for as little
    // time as possible.
    nodogsplash::install_and_stop(router, pkg_mgr)
        .context("install nodogsplash — tollgate-wrt cannot gate clients without it")?;

    // Wire NoDogSplash's webroot to TollGate's actual payment portal instead
    // of the generic stock splash page it ships with. Confirmed live: without
    // this, "click continue" on the stock page authorizes the client via
    // NoDogSplash's own built-in handler with zero payment involved.
    nodogsplash::install_captive_portal_symlink(router).context(
        "wire up TollGate's captive portal — without it NoDogSplash serves its own \
         generic splash page, which authorizes clients on click with no payment",
    )?;

    config::apply(router, config)?;
    wifi::provision_ssid(router, config)?;
    // Must come after provision_ssid (creates the `tollgate` network this
    // folds the upstream installer's own default AP onto) — see
    // regate_upstream_default_aps for why this is needed at all.
    wifi::regate_upstream_default_aps(router)
        .context("re-gate upstream tollgate-module-basic-go default AP(s)")?;
    // Must come after provision_ssid (which creates br-tollgate) and before
    // the daemon restart below — config.json is only read at startup.
    config::apply_daemon_config(router, config)
        .context("write /etc/tollgate/config.json — tollgate-wrt reads this, not UCI")?;
    config::apply_payout_identity(router, config.payout_address.as_deref())
        .context("write /etc/tollgate/identities.json owner payout address")?;
    // Also must come after provision_ssid: points gatewayinterface at
    // br-tollgate, which provision_ssid is what creates.
    nodogsplash::configure(router, config)
        .context("configure nodogsplash — tollgate-wrt cannot gate clients without it")?;

    restart_services(router, config.enabled)?;
    nodogsplash::restart(router)?;

    info!("[{}] TollGate provisioning complete", router.host);
    Ok(())
}

/// Applies `enabled` to the actual running service, not just the UCI value —
/// the tollgate-wrt init script doesn't consult `tollgate.main.enabled`
/// itself, so toggling it requires an explicit enable/start or disable/stop.
///
/// The service's init script is `/etc/init.d/tollgate-wrt` (its actual
/// on-disk name — "tollgate" alone does not exist).
fn restart_services(router: &Router, enabled: bool) -> Result<()> {
    if enabled {
        router.run_ok("/etc/init.d/tollgate-wrt enable")?;
        router.run_ok("/etc/init.d/tollgate-wrt restart || /etc/init.d/tollgate-wrt start")?;
    } else {
        router.run_ok("/etc/init.d/tollgate-wrt stop || true")?;
        router.run_ok("/etc/init.d/tollgate-wrt disable || true")?;
    }
    router.run_ok("/etc/init.d/network restart")?;
    // Reload wireless so wireless.tollgate.disabled takes effect on the radio —
    // `network restart` alone doesn't reliably reconfigure wifi interfaces.
    router.run_ok("wifi down 2>&1; wifi up 2>&1")?;
    // Observed live, twice, in two different ways: netifd can lose the race
    // to claim br-tollgate as the wifi vif attaches to it during the restart
    // above. The first time it showed up as netifd reporting
    // "up: false, DEVICE_CLAIM_FAILED"; the second time netifd reported the
    // interface up with its address assigned while the kernel-level device
    // genuinely had none (`ip -4 addr show br-tollgate` empty) — dnsmasq
    // logged "DHCP packet received on br-tollgate which has no address" and
    // silently dropped every DISCOVER. A single blind ifdown/ifup isn't
    // trustworthy here — verify the address actually landed at the kernel
    // level (not just what netifd claims) and retry the cycle if not, since
    // NoDogSplash refuses to start against an interface that isn't really up
    // and dnsmasq will silently refuse to answer DHCP without erroring loudly.
    router.run_ok(
        "sleep 2; \
         for i in 1 2 3 4 5; do \
             ifdown tollgate 2>&1; sleep 1; ifup tollgate 2>&1; sleep 2; \
             ip -4 addr show br-tollgate 2>/dev/null | grep -q 'inet ' && break; \
             echo \"br-tollgate has no kernel-level IPv4 address after cycle $i, retrying\"; \
         done; \
         ip -4 addr show br-tollgate 2>/dev/null | grep -q 'inet ' || \
             { echo 'br-tollgate never got a kernel-level IPv4 address after 5 cycles'; exit 1; }",
    )?;
    Ok(())
}
