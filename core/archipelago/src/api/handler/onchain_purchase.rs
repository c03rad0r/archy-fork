use super::{build_response, ApiHandler};
use crate::{content_lightning::Binding, content_onchain_seller::Journal};
use anyhow::{Context, Result};
use hyper::{body::HttpBody, Body, Method, Request, Response, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
pub(crate) const ROUTE: &str = "/content/onchain/v1/operation";
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Operation {
    pub binding: Binding,
    pub action: String,
}
// Load wallet credentials only after authenticated request validation reaches a
// wallet operation. Tests inject the same typed boundary without live services.
struct NativeSellerWallet<'a>(&'a crate::api::rpc::RpcHandler);
impl crate::content_onchain_seller::Wallet for NativeSellerWallet<'_> {
    async fn network(&self) -> Result<crate::content_onchain::ChainNetwork> {
        self.0.onchain_purchase_wallet().await?.network().await
    }
    async fn preflight(&self, network: crate::content_onchain::ChainNetwork) -> Result<()> {
        self.0
            .onchain_purchase_wallet()
            .await?
            .preflight(network)
            .await
    }
    async fn allocate(&self) -> Result<String> {
        self.0.onchain_purchase_wallet().await?.allocate().await
    }
    async fn received(&self, address: &str, amount: u64) -> Result<bool> {
        self.0
            .onchain_purchase_wallet()
            .await?
            .received(address, amount)
            .await
    }
}
impl ApiHandler {
    pub(super) async fn handle_onchain_purchase(
        &self,
        request: Request<Body>,
    ) -> Result<Response<Body>> {
        self.handle_onchain_purchase_with_wallet(request, &NativeSellerWallet(&self.rpc_handler))
            .await
    }
    async fn handle_onchain_purchase_with_wallet<W: crate::content_onchain_seller::Wallet>(
        &self,
        mut request: Request<Body>,
        wallet: &W,
    ) -> Result<Response<Body>> {
        anyhow::ensure!(
            request.method() == Method::POST && request.uri().path() == ROUTE,
            "Invalid on-chain purchase route"
        );
        let bytes = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let mut bytes = Vec::new();
            while let Some(chunk) = request.body_mut().data().await {
                let chunk = chunk?;
                anyhow::ensure!(
                    bytes.len() + chunk.len() <= 16384,
                    "On-chain purchase request too large"
                );
                bytes.extend_from_slice(&chunk)
            }
            Ok::<_, anyhow::Error>(bytes)
        })
        .await
        .context("On-chain purchase request timed out")??;
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
            "On-chain purchase peer identity mismatch"
        );
        anyhow::ensure!(
            matches!(
                operation.action.as_str(),
                "create" | "offer" | "allocate" | "status" | "download" | "cancel"
            ),
            "Invalid on-chain purchase action"
        );
        let binding = &operation.binding;
        let journal = Journal::open(&self.config.data_dir).await?;
        let retired = if operation.action == "cancel" {
            Some(journal.retire_unallocated(binding)?)
        } else {
            journal.retirement(binding)?
        };
        if let Some(ack) = retired {
            return Ok(build_response(
                StatusCode::OK,
                "application/json",
                Body::from(serde_json::to_vec(&ack)?),
            ));
        }
        let mut saved = journal.load(binding)?;
        if saved.is_none() {
            anyhow::ensure!(
                matches!(operation.action.as_str(), "create" | "offer"),
                "Unknown original on-chain purchase operation"
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
                    && crate::content_server::method_accepted(&item.access, "onchain"),
                "On-chain purchase price or accepted method changed"
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
                .context("Unsupported on-chain purchase source root")?;
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
                "Shared file changed before on-chain purchase"
            );
            // Source metadata is private and committed before address allocation.
            let record = crate::content_server::publish_snapshot_onchain(
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
                wallet.network().await?,
            )
            .await?;
            saved = Some(record);
        }
        saved.context("Missing original on-chain operation")?;
        let status = if operation.action == "allocate" {
            crate::content_server::allocate_onchain_offer(
                &self.config.data_dir,
                &journal,
                binding,
                wallet,
            )
            .await?
        } else {
            crate::content_onchain_seller::drive(&journal, binding, false, wallet).await?
        };
        if operation.action == "download" {
            anyhow::ensure!(status.paid, "Original on-chain purchase has not settled");
            let source = &status.source;
            let data = self.config.data_dir.clone();
            let id = binding.content_id.clone();
            let retained = source.clone();
            let snapshot = tokio::task::spawn_blocking(move || {
                crate::content_snapshot::open_matching(&data, &id, &retained.sha256, retained.size)
            })
            .await??;
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
                            "Original on-chain purchase snapshot ended early",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content_onchain_seller::{Allocation, UnallocatedAck};
    use hyper::service::{make_service_fn, service_fn};
    use std::{convert::Infallible, sync::Arc};
    #[derive(Default)]
    struct MockWallet {
        allocations: std::sync::atomic::AtomicUsize,
        lose_reply: std::sync::atomic::AtomicBool,
    }
    impl crate::content_onchain_seller::Wallet for MockWallet {
        async fn network(&self) -> Result<crate::content_onchain::ChainNetwork> {
            Ok(crate::content_onchain::ChainNetwork::Regtest)
        }
        async fn preflight(&self, _: crate::content_onchain::ChainNetwork) -> Result<()> {
            Ok(())
        }
        async fn allocate(&self) -> Result<String> {
            self.allocations
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            anyhow::ensure!(
                !self
                    .lose_reply
                    .swap(false, std::sync::atomic::Ordering::SeqCst),
                "Simulated lost allocation response"
            );
            let mut bytes = vec![0, 20];
            bytes.extend([17u8; 20]);
            Ok(bitcoin::Address::from_script(
                &bitcoin::ScriptBuf::from_bytes(bytes),
                bitcoin::Network::Regtest,
            )?
            .to_string())
        }
        async fn received(&self, _: &str, _: u64) -> Result<bool> {
            Ok(false)
        }
    }
    struct HttpFixture {
        wallet: Arc<MockWallet>,
        data: tempfile::TempDir,
        _buyer_data: tempfile::TempDir,
        buyer: crate::identity::NodeIdentity,
        seller: String,
        url: String,
        task: tokio::task::JoinHandle<()>,
    }
    impl Drop for HttpFixture {
        fn drop(&mut self) {
            self.task.abort();
        }
    }
    async fn fixture() -> HttpFixture {
        let data = tempfile::tempdir().unwrap();
        let buyer_data = tempfile::tempdir().unwrap();
        let buyer = crate::identity::NodeIdentity::load_or_create(buyer_data.path())
            .await
            .unwrap();
        let mut config = crate::config::Config::default();
        config.data_dir = data.path().to_path_buf();
        let handler = Arc::new(
            ApiHandler::new(
                config,
                Arc::new(crate::state::StateManager::new()),
                Arc::new(crate::monitoring::MetricsStore::new()),
                None,
                None,
            )
            .await
            .unwrap(),
        );
        let seller = crate::identity::did_key_from_pubkey_hex(&handler.self_pubkey_hex).unwrap();
        let wallet = Arc::new(MockWallet::default());
        let server_wallet = wallet.clone();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = hyper::Server::from_tcp(listener)
            .unwrap()
            .serve(make_service_fn(move |_| {
                let handler = handler.clone();
                let wallet = server_wallet.clone();
                async move {
                    Ok::<_, Infallible>(service_fn(move |request| {
                        let handler = handler.clone();
                        let wallet = wallet.clone();
                        async move {
                            Ok::<_, Infallible>(
                                handler
                                    .handle_onchain_purchase_with_wallet(request, wallet.as_ref())
                                    .await
                                    .unwrap_or_else(|_| {
                                        build_response(
                                            StatusCode::BAD_REQUEST,
                                            "application/json",
                                            Body::from("{\"error\":\"rejected\"}"),
                                        )
                                    }),
                            )
                        }
                    }))
                }
            }));
        let task = tokio::spawn(async move {
            server.await.unwrap();
        });
        HttpFixture {
            wallet,
            data,
            _buyer_data: buyer_data,
            buyer,
            seller,
            url,
            task,
        }
    }
    impl HttpFixture {
        fn binding(&self) -> Binding {
            Binding {
                id: uuid::Uuid::new_v4().to_string(),
                buyer_did: self.buyer.did_key().unwrap(),
                seller_did: self.seller.clone(),
                content_id: "file".into(),
                price_sats: 546,
            }
        }
        async fn send(
            &self,
            body: &[u8],
            signed_body: Option<&[u8]>,
            audience: Option<&str>,
        ) -> reqwest::Response {
            let mut request = reqwest::Client::new()
                .post(format!("{}{}", self.url, ROUTE))
                .header("content-type", "application/json")
                .body(body.to_vec());
            if let Some(signed) = signed_body {
                let proof = crate::content_auth::sign_request(
                    &self.buyer,
                    audience.unwrap_or(&self.seller),
                    &Method::POST,
                    ROUTE,
                    signed,
                    chrono::Utc::now().timestamp(),
                )
                .unwrap();
                request = request.header(crate::content_auth::REQUEST_HEADER, proof);
            }
            request.send().await.unwrap()
        }
        async fn operation(&self, binding: &Binding, action: &str) -> reqwest::Response {
            let body = serde_json::to_vec(&Operation {
                binding: binding.clone(),
                action: action.into(),
            })
            .unwrap();
            self.send(&body, Some(&body), None).await
        }
    }
    #[tokio::test]
    async fn authenticated_cancel_roundtrip_lost_reply_and_delayed_create_return_same_retirement() {
        let server = fixture().await;
        let binding = server.binding();
        // Drop the original reply after headers: terminal state must already be durable.
        let first = server.operation(&binding, "cancel").await;
        assert_eq!(first.status(), reqwest::StatusCode::OK);
        drop(first);
        let replay = server.operation(&binding, "cancel").await;
        assert_eq!(replay.status(), reqwest::StatusCode::OK);
        let ack: UnallocatedAck = replay.json().await.unwrap();
        ack.validate(&binding).unwrap();
        let delayed = server.operation(&binding, "create").await;
        assert_eq!(delayed.status(), reqwest::StatusCode::OK);
        assert_eq!(delayed.json::<UnallocatedAck>().await.unwrap(), ack);
        let journal = Journal::open(server.data.path()).await.unwrap();
        assert_eq!(journal.retirement(&binding).unwrap(), Some(ack));
        assert!(journal.load(&binding).unwrap().is_none());
        assert!(!server.data.path().join("content-snapshots").exists());
    }
    #[tokio::test]
    async fn cancellation_http_rejects_missing_proof_body_tamper_and_wrong_seller_without_tombstone(
    ) {
        let server = fixture().await;
        let binding = server.binding();
        let body = serde_json::to_vec(&Operation {
            binding: binding.clone(),
            action: "cancel".into(),
        })
        .unwrap();
        assert!(!server.send(&body, None, None).await.status().is_success());
        let mut changed = binding.clone();
        changed.price_sats += 1;
        let changed = serde_json::to_vec(&Operation {
            binding: changed,
            action: "cancel".into(),
        })
        .unwrap();
        assert!(!server
            .send(&changed, Some(&body), None)
            .await
            .status()
            .is_success());
        let wrong = crate::identity::did_key_from_pubkey_hex(&hex::encode([8; 32])).unwrap();
        assert!(!server
            .send(&body, Some(&body), Some(&wrong))
            .await
            .status()
            .is_success());
        let journal = Journal::open(server.data.path()).await.unwrap();
        assert!(journal.retirement(&binding).unwrap().is_none());
    }
    #[tokio::test]
    async fn authenticated_cancel_cannot_retire_dispatched_or_issued_address() {
        let server = fixture().await;
        let mut script = vec![0, 20];
        script.extend([1; 20]);
        let address = bitcoin::Address::from_script(
            &bitcoin::ScriptBuf::from_bytes(script),
            bitcoin::Network::Regtest,
        )
        .unwrap()
        .to_string();
        for allocation in [Allocation::Dispatched, Allocation::Ready { address }] {
            let binding = server.binding();
            let journal = Journal::open(server.data.path()).await.unwrap();
            let mut record = journal
                .prepare(
                    binding.clone(),
                    crate::content_lightning::RetainedFile {
                        sha256: "a".repeat(64),
                        size: 4,
                        filename: "original.txt".into(),
                        mime_type: "text/plain".into(),
                    },
                    crate::content_onchain::ChainNetwork::Regtest,
                )
                .unwrap();
            record.allocation = allocation.clone();
            journal.save(&record).unwrap();
            drop(journal);
            assert!(!server
                .operation(&binding, "cancel")
                .await
                .status()
                .is_success());
            let journal = Journal::open(server.data.path()).await.unwrap();
            assert!(journal.retirement(&binding).unwrap().is_none());
            assert_eq!(
                journal.load(&binding).unwrap().unwrap().allocation,
                allocation
            );
        }
    }
    async fn seed_unallocated_offer(server: &HttpFixture) -> Binding {
        let binding = server.binding();
        crate::content_server::save_catalog(
            server.data.path(),
            &crate::content_server::ContentCatalog {
                items: vec![crate::content_server::ContentItem {
                    id: binding.content_id.clone(),
                    filename: "original.txt".into(),
                    mime_type: "text/plain".into(),
                    size_bytes: 4,
                    description: String::new(),
                    added_at: String::new(),
                    availability: crate::content_server::Availability::AllPeers,
                    access: crate::content_server::AccessControl::Paid {
                        price_sats: 546,
                        accepted: vec!["onchain".into()],
                    },
                }],
            },
        )
        .await
        .unwrap();
        let root = server.data.path().join("content/files");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("original.txt"), b"test").unwrap();
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        let snapshot = crate::content_snapshot::prepare(
            server.data.path(),
            &root,
            &binding.content_id,
            std::path::Path::new("original.txt"),
            &crate::media_registration::Limits {
                max_bytes: 1024,
                cancelled: &cancelled,
            },
            1024 * 1024,
            0,
            |_| Ok(()),
        )
        .unwrap();
        let journal = Journal::open(server.data.path()).await.unwrap();
        journal
            .prepare(
                binding.clone(),
                crate::content_lightning::RetainedFile {
                    sha256: snapshot.sha256,
                    size: 4,
                    filename: "original.txt".into(),
                    mime_type: "text/plain".into(),
                },
                crate::content_onchain::ChainNetwork::Regtest,
            )
            .unwrap();
        binding
    }

    #[tokio::test]
    async fn authenticated_offer_never_allocates_or_returns_a_receive_address() {
        let server = fixture().await;
        let binding = seed_unallocated_offer(&server).await;
        let result = server.operation(&binding, "offer").await;
        assert_eq!(result.status(), reqwest::StatusCode::OK);
        let body: serde_json::Value = result.json().await.unwrap();
        assert_eq!(body["allocation"]["state"], "prepared");
        assert!(body["allocation"].get("address").is_none());
        assert!(body.get("address").is_none());
        let journal = Journal::open(server.data.path()).await.unwrap();
        assert_eq!(
            journal.load(&binding).unwrap().unwrap().allocation,
            Allocation::Prepared
        );
    }

    #[tokio::test]
    async fn reviewed_offer_can_cancel_and_delayed_explicit_allocate_cannot_revive_it() {
        let server = fixture().await;
        let binding = seed_unallocated_offer(&server).await;
        assert_eq!(
            server.operation(&binding, "offer").await.status(),
            reqwest::StatusCode::OK
        );
        let retired: UnallocatedAck = server
            .operation(&binding, "cancel")
            .await
            .json()
            .await
            .unwrap();
        retired.validate(&binding).unwrap();
        // Represents a delayed Pay request from the old modal after cancellation.
        let late = server.operation(&binding, "allocate").await;
        assert_eq!(late.status(), reqwest::StatusCode::OK);
        assert_eq!(late.json::<UnallocatedAck>().await.unwrap(), retired);
        let journal = Journal::open(server.data.path()).await.unwrap();
        assert_eq!(
            journal.load(&binding).unwrap().unwrap().allocation,
            Allocation::Prepared
        );
        assert_eq!(journal.retirement(&binding).unwrap(), Some(retired));
    }

    #[tokio::test]
    async fn changing_authenticated_offer_body_to_allocate_cannot_dispatch_an_address() {
        let server = fixture().await;
        let binding = seed_unallocated_offer(&server).await;
        let reviewed = serde_json::to_vec(&Operation {
            binding: binding.clone(),
            action: "offer".into(),
        })
        .unwrap();
        let changed = serde_json::to_vec(&Operation {
            binding: binding.clone(),
            action: "allocate".into(),
        })
        .unwrap();
        assert!(!server
            .send(&changed, Some(&reviewed), None)
            .await
            .status()
            .is_success());
        let journal = Journal::open(server.data.path()).await.unwrap();
        assert_eq!(
            journal.load(&binding).unwrap().unwrap().allocation,
            Allocation::Prepared
        );
        assert!(journal.retirement(&binding).unwrap().is_none());
    }

    #[tokio::test]
    async fn explicit_allocation_reuses_original_address_after_lost_http_reply() {
        let server = fixture().await;
        let binding = seed_unallocated_offer(&server).await;
        assert!(server
            .operation(&binding, "offer")
            .await
            .status()
            .is_success());
        assert_eq!(
            server
                .wallet
                .allocations
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        // Caller loses the response after seller durability; recovery returns the same record.
        drop(server.operation(&binding, "allocate").await);
        let recovered: crate::content_onchain_seller::Record = server
            .operation(&binding, "allocate")
            .await
            .json()
            .await
            .unwrap();
        assert!(recovered.quote().unwrap().is_some());
        let repeated: crate::content_onchain_seller::Record = server
            .operation(&binding, "allocate")
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(recovered, repeated);
        assert_eq!(
            server
                .wallet
                .allocations
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert!(!server
            .operation(&binding, "cancel")
            .await
            .status()
            .is_success());
    }
    #[tokio::test]
    async fn lost_wallet_allocation_reply_never_allocates_a_second_address() {
        let server = fixture().await;
        let binding = seed_unallocated_offer(&server).await;
        server
            .wallet
            .lose_reply
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(!server
            .operation(&binding, "allocate")
            .await
            .status()
            .is_success());
        let recovered: crate::content_onchain_seller::Record = server
            .operation(&binding, "allocate")
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(recovered.allocation, Allocation::Dispatched);
        assert!(recovered.quote().unwrap().is_none());
        assert_eq!(
            server
                .wallet
                .allocations
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert!(!server
            .operation(&binding, "cancel")
            .await
            .status()
            .is_success());
    }
}
