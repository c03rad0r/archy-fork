//! Swarm-assist content fetch — the *transport & swarm* tier of the DHT
//! distribution plan (`docs/dht-distribution-design.md` §4).
//!
//! ## Guiding principle: swarm-assist, origin ALWAYS wins
//! The peer swarm is an optimization layered *above* a proven HTTP path, never
//! in place of it. A node asks each available [`BlobProvider`] (e.g. an
//! iroh-blobs swarm) for content by its [`ContentDigest`]; the first peer that
//! serves bytes which **verify** against the digest wins. If no provider has it
//! — or the swarm is disabled, or every peer is offline — we fall back to the
//! origin HTTP download, which is the guaranteed source of truth. Worst case is
//! exactly today's behaviour.
//!
//! Peer-sourced bytes are UNTRUSTED, so this module verifies them against the
//! content digest before accepting. Origin bytes run through the caller's
//! existing verification (e.g. the SHA-256 gate in `update.rs`).
//!
//! The actual iroh-blobs provider is gated behind the `iroh-swarm` feature
//! (heavy QUIC dep tree); with the feature off, [`providers`] is empty and
//! every fetch goes straight to origin — byte-for-byte today's path.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use anyhow::Result;
use async_trait::async_trait;
use tracing::{debug, info, warn};

use crate::content_hash::ContentDigest;

pub mod payment;
pub mod seed_advert;

#[cfg(feature = "iroh-swarm")]
pub mod iroh_provider;

#[cfg(feature = "iroh-swarm")]
pub mod paid;

#[cfg(feature = "iroh-swarm")]
pub mod paid_alpn;

/// Which source ultimately served the content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchSource {
    /// A peer in the swarm served (and the bytes verified).
    Swarm,
    /// The origin HTTP fallback served.
    Origin,
}

/// A source that may be able to serve content addressed by its digest.
#[async_trait]
pub trait BlobProvider: Send + Sync {
    /// Short name for logging (e.g. "iroh").
    fn name(&self) -> &str;

    /// Try to fetch the content for `digest` into `dest`.
    ///
    /// * `Ok(true)`  — bytes written to `dest` (caller verifies the digest).
    /// * `Ok(false)` — this provider does not have the content; try the next.
    /// * `Err(_)`    — a transient failure; try the next provider.
    async fn try_fetch(&self, digest: &ContentDigest, dest: &Path) -> Result<bool>;
}

/// Process-wide swarm runtime, built once at startup by [`init`]. Holding the
/// providers here (rather than rebuilding per download) keeps the iroh endpoint
/// + blob store + protocol router alive for the life of the process, so a node
/// keeps *seeding* between downloads. Empty/inert unless the `iroh-swarm`
/// feature is built AND `swarm_enabled` is set.
struct SwarmRuntime {
    providers: Vec<Arc<dyn BlobProvider>>,
    /// Context for announcing held public blobs; `None` when seeding is off.
    #[cfg(feature = "iroh-swarm")]
    announce: Option<AnnounceCtx>,
}

#[cfg(feature = "iroh-swarm")]
struct AnnounceCtx {
    iroh: Arc<iroh_provider::IrohProvider>,
    relays: Vec<String>,
    tor_proxy: Option<String>,
    identity_dir: std::path::PathBuf,
}

static RUNTIME: OnceLock<SwarmRuntime> = OnceLock::new();

