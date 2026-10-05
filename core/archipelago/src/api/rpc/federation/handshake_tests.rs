//! Exercise encrypted replies through a relay configured in the UI only.
use crate::federation::pending::{self, PendingState};
use futures_util::{SinkExt, StreamExt};
use nostr_sdk::prelude::{nip44, Event, Keys};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn managed_relay_receives_approval_rejection_and_cancellation() {
    for (operation, accepted) in [
        ("approve", true),
        ("reject", true),
        ("cancel", true),
        ("approve", false),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_url = format!("ws://{}", listener.local_addr().unwrap());
        let relay = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(socket).await.unwrap();
            while let Some(Ok(message)) = ws.next().await {
                if !message.is_text() {
                    continue;
                }
                let value: serde_json::Value =
                    serde_json::from_str(message.to_text().unwrap()).unwrap();
                if value[0] != "EVENT" {
                    continue;
                }
                let event: Event = serde_json::from_value(value[1].clone()).unwrap();
                event.verify().unwrap();
                ws.send(tokio_tungstenite::tungstenite::Message::Text(
                    serde_json::json!([
                        "OK",
                        event.id.to_hex(),
                        accepted,
                        "blocked: fixture rejection"
                    ])
                    .to_string(),
                ))
                .await
                .unwrap();
                return event;
            }
            panic!("relay closed without a signed event");
        });
        let mut config = crate::config::Config::default();
        config.data_dir = tmp.path().to_path_buf();
        config.nostr_relays.clear();
        config.nostr_tor_proxy = None;
        crate::nostr_relays::save_relays(
            tmp.path(),
            &crate::nostr_relays::RelayStore {
                relays: vec![crate::nostr_relays::RelayConfig {
                    url: relay_url,
                    enabled: true,
                    added_at: chrono::Utc::now().to_rfc3339(),
                }],
            },
        )
        .await
        .unwrap();
        let sender = Keys::parse(&"11".repeat(32)).unwrap();
        let recipient = Keys::parse(&"22".repeat(32)).unwrap();
        let identity_dir = tmp.path().join("identity");
        tokio::fs::create_dir_all(&identity_dir).await.unwrap();
        tokio::fs::write(identity_dir.join("nostr_secret"), "11".repeat(32))
            .await
            .unwrap();
        let state = Arc::new(crate::state::StateManager::new());
        state
            .mutate_data(|data| {
                data.server_info.pubkey = "33".repeat(32);
                data.server_info.tor_address = Some(format!("{}.onion", "a".repeat(56)));
            })
            .await;
        let handler = crate::api::rpc::RpcHandler::new(
            config,
            state,
            Arc::new(crate::monitoring::MetricsStore::new()),
            crate::session::SessionStore::new_for_tests(tmp.path().join("sessions.json")),
            None,
            None,
        )
        .await
        .unwrap();
        let row = if operation == "cancel" {
            pending::insert_outbound(
                tmp.path(),
                recipient.public_key().to_hex(),
                String::new(),
                String::new(),
                None,
                None,
            )
            .await
            .unwrap()
        } else {
            pending::insert_inbound(
                tmp.path(),
                recipient.public_key().to_hex(),
                String::new(),
                String::new(),
                None,
                None,
            )
            .await
            .unwrap()
            .unwrap()
        };
        let params = Some(serde_json::json!({"id": row.id, "notify": true}));
        let action = async {
            match operation {
                "approve" => handler.handle_federation_approve_request(params).await,
                "reject" => handler.handle_federation_reject_request(params).await,
                _ => handler.handle_federation_cancel_request(params).await,
            }
        };
        let outcome = tokio::time::timeout(Duration::from_secs(20), action)
            .await
            .unwrap();
        assert_eq!(outcome.is_ok(), accepted);
        let event = tokio::time::timeout(Duration::from_secs(5), relay)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.pubkey, sender.public_key());
        let plaintext =
            nip44::decrypt(recipient.secret_key(), &event.pubkey, &event.content).unwrap();
        let message: serde_json::Value = serde_json::from_str(&plaintext).unwrap();
        let expected = match operation {
            "approve" => "peer-invite",
            "reject" => "peer-reject",
            _ => "peer-cancel",
        };
        assert_eq!(message["type"], expected);
        let saved = pending::find_by_id(tmp.path(), &row.id).await.unwrap();
        if !accepted {
            assert_eq!(saved.unwrap().state, PendingState::Pending);
            assert!(crate::federation::load_nodes(tmp.path())
                .await
                .unwrap()
                .is_empty());
            continue;
        }
        match operation {
            "approve" => {
                assert_eq!(saved.unwrap().state, PendingState::Approved);
                let invite =
                    crate::federation::parse_invite(message["invite_code"].as_str().unwrap())
                        .unwrap();
                assert_eq!(invite.trust_level, crate::federation::TrustLevel::Observer);
                assert!(!event.content.contains(".onion"));
            }
            "reject" => assert_eq!(saved.unwrap().state, PendingState::Rejected),
            _ => assert!(saved.is_none()),
        }
    }
}
