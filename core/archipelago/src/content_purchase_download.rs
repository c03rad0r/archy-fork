//! Permanent purchase caching. Hash verification occurs inside the stream before
//! owned-cache publication; receipt remains recoverable after any interruption.
use crate::content_purchase::{Contract, Journal, Receipt};
use anyhow::{Context, Result};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::Path;

pub(crate) async fn cache(
    data_dir: &Path,
    onion: &str,
    contract: &Contract,
    receipt: &Receipt,
) -> Result<crate::content_owned::OwnedItem> {
    anyhow::ensure!(
        !contract.content_id.starts_with("registered_"),
        "Rentals use the scoped playback proxy, not permanent caching"
    );
    let envelope = {
        let journal = Journal::open(data_dir).await?;
        let buyer = journal
            .buyer(&contract.id)
            .await?
            .context("Buyer purchase is missing")?;
        anyhow::ensure!(
            buyer.contract == *contract && buyer.receipt() == Some(receipt),
            "Original buyer receipt changed"
        );
        journal
            .protocol_envelope("buyer", &contract.id)
            .await?
            .context("Original delivery metadata is missing")?
    };
    let peer = crate::federation::load_unique_payment_peer(data_dir, onion).await?;
    anyhow::ensure!(peer.did == contract.seller_did, "Delivery seller changed");
    let path = format!("/content/{}/purchase/{}", contract.content_id, contract.id);
    let (response, _) =
        crate::fips::dial::PeerRequest::new(peer.fips_npub.as_deref(), onion, &path)
            .require_fips()
            .single_delivery()
            .timeout(std::time::Duration::from_secs(900))
            .header("X-Content-Capability", receipt.capability.clone())
            .send_content_get(data_dir)
            .await?;
    anyhow::ensure!(
        response.status() == reqwest::StatusCode::OK,
        "Original purchase delivery is unavailable; no new payment sent"
    );
    anyhow::ensure!(
        response.content_length() == Some(contract.content_size),
        "Original snapshot length changed"
    );
    let stream = verified_stream(
        response.bytes_stream(),
        contract.content_sha256.clone(),
        contract.content_size,
    );
    let owned = crate::content_owned::record_purchase_stream(
        data_dir,
        crate::content_owned::OwnedItem {
            onion: onion.into(),
            content_id: contract.content_id.clone(),
            filename: envelope.offer.filename,
            mime_type: envelope.offer.mime_type,
            size_bytes: contract.content_size,
            paid_sats: contract.gross_token_sats,
            ecash_backend: "cashu".into(),
            purchased_at: chrono::Utc::now().to_rfc3339(),
            download_complete: false,
        },
        Box::pin(stream),
        Some(contract.content_size),
    )
    .await?;
    Journal::open(data_dir)
        .await?
        .record_delivery(contract, &contract.content_sha256, contract.content_size)
        .await?;
    Ok(owned)
}

pub(crate) fn verified_stream<S, E>(
    stream: S,
    expected_hash: String,
    expected_size: u64,
) -> impl futures_util::Stream<Item = std::io::Result<bytes::Bytes>>
where
    S: futures_util::Stream<Item = Result<bytes::Bytes, E>>,
    E: std::error::Error + Send + Sync + 'static,
{
    futures_util::stream::try_unfold(
        (Box::pin(stream), Sha256::new(), 0u64),
        move |(mut stream, mut hash, total)| {
            let expected_hash = expected_hash.clone();
            async move {
                match stream.next().await {
                    Some(chunk) => {
                        let chunk = chunk.map_err(std::io::Error::other)?;
                        let total = total
                            .checked_add(chunk.len() as u64)
                            .filter(|n| *n <= expected_size)
                            .ok_or_else(|| {
                                std::io::Error::other("Snapshot exceeded accepted size")
                            })?;
                        hash.update(&chunk);
                        Ok(Some((chunk, (stream, hash, total))))
                    }
                    None => {
                        if total != expected_size || hex::encode(hash.finalize()) != expected_hash {
                            return Err(std::io::Error::other(
                                "Snapshot did not match accepted hash/size",
                            ));
                        }
                        Ok::<_, std::io::Error>(None)
                    }
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_owned::{self, OwnedItem};
    fn item() -> OwnedItem {
        OwnedItem {
            onion: "seller.onion".into(),
            content_id: "video".into(),
            filename: "clip.mp4".into(),
            mime_type: "video/mp4".into(),
            size_bytes: 4,
            paid_sats: 8,
            ecash_backend: "cashu".into(),
            purchased_at: "now".into(),
            download_complete: false,
        }
    }
    #[tokio::test]
    async fn corrupted_truncated_and_excess_bytes_remain_recoverable_until_original_hash_arrives() {
        let root = tempfile::tempdir().unwrap();
        let hash = hex::encode(Sha256::digest(b"good"));
        for bytes in [b"evil".as_slice(), b"goo".as_slice(), b"good!".as_slice()] {
            let input = futures_util::stream::iter([Ok::<_, std::io::Error>(
                bytes::Bytes::copy_from_slice(bytes),
            )]);
            let stream = Box::pin(verified_stream(input, hash.clone(), 4));
            assert!(
                content_owned::record_purchase_stream(root.path(), item(), stream, Some(4))
                    .await
                    .is_err()
            );
            let entries = content_owned::list_owned_checked(root.path())
                .await
                .unwrap();
            assert_eq!(entries.len(), 1);
            assert!(!entries[0].download_complete);
            assert_eq!(entries[0].paid_sats, 8);
            let error = content_owned::open_owned(root.path(), "seller.onion", "video")
                .await
                .err()
                .expect("Incomplete paid bytes must not be served");
            assert!(
                error.to_string().contains("delivery is incomplete"),
                "{error:#}"
            );
        }
        let input = futures_util::stream::iter([
            Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"go")),
            Ok(bytes::Bytes::from_static(b"od")),
        ]);
        let stream = Box::pin(verified_stream(input, hash, 4));
        let completed = content_owned::record_purchase_stream(root.path(), item(), stream, Some(4))
            .await
            .unwrap();
        assert!(completed.download_complete);
        let (_, mut file) = content_owned::open_owned(root.path(), "seller.onion", "video")
            .await
            .unwrap()
            .unwrap();
        let mut bytes = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut file, &mut bytes)
            .await
            .unwrap();
        assert_eq!(bytes, b"good");
        assert_eq!(
            content_owned::list_owned_checked(root.path())
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
