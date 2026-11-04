//! Keep the paid bridge reachable to the TollGate API on `:2121`.
//!
//! ## The defect this exists for
//!
//! `tollgate-wrt` ships nftables fragments that hardcode the name of the
//! captive bridge. `/etc/nftables.d/30-backend-firewall.nft` drops the backend
//! API from every interface except a fixed set:
//!
//! ```text
//! meta nfproto ipv4 iifname != { "br-lan", "br-private", "lo" } \
//!      tcp dport 2121 counter drop
//! ```
//!
//! Archipelago names that bridge `br-tollgate` (see [`wifi::provision_ssid`]),
//! so the package's own hardening drops the customer's call to the portal API.
//! Measured on a GL-MT3000 (25.12.5, tollgate-wrt 0.6.0-r1): the portal page
//! served fine on `:2050` while `:2121` timed out from the paid network, the
//! page sat on "Loading" forever, and the drop rule's own counter read
//! `315 packets / 18900 bytes`. Adding `br-tollgate` to that set takes the
//! client from `000` to `200` and the portal renders its price.
//!
//! ## Why it edits the packaged fragment rather than adding a rule
//!
//! nftables runs every base chain on a hook; an `accept` in one chain does not
//! stop a later chain from dropping. A separate include therefore cannot
//! rescue the packet — verified: a chain at `priority filter - 2` accepting
//! `br-tollgate tcp dport 2121` saw **zero** packets while the packaged drop
//! still fired. The only fix is to stop that fragment from dropping, so this
//! patches the interface set in place.
//!
//! ## This is a shim and says so
//!
//! The durable fix belongs upstream: the package should not hardcode the
//! bridge name (its own comment admits "an operator who renamed the captive
//! bridge loses this rule"). When upstream ships a name-independent rule this
//! module should be deleted. A package upgrade that replaces the fragment
//! undoes the patch, so provision re-applies it every run — which is exactly
//! why it is idempotent and cheap.

use anyhow::Result;
use tracing::info;

use crate::Router;

/// The packaged fragment that drops the backend API off non-LAN interfaces.
pub const BACKEND_FIREWALL_NFT: &str = "/etc/nftables.d/30-backend-firewall.nft";

/// The packaged fw4 bridge that enforces NoDogSplash marks in `forward`.
pub const NDS_ENFORCE_NFT: &str = "/etc/nftables.d/20-nds-enforce.nft";

/// The bridge name the packaged fragment assumes.
const NDS_SHIPPED_IFACE: &str = "br-lan";

/// The interface set as the package ships it. Order is significant: it must
/// match the file byte-for-byte for `sed` to replace it.
const SHIPPED_SET: &str = r#"{ "br-lan", "br-private", "lo" }"#;

/// The same set with the bridge Archipelago actually creates.
const PATCHED_SET: &str = r#"{ "br-lan", "br-private", "lo", "br-tollgate" }"#;

/// What [`ensure_paid_bridge_can_reach_api`] found.
#[derive(Debug, PartialEq, Eq)]
pub enum ApiReachability {
    /// The packaged fragment does not drop the API from the paid bridge.
    AlreadyCorrect,
    /// The fragment assumed `br-lan`; the paid bridge was added and the
    /// firewall reloaded.
    Patched,
    /// The new package ships `tollgate-nft-interfaces-render`; the guards were
    /// regenerated from `nodogsplash.gatewayinterface` instead of being patched.
    Rendered,
    /// The fragment is absent, so the package no longer ships what this shim
    /// defends against — re-check whether the defect still exists.
    FragmentAbsent,
}

