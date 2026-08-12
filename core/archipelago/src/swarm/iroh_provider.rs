//! iroh-blobs swarm provider — the DHT Phase 2 engine, gated behind the
//! `iroh-swarm` feature (heavy QUIC dep tree, off by default).
//!
//! Stands up a real iroh node: binds a QUIC [`Endpoint`], opens a persistent
//! blob [`FsStore`] under `data_dir/iroh-blobs`, and serves blobs over the
//! iroh-blobs protocol — so a node that *fetches* content also *seeds* it
//! afterwards. Content is addressed by BLAKE3 ([`Hash`]) and range-verified by
//! iroh on arrival.
//!
//! This provider is an optimization beneath the origin HTTP path: the [`super`]
//! swarm seam falls back to origin whenever [`try_fetch`](IrohProvider::try_fetch)
//! returns `Ok(false)` (no known seeds) or `Err` (transient swarm failure).
//!
//! ## Discovery boundary (Phase 3)
//! Downloading needs the [`EndpointId`]s of peers that hold the hash. That
//! discovery — design Phase 3, *signed Nostr advertisement events* mapping
//! `{content-hash → provider endpoint}` — is injected via [`ProviderDiscovery`].
//! Until it is wired, discovery yields nothing and every fetch defers to origin,
//! so enabling the feature is safe (never worse than today).

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use iroh::{endpoint::presets, protocol::Router, Endpoint, EndpointId};
use iroh_blobs::{store::fs::FsStore, BlobsProtocol, Hash};

use super::payment::PaymentPolicy;
use super::BlobProvider;
use crate::content_hash::{ContentDigest, HashAlg};

/// Resolves which peers are believed to hold a given content hash.
///
/// Phase 3 (signed Nostr advertisement events) provides the production impl
/// [`NostrSeedDiscovery`]; `None` discovery means "origin-only" — a safe
/// default. The query is async (it hits relays), so the trait is async.
#[async_trait]
pub trait ProviderDiscovery: Send + Sync {
    /// Candidate seed endpoints for `hash` (may be empty).
    async fn providers_for(&self, hash: &Hash) -> Vec<EndpointId>;
}

/// Production [`ProviderDiscovery`]: reads signed seed advertisements from Nostr
/// relays and parses the advertised endpoint-id strings into [`EndpointId`]s.
///
/// Unparseable ids are skipped (an advert from an incompatible/garbage peer must
/// not abort discovery). Reuses the node's existing relay list + Tor proxy.
pub struct NostrSeedDiscovery {
    relays: Vec<String>,
    tor_proxy: Option<String>,
}

impl NostrSeedDiscovery {
    pub fn new(relays: Vec<String>, tor_proxy: Option<String>) -> Self {
        Self { relays, tor_proxy }
    }
}

#[async_trait]
impl ProviderDiscovery for NostrSeedDiscovery {
    async fn providers_for(&self, hash: &Hash) -> Vec<EndpointId> {
        let hex = hash.to_hex();
        let ids = super::seed_advert::fetch_seed_endpoint_ids(
            &self.relays,
            self.tor_proxy.as_deref(),
            &hex,
        )
        .await;
        ids.into_iter()
            .filter_map(|s| match EndpointId::from_str(&s) {
                Ok(id) => Some(id),
                Err(e) => {
                    tracing::debug!("swarm: skipping unparseable seed endpoint id {s}: {e}");
                    None
                }
            })
            .collect()
    }
}

/// Fetches content-addressed blobs from the iroh swarm, and seeds what it has.
#[allow(dead_code)] // constructed once Phase 3 discovery is wired into providers()
pub struct IrohProvider {
    endpoint: Endpoint,
    store: FsStore,
    /// Kept alive so the node keeps accepting blob-protocol connections (seeds).
    _router: Router,
    discovery: Option<Arc<dyn ProviderDiscovery>>,
    /// Where pricing/session/wallet state lives — for paid-fetch negotiation.
    data_dir: std::path::PathBuf,
    /// Willingness to pay swarm peers when fetching. Defaults to
    /// [`PaymentPolicy::free`]: never pay (releases/catalog stay free), so a
    /// seeder that prices a blob is skipped → origin. A future film fetch can
    /// pass a real budget.
    pay_policy: PaymentPolicy,
}

