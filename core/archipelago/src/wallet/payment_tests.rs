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
    lose_swap_reply: Arc<std::sync::atomic::AtomicBool>,
    restore_reply: Arc<Mutex<Option<Value>>>,
    state_reply: Arc<Mutex<Option<Value>>>,
    // Per-fixture clock advancement proves the spend deadline is checked after
    // mint preflight; no process-global clock or timing-sensitive sleep.
    restore_clock: Arc<Mutex<Option<(Arc<std::sync::atomic::AtomicI64>, i64)>>>,
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
        let issued = Arc::new(Mutex::new(std::collections::HashMap::<String, Value>::new()));
        let lose_swap_reply = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let lose_reply = lose_swap_reply.clone();
        let restore_reply = Arc::new(Mutex::new(None::<Value>));
        let restore_override = restore_reply.clone();
        let state_reply = Arc::new(Mutex::new(None::<Value>));
        let state_override = state_reply.clone();
        let restore_clock = Arc::new(Mutex::new(None::<(Arc<std::sync::atomic::AtomicI64>, i64)>));
        let advance_clock = restore_clock.clone();
        let service = make_service_fn(move |_| {
            let seen = seen.clone();
            let rejection = rejection.clone();
            let spent = spent.clone();
            let issued = issued.clone();
            let lose_reply = lose_reply.clone();
            let restore_override = restore_override.clone();
            let state_override = state_override.clone();
            let advance_clock = advance_clock.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |req: Request<Body>| {
                    let seen = seen.clone();
                    let rejection = rejection.clone();
                    let spent = spent.clone();
                    let issued = issued.clone();
                    let lose_reply = lose_reply.clone();
                    let restore_override = restore_override.clone();
                    let state_override = state_override.clone();
                    let advance_clock = advance_clock.clone();
                    async move {
                        let mut status = 200;
                        let body = match req.uri().path() {
                            "/v1/keysets" => json!({"keysets":[
                                {"id": ACTIVE,"unit":"sat","active":true,"input_fee_ppk":fee},
                                {"id": V2,"unit":"sat","active":false,"input_fee_ppk":fee}
                            ]}),
                            path if path == "/v1/keys" || path.starts_with("/v1/keys/") => {
                                let public =
                                    PublicKey::from_secret_key(&Secp256k1::new(), &signing_key())
                                        .to_string();
                                let keys: serde_json::Map<String, Value> = (0..16)
                                    .map(|i| ((1u64 << i).to_string(), json!(public)))
                                    .collect();
                                let id = path.strip_prefix("/v1/keys/").unwrap_or(ACTIVE);
                                json!({"keysets":[{"id": id,"unit":"sat","keys":keys}]})
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
                                    let signatures: Vec<_> = outputs.iter().map(|o| {
                                        let signature = json!({"amount":o["amount"],"id":ACTIVE,
                                            "C_":signed_point(o["B_"].as_str().unwrap().parse().unwrap())});
                                        issued.lock().unwrap().insert(o["B_"].as_str().unwrap().into(), signature.clone());
                                        signature
                                    }).collect();
                                    if lose_reply.load(std::sync::atomic::Ordering::SeqCst) {
                                        status = 500;
                                        json!({"detail":"Fixture lost the reply after consuming inputs"})
                                    } else {
                                        json!({"signatures":signatures})
                                    }
                                }
                            }
                            "/v1/checkstate" => {
                                let body: Value = serde_json::from_slice(
                                    &hyper::body::to_bytes(req.into_body()).await.unwrap(),
                                )
                                .unwrap();
                                let overridden = state_override.lock().unwrap().clone();
                                overridden.unwrap_or_else(|| {
                                    let spent_points: std::collections::HashSet<_> = spent.lock().unwrap().iter().map(|secret| hex::encode(bdhke::hash_to_curve(secret.as_bytes()).unwrap().serialize())).collect();
                                    json!({"states": body["Ys"].as_array().unwrap().iter().map(|y| json!({"Y":y,"state":if spent_points.contains(y.as_str().unwrap()) {"SPENT"} else {"UNSPENT"}})).collect::<Vec<_>>()})
                                })
                            }
                            "/v1/restore" => {
                                if let Some((clock, deadline)) =
                                    advance_clock.lock().unwrap().as_ref()
                                {
                                    clock.store(*deadline, std::sync::atomic::Ordering::SeqCst);
                                }
                                let body: Value = serde_json::from_slice(
                                    &hyper::body::to_bytes(req.into_body()).await.unwrap(),
                                )
                                .unwrap();
                                let override_reply = restore_override.lock().unwrap().clone();
                                if let Some(reply) = override_reply {
                                    reply
                                } else {
                                    let issued = issued.lock().unwrap();
                                    let mut outputs = Vec::new();
                                    let mut signatures = Vec::new();
                                    for output in body["outputs"].as_array().unwrap().iter().rev() {
                                        if let Some(signature) =
                                            issued.get(output["B_"].as_str().unwrap())
                                        {
                                            let mut echoed = output.clone();
                                            echoed["B_"] = json!(output["B_"]
                                                .as_str()
                                                .unwrap()
                                                .to_uppercase());
                                            outputs.push(echoed);
                                            signatures.push(signature.clone());
                                        }
                                    }
                                    json!({"outputs":outputs,"signatures":signatures})
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
            lose_swap_reply,
            restore_reply,
            state_reply,
            restore_clock,
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
            if !exists {
                assert!(matches!(result, ServeResult::Unavailable));
                assert!(content_server::load_catalog(seller.path())
                    .await
                    .unwrap()
                    .items
                    .iter()
                    .any(|item| item.id == "paid-test"));
            } else {
                assert!(matches!(result, ServeResult::PaymentRequired(_)));
            }
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

#[tokio::test]
async fn prepared_swap_recovers_a_lost_reply_without_a_second_spend() {
    let mint = Mint::start(0, None).await;
    let client = MintClient::new(&mint.url).unwrap();
    let prepared = client
        .prepare_swap_at_least(&[proof(ACTIVE, 8)], &[4, 4], 4)
        .await
        .unwrap();
    assert!(
        mint.requests.lock().unwrap().is_empty(),
        "Preparation must not consume inputs"
    );
    assert!(client
        .restore_prepared_swap(&prepared)
        .await
        .unwrap()
        .is_none());
    let stored = serde_json::to_vec(&prepared).unwrap();
    mint.lose_swap_reply
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(client.execute_prepared_swap(&prepared).await.is_err());
    let reconstructed: crate::wallet::mint_client::PreparedSwap =
        serde_json::from_slice(&stored).unwrap();
    let restarted = MintClient::new(&mint.url).unwrap();
    let restored = restarted
        .restore_prepared_swap(&reconstructed)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(restored.new_proofs.iter().map(|p| p.amount).sum::<u64>(), 8);
    for proof in &restored.new_proofs {
        assert_eq!(
            proof.c,
            signed_point(bdhke::hash_to_curve(proof.secret.as_bytes()).unwrap())
        );
    }
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    assert_eq!(
        serde_json::to_value(&reconstructed).unwrap(),
        serde_json::from_slice::<Value>(&stored).unwrap()
    );
    assert!(!format!("{:?}", reconstructed).contains(&restored.new_proofs[0].secret));
}

#[tokio::test]
async fn proof_state_checks_reject_foreign_duplicate_missing_and_unknown_states() {
    let mint = Mint::start(0, None).await;
    let client = MintClient::new(&mint.url).unwrap();
    let proofs = vec![proof(ACTIVE, 4), proof(ACTIVE, 8)];
    let states = client.check_state(&proofs).await.unwrap();
    assert_eq!(states.len(), 2);
    let valid: Vec<_> = states
        .iter()
        .map(|state| json!({"Y":state.y,"state":state.state}))
        .collect();
    let mut duplicate = valid.clone();
    duplicate[1] = duplicate[0].clone();
    let mut foreign = valid.clone();
    foreign[0]["Y"] = json!("00".repeat(33));
    let mut unknown = valid.clone();
    unknown[0]["state"] = json!("UNKNOWN");
    let mut reversed = valid.clone();
    reversed.reverse();
    for response in [
        json!({}),
        json!({"states":[valid[0].clone()]}),
        json!({"states":duplicate}),
        json!({"states":foreign}),
        json!({"states":unknown}),
        json!({"states":reversed}),
    ] {
        *mint.state_reply.lock().unwrap() = Some(response);
        assert!(client.check_state(&proofs).await.is_err());
    }
    let mut mixed = valid;
    mixed[0]["Y"] = json!(states[0].y.to_uppercase());
    mixed[0]["state"] = json!("PENDING");
    mixed[1]["state"] = json!("SPENT");
    *mint.state_reply.lock().unwrap() = Some(json!({"states":mixed}));
    let result = client.check_state(&proofs).await.unwrap();
    assert_eq!(result[0].state, "PENDING");
    assert_eq!(result[1].state, "SPENT");
    assert!(client
        .check_state(&[proofs[0].clone(), proofs[0].clone()])
        .await
        .is_err());
    assert!(client.check_state(&[]).await.unwrap().is_empty());
}

#[tokio::test]
async fn journal_recovers_lost_swap_reply_and_commits_change_once() {
    use crate::wallet::{
        mutation,
        send_journal::{Binding, Journal, Outcome, Request as SendRequest},
    };
    let mint = Mint::start(0, None).await;
    let root = tempfile::tempdir().unwrap();
    let client = MintClient::new(&mint.url).unwrap();
    let input = proof(ACTIVE, 8);
    let binding = Binding {
        id: uuid::Uuid::new_v4().to_string(),
        network: EcashNetwork::Mainnet,
        mint_url: mint.url.clone(),
        amount_sats: 4,
        context_hash: "ab".repeat(32),
    };
    {
        let held = mutation::guard(root.path()).await.unwrap();
        let journal = Journal::new(&held);
        let mut wallet = WalletState::default();
        wallet.mint_url = mint.url.clone();
        wallet.add_proofs(&mint.url, vec![input.clone()]);
        save_wallet(root.path(), &wallet).await.unwrap();
        let prepared = client
            .prepare_swap_at_least(&[input], &[4, 4], 4)
            .await
            .unwrap();
        let mut impossible = binding.clone();
        impossible.id = uuid::Uuid::new_v4().to_string();
        impossible.amount_sats = 16;
        assert!(journal
            .prepare(impossible, SendRequest::Swap(prepared.clone()))
            .await
            .is_err());
        journal
            .prepare(binding.clone(), SendRequest::Swap(prepared.clone()))
            .await
            .unwrap();
        journal.reserve_wallet(&binding).await.unwrap();
        mint.lose_swap_reply
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(client.execute_prepared_swap(&prepared).await.is_err());
        assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 0);
    }
    let held = mutation::guard(root.path()).await.unwrap();
    let journal = Journal::new(&held);
    let record = journal.load(&binding.id).await.unwrap().unwrap();
    let SendRequest::Swap(prepared) = record.request else {
        panic!("Lost prepared swap")
    };
    let mut restored = MintClient::new(&mint.url)
        .unwrap()
        .restore_prepared_swap(&prepared)
        .await
        .unwrap()
        .unwrap()
        .new_proofs;
    let send = restored.remove(0);
    let token = CashuToken::new(&mint.url, vec![send.clone()])
        .serialize()
        .unwrap();
    journal
        .record_result(
            &binding,
            Outcome {
                token: token.clone(),
                change: restored,
            },
        )
        .await
        .unwrap();
    assert_eq!(journal.commit_wallet(&binding).await.unwrap(), token);
    assert_eq!(journal.commit_wallet(&binding).await.unwrap(), token);
    let wallet = load_wallet(root.path()).await.unwrap();
    assert_eq!(wallet.balance(), 4);
    assert_eq!(wallet.transactions.len(), 1);
    assert_eq!(wallet.transactions[0].id, binding.id);
    assert_eq!(wallet.proofs.len(), 3);
    assert!(
        wallet
            .proofs
            .iter()
            .find(|stored| stored.proof.secret == send.secret)
            .unwrap()
            .spent
    );
    assert!(wallet
        .proofs
        .iter()
        .all(|stored| !stored.reserved && stored.reserved_by.is_none()));
    assert_eq!(
        mint.requests.lock().unwrap().len(),
        1,
        "Recovery must not send another swap"
    );
}

#[tokio::test]
async fn recoverable_send_rejects_broken_recovery_before_reserving_or_spending() {
    let mint = Mint::start(0, None).await;
    let root = tempfile::tempdir().unwrap();
    let mut wallet = WalletState::default();
    wallet.mint_url = mint.url.clone();
    wallet.add_proofs(&mint.url, vec![proof(ACTIVE, 8)]);
    save_wallet(root.path(), &wallet).await.unwrap();
    *mint.restore_reply.lock().unwrap() = Some(json!({}));
    let id = uuid::Uuid::new_v4().to_string();
    let context = "ab".repeat(32);
    let error = send_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        4,
        &context,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("recovery support"));
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 8);
    assert!(mint.requests.lock().unwrap().is_empty());
    // Once the mint responds correctly the same unspent operation can proceed.
    *mint.restore_reply.lock().unwrap() = None;
    assert!(send_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        4,
        &context,
    )
    .await
    .is_ok());
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 4);
}

#[tokio::test]
async fn recoverable_send_reuses_original_operation_after_ambiguous_mint_response() {
    let mint = Mint::start(0, None).await;
    let root = tempfile::tempdir().unwrap();
    let mut wallet = WalletState::default();
    wallet.mint_url = mint.url.clone();
    wallet.add_proofs(&mint.url, vec![proof(ACTIVE, 8)]);
    save_wallet(root.path(), &wallet).await.unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let context = "ab".repeat(32);
    mint.lose_swap_reply
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(send_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        4,
        &context
    )
    .await
    .is_err());
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 0);
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    // A missing restore response is not permission to swap spent inputs again.
    *mint.restore_reply.lock().unwrap() = Some(json!({"outputs":[],"signatures":[]}));
    assert!(send_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        4,
        &context
    )
    .await
    .is_err());
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    *mint.restore_reply.lock().unwrap() = None;
    let token = send_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        4,
        &context,
    )
    .await
    .unwrap();
    assert_eq!(CashuToken::deserialize(&token).unwrap().total_amount(), 4);
    let purse = std::fs::read(root.path().join("wallet/ecash.json")).unwrap();
    assert_eq!(
        send_token_recoverable(
            root.path(),
            &id,
            EcashNetwork::Mainnet,
            &mint.url,
            4,
            &context
        )
        .await
        .unwrap(),
        token
    );
    assert!(send_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        8,
        &context
    )
    .await
    .is_err());
    assert!(send_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        4,
        &"cd".repeat(32)
    )
    .await
    .is_err());
    assert_eq!(
        std::fs::read(root.path().join("wallet/ecash.json")).unwrap(),
        purse
    );
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 4);
    assert_eq!(
        load_wallet(root.path()).await.unwrap().transactions.len(),
        1
    );
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn recoverable_exact_send_supports_compact_v2_and_keeps_full_wallet_id() {
    let root = tempfile::tempdir().unwrap();
    let mint = "https://unused-mint.invalid/";
    let mut wallet = WalletState::default();
    wallet.mint_url = mint.into();
    wallet.add_proofs(mint, vec![proof(V2, 8)]);
    save_wallet(root.path(), &wallet).await.unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let context = "ab".repeat(32);
    let token = send_token_recoverable(root.path(), &id, EcashNetwork::Mainnet, mint, 8, &context)
        .await
        .unwrap();
    assert_eq!(
        CashuToken::deserialize(&token).unwrap().token[0].proofs[0].id,
        &V2[..16]
    );
    assert_eq!(
        send_token_recoverable(root.path(), &id, EcashNetwork::Mainnet, mint, 8, &context)
            .await
            .unwrap(),
        token
    );
    let wallet = load_wallet(root.path()).await.unwrap();
    assert_eq!(wallet.balance(), 0);
    assert_eq!(wallet.proofs[0].proof.id, V2);
    assert_eq!(wallet.transactions.len(), 1);
}

