//! Real HTTP/curve-signature regressions for paid Cashu redemption.
use super::*;
use crate::wallet::{bdhke, cashu::Proof};
use bitcoin::secp256k1::{PublicKey, Scalar, Secp256k1, SecretKey};
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};
use serde_json::{json, Value};
use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
};

const ACTIVE: &str = "0011223344556677";
const V2: &str = "011111111111111111111111111111111111111111111111111111111111111111";

struct Mint {
    url: String,
    requests: Arc<Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
    failure: Arc<std::sync::atomic::AtomicU16>,
}
impl Drop for Mint {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn signing_key() -> SecretKey {
    SecretKey::from_slice(&[7; 32]).unwrap()
}
fn signed_point(point: PublicKey) -> String {
    point
        .mul_tweak(&Secp256k1::new(), &Scalar::from(signing_key()))
        .unwrap()
        .to_string()
}
fn proof(id: &str, amount: u64) -> Proof {
    let secret = format!("test-{id}-{amount}");
    Proof {
        amount,
        id: id.into(),
        c: signed_point(bdhke::hash_to_curve(secret.as_bytes()).unwrap()),
        secret,
    }
}
impl Mint {
    async fn start(fee: u64, failure: Option<u16>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let failure = Arc::new(std::sync::atomic::AtomicU16::new(failure.unwrap_or(0)));
        let rejection = failure.clone();
        let spent = Arc::new(Mutex::new(std::collections::HashSet::<String>::new()));
        let service = make_service_fn(move |_| {
            let seen = seen.clone();
            let rejection = rejection.clone();
            let spent = spent.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |req: Request<Body>| {
                    let seen = seen.clone();
                    let rejection = rejection.clone();
                    let spent = spent.clone();
                    async move {
                        let mut status = 200;
                        let body = match req.uri().path() {
                            "/v1/keysets" => json!({"keysets":[
                                {"id": ACTIVE,"unit":"sat","active":true,"input_fee_ppk":fee},
                                {"id": V2,"unit":"sat","active":false,"input_fee_ppk":fee}
                            ]}),
                            "/v1/keys" => {
                                let public =
                                    PublicKey::from_secret_key(&Secp256k1::new(), &signing_key())
                                        .to_string();
                                let keys: serde_json::Map<String, Value> = (0..16)
                                    .map(|i| ((1u64 << i).to_string(), json!(public)))
                                    .collect();
                                json!({"keysets":[{"id": ACTIVE,"unit":"sat","keys":keys}]})
                            }
                            "/v1/swap" => {
                                let body: Value = serde_json::from_slice(
                                    &hyper::body::to_bytes(req.into_body()).await.unwrap(),
                                )
                                .unwrap();
                                seen.lock().unwrap().push(body.clone());
                                let inputs = body["inputs"].as_array().unwrap();
                                let outputs = body["outputs"].as_array().unwrap();
                                let code = rejection.load(std::sync::atomic::Ordering::SeqCst);
                                if code != 0 {
                                    status = code;
                                    json!({"detail":"mock mint rejection"})
                                } else if inputs.iter().any(|p| p["id"] != V2 && p["id"] != ACTIVE)
                                {
                                    status = 422;
                                    json!({"detail":[{"msg":"NUT02: ID length invalid"}]})
                                } else if inputs.iter().any(|p| {
                                    spent
                                        .lock()
                                        .unwrap()
                                        .contains(p["secret"].as_str().unwrap())
                                }) {
                                    status = 400;
                                    json!({"code":11001,"detail":"Token Already Spent"})
                                } else {
                                    let total: u64 =
                                        inputs.iter().map(|p| p["amount"].as_u64().unwrap()).sum();
                                    let out: u64 =
                                        outputs.iter().map(|p| p["amount"].as_u64().unwrap()).sum();
                                    assert_eq!(
                                        out,
                                        total - (inputs.len() as u64 * fee).div_ceil(1000)
                                    );
                                    for p in inputs {
                                        spent
                                            .lock()
                                            .unwrap()
                                            .insert(p["secret"].as_str().unwrap().into());
                                    }
                                    json!({"signatures":outputs.iter().map(|o| json!({
                                    "amount":o["amount"],"id":ACTIVE,
                                    "C_":signed_point(o["B_"].as_str().unwrap().parse().unwrap())
                                })).collect::<Vec<_>>()})
                                }
                            }
                            _ => {
                                status = 404;
                                json!({})
                            }
                        };
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(status)
                                .header("Content-Type", "application/json")
                                .body(Body::from(body.to_string()))
                                .unwrap(),
                        )
                    }
                }))
            }
        });
        let server = Server::from_tcp(listener).unwrap().serve(service);
        let task = tokio::spawn(async move {
            server.await.unwrap();
        });
        Self {
            url,
            requests,
            task,
            failure,
        }
    }
    async fn wallet(&self) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        save_accepted_mints(
            dir.path(),
            &AcceptedMints {
                mints: vec![format!("{}/", self.url)],
            },
        )
        .await
        .unwrap();
        dir
    }
}

