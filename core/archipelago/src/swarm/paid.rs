//! Paid swarm serving — gate the iroh-blobs provider through the ecash
//! `streaming` payment layer (DHT distribution plan, Phase 4 step F).
//!
//! ## Free by default
//! Serving is **free unless the node operator turns it on** in
//! *Networking Profits → Settings* (which enables the `content-download`
//! streaming service). With that service disabled — the shipped default —
//! [`is_authorized`] returns `true` for everyone and behaviour is byte-for-byte
//! the old open seeder. When it is enabled, a peer must hold an active paid
//! session (opened out-of-band via the `streaming.pay` RPC with a Cashu token)
//! before the swarm will serve them; otherwise the request is refused and they
//! fall back to the HTTP origin.
//!
//! ## How it hooks in
//! iroh-blobs 0.103 lets a provider authorize each request: we pass an
//! [`EventSender`] (built here) to `BlobsProtocol::new`, set the [`EventMask`]
//! to intercept connections + GET requests, and answer each one with
//! `Ok(())` (serve) or `Err(AbortReason::Permission)` (refuse). Peer-initiated
//! writes (`push`) are hard-disabled so a peer can never mutate our store.
//!
//! Scope note: today every swarm blob is a public release/app component, so the
//! gate only ever charges if the operator explicitly priced `content-download`.
//! When IndeeHub films land on the same blob layer (Phase 4), they reuse this
//! exact path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use iroh::EndpointId;
use iroh_blobs::api::blobs::BlobStatus;
use iroh_blobs::api::Store;
use iroh_blobs::provider::events::{
    AbortReason, ConnectMode, EventMask, EventResult, EventSender, ObserveMode, ProviderMessage,
    RequestMode, ThrottleMode,
};
use iroh_blobs::Hash;

use crate::streaming::gate::{self, GateResult};

/// The streaming pricing service that meters swarm blob serving. Enabling it in
/// the Settings UI is what flips swarm serving from free to paid.
const SERVICE_ID: &str = "content-download";

/// Build the gated [`EventSender`] for `BlobsProtocol` and spawn the task that
/// authorizes each blob GET through the ecash gate.
///
/// `data_dir` locates the pricing/session state; `store` is cloned in to look up
/// blob sizes for metering. The spawned task lives as long as the provider keeps
/// the returned sender alive (i.e. the life of the node).
pub fn gated_event_sender(data_dir: PathBuf, store: Store) -> EventSender {
    // Intercept connections + read requests so we can allow/deny per peer & hash.
    // `push` (peer writes into our store) is hard-disabled. `throttle`/`observe`
    // stay off — we meter coarsely at request time, not per 16 KiB chunk.
    let mask = EventMask {
        connected: ConnectMode::Intercept,
        get: RequestMode::Intercept,
        get_many: RequestMode::Intercept,
        push: RequestMode::Disabled,
        observe: ObserveMode::None,
        throttle: ThrottleMode::None,
    };
    let (sender, mut rx) = EventSender::channel(64, mask);
    tokio::spawn(async move {
        // connection_id → remote endpoint id, learned at ClientConnected and used
        // to key the paying peer's streaming session on each request.
        let mut peers: HashMap<u64, Option<EndpointId>> = HashMap::new();
        while let Some(msg) = rx.recv().await {
            match msg {
                ProviderMessage::ClientConnected(m) => {
                    peers.insert(m.inner.connection_id, m.inner.endpoint_id);
                    // Accept the connection; gating happens per request.
                    let _ = m.tx.send(Ok(())).await;
                }
                ProviderMessage::ConnectionClosed(m) => {
                    peers.remove(&m.inner.connection_id);
                }
                ProviderMessage::GetRequestReceived(m) => {
                    let peer = peers.get(&m.inner.connection_id).copied().flatten();
                    let hash = m.inner.request.hash;
                    let verdict = authorize(&data_dir, &store, peer, &hash).await;
                    let _ = m.tx.send(verdict).await;
                }
                ProviderMessage::GetManyRequestReceived(m) => {
                    let peer = peers.get(&m.inner.connection_id).copied().flatten();
                    // A get-many is all-or-nothing here: authorize on the first hash.
                    let verdict = match m.inner.request.hashes.first().copied() {
                        Some(h) => authorize(&data_dir, &store, peer, &h).await,
                        None => Ok(()),
                    };
                    let _ = m.tx.send(verdict).await;
                }
                ProviderMessage::PushRequestReceived(m) => {
                    // Disabled in the mask; refuse defensively if one ever arrives.
                    let _ = m.tx.send(Err(AbortReason::Permission)).await;
                }
                // Notify-only variants, observe and throttle: nothing to gate.
                _ => {}
            }
        }
    });
    sender
}