#[tokio::test]
async fn seed_restore_refuses_to_credit_when_counter_persistence_fails() {
    let mint = Mint::start(0, None).await;
    let root = tempfile::tempdir().unwrap();
    crate::wallet::nut13::establish_independent(root.path())
        .await
        .unwrap();
    let client = MintClient::new(&mint.url).unwrap().with_recovery(
        crate::wallet::nut13::RecoverySource::load(root.path())
            .await
            .unwrap(),
    );
    let prepared = client
        .prepare_swap_at_least(&[proof(ACTIVE, 8)], &[4, 4], 4)
        .await
        .unwrap();
    client.execute_prepared_swap(&prepared).await.unwrap();
    let counter = root.path().join("wallet/cashu_counters.json");
    std::fs::rename(&counter, root.path().join("counter-fixture-backup")).unwrap();
    std::fs::create_dir(&counter).unwrap();
    let error = restore_from_seed(root.path(), &mint.url).await.unwrap_err();
    assert!(error.to_string().contains("derivation position"));
    assert!(counter.is_dir());
    assert!(!root.path().join("wallet/ecash.json").exists());
    std::fs::remove_dir(&counter).unwrap();
    std::fs::rename(root.path().join("counter-fixture-backup"), &counter).unwrap();
    assert_eq!(
        restore_from_seed(root.path(), &mint.url)
            .await
            .unwrap()
            .recovered_sats,
        8
    );
    assert_eq!(
        restore_from_seed(root.path(), &mint.url)
            .await
            .unwrap()
            .recovered_sats,
        0
    );
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 8);
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn seed_restore_matches_points_and_rejects_foreign_or_duplicated_metadata() {
    let mint = Mint::start(0, None).await;
    let client = MintClient::new(&mint.url).unwrap();
    let prepared = client
        .prepare_swap_at_least(&[proof(ACTIVE, 8)], &[4, 4], 4)
        .await
        .unwrap();
    client.execute_prepared_swap(&prepared).await.unwrap();
    let encoded = serde_json::to_value(&prepared).unwrap();
    let outputs: Vec<crate::wallet::cashu::BlindedMessageRequest> =
        serde_json::from_value(encoded["outputs"].clone()).unwrap();
    let restored = client.restore(&outputs).await.unwrap();
    assert_eq!(restored.len(), 2);
    assert!(restored
        .iter()
        .all(|(point, _)| outputs.iter().any(|output| &output.b_prime == point)));
    let output = encoded["outputs"][0].clone();
    let signature = json!({"id":ACTIVE,"amount":4,"C_":signed_point(output["B_"].as_str().unwrap().parse().unwrap())});
    // A seed scan legitimately receives only the outputs the mint signed.
    *mint.restore_reply.lock().unwrap() =
        Some(json!({"outputs":[output.clone()],"signatures":[signature.clone()]}));
    assert_eq!(client.restore(&outputs).await.unwrap().len(), 1);
    let mut foreign = output.clone();
    foreign["B_"] =
        json!(PublicKey::from_secret_key(&Secp256k1::new(), &signing_key()).to_string());
    let mut wrong_keyset = signature.clone();
    wrong_keyset["id"] = json!(V2);
    let mut wrong_amount = signature.clone();
    wrong_amount["amount"] = json!(8);
    for response in [
        json!({}),
        json!({"outputs":[]}),
        json!({"signatures":[]}),
        json!({"outputs":null,"signatures":[]}),
        json!({"outputs":[output.clone(),output.clone()],"signatures":[signature.clone(),signature.clone()]}),
        json!({"outputs":[foreign],"signatures":[signature.clone()]}),
        json!({"outputs":[output.clone()],"signatures":[wrong_keyset]}),
        json!({"outputs":[output],"signatures":[wrong_amount]}),
    ] {
        *mint.restore_reply.lock().unwrap() = Some(response);
        assert!(client.restore(&outputs).await.is_err());
    }
}

