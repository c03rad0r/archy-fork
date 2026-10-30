use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::Router;

/// One accepted Cashu mint, mirroring the TollGate schema's
/// `accepted_mints[]` object. Missing fields take the same defaults as the
/// package (so a partial row from the UI is valid), and `price_unit` is
/// normalised to the schema's `sat` spelling.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MintConfig {
    pub url: String,
    pub min_balance: u64,
    pub balance_tolerance_percent: u64,
    pub payout_interval_seconds: u64,
    pub min_payout_amount: u64,
    pub price_per_step: u64,
    pub price_unit: String,
    pub purchase_min_steps: u64,
}

impl Default for MintConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            min_balance: 64,
            balance_tolerance_percent: 10,
            payout_interval_seconds: 60,
            min_payout_amount: 128,
            price_per_step: 1,
            price_unit: "sat".to_string(),
            purchase_min_steps: 1,
        }
    }
}

/// TollGate provisioning parameters.
///
/// `mint_url` must be the externally-reachable URL of the Archy Cashu mint —
/// TollGate customers connect from outside the Archy node's loopback, so
/// localhost URLs will not work. Resolve this from the running mint app before
/// calling `provision`. `accepted_mints`, when present, supersedes the
/// single-mint fields and is written as the complete list (add/remove).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TollGateConfig {
    /// SSID name for the pay-as-you-go network.
    pub ssid: String,
    /// Externally-reachable URL of the Archy Cashu mint.
    pub mint_url: String,
    /// Price in satoshis per `step_size` interval.
    pub price_sats: u64,
    /// Step size in milliseconds (default: 60000 = 1 minute), or bytes when
    /// `metric` is `bytes`.
    pub step_size_ms: u64,
    /// Metering metric: `bytes` or `milliseconds`. `None` preserves whatever
    /// the daemon already has; a fresh install defaults to milliseconds.
    pub metric: Option<String>,
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
    /// Name of the private management SSID. Empty keeps whatever the router
    /// already broadcasts (the TollGate package's `<user>-<serial>` default).
    pub private_ssid: String,
    /// Passphrase for the private management SSID. `None` keeps the existing
    /// key. The schema bounds it to 8–63 characters; the daemon validates it.
    pub private_key: Option<String>,
    /// Complete accepted-mint list. `None` keeps the single-mint merge path
    /// for older callers; `Some` replaces the list (add/remove in the UI).
    pub accepted_mints: Option<Vec<MintConfig>>,
}

impl Default for TollGateConfig {
    fn default() -> Self {
        Self {
            ssid: "archipelago".to_string(),
            mint_url: String::new(), // must be set by caller from the running mint app
            price_sats: 10,
            step_size_ms: 60_000,
            metric: None,
            min_steps: 1,
            enabled: true,
            payout_address: None,
            private_ssid: "noderunner".to_string(),
            private_key: None,
            accepted_mints: None,
        }
    }
}

