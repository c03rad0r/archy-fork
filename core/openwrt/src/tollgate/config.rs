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
    router.uci_apply(
        "tollgate",
        &[
            ("tollgate.main", "tollgate"),
            ("tollgate.main.enabled", if cfg.enabled { "1" } else { "0" }),
            ("tollgate.main.metric", "milliseconds"),
            ("tollgate.main.step_size", &cfg.step_size_ms.to_string()),
            ("tollgate.main.min_steps", &cfg.min_steps.to_string()),
            ("tollgate.main.price_per_step", &cfg.price_sats.to_string()),
            ("tollgate.main.currency", "sat"),
            ("tollgate.main.mint_url", &cfg.mint_url),
        ],
    )?;
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