#[tokio::test]
async fn damaged_or_wrong_mint_preparation_fails_before_spending() {
    let mint = Mint::start(0, None).await;
    let client = MintClient::new(&mint.url).unwrap();
    let prepared = client
        .prepare_swap_at_least(&[proof(ACTIVE, 8)], &[4, 4], 4)
        .await
        .unwrap();
    for field in ["mint_url", "outputs", "blinding", "duplicate"] {
        let mut data = serde_json::to_value(&prepared).unwrap();
        match field {
            "mint_url" => data["mint_url"] = json!("https://different.invalid"),
            "outputs" => data["outputs"][0]["amount"] = json!(2),
            "duplicate" => {
                data["outputs"][1] = data["outputs"][0].clone();
                data["blinding"][1] = data["blinding"][0].clone();
            }
            _ => data["blinding"][0]["secret"] = json!([1, 2, 3]),
        }
        let corrupted = serde_json::from_value(data).unwrap();
        assert!(client.execute_prepared_swap(&corrupted).await.is_err());
    }
    assert!(mint.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn incomplete_duplicate_or_unknown_restoration_never_completes_a_swap() {
    let mint = Mint::start(0, None).await;
    let client = MintClient::new(&mint.url).unwrap();
    let prepared = client
        .prepare_swap_at_least(&[proof(ACTIVE, 8)], &[4, 4], 4)
        .await
        .unwrap();
    let data = serde_json::to_value(&prepared).unwrap();
    let output = data["outputs"][0].clone();
    let signature = json!({"amount":4,"id":ACTIVE,
        "C_":signed_point(output["B_"].as_str().unwrap().parse().unwrap())});
    let mut unknown = output.clone();
    unknown["B_"] =
        json!(PublicKey::from_secret_key(&Secp256k1::new(), &signing_key()).to_string());
    let second = data["outputs"][1].clone();
    let second_signature = json!({"amount":4,"id":ACTIVE,
        "C_":signed_point(second["B_"].as_str().unwrap().parse().unwrap())});
    let mut wrong_amount = signature.clone();
    wrong_amount["amount"] = json!(2);
    let mut wrong_keyset = signature.clone();
    wrong_keyset["id"] = json!(V2);
    for reply in [
        json!({"outputs":[output.clone(),second.clone()],"signatures":[wrong_amount,second_signature.clone()]}),
        json!({"outputs":[output.clone(),second],"signatures":[wrong_keyset,second_signature]}),
        json!({"outputs":[output.clone()],"signatures":[signature.clone()]}),
        json!({"outputs":[output.clone(),output.clone()],"signatures":[signature.clone(),signature.clone()]}),
        json!({"outputs":[unknown],"signatures":[signature.clone()]}),
        json!({"outputs":[output],"signatures":[]}),
    ] {
        *mint.restore_reply.lock().unwrap() = Some(reply);
        assert!(client.restore_prepared_swap(&prepared).await.is_err());
    }
    assert!(mint.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn recoverable_receive_lost_reply_claims_inputs_and_recovers_once() {
    let mint = Mint::start(0, None).await;
    let root = mint.wallet().await;
    let incoming = CashuToken::new(&mint.url, vec![proof(V2, 8), proof(ACTIVE, 4)]);
    let token = incoming.serialize_v4().unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let context = "ab".repeat(32);
    mint.lose_swap_reply
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(receive_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        &token,
        12,
        &context
    )
    .await
    .is_err());
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 0);
    assert!(restore_from_seed(root.path(), &mint.url)
        .await
        .unwrap_err()
        .to_string()
        .contains("pending receipts"));
    for duplicate in [
        token.clone(),
        incoming.serialize().unwrap(),
        CashuToken::new(&mint.url, vec![proof(ACTIVE, 4), proof(ACTIVE, 2)])
            .serialize()
            .unwrap(),
    ] {
        let other = uuid::Uuid::new_v4().to_string();
        assert!(receive_token_recoverable(
            root.path(),
            &other,
            EcashNetwork::Mainnet,
            &mint.url,
            &duplicate,
            1,
            &context
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("another settlement"));
    }
    assert!(receive_token(root.path(), &token)
        .await
        .unwrap_err()
        .to_string()
        .contains("another settlement"));
    *mint.restore_reply.lock().unwrap() = Some(json!({"outputs":[],"signatures":[]}));
    assert!(receive_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        &token,
        12,
        &context
    )
    .await
    .unwrap_err()
    .to_string()
    .contains("pending at the mint"));
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    *mint.restore_reply.lock().unwrap() = None;
    assert_eq!(
        receive_token_recoverable(
            root.path(),
            &id,
            EcashNetwork::Mainnet,
            &mint.url,
            &token,
            12,
            &context
        )
        .await
        .unwrap(),
        12
    );
    let wallet = load_wallet(root.path()).await.unwrap();
    assert_eq!(wallet.balance(), 12);
    assert_eq!(wallet.transactions.len(), 1);
    assert_eq!(wallet.transactions[0].id, format!("received:{id}"));
    assert_eq!(wallet.receive_commits.len(), 1);
    for output in &wallet.proofs {
        assert_eq!(
            output.proof.c,
            signed_point(bdhke::hash_to_curve(output.proof.secret.as_bytes()).unwrap())
        );
        assert!(!incoming.token[0]
            .proofs
            .iter()
            .any(|input| input.secret == output.proof.secret));
    }
    let before = std::fs::read(root.path().join("wallet/ecash.json")).unwrap();
    assert_eq!(
        receive_token_recoverable(
            root.path(),
            &id,
            EcashNetwork::Mainnet,
            &mint.url,
            &token,
            12,
            &context
        )
        .await
        .unwrap(),
        12
    );
    assert_eq!(
        std::fs::read(root.path().join("wallet/ecash.json")).unwrap(),
        before
    );
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    for (network, price, terms) in [
        (EcashNetwork::Testnet, 12, context.clone()),
        (EcashNetwork::Mainnet, 11, context.clone()),
        (EcashNetwork::Mainnet, 12, "cd".repeat(32)),
    ] {
        assert!(receive_token_recoverable(
            root.path(),
            &id,
            network,
            &mint.url,
            &token,
            price,
            &terms
        )
        .await
        .is_err());
    }
    // Completed receipts remain valid without re-crediting a pruned wallet.
    std::fs::remove_file(root.path().join("wallet/ecash.json")).unwrap();
    assert_eq!(
        receive_token_recoverable(
            root.path(),
            &id,
            EcashNetwork::Mainnet,
            &mint.url,
            &token,
            12,
            &context
        )
        .await
        .unwrap(),
        12
    );
    assert!(!root.path().join("wallet/ecash.json").exists());
    assert!(receive_token_recoverable(
        root.path(),
        &uuid::Uuid::new_v4().to_string(),
        EcashNetwork::Mainnet,
        &mint.url,
        &incoming.serialize().unwrap(),
        12,
        &context
    )
    .await
    .is_err());
}

#[tokio::test]
async fn recoverable_receive_recovery_support_fees_and_terms_fail_before_spend() {
    let mint = Mint::start(1000, None).await;
    let root = mint.wallet().await;
    let token = CashuToken::new(&mint.url, vec![proof(ACTIVE, 8)])
        .serialize()
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let context = "ab".repeat(32);
    assert!(receive_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        &token,
        8,
        &context
    )
    .await
    .is_err());
    *mint.restore_reply.lock().unwrap() = Some(json!({}));
    assert!(receive_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        &token,
        7,
        &context
    )
    .await
    .unwrap_err()
    .to_string()
    .contains("recovery support"));
    assert!(mint.requests.lock().unwrap().is_empty());
    assert!(!root.path().join("wallet/receive-operations").exists());
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 0);
    let mut foreign = CashuToken::new(&mint.url, vec![proof(ACTIVE, 8)]);
    foreign.unit = Some("usd".into());
    assert!(receive_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        &foreign.serialize().unwrap(),
        7,
        &context
    )
    .await
    .is_err());
    foreign.unit = Some("sat".into());
    foreign.token.push(foreign.token[0].clone());
    assert!(receive_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        &foreign.serialize().unwrap(),
        7,
        &context
    )
    .await
    .is_err());
    *mint.restore_reply.lock().unwrap() = None;
    assert_eq!(
        receive_token_recoverable(
            root.path(),
            &id,
            EcashNetwork::Mainnet,
            &format!("{}/", mint.url),
            &token,
            7,
            &context
        )
        .await
        .unwrap(),
        7
    );
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 7);
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
}