/// Build the swarm runtime once, at startup. Idempotent: a second call is a
/// no-op (the first registration wins). Safe to call unconditionally — when the
/// `iroh-swarm` feature is absent, or `enabled` is false, it registers an empty
/// runtime so every fetch goes straight to origin (today's path).
///
/// `relays` / `tor_proxy` come from the node's Nostr config and double as the
/// seed-advert transport; `data_dir` hosts the persistent iroh blob store under
/// `data_dir/iroh-blobs` and the node identity under `data_dir/identity`.
pub async fn init(
    data_dir: &Path,
    relays: &[String],
    tor_proxy: Option<&str>,
    enabled: bool,
) -> Result<()> {
    if RUNTIME.get().is_some() {
        return Ok(());
    }

    #[cfg(not(feature = "iroh-swarm"))]
    {
        let _ = (data_dir, relays, tor_proxy);
        if enabled {
            warn!("swarm: swarm_enabled set but binary built without the `iroh-swarm` feature — staying origin-only");
        }
        let _ = RUNTIME.set(SwarmRuntime {
            providers: Vec::new(),
        });
        return Ok(());
    }

    #[cfg(feature = "iroh-swarm")]
    {
        if !enabled {
            info!("swarm: disabled (swarm_enabled=false) — origin-only");
            let _ = RUNTIME.set(SwarmRuntime {
                providers: Vec::new(),
                announce: None,
            });
            return Ok(());
        }

        let discovery: Arc<dyn iroh_provider::ProviderDiscovery> = Arc::new(
            iroh_provider::NostrSeedDiscovery::new(relays.to_vec(), tor_proxy.map(str::to_string)),
        );
        let provider = Arc::new(iroh_provider::IrohProvider::new(data_dir, Some(discovery)).await?);
        info!(
            "swarm: iroh provider active (endpoint {}) — swarm-assist enabled, origin always wins",
            provider.endpoint_id()
        );
        let providers: Vec<Arc<dyn BlobProvider>> = vec![provider.clone()];
        let _ = RUNTIME.set(SwarmRuntime {
            providers,
            announce: Some(AnnounceCtx {
                iroh: provider,
                relays: relays.to_vec(),
                tor_proxy: tor_proxy.map(str::to_string),
                identity_dir: data_dir.join("identity"),
            }),
        });
        Ok(())
    }
}

/// The ordered list of swarm providers to consult before the origin.
///
/// Empty until [`init`] registers a provider (needs the `iroh-swarm` feature +
/// `swarm_enabled`). While empty, [`fetch_content_addressed`] goes straight to
/// origin — byte-for-byte today's path.
pub fn providers() -> Vec<Arc<dyn BlobProvider>> {
    RUNTIME
        .get()
        .map(|r| r.providers.clone())
        .unwrap_or_default()
}

/// Announce that this node now holds a PUBLIC release/catalog blob (addressed by
/// `blake3_hex`, bytes at `path`) so peers can fetch it from us: import it into
/// the seed store and publish a signed Nostr advert. Best-effort and inert
/// unless the iroh provider is active — a failure never affects the install.
///
/// **Scope:** call only for releases/catalog content, never private user blobs.
pub async fn announce_held_blob(_blake3_hex: &str, _path: &Path) {
    #[cfg(feature = "iroh-swarm")]
    {
        let Some(rt) = RUNTIME.get() else { return };
        let Some(ctx) = rt.announce.as_ref() else {
            return;
        };
        if let Err(e) = ctx
            .iroh
            .seed_and_advertise(
                _path,
                _blake3_hex,
                &ctx.identity_dir,
                &ctx.relays,
                ctx.tor_proxy.as_deref(),
            )
            .await
        {
            warn!("swarm: failed to announce held blob {_blake3_hex}: {e}");
        }
    }
}

/// Fetch content-addressed bytes: swarm-assist, origin always wins.
///
/// Tries each provider in order; the first to write bytes that VERIFY against
/// `digest` wins and returns [`FetchSource::Swarm`]. If none succeed, runs
/// `origin` (the guaranteed HTTP fallback) and returns [`FetchSource::Origin`].
/// A node that obtained bytes from the swarm has, by definition, a verified
/// copy it can itself seed afterwards.
pub async fn fetch_content_addressed<F, Fut>(
    digest: &ContentDigest,
    providers: &[Arc<dyn BlobProvider>],
    dest: &Path,
    origin: F,
) -> Result<FetchSource>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    for provider in providers {
        match provider.try_fetch(digest, dest).await {
            Ok(true) => match verify_dest(digest, dest).await {
                Ok(()) => {
                    info!("swarm: {} served {} (verified)", provider.name(), digest);
                    return Ok(FetchSource::Swarm);
                }
                Err(e) => {
                    // A peer served bytes that don't match the digest — could be
                    // corruption or a malicious seed. Discard and try the next
                    // source; never let unverified peer bytes through.
                    warn!(
                        "swarm: {} served bytes failing verification for {}: {} — discarding",
                        provider.name(),
                        digest,
                        e
                    );
                    let _ = tokio::fs::remove_file(dest).await;
                }
            },
            Ok(false) => debug!("swarm: {} does not have {}", provider.name(), digest),
            Err(e) => debug!("swarm: {} failed for {}: {}", provider.name(), digest, e),
        }
    }

    debug!(
        "swarm: no provider served {} — falling back to origin",
        digest
    );
    origin().await?;
    Ok(FetchSource::Origin)
}

