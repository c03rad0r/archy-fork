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
        let relative = file_purchase_in_files(dir.path(), "../name #?.bin", mime, b"paid")
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
    assert!(
        file_purchase_in_files(dir.path(), "name", "text/plain", b"bytes")
            .await
            .is_err()
    );
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