/// Make the TollGate API reachable from the paid bridge, and report what was
/// needed.
///
/// Idempotent: a second call on a patched router issues no write and no reload.
pub fn ensure_paid_bridge_can_reach_api(router: &Router) -> Result<ApiReachability> {
    // #784 (merged upstream, not yet in the released 0.6.0 package) replaces
    // the hardcoded set with defines rendered from the router's own config.
    // Prefer the renderer when it exists; it follows
    // nodogsplash.gatewayinterface, which `nodogsplash::configure` has just
    // set to br-tollgate. A router still on the old package falls through to
    // the literal-set shim below.
    let (_, renderer) = router.run("test -x /usr/local/bin/tollgate-nft-interfaces-render")?;
    if renderer == 0 {
        router.run_ok("/usr/local/bin/tollgate-nft-interfaces-render --reload")?;
        info!(
            "[{}] regenerated the nft guard interfaces from nodogsplash.gatewayinterface \
             (tollgate-nft-interfaces-render)",
            router.host
        );
        return Ok(ApiReachability::Rendered);
    }

    let (exists, code) = router.run(&format!("test -f {BACKEND_FIREWALL_NFT}"))?;
    let _ = exists;
    if code != 0 {
        // Not an error: a future package may stop shipping this fragment. Say
        // so loudly instead of silently claiming the shim is in place.
        info!(
            "[{}] {BACKEND_FIREWALL_NFT} not present — the hardcoded-bridge rule \
             this shim defends against may be gone upstream; re-check whether \
             the portal API is reachable from the paid bridge",
            router.host
        );
        return Ok(ApiReachability::FragmentAbsent);
    }

    let (body, _) = router.run(&format!("cat {BACKEND_FIREWALL_NFT}"))?;
    if body.contains(PATCHED_SET) {
        return Ok(ApiReachability::AlreadyCorrect);
    }
    if !body.contains(SHIPPED_SET) {
        anyhow::bail!(
            "{BACKEND_FIREWALL_NFT} no longer matches the shipped interface set \
             {SHIPPED_SET}, so it cannot be patched safely — the package changed \
             shape and this shim needs re-deriving rather than guessing"
        );
    }

    // `sed` with `|` delimiters: the pattern contains braces, quotes and commas.
    router.run_ok(&format!(
        "sed -i 's|{SHIPPED_SET}|{PATCHED_SET}|g' {BACKEND_FIREWALL_NFT}"
    ))?;

    // The fragment is loaded by fw4, so the rule only changes after a reload.
    router.run_ok("/etc/init.d/firewall reload >/dev/null 2>&1")?;

    info!(
        "[{}] allowed the TollGate API from br-tollgate — the packaged \
         {BACKEND_FIREWALL_NFT} only allowed br-lan/br-private, which dropped \
         the portal's own API call from the paid network",
        router.host
    );
    Ok(ApiReachability::Patched)
}

/// What [`ensure_paid_bridge_is_enforced`] found.
#[derive(Debug, PartialEq, Eq)]
pub enum NdsEnforcement {
    /// The fragment already names the paid bridge.
    AlreadyCorrect,
    /// The fragment assumed `br-lan`; it was rewritten and fw4 reloaded.
    Patched,
    /// The package no longer ships the fragment; re-derive the check.
    FragmentAbsent,
}