#[allow(dead_code)]
impl IrohProvider {
    /// Bind an iroh endpoint, open the persistent blob store at
    /// `data_dir/iroh-blobs`, and start serving blobs (seed capability).
    pub async fn new(
        data_dir: &Path,
        discovery: Option<Arc<dyn ProviderDiscovery>>,
    ) -> Result<Self> {
        let root = data_dir.join("iroh-blobs");
        tokio::fs::create_dir_all(&root).await.ok();

        let store = FsStore::load(&root)
            .await
            .map_err(|e| anyhow::anyhow!("open iroh blob store: {e}"))?;

        let endpoint = Endpoint::bind(presets::N0)
            .await
            .map_err(|e| anyhow::anyhow!("bind iroh endpoint: {e}"))?;

        // Serve blobs: a node that fetches a blob can then seed it to others.
        // The event sender gates each request through the ecash `streaming` layer
        // — free by default, paid only if the operator priced `content-download`
        // (Networking Profits → Settings). It also hard-disables peer writes.
        let event_sender =
            super::paid::gated_event_sender(data_dir.to_path_buf(), (*store).clone());
        let blobs = BlobsProtocol::new(&store, Some(event_sender));
        // Shape-A paid negotiation rides a second ALPN on the same endpoint so a
        // downloader can pay (open a session) before the blob-GET above serves it.
        let paid =
            super::paid_alpn::PaidBlobsProtocol::new(data_dir.to_path_buf(), (*store).clone());
        let router = Router::builder(endpoint.clone())
            .accept(iroh_blobs::ALPN, blobs)
            .accept(super::paid_alpn::PAID_ALPN, paid)
            .spawn();

        Ok(Self {
            endpoint,
            store,
            _router: router,
            discovery,
            data_dir: data_dir.to_path_buf(),
            pay_policy: PaymentPolicy::free(),
        })
    }

    /// This node's iroh endpoint id — what Phase 3 advertises as a seed address.
    pub fn endpoint_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    /// Import a held PUBLIC blob into the seed store and advertise it on Nostr so
    /// other nodes can fetch it from us. Call this only for releases/catalog
    /// content (the design's privacy scope) — never private user blobs.
    ///
    /// Importing makes us an actual seed: a node that downloaded a release from
    /// the HTTP origin can now serve it to peers over iroh-blobs. The advert maps
    /// `blake3_hex → this endpoint id`. Defensive check: the bytes we import must
    /// hash to what we advertise, so a path/hash mismatch can never publish a lie.
    pub async fn seed_and_advertise(
        &self,
        path: &Path,
        blake3_hex: &str,
        identity_dir: &Path,
        relays: &[String],
        tor_proxy: Option<&str>,
    ) -> Result<()> {
        let expected = {
            let raw = hex::decode(blake3_hex).map_err(|e| anyhow::anyhow!("blake3 hex: {e}"))?;
            let arr: [u8; 32] = raw
                .as_slice()
                .try_into()
                .map_err(|_| anyhow::anyhow!("blake3 digest must be 32 bytes"))?;
            Hash::from_bytes(arr)
        };
        let info = self
            .store
            .blobs()
            .add_path(path)
            .await
            .map_err(|e| anyhow::anyhow!("import blob into seed store: {e}"))?;
        if info.hash != expected {
            anyhow::bail!(
                "imported blob hash {} != advertised {}",
                info.hash.to_hex(),
                blake3_hex
            );
        }
        super::seed_advert::publish_seed_advert(
            identity_dir,
            relays,
            tor_proxy,
            blake3_hex,
            &self.endpoint_id().to_string(),
        )
        .await
    }
}

#[async_trait]
impl BlobProvider for IrohProvider {
    fn name(&self) -> &str {
        "iroh"
    }

    async fn try_fetch(&self, digest: &ContentDigest, dest: &Path) -> Result<bool> {
        // iroh addresses content by BLAKE3. A sha256-only digest isn't fetchable
        // from the swarm — defer to origin.
        if digest.alg != HashAlg::Blake3 {
            return Ok(false);
        }
        let raw = hex::decode(&digest.hex).map_err(|e| anyhow::anyhow!("digest hex: {e}"))?;
        let arr: [u8; 32] = raw
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("blake3 digest must be 32 bytes"))?;
        let hash = Hash::from_bytes(arr);

        // Who has it? Without discovery (Phase 3) this is empty → origin wins.
        let providers = match &self.discovery {
            Some(d) => d.providers_for(&hash).await,
            None => Vec::new(),
        };
        if providers.is_empty() {
            return Ok(false);
        }

        // Shape-A: negotiate paid access with each candidate. Best-effort and
        // additive — a peer is dropped only if it explicitly requires a payment
        // we won't make under `pay_policy` (free by default → priced seeders are
        // skipped). Connect/protocol failures keep the peer; the blob-GET gate is
        // the real enforcement and a refused GET still falls back to origin.
        let mut allowed = Vec::with_capacity(providers.len());
        for peer in providers {
            if super::paid_alpn::negotiate_access(
                &self.endpoint,
                &self.data_dir,
                peer,
                &digest.hex,
                &self.pay_policy,
            )
            .await
            {
                allowed.push(peer);
            }
        }
        if allowed.is_empty() {
            return Ok(false);
        }

        // Fetch (range-verified by iroh) then export the verified blob to the
        // staging path the caller expects. The seam re-verifies the digest.
        let downloader = self.store.downloader(&self.endpoint);
        downloader
            .download(hash, allowed)
            .await
            .map_err(|e| anyhow::anyhow!("iroh swarm download: {e}"))?;
        self.store
            .blobs()
            .export(hash, dest)
            .await
            .map_err(|e| anyhow::anyhow!("export blob to staging: {e}"))?;
        Ok(true)
    }
}
