use super::*;
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Response, Server,
};
use std::{
    collections::VecDeque,
    convert::Infallible,
    sync::{Arc, Mutex},
};

struct FilesApi {
    url: String,
    seen: Arc<Mutex<Vec<(String, String, Vec<u8>)>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for FilesApi {
    fn drop(&mut self) {
        self.task.abort();
    }
}
fn files_api(statuses: Vec<u16>) -> FilesApi {
    let statuses = Arc::new(Mutex::new(VecDeque::from(statuses)));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let history = seen.clone();
    let server = Server::bind(&([127, 0, 0, 1], 0).into());
    let address = server.local_addr();
    let service = make_service_fn(move |_| {
        let statuses = statuses.clone();
        let seen = history.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |request: hyper::Request<Body>| {
                let statuses = statuses.clone();
                let seen = seen.clone();
                async move {
                    assert_eq!(request.headers().get("X-Auth").unwrap(), "test-session");
                    let method = request.method().to_string();
                    let uri = request.uri().to_string();
                    let body = hyper::body::to_bytes(request.into_body())
                        .await
                        .unwrap()
                        .to_vec();
                    seen.lock().unwrap().push((method, uri, body));
                    let status = statuses
                        .lock()
                        .unwrap()
                        .pop_front()
                        .expect("unexpected extra Files request");
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(status)
                            .body(Body::empty())
                            .unwrap(),
                    )
                }
            }))
        }
    });
    FilesApi {
        url: format!("http://{address}"),
        seen,
        task: tokio::spawn(async move {
            server.serve(service).await.unwrap();
        }),
    }
}

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
async fn files_copy_uses_authenticated_api_and_preserves_existing_names() {
    let api = files_api(vec![200, 409, 200]);
    let client = reqwest::Client::new();
    let path = file_purchase_in_files(
        &client,
        &api.url,
        "test-session",
        "../my #file?.txt",
        "text/plain",
        b"paid bytes",
    )
    .await
    .unwrap();
    assert_eq!(path, "Documents/my #file? (2).txt");
    let seen = api.seen.lock().unwrap();
    assert_eq!(seen[0].0, "GET");
    assert_eq!(seen[0].1, "/api/resources/Documents/");
    assert_eq!(seen.len(), 3);
    for (_, uri, body) in &seen[1..] {
        assert!(uri.contains("override=false"));
        assert!(uri.contains("%23file%3F"));
        assert!(!uri.contains("../"));
        assert_eq!(body, b"paid bytes");
    }
}

#[tokio::test]
async fn files_copy_creates_missing_media_folder() {
    for (mime, folder) in [
        ("image/png", "Photos"),
        ("video/mp4", "Photos"),
        ("audio/ogg", "Music"),
    ] {
        let api = files_api(vec![404, 200, 200]);
        let path = file_purchase_in_files(
            &reqwest::Client::new(),
            &api.url,
            "test-session",
            "file",
            mime,
            b"bytes",
        )
        .await
        .unwrap();
        assert_eq!(path, format!("{folder}/file"));
        let seen = api.seen.lock().unwrap();
        assert_eq!(seen[1].0, "POST");
        assert!(seen[1].1.ends_with('/'));
        assert!(seen[1].2.is_empty());
        assert_eq!(seen[2].2, b"bytes");
    }
}

#[tokio::test]
async fn files_copy_fails_without_overwriting_or_claiming_success_on_errors() {
    for statuses in [
        vec![401],
        vec![503],
        vec![404, 500],
        vec![200, 507],
        vec![200, 403],
    ] {
        let expected = statuses.len();
        let api = files_api(statuses);
        assert!(file_purchase_in_files(
            &reqwest::Client::new(),
            &api.url,
            "test-session",
            "file.txt",
            "text/plain",
            b"bytes"
        )
        .await
        .is_err());
        assert_eq!(api.seen.lock().unwrap().len(), expected);
    }
}
