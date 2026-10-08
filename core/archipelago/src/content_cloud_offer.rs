//! Ordinary owner-shared Cloud offers reuse a retained immutable version; they
//! never copy a large file for each buyer. Snapshot copying happens off-runtime.
use crate::{
    content_purchase_protocol::Offer,
    content_server::{self, AccessControl, Availability},
    wallet::ecash::EcashNetwork,
};
use anyhow::{Context, Result};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
pub(crate) struct SnapshotPolicy {
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub minimum_free_bytes: u64,
}
fn visible(item: &content_server::ContentItem, buyer: &str) -> bool {
    match &item.availability {
        Availability::Nobody => false,
        Availability::AllPeers => true,
        Availability::Specific { peers } => peers.iter().any(|did| did == buyer),
    }
}
/// Caller identities come from v2 authentication/current node identity, not body.
pub(crate) async fn offer(
    data_dir: &Path,
    id: &str,
    content_id: &str,
    buyer: &str,
    seller: &str,
    network: EcashNetwork,
    mint: &str,
    policy: SnapshotPolicy,
) -> Result<Offer> {
    crate::content_purchase_protocol::ensure_seller_mint_policy(data_dir, network, mint).await?;
    let catalog = content_server::load_catalog(data_dir).await?;
    let item = catalog
        .items
        .into_iter()
        .find(|item| item.id == content_id)
        .context("Shared content is unavailable")?;
    anyhow::ensure!(
        visible(&item, buyer),
        "Content is not shared with this buyer"
    );
    let price = match &item.access {
        AccessControl::Paid { price_sats, .. } if *price_sats > 0 => *price_sats,
        _ => anyhow::bail!("This shared item does not require a payment"),
    };
    anyhow::ensure!(
        content_server::method_accepted(&item.access, "ecash")
            || content_server::method_accepted(&item.access, "cashu"),
        "This shared item does not accept Cashu"
    );
    content_server::ensure_payment_source_available(data_dir, &item).await?;
    let source = content_server::content_file_path(data_dir, &item);
    let roots = [data_dir.join("content/files"), data_dir.join("filebrowser")];
    let (root, relative): (std::path::PathBuf, std::path::PathBuf) = roots
        .iter()
        .find_map(|root| {
            source
                .strip_prefix(root)
                .ok()
                .map(|relative| (root.clone(), relative.to_path_buf()))
        })
        .context("Content has no configured source root")?;
    let data = data_dir.to_path_buf();
    let selected = content_id.to_owned();
    struct CancelCopy(Arc<AtomicBool>);
    impl Drop for CancelCopy {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let cancel_copy = CancelCopy(Arc::new(AtomicBool::new(false)));
    let cancelled = cancel_copy.0.clone();
    let snapshot = tokio::task::spawn_blocking(move || {
        crate::content_snapshot::prepare(
            &data,
            &root,
            &selected,
            &relative,
            &crate::media_registration::Limits {
                max_bytes: policy.max_file_bytes,
                cancelled: &cancelled,
            },
            policy.max_total_bytes,
            policy.minimum_free_bytes,
            |_| Ok(()),
        )
    })
    .await??;
    anyhow::ensure!(
        snapshot.size == item.size_bytes,
        "Shared file changed; refresh its catalog before accepting payment"
    );
    let terms = {
        use sha2::{Digest, Sha256};
        hex::encode(Sha256::digest(serde_json::to_vec(&(
            "archipelago-cloud-purchase-terms-v1",
            seller,
            content_id,
            &snapshot.sha256,
            snapshot.size,
            price,
            "permanent-download",
            &item.filename,
            &item.mime_type,
            "cashu",
        ))?))
    };
    let now = chrono::Utc::now().timestamp();
    let offer = Offer {
        id: id.into(),
        buyer_did: buyer.into(),
        seller_did: seller.into(),
        content_id: content_id.into(),
        filename: item.filename.clone(),
        mime_type: item.mime_type.clone(),
        content_sha256: snapshot.sha256,
        content_size: snapshot.size,
        viewing_seconds: None,
        terms_sha256: terms,
        network,
        mint_url: mint.into(),
        seller_net_sats: price,
        offered_at: now,
        expires_at: now.checked_add(120).context("Offer clock overflow")?,
    };
    // Recheck owner visibility/price under the catalog writer lock when publishing
    // the offer, so an unshare during a large snapshot copy blocks NEW offers.
    content_server::publish_snapshot_offer(data_dir, &item, &offer).await
}
