//! Test-only loopback transport and disposable process restart qualification.
//! This deliberately does not claim authenticated FIPS transport acceptance.
use super::*;
use crate::content_purchase_caller::{purchase, PurchaseConsent, PurchaseTransport, ReadyPurchase};
use crate::content_purchase_protocol::{
    Accepted, Cancelled, Envelope, Offer, SellerStatus, Settlement,
};
use serde::{de::DeserializeOwned, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
const CHILD: &str = "wallet::ecash::payment_tests::process_fixture::synthetic_process_child";
const BYTES: &[u8] = b"disposable paid fixture: exact bytes after process restart";
const ONION: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.onion";

struct HttpTransport {
    endpoint: String,
    seller: String,
}
impl HttpTransport {
    async fn post<B: Serialize + Sync, R: DeserializeOwned>(
        &self,
        route: &str,
        body: &B,
    ) -> anyhow::Result<R> {
        Ok(reqwest::Client::new()
            .post(format!("{}{route}", self.endpoint))
            .timeout(Duration::from_secs(15))
            .json(body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }
}
impl PurchaseTransport for HttpTransport {
    fn seller_did(&self) -> &str {
        &self.seller
    }
    fn seller_onion(&self) -> &str {
        ONION
    }
    async fn offer(&self, id: &str, content_id: &str) -> anyhow::Result<Offer> {
        self.post("/offer", &json!({"id":id,"content_id":content_id}))
            .await
    }
    async fn accept(&self, v: &Envelope) -> anyhow::Result<Accepted> {
        self.post("/accept", v).await
    }
    async fn status(&self, v: &Envelope) -> anyhow::Result<SellerStatus> {
        self.post("/status", v).await
    }
    async fn cancel(&self, v: &Envelope) -> anyhow::Result<Cancelled> {
        self.post("/cancel", v).await
    }
    async fn settle(&self, v: &Settlement) -> anyhow::Result<crate::content_purchase::Receipt> {
        self.post("/settle", v).await
    }
}
fn fixture_offer(root: &Path) -> Offer {
    serde_json::from_slice(&std::fs::read(root.join("offer.json")).unwrap()).unwrap()
}
async fn seller(root: PathBuf) {
    let transport = Arc::new(PurchaseTestTransport {
        seller_root: root.join("seller"),
        template: fixture_offer(&root),
        lose_offer: false.into(),
        lose_accept: false.into(),
        lose_settle: false.into(),
        offers: Default::default(),
    });
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let service = make_service_fn(move |_| {
        let root = root.clone();
        let transport = transport.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req: Request<Body>| {
                let root = root.clone();
                let transport = transport.clone();
                async move {
                    let path = req.uri().path().to_string();
                    let body: Value = serde_json::from_slice(
                        &hyper::body::to_bytes(req.into_body()).await.unwrap(),
                    )
                    .unwrap();
                    let result: Value = match path.as_str() {
                        "/offer" => serde_json::to_value(
                            transport
                                .offer(
                                    body["id"].as_str().unwrap(),
                                    body["content_id"].as_str().unwrap(),
                                )
                                .await
                                .unwrap(),
                        )
                        .unwrap(),
                        "/accept" => serde_json::to_value(
                            transport
                                .accept(&serde_json::from_value(body).unwrap())
                                .await
                                .unwrap(),
                        )
                        .unwrap(),
                        "/status" => serde_json::to_value(
                            transport
                                .status(&serde_json::from_value(body).unwrap())
                                .await
                                .unwrap(),
                        )
                        .unwrap(),
                        "/cancel" => serde_json::to_value(
                            transport
                                .cancel(&serde_json::from_value(body).unwrap())
                                .await
                                .unwrap(),
                        )
                        .unwrap(),
                        "/settle" => {
                            let reply = transport
                                .settle(&serde_json::from_value(body).unwrap())
                                .await
                                .unwrap();
                            // Persisted seller receipt and mint settlement exist BEFORE the
                            // transport deliberately closes without HTTP response headers.
                            if !root.join("settlement-response-lost").exists() {
                                std::fs::write(root.join("settlement-response-lost"), b"committed")
                                    .unwrap();
                                return Err(std::io::Error::other(
                                    "fixture severed committed settlement response",
                                ));
                            }
                            serde_json::to_value(reply).unwrap()
                        }
                        "/delivery" => {
                            let envelope: Envelope =
                                serde_json::from_value(body["envelope"].clone()).unwrap();
                            let SellerStatus::Settled { receipt } =
                                transport.status(&envelope).await.unwrap()
                            else {
                                panic!("delivery without entitlement")
                            };
                            assert_eq!(
                                body["capability"].as_str(),
                                Some(receipt.capability.as_str())
                            );
                            return Ok(Response::builder()
                                .header("Content-Length", BYTES.len().to_string())
                                .body(Body::from(BYTES))
                                .unwrap());
                        }
                        _ => panic!("unexpected fixture route"),
                    };
                    Ok::<_, std::io::Error>(Response::new(Body::from(
                        serde_json::to_vec(&result).unwrap(),
                    )))
                }
            }))
        }
    });
    let server = Server::from_tcp(listener).unwrap().serve(service);
    // Published only after bind/service construction; no live-node addresses.
    // parent replaces this file between seller process generations.
    let root = std::env::var_os("ARCHY_SYNTHETIC_PAYMENT_ROOT").unwrap();
    std::fs::write(Path::new(&root).join("endpoint.tmp"), address).unwrap();
    std::fs::rename(
        Path::new(&root).join("endpoint.tmp"),
        Path::new(&root).join("endpoint"),
    )
    .unwrap();
    server.await.unwrap();
}
async fn buyer(root: &Path, initial: bool) {
    let offer = fixture_offer(root);
    let transport = HttpTransport {
        endpoint: std::fs::read_to_string(root.join("endpoint")).unwrap(),
        seller: offer.seller_did.clone(),
    };
    assert!(transport.endpoint.starts_with("http://127.0.0.1:"));
    let data = root.join("buyer");
    if initial {
        let quote = purchase(
            &data,
            &offer.buyer_did,
            &offer.content_id,
            None,
            8,
            None,
            &transport,
        )
        .await
        .unwrap();
        let ReadyPurchase::AwaitingConfirmation {
            operation_id,
            envelope_sha256,
            wallet_debit_sats,
            ..
        } = quote
        else {
            panic!("no fresh consent")
        };
        let consent = PurchaseConsent {
            operation_id,
            envelope_sha256,
            wallet_debit_sats,
        };
        std::fs::write(root.join("consent.json"),serde_json::to_vec(&json!({"operation_id":consent.operation_id,"envelope_sha256":consent.envelope_sha256,"wallet_debit_sats":consent.wallet_debit_sats})).unwrap()).unwrap();
        assert!(purchase(
            &data,
            &offer.buyer_did,
            &offer.content_id,
            None,
            8,
            Some(&consent),
            &transport
        )
        .await
        .is_err());
        assert!(root.join("settlement-response-lost").exists());
        assert!(crate::content_owned::list_owned_checked(&data)
            .await
            .unwrap()
            .is_empty());
    } else {
        let consent: PurchaseConsent =
            serde_json::from_slice(&std::fs::read(root.join("consent.json")).unwrap()).unwrap();
        let result = purchase(
            &data,
            &offer.buyer_did,
            &offer.content_id,
            None,
            8,
            None,
            &transport,
        )
        .await
        .unwrap();
        let ReadyPurchase::Entitlement { contract, receipt } = result else {
            panic!("original receipt not recovered")
        };
        assert_eq!(contract.id, consent.operation_id);
        let journal = crate::content_purchase::Journal::open(&data).await.unwrap();
        let envelope = journal
            .protocol_envelope("buyer", &contract.id)
            .await
            .unwrap()
            .unwrap();
        drop(journal);
        let response = reqwest::Client::new()
            .post(format!("{}/delivery", transport.endpoint))
            .json(&json!({"envelope":envelope,"capability":receipt.capability}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        assert_eq!(response.content_length(), Some(BYTES.len() as u64));
        let stream = crate::content_purchase_download::verified_stream(
            response.bytes_stream(),
            contract.content_sha256.clone(),
            contract.content_size,
        );
        crate::content_owned::record_purchase_stream(
            &data,
            crate::content_owned::OwnedItem {
                onion: ONION.into(),
                content_id: contract.content_id.clone(),
                filename: offer.filename,
                mime_type: offer.mime_type,
                size_bytes: contract.content_size,
                paid_sats: contract.gross_token_sats,
                ecash_backend: "cashu".into(),
                purchased_at: chrono::Utc::now().to_rfc3339(),
                download_complete: false,
            },
            Box::pin(stream),
            Some(contract.content_size),
        )
        .await
        .unwrap();
        crate::content_purchase::Journal::open(&data)
            .await
            .unwrap()
            .record_delivery(&contract, &contract.content_sha256, contract.content_size)
            .await
            .unwrap();
        std::fs::write(root.join("recovered-operation"), contract.id).unwrap();
    }
}
#[tokio::test]
async fn synthetic_process_child() {
    let Some(root) = std::env::var_os("ARCHY_SYNTHETIC_PAYMENT_ROOT") else {
        return;
    };
    assert_eq!(std::env::var("ARCHY_TEST_ISOLATED").as_deref(), Ok("1"));
    let root = PathBuf::from(root).canonicalize().unwrap();
    assert_eq!(
        std::fs::read(root.join("fixture-only")).unwrap(),
        b"disposable-no-real-funds"
    );
    match std::env::var("ARCHY_SYNTHETIC_PAYMENT_ROLE")
        .unwrap()
        .as_str()
    {
        "seller" => seller(root).await,
        "buyer-initial" => buyer(&root, true).await,
        "buyer-resume" => buyer(&root, false).await,
        _ => panic!("invalid fixture role"),
    }
}
fn child(root: &Path, role: &str) -> tokio::process::Child {
    tokio::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD, "--nocapture"])
        .env("ARCHY_SYNTHETIC_PAYMENT_ROOT", root)
        .env("ARCHY_SYNTHETIC_PAYMENT_ROLE", role)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap()
}
async fn start_seller(root: &Path) -> tokio::process::Child {
    let _ = std::fs::remove_file(root.join("endpoint"));
    let mut child = child(root, "seller");
    tokio::time::timeout(Duration::from_secs(20), async {
        while !root.join("endpoint").exists() {
            assert!(child.try_wait().unwrap().is_none(), "seller child exited");
            tokio::time::sleep(Duration::from_millis(20)).await
        }
    })
    .await
    .unwrap();
    child
}
#[tokio::test]
async fn committed_settlement_reply_loss_recovers_after_both_processes_restart() {
    use sha2::{Digest, Sha256};
    assert_eq!(std::env::var("ARCHY_TEST_ISOLATED").as_deref(), Ok("1"));
    let mint = Mint::start(0, None).await;
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("fixture-only"),
        b"disposable-no-real-funds",
    )
    .unwrap();
    let now = chrono::Utc::now().timestamp();
    let offer = Offer {
        id: uuid::Uuid::new_v4().to_string(),
        buyer_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([1u8; 32])).unwrap(),
        seller_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([2u8; 32])).unwrap(),
        content_id: "paid-process-fixture".into(),
        filename: "fixture.bin".into(),
        mime_type: "application/octet-stream".into(),
        content_sha256: hex::encode(Sha256::digest(BYTES)),
        content_size: BYTES.len() as u64,
        viewing_seconds: None,
        terms_sha256: "cd".repeat(32),
        network: EcashNetwork::Mainnet,
        mint_url: mint.url.clone(),
        seller_net_sats: 8,
        offered_at: now,
        expires_at: now + 600,
    };
    std::fs::write(
        root.path().join("offer.json"),
        serde_json::to_vec(&offer).unwrap(),
    )
    .unwrap();
    for role in ["buyer", "seller"] {
        let data = root.path().join(role);
        let mut wallet = WalletState::default();
        wallet.mint_url = mint.url.clone();
        if role == "buyer" {
            wallet.add_proofs(&mint.url, vec![proof(ACTIVE, 8)])
        }
        save_wallet(&data, &wallet).await.unwrap();
        save_accepted_mints(
            &data,
            &AcceptedMints {
                mints: vec![mint.url.clone()],
            },
        )
        .await
        .unwrap();
    }
    let mut server = start_seller(root.path()).await;
    let mut initial = child(root.path(), "buyer-initial");
    assert!(
        tokio::time::timeout(Duration::from_secs(45), initial.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    assert_eq!(
        load_wallet(&root.path().join("buyer"))
            .await
            .unwrap()
            .balance(),
        0
    );
    assert_eq!(
        load_wallet(&root.path().join("seller"))
            .await
            .unwrap()
            .balance(),
        8
    );
    server.kill().await.unwrap();
    server.wait().await.unwrap();
    let mut restarted = start_seller(root.path()).await;
    let mut resumed = child(root.path(), "buyer-resume");
    assert!(
        tokio::time::timeout(Duration::from_secs(45), resumed.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    restarted.kill().await.unwrap();
    restarted.wait().await.unwrap();
    assert_eq!(
        mint.requests.lock().unwrap().len(),
        1,
        "recovery must never redeem/spend again"
    );
    for (role, balance) in [("buyer", 0), ("seller", 8)] {
        let wallet = load_wallet(&root.path().join(role)).await.unwrap();
        assert_eq!(wallet.balance(), balance);
        assert_eq!(wallet.transactions.len(), 1);
    }
    let owned =
        crate::content_owned::read_owned(&root.path().join("buyer"), ONION, &offer.content_id)
            .await
            .unwrap();
    assert_eq!(owned.1, BYTES);
    let consent: PurchaseConsent =
        serde_json::from_slice(&std::fs::read(root.path().join("consent.json")).unwrap()).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("recovered-operation")).unwrap(),
        consent.operation_id
    );
}
