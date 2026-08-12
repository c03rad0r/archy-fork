use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use hyper::{Request, Response};
use hyper_ws_listener::WsStream;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info};

use super::ApiHandler;

impl ApiHandler {
    /// WebSocket endpoint for browser clients to receive relayed companion input.
    /// The browser's remote-relay.ts dispatches these as DOM keyboard/mouse events.
    ///
    /// The kiosk also uses this socket in the *reverse* direction: when an "open
    /// in external browser" app is launched, the kiosk can't usefully open it
    /// itself, so it sends `{"t":"o","url":"https://…"}` here. We validate the
    /// URL and publish it on `external_open_tx`, which the companion (phone)
    /// socket forwards so the link opens in the phone's default browser.
    pub(super) async fn handle_remote_relay(
        req: Request<hyper::Body>,
        mut relay_rx: broadcast::Receiver<String>,
        external_open_tx: broadcast::Sender<String>,
    ) -> Result<Response<hyper::Body>> {
        let (response, ws_fut_opt) = hyper_ws_listener::create_ws(req)
            .map_err(|e| anyhow::anyhow!("WebSocket upgrade failed: {}", e))?;

        if let Some(ws_fut) = ws_fut_opt {
            tokio::spawn(async move {
                let ws_stream: WsStream = match ws_fut.await {
                    Ok(Ok(s)) => s,
                    Ok(Err(e)) => {
                        debug!("Remote relay WS handshake failed: {}", e);
                        return;
                    }
                    Err(e) => {
                        debug!("Remote relay WS task join failed: {}", e);
                        return;
                    }
                };

                info!("Remote relay client connected");

                let (mut tx, mut rx) = ws_stream.split();

                // Send ready message
                let _ = tx.send(Message::Text(r#"{"t":"ok"}"#.to_string())).await;

                let ping_interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
                tokio::pin!(ping_interval);

                loop {
                    tokio::select! {
                        _ = ping_interval.tick() => {
                            if tx.send(Message::Ping(vec![])).await.is_err() {
                                break;
                            }
                        }
                        // Forward relayed input from companion app
                        msg = relay_rx.recv() => {
                            match msg {
                                Ok(text) => {
                                    if tx.send(Message::Text(text)).await.is_err() {
                                        break;
                                    }
                                }
                                Err(broadcast::error::RecvError::Lagged(n)) => {
                                    debug!("Remote relay lagged, dropped {} messages", n);
                                }
                                Err(broadcast::error::RecvError::Closed) => break,
                            }
                        }
                        // Handle client-side messages (pong, close, open-url requests)
                        client_msg = rx.next() => {
                            match client_msg {
                                Some(Ok(Message::Pong(_))) | Some(Ok(Message::Ping(_))) => {}
                                Some(Ok(Message::Text(text))) => {
                                    // The only kiosk→server message we accept is an
                                    // external-open request: {"t":"o","url":"https://…"}.
                                    if let Some(url) = parse_open_url(&text) {
                                        debug!("Relaying external-open to companion: {}", url);
                                        let _ = external_open_tx.send(
                                            format!(r#"{{"t":"o","url":{}}}"#, json_string(&url))
                                        );
                                    }
                                }
                                Some(Ok(Message::Close(_))) | None => break,
                                _ => {}
                            }
                        }
                    }
                }

                info!("Remote relay client disconnected");
            });
        }

        Ok(response)
    }
}

/// Parse a kiosk `{"t":"o","url":"…"}` external-open request, returning the URL
/// only if it's a well-formed http(s) URL. Anything else (other message tags,
/// non-http schemes like `javascript:`/`file:`, malformed JSON) is rejected so a
/// compromised kiosk page can't push arbitrary URIs to the phone.
fn parse_open_url(text: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    if v.get("t").and_then(|t| t.as_str()) != Some("o") {
        return None;
    }
    let url = v.get("url").and_then(|u| u.as_str())?.trim();
    if url.len() > 2048 {
        return None;
    }
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        Some(url.to_string())
    } else {
        None
    }
}

/// Serialize a string as a JSON string literal (with surrounding quotes).
fn json_string(s: &str) -> String {
    serde_json::Value::String(s.to_string()).to_string()
}
