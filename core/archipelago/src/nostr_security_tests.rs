//! Compatibility and hostile-relay regression tests for the 0.44 security update.
//! Loopback sockets run only through scripts/test-backend-isolated.sh.
use nostr_sdk::prelude::*;

#[test]
fn native_signer_encryption_remains_compatible_and_rejects_hostile_payloads() {
    let alice = Keys::generate();
    let bob = Keys::generate();
    let message = "Archipelago signing compatibility — \u{1f30a}";
    let encrypted = nip44::encrypt(
        alice.secret_key(),
        &bob.public_key(),
        message,
        nip44::Version::V2,
    )
    .unwrap();
    assert_eq!(
        nip44::decrypt(bob.secret_key(), &alice.public_key(), &encrypted).unwrap(),
        message
    );
    let encrypted = nip04::encrypt(alice.secret_key(), &bob.public_key(), message).unwrap();
    assert_eq!(
        nip04::decrypt(bob.secret_key(), &alice.public_key(), encrypted).unwrap(),
        message
    );
    // Valid v2 prefix followed by a payload exceeding the codec's maximum.
    // This must fail at the size gate, before allocation/decoding/HMAC work.
    let oversized = format!("AgAA{}", "A".repeat(100_000));
    assert!(matches!(
        nip44::decrypt(bob.secret_key(), &alice.public_key(), oversized),
        Err(nip44::Error::MessageTooLong)
    ));
    for malformed in ["", "Ag==", "not base64!", "?iv=", "YQ==?iv=YQ=="] {
        assert!(nip04::decrypt(bob.secret_key(), &alice.public_key(), malformed).is_err());
        assert!(nip44::decrypt(bob.secret_key(), &alice.public_key(), malformed).is_err());
    }
}

#[tokio::test]
async fn relay_cannot_substitute_forged_fields_for_a_known_event_id() {
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, tungstenite::Message};
    let test = async {
        let author = Keys::generate();
        let known = EventBuilder::text_note("already verified")
            .sign_with_keys(&author)
            .unwrap();
        let valid = EventBuilder::text_note("new legitimate message")
            .sign_with_keys(&author)
            .unwrap();
        let mut forged_content = known.clone();
        forged_content.content = "forged content".into();
        let mut forged_author = known.clone();
        forged_author.pubkey = Keys::generate().public_key();
        let mut forged_signature = known.clone();
        forged_signature.sig = valid.sig;
        for forged in [&forged_content, &forged_author, &forged_signature] {
            assert!(forged.verify().is_err());
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let expected_id = valid.id;
        let relay = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(socket).await.unwrap();
            while let Some(message) = socket.next().await {
                let text = match message.unwrap() {
                    Message::Text(text) => text,
                    Message::Ping(payload) => {
                        socket.send(Message::Pong(payload)).await.unwrap();
                        continue;
                    }
                    Message::Close(_) => break,
                    _ => continue,
                };
                let message: serde_json::Value = serde_json::from_str(&text).unwrap();
                if message[0] != "REQ" {
                    continue;
                }
                let subscription = message[1].as_str().unwrap();
                for event in [&forged_content, &forged_author, &forged_signature, &valid] {
                    socket
                        .send(Message::Text(
                            serde_json::json!(["EVENT", subscription, event]).to_string(),
                        ))
                        .await
                        .unwrap();
                }
                socket
                    .send(Message::Text(
                        serde_json::json!(["EOSE", subscription]).to_string(),
                    ))
                    .await
                    .unwrap();
            }
        });
        let client = Client::new(Keys::generate());
        client.database().save_event(&known).await.unwrap();
        client.add_relay(&url).await.unwrap();
        client
            .try_connect_relay(&url, std::time::Duration::from_secs(3))
            .await
            .unwrap();
        let events = client
            .fetch_events(
                Filter::new().kind(Kind::TextNote),
                std::time::Duration::from_secs(5),
            )
            .await
            .unwrap();
        assert_eq!(
            events.len(),
            1,
            "forged events must never reach SDK consumers"
        );
        assert_eq!(events.iter().next().unwrap().id, expected_id);
        client.disconnect().await;
        relay.abort();
    };
    tokio::time::timeout(std::time::Duration::from_secs(15), test)
        .await
        .expect("local relay test timed out");
}
