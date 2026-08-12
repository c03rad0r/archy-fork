//! Streaming access gate — controls access to metered services.
//!
//! The gate sits between incoming requests and the resource being served.
//! It checks for active sessions, verifies/receives payments, and
//! records usage against allotments.

use super::meter::{self, MeterDecision};
use super::pricing::{self, ServicePricing};
use super::session::{self};
use crate::wallet::ecash;
use anyhow::Result;
use std::path::Path;
use tracing::{debug, warn};

/// Result of a gate check.
#[derive(Debug)]
pub enum GateResult {
    /// Access granted — session is active with sufficient allotment.
    Allowed { session_id: String, remaining: u64 },
    /// Access granted after accepting payment — new or topped-up session.
    PaidAndAllowed {
        session_id: String,
        allotment: u64,
        paid_sats: u64,
    },
    /// Payment required — no active session and no payment token provided.
    PaymentRequired {
        service_id: String,
        minimum_sats: u64,
        pricing: PricingInfo,
    },
    /// Payment insufficient — token was provided but doesn't meet minimum.
    InsufficientPayment {
        provided_sats: u64,
        minimum_sats: u64,
    },
    /// Payment failed — token was invalid or couldn't be verified at mint.
    PaymentFailed { reason: String },
    /// Service not found or not enabled.
    ServiceUnavailable,
}

/// Pricing information for the payment-required response.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PricingInfo {
    pub metric: String,
    pub step_size: u64,
    pub price_per_step: u64,
    pub min_steps: u64,
    pub accepted_mints: Vec<String>,
}

impl From<&ServicePricing> for PricingInfo {
    fn from(p: &ServicePricing) -> Self {
        Self {
            metric: p.metric.to_string(),
            step_size: p.step_size,
            price_per_step: p.price_per_step,
            min_steps: p.min_steps,
            accepted_mints: p.accepted_mints.clone(),
        }
    }
}

/// Check the gate for a streaming service request.
///
/// If `payment_token` is provided (cashuA string), it will be verified and
/// accepted to create or top up a session. If no token is provided, checks
/// for an existing active session.
///
/// `usage_cost` is the cost of the current request in the service's metric units
/// (e.g., bytes for download, 1 for a single API request).
pub async fn check_gate(
    data_dir: &Path,
    peer_id: &str,
    service_id: &str,
    payment_token: Option<&str>,
    usage_cost: u64,
) -> Result<GateResult> {
    // Load pricing config
    let config = pricing::load_pricing(data_dir).await?;
    let service = match config.get_active_service(service_id) {
        Some(s) => s,
        None => return Ok(GateResult::ServiceUnavailable),
    };

    // If payment token provided, process it first
    if let Some(token_str) = payment_token {
        return process_payment(data_dir, peer_id, service, token_str, usage_cost).await;
    }

    // No payment — check for existing session
    let decision = meter::check_access(data_dir, peer_id, service_id, usage_cost).await?;

    match decision {
        MeterDecision::Allow {
            session_id,
            remaining,
        } => {
            // Record usage
            let _ = meter::record_and_check(data_dir, peer_id, service_id, usage_cost).await?;
            Ok(GateResult::Allowed {
                session_id,
                remaining: remaining.saturating_sub(usage_cost),
            })
        }
        MeterDecision::Exhausted { .. } | MeterDecision::NoSession => {
            let accepted_mints = if service.accepted_mints.is_empty() {
                let wallet_mints = ecash::load_accepted_mints(data_dir).await?;
                wallet_mints.mints
            } else {
                service.accepted_mints.clone()
            };

            let mut pricing_info = PricingInfo::from(service);
            pricing_info.accepted_mints = accepted_mints;

            Ok(GateResult::PaymentRequired {
                service_id: service_id.to_string(),
                minimum_sats: service.minimum_payment(),
                pricing: pricing_info,
            })
        }
        MeterDecision::NotMetered => Ok(GateResult::Allowed {
            session_id: String::new(),
            remaining: u64::MAX,
        }),
    }
}