fn rewrite_receive_phase(root: &std::path::Path, id: &str, phase: Value) {
    use sha2::{Digest, Sha256};
    let path = root.join(format!("wallet/receive-operations/{id}.json"));
    let mut envelope: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut record: Value = serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
    record["phase"] = phase;
    let payload = serde_json::to_string(&record).unwrap();
    envelope["checksum"] = json!(hex::encode(Sha256::digest(payload.as_bytes())));
    envelope["payload"] = json!(payload);
    std::fs::write(path, serde_json::to_vec(&envelope).unwrap()).unwrap();
}

#[tokio::test]
async fn recoverable_receive_commit_boundaries_survive_history_pruning_without_recredit() {
    use sha2::{Digest, Sha256};
    let mint = Mint::start(0, None).await;
    let root = mint.wallet().await;
    let token = CashuToken::new(&mint.url, vec![proof(ACTIVE, 8)])
        .serialize()
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let context = "ab".repeat(32);
    assert_eq!(
        receive_token_recoverable(
            root.path(),
            &id,
            EcashNetwork::Mainnet,
            &mint.url,
            &token,
            8,
            &context
        )
        .await
        .unwrap(),
        8
    );
    let mut wallet = load_wallet(root.path()).await.unwrap();
    let proofs: Vec<_> = wallet.proofs.iter().map(|p| p.proof.clone()).collect();
    let committing =
        json!({"Committing":{"proofs":proofs,"before":hex::encode(Sha256::digest(b"missing"))}});
    let journal_path = root
        .path()
        .join(format!("wallet/receive-operations/{id}.json"));
    let committed_record = std::fs::read(&journal_path).unwrap();
    // Crash after purse save but before phase save; unrelated history pruning
    // preserves the durable marker and must not restore already-spent outputs.
    rewrite_receive_phase(root.path(), &id, committing.clone());
    wallet.transactions.clear();
    wallet.proofs.clear();
    save_wallet(root.path(), &wallet).await.unwrap();
    let purse = std::fs::read(root.path().join("wallet/ecash.json")).unwrap();
    assert_eq!(
        receive_token_recoverable(
            root.path(),
            &id,
            EcashNetwork::Mainnet,
            &mint.url,
            &token,
            8,
            &context
        )
        .await
        .unwrap(),
        8
    );
    assert_eq!(
        std::fs::read(root.path().join("wallet/ecash.json")).unwrap(),
        purse
    );
    // Old writers dropping the marker cause a recovery hold, never a guessed credit.
    rewrite_receive_phase(root.path(), &id, committing.clone());
    wallet.receive_commits.clear();
    save_wallet(root.path(), &wallet).await.unwrap();
    assert!(receive_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        &token,
        8,
        &context
    )
    .await
    .unwrap_err()
    .to_string()
    .contains("manual recovery"));
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 0);
    // Crash before the purse save: exact absent pre-image authorizes first commit.
    std::fs::remove_file(root.path().join("wallet/ecash.json")).unwrap();
    assert_eq!(
        receive_token_recoverable(
            root.path(),
            &id,
            EcashNetwork::Mainnet,
            &mint.url,
            &token,
            8,
            &context
        )
        .await
        .unwrap(),
        8
    );
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 8);
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    // Completed receipt survives a missing purse without rebuilding its proofs.
    std::fs::write(journal_path, committed_record).unwrap();
    std::fs::remove_file(root.path().join("wallet/ecash.json")).unwrap();
    assert_eq!(
        receive_token_recoverable(
            root.path(),
            &id,
            EcashNetwork::Mainnet,
            &mint.url,
            &token,
            8,
            &context
        )
        .await
        .unwrap(),
        8
    );
    assert!(!root.path().join("wallet/ecash.json").exists());
}

#[tokio::test]
async fn recoverable_receive_corrupt_claims_and_write_failures_preserve_funds() {
    let mint = Mint::start(0, None).await;
    let root = mint.wallet().await;
    let token = CashuToken::new(&mint.url, vec![proof(ACTIVE, 8)])
        .serialize()
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let context = "ab".repeat(32);
    // A directory at the target blocks the private journal replacement before POST.
    let path = root
        .path()
        .join(format!("wallet/receive-operations/{id}.json"));
    std::fs::create_dir_all(&path).unwrap();
    assert!(receive_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        &token,
        8,
        &context
    )
    .await
    .is_err());
    assert!(mint.requests.lock().unwrap().is_empty());
    std::fs::remove_dir(&path).unwrap();
    mint.lose_swap_reply
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(receive_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        &token,
        8,
        &context
    )
    .await
    .is_err());
    let saved = std::fs::read(&path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
    std::fs::write(&path, b"damaged").unwrap();
    assert!(receive_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        &token,
        8,
        &context
    )
    .await
    .is_err());
    assert!(receive_token_recoverable(
        root.path(),
        &uuid::Uuid::new_v4().to_string(),
        EcashNetwork::Mainnet,
        &mint.url,
        &token,
        8,
        &context
    )
    .await
    .is_err());
    assert!(receive_token(root.path(), &token).await.is_err());
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    std::fs::write(&path, saved).unwrap();
    assert_eq!(
        receive_token_recoverable(
            root.path(),
            &id,
            EcashNetwork::Mainnet,
            &mint.url,
            &token,
            8,
            &context
        )
        .await
        .unwrap(),
        8
    );
}

#[tokio::test]
async fn recoverable_receive_unspent_retry_reuses_exact_request_and_waits_for_verified_state() {
    let mint = Mint::start(0, Some(503)).await;
    let root = mint.wallet().await;
    let token = CashuToken::new(&mint.url, vec![proof(ACTIVE, 8)])
        .serialize()
        .unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let context = "ab".repeat(32);
    assert!(receive_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        &token,
        8,
        &context
    )
    .await
    .is_err());
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    // Legacy paid-content redemption must respect claims even while mint inputs
    // remain unspent; otherwise it could steal the pending operation's proofs.
    assert!(verify_and_receive_payment(root.path(), &token, 8)
        .await
        .unwrap_err()
        .to_string()
        .contains("another settlement"));
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    let original = mint.requests.lock().unwrap()[0].clone();
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 0);
    // An empty state response must not authorize a second POST.
    *mint.state_reply.lock().unwrap() = Some(json!({"states":[]}));
    mint.failure.store(0, std::sync::atomic::Ordering::SeqCst);
    assert!(receive_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        &token,
        8,
        &context
    )
    .await
    .is_err());
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    *mint.state_reply.lock().unwrap() = None;
    assert_eq!(
        receive_token_recoverable(
            root.path(),
            &id,
            EcashNetwork::Mainnet,
            &mint.url,
            &token,
            8,
            &context
        )
        .await
        .unwrap(),
        8
    );
    let requests = mint.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1], original);
    drop(requests);
    let wallet = load_wallet(root.path()).await.unwrap();
    assert_eq!(wallet.balance(), 8);
    assert_eq!(wallet.transactions.len(), 1);
    assert_eq!(wallet.receive_commits.len(), 1);
}

#[tokio::test]
async fn purchase_deadline_rejects_fresh_exact_send_but_recovers_prior_committed_token() {
    let root = tempfile::tempdir().unwrap();
    let mint = "https://unused-mint.invalid";
    let mut wallet = WalletState::default();
    wallet.mint_url = mint.into();
    wallet.add_proofs(mint, vec![proof(ACTIVE, 8)]);
    save_wallet(root.path(), &wallet).await.unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let context = "ab".repeat(32);
    assert!(send_token_recoverable_before(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        mint,
        8,
        &context,
        1
    )
    .await
    .is_err());
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 8);
    // Fixture a prior completed send; an expired caller must recover its result,
    // not require another payment or credit the old proofs back to the purse.
    let original =
        send_token_recoverable(root.path(), &id, EcashNetwork::Mainnet, mint, 8, &context)
            .await
            .unwrap();
    assert_eq!(
        send_token_recoverable_before(
            root.path(),
            &id,
            EcashNetwork::Mainnet,
            mint,
            8,
            &context,
            1
        )
        .await
        .unwrap(),
        original
    );
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 0);
}

