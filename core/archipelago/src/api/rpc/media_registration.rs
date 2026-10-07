//! Owner-session/CSRF RPC plus producer-signed, exact media approval.
//! App callers use the native dashboard bridge; this is never an origin-only grant.
use super::RpcHandler;
use crate::media_registration::{AuthorizedSelection, Intent, Limits};
use anyhow::{Context, Result};
use nostr_sdk::prelude::{Event, Kind};
use serde::Deserialize;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

// Dropping the request future cancels queued locks and chunked snapshot work.
// A completed durable record is still recovered by the original operation ID.
struct RequestCancellation(Arc<AtomicBool>);
impl Drop for RequestCancellation {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}
fn request_cancellation() -> (RequestCancellation, Arc<AtomicBool>) {
    let signal = Arc::new(AtomicBool::new(false));
    (RequestCancellation(signal.clone()), signal)
}

const DOMAIN: &str = "archipelago.media-registration.approval.v1";
const KIND: u16 = 27236;
const MAX_APPROVAL: usize = 32 * 1024;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Params {
    intent: Intent,
    selection: AuthorizedSelection,
    producer_event: Event,
}

const RESOLUTION_DOMAIN: &str = "archipelago.media-registration.resolution.v1";
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResolveParams {
    intent: Intent,
    producer_event: Event,
}
fn verified_resolution(input: serde_json::Value, now: u64) -> Result<ResolveParams> {
    anyhow::ensure!(
        serde_json::to_vec(&input)?.len() <= MAX_APPROVAL,
        "Resolution approval is too large"
    );
    let params: ResolveParams = serde_json::from_value(input)?;
    let event = &params.producer_event;
    event
        .verify()
        .context("Resolution producer signature failed")?;
    anyhow::ensure!(
        event.kind == Kind::Custom(27237) && event.pubkey.to_hex() == params.intent.producer,
        "Resolution signature purpose or producer changed"
    );
    let encoded = serde_json::to_value(event)?;
    anyhow::ensure!(
        encoded["tags"] == serde_json::json!([["d", RESOLUTION_DOMAIN]])
            && event.created_at.as_u64() >= params.intent.created_at.saturating_sub(30)
            && event.created_at.as_u64() <= now.saturating_add(30),
        "Invalid resolution signature time or scope"
    );
    let content: serde_json::Value = serde_json::from_str(&event.content)?;
    anyhow::ensure!(
        content
            == serde_json::json!({
                "action":"Recover prepared video or retire this expired incomplete registration",
                "scope":RESOLUTION_DOMAIN, "intent":params.intent,
            }),
        "Resolution signature changed the original intent"
    );
    Ok(params)
}

fn approval_content(intent: &Intent, selection: &AuthorizedSelection) -> Result<serde_json::Value> {
    let path = selection
        .relative_path
        .to_str()
        .context("Cloud file name is not UTF-8")?;
    Ok(serde_json::json!({
        "action":"Register this Cloud video for an IndeeHub project",
        "scope":DOMAIN,
        "intent":intent,
        "selection":{"cloudFile":path,"paymentMethods":selection.payment_methods},
    }))
}
fn verified_producer(params: &Params, now: u64) -> Result<String> {
    let event = &params.producer_event;
    anyhow::ensure!(
        event.kind == Kind::Custom(KIND),
        "This signature is not a media registration approval"
    );
    event
        .verify()
        .context("Producer approval signature failed")?;
    let producer = event.pubkey.to_hex();
    anyhow::ensure!(
        producer == params.intent.producer,
        "The signing identity differs from the project producer"
    );
    let created = event.created_at.as_u64();
    anyhow::ensure!(
        created >= params.intent.created_at.saturating_sub(30)
            && created < params.intent.expires_at
            && created <= now.saturating_add(30),
        "Producer approval was not signed within this registration intent"
    );
    let encoded = serde_json::to_value(event)?;
    anyhow::ensure!(
        encoded["tags"] == serde_json::json!([["d", DOMAIN]]),
        "Media approval signature scope changed"
    );
    let content: serde_json::Value = serde_json::from_str(&event.content)
        .context("Producer approval is not readable registration terms")?;
    anyhow::ensure!(
        content == approval_content(&params.intent, &params.selection)?,
        "Approved project, Cloud selection or rental terms changed"
    );
    Ok(producer)
}
fn parse(input: serde_json::Value, now: u64) -> Result<(Params, String)> {
    anyhow::ensure!(
        serde_json::to_vec(&input)?.len() <= MAX_APPROVAL,
        "Registration approval is too large"
    );
    let params: Params = serde_json::from_value(input).context("Invalid registration approval")?;
    let producer = verified_producer(&params, now)?;
    Ok((params, producer))
}
fn registration_limit(_data_dir: &Path) -> u64 {
    // Explicit per-file staging bound; shared immutable snapshot storage handles
    // disk reservations separately before this route is enabled for live apps.
    16 * 1024 * 1024 * 1024
}
impl RpcHandler {
    /// Read-only public installation bindings for the native consent bridge.
    /// A hidden standalone signer must compare its actual app origin with these
    /// installed addresses; a caller-supplied app name is never sufficient.
    pub(super) async fn handle_media_registration_context(
        &self,
        _input: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let (state, _) = self.state_manager.get_snapshot().await;
        let identity =
            crate::identity::NodeIdentity::load_existing(&self.config.data_dir.join("identity"))
                .await?;
        let data_dir = self.config.data_dir.clone();
        let context = tokio::task::spawn_blocking(move || {
            crate::container::registration_pin::installed_context(&data_dir, &identity, &state)
        })
        .await??;
        let mut result = serde_json::to_value(context)?;
        result["paymentMethods"] = serde_json::json!(["cashu"]);
        Ok(result)
    }