/// Read the metric from the daemon's existing config.json, if present.
fn daemon_metric(router: &Router) -> Option<String> {
    let existing = router
        .run_ok("cat /etc/tollgate/config.json 2>/dev/null || echo '{}'")
        .ok()?;
    let doc: serde_json::Value = serde_json::from_str(existing.trim()).ok()?;
    doc.get("metric")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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
    let first_mint = cfg.accepted_mints.as_ref().and_then(|m| m.first());
    let mint_url = first_mint
        .map(|m| m.url.clone())
        .unwrap_or_else(|| cfg.mint_url.clone());
    let min_steps = first_mint
        .map(|m| m.purchase_min_steps)
        .unwrap_or(cfg.min_steps as u64)
        .to_string();
    let price_sats = first_mint
        .map(|m| m.price_per_step)
        .unwrap_or(cfg.price_sats)
        .to_string();
    let metric = cfg
        .metric
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_string)
        .or_else(|| daemon_metric(router))
        .unwrap_or_else(|| "milliseconds".to_string());

    let mut pairs = vec![
        ("tollgate.main", "tollgate"),
        ("tollgate.main.enabled", if cfg.enabled { "1" } else { "0" }),
        ("tollgate.main.metric", metric.as_str()),
        ("tollgate.main.step_size", step_size.as_str()),
        ("tollgate.main.min_steps", min_steps.as_str()),
        ("tollgate.main.price_per_step", price_sats.as_str()),
        ("tollgate.main.currency", "sat"),
        ("tollgate.main.mint_url", mint_url.as_str()),
    ];
    // Status-display only (see doc comment above) — only written when the
    // caller actually supplied one, so a reconfigure that doesn't touch
    // payout leaves whatever's already there alone.
    if let Some(addr) = &cfg.payout_address {
        pairs.push(("tollgate.main.payout_address", addr));
    }
    if !cfg.private_ssid.trim().is_empty() {
        pairs.push(("tollgate.main.private_ssid", cfg.private_ssid.trim()));
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

    match cfg
        .metric
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
    {
        Some(metric) => doc["metric"] = serde_json::json!(metric),
        None => {
            if doc
                .get("metric")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .is_none()
            {
                doc["metric"] = serde_json::json!("milliseconds");
            }
        }
    }
    doc["step_size"] = serde_json::json!(cfg.step_size_ms);

    // The private management SSID/key are first-class schema fields. Empty
    // means "keep the router's existing value", which is what the daemon's
    // own migration uses.
    if !cfg.private_ssid.trim().is_empty() {
        doc["private_ssid"] = serde_json::json!(cfg.private_ssid.trim());
    }
    if let Some(key) = cfg
        .private_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        doc["private_key"] = serde_json::json!(key);
        if doc
            .get("private_encryption")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .is_empty()
        {
            doc["private_encryption"] = serde_json::json!("psk2+ccmp");
        }
    }

    if let Some(list) = &cfg.accepted_mints {
        // The UI sends the complete list (add/remove). Normalise the legacy
        // `sats` spelling the same way the daemon's migration does.
        let mut mints = list.clone();
        for mint in &mut mints {
            if mint.price_unit.trim().is_empty() || mint.price_unit.eq_ignore_ascii_case("sats") {
                mint.price_unit = "sat".to_string();
            }
        }
        doc["accepted_mints"] = serde_json::json!(mints);
    } else {
        // Update the configured mint in place. Replacing the whole array used
        // to drop every other accepted mint and every per-mint setting the
        // operator had configured; the portal advertises the daemon's accepted
        // set, so that was a money-path data loss, not just a display bug.
        let mut mints = doc
            .get("accepted_mints")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let wanted_url = cfg.mint_url.trim_end_matches('/');
        let mut updated = false;
        for mint in mints.iter_mut() {
            let mint_url = mint
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim_end_matches('/');
            if mint_url == wanted_url {
                mint["price_per_step"] = serde_json::json!(cfg.price_sats);
                mint["price_unit"] = serde_json::json!("sats");
                mint["purchase_min_steps"] = serde_json::json!(cfg.min_steps);
                updated = true;
                break;
            }
        }
        if !updated {
            mints.push(serde_json::json!({
                "url": cfg.mint_url,
                "min_balance": 64,
                "balance_tolerance_percent": 10,
                "payout_interval_seconds": 60,
                "min_payout_amount": 128,
                "price_per_step": cfg.price_sats,
                "price_unit": "sats",
                "purchase_min_steps": cfg.min_steps,
            }));
        }
        doc["accepted_mints"] = serde_json::json!(mints);
    }

    write_daemon_doc(router, &doc)
}

/// Write a full `/etc/tollgate/config.json` document without dropping fields
/// this project does not manage. Prefer the package's schema CLI; fall back
/// to a file upload for older packages or a stale socket.
fn write_daemon_doc(router: &Router, doc: &serde_json::Value) -> Result<()> {
    let json_str = serde_json::to_string_pretty(doc).context("serialize config.json")?;

    let tmp = "/tmp/tollgate-config.json";
    router
        .upload_file(tmp, json_str.as_bytes())
        .context("upload temporary config.json")?;
    let schema_ok = router
        .run("tollgate --json config schema 2>/dev/null")
        .map(|(_, code)| code == 0)
        .unwrap_or(false);
    if schema_ok {
        match router.run("tollgate --json config save \"$(cat /tmp/tollgate-config.json)\" 2>&1") {
            Ok((_, 0)) => {
                let _ = router.run_ok("rm -f /tmp/tollgate-config.json");
                return Ok(());
            }
            Ok((out, code)) => tracing::warn!(
                "[{}] tollgate config save failed ({}): {} — falling back to file upload",
                router.host,
                code,
                out.trim()
            ),
            Err(err) => tracing::warn!(
                "[{}] tollgate config save could not run: {} — falling back to file upload",
                router.host,
                err
            ),
        }
    }

    router
        .upload_file("/etc/tollgate/config.json", json_str.as_bytes())
        .context("upload /etc/tollgate/config.json")?;
    let _ = router.run_ok("rm -f /tmp/tollgate-config.json");
    Ok(())
}

