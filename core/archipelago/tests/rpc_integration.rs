//! Integration test scaffolding for the Archipelago RPC server.
//!
//! Starts the backend on a random port with a temp data dir,
//! sends RPC requests, and tears down after each test.
//!
//! Run on dev server: `cargo test --test rpc_integration`

use std::net::TcpListener;
use std::path::PathBuf;
use std::time::Duration;

/// Find an available TCP port by binding to port 0.
fn find_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to port 0");
    listener.local_addr().unwrap().port()
}

/// Helper to send an RPC request and get the JSON response.
async fn rpc_call(
    port: u16,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let body = serde_json::json!({
        "method": method,
        "params": params,
    });

    let resp = client
        .post(format!("http://127.0.0.1:{}/rpc/v1", port))
        .json(&body)
        .send()
        .await?;

    let json: serde_json::Value = resp.json().await?;
    Ok(json)
}

/// Start the server in the background, returning the port and a handle to shut it down.
async fn start_test_server() -> (u16, PathBuf, tokio::task::JoinHandle<()>) {
    let port = find_free_port();
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let data_dir = temp_dir.path().to_path_buf();

    // Create required subdirectories
    std::fs::create_dir_all(data_dir.join("identity")).unwrap();
    std::fs::create_dir_all(data_dir.join("users")).unwrap();

    // Write a minimal config
    let config_path = data_dir.join("config.toml");
    let config_content = format!(
        r#"
data_dir = "{}"
bind_host = "127.0.0.1"
bind_port = {}
log_level = "warn"
host_ip = "127.0.0.1"
dev_mode = true
container_runtime = "podman"
port_offset = 0
nostr_discovery_enabled = false
"#,
        data_dir.display(),
        port
    );
    std::fs::write(&config_path, config_content).unwrap();

    // Set env var so Config::load() finds our config
    std::env::set_var("ARCHIPELAGO_CONFIG", config_path.to_str().unwrap());
    std::env::set_var("ARCHIPELAGO_DATA_DIR", data_dir.to_str().unwrap());

    let server_data_dir = data_dir.clone();
    let handle = tokio::spawn(async move {
        // Import and start the server
        // For now, we'll use a simple HTTP listener that responds to echo
        // This scaffolding will be replaced with the actual server once
        // the Server::new() constructor supports test configurations
        use hyper::service::{make_service_fn, service_fn};
        use hyper::{Body, Request, Response, Server, StatusCode};

        let addr = ([127, 0, 0, 1], port).into();

        let make_svc = make_service_fn(move |_| {
            let _data_dir = server_data_dir.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn(move |req: Request<Body>| async move {
                    if req.uri().path() == "/rpc/v1" {
                        let body_bytes = hyper::body::to_bytes(req.into_body()).await.unwrap();
                        let request: serde_json::Value =
                            serde_json::from_slice(&body_bytes).unwrap_or_default();

                        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

                        let response = match method {
                            "server.echo" => {
                                let message = request
                                    .get("params")
                                    .and_then(|p| p.get("message"))
                                    .and_then(|m| m.as_str())
                                    .unwrap_or("");
                                serde_json::json!({ "result": message })
                            }
                            "health" => {
                                serde_json::json!({ "result": "ok" })
                            }
                            _ => {
                                serde_json::json!({
                                    "error": {
                                        "code": -32601,
                                        "message": format!("Method not found: {}", method)
                                    }
                                })
                            }
                        };

                        Ok::<_, hyper::Error>(
                            Response::builder()
                                .status(StatusCode::OK)
                                .header("Content-Type", "application/json")
                                .body(Body::from(serde_json::to_string(&response).unwrap()))
                                .unwrap(),
                        )
                    } else if req.uri().path() == "/health" {
                        Ok(Response::new(Body::from("OK")))
                    } else {
                        Ok(Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(Body::from("Not Found"))
                            .unwrap())
                    }
                }))
            }
        });

        Server::bind(&addr)
            .serve(make_svc)
            .await
            .expect("Test server failed");
    });

    // Wait for server to be ready
    for _ in 0..50 {
        if let Ok(resp) = reqwest::get(format!("http://127.0.0.1:{}/health", port)).await {
            if resp.status().is_success() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    (port, data_dir, handle)
}

#[tokio::test]
async fn test_echo_rpc() {
    let (port, _data_dir, handle) = start_test_server().await;

    let response = rpc_call(
        port,
        "server.echo",
        serde_json::json!({ "message": "hello integration test" }),
    )
    .await
    .expect("RPC call failed");

    assert_eq!(
        response.get("result").and_then(|r| r.as_str()),
        Some("hello integration test")
    );

    // Clean up
    handle.abort();
}

#[tokio::test]
async fn test_health_endpoint() {
    let (port, _data_dir, handle) = start_test_server().await;

    let resp = reqwest::get(format!("http://127.0.0.1:{}/health", port))
        .await
        .expect("Health check failed");

    assert!(resp.status().is_success());
    let text = resp.text().await.unwrap();
    assert_eq!(text, "OK");

    handle.abort();
}

#[tokio::test]
async fn test_unknown_method_returns_error() {
    let (port, _data_dir, handle) = start_test_server().await;

    let response = rpc_call(port, "nonexistent.method", serde_json::json!({}))
        .await
        .expect("RPC call failed");

    assert!(response.get("error").is_some());
    let error = response.get("error").unwrap();
    assert_eq!(error.get("code").and_then(|c| c.as_i64()), Some(-32601));

    handle.abort();
}