#[tokio::test]
async fn expired_purchase_recovers_issued_swap_but_never_posts_still_unspent_inputs() {
    for issued in [false, true] {
        let mint = Mint::start(0, if issued { None } else { Some(503) }).await;
        let root = mint.wallet().await;
        let mut wallet = load_wallet(root.path()).await.unwrap();
        wallet.add_proofs(&mint.url, vec![proof(ACTIVE, 8)]);
        save_wallet(root.path(), &wallet).await.unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let context = "ab".repeat(32);
        mint.lose_swap_reply
            .store(issued, std::sync::atomic::Ordering::SeqCst);
        assert!(send_token_recoverable(
            root.path(),
            &id,
            EcashNetwork::Mainnet,
            &mint.url,
            4,
            &context
        )
        .await
        .is_err());
        mint.failure.store(0, std::sync::atomic::Ordering::SeqCst);
        let result = send_token_recoverable_before(
            root.path(),
            &id,
            EcashNetwork::Mainnet,
            &mint.url,
            4,
            &context,
            1,
        )
        .await;
        if issued {
            assert_eq!(
                CashuToken::deserialize(&result.unwrap())
                    .unwrap()
                    .total_amount(),
                4
            );
            assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 4);
        } else {
            assert!(result.unwrap_err().to_string().contains("expired"));
            assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 0);
            assert!(load_wallet(root.path())
                .await
                .unwrap()
                .proofs
                .iter()
                .any(|p| p.reserved));
        }
        assert_eq!(mint.requests.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn purchase_executor_recovers_prior_buyer_spend_and_delayed_accepted_seller_settlement() {
    use crate::content_purchase::{BuyerPhase, Contract, Journal};
    use crate::content_purchase_executor::{prepare_buyer_token, settle_seller_token};
    let mint = Mint::start(0, None).await;
    let buyer = mint.wallet().await;
    let seller = mint.wallet().await;
    let contract = Contract {
        version: 1,
        id: uuid::Uuid::new_v4().to_string(),
        buyer_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([1; 32])).unwrap(),
        seller_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([2; 32])).unwrap(),
        content_id: "film-1".into(),
        content_sha256: "ab".repeat(32),
        content_size: 1024,
        terms_sha256: "cd".repeat(32),
        network: EcashNetwork::Mainnet,
        mint_url: mint.url.clone(),
        gross_token_sats: 8,
        minimum_net_sats: 8,
        offered_at: 1000,
        expires_at: 2000,
    };
    let mut wallet = load_wallet(buyer.path()).await.unwrap();
    wallet.add_proofs(&mint.url, vec![proof(ACTIVE, 8)]);
    save_wallet(buyer.path(), &wallet).await.unwrap();
    // An expired offer without durable seller acceptance cannot redeem a token
    // or create settlement state, even when the token and buyer are otherwise valid.
    let unaccepted_token = CashuToken::new(&mint.url, vec![proof(ACTIVE, 8)])
        .serialize()
        .unwrap();
    let seller_wallet_before = std::fs::read(seller.path().join("wallet/ecash.json")).ok();
    let rejected = settle_seller_token(
        seller.path(),
        &contract,
        &unaccepted_token,
        &contract.buyer_did,
    )
    .await
    .err()
    .unwrap();
    assert!(rejected
        .to_string()
        .contains("Seller intent is not durable"));
    assert!(mint.requests.lock().unwrap().is_empty());
    assert_eq!(
        std::fs::read(seller.path().join("wallet/ecash.json")).ok(),
        seller_wallet_before
    );
    {
        let journal = Journal::open(seller.path()).await.unwrap();
        assert!(journal.seller(&contract.id).await.unwrap().is_none());
    }
    // Deterministically fixture durable acceptance before the historical deadline.
    let accepted = {
        let journal = Journal::open(seller.path()).await.unwrap();
        journal
            .prepare_seller(&contract, 1500)
            .await
            .unwrap()
            .acceptance()
            .unwrap()
    };
    {
        let journal = Journal::open(buyer.path()).await.unwrap();
        journal.prepare_buyer(&contract, 1500).await.unwrap();
        assert!(journal
            .record_acceptance(&contract, &accepted, &contract.buyer_did)
            .await
            .is_err());
        journal
            .record_acceptance(&contract, &accepted, &contract.seller_did)
            .await
            .unwrap();
    }
    // Fixture a prior wallet commit, interrupted before the buyer journal saved
    // its token. The actual executor must recover that result after expiry.
    let original = send_token_recoverable(
        buyer.path(),
        &contract.id,
        contract.network,
        &mint.url,
        8,
        &contract.context_hash().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        prepare_buyer_token(buyer.path(), &contract).await.unwrap(),
        original
    );
    assert!(mint.requests.lock().unwrap().is_empty());
    mint.lose_swap_reply
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(
        settle_seller_token(seller.path(), &contract, &original, &contract.buyer_did)
            .await
            .is_err()
    );
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    let receipt = settle_seller_token(seller.path(), &contract, &original, &contract.buyer_did)
        .await
        .unwrap();
    assert!(
        settle_seller_token(seller.path(), &contract, &original, &contract.buyer_did)
            .await
            .unwrap()
            == receipt
    );
    assert!(
        settle_seller_token(seller.path(), &contract, &original, &contract.seller_did)
            .await
            .is_err()
    );
    let altered = CashuToken::new(&mint.url, vec![proof(V2, 8)])
        .serialize()
        .unwrap();
    assert!(
        settle_seller_token(seller.path(), &contract, &altered, &contract.buyer_did)
            .await
            .is_err()
    );
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    assert_eq!(load_wallet(buyer.path()).await.unwrap().balance(), 0);
    assert_eq!(load_wallet(seller.path()).await.unwrap().balance(), 8);
    let journal = Journal::open(buyer.path()).await.unwrap();
    journal.record_receipt(&contract, &receipt).await.unwrap();
    assert_eq!(
        journal.buyer(&contract.id).await.unwrap().unwrap().phase,
        BuyerPhase::ReceiptSaved
    );
}

#[tokio::test]
async fn purchase_expiring_during_mint_preflight_never_reserves_or_posts_swap() {
    use std::sync::atomic::{AtomicI64, Ordering};
    let mint = Mint::start(0, None).await;
    let root = mint.wallet().await;
    let mut wallet = load_wallet(root.path()).await.unwrap();
    wallet.add_proofs(&mint.url, vec![proof(ACTIVE, 8)]);
    save_wallet(root.path(), &wallet).await.unwrap();
    let wallet_before = std::fs::read(root.path().join("wallet/ecash.json")).unwrap();
    let clock = Arc::new(AtomicI64::new(1000));
    *mint.restore_clock.lock().unwrap() = Some((clock.clone(), 2000));
    let id = uuid::Uuid::new_v4().to_string();
    let error = send_token_recoverable_with_deadline(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        4,
        &"ab".repeat(32),
        Some(2000),
        || clock.load(Ordering::SeqCst),
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(
        clock.load(Ordering::SeqCst),
        2000,
        "preflight must have completed"
    );
    assert!(error.to_string().contains("expired"));
    assert!(
        mint.requests.lock().unwrap().is_empty(),
        "no swap POST after expiry"
    );
    assert_eq!(
        std::fs::read(root.path().join("wallet/ecash.json")).unwrap(),
        wallet_before
    );
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 8);
    let held = crate::wallet::mutation::guard(root.path()).await.unwrap();
    assert!(crate::wallet::send_journal::Journal::new(&held)
        .load(&id)
        .await
        .unwrap()
        .is_none());
}

// Append to wallet/payment_tests.rs when the next payment-plan batch is applied.
#[tokio::test]
async fn stale_planned_inputs_do_not_reselect_wallet_coins_or_post_to_mint() {
    use crate::wallet::{mutation, send_journal};
    let mint = Mint::start(0, None).await;
    let root = tempfile::tempdir().unwrap();
    let mut wallet = WalletState::default();
    wallet.mint_url = mint.url.clone();
    let original = proof(ACTIVE, 8);
    wallet.add_proofs(&mint.url, vec![original.clone()]);
    save_wallet(root.path(), &wallet).await.unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let context = "ab".repeat(32);
    let prepared = MintClient::new(&mint.url)
        .unwrap()
        .prepare_swap_at_least(&[original], &[4, 4], 4)
        .await
        .unwrap();
    {
        let guard = mutation::guard(root.path()).await.unwrap();
        send_journal::Journal::new(&guard)
            .prepare(
                send_journal::Binding {
                    id: id.clone(),
                    network: EcashNetwork::Mainnet,
                    mint_url: mint.url.clone(),
                    amount_sats: 4,
                    context_hash: context.clone(),
                },
                send_journal::Request::Swap(prepared),
            )
            .await
            .unwrap();
    }
    // Simulate a selected proof becoming unavailable before reservation. Other
    // sufficient coins exist, but the accepted immutable plan cannot select them.
    wallet.proofs.clear();
    let mut replacement = proof(ACTIVE, 8);
    replacement.secret = "replacement-not-in-plan".into();
    wallet.add_proofs(&mint.url, vec![replacement]);
    save_wallet(root.path(), &wallet).await.unwrap();
    let before = std::fs::read(root.path().join("wallet/ecash.json")).unwrap();
    assert!(send_token_recoverable(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        &mint.url,
        4,
        &context
    )
    .await
    .is_err());
    assert_eq!(
        std::fs::read(root.path().join("wallet/ecash.json")).unwrap(),
        before
    );
    assert!(mint.requests.lock().unwrap().is_empty());
}