/// Remove the named mints from `accepted_mints` while preserving every other
/// daemon setting (metric, profit share, upstream tuning, ...). Returns the
/// remaining URLs. Refuses to empty the list.
///
/// This deliberately does not touch `wallet.db` or the ecash directory:
/// removing a mint from the accepted set stops new payments there; it does not
/// delete funds the wallet already holds.
pub fn remove_mints(router: &Router, remove: &[String]) -> Result<Vec<String>> {
    let existing = router.run_ok("cat /etc/tollgate/config.json 2>/dev/null || echo '{}'")?;
    let mut doc: serde_json::Value =
        serde_json::from_str(existing.trim()).unwrap_or_else(|_| serde_json::json!({}));
    let Some(mints) = doc.get_mut("accepted_mints").and_then(|v| v.as_array_mut()) else {
        anyhow::bail!("config.json has no accepted_mints list");
    };
    let before = mints.len();
    mints.retain(|mint| {
        let url = mint.get("url").and_then(|v| v.as_str()).unwrap_or("");
        !remove.iter().any(|remove_url| {
            remove_url
                .trim_end_matches('/')
                .eq_ignore_ascii_case(url.trim_end_matches('/'))
        })
    });
    if mints.is_empty() {
        anyhow::bail!("refusing to remove every accepted mint");
    }
    if mints.len() == before {
        return Ok(mints
            .iter()
            .filter_map(|m| m.get("url").and_then(|v| v.as_str()).map(str::to_string))
            .collect());
    }
    let remaining: Vec<String> = mints
        .iter()
        .filter_map(|m| m.get("url").and_then(|v| v.as_str()).map(str::to_string))
        .collect();
    write_daemon_doc(router, &doc)?;
    Ok(remaining)
}

/// Update only the daemon's `private_ssid` field, preserving every other
/// setting in `/etc/tollgate/config.json`.
pub fn set_private_ssid(router: &Router, ssid: &str) -> Result<()> {
    let existing = router.run_ok("cat /etc/tollgate/config.json 2>/dev/null || echo '{}'")?;
    let mut doc: serde_json::Value =
        serde_json::from_str(existing.trim()).unwrap_or_else(|_| serde_json::json!({}));
    doc["private_ssid"] = serde_json::json!(ssid);
    write_daemon_doc(router, &doc)
}