/// Authorize one blob GET, returning the iroh [`EventResult`]
/// (`Ok(())` = serve, `Err(Permission)` = refuse).
async fn authorize(
    data_dir: &Path,
    store: &Store,
    peer: Option<EndpointId>,
    hash: &Hash,
) -> EventResult {
    // Cost = full blob size (coarse, request-time metering). If we don't hold the
    // complete blob there's nothing to meter — let iroh serve what it can.
    let size = match store.blobs().status(*hash).await {
        Ok(BlobStatus::Complete { size }) => size,
        _ => 0,
    };
    let peer_id = peer
        .map(|e| e.to_string())
        .unwrap_or_else(|| "anonymous".to_string());
    if is_authorized(data_dir, &peer_id, size).await {
        Ok(())
    } else {
        Err(AbortReason::Permission)
    }
}

/// Pure allow/deny decision (no iroh types) — unit-testable without a live node.
async fn is_authorized(data_dir: &Path, peer_id: &str, size: u64) -> bool {
    match gate::check_gate(data_dir, peer_id, SERVICE_ID, None, size).await {
        // Service disabled (the default) → free for everyone. Or the peer holds an
        // active paid session with remaining allotment.
        Ok(GateResult::ServiceUnavailable)
        | Ok(GateResult::Allowed { .. })
        | Ok(GateResult::PaidAndAllowed { .. }) => true,
        // Metered + no/exhausted session: the peer must pay out-of-band first
        // (streaming.pay) before the swarm serves them — they fall back to origin.
        Ok(_) => false,
        // Never let a payment-layer fault break content distribution: fail OPEN
        // (serve free) and log. Availability beats revenue when something breaks.
        Err(e) => {
            tracing::warn!("paid-gate: check errored ({e}); serving free");
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streaming::pricing::{self, Metric, PricingConfig, ServicePricing};

    fn content_download(enabled: bool) -> PricingConfig {
        PricingConfig {
            services: vec![ServicePricing {
                service_id: SERVICE_ID.to_string(),
                name: "Content Downloads".to_string(),
                metric: Metric::Bytes,
                step_size: 1_048_576,
                price_per_step: 1,
                min_steps: 0,
                enabled,
                description: String::new(),
                accepted_mints: vec![],
            }],
        }
    }

    #[tokio::test]
    async fn free_when_service_disabled_by_default() {
        let dir = tempfile::tempdir().unwrap();
        // No pricing file → defaults → content-download disabled → free for all.
        assert!(is_authorized(dir.path(), "peer-a", 1_000_000).await);
    }

    #[tokio::test]
    async fn free_when_service_explicitly_disabled() {
        let dir = tempfile::tempdir().unwrap();
        pricing::save_pricing(dir.path(), &content_download(false))
            .await
            .unwrap();
        assert!(is_authorized(dir.path(), "peer-a", 1_048_576).await);
    }

    #[tokio::test]
    async fn denied_when_metered_and_peer_has_not_paid() {
        let dir = tempfile::tempdir().unwrap();
        pricing::save_pricing(dir.path(), &content_download(true))
            .await
            .unwrap();
        // Enabled service + no session/token → the swarm refuses; peer uses origin.
        assert!(!is_authorized(dir.path(), "peer-b", 1_048_576).await);
    }
}