// Add within wallet payment_tests, using its existing fake proof fixtures.
#[tokio::test]
async fn expired_saved_exact_plan_never_reserves_spendable_inputs() {
    let root = tempfile::tempdir().unwrap();
    let mint = "https://unused-mint.invalid";
    let mut wallet = WalletState::default();
    wallet.mint_url = mint.into();
    wallet.add_proofs(mint, vec![proof(ACTIVE, 8)]);
    save_wallet(root.path(), &wallet).await.unwrap();
    let original = std::fs::read(root.path().join("wallet/ecash.json")).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let context = "ab".repeat(32);
    {
        let guard = crate::wallet::mutation::guard(root.path()).await.unwrap();
        let binding = crate::wallet::send_journal::Binding {
            id: id.clone(),
            network: EcashNetwork::Mainnet,
            mint_url: mint.into(),
            amount_sats: 8,
            context_hash: context.clone(),
        };
        crate::wallet::send_journal::Journal::new(&guard)
            .prepare(
                binding,
                crate::wallet::send_journal::Request::Exact {
                    proofs: vec![proof(ACTIVE, 8)],
                },
            )
            .await
            .unwrap();
    }
    assert!(send_token_recoverable_before(
        root.path(),
        &id,
        EcashNetwork::Mainnet,
        mint,
        8,
        &context,
        chrono::Utc::now().timestamp() - 1
    )
    .await
    .is_err());
    assert_eq!(
        std::fs::read(root.path().join("wallet/ecash.json")).unwrap(),
        original
    );
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 8);
}

