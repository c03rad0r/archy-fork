//! Phase 3 discovery — signed Nostr "seed advertisement" events.
//!
//! A node that holds a PUBLIC release / app-image blob (addressed by BLAKE3)
//! announces "I can seed hash H from iroh endpoint E" as a signed, NIP-33
//! addressable Nostr event. **Scope is releases/catalog content ONLY** — never
//! private user blobs (decided 2026-06-16): smallest privacy surface, covers
//! the OTA + app-install use-cases. Discovery queries these events to find
//! swarm seeds for a hash; the iroh provider then dials those endpoints.
//!
//! Event shape (NIP-33 addressable, kind [`ARCHIPELAGO_SEED_KIND`]):
//!   - `d` tag = blake3 hex of the content → one current advert per (author, hash)
//!   - content = `{"v":1,"endpoint_id":"<iroh endpoint id>"}`
//!   - author pubkey = the node's seed-derived Nostr identity (signs the event)
//!
//! Endpoint ids stay opaque strings here so this protocol layer builds/parses/
//! publishes/queries WITHOUT the heavy iroh dep; only the `iroh-swarm`
//! discovery glue parses the string into an `iroh::EndpointId`.

// The publish/query path that calls these lives behind `iroh-swarm` (it needs
// the node's iroh EndpointId), so in the default build they're exercised only
// by unit tests — allow them to stand without a production caller.
#![allow(dead_code)]

use std::path::Path;
use std::time::Duration;

use nostr_sdk::{Event, EventBuilder, Filter, Keys, Kind, Tag};
use serde::{Deserialize, Serialize};

/// How long to wait for relay connects / event fetches. Matches the rest of the
/// Nostr discovery path so the swarm never stalls the download longer than node
/// discovery already might.
const RELAY_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const RELAY_FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// NIP-33 addressable kind for Archipelago seed advertisements.
/// Distinct from the node-discovery app-data kind (30078).
pub const ARCHIPELAGO_SEED_KIND: u16 = 30081;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdvertContent {
    v: u8,
    endpoint_id: String,
}

/// Build the (unsigned) advertisement event for `blake3_hex` served from
/// `endpoint_id`. Sign with the node's Nostr key (`.sign_with_keys()` /
/// `.sign()`) or publish via `client.send_event_builder()`.
pub fn advertisement_builder(blake3_hex: &str, endpoint_id: &str) -> EventBuilder {
    let content = serde_json::to_string(&AdvertContent {
        v: 1,
        endpoint_id: endpoint_id.to_string(),
    })
    .expect("serialize advert content");
    EventBuilder::new(Kind::Custom(ARCHIPELAGO_SEED_KIND), content)
        .tag(Tag::identifier(blake3_hex.to_string()))
}

/// Filter matching all current seed advertisements for `blake3_hex` (one per
/// advertising node; NIP-33 latest-replaces per author).
pub fn advertisement_filter(blake3_hex: &str) -> Filter {
    Filter::new()
        .kind(Kind::Custom(ARCHIPELAGO_SEED_KIND))
        .identifier(blake3_hex.to_string())
}

/// Extract the advertised endpoint id from an event, or `None` if it is the
/// wrong kind or malformed.
pub fn parse_endpoint_id(event: &Event) -> Option<String> {
    if event.kind != Kind::Custom(ARCHIPELAGO_SEED_KIND) {
        return None;
    }
    serde_json::from_str::<AdvertContent>(&event.content)
        .ok()
        .map(|c| c.endpoint_id)
        .filter(|s| !s.is_empty())
}

/// Collect the unique advertised endpoint ids across a set of events, skipping
/// malformed ones. Order-preserving, de-duplicated.
pub fn endpoint_ids_from_events<'a>(events: impl IntoIterator<Item = &'a Event>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for ev in events {
        if let Some(id) = parse_endpoint_id(ev) {
            if seen.insert(id.clone()) {
                out.push(id);
            }
        }
    }
    out
}

