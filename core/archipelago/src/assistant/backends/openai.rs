//! Explicit OpenAI API selection using the shared tool loop and egress policy.
//! Keys stay node-side; no redirects, automatic retries, or provider fallback.
use super::{Backend, BackendTurn};
use crate::assistant::{
    egress::{self, EgressVerdict},
    tools::{ChatMessage, ToolCall, ToolDef},
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{path::PathBuf, time::Duration};

const URL: &str = "https://api.openai.com/v1/chat/completions";
const RESPONSE_LIMIT: usize = 2 * 1024 * 1024;
pub struct OpenaiBackend {
    data_dir: PathBuf,
    model: String,
}
impl OpenaiBackend {
    pub fn new(data_dir: PathBuf, model: String) -> Self {
        Self { data_dir, model }
    }
    async fn send_at(
        &self,
        url: &str,
        system: &str,
        tools: &[ToolDef],
        history: &[ChatMessage],
    ) -> Result<BackendTurn> {
        let key = tokio::fs::read_to_string(self.data_dir.join("secrets/openai-api-key"))
            .await
            .map_err(|_| {
                anyhow::anyhow!("OpenAI API key is not configured. Open AI connection settings.")
            })?;
        anyhow::ensure!(!key.trim().is_empty(), "OpenAI API key is not configured");
        anyhow::ensure!(
            !self.model.is_empty(),
            "Choose an OpenAI model in AI connection settings"
        );
        let mut messages = vec![json!({"role": "system", "content": system})];
        messages.extend(history.iter().flat_map(super::routstr::message_to_wire));
        let mut body = json!({"model": self.model, "messages": messages, "stream": false,
            "store": false, "max_completion_tokens": 2048, "n": 1});
        if !tools.is_empty() {
            body["tools"] = json!(tools.iter().map(|tool| json!({"type": "function", "function": {
                "name": tool.name, "description": tool.description, "parameters": tool.parameters,
            }})).collect::<Vec<_>>());
            body["parallel_tool_calls"] = json!(false);
        }
        let context = egress::EgressContext::from_turn(
            history,
            &tools.iter().map(|tool| tool.name).collect::<Vec<_>>(),
            &self.data_dir.join("secrets"),
        )
        .await;
        match egress::screen_outbound(&body.to_string(), &context) {
            EgressVerdict::Allow => {}
            EgressVerdict::Truncate(value) => {
                body = serde_json::from_str(&value)
                    .context("Could not apply outbound privacy filter")?;
            }
            EgressVerdict::BlockFallBackLocal => {
                crate::assistant::global_counters().note_blocked_egress();
                anyhow::bail!("This message contains private key or recovery material and was not sent to OpenAI");
            }
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .connect_timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let mut response = client.post(url).bearer_auth(key.trim()).json(&body).send().await
            .map_err(|_| anyhow::anyhow!("OpenAI is temporarily unreachable. Your request was not retried automatically."))?;
        if !response.status().is_success() {
            anyhow::bail!("{}", error_message(response.status().as_u16()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .context("OpenAI response interrupted")?
        {
            anyhow::ensure!(
                bytes.len().saturating_add(chunk.len()) <= RESPONSE_LIMIT,
                "OpenAI response exceeded the size limit"
            );
            bytes.extend_from_slice(&chunk);
        }
        parse_response(
            &serde_json::from_slice(&bytes).context("OpenAI returned an invalid response")?,
        )
    }
}
#[async_trait]
impl Backend for OpenaiBackend {
    async fn send(
        &self,
        system: &str,
        tools: &[ToolDef],
        history: &[ChatMessage],
    ) -> Result<BackendTurn> {
        self.send_at(URL, system, tools, history).await
    }
}
fn error_message(status: u16) -> &'static str {
    match status {
        401 | 403 => "OpenAI rejected the API key or project access. Check AI connection settings.",
        404 => "This OpenAI model is unavailable for your account. Choose another model in AI connection settings.",
        429 => "OpenAI usage or rate limit reached. Check your API billing and retry later.",
        500..=599 => "OpenAI is temporarily unavailable. Retry later.",
        _ => "OpenAI rejected the request. Check the selected model and retry.",
    }
}
fn parse_response(value: &Value) -> Result<BackendTurn> {
    let choice = value["choices"]
        .as_array()
        .and_then(|items| items.first())
        .context("OpenAI returned no answer")?;
    anyhow::ensure!(
        choice["finish_reason"] != "length",
        "OpenAI reached the response limit. Try a shorter request."
    );
    let message = &choice["message"];
    if let Some(calls) = message["tool_calls"]
        .as_array()
        .filter(|calls| !calls.is_empty())
    {
        let mut parsed = Vec::new();
        for call in calls {
            anyhow::ensure!(
                call["type"] == "function",
                "Unsupported OpenAI tool response"
            );
            let id = call["id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .context("Missing OpenAI tool call ID")?;
            let name = call["function"]["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .context("Missing OpenAI tool name")?;
            let arguments: Value = serde_json::from_str(
                call["function"]["arguments"]
                    .as_str()
                    .context("Invalid OpenAI tool arguments")?,
            )
            .context("Invalid OpenAI tool arguments")?;
            anyhow::ensure!(
                arguments.is_object()
                    && !parsed.iter().any(|previous: &ToolCall| previous.id == id),
                "Invalid OpenAI tool call"
            );
            parsed.push(ToolCall {
                id: id.into(),
                name: name.into(),
                arguments,
            });
        }
        return Ok(BackendTurn::ToolCalls(parsed));
    }
    let text = message["content"]
        .as_str()
        .or_else(|| message["refusal"].as_str())
        .filter(|text| !text.trim().is_empty())
        .context("OpenAI returned no text; check model compatibility")?;
    Ok(BackendTurn::Text(text.into()))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::tools::{Role, ToolResult};
    #[test]
    fn parses_text_and_rejects_incomplete_or_malformed_tool_calls() {
        assert!(
            matches!(parse_response(&json!({"choices":[{"message":{"content":"hello"}}]})).unwrap(), BackendTurn::Text(text) if text == "hello")
        );
        let valid = json!({"choices":[{"message":{"tool_calls":[{"id":"call_1","type":"function","function":{"name":"status","arguments":"{\"count\":1}"}}]}}]});
        assert!(
            matches!(parse_response(&valid).unwrap(), BackendTurn::ToolCalls(calls) if calls[0].arguments["count"] == 1)
        );
        for args in ["{", "null", "[]"] {
            let mut invalid = valid.clone();
            invalid["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"] =
                json!(args);
            assert!(parse_response(&invalid).is_err());
        }
        assert!(parse_response(
            &json!({"choices":[{"finish_reason":"length","message":{"content":"partial"}}]})
        )
        .is_err());
        assert!(parse_response(&json!({"choices":[]})).is_err());
    }
    #[test]
    fn errors_distinguish_credentials_limits_and_outages_without_raw_provider_data() {
        assert!(error_message(401).contains("API key"));
        assert!(error_message(429).contains("limit"));
        assert!(!error_message(503).contains("key"));
        let wire = super::super::routstr::message_to_wire(&ChatMessage {
            role: Role::Tool,
            text: None,
            tool_calls: vec![],
            tool_results: vec![ToolResult {
                call_id: "call_1".into(),
                content: "result".into(),
                is_error: false,
            }],
        });
        assert_eq!(wire[0]["tool_call_id"], "call_1");
    }
    #[tokio::test]
    async fn real_http_adapter_sends_private_key_only_in_header_and_never_follows_redirect() {
        use hyper::{
            service::{make_service_fn, service_fn},
            Body, Response, Server,
        };
        use std::sync::{Arc, Mutex};
        let captured = Arc::new(Mutex::new(Vec::new()));
        let capture = captured.clone();
        let server = Server::bind(&([127, 0, 0, 1], 0).into()).serve(make_service_fn(move |_| {
            let capture = capture.clone();
            async move {
                Ok::<_, hyper::Error>(service_fn(move |request: hyper::Request<Body>| {
                    let capture = capture.clone();
                    async move {
                        let (parts, body) = request.into_parts();
                        let body = hyper::body::to_bytes(body).await?;
                        capture.lock().unwrap().push((
                            parts.headers,
                            serde_json::from_slice::<Value>(&body).unwrap(),
                        ));
                        Ok::<_, hyper::Error>(
                            Response::builder()
                                .status(302)
                                .header("Location", "/leak")
                                .body(Body::empty())
                                .unwrap(),
                        )
                    }
                }))
            }
        }));
        let url = format!("http://{}/v1/chat/completions", server.local_addr());
        let task = tokio::spawn(server);
        let dir = tempfile::tempdir().unwrap();
        crate::settings::model_provider::save_key(dir.path(), "openai", "fixture-private-key")
            .await
            .unwrap();
        let backend = OpenaiBackend::new(dir.path().into(), "test-model".into());
        let history = [ChatMessage {
            role: Role::User,
            text: Some("Hello".into()),
            tool_calls: vec![],
            tool_results: vec![],
        }];
        assert!(backend
            .send_at(&url, "Be helpful", &[], &history)
            .await
            .is_err());
        let requests = captured.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].0["authorization"], "Bearer fixture-private-key");
        assert_eq!(requests[0].1["store"], false);
        assert_eq!(requests[0].1["max_completion_tokens"], 2048);
        assert!(!requests[0].1.to_string().contains("fixture-private-key"));
        drop(requests);
        let private = [ChatMessage {
            role: Role::User,
            text: Some("fixture-private-key".into()),
            tool_calls: vec![],
            tool_results: vec![],
        }];
        assert!(backend
            .send_at(&url, "Be helpful", &[], &private)
            .await
            .is_err());
        assert_eq!(captured.lock().unwrap().len(), 1);
        task.abort();
    }
}