/// Process a payment token and create/topup a session.
async fn process_payment(
    data_dir: &Path,
    peer_id: &str,
    service: &ServicePricing,
    token_str: &str,
    usage_cost: u64,
) -> Result<GateResult> {
    let minimum = service.minimum_payment();

    // Verify and receive the payment
    let received_sats = match ecash::verify_and_receive_payment(data_dir, token_str, minimum).await
    {
        Ok(amount) => amount,
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("Insufficient payment") {
                // Try to parse what was provided
                let provided = extract_token_amount(token_str);
                return Ok(GateResult::InsufficientPayment {
                    provided_sats: provided,
                    minimum_sats: minimum,
                });
            }
            warn!("Payment verification failed for peer {}: {}", peer_id, e);
            return Ok(GateResult::PaymentFailed { reason: err_str });
        }
    };

    // Create or top-up session
    let mut store = session::load_sessions(data_dir).await?;
    let session = store.create_or_topup(peer_id, &service.service_id, service, received_sats);
    let session_id = session.id.clone();
    let allotment = session.allotment;

    // Record initial usage if applicable
    if usage_cost > 0 {
        if let Some(s) = store.get_mut(&session_id) {
            s.record_usage(usage_cost);
        }
    }

    session::save_sessions(data_dir, &store).await?;

    // Record the streaming revenue
    let mut wallet = ecash::load_wallet(data_dir).await?;
    wallet.record_tx(
        ecash::TransactionType::StreamingRevenue,
        received_sats,
        &format!(
            "Streaming payment: {} sats for {} from {}",
            received_sats, service.service_id, peer_id
        ),
        &wallet.mint_url.clone(),
        peer_id,
    );
    ecash::save_wallet(data_dir, &wallet).await?;

    debug!(
        "Gate: accepted {} sats from {} for {}, allotment={}",
        received_sats, peer_id, service.service_id, allotment
    );

    Ok(GateResult::PaidAndAllowed {
        session_id,
        allotment,
        paid_sats: received_sats,
    })
}

/// Try to extract the total amount from a token string (best-effort for error messages).
fn extract_token_amount(token_str: &str) -> u64 {
    // Try cashuA format
    if let Ok(token) = super::super::wallet::cashu::CashuToken::deserialize(token_str) {
        return token.total_amount();
    }
    // Try legacy format
    if token_str.starts_with("cashuSend_") {
        return token_str
            .split('_')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
    }
    0
}

/// Quick check: does a peer have an active session for a service?
/// Lighter weight than check_gate — doesn't record usage or process payments.
pub async fn has_active_session(data_dir: &Path, peer_id: &str, service_id: &str) -> Result<bool> {
    let store = session::load_sessions(data_dir).await?;
    Ok(store.find_active(peer_id, service_id).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_gate_service_unavailable() {
        let tmp = TempDir::new().unwrap();
        let result = check_gate(tmp.path(), "peer1", "nonexistent", None, 1)
            .await
            .unwrap();
        assert!(matches!(result, GateResult::ServiceUnavailable));
    }

    #[tokio::test]
    async fn test_gate_payment_required_default_services() {
        let tmp = TempDir::new().unwrap();

        // Enable a service first
        let mut config = pricing::load_pricing(tmp.path()).await.unwrap();
        config.services[0].enabled = true; // content-download
        pricing::save_pricing(tmp.path(), &config).await.unwrap();

        let result = check_gate(tmp.path(), "peer1", "content-download", None, 1024)
            .await
            .unwrap();

        match result {
            GateResult::PaymentRequired {
                minimum_sats,
                pricing,
                ..
            } => {
                assert_eq!(pricing.metric, "bytes");
                assert!(minimum_sats > 0);
            }
            other => panic!("Expected PaymentRequired, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_has_active_session_false() {
        let tmp = TempDir::new().unwrap();
        assert!(!has_active_session(tmp.path(), "peer1", "test")
            .await
            .unwrap());
    }
}