/// Mirror the daemon's real config into the `tollgate.main.*` UCI fields.
///
/// These fields are a display/detection mirror only; the daemon reads
/// `/etc/tollgate/config.json`. A router installed by another path (or renamed
/// without a full re-provision) can have no mirror at all, which used to make
/// the dashboard report `Disabled / 0 sat / 0 ms`. Callers that already know
/// whether the service should run pass `enabled`.
pub fn sync_uci_from_daemon(router: &Router, enabled: bool) -> Result<()> {
    let existing = router.run_ok("cat /etc/tollgate/config.json 2>/dev/null || echo '{}'")?;
    let doc: serde_json::Value =
        serde_json::from_str(existing.trim()).unwrap_or_else(|_| serde_json::json!({}));
    let first_mint = doc
        .get("accepted_mints")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first());

    let metric = doc
        .get("metric")
        .and_then(|v| v.as_str())
        .unwrap_or("milliseconds");
    let step = doc
        .get("step_size")
        .and_then(|v| v.as_u64())
        .unwrap_or(60_000)
        .to_string();
    let price = first_mint
        .and_then(|m| m.get("price_per_step"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
        .to_string();
    let currency = first_mint
        .and_then(|m| m.get("price_unit"))
        .and_then(|v| v.as_str())
        .unwrap_or("sat");
    let min_steps = first_mint
        .and_then(|m| m.get("purchase_min_steps"))
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .to_string();
    let mint_url = first_mint
        .and_then(|m| m.get("url"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let private_ssid = doc
        .get("private_ssid")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut pairs = vec![
        ("tollgate.main", "tollgate"),
        ("tollgate.main.enabled", if enabled { "1" } else { "0" }),
        ("tollgate.main.metric", metric),
        ("tollgate.main.step_size", step.as_str()),
        ("tollgate.main.price_per_step", price.as_str()),
        ("tollgate.main.currency", currency),
        ("tollgate.main.min_steps", min_steps.as_str()),
        ("tollgate.main.mint_url", mint_url),
    ];
    if !private_ssid.trim().is_empty() {
        pairs.push(("tollgate.main.private_ssid", private_ssid));
    }
    router.uci_apply("tollgate", &pairs)?;
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
    validate_payout_address(address)?;

    let existing = router.run_ok("cat /etc/tollgate/identities.json 2>/dev/null || echo '{}'")?;
    let mut doc = parse_identities(&existing)?;

    merge_payout_identity(&mut doc, address)?;

    let json_str = serde_json::to_string_pretty(&doc).context("serialize identities.json")?;
    router
        .upload_file("/etc/tollgate/identities.json", json_str.as_bytes())
        .context("upload /etc/tollgate/identities.json")?;
    Ok(())
}

fn parse_identities(existing: &str) -> Result<serde_json::Value> {
    serde_json::from_str(existing.trim()).context(
        "parse existing /etc/tollgate/identities.json; refusing to overwrite malformed identity data",
    )
}

/// Reject malformed values before provisioning changes anything on the
/// router. A payout typo otherwise remains dormant until the threshold is
/// reached, when the operator discovers that settlement cannot resolve.
pub fn validate_payout_address(address: &str) -> Result<()> {
    let (name, domain) = address
        .split_once('@')
        .context("Lightning address must look like name@example.com")?;
    if name.is_empty()
        || domain.is_empty()
        || domain.contains('@')
        || address.chars().any(char::is_whitespace)
    {
        anyhow::bail!("Lightning address must look like name@example.com");
    }
    Ok(())
}

fn merge_payout_identity(doc: &mut serde_json::Value, address: &str) -> Result<()> {
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{merge_payout_identity, parse_identities, validate_payout_address};

    #[test]
    fn payout_merge_changes_only_owner_address() {
        let mut doc = serde_json::json!({
            "config_version": "v0.0.1",
            "owned_identities": [{ "name": "merchant", "privatekey": "keep-secret" }],
            "public_identities": [
                { "name": "owner", "pubkey": "not currently used", "lightning_address": "old@example.com" },
                { "name": "upstream", "lightning_address": "keep@example.com" }
            ]
        });
        let before_owned = doc["owned_identities"].clone();
        let before_other = doc["public_identities"][1].clone();

        merge_payout_identity(&mut doc, "operator@example.com").unwrap();

        assert_eq!(doc["owned_identities"], before_owned);
        assert_eq!(doc["public_identities"][1], before_other);
        assert_eq!(
            doc["public_identities"][0]["lightning_address"],
            "operator@example.com"
        );
    }

    #[test]
    fn payout_merge_can_create_missing_owner() {
        let mut doc = serde_json::json!({ "public_identities": [] });
        merge_payout_identity(&mut doc, "operator@example.com").unwrap();
        assert_eq!(doc["public_identities"][0]["name"], "owner");
        assert_eq!(
            doc["public_identities"][0]["lightning_address"],
            "operator@example.com"
        );
    }

    #[test]
    fn payout_address_validation_rejects_typographical_failures() {
        assert!(validate_payout_address("operator@example.com").is_ok());
        for invalid in [
            "",
            "operator",
            "@example.com",
            "operator@",
            "a@b@c",
            "a b@example.com",
        ] {
            assert!(
                validate_payout_address(invalid).is_err(),
                "accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn malformed_identity_data_is_never_replaced() {
        let err = parse_identities("{ truncated").unwrap_err();
        assert!(err
            .to_string()
            .contains("refusing to overwrite malformed identity data"));
    }
}
