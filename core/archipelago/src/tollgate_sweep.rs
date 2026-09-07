use std::path::Path;

use anyhow::{Context, Result};
use archipelago_openwrt::router::Router;
use tracing::{info, warn};

use crate::network::router as net_router;
use crate::wallet::ecash;

/// Pull whatever TollGate has collected in its on-router Cashu wallet into
/// this node's own wallet.
///
/// `tollgate-wrt` keeps a completely separate Cashu wallet on the router
/// (`/etc/tollgate/wallet.db`) — customer payments land there, never in this
/// node's wallet directly. Its own Lightning auto-payout is configured
/// independently in `/etc/tollgate/identities.json`, which is easy to leave
/// pointed at the wrong (or a placeholder) address and easy to lose track of.
/// This sweep sidesteps that entirely: periodically drain the router's
/// TollGate wallet to Cashu tokens (`tollgate wallet drain cashu`, over SSH)
/// and receive them straight into the local wallet via the same path the
/// "Receive ecash" UI uses.
///
/// Returns the total sats swept in (0 if there was nothing to do, including
/// when no router is configured or it doesn't have TollGate installed).
///
/// # KNOWN BROKEN as of 2026-09-07 — do not "fix" by adding `--json` without
/// reading the rest of this comment first.
///
/// Confirmed live against archy-x250-pa3, two stacked bugs in the upstream
/// `tollgate` CLI, not in this function:
///
/// 1. **This call never actually drains anything.** `tollgate wallet drain
///    cashu` (no flags — what this function runs) prints an interactive
///    `Are you sure? (y/N)` confirmation and reads stdin for the answer.
///    `Router::run` executes over SSH with no PTY and empty stdin, so it
///    always reads EOF, defaults to "N", and prints "Operation cancelled." —
///    **with exit code 0**. The `drain_code != 0` check below can never catch
///    this, so every single tick silently falls through to "no `Token:`
///    lines found" → `Ok(0)`. No error, no log line (even at `warn!`), just
///    quiet total inaction, forever. This has presumably never swept a
///    single sat on any node.
///
/// 2. **The obvious fix is worse.** `tollgate --json wallet drain cashu`
///    *does* skip the confirmation prompt — but confirmed live: when the
///    wallet's internal per-mint registry holds more than one entry for what
///    is really the same mint (here: `https://mint.minibits.cash/Bitcoin` vs.
///    a stale `.../Bitcoin/` — leftover from before the trailing-slash
///    `mint_url` fix elsewhere in this codebase; `wallet.db` still had a
///    proof/registry entry keyed under the old slashed URL even after
///    `config.json` was corrected), the CLI appears to complete a real swap
///    against the *good* entry — spending and irreversibly consuming the
///    original proofs, per how Cashu swaps work — then hits the second,
///    empty, stale-keyed entry, reports the whole command as
///    `"success": false`, and **never prints or persists the resulting
///    token anywhere** (checked every location its own "will be saved to a
///    file" warning implies: `/etc/tollgate/ecash/`, `/root`, `/tmp`,
///    nothing). Balance went from 50 sats to 0 across that one call. The
///    funds are gone — there is no undo once a swap is submitted to the
///    mint.
///
/// Do not wire `--json` into this function until upstream fixes partial
/// per-mint failure handling in `drain cashu` to preserve/return whatever it
/// already successfully drained. Until then, the current silent-no-op
/// behavior, while useless, is at least safe.
pub async fn sweep_once(data_dir: &Path) -> Result<u64> {
    let cfg = net_router::load_router_config(data_dir).await?;
    if !cfg.configured {
        return Ok(0);
    }
    let ssh_user = cfg.username.clone().unwrap_or_else(|| "root".to_string());
    let ssh_password = cfg.password.clone().unwrap_or_default();

    let router = Router::connect_password(&cfg.address, 22, &ssh_user, &ssh_password)
        .context("connect to router for tollgate wallet sweep")?;

    // `tollgate` CLI missing (no TollGate installed here) is a normal,
    // expected case, not an error — just nothing to sweep.
    let (balance_out, code) = router.run("tollgate wallet balance 2>/dev/null")?;
    if code != 0 {
        return Ok(0);
    }
    let balance_sats = balance_out
        .lines()
        .find_map(|l| l.trim().strip_prefix("balance_sats:"))
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(0);
    if balance_sats == 0 {
        return Ok(0);
    }

    let (drain_out, drain_code) = router.run("tollgate wallet drain cashu 2>&1")?;
    if drain_code != 0 {
        anyhow::bail!("tollgate wallet drain cashu failed: {}", drain_out.trim());
    }

    // The CLI has no machine-readable output mode; pull tokens out of its
    // "  Token: cashuB..." lines.
    let mut received_total = 0u64;
    for line in drain_out.lines() {
        let Some(token) = line.trim().strip_prefix("Token:") else {
            continue;
        };
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        match ecash::receive_token(data_dir, token).await {
            Ok(amount) => {
                received_total += amount;
                info!(
                    amount_sats = amount,
                    "swept TollGate ecash into local wallet"
                );
            }
            Err(e) => {
                // The token is still in this log line if this happens — not
                // silently lost, just needs manual `wallet.ecash-receive`.
                warn!(error = %e, token, "failed to receive swept TollGate token");
            }
        }
    }

    Ok(received_total)
}
