use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::Router;

/// TollGate provisioning parameters.
///
/// `mint_url` must be the externally-reachable URL of the Archy Cashu mint —
/// TollGate customers connect from outside the Archy node's loopback, so
/// localhost URLs will not work. Resolve this from the running mint app before
/// calling `provision`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TollGateConfig {
    /// SSID name for the pay-as-you-go network.
    pub ssid: String,
    /// Externally-reachable URL of the Archy Cashu mint.
    pub mint_url: String,
    /// Price in satoshis per `step_size` interval.
    pub price_sats: u64,
    /// Step size in milliseconds (default: 60000 = 1 minute).
    pub step_size_ms: u64,
    /// Minimum steps a customer must purchase at once.
    pub min_steps: u32,
    /// Whether the TollGate service should be running and enabled at boot.
    pub enabled: bool,
    /// Operator's own Lightning address for the daemon's built-in payout
    /// (the "owner" entry in `/etc/tollgate/identities.json`, `profit_share`
    /// weight 0.79 in the upstream default). `None` leaves whatever is
    /// already on the router untouched — which, on a router whose TollGate
    /// wasn't provisioned through this project, is an unmodified upstream
    /// placeholder nobody actually controls (confirmed live against
    /// archy-x250-pa3 2026-09-07: shipped as `tollgate@minibits.cash`).
    pub payout_address: Option<String>,
}

impl Default for TollGateConfig {
    fn default() -> Self {
        Self {
            ssid: "archipelago".to_string(),
            mint_url: String::new(), // must be set by caller from the running mint app
            price_sats: 10,
            step_size_ms: 60_000,
            min_steps: 1,
            enabled: true,
            payout_address: None,
        }
    }
}

/// Write TollGate UCI configuration and commit.
///
/// `tollgate-wrt` never reads UCI — see `apply_daemon_config` below for the
/// config it actually consumes. These `tollgate.main.*` keys exist only for
/// this project's own status display / detection probes (`uci get
/// tollgate.main.enabled` etc.); changing pricing or the mint here has no
/// effect on what the daemon advertises or accepts.
pub fn apply(router: &Router, cfg: &TollGateConfig) -> Result<()> {
    let step_size = cfg.step_size_ms.to_string();
    let min_steps = cfg.min_steps.to_string();
    let price_sats = cfg.price_sats.to_string();

    let mut pairs = vec![
        ("tollgate.main", "tollgate"),
        ("tollgate.main.enabled", if cfg.enabled { "1" } else { "0" }),
        ("tollgate.main.metric", "milliseconds"),
        ("tollgate.main.step_size", step_size.as_str()),
        ("tollgate.main.min_steps", min_steps.as_str()),
        ("tollgate.main.price_per_step", price_sats.as_str()),
        ("tollgate.main.currency", "sat"),
        ("tollgate.main.mint_url", &cfg.mint_url),
    ];
    // Status-display only (see doc comment above) — only written when the
    // caller actually supplied one, so a reconfigure that doesn't touch
    // payout leaves whatever's already there alone.
    if let Some(addr) = &cfg.payout_address {
        pairs.push(("tollgate.main.payout_address", addr));
    }
    router.uci_apply("tollgate", &pairs)?;
    Ok(())
}

/// Write the config `tollgate-wrt` actually reads: `/etc/tollgate/config.json`
/// (schema `v0.0.6`/`v0.0.7`, see `config_manager` in the upstream Go source).
///
/// Merges into whatever config.json already exists (the daemon writes a
/// default on first boot) rather than overwriting it wholesale — fields this
/// project doesn't manage (`profit_share`, `upstream_detector`,
/// `upstream_session_manager`/`chandler`, `relays`, ...) must survive
/// re-provisioning.
///
/// Must run before the daemon is (re)started — it only reads this file at
/// startup, it does not hot-reload.
pub fn apply_daemon_config(router: &Router, cfg: &TollGateConfig) -> Result<()> {
    let existing = router.run_ok("cat /etc/tollgate/config.json 2>/dev/null || echo '{}'")?;
    let mut doc: serde_json::Value =
        serde_json::from_str(existing.trim()).unwrap_or_else(|_| serde_json::json!({}));

    doc["metric"] = serde_json::json!("milliseconds");
    doc["step_size"] = serde_json::json!(cfg.step_size_ms);
    doc["accepted_mints"] = serde_json::json!([{
        "url": cfg.mint_url,
        "min_balance": 64,
        "balance_tolerance_percent": 10,
        "payout_interval_seconds": 60,
        "min_payout_amount": 128,
        "price_per_step": cfg.price_sats,
        "price_unit": "sats",
        "purchase_min_steps": cfg.min_steps,
    }]);

    let json_str = serde_json::to_string_pretty(&doc).context("serialize config.json")?;
    router
        .upload_file("/etc/tollgate/config.json", json_str.as_bytes())
        .context("upload /etc/tollgate/config.json")?;
    Ok(())
}

/// Set the operator's own payout Lightning address in
/// `/etc/tollgate/identities.json` — the "owner" entry under
/// `public_identities` (`profit_share` weight 0.79 in the upstream default;
/// the other entries there are revenue-share addresses for the upstream
/// project's own maintainers and must never be touched by this function).
///
/// No-op when `payout_address` is `None` — the UI only sends one when the
/// operator has actually filled the field in, so a reconfigure of price/mint
/// alone never overwrites this. Merges into whatever identities.json already
/// exists (same reasoning as `apply_daemon_config`: `owned_identities` holds
/// the merchant's own private key and must survive untouched); creates an
/// "owner" entry if none exists yet rather than erroring, since a router
/// whose TollGate wasn't provisioned through this project may have any
/// upstream-default shape here.
///
/// Must run before the daemon restart in `restart_services` — like
/// `config.json`, `tollgate-wrt` only reads `identities.json` at startup.
pub fn apply_payout_identity(router: &Router, payout_address: Option<&str>) -> Result<()> {
    let Some(address) = payout_address else {
        return Ok(());
    };

    let existing = router.run_ok("cat /etc/tollgate/identities.json 2>/dev/null || echo '{}'")?;
    let mut doc: serde_json::Value =
        serde_json::from_str(existing.trim()).unwrap_or_else(|_| serde_json::json!({}));

    let identities = doc
        .as_object_mut()
        .context("identities.json root is not a JSON object")?
        .entry("public_identities")
        .or_insert_with(|| serde_json::json!([]));
    let identities = identities
        .as_array_mut()
        .context("identities.json public_identities is not an array")?;

    match identities
        .iter_mut()
        .find(|i| i.get("name").and_then(|n| n.as_str()) == Some("owner"))
    {
        Some(owner) => {
            owner["lightning_address"] = serde_json::json!(address);
        }
        None => {
            identities.push(serde_json::json!({
                "name": "owner",
                "pubkey": "not currently used",
                "lightning_address": address,
            }));
        }
    }

    let json_str = serde_json::to_string_pretty(&doc).context("serialize identities.json")?;
    router
        .upload_file("/etc/tollgate/identities.json", json_str.as_bytes())
        .context("upload /etc/tollgate/identities.json")?;
    Ok(())
}