/// Read `dest` and verify it hashes to `digest`.
async fn verify_dest(digest: &ContentDigest, dest: &Path) -> Result<()> {
    let bytes = tokio::fs::read(dest).await?;
    digest.verify(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn digest_of(bytes: &[u8]) -> ContentDigest {
        ContentDigest::parse(&format!(
            "blake3:{}",
            crate::content_hash::blake3_hex(bytes)
        ))
        .unwrap()
    }

    /// Provider that writes a fixed payload (which may or may not match).
    struct FixedProvider {
        name: &'static str,
        payload: Option<Vec<u8>>,
    }
    #[async_trait]
    impl BlobProvider for FixedProvider {
        fn name(&self) -> &str {
            self.name
        }
        async fn try_fetch(&self, _d: &ContentDigest, dest: &Path) -> Result<bool> {
            match &self.payload {
                Some(p) => {
                    tokio::fs::write(dest, p).await?;
                    Ok(true)
                }
                None => Ok(false),
            }
        }
    }

    fn arc(p: FixedProvider) -> Arc<dyn BlobProvider> {
        Arc::new(p)
    }

    #[tokio::test]
    async fn swarm_hit_verifies_and_skips_origin() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out");
        let content = b"hello swarm".to_vec();
        let digest = digest_of(&content);
        let providers = vec![arc(FixedProvider {
            name: "good",
            payload: Some(content.clone()),
        })];
        let origin_ran = AtomicBool::new(false);
        let src = fetch_content_addressed(&digest, &providers, &dest, || async {
            origin_ran.store(true, Ordering::SeqCst);
            tokio::fs::write(&dest, b"from-origin").await?;
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(src, FetchSource::Swarm);
        assert!(
            !origin_ran.load(Ordering::SeqCst),
            "origin must not run on swarm hit"
        );
        assert_eq!(tokio::fs::read(&dest).await.unwrap(), content);
    }

    #[tokio::test]
    async fn bad_swarm_bytes_are_discarded_and_origin_wins() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out");
        let content = b"the real bytes".to_vec();
        let digest = digest_of(&content);
        // Provider claims a hit but serves tampered bytes.
        let providers = vec![arc(FixedProvider {
            name: "evil",
            payload: Some(b"TAMPERED".to_vec()),
        })];
        let src = fetch_content_addressed(&digest, &providers, &dest, || async {
            tokio::fs::write(&dest, &content).await?;
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(
            src,
            FetchSource::Origin,
            "tampered swarm bytes must not be accepted"
        );
        assert_eq!(tokio::fs::read(&dest).await.unwrap(), content);
    }

    #[tokio::test]
    async fn no_providers_goes_straight_to_origin() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out");
        let content = b"x".to_vec();
        let digest = digest_of(&content);
        let providers: Vec<Arc<dyn BlobProvider>> = vec![];
        let src = fetch_content_addressed(&digest, &providers, &dest, || async {
            tokio::fs::write(&dest, &content).await?;
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(src, FetchSource::Origin);
    }

    #[tokio::test]
    async fn falls_through_providers_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out");
        let content = b"second wins".to_vec();
        let digest = digest_of(&content);
        let providers = vec![
            arc(FixedProvider {
                name: "miss",
                payload: None,
            }),
            arc(FixedProvider {
                name: "hit",
                payload: Some(content.clone()),
            }),
        ];
        let src = fetch_content_addressed(&digest, &providers, &dest, || async {
            tokio::fs::write(&dest, b"origin").await?;
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(src, FetchSource::Swarm);
        assert_eq!(tokio::fs::read(&dest).await.unwrap(), content);
    }
}
