use super::build_response;
use crate::config::Config;
use crate::network::dwn_store::DwnStore;
use anyhow::Result;
use hyper::{Response, StatusCode};

use super::ApiHandler;

impl ApiHandler {
    /// DWN health endpoint — returns store stats.
    pub(super) async fn handle_dwn_health(config: &Config) -> Result<Response<hyper::Body>> {
        match DwnStore::new(&config.data_dir).await {
            Ok(store) => {
                let stats = store
                    .stats()
                    .await
                    .unwrap_or(crate::network::dwn_store::StoreStats {
                        message_count: 0,
                        protocol_count: 0,
                        total_bytes: 0,
                    });
                let body = serde_json::json!({
                    "status": "ok",
                    "message_count": stats.message_count,
                    "protocol_count": stats.protocol_count,
                    "total_bytes": stats.total_bytes,
                });
                Ok(Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", "application/json")
                    .body(hyper::Body::from(body.to_string()))
                    .unwrap())
            }
            Err(_) => Ok(build_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "application/json",
                hyper::Body::from(r#"{"status":"unavailable"}"#),
            )),
        }
    }

    /// DWN message processing endpoint — handles RecordsWrite, RecordsQuery, RecordsRead, RecordsDelete.
    /// Supports batch processing: all messages in the array are processed.
    pub(super) async fn handle_dwn_message(
        body: hyper::body::Bytes,
        config: &Config,
    ) -> Result<Response<hyper::Body>> {
        let request: serde_json::Value = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => {
                let err = serde_json::json!({"error": format!("Invalid JSON: {}", e)});
                return Ok(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header("Content-Type", "application/json")
                    .body(hyper::Body::from(err.to_string()))
                    .unwrap());
            }
        };

        // Collect all messages to process
        let messages: Vec<serde_json::Value> = if request.get("message").is_some() {
            vec![request["message"].clone()]
        } else if let Some(msgs) = request["messages"].as_array() {
            msgs.clone()
        } else {
            vec![serde_json::Value::Null]
        };

        let store = DwnStore::new(&config.data_dir).await?;
        let mut results = Vec::new();

        for message in &messages {
            let interface = message["descriptor"]["interface"].as_str().unwrap_or("");
            let method = message["descriptor"]["method"].as_str().unwrap_or("");

            let result = match (interface, method) {
                ("Records", "Write") => {
                    let author = message["author"].as_str().unwrap_or("unknown");
                    let protocol = message["descriptor"]["protocol"].as_str();
                    let schema = message["descriptor"]["schema"].as_str();
                    let data_format = message["descriptor"]["dataFormat"].as_str();
                    let data = message.get("data").cloned();
                    // Deduplicate: check if recordId already exists
                    if let Some(record_id) = message["recordId"].as_str() {
                        if store.read_message(record_id).await.ok().flatten().is_some() {
                            serde_json::json!({"status": {"code": 200, "detail": "Already exists"}})
                        } else {
                            match store
                                .write_message(author, protocol, schema, data_format, data)
                                .await
                            {
                                Ok(msg) => {
                                    serde_json::json!({"status": {"code": 202}, "entry": msg})
                                }
                                Err(e) => {
                                    serde_json::json!({"status": {"code": 500, "detail": e.to_string()}})
                                }
                            }
                        }
                    } else {
                        match store
                            .write_message(author, protocol, schema, data_format, data)
                            .await
                        {
                            Ok(msg) => serde_json::json!({"status": {"code": 202}, "entry": msg}),
                            Err(e) => {
                                serde_json::json!({"status": {"code": 500, "detail": e.to_string()}})
                            }
                        }
                    }
                }
                ("Records", "Query") => {
                    let query = crate::network::dwn_store::MessageQuery {
                        protocol: message["descriptor"]["filter"]["protocol"]
                            .as_str()
                            .map(|s| s.to_string()),
                        schema: message["descriptor"]["filter"]["schema"]
                            .as_str()
                            .map(|s| s.to_string()),
                        author: message["descriptor"]["filter"]["author"]
                            .as_str()
                            .map(|s| s.to_string()),
                        date_from: message["descriptor"]["filter"]["dateFrom"]
                            .as_str()
                            .map(|s| s.to_string()),
                        date_to: message["descriptor"]["filter"]["dateTo"]
                            .as_str()
                            .map(|s| s.to_string()),
                        limit: message["descriptor"]["filter"]["limit"]
                            .as_u64()
                            .map(|n| n as usize),
                    };
                    match store.query_messages(&query).await {
                        Ok(messages) => {
                            serde_json::json!({"status": {"code": 200}, "entries": messages})
                        }
                        Err(e) => {
                            serde_json::json!({"status": {"code": 500, "detail": e.to_string()}})
                        }
                    }
                }
                ("Records", "Read") => {
                    let record_id = message["descriptor"]["recordId"].as_str().unwrap_or("");
                    match store.read_message(record_id).await {
                        Ok(Some(msg)) => {
                            serde_json::json!({"status": {"code": 200}, "entry": msg})
                        }
                        Ok(None) => {
                            serde_json::json!({"status": {"code": 404, "detail": "Record not found"}})
                        }
                        Err(e) => {
                            serde_json::json!({"status": {"code": 500, "detail": e.to_string()}})
                        }
                    }
                }
                ("Records", "Delete") => {
                    let record_id = message["descriptor"]["recordId"].as_str().unwrap_or("");
                    match store.delete_message(record_id).await {
                        Ok(true) => serde_json::json!({"status": {"code": 200}}),
                        Ok(false) => {
                            serde_json::json!({"status": {"code": 404, "detail": "Record not found"}})
                        }
                        Err(e) => {
                            serde_json::json!({"status": {"code": 500, "detail": e.to_string()}})
                        }
                    }
                }
                _ => {
                    serde_json::json!({"status": {"code": 400, "detail": format!("Unknown method: {}.{}", interface, method)}})
                }
            };

            results.push(result);
        }

        // Return single result for single message, array for batch
        let (response_body, http_status) = if results.len() == 1 {
            let result = &results[0];
            let status_code = result["status"]["code"].as_u64().unwrap_or(200);
            let http_status = match status_code {
                202 => StatusCode::ACCEPTED,
                400 => StatusCode::BAD_REQUEST,
                404 => StatusCode::NOT_FOUND,
                500 => StatusCode::INTERNAL_SERVER_ERROR,
                _ => StatusCode::OK,
            };
            (result.to_string(), http_status)
        } else {
            (
                serde_json::json!({"replies": results}).to_string(),
                StatusCode::OK,
            )
        };

        Ok(build_response(
            http_status,
            "application/json",
            hyper::Body::from(response_body),
        ))
    }
}
