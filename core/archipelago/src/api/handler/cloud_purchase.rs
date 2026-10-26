//! Permanent Cloud snapshot delivery. Current source path/share state cannot
//! revoke a settled immutable snapshot; no rental clock is started here.
use super::{build_response, ApiHandler};
use crate::{
    content_purchase::{Journal, SellerPhase},
    content_server::ByteRange,
};
use anyhow::{Context, Result};
use hyper::{Body, HeaderMap, Response, StatusCode};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
impl ApiHandler {
    pub(super) async fn handle_cloud_purchase(
        &self,
        path: &str,
        headers: &HeaderMap,
    ) -> Result<Response<Body>> {
        let (content_id, purchase_id) = path
            .strip_prefix("/content/")
            .and_then(|value| value.split_once("/purchase/"))
            .context("Invalid purchase delivery route")?;
        anyhow::ensure!(
            !content_id.contains('/')
                && !content_id.starts_with("registered_")
                && !purchase_id.contains('/'),
            "Invalid Cloud delivery route"
        );
        let audience = crate::identity::did_key_from_pubkey_hex(&self.self_pubkey_hex)?;
        let buyer = crate::content_auth::incoming(
            headers,
            &audience,
            path,
            chrono::Utc::now().timestamp(),
        )?
        .context("Authenticated peer proof is required")?;
        let mut values = headers.get_all("x-content-capability").iter();
        let capability = values
            .next()
            .context("Delivery capability is required")?
            .to_str()?;
        anyhow::ensure!(values.next().is_none(), "Duplicate delivery capability");
        let (contract, mime) = {
            let journal = Journal::open(&self.config.data_dir).await?;
            let record = journal
                .seller(purchase_id)
                .await?
                .context("Purchase settlement not found")?;
            let receipt = match record.phase {
                SellerPhase::ReceiptSaved(receipt) => receipt,
                _ => anyhow::bail!("Purchase settlement is not durable"),
            };
            anyhow::ensure!(
                record.contract.buyer_did == buyer
                    && record.contract.seller_did == audience
                    && record.contract.content_id == content_id
                    && receipt.capability == capability,
                "Purchase delivery binding changed"
            );
            let envelope = journal
                .protocol_envelope("seller", purchase_id)
                .await?
                .context("Original delivery metadata is missing")?;
            anyhow::ensure!(
                envelope.contract()? == record.contract,
                "Delivery metadata binding changed"
            );
            (record.contract, envelope.offer.mime_type)
        };
        let range = headers
            .get("range")
            .map(|value| -> Result<_> {
                crate::content_server::parse_range_header(value.to_str()?).context("Invalid range")
            })
            .transpose()?;
        let total = contract.content_size;
        let (start, end, partial) = match range {
            None => (0, total - 1, false),
            Some(ByteRange::From { start, end }) => {
                (start, end.unwrap_or(total - 1).min(total - 1), true)
            }
            Some(ByteRange::Suffix(count)) if count > 0 => {
                (total.saturating_sub(count), total - 1, true)
            }
            _ => anyhow::bail!("Invalid range"),
        };
        if start > end || start >= total {
            let mut response = build_response(
                StatusCode::RANGE_NOT_SATISFIABLE,
                "text/plain",
                Body::empty(),
            );
            response
                .headers_mut()
                .insert("content-range", format!("bytes */{total}").parse()?);
            return Ok(response);
        }
        let data = self.config.data_dir.clone();
        // Bounded (P2): an unbounded spawn_blocking per delivery let a purchase burst
        // monopolise the shared blocking pool with directory scans.
        let file = crate::content_snapshot::open_matching_bounded(
            data,
            contract.content_id.clone(),
            contract.content_sha256.clone(),
            contract.content_size,
        )
        .await?;
        let mut file = tokio::fs::File::from_std(file.file);
        file.seek(std::io::SeekFrom::Start(start)).await?;
        let length = end - start + 1;
        let chunks =
            futures_util::stream::try_unfold((file, length), |(mut file, left)| async move {
                if left == 0 {
                    return Ok::<_, std::io::Error>(None);
                }
                let mut bytes = vec![0; left.min(65536) as usize];
                let count = file.read(&mut bytes).await?;
                if count == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "Purchase snapshot ended early",
                    ));
                }
                bytes.truncate(count);
                Ok(Some((bytes, (file, left - count as u64))))
            });
        let mut response = Response::builder()
            .status(if partial {
                StatusCode::PARTIAL_CONTENT
            } else {
                StatusCode::OK
            })
            .header("content-type", mime)
            .header("content-length", length)
            .header("accept-ranges", "bytes")
            .header("cache-control", "private, no-store")
            .header("x-content-type-options", "nosniff")
            .header("content-security-policy", "sandbox; default-src 'none'");
        if partial {
            response = response.header("content-range", format!("bytes {start}-{end}/{total}"));
        }
        Ok(response.body(Body::wrap_stream(crate::media_metrics::counted(chunks)))?)
    }
}