/// Query `relays` for the current seed advertisements for `blake3_hex` and
/// return the de-duplicated endpoint-id strings (opaque here; the `iroh-swarm`
/// glue parses them into `iroh::EndpointId`).
///
/// Best-effort by design: an empty relay list, a connect timeout, or a fetch
/// failure all yield an empty list — never an error. The swarm seam treats "no
/// providers" as "use origin", so discovery problems can only ever degrade to
/// today's HTTP path, never block it.
pub async fn fetch_seed_endpoint_ids(
    relays: &[String],
    tor_proxy: Option<&str>,
    blake3_hex: &str,
) -> Vec<String> {
    if relays.is_empty() {
        return Vec::new();
    }
    // Query anonymously — discovery reads public adverts and must not link the
    // query back to this node's seed identity.
    let anon = Keys::generate();
    let client = match crate::nostr_discovery::build_nostr_client(anon, tor_proxy) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("seed-advert: build relay client failed: {e}");
            return Vec::new();
        }
    };
    for url in relays {
        let _ = client.add_relay(url).await;
    }
    if tokio::time::timeout(RELAY_CONNECT_TIMEOUT, client.connect())
        .await
        .is_err()
    {
        tracing::warn!("seed-advert: relay connect timed out, continuing anyway");
    }
    let events = client
        .fetch_events(advertisement_filter(blake3_hex), RELAY_FETCH_TIMEOUT)
        .await
        .map(|e| e.to_vec())
        .unwrap_or_default();
    client.disconnect().await;
    endpoint_ids_from_events(events.iter())
}

/// Publish a signed advertisement — "this node can seed `blake3_hex` from
/// `endpoint_id`" — to `relays`, signed with the node's seed-derived Nostr key.
///
/// **Caller must restrict this to PUBLIC releases/catalog blobs** (the design's
/// privacy scope, decided 2026-06-16) — never private user content. Best-effort:
/// relay failures are logged, not fatal, since seeding is an optimization.
pub async fn publish_seed_advert(
    identity_dir: &Path,
    relays: &[String],
    tor_proxy: Option<&str>,
    blake3_hex: &str,
    endpoint_id: &str,
) -> anyhow::Result<()> {
    if relays.is_empty() {
        return Ok(());
    }
    let keys = crate::nostr_discovery::load_or_create_nostr_keys(identity_dir).await?;
    let client = crate::nostr_discovery::build_nostr_client(keys, tor_proxy)?;
    for url in relays {
        let _ = client.add_relay(url).await;
    }
    if tokio::time::timeout(RELAY_CONNECT_TIMEOUT, client.connect())
        .await
        .is_err()
    {
        tracing::warn!("seed-advert: publish relay connect timed out, continuing anyway");
    }
    let _ = client
        .send_event_builder(advertisement_builder(blake3_hex, endpoint_id))
        .await;
    client.disconnect().await;
    tracing::info!("seed-advert: announced {blake3_hex} seedable from {endpoint_id}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_sign_parse_roundtrip() {
        let keys = Keys::generate();
        let hash = "a".repeat(64);
        let endpoint = "node-example-endpoint-id";
        let event = advertisement_builder(&hash, endpoint)
            .sign_with_keys(&keys)
            .unwrap();
        assert_eq!(event.kind, Kind::Custom(ARCHIPELAGO_SEED_KIND));
        assert_eq!(parse_endpoint_id(&event).as_deref(), Some(endpoint));
    }

    #[test]
    fn filter_targets_the_hash_dtag_and_kind() {
        let hash = "b".repeat(64);
        let json = serde_json::to_string(&advertisement_filter(&hash)).unwrap();
        assert!(json.contains(&hash), "filter must target the hash d-tag");
        assert!(
            json.contains("30081"),
            "filter must constrain the seed kind"
        );
    }

    #[test]
    fn parse_rejects_wrong_kind_and_empty_endpoint() {
        let keys = Keys::generate();
        let wrong_kind = EventBuilder::new(Kind::Custom(1), "{}")
            .sign_with_keys(&keys)
            .unwrap();
        assert_eq!(parse_endpoint_id(&wrong_kind), None);
        let empty_endpoint = advertisement_builder(&"c".repeat(64), "")
            .sign_with_keys(&keys)
            .unwrap();
        assert_eq!(parse_endpoint_id(&empty_endpoint), None);
    }

    #[test]
    fn dedups_endpoint_ids_across_events() {
        let a = Keys::generate();
        let b = Keys::generate();
        let hash = "d".repeat(64);
        let e1 = advertisement_builder(&hash, "endpoint-A")
            .sign_with_keys(&a)
            .unwrap();
        let e2 = advertisement_builder(&hash, "endpoint-A")
            .sign_with_keys(&b)
            .unwrap();
        let e3 = advertisement_builder(&hash, "endpoint-B")
            .sign_with_keys(&b)
            .unwrap();
        let ids = endpoint_ids_from_events([&e1, &e2, &e3]);
        assert_eq!(
            ids,
            vec!["endpoint-A".to_string(), "endpoint-B".to_string()]
        );
    }
}