// Append to wallet/payment_tests.rs after integrating dispatch/cancellation APIs.
#[tokio::test]
async fn cancelled_exact_plan_cannot_later_send_and_releases_only_its_reservation() {
    use crate::wallet::{
        mutation,
        send_journal::{Binding, Journal, Request},
    };
    let root = tempfile::tempdir().unwrap();
    let mint = "https://unused-mint.invalid";
    let mut wallet = WalletState::default();
    wallet.mint_url = mint.into();
    wallet.add_proofs(mint, vec![proof(ACTIVE, 8), proof(ACTIVE, 4)]);
    save_wallet(root.path(), &wallet).await.unwrap();
    let binding = Binding {
        id: uuid::Uuid::new_v4().to_string(),
        network: EcashNetwork::Mainnet,
        mint_url: mint.into(),
        amount_sats: 8,
        context_hash: "ab".repeat(32),
    };
    {
        let guard = mutation::guard(root.path()).await.unwrap();
        let journal = Journal::new(&guard);
        journal
            .prepare(
                binding.clone(),
                Request::Exact {
                    proofs: vec![proof(ACTIVE, 8)],
                },
            )
            .await
            .unwrap();
        journal.reserve_wallet(&binding).await.unwrap();
        assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 4);
        journal.cancel_unspent(&binding).await.unwrap();
        journal.cancel_unspent(&binding).await.unwrap();
    }
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 12);
    assert!(send_token_recoverable(
        root.path(),
        &binding.id,
        binding.network,
        mint,
        8,
        &binding.context_hash
    )
    .await
    .is_err());
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 12);
    assert!(load_wallet(root.path())
        .await
        .unwrap()
        .transactions
        .is_empty());
}
#[tokio::test]
async fn dispatched_or_legacy_unknown_swap_cannot_cancel_despite_unspent_mint_inputs() {
    use crate::wallet::{
        mutation,
        send_journal::{Binding, Journal, Request},
    };
    use sha2::{Digest, Sha256};
    for legacy in [false, true] {
        let mint = Mint::start(0, None).await;
        let root = tempfile::tempdir().unwrap();
        let mut wallet = WalletState::default();
        wallet.mint_url = mint.url.clone();
        wallet.add_proofs(&mint.url, vec![proof(ACTIVE, 8)]);
        save_wallet(root.path(), &wallet).await.unwrap();
        let binding = Binding {
            id: uuid::Uuid::new_v4().to_string(),
            network: EcashNetwork::Mainnet,
            mint_url: mint.url.clone(),
            amount_sats: 4,
            context_hash: "ab".repeat(32),
        };
        let prepared = MintClient::new(&mint.url)
            .unwrap()
            .prepare_swap_at_least(&[proof(ACTIVE, 8)], &[4, 4], 4)
            .await
            .unwrap();
        {
            let guard = mutation::guard(root.path()).await.unwrap();
            let journal = Journal::new(&guard);
            journal
                .prepare(binding.clone(), Request::Swap(prepared))
                .await
                .unwrap();
            journal.reserve_wallet(&binding).await.unwrap();
            if !legacy {
                journal.mark_dispatched(&binding).await.unwrap();
            }
        }
        if legacy {
            let path = root
                .path()
                .join("wallet/send-operations")
                .join(format!("{}.json", binding.id));
            let mut envelope: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let mut payload: serde_json::Value =
                serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
            payload.as_object_mut().unwrap().remove("dispatch");
            let payload = serde_json::to_string(&payload).unwrap();
            envelope["checksum"] = json!(hex::encode(Sha256::digest(payload.as_bytes())));
            envelope["payload"] = json!(payload);
            std::fs::write(path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        }
        let guard = mutation::guard(root.path()).await.unwrap();
        assert!(Journal::new(&guard).cancel_unspent(&binding).await.is_err());
        assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 0);
        assert!(mint.requests.lock().unwrap().is_empty());
    }
}
#[tokio::test]
async fn planner_rejects_wrong_network_before_creating_intent_or_reservation() {
    let root = tempfile::tempdir().unwrap();
    let mut wallet = WalletState::default();
    wallet.mint_url = "http://127.0.0.1:1".into();
    wallet.add_proofs("http://127.0.0.1:1", vec![proof(ACTIVE, 8)]);
    save_wallet(root.path(), &wallet).await.unwrap();
    let original = std::fs::read(root.path().join("wallet/ecash.json")).unwrap();
    let error = crate::wallet::purchase_plan::prepare(
        root.path(),
        "http://127.0.0.1:1",
        EcashNetwork::Testnet,
        4,
        8,
    )
    .await
    .err()
    .unwrap();
    assert!(error.to_string().contains("another wallet network"));
    assert_eq!(
        std::fs::read(root.path().join("wallet/ecash.json")).unwrap(),
        original
    );
    assert!(!root.path().join("content-purchases").exists());
    assert!(!root.path().join("wallet/send-operations").exists());
}
#[tokio::test]
async fn planner_skips_unfundable_swaps_and_finds_later_exact_shape_within_budget() {
    let mint = Mint::start(2000, None).await;
    let root = tempfile::tempdir().unwrap();
    let mut wallet = WalletState::default();
    wallet.mint_url = mint.url.clone();
    wallet.add_proofs(&mint.url, vec![proof(ACTIVE, 8), proof(ACTIVE, 4)]);
    save_wallet(root.path(), &wallet).await.unwrap();
    let plan =
        crate::wallet::purchase_plan::prepare(root.path(), &mint.url, EcashNetwork::Mainnet, 7, 12)
            .await
            .unwrap();
    assert_eq!(plan.fee_plan.gross_sats, 12);
    assert_eq!(plan.fee_plan.fee_sats, 4);
    assert_eq!(plan.fee_plan.net_sats, 8);
    assert_eq!(plan.wallet_debit_sats, 12);
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 12);
    assert!(mint.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn cancellation_seal_wins_against_an_already_waiting_fresh_swap_executor() {
    use crate::wallet::{
        mutation,
        send_journal::{Binding, Journal, Request},
    };
    let mint = Mint::start(0, None).await;
    let root = tempfile::tempdir().unwrap();
    let mut wallet = WalletState::default();
    wallet.mint_url = mint.url.clone();
    wallet.add_proofs(&mint.url, vec![proof(ACTIVE, 8)]);
    save_wallet(root.path(), &wallet).await.unwrap();
    let binding = Binding {
        id: uuid::Uuid::new_v4().to_string(),
        network: EcashNetwork::Mainnet,
        mint_url: mint.url.clone(),
        amount_sats: 4,
        context_hash: "ab".repeat(32),
    };
    let prepared = MintClient::new(&mint.url)
        .unwrap()
        .prepare_swap_at_least(&[proof(ACTIVE, 8)], &[4, 4], 4)
        .await
        .unwrap();
    let guard = mutation::guard(root.path()).await.unwrap();
    let journal = Journal::new(&guard);
    journal
        .prepare(binding.clone(), Request::Swap(prepared))
        .await
        .unwrap();
    journal.reserve_wallet(&binding).await.unwrap();
    let waiting = send_token_recoverable(
        root.path(),
        &binding.id,
        binding.network,
        &binding.mint_url,
        binding.amount_sats,
        &binding.context_hash,
    );
    tokio::pin!(waiting);
    assert!(futures_util::poll!(&mut waiting).is_pending());
    journal.cancel_unspent(&binding).await.unwrap();
    drop(journal);
    drop(guard);
    assert!(waiting.await.is_err());
    assert!(mint.requests.lock().unwrap().is_empty());
    assert_eq!(load_wallet(root.path()).await.unwrap().balance(), 8);
    assert!(load_wallet(root.path())
        .await
        .unwrap()
        .transactions
        .is_empty());
}

// Append under wallet/payment_tests.rs: real local journals + fake mint, no real funds.
struct PurchaseTestTransport {
    seller_root: std::path::PathBuf,
    template: crate::content_purchase_protocol::Offer,
    lose_offer: std::sync::atomic::AtomicBool,
    lose_accept: std::sync::atomic::AtomicBool,
    lose_settle: std::sync::atomic::AtomicBool,
    offers: std::sync::Mutex<Vec<String>>,
}
impl crate::content_purchase_caller::PurchaseTransport for PurchaseTestTransport {
    fn seller_did(&self) -> &str {
        &self.template.seller_did
    }
    fn seller_onion(&self) -> &str {
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.onion"
    }
    async fn prepare_offer(
        &self,
        content_id: &str,
        expected: Option<&crate::content_purchase_caller::ExpectedRental>,
    ) -> anyhow::Result<Option<(u64, u64)>> {
        if let Some(expected) = expected {
            expected.verify(&self.template)?;
        }
        // A registered movie still being hashed, without asking the fake mint.
        Ok(content_id.starts_with("registered_").then_some((3, 16)))
    }
    async fn offer(
        &self,
        id: &str,
        content_id: &str,
    ) -> anyhow::Result<crate::content_purchase_protocol::Offer> {
        self.offers.lock().unwrap().push(id.into());
        let mut offer = self.template.clone();
        offer.id = id.into();
        offer.content_id = content_id.into();
        let saved = crate::content_purchase_protocol::save_offer(
            &self.seller_root,
            &offer,
            &offer.buyer_did,
            chrono::Utc::now().timestamp(),
        )
        .await?;
        anyhow::ensure!(
            !self
                .lose_offer
                .swap(false, std::sync::atomic::Ordering::SeqCst),
            "Lost offer response"
        );
        Ok(saved)
    }
    async fn accept(
        &self,
        envelope: &crate::content_purchase_protocol::Envelope,
    ) -> anyhow::Result<crate::content_purchase_protocol::Accepted> {
        let reply = crate::content_purchase_protocol::accept(
            &self.seller_root,
            envelope,
            &envelope.offer.buyer_did,
            || chrono::Utc::now().timestamp(),
        )
        .await?;
        anyhow::ensure!(
            !self
                .lose_accept
                .swap(false, std::sync::atomic::Ordering::SeqCst),
            "Lost acceptance response"
        );
        Ok(reply)
    }
    async fn status(
        &self,
        envelope: &crate::content_purchase_protocol::Envelope,
    ) -> anyhow::Result<crate::content_purchase_protocol::SellerStatus> {
        crate::content_purchase_protocol::status(
            &self.seller_root,
            envelope,
            &envelope.offer.buyer_did,
        )
        .await
    }
    async fn cancel(
        &self,
        envelope: &crate::content_purchase_protocol::Envelope,
    ) -> anyhow::Result<crate::content_purchase_protocol::Cancelled> {
        crate::content_purchase_protocol::cancel(
            &self.seller_root,
            envelope,
            &envelope.offer.buyer_did,
        )
        .await
    }
    async fn settle(
        &self,
        request: &crate::content_purchase_protocol::Settlement,
    ) -> anyhow::Result<crate::content_purchase::Receipt> {
        let reply = crate::content_purchase_protocol::settle(
            &self.seller_root,
            request,
            &request.envelope.offer.buyer_did,
        )
        .await?;
        anyhow::ensure!(
            !self
                .lose_settle
                .swap(false, std::sync::atomic::Ordering::SeqCst),
            "Invalid purchase response after settlement"
        );
        Ok(reply)
    }
}
#[tokio::test]
async fn full_caller_recovers_lost_acceptance_and_settlement_without_another_payment() {
    use crate::content_purchase_caller::{purchase, PurchaseConsent, ReadyPurchase};
    use std::sync::atomic::{AtomicBool, Ordering};
    let mint = Mint::start(0, None).await;
    let buyer = tempfile::tempdir().unwrap();
    let seller = mint.wallet().await;
    let buyer_did = crate::identity::did_key_from_pubkey_hex(&hex::encode([1u8; 32])).unwrap();
    let seller_did = crate::identity::did_key_from_pubkey_hex(&hex::encode([2u8; 32])).unwrap();
    let mut wallet = WalletState::default();
    wallet.mint_url = mint.url.clone();
    wallet.add_proofs(&mint.url, vec![proof(ACTIVE, 8)]);
    save_wallet(buyer.path(), &wallet).await.unwrap();
    let mut wallet = WalletState::default();
    wallet.mint_url = mint.url.clone();
    save_wallet(seller.path(), &wallet).await.unwrap();
    let now = chrono::Utc::now().timestamp();
    let transport = PurchaseTestTransport {
        seller_root: seller.path().into(),
        template: crate::content_purchase_protocol::Offer {
            id: uuid::Uuid::new_v4().to_string(),
            buyer_did: buyer_did.clone(),
            seller_did,
            content_id: "paid-film".into(),
            filename: "film.mp4".into(),
            mime_type: "video/mp4".into(),
            content_sha256: "ab".repeat(32),
            content_size: 16,
            viewing_seconds: None,
            terms_sha256: "cd".repeat(32),
            network: EcashNetwork::Mainnet,
            mint_url: mint.url.clone(),
            seller_net_sats: 8,
            offered_at: now,
            expires_at: now + 300,
        },
        lose_offer: AtomicBool::new(true),
        lose_accept: AtomicBool::new(true),
        lose_settle: AtomicBool::new(true),
        offers: Default::default(),
    };
    let preparing_id = format!("registered_{}", uuid::Uuid::new_v4());
    for _ in 0..3 {
        assert!(matches!(
            purchase(
                buyer.path(),
                &buyer_did,
                &preparing_id,
                None,
                8,
                None,
                &transport
            )
            .await
            .unwrap(),
            ReadyPurchase::Preparing {
                completed_bytes: 3,
                total_bytes: 16
            }
        ));
    }
    assert!(transport.offers.lock().unwrap().is_empty());
    assert!(mint.requests.lock().unwrap().is_empty());
    assert_eq!(load_wallet(buyer.path()).await.unwrap().balance(), 8);
    assert!(crate::content_purchase::Journal::open(buyer.path())
        .await
        .unwrap()
        .find_buyers(&buyer_did, &transport.template.seller_did, &preparing_id)
        .await
        .unwrap()
        .is_empty());
    assert!(purchase(
        buyer.path(),
        &buyer_did,
        "paid-film",
        Some("film.mp4"),
        8,
        None,
        &transport
    )
    .await
    .is_err());
    assert!(mint.requests.lock().unwrap().is_empty());
    assert_eq!(load_wallet(buyer.path()).await.unwrap().balance(), 8);
    let quote = purchase(
        buyer.path(),
        &buyer_did,
        "paid-film",
        Some("film.mp4"),
        8,
        None,
        &transport,
    )
    .await
    .unwrap();
    let consent = match quote {
        ReadyPurchase::AwaitingConfirmation {
            operation_id,
            envelope_sha256,
            wallet_debit_sats,
            ..
        } => PurchaseConsent {
            operation_id,
            envelope_sha256,
            wallet_debit_sats,
        },
        _ => panic!("Fresh purchase spent before confirmation"),
    };
    let reopened = purchase(
        buyer.path(),
        &buyer_did,
        "paid-film",
        Some("film.mp4"),
        9_007_199_254_740_991,
        None,
        &transport,
    )
    .await
    .unwrap();
    match reopened {
        ReadyPurchase::AwaitingConfirmation {
            operation_id,
            wallet_debit_sats,
            ..
        } => {
            assert_eq!(operation_id, consent.operation_id);
            assert_eq!(wallet_debit_sats, consent.wallet_debit_sats);
        }
        _ => panic!("A broad quote budget initiated spending without consent"),
    }
    assert!(mint.requests.lock().unwrap().is_empty());
    assert_eq!(load_wallet(buyer.path()).await.unwrap().balance(), 8);
    // A quote alone does not pin seller policy. Removing acceptance must stop
    // the buyer before any token is exposed; the same quote can resume later.
    save_accepted_mints(seller.path(), &AcceptedMints { mints: vec![] })
        .await
        .unwrap();
    let rejected = purchase(
        buyer.path(),
        &buyer_did,
        "paid-film",
        Some("film.mp4"),
        8,
        Some(&consent),
        &transport,
    )
    .await
    .err()
    .unwrap();
    assert!(rejected
        .to_string()
        .contains("does not accept the quoted mint"));
    assert_eq!(load_wallet(buyer.path()).await.unwrap().balance(), 8);
    assert!(mint.requests.lock().unwrap().is_empty());
    save_accepted_mints(
        seller.path(),
        &AcceptedMints {
            mints: vec![mint.url.clone()],
        },
    )
    .await
    .unwrap();
    let error = purchase(
        buyer.path(),
        &buyer_did,
        "paid-film",
        Some("film.mp4"),
        8,
        Some(&consent),
        &transport,
    )
    .await
    .err()
    .expect("The simulated lost response must surface");
    assert!(
        error.to_string().contains("Lost acceptance response"),
        "unexpected purchase failure: {error:#}"
    );
    assert!(mint.requests.lock().unwrap().is_empty());
    assert_eq!(load_wallet(buyer.path()).await.unwrap().balance(), 8);
    // Durable acceptance now pins redemption policy until cancellation or
    // settlement, including when the acceptance reply was lost.
    assert!(
        save_accepted_mints(seller.path(), &AcceptedMints { mints: vec![] })
            .await
            .is_err()
    );
    assert!(save_network(seller.path(), EcashNetwork::Testnet)
        .await
        .is_err());
    assert_eq!(
        load_network(seller.path()).await.unwrap(),
        EcashNetwork::Mainnet
    );
    let error = purchase(
        buyer.path(),
        &buyer_did,
        "paid-film",
        Some("film.mp4"),
        8,
        Some(&consent),
        &transport,
    )
    .await
    .err()
    .expect("The simulated lost response must surface");
    assert!(
        error
            .to_string()
            .contains("Invalid purchase response after settlement"),
        "unexpected purchase failure: {error:#}"
    );
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    let recovered = purchase(
        buyer.path(),
        &buyer_did,
        "paid-film",
        Some("film.mp4"),
        8,
        None,
        &transport,
    )
    .await
    .unwrap();
    let original = match recovered {
        ReadyPurchase::Entitlement { contract, receipt } => {
            assert_eq!(contract.id, consent.operation_id);
            receipt
        }
        _ => panic!("Original entitlement was not recovered"),
    };
    let again = purchase(
        buyer.path(),
        &buyer_did,
        "paid-film",
        Some("film.mp4"),
        8,
        None,
        &transport,
    )
    .await
    .unwrap();
    match again {
        ReadyPurchase::Entitlement { receipt, .. } => assert!(receipt == original),
        _ => panic!("Receipt replay changed"),
    }
    assert_eq!(transport.offers.lock().unwrap().len(), 2);
    assert_ne!(transport.offers.lock().unwrap()[0], consent.operation_id);
    assert_eq!(transport.offers.lock().unwrap()[1], consent.operation_id);
    assert_eq!(mint.requests.lock().unwrap().len(), 1);
    assert_eq!(load_wallet(buyer.path()).await.unwrap().balance(), 0);
    assert_eq!(load_wallet(seller.path()).await.unwrap().balance(), 8);
    assert_eq!(
        load_wallet(buyer.path()).await.unwrap().transactions.len(),
        1
    );
    assert_eq!(
        load_wallet(seller.path()).await.unwrap().transactions.len(),
        1
    );
    assert!(!transport.lose_accept.load(Ordering::SeqCst));
}

#[tokio::test]
async fn rental_catalog_term_mismatch_never_plans_or_creates_buyer_intent() {
    use crate::content_purchase_caller::{purchase_bound, ExpectedRental};
    use std::sync::atomic::AtomicBool;
    let buyer = tempfile::tempdir().unwrap();
    let seller = tempfile::tempdir().unwrap();
    save_accepted_mints(
        seller.path(),
        &AcceptedMints {
            mints: vec!["http://127.0.0.1:1".into()],
        },
    )
    .await
    .unwrap();
    let buyer_did = crate::identity::did_key_from_pubkey_hex(&hex::encode([1u8; 32])).unwrap();
    let seller_did = crate::identity::did_key_from_pubkey_hex(&hex::encode([2u8; 32])).unwrap();
    let now = chrono::Utc::now().timestamp();
    let transport = PurchaseTestTransport {
        seller_root: seller.path().into(),
        template: crate::content_purchase_protocol::Offer {
            id: uuid::Uuid::new_v4().to_string(),
            buyer_did: buyer_did.clone(),
            seller_did: seller_did.clone(),
            content_id: "registered_film".into(),
            filename: "film.mp4".into(),
            mime_type: "video/mp4".into(),
            content_sha256: "ab".repeat(32),
            content_size: 16,
            viewing_seconds: Some(60),
            terms_sha256: "cd".repeat(32),
            network: EcashNetwork::Mainnet,
            mint_url: "http://127.0.0.1:1".into(),
            seller_net_sats: 8,
            offered_at: now,
            expires_at: now + 300,
        },
        lose_offer: AtomicBool::new(false),
        lose_accept: AtomicBool::new(false),
        lose_settle: AtomicBool::new(false),
        offers: Default::default(),
    };
    let expected = ExpectedRental {
        seller_did,
        content_id: "registered_film".into(),
        sha256: "ab".repeat(32),
        price_sats: 8,
        viewing_seconds: 60,
    };
    // Preparation checks already-verified signed metadata, without scanning bytes
    // or allocating a quote. Keep this independent of the fake offer response.
    let metadata = crate::media_registration::Receipt {
        version: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        nonce: "fixture".into(),
        app_audience: "indeedhub".into(),
        node_did: expected.seller_did.clone(),
        producer: "fixture".into(),
        project_id: "fixture".into(),
        price_sats: 8,
        viewing_seconds: 60,
        expires_at: u64::try_from(now + 300).unwrap(),
        content_id: "registered_film".into(),
        sha256: "ab".repeat(32),
        size_bytes: "16".into(),
        payment_methods: vec!["cashu".into()],
        issued_at: u64::try_from(now).unwrap(),
        signature: "verified by registration boundary".into(),
    };
    expected
        .verify_metadata(&expected.seller_did, &metadata)
        .unwrap();
    let mut changed_hash = expected.clone();
    changed_hash.sha256 = "ef".repeat(32);
    let mut changed_price = expected.clone();
    changed_price.price_sats = 9;
    let mut changed_duration = expected.clone();
    changed_duration.viewing_seconds = 120;
    for wrong in [changed_hash, changed_price, changed_duration] {
        assert!(wrong
            .verify_metadata(&expected.seller_did, &metadata)
            .is_err());
        let error = purchase_bound(
            buyer.path(),
            &buyer_did,
            "registered_film",
            None,
            20,
            None,
            &transport,
            Some(&wrong),
        )
        .await
        .err()
        .unwrap();
        assert!(error.to_string().contains("Published rental"), "{error:#}");
        assert!(!buyer.path().join("wallet/ecash.json").exists());
        assert!(!buyer.path().join("wallet/send-operations").exists());
        assert!(crate::content_purchase::Journal::open(buyer.path())
            .await
            .unwrap()
            .find_buyers(&buyer_did, &expected.seller_did, "registered_film")
            .await
            .unwrap()
            .is_empty());
    }
}

#[tokio::test]
async fn unconfirmed_quote_can_cancel_and_requote_without_exposing_wallet_funds() {
    use crate::content_purchase_caller::{purchase, ReadyPurchase};
    use std::sync::atomic::AtomicBool;
    let mint = Mint::start(0, None).await;
    let buyer = tempfile::tempdir().unwrap();
    let seller = mint.wallet().await;
    let buyer_did = crate::identity::did_key_from_pubkey_hex(&hex::encode([1u8; 32])).unwrap();
    let seller_did = crate::identity::did_key_from_pubkey_hex(&hex::encode([2u8; 32])).unwrap();
    let mut wallet = WalletState::default();
    wallet.mint_url = mint.url.clone();
    wallet.add_proofs(&mint.url, vec![proof(ACTIVE, 8)]);
    save_wallet(buyer.path(), &wallet).await.unwrap();
    let mut wallet = WalletState::default();
    wallet.mint_url = mint.url.clone();
    save_wallet(seller.path(), &wallet).await.unwrap();
    let now = chrono::Utc::now().timestamp();
    let transport = PurchaseTestTransport {
        seller_root: seller.path().into(),
        template: crate::content_purchase_protocol::Offer {
            id: uuid::Uuid::new_v4().to_string(),
            buyer_did: buyer_did.clone(),
            seller_did,
            content_id: "paid-film".into(),
            filename: "film.mp4".into(),
            mime_type: "video/mp4".into(),
            content_sha256: "ab".repeat(32),
            content_size: 16,
            viewing_seconds: None,
            terms_sha256: "cd".repeat(32),
            network: EcashNetwork::Mainnet,
            mint_url: mint.url.clone(),
            seller_net_sats: 8,
            offered_at: now,
            expires_at: now + 300,
        },
        lose_offer: AtomicBool::new(false),
        lose_accept: AtomicBool::new(false),
        lose_settle: AtomicBool::new(false),
        offers: Default::default(),
    };
    let quote = purchase(
        buyer.path(),
        &buyer_did,
        "paid-film",
        Some("film.mp4"),
        8,
        None,
        &transport,
    )
    .await
    .unwrap();
    let id = match quote {
        ReadyPurchase::AwaitingConfirmation { operation_id, .. } => operation_id,
        _ => panic!("Quote spent funds"),
    };
    assert!(!buyer
        .path()
        .join("wallet/send-operations")
        .join(format!("{id}.json"))
        .exists());
    let (envelope, plan) = {
        let journal = crate::content_purchase::Journal::open(buyer.path())
            .await
            .unwrap();
        (
            journal
                .protocol_envelope("buyer", &id)
                .await
                .unwrap()
                .unwrap(),
            journal.buyer_plan(&id).await.unwrap().unwrap(),
        )
    };
    crate::content_purchase_caller::cancel_purchase(buyer.path(), &envelope, &plan, &transport)
        .await
        .unwrap();
    crate::content_purchase_caller::cancel_purchase(buyer.path(), &envelope, &plan, &transport)
        .await
        .unwrap();
    assert_eq!(load_wallet(buyer.path()).await.unwrap().balance(), 8);
    assert!(load_wallet(buyer.path())
        .await
        .unwrap()
        .transactions
        .is_empty());
    assert!(mint.requests.lock().unwrap().is_empty());
    let next = purchase(
        buyer.path(),
        &buyer_did,
        "paid-film",
        Some("film.mp4"),
        8,
        None,
        &transport,
    )
    .await
    .unwrap();
    match next {
        ReadyPurchase::AwaitingConfirmation { operation_id, .. } => assert_ne!(operation_id, id),
        _ => panic!("Replacement quote spent funds"),
    }
    assert!(mint.requests.lock().unwrap().is_empty());
}

#[path = "payment_tests/process_fixture.rs"]
mod process_fixture;
