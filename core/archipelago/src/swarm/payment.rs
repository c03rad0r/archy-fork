//! Fetch-side auto-pay — the *downloader's* decision layer for paid swarm
//! content (plan §1 "fetch side" + §2a cross-mint).
//!
//! When a swarm seeder gates a blob behind payment (its `PaymentRequired`
//! advertises a price and a set of `accepted_mints`), a downloading node uses
//! this layer to decide whether to pay and, if so, to build a `cashuA` token
//! denominated in one of the seeder's accepted mints — auto-swapping across
//! mints when needed (see [`crate::wallet::ecash::build_payment_token`]).
//!
//! ## North star: origin always wins
//! Paying is strictly an optimization. If the price is over budget, the wallet
//! can't cover it, no trusted mint is reachable, or a swap would cost too much,
//! this layer returns `None` and the caller falls back to the free HTTP origin —
//! exactly today's path. A wallet/mint problem must never block content.
//!
//! ## Scope / what's NOT here
//! This builds the *token*; it does not yet carry it to the seeder. The on-wire
//! exchange (a downloader presenting the token to a paid seeder, then streaming
//! the blob) is the in-band paid-blobs ALPN — "shape (A)" in the design doc —
//! which is deferred. Today's seeder side (`swarm::paid`) only allow/deny-gates
//! iroh-blobs requests; once shape (A) lands, the provider's fetch path calls
//! [`auto_pay_token`] on a `PaymentRequired` and retries with the token.

use std::path::Path;

use anyhow::Result;
use tracing::debug;

use crate::wallet::ecash;

/// A downloader's willingness to pay swarm peers for a single fetch.
#[derive(Debug, Clone, Copy)]
pub struct PaymentPolicy {
    /// Maximum total sats to spend for this content. `0` disables paying
    /// entirely (origin-only) — the safe default.
    pub budget_sats: u64,
    /// Maximum cross-mint swap fee tolerated when we must swap into the
    /// seeder's mint. Ignored when we already hold the right mint.
    pub max_fee_sats: u64,
}

impl PaymentPolicy {
    /// The default: never pay, always use the free origin. The production caller
    /// is the deferred in-band paid-blobs ALPN (shape A); used by tests today.
    #[allow(dead_code)]
    pub fn free() -> Self {
        Self {
            budget_sats: 0,
            max_fee_sats: 0,
        }
    }

    /// A budget-capped policy.
    pub fn with_budget(budget_sats: u64, max_fee_sats: u64) -> Self {
        Self {
            budget_sats,
            max_fee_sats,
        }
    }

    /// Whether a seeder's `price_sats` is worth paying under this policy. A zero
    /// price is treated as "not a real paid request" (use origin / free path).
    pub fn affords(&self, price_sats: u64) -> bool {
        price_sats > 0 && price_sats <= self.budget_sats
    }
}

/// Decide whether to pay a seeder `price_sats`, and if so build a `cashuA` token
/// denominated in one of its `accepted_mints` (auto-swapping if needed).
///
/// * `Ok(Some(token))` — pay the seeder with this token.
/// * `Ok(None)`        — decline (over budget, unpayable, or swap too costly);
///   the caller should fall back to the free origin.
///
/// Never returns `Err` for a wallet/mint problem: those degrade to `Ok(None)`
/// so a payment failure can never block content.
pub async fn auto_pay_token(
    data_dir: &Path,
    policy: &PaymentPolicy,
    accepted_mints: &[String],
    price_sats: u64,
) -> Result<Option<String>> {
    if !policy.affords(price_sats) {
        debug!(
            "auto-pay: price {} sats over budget {} (or zero) — using origin",
            price_sats, policy.budget_sats
        );
        return Ok(None);
    }

    match ecash::build_payment_token(data_dir, accepted_mints, price_sats, policy.max_fee_sats)
        .await
    {
        Ok(token) => Ok(Some(token)),
        Err(e) => {
            // Unpayable within balance/trust/fee — not an error, just decline.
            debug!("auto-pay: declined ({}) — falling back to origin", e);
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_policy_never_affords() {
        let p = PaymentPolicy::free();
        assert!(!p.affords(1));
        assert!(!p.affords(0));
    }

    #[test]
    fn budget_policy_affordability() {
        let p = PaymentPolicy::with_budget(100, 5);
        assert!(p.affords(100)); // exactly at budget
        assert!(p.affords(1));
        assert!(!p.affords(101)); // over budget
        assert!(!p.affords(0)); // zero price is never a real paid request
    }

    #[tokio::test]
    async fn over_budget_declines_without_touching_wallet() {
        let tmp = tempfile::tempdir().unwrap();
        // Price exceeds budget → None, and no wallet/mint interaction occurs.
        let out = auto_pay_token(
            tmp.path(),
            &PaymentPolicy::with_budget(50, 5),
            &["https://seeder.example.com".into()],
            100,
        )
        .await
        .unwrap();
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn zero_budget_is_origin_only() {
        let tmp = tempfile::tempdir().unwrap();
        let out = auto_pay_token(
            tmp.path(),
            &PaymentPolicy::free(),
            &["https://seeder.example.com".into()],
            10,
        )
        .await
        .unwrap();
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn unpayable_within_budget_declines_gracefully() {
        let tmp = tempfile::tempdir().unwrap();
        // Within budget, but empty wallet + untrusted seeder mint → build fails;
        // auto_pay degrades to None (origin) rather than erroring.
        let out = auto_pay_token(
            tmp.path(),
            &PaymentPolicy::with_budget(1000, 10),
            &["https://untrusted.example.com".into()],
            100,
        )
        .await
        .unwrap();
        assert!(out.is_none());
    }
}