    pub(super) async fn handle_media_registration_resolve(
        &self,
        input: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let now = u64::try_from(chrono::Utc::now().timestamp()).context("Invalid node clock")?;
        let params = verified_resolution(input, now)?;
        let (state, _) = self.state_manager.get_snapshot().await;
        let identity =
            crate::identity::NodeIdentity::load_existing(&self.config.data_dir.join("identity"))
                .await?;
        let data_dir = self.config.data_dir.clone();
        let (_cancellation, cancelled) = request_cancellation();
        tokio::task::spawn_blocking(move || {
            crate::container::registration_pin::installed_context(&data_dir, &identity, &state)?;
            crate::registered_media::resolve_registration(
                &data_dir,
                &identity,
                &params.intent,
                &params.producer_event.pubkey.to_hex(),
                now,
                &Limits {
                    max_bytes: registration_limit(&data_dir),
                    cancelled: &cancelled,
                },
            )
        })
        .await?
    }

    /// Standard RPC front door already requires owner session, permitted origin
    /// and CSRF. Producer signature is additional exact-scope consent, not a
    /// replacement for those owner checks. A replay repeats the original UUID.
    pub(super) async fn handle_media_registration_prepare(
        &self,
        input: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let now = u64::try_from(chrono::Utc::now().timestamp()).context("Invalid node clock")?;
        let (params, producer) = parse(input, now)?;
        let (state, _) = self.state_manager.get_snapshot().await;
        anyhow::ensure!(
            state
                .package_data
                .get("indeedhub-api")
                .is_some_and(|entry| matches!(
                    entry.state,
                    crate::data_model::PackageState::Running
                )),
            "The installed IndeeHub API must be running to register its media"
        );
        let identity =
            crate::identity::NodeIdentity::load_existing(&self.config.data_dir.join("identity"))
                .await?;
        let data_dir = self.config.data_dir.clone();
        let (_cancellation, cancelled) = request_cancellation();
        let receipt = tokio::task::spawn_blocking(move || {
            crate::container::registration_pin::installed_context(&data_dir, &identity, &state)?;
            let project = params.intent.project_id.clone();
            let limits = Limits {
                max_bytes: registration_limit(&data_dir),
                cancelled: &cancelled,
            };
            crate::registered_media::register_approved_selection(
                &data_dir,
                &data_dir.join("filebrowser"),
                &identity,
                &crate::registered_media::ApprovedSelection {
                    authenticated_producer: &producer,
                    authenticated_project: &project,
                    intent: &params.intent,
                    selection: &params.selection,
                },
                now,
                &limits,
                |_| Ok(()),
            )
        })
        .await??;
        Ok(serde_json::to_value(receipt)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr_sdk::prelude::{EventBuilder, Keys, Tag, Timestamp};
    fn fixture() -> Params {
        let keys = Keys::parse(&"07".repeat(32)).unwrap();
        let intent = Intent {
            version: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
            nonce: "ab".repeat(32),
            app_audience: uuid::Uuid::new_v4().to_string(),
            node_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([7; 32])).unwrap(),
            producer: keys.public_key().to_hex(),
            project_id: "fixture-project".into(),
            price_sats: 8,
            viewing_seconds: 3600,
            created_at: 1000,
            expires_at: 1600,
        };
        let selection = AuthorizedSelection {
            relative_path: "Movies/Film.mp4".into(),
            payment_methods: vec!["cashu".into()],
        };
        let event = EventBuilder::new(
            Kind::Custom(KIND),
            serde_json::to_string_pretty(&approval_content(&intent, &selection).unwrap()).unwrap(),
        )
        .tag(Tag::identifier(DOMAIN))
        .custom_created_at(Timestamp::from(1200))
        .sign_with_keys(&keys)
        .unwrap();
        Params {
            intent,
            selection,
            producer_event: event,
        }
    }
    #[test]
    fn producer_signature_binds_human_readable_exact_selection_terms_node_and_installation() {
        let original = fixture();
        assert_eq!(
            verified_producer(&original, 1300).unwrap(),
            original.intent.producer
        );
        // Original consent can recover an already-completed operation after
        // expiry; underlying snapshot journal refuses creating a fresh one.
        assert!(verified_producer(&original, 2000).is_ok());
        let mut changed = fixture();
        changed.intent.price_sats += 1;
        assert!(verified_producer(&changed, 1300).is_err());
        let mut changed = fixture();
        changed.selection.relative_path = "Other.mp4".into();
        assert!(verified_producer(&changed, 1300).is_err());
        let mut changed = fixture();
        changed.intent.app_audience = uuid::Uuid::new_v4().to_string();
        assert!(verified_producer(&changed, 1300).is_err());
        let mut changed = fixture();
        changed.intent.producer = "cd".repeat(32);
        assert!(verified_producer(&changed, 1300).is_err());
        assert!(verified_producer(&fixture(), 1000).is_err());
    }
    #[test]
    fn request_parser_rejects_unsigned_claims_unknown_fields_and_changed_signed_content() {
        let original = fixture();
        let mut encoded = serde_json::json!({"intent":original.intent,"selection":original.selection,"producerEvent":original.producer_event});
        assert!(parse(encoded.clone(), 1300).is_ok());
        encoded["producerEvent"]["content"] = serde_json::json!("approve everything");
        assert!(parse(encoded, 1300).is_err());
        assert!(parse(serde_json::json!({"producer":"cd".repeat(32)}), 1300).is_err());
    }
    #[test]
    fn dropped_request_signals_blocking_copy_cancellation() {
        let (guard, signal) = request_cancellation();
        assert!(!signal.load(Ordering::Relaxed));
        drop(guard);
        assert!(signal.load(Ordering::Relaxed));
    }
    #[test]
    fn resolution_signature_recovers_only_exact_intent_after_expiry_without_file_authority() {
        let original = fixture();
        let keys = Keys::parse(&"07".repeat(32)).unwrap();
        let event = EventBuilder::new(
            Kind::Custom(27237),
            serde_json::json!({
                "action":"Recover prepared video or retire this expired incomplete registration",
                "scope":RESOLUTION_DOMAIN,"intent":original.intent,
            })
            .to_string(),
        )
        .tag(Tag::identifier(RESOLUTION_DOMAIN))
        .custom_created_at(Timestamp::from(1700))
        .sign_with_keys(&keys)
        .unwrap();
        let encoded = serde_json::json!({"intent":original.intent,"producerEvent":event});
        assert!(verified_resolution(encoded.clone(), 1800).is_ok());
        assert!(verified_resolution(encoded.clone(), 9999).is_ok());
        assert!(parse(encoded.clone(), 1800).is_err());
        let mut changed = encoded.clone();
        changed["intent"]["priceSats"] = serde_json::json!(999);
        assert!(verified_resolution(changed, 1800).is_err());
        let mut changed = encoded;
        changed["selection"] =
            serde_json::json!({"relative_path":"film.mp4","payment_methods":["cashu"]});
        assert!(verified_resolution(changed, 1800).is_err());
        assert!(verified_resolution(
            serde_json::json!({"intent":original.intent,"producerEvent":original.producer_event}),
            1800
        )
        .is_err());
    }
}
