//! Shape-A paid-blobs negotiation ALPN (`archy/paid-blobs/1`) — the on-wire
//! exchange that lets a downloader pay a seeder *before* fetching a gated blob
//! (DHT distribution plan §1, "shape A"). Gated behind `iroh-swarm`.
//!
//! ## Why a side ALPN
//! iroh-blobs carries the raw bytes; this tiny request/grant protocol rides a
//! second ALPN on the *same* endpoint so a downloader can discover the price and
//! deliver an ecash token first. The token opens a metered `streaming` session
//! keyed by the downloader's endpoint id — exactly the session the blob-GET gate
//! ([`super::paid`]) already checks. Same endpoint → same session → the GET is
//! then served.
//!
//! ```text
//!   B ──(archy/paid-blobs/1)──▶ A   PaidRequest { want: H, token: None }
//!   B ◀───────────────────────  A   PaymentRequired { price, accepted_mints }
//!   B: auto_pay_token(...)  ── builds a cashuA token (cross-mint aware)
//!   B ──(archy/paid-blobs/1)──▶ A   PaidRequest { want: H, token: Some(t) }
//!   B ◀───────────────────────  A   Granted        (session now exists on A)
//!   B ──(iroh-blobs ALPN)─────▶ A   GET H  → served (gate sees the session)
//! ```
//!
//! ## North star: origin always wins, releases stay free
//! Negotiation is **best-effort and additive**. A peer that doesn't speak this
//! ALPN, or any connect/protocol error, is treated as "proceed" — the blob-GET
//! gate is the real enforcement, and a denied GET just falls back to origin.
//! With the default [`PaymentPolicy::free`] a downloader never sends a token, so
//! a seeder that prices a blob is simply skipped → origin. Only films (a future
//! caller with a real budget) will actually pay.

use std::path::{Path, PathBuf};

use anyhow::Result;
use iroh::endpoint::Connection;
use iroh::protocol::{AcceptError, ProtocolHandler};
use iroh::{Endpoint, EndpointAddr, EndpointId};
use iroh_blobs::api::blobs::BlobStatus;
use iroh_blobs::api::Store;
use iroh_blobs::Hash;
use serde::{Deserialize, Serialize};

use super::payment::PaymentPolicy;
use crate::streaming::gate::{self, GateResult};

/// ALPN for the paid-blobs negotiation protocol.
pub const PAID_ALPN: &[u8] = b"archy/paid-blobs/1";

/// The streaming service that meters swarm blob serving (same id as [`super::paid`]).
const SERVICE_ID: &str = "content-download";

/// Cap on a single negotiation message (JSON). Requests/responses are tiny.
const MAX_MSG: usize = 64 * 1024;

/// A downloader's ask for one content-addressed blob, optionally with payment.
#[derive(Debug, Serialize, Deserialize)]
struct PaidRequest {
    /// BLAKE3 hex of the wanted blob.
    want: String,
    /// A `cashuA` token, present on the paying retry.
    #[serde(skip_serializing_if = "Option::is_none")]
    token: Option<String>,
}

/// The seeder's verdict.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum PaidResponse {
    /// Fetch away — free, or a paid session is now active for this peer.
    Granted,
    /// Payment needed before serving. The downloader may pay and retry.
    PaymentRequired {
        price_sats: u64,
        accepted_mints: Vec<String>,
    },
    /// Refused (bad request, insufficient/failed payment).
    Denied { reason: String },
}

// ── Serve side ─────────────────────────────────────────────────────────────

/// Accept-side handler for [`PAID_ALPN`]. Registered on the provider's `Router`
/// alongside the iroh-blobs protocol.
#[derive(Clone)]
pub struct PaidBlobsProtocol {
    data_dir: PathBuf,
    store: Store,
}

impl std::fmt::Debug for PaidBlobsProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaidBlobsProtocol").finish()
    }
}

impl PaidBlobsProtocol {
    pub fn new(data_dir: PathBuf, store: Store) -> Self {
        Self { data_dir, store }
    }

    /// Decide the verdict for a request from `peer`. Mirrors [`super::paid`]'s
    /// policy: free when the service is disabled (default) or the peer holds an
    /// active session; payment-required when metered and unpaid; fail-OPEN
    /// (Granted) on an internal gate error so a fault never blocks distribution.
    async fn decide(&self, peer: &str, req: &PaidRequest) -> PaidResponse {
        let size = self.blob_size(&req.want).await;
        match gate::check_gate(&self.data_dir, peer, SERVICE_ID, req.token.as_deref(), size).await {
            Ok(GateResult::ServiceUnavailable)
            | Ok(GateResult::Allowed { .. })
            | Ok(GateResult::PaidAndAllowed { .. }) => PaidResponse::Granted,
            Ok(GateResult::PaymentRequired {
                minimum_sats,
                pricing,
                ..
            }) => PaidResponse::PaymentRequired {
                price_sats: minimum_sats,
                accepted_mints: pricing.accepted_mints,
            },
            Ok(GateResult::InsufficientPayment {
                provided_sats,
                minimum_sats,
            }) => PaidResponse::Denied {
                reason: format!("insufficient payment: {provided_sats} < {minimum_sats} sats"),
            },
            Ok(GateResult::PaymentFailed { reason }) => PaidResponse::Denied { reason },
            // Availability beats revenue: a gate fault serves free, matching the
            // blob-GET gate's fail-open behaviour.
            Err(e) => {
                tracing::warn!("paid-alpn: gate errored ({e}); granting free");
                PaidResponse::Granted
            }
        }
    }