/// Point the packaged NoDogSplash forward-enforcement bridge at the bridge
/// Archipelago actually creates.
///
/// ## The defect this exists for
///
/// `tollgate-wrt` ships `/etc/nftables.d/20-nds-enforce.nft`. Every rule in it
/// is scoped `iifname "br-lan"`, but Archipelago's paid network is
/// `br-tollgate`. On 25.12.5 / NoDogSplash 5.0.2 the `inet fw4 forward` chain
/// (priority filter) accepts forwarded packets before the `ip filter` FORWARD
/// chain sees them, so NDS's own rules never run; the packaged bridge is the
/// only enforcement. Scoped to `br-lan`, it matches nothing: the chain's
/// counters stay at 0 and a pre-authenticated client reaches the internet.
///
/// Measured on a GL-MT3000: `ndsctl clients` reported the client
/// `Preauthenticated`, `nds_enforce_forward` counters were 0, and
/// `scripts/paid-client-smoke.sh` failed on "internet blocked before payment".
/// After rewriting the interface set the same command passed.
///
/// Idempotent, and re-applied on every provision because a package upgrade
/// restores the packaged `br-lan` text.
pub fn ensure_paid_bridge_is_enforced(
    router: &Router,
    gateway_interface: &str,
) -> Result<NdsEnforcement> {
    let iface = gateway_interface.trim();
    if iface.is_empty()
        || !iface
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        anyhow::bail!("refusing to render an unsafe bridge name: {iface:?}");
    }
    if iface == NDS_SHIPPED_IFACE {
        return Ok(NdsEnforcement::AlreadyCorrect);
    }

    let (_, exists) = router.run(&format!("test -f {NDS_ENFORCE_NFT}"))?;
    if exists != 0 {
        info!(
            "[{}] {NDS_ENFORCE_NFT} not present — the package no longer gates via \
             this fragment; re-check how pre-authenticated traffic is blocked",
            router.host
        );
        return Ok(NdsEnforcement::FragmentAbsent);
    }

    let (body, _) = router.run(&format!("cat {NDS_ENFORCE_NFT}"))?;
    let wrong = format!("iifname \"{NDS_SHIPPED_IFACE}\"");
    let right = format!("iifname \"{iface}\"");
    if !body.contains(&wrong) {
        // Already rewritten (or the package changed shape). Only claim success
        // when the paid bridge is actually named; otherwise say we do not know
        // rather than silently leaving the gate open.
        if body.contains(&right) {
            return Ok(NdsEnforcement::AlreadyCorrect);
        }
        anyhow::bail!(
            "{NDS_ENFORCE_NFT} names neither {NDS_SHIPPED_IFACE} nor {iface}; \
             the package changed shape and this shim needs re-deriving"
        );
    }

    // Replace the whole interface token, including `oifname != "br-lan"`.
    router.run_ok(&format!(
        "sed -i 's/\"{NDS_SHIPPED_IFACE}\"/\"{iface}\"/g' {NDS_ENFORCE_NFT}"
    ))?;
    router.run_ok("/etc/init.d/firewall reload >/dev/null 2>&1")?;

    // Do not report success on a write alone: the chain must now name the
    // paid bridge. A failed reload leaves the old rules live.
    let (chain, _) = router.run("nft list chain inet fw4 nds_enforce_forward 2>/dev/null")?;
    if !chain.contains(&right) {
        anyhow::bail!(
            "rewrote {NDS_ENFORCE_NFT} but inet fw4 nds_enforce_forward does not name \
             {iface}; the firewall reload did not take effect"
        );
    }

    info!(
        "[{}] scoped NoDogSplash forward enforcement to {iface} — the packaged \
         fragment only matched {NDS_SHIPPED_IFACE}, so pre-authenticated clients \
         reached the internet",
        router.host
    );
    Ok(NdsEnforcement::Patched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn patches_the_shipped_set_and_reloads_the_firewall() {
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let router = Router::scripted(move |cmd| {
            log.lock().unwrap().push(cmd.to_string());
            // Old package: no renderer, so the literal-set shim runs.
            if cmd.contains("test -x") {
                return Ok((String::new(), 1));
            }
            if cmd.contains("test -f") {
                return Ok((String::new(), 0));
            }
            if cmd.starts_with("cat ") {
                return Ok((
                    format!("meta iifname != {SHIPPED_SET} tcp dport 2121 counter drop\n"),
                    0,
                ));
            }
            Ok((String::new(), 0))
        });

        let outcome = ensure_paid_bridge_can_reach_api(&router).unwrap();
        assert_eq!(outcome, ApiReachability::Patched);

        let cmds = seen.lock().unwrap().clone();
        assert!(
            cmds.iter()
                .any(|c| c.contains("sed -i") && c.contains("br-tollgate")),
            "must add br-tollgate to the interface set: {cmds:#?}"
        );
        assert!(
            cmds.iter()
                .any(|c| c.contains("/etc/init.d/firewall reload")),
            "must reload the firewall or the patched fragment is never loaded: {cmds:#?}"
        );
    }

    #[test]
    fn an_already_patched_router_is_not_written_to() {
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let router = Router::scripted(move |cmd| {
            log.lock().unwrap().push(cmd.to_string());
            if cmd.contains("test -x") {
                return Ok((String::new(), 1));
            }
            if cmd.contains("test -f") {
                return Ok((String::new(), 0));
            }
            if cmd.starts_with("cat ") {
                return Ok((format!("iifname != {PATCHED_SET} drop\n"), 0));
            }
            Ok((String::new(), 0))
        });

        assert_eq!(
            ensure_paid_bridge_can_reach_api(&router).unwrap(),
            ApiReachability::AlreadyCorrect
        );
        let cmds = seen.lock().unwrap().clone();
        assert!(
            !cmds.iter().any(|c| c.contains("sed -i")),
            "must not rewrite an already-correct fragment: {cmds:#?}"
        );
        assert!(
            !cmds.iter().any(|c| c.contains("firewall reload")),
            "must not reload a firewall it did not change: {cmds:#?}"
        );
    }

    #[test]
    fn a_changed_fragment_fails_instead_of_guessing() {
        let router = Router::scripted(|cmd| {
            if cmd.contains("test -x") {
                return Ok((String::new(), 1));
            }
            if cmd.contains("test -f") {
                return Ok((String::new(), 0));
            }
            if cmd.starts_with("cat ") {
                // Package reshaped the rule: different set, same intent.
                return Ok((
                    "iifname != { \"br-lan\", \"br-guest\", \"lo\" } drop\n".into(),
                    0,
                ));
            }
            Ok((String::new(), 0))
        });

        let err = ensure_paid_bridge_can_reach_api(&router)
            .expect_err("an unrecognised fragment must not be patched blind");
        assert!(
            err.to_string().contains("no longer matches"),
            "error should say the shape changed, got: {err}"
        );
    }

    #[test]
    fn an_absent_fragment_is_reported_not_treated_as_success() {
        let router = Router::scripted(|cmd| {
            if cmd.contains("test -x") {
                return Ok((String::new(), 1));
            }
            if cmd.contains("test -f") {
                return Ok((String::new(), 1)); // gone
            }
            Ok((String::new(), 0))
        });
        assert_eq!(
            ensure_paid_bridge_can_reach_api(&router).unwrap(),
            ApiReachability::FragmentAbsent
        );
    }

    #[test]
    fn scopes_the_nds_enforcement_bridge_to_the_paid_bridge() {
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let router = Router::scripted(move |cmd| {
            log.lock().unwrap().push(cmd.to_string());
            if cmd.contains("test -f") {
                return Ok((String::new(), 0));
            }
            if cmd.starts_with("cat ") {
                return Ok((
                    "meta iifname \"br-lan\" meta mark & 0x30000 == 0x10000 drop\n\
                     meta iifname \"br-lan\" oifname != \"br-lan\" reject\n"
                        .into(),
                    0,
                ));
            }
            if cmd.contains("nft list chain") {
                return Ok(("meta iifname \"br-tollgate\" drop\n".into(), 0));
            }
            Ok((String::new(), 0))
        });

        assert_eq!(
            ensure_paid_bridge_is_enforced(&router, "br-tollgate").unwrap(),
            NdsEnforcement::Patched
        );
        let cmds = seen.lock().unwrap().clone();
        assert!(
            cmds.iter()
                .any(|c| c.contains("sed -i") && c.contains("br-tollgate")),
            "must rewrite the interface token: {cmds:#?}"
        );
        assert!(
            cmds.iter()
                .any(|c| c.contains("/etc/init.d/firewall reload")),
            "must reload the firewall: {cmds:#?}"
        );
    }

    #[test]
    fn an_already_scoped_nds_bridge_is_not_written_to() {
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let router = Router::scripted(move |cmd| {
            log.lock().unwrap().push(cmd.to_string());
            if cmd.contains("test -f") {
                return Ok((String::new(), 0));
            }
            if cmd.starts_with("cat ") {
                return Ok(("meta iifname \"br-tollgate\" drop\n".into(), 0));
            }
            Ok((String::new(), 0))
        });

        assert_eq!(
            ensure_paid_bridge_is_enforced(&router, "br-tollgate").unwrap(),
            NdsEnforcement::AlreadyCorrect
        );
        let cmds = seen.lock().unwrap().clone();
        assert!(!cmds.iter().any(|c| c.contains("sed -i")), "{cmds:#?}");
        assert!(
            !cmds.iter().any(|c| c.contains("firewall reload")),
            "{cmds:#?}"
        );
    }

    #[test]
    fn a_reload_that_did_not_take_effect_is_an_error() {
        let router = Router::scripted(|cmd| {
            if cmd.contains("test -f") {
                return Ok((String::new(), 0));
            }
            if cmd.starts_with("cat ") {
                return Ok(("meta iifname \"br-lan\" drop\n".into(), 0));
            }
            if cmd.contains("nft list chain") {
                // Reload silently failed; the old rules are still live.
                return Ok(("meta iifname \"br-lan\" drop\n".into(), 0));
            }
            Ok((String::new(), 0))
        });

        let err = ensure_paid_bridge_is_enforced(&router, "br-tollgate")
            .expect_err("must not claim success without the live chain");
        assert!(err.to_string().contains("does not name"), "got: {err}");
    }

    #[test]
    fn refuses_an_unsafe_bridge_name() {
        let router = Router::scripted(|_| Ok((String::new(), 0)));
        let err = ensure_paid_bridge_is_enforced(&router, "br-tollgate\";reboot")
            .expect_err("must refuse shell metacharacters");
        assert!(err.to_string().contains("unsafe"), "got: {err}");
    }

    #[test]
    fn prefers_the_upstream_renderer_when_present() {
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let router = Router::scripted(move |cmd| {
            log.lock().unwrap().push(cmd.to_string());
            if cmd.contains("test -x") {
                return Ok((String::new(), 0));
            }
            Ok((String::new(), 0))
        });

        assert_eq!(
            ensure_paid_bridge_can_reach_api(&router).unwrap(),
            ApiReachability::Rendered
        );
        let cmds = seen.lock().unwrap().clone();
        assert!(
            cmds.iter()
                .any(|c| c.contains("tollgate-nft-interfaces-render --reload")),
            "must run the upstream renderer: {cmds:#?}"
        );
        assert!(
            !cmds.iter().any(|c| c.contains("sed -i")),
            "must not patch a fragment when the renderer is available: {cmds:#?}"
        );
    }
}
