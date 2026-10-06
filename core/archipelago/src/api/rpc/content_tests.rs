use super::*;

#[test]
fn first_and_cached_paid_downloads_have_the_same_client_payload_contract() {
    use base64::Engine;
    for paid in [0, 1] {
        let response = paid_content_response(&[0, 255, 123], "application/octet-stream", paid);
        assert_eq!(response["data"], response["data_base64"]);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(response["data"].as_str().unwrap())
                .unwrap(),
            [0, 255, 123]
        );
        assert_eq!(response["size"], 3);
        assert_eq!(response["size_bytes"], 3);
        assert_eq!(response["paid_sats"], paid);
        assert_eq!(response["owned"], true);
    }
}

#[tokio::test]
async fn files_copy_routes_media_and_sanitizes_the_filename() {
    let dir = tempfile::tempdir().unwrap();
    tokio::fs::create_dir(dir.path().join("filebrowser"))
        .await
        .unwrap();
    for (mime, folder) in [
        ("image/png", "Photos"),
        ("video/mp4", "Photos"),
        ("audio/mpeg", "Music"),
        ("text/plain", "Documents"),
    ] {
        crate::content_owned::record_purchase(
            dir.path(),
            "seller.onion",
            "id",
            "../name #?.bin",
            mime,
            b"paid",
            1,
            "cashu",
            "now",
        )
        .await
        .unwrap();
        let item = crate::content_owned::list_owned_checked(dir.path())
            .await
            .unwrap()
            .remove(0);
        let relative = file_cached_purchase_in_files(dir.path(), &item)
            .await
            .unwrap();
        assert!(relative.starts_with(&format!("{folder}/name #?")));
        assert_eq!(
            tokio::fs::read(dir.path().join("filebrowser").join(relative))
                .await
                .unwrap(),
            b"paid"
        );
    }
}

#[tokio::test]
async fn unavailable_files_storage_is_reported_without_creating_a_fake_installation() {
    let dir = tempfile::tempdir().unwrap();
    crate::content_owned::record_purchase(
        dir.path(),
        "seller.onion",
        "id",
        "name",
        "text/plain",
        b"bytes",
        1,
        "cashu",
        "now",
    )
    .await
    .unwrap();
    let item = crate::content_owned::list_owned_checked(dir.path())
        .await
        .unwrap()
        .remove(0);
    assert!(file_cached_purchase_in_files(dir.path(), &item)
        .await
        .is_err());
    assert!(!dir.path().join("filebrowser").exists());
}

#[test]
fn seller_errors_are_bounded_printable_and_identified_as_peer_text() {
    let status = reqwest::StatusCode::SERVICE_UNAVAILABLE;
    let message = seller_error_message(status, r#"{"error":"Cannot read file\n\u0000"}"#);
    assert!(message.starts_with("Seller response (503"));
    assert!(message.ends_with("Cannot read file"));
    assert!(!message.contains('\n') && !message.contains('\0'));
    let body = serde_json::json!({"error": "é".repeat(1000)}).to_string();
    assert!(seller_error_message(status, &body).chars().count() < 300);
    for body in ["not JSON", r#"{"error": 7}"#, r#"{"error":"   "}"#] {
        assert_eq!(
            seller_error_message(status, body),
            "Peer returned an error (503 Service Unavailable)."
        );
    }
}

#[tokio::test]
async fn known_purchase_never_becomes_a_new_spend_when_cache_or_index_is_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        existing_paid_content(dir.path(), "seller.onion", "id", None, false)
            .await
            .unwrap()
            .is_none()
    );
    crate::content_owned::record_purchase(
        dir.path(),
        "seller.onion",
        "id",
        "file.txt",
        "text/plain",
        b"paid",
        1,
        "cashu",
        "now",
    )
    .await
    .unwrap();
    for (id, filename) in [("id", None), ("duplicate-id", Some("/file.txt"))] {
        let cached = existing_paid_content(dir.path(), "seller.onion", id, filename, false)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(cached["paid_sats"], 0);
        assert_eq!(cached["already_owned"], true);
        assert_eq!(cached["data"], "cGFpZA==");
    }
    assert!(
        existing_paid_content(dir.path(), "different.onion", "id", None, false)
            .await
            .unwrap()
            .is_none()
    );
    tokio::fs::remove_file(dir.path().join("purchased-content/seller.onion/id"))
        .await
        .unwrap();
    assert!(
        existing_paid_content(dir.path(), "seller.onion", "id", None, false)
            .await
            .unwrap_err()
            .to_string()
            .contains("No new payment")
    );
    tokio::fs::write(dir.path().join("purchased-content/owned.json"), b"damaged")
        .await
        .unwrap();
    assert!(
        existing_paid_content(dir.path(), "seller.onion", "other-id", None, false)
            .await
            .unwrap_err()
            .to_string()
            .contains("no new payment")
    );
    assert_eq!(
        tokio::fs::read(dir.path().join("purchased-content/owned.json"))
            .await
            .unwrap(),
        b"damaged"
    );
}