    /// Full size of a held blob (for metering); 0 if we don't hold it complete.
    async fn blob_size(&self, blake3_hex: &str) -> u64 {
        let Ok(raw) = hex::decode(blake3_hex) else {
            return 0;
        };
        let Ok(arr) = <[u8; 32]>::try_from(raw.as_slice()) else {
            return 0;
        };
        match self.store.blobs().status(Hash::from_bytes(arr)).await {
            Ok(BlobStatus::Complete { size }) => size,
            _ => 0,
        }
    }
}

impl ProtocolHandler for PaidBlobsProtocol {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let peer = connection.remote_id().to_string();
        // One bi-stream per request (a paying downloader opens a second one).
        loop {
            let (mut send, mut recv) = match connection.accept_bi().await {
                Ok(s) => s,
                // Connection closed by the peer — normal end of negotiation.
                Err(_) => break,
            };
            let buf = recv
                .read_to_end(MAX_MSG)
                .await
                .map_err(AcceptError::from_err)?;
            let response = match serde_json::from_slice::<PaidRequest>(&buf) {
                Ok(req) => self.decide(&peer, &req).await,
                Err(e) => PaidResponse::Denied {
                    reason: format!("bad request: {e}"),
                },
            };
            let bytes = serde_json::to_vec(&response).map_err(AcceptError::from_err)?;
            send.write_all(&bytes)
                .await
                .map_err(AcceptError::from_err)?;
            send.finish().map_err(AcceptError::from_err)?;
        }
        Ok(())
    }
}

// ── Fetch side ───────────────────────────────────────────────────────────────

/// Negotiate access to `blake3_hex` from `peer` before fetching. Returns whether
/// the caller should proceed to download from this peer.
///
/// Best-effort: any connect/protocol failure returns `true` (proceed — the
/// blob-GET gate is the real enforcement, and a denied GET falls back to origin).
/// Returns `false` only when the seeder explicitly requires a payment we won't or
/// can't make under `policy`.
pub async fn negotiate_access(
    endpoint: &Endpoint,
    data_dir: &Path,
    peer: EndpointId,
    blake3_hex: &str,
    policy: &PaymentPolicy,
) -> bool {
    match negotiate_inner(endpoint, data_dir, peer, blake3_hex, policy).await {
        Ok(proceed) => proceed,
        Err(e) => {
            tracing::debug!(
                "paid-alpn: negotiation with {peer} failed ({e}) — proceeding (gate decides)"
            );
            true
        }
    }
}

async fn negotiate_inner(
    endpoint: &Endpoint,
    data_dir: &Path,
    peer: EndpointId,
    blake3_hex: &str,
    policy: &PaymentPolicy,
) -> Result<bool> {
    let conn = endpoint.connect(EndpointAddr::new(peer), PAID_ALPN).await?;

    // First ask with no token.
    let resp = exchange(
        &conn,
        &PaidRequest {
            want: blake3_hex.to_string(),
            token: None,
        },
    )
    .await?;

    match resp {
        PaidResponse::Granted => Ok(true),
        PaidResponse::Denied { .. } => Ok(false),
        PaidResponse::PaymentRequired {
            price_sats,
            accepted_mints,
        } => {
            // Build a token within budget (cross-mint aware); None ⇒ use origin.
            match super::payment::auto_pay_token(data_dir, policy, &accepted_mints, price_sats)
                .await?
            {
                None => Ok(false),
                Some(token) => {
                    let resp2 = exchange(
                        &conn,
                        &PaidRequest {
                            want: blake3_hex.to_string(),
                            token: Some(token),
                        },
                    )
                    .await?;
                    Ok(matches!(resp2, PaidResponse::Granted))
                }
            }
        }
    }
}

/// One request/response round trip on a fresh bi-stream.
async fn exchange(conn: &Connection, req: &PaidRequest) -> Result<PaidResponse> {
    let (mut send, mut recv) = conn.open_bi().await?;
    send.write_all(&serde_json::to_vec(req)?).await?;
    send.finish()?;
    let buf = recv.read_to_end(MAX_MSG).await?;
    Ok(serde_json::from_slice(&buf)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips_and_omits_absent_token() {
        let req = PaidRequest {
            want: "abcd".into(),
            token: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(
            !json.contains("token"),
            "absent token must be omitted: {json}"
        );
        let back: PaidRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.want, "abcd");
        assert!(back.token.is_none());
    }

    #[test]
    fn request_with_token_round_trips() {
        let req = PaidRequest {
            want: "ff".into(),
            token: Some("cashuAbc".into()),
        };
        let back: PaidRequest =
            serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        assert_eq!(back.token.as_deref(), Some("cashuAbc"));
    }

    #[test]
    fn response_tagged_serialization() {
        let granted = serde_json::to_string(&PaidResponse::Granted).unwrap();
        assert_eq!(granted, r#"{"status":"granted"}"#);

        let pr = serde_json::to_string(&PaidResponse::PaymentRequired {
            price_sats: 7,
            accepted_mints: vec!["https://m".into()],
        })
        .unwrap();
        let back: PaidResponse = serde_json::from_str(&pr).unwrap();
        match back {
            PaidResponse::PaymentRequired {
                price_sats,
                accepted_mints,
            } => {
                assert_eq!(price_sats, 7);
                assert_eq!(accepted_mints, vec!["https://m".to_string()]);
            }
            other => panic!("expected PaymentRequired, got {other:?}"),
        }
    }
}
