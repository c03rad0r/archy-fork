use super::{build_response, ApiHandler};
use crate::content_lightning::{Binding, Journal, Phase};
use anyhow::{Context, Result};
use hyper::{body::HttpBody, Body, Method, Request, Response, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
pub(crate) const ROUTE: &str = "/content/lightning/v1/operation";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Operation {
    pub binding: Binding,
    pub action: String,
}
impl ApiHandler {
    pub(super) async fn handle_lightning_purchase(
        &self,
        mut request: Request<Body>,
    ) -> Result<Response<Body>> {
        anyhow::ensure!(
            request.method() == Method::POST && request.uri().path() == ROUTE,
            "Invalid invoice route"
        );
        let bytes = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let mut bytes = Vec::new();
            while let Some(chunk) = request.body_mut().data().await {
                let chunk = chunk?;
                anyhow::ensure!(
                    bytes.len() + chunk.len() <= 16384,
                    "Invoice request too large"
                );
                bytes.extend_from_slice(&chunk)
            }
            Ok::<_, anyhow::Error>(bytes)
        })
        .await
        .context("Invoice request timed out")??;
        let seller = crate::identity::did_key_from_pubkey_hex(&self.self_pubkey_hex)?;
        let buyer = crate::content_auth::authenticate_request(
            request.headers(),
            &seller,
            &Method::POST,
            ROUTE,
            &bytes,
            chrono::Utc::now().timestamp(),
        )?;
        let operation: Operation = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(
            operation.binding.buyer_did == buyer && operation.binding.seller_did == seller,
            "Invoice peer identity mismatch"
        );
        anyhow::ensure!(
            matches!(
                operation.action.as_str(),
                "create" | "status" | "cancel" | "download"
            ),
            "Invalid invoice action"
        );
        let binding = &operation.binding;
        let journal = Journal::open(&self.config.data_dir).await?;
        let mut saved = journal.seller(binding)?;
        if saved.is_none() {
            anyhow::ensure!(
                operation.action == "create",
                "Unknown original invoice operation"
            );
            anyhow::ensure!(
                !binding.content_id.starts_with("registered_"),
                "Registered rentals use their native purchase contract"
            );
            let catalog = crate::content_server::load_catalog(&self.config.data_dir).await?;
            let item = catalog
                .items
                .iter()
                .find(|v| v.id == binding.content_id)
                .context("Shared item unavailable")?;
            let visible = match &item.availability {
                crate::content_server::Availability::Nobody => false,
                crate::content_server::Availability::AllPeers => true,
                crate::content_server::Availability::Specific { peers } => peers.contains(&buyer),
            };
            anyhow::ensure!(visible, "Item is not shared with this buyer");
            anyhow::ensure!(
                matches!(&item.access,crate::content_server::AccessControl::Paid{price_sats,..} if *price_sats==binding.price_sats)
                    && crate::content_server::method_accepted(&item.access, "lightning"),
                "Invoice price or accepted method changed"
            );
            crate::content_server::ensure_payment_source_available(&self.config.data_dir, item)
                .await?;
            let source = crate::content_server::content_file_path(&self.config.data_dir, item);
            let roots = [
                self.config.data_dir.join("content/files"),
                self.config.data_dir.join("filebrowser"),
            ];
            let (root, relative) = roots
                .iter()
                .find_map(|root| {
                    source
                        .strip_prefix(root)
                        .ok()
                        .map(|p| (root.clone(), p.to_path_buf()))
                })
                .context("Unsupported invoice source root")?;
            let data = self.config.data_dir.clone();
            let id = binding.content_id.clone();
            struct CancelCopy(std::sync::Arc<std::sync::atomic::AtomicBool>);
            impl Drop for CancelCopy {
                fn drop(&mut self) {
                    self.0.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            let cancel_copy = CancelCopy(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                false,
            )));
            let cancelled = cancel_copy.0.clone();
            let snapshot = tokio::task::spawn_blocking(move || {
                crate::content_snapshot::prepare(
                    &data,
                    &root,
                    &id,
                    &relative,
                    &crate::media_registration::Limits {
                        max_bytes: 64 * 1024 * 1024 * 1024,
                        cancelled: &cancelled,
                    },
                    64 * 1024 * 1024 * 1024,
                    512 * 1024 * 1024,
                    |_| Ok(()),
                )
            })
            .await??;
            anyhow::ensure!(
                snapshot.size == item.size_bytes,
                "Shared file changed before invoice"
            );
            // Source metadata is private and committed before AddInvoice dispatch.
            let record = crate::content_server::publish_snapshot_invoice(
                &self.config.data_dir,
                item,
                &journal,
                binding.clone(),
                crate::content_lightning::RetainedFile {
                    sha256: snapshot.sha256,
                    size: snapshot.size,
                    filename: item.filename.clone(),
                    mime_type: item.mime_type.clone(),
                },
            )
            .await?;
            saved = Some(record);
        }
        let mut saved = saved.context("Missing invoice operation")?;
        anyhow::ensure!(
            saved.source.is_some(),
            "Original invoice source is not prepared; no new invoice dispatched"
        );
        let status = if operation.action == "cancel" && saved.phase == Phase::Prepared {
            saved.phase = Phase::CanceledUnpaid;
            journal.save_seller(&saved)?;
            saved.status()
        } else if operation.action != "create"
            && operation.action != "cancel"
            && saved.phase == Phase::Prepared
        {
            saved.status()
        } else {
            self.rpc_handler
                .drive_external_invoice(&journal, binding, operation.action == "cancel")
                .await?
        };
        // The original legacy delivery mechanism remains usable by its hash.
        if status.bolt11.is_some() {
            crate::content_invoice::record_pending(
                &self.config.data_dir,
                &status.payment_hash,
                &binding.content_id,
                binding.price_sats,
            )
            .await?;
            if status.state == Phase::Settled {
                crate::content_invoice::mark_paid(&self.config.data_dir, &status.payment_hash)
                    .await?;
            }
        }
        if operation.action == "download" {
            anyhow::ensure!(
                status.state == Phase::Settled,
                "Original invoice has not settled"
            );
            let source = status
                .source
                .as_ref()
                .context("Original invoice snapshot is missing")?;
            let data = self.config.data_dir.clone();
            let id = binding.content_id.clone();
            let retained = source.clone();
            // Bounded (P2): unbounded spawn_blocking let a media burst starve the shared
            // blocking pool.
            let snapshot = crate::content_snapshot::open_matching_bounded(
                data,
                id,
                retained.sha256.clone(),
                retained.size,
            )
            .await?;
            let stream = futures_util::stream::try_unfold(
                (tokio::fs::File::from_std(snapshot.file), source.size),
                |(mut file, left)| async move {
                    if left == 0 {
                        return Ok::<_, std::io::Error>(None);
                    }
                    let mut bytes = vec![0; left.min(65536) as usize];
                    let count = file.read(&mut bytes).await?;
                    if count == 0 {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::UnexpectedEof,
                            "Original invoice snapshot ended early",
                        ));
                    }
                    bytes.truncate(count);
                    Ok(Some((bytes, (file, left - count as u64))))
                },
            );
            return Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", &source.mime_type)
                .header("Content-Length", source.size)
                .header("Cache-Control", "private, no-store")
                .body(Body::wrap_stream(stream))?);
        }
        Ok(build_response(
            StatusCode::OK,
            "application/json",
            Body::from(serde_json::to_vec(&status)?),
        ))
    }
}