#[tokio::test]
async fn paid_v4_inactive_v2_keyset_is_expanded_and_cryptographic_proofs_saved() {
    let mint = Mint::start(0, None).await;
    let dir = mint.wallet().await;
    let token = CashuToken::new(&mint.url, vec![proof(V2, 64), proof(V2, 32), proof(V2, 4)])
        .serialize_v4()
        .unwrap();
    let decoded = CashuToken::deserialize(&token).unwrap();
    assert_eq!(
        decoded.token[0].proofs[0].id.len(),
        16,
        "reproduce the short V4 ID"
    );
    assert_eq!(
        verify_and_receive_payment(dir.path(), &token, 100)
            .await
            .unwrap(),
        100
    );
    let wallet = load_wallet(dir.path()).await.unwrap();
    assert_eq!(wallet.balance(), 100);
    for p in wallet.proofs {
        assert_eq!(
            p.proof.c,
            signed_point(bdhke::hash_to_curve(p.proof.secret.as_bytes()).unwrap())
        );
    }
    assert!(mint.requests.lock().unwrap()[0]["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| p["id"] == V2));
    assert!(verify_and_receive_payment(dir.path(), &token, 100)
        .await
        .is_err());
    assert_eq!(load_wallet(dir.path()).await.unwrap().balance(), 100);
}

#[tokio::test]
async fn paid_v3_full_v2_and_v1_ids_work() {
    for id in [V2, ACTIVE] {
        let mint = Mint::start(0, None).await;
        let dir = mint.wallet().await;
        let token = CashuToken::new(&mint.url, vec![proof(id, 128)])
            .serialize()
            .unwrap();
        assert_eq!(
            verify_and_receive_payment(dir.path(), &token, 100)
                .await
                .unwrap(),
            128
        );
    }
}

#[tokio::test]
async fn fees_cannot_consume_underpayment_and_allowed_fees_credit_actual_value() {
    let mint = Mint::start(1000, None).await;
    let dir = mint.wallet().await;
    let token = CashuToken::new(&mint.url, vec![proof(V2, 128)])
        .serialize_v4()
        .unwrap();
    assert!(verify_and_receive_payment(dir.path(), &token, 128)
        .await
        .unwrap_err()
        .to_string()
        .contains("after mint fees"));
    assert!(mint.requests.lock().unwrap().is_empty());
    assert_eq!(
        verify_and_receive_payment(dir.path(), &token, 127)
            .await
            .unwrap(),
        127
    );
    assert_eq!(load_wallet(dir.path()).await.unwrap().balance(), 127);
}

#[tokio::test]
async fn rejected_mint_response_does_not_credit_wallet() {
    for status in [200, 400, 422, 500, 503] {
        let mint = Mint::start(0, Some(status)).await;
        let dir = mint.wallet().await;
        let token = CashuToken::new(&mint.url, vec![proof(V2, 128)])
            .serialize_v4()
            .unwrap();
        assert!(verify_and_receive_payment(dir.path(), &token, 100)
            .await
            .is_err());
        assert_eq!(load_wallet(dir.path()).await.unwrap().balance(), 0);
    }
}

#[tokio::test]
async fn invalid_untrusted_multimint_and_underpaid_tokens_never_reach_swap() {
    let mint = Mint::start(0, None).await;
    let dir = mint.wallet().await;
    let token = CashuToken::new(&mint.url, vec![proof(V2, 128)]);
    let mut invalid = vec![
        "cashuSend_500_abc_1700000000".into(),
        "cashuBinvalid".into(),
    ];
    let mut wrong_unit = token.clone();
    wrong_unit.unit = Some("usd".into());
    invalid.push(wrong_unit.serialize().unwrap());
    let mut multi = token.clone();
    multi.token.push(token.token[0].clone());
    invalid.push(multi.serialize().unwrap());
    let mut untrusted = token.clone();
    untrusted.token[0].mint = "http://127.0.0.1:1".into();
    invalid.push(untrusted.serialize().unwrap());
    for id in ["00ffffffffffffff", "01ffffffffffffff"] {
        invalid.push(
            CashuToken::new(&mint.url, vec![proof(id, 128)])
                .serialize()
                .unwrap(),
        );
    }
    for value in invalid {
        assert!(verify_and_receive_payment(dir.path(), &value, 100)
            .await
            .is_err());
    }
    assert!(
        verify_and_receive_payment(dir.path(), &token.serialize().unwrap(), 129)
            .await
            .is_err()
    );
    assert!(mint.requests.lock().unwrap().is_empty());
    assert_eq!(load_wallet(dir.path()).await.unwrap().balance(), 0);
}

#[tokio::test]
async fn buyer_token_rejected_by_seller_can_be_refunded_without_balance_loss() {
    let mint = Mint::start(0, Some(422)).await;
    let buyer = mint.wallet().await;
    let seller = mint.wallet().await;
    let mut wallet = load_wallet(buyer.path()).await.unwrap();
    wallet.mint_url = mint.url.clone();
    wallet.add_proofs(&mint.url, vec![proof(V2, 64), proof(V2, 32), proof(V2, 4)]);
    save_wallet(buyer.path(), &wallet).await.unwrap();
    let token = send_token(buyer.path(), 100).await.unwrap();
    assert_eq!(load_wallet(buyer.path()).await.unwrap().balance(), 0);
    assert!(verify_and_receive_payment(seller.path(), &token, 100)
        .await
        .is_err());
    mint.failure.store(0, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(receive_token(buyer.path(), &token).await.unwrap(), 100);
    assert_eq!(load_wallet(buyer.path()).await.unwrap().balance(), 100);
    assert_eq!(load_wallet(seller.path()).await.unwrap().balance(), 0);
    assert!(receive_token(buyer.path(), &token).await.is_err());
    assert_eq!(load_wallet(buyer.path()).await.unwrap().balance(), 100);
}

#[tokio::test]
async fn unreachable_mint_does_not_credit_seller() {
    let mint = Mint::start(0, None).await;
    let dir = mint.wallet().await;
    let token = CashuToken::new(&mint.url, vec![proof(V2, 128)])
        .serialize_v4()
        .unwrap();
    mint.task.abort();
    tokio::task::yield_now().await;
    assert!(verify_and_receive_payment(dir.path(), &token, 100)
        .await
        .is_err());
    assert_eq!(load_wallet(dir.path()).await.unwrap().balance(), 0);
}

#[tokio::test]
async fn send_with_fees_preserves_payment_denominations_and_saves_change() {
    // 128 inputs - 2 fee = 126. Splitting 126 as one sum omits 1,
    // which is needed for a 65-sat payment, after consuming the inputs.
    let mint = Mint::start(1000, None).await;
    let buyer = mint.wallet().await;
    let mut wallet = load_wallet(buyer.path()).await.unwrap();
    wallet.mint_url = mint.url.clone();
    let first = proof(V2, 64);
    let mut second = first.clone();
    second.secret.push_str("-second");
    second.c = signed_point(bdhke::hash_to_curve(second.secret.as_bytes()).unwrap());
    wallet.add_proofs(&mint.url, vec![first, second]);
    save_wallet(buyer.path(), &wallet).await.unwrap();
    let encoded = send_token(buyer.path(), 65).await.unwrap();
    assert_eq!(
        CashuToken::deserialize(&encoded).unwrap().total_amount(),
        65
    );
    assert_eq!(load_wallet(buyer.path()).await.unwrap().balance(), 61);
}

#[tokio::test]
async fn paid_file_gate_delivers_bytes_only_after_payment_and_does_not_charge_missing_files() {
    use crate::content_server::{
        self, AccessControl, Availability, ContentCatalog, ContentItem, ServeResult,
    };
    for (exists, accepts_cashu, price) in [
        (true, true, 100),
        (true, false, 100),
        (false, true, 100),
        (true, true, 129),
    ] {
        let mint = Mint::start(0, None).await;
        let seller = mint.wallet().await;
        let item = ContentItem {
            id: "paid-test".into(),
            filename: "test.txt".into(),
            mime_type: "text/plain".into(),
            size_bytes: 5,
            description: String::new(),
            added_at: String::new(),
            availability: Availability::AllPeers,
            access: AccessControl::Paid {
                price_sats: price,
                accepted: vec![if accepts_cashu { "ecash" } else { "fedimint" }.into()],
            },
        };
        content_server::save_catalog(seller.path(), &ContentCatalog { items: vec![item] })
            .await
            .unwrap();
        if exists {
            tokio::fs::create_dir_all(seller.path().join("content/files"))
                .await
                .unwrap();
            tokio::fs::write(seller.path().join("content/files/test.txt"), b"hello")
                .await
                .unwrap();
        }
        let token = CashuToken::new(&mint.url, vec![proof(V2, 128)])
            .serialize_v4()
            .unwrap();
        let result = content_server::serve_content(
            seller.path(),
            "paid-test",
            Some(&token),
            None,
            None,
            None,
            false,
        )
        .await
        .unwrap();
        if exists && accepts_cashu && price <= 128 {
            match result {
                ServeResult::Ok(bytes, mime) => {
                    assert_eq!(bytes, b"hello");
                    assert_eq!(mime, "text/plain");
                }
                _ => panic!("paid content was not delivered"),
            }
            assert_eq!(load_wallet(seller.path()).await.unwrap().balance(), 128);
        } else {
            assert!(matches!(
                result,
                ServeResult::NotFound | ServeResult::PaymentRequired(_)
            ));
            assert_eq!(load_wallet(seller.path()).await.unwrap().balance(), 0);
            assert!(mint.requests.lock().unwrap().is_empty());
        }
    }
}

#[tokio::test]
async fn simultaneous_receipts_and_revenue_writes_preserve_every_payment() {
    let mint = Mint::start(0, None).await;
    let wallet_dir = mint.wallet().await;
    let mut tasks = tokio::task::JoinSet::new();
    for exponent in 0..8 {
        let amount = 1u64 << exponent;
        let token = CashuToken::new(&mint.url, vec![proof(ACTIVE, amount)])
            .serialize()
            .unwrap();
        let data_dir = wallet_dir.path().to_path_buf();
        tasks.spawn(async move {
            verify_and_receive_payment(&data_dir, &token, amount)
                .await
                .unwrap();
        });
    }
    for index in 0..16 {
        let data_dir = wallet_dir.path().to_path_buf();
        tasks.spawn(async move {
            record_streaming_revenue(&data_dir, 1, "fixture-service", &format!("peer-{index}"))
                .await
                .unwrap();
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
    let wallet = load_wallet(wallet_dir.path()).await.unwrap();
    assert_eq!(wallet.balance(), 255);
    assert_eq!(wallet.transactions.len(), 24);
    assert_eq!(mint.requests.lock().unwrap().len(), 8);
}
