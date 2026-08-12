use hyper::{Response, StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub(super) struct RpcRequest {
    pub method: String,
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub(super) struct RpcResponse {
    pub result: Option<serde_json::Value>,
    pub error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
pub(super) struct RpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// Simple TTL cache for read-only RPC responses.
pub(super) struct ResponseCache {
    entries: tokio::sync::RwLock<
        std::collections::HashMap<String, (std::time::Instant, serde_json::Value)>,
    >,
    ttl: std::time::Duration,
}

impl ResponseCache {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            entries: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            ttl: std::time::Duration::from_secs(ttl_secs),
        }
    }

    pub async fn get(&self, key: &str) -> Option<serde_json::Value> {
        let entries = self.entries.read().await;
        if let Some((ts, value)) = entries.get(key) {
            if ts.elapsed() < self.ttl {
                return Some(value.clone());
            }
        }
        None
    }

    pub async fn set(&self, key: String, value: serde_json::Value) {
        let mut entries = self.entries.write().await;
        entries.insert(key, (std::time::Instant::now(), value));
    }
}

/// Build a JSON HTTP response without unwrap. Falls back to a plain 500 if builder fails.
pub(super) fn json_response(status: StatusCode, body: &[u8]) -> Response<hyper::Body> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(hyper::Body::from(body.to_vec()))
        .unwrap_or_else(|_| {
            Response::new(hyper::Body::from(
                r#"{"error":{"code":500,"message":"Internal error"}}"#,
            ))
        })
}

/// Parse a Set-Cookie header value, returning a default if parsing fails.
pub(super) fn cookie_header(value: &str) -> hyper::header::HeaderValue {
    value
        .parse()
        .unwrap_or_else(|_| hyper::header::HeaderValue::from_static(""))
}
