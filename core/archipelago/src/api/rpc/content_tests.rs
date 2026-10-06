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

#[tokio::test]
async fn inline_download_limits_known_and_chunked_bodies_without_draining_them() {
    use futures_util::StreamExt;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let response: reqwest::Response = hyper::Response::new("small").into();
    assert!(bounded_content_bytes(response, 4).await.is_err());
    let response: reqwest::Response = hyper::Response::new("small").into();
    assert_eq!(bounded_content_bytes(response, 5).await.unwrap(), b"small");
    let consumed = Arc::new(AtomicUsize::new(0));
    let counter = consumed.clone();
    let chunks = futures_util::stream::iter(0..1000).map(move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"abc"))
    });
    let body = reqwest::Body::wrap_stream(chunks);
    let response: reqwest::Response = hyper::Response::new(body).into();
    assert!(bounded_content_bytes(response, 4).await.is_err());
    assert_eq!(consumed.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn onchain_delivery_streams_to_owned_cache_and_preserves_incomplete_recovery() {
    use tokio::io::AsyncReadExt;
    let dir = tempfile::tempdir().unwrap();
    let response: reqwest::Response = hyper::Response::builder()
        .header("content-length", "2097152")
        .body(hyper::Body::from(vec![71u8; 2 * 1024 * 1024]))
        .unwrap()
        .into();
    let item = cache_peer_response(dir.path(), "seller.onion", "film", "film.mp4", "video/mp4", 1, "onchain", response).await.unwrap();
    assert!(item.download_complete);
    assert_eq!(item.ecash_backend, "onchain");
    let (_, mut file) = crate::content_owned::open_owned(dir.path(), "seller.onion", "film").await.unwrap().unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, vec![71u8; 2 * 1024 * 1024]);
    let reply = cached_purchase_response(dir.path(), "seller.onion", "film", true, 0).await.unwrap();
    assert_eq!(reply["owned"], true);
    assert!(reply.get("data").is_none());

    let truncated: reqwest::Response = hyper::Response::builder()
        .header("content-length", "100")
        .body(hyper::Body::from("short"))
        .unwrap()
        .into();
    assert!(cache_peer_response(dir.path(), "seller.onion", "interrupted", "film.mp4", "video/mp4", 1, "onchain", truncated).await.is_err());
    let entries = crate::content_owned::list_owned_checked(dir.path()).await.unwrap();
    assert!(!entries.iter().find(|item| item.content_id == "interrupted").unwrap().download_complete);
    assert!(existing_paid_content(dir.path(), "seller.onion", "interrupted", None, true).await.is_err());
}
