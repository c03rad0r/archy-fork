//! Seller-side pending entitlements for Lightning-invoice peer-file sales (#46).
//!
//! When a buyer asks to pay for a paid catalog item with an external wallet (as
//! opposed to the local-ecash fast path), the *selling* node mints a Lightning
//! invoice on its own LND and records a pending entitlement here, keyed by the
//! invoice's payment hash. The buyer pays the invoice from any wallet and polls
//! for settlement; once the seller's LND confirms the invoice is settled we mark
//! the entitlement paid, and the content gate (`content_server::serve_content`)
//! then releases the file to anyone presenting that payment hash.
//!
//! State is in-memory and bounded by a TTL. If the seller restarts before the
//! buyer pays, the buyer simply requests a fresh invoice — no value is lost
//! because an unpaid invoice represents no money.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// How long a pending/paid entitlement is retained. Generous enough for a human
/// to pay an invoice and download, short enough to keep the map small.
const ENTITLEMENT_TTL: Duration = Duration::from_secs(3600); // 1 hour

#[derive(Clone)]
struct Entitlement {
    content_id: String,
    price_sats: u64,
    paid: bool,
    created_at: Instant,
}

static ENTITLEMENTS: LazyLock<Mutex<HashMap<String, Entitlement>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Drop expired entries. Caller must hold the lock.
fn prune(map: &mut HashMap<String, Entitlement>) {
    map.retain(|_, e| e.created_at.elapsed() < ENTITLEMENT_TTL);
}

/// Record a freshly-minted invoice as a pending (unpaid) entitlement.
pub async fn record_pending(payment_hash: &str, content_id: &str, price_sats: u64) {
    let mut map = ENTITLEMENTS.lock().await;
    prune(&mut map);
    map.insert(
        payment_hash.to_string(),
        Entitlement {
            content_id: content_id.to_string(),
            price_sats,
            paid: false,
            created_at: Instant::now(),
        },
    );
}

/// Mark the entitlement for `payment_hash` paid. No-op if unknown/expired.
pub async fn mark_paid(payment_hash: &str) {
    let mut map = ENTITLEMENTS.lock().await;
    prune(&mut map);
    if let Some(e) = map.get_mut(payment_hash) {
        e.paid = true;
    }
}

/// The content_id + price an entitlement was issued for, if still live.
pub async fn lookup(payment_hash: &str) -> Option<(String, u64)> {
    let mut map = ENTITLEMENTS.lock().await;
    prune(&mut map);
    map.get(payment_hash)
        .map(|e| (e.content_id.clone(), e.price_sats))
}

/// True if `payment_hash` is a paid entitlement for exactly `content_id`.
/// This is the gate the content server consults to release a file.
pub async fn is_paid_for(payment_hash: &str, content_id: &str) -> bool {
    let mut map = ENTITLEMENTS.lock().await;
    prune(&mut map);
    map.get(payment_hash)
        .map(|e| e.paid && e.content_id == content_id)
        .unwrap_or(false)
}
