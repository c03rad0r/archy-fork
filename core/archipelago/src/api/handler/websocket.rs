use crate::monitoring::MetricsStore;
use crate::state::StateManager;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use hyper::{Request, Response};
use hyper_ws_listener::WsStream;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info};

use super::ApiHandler;

impl ApiHandler {
    pub(super) async fn handle_websocket(
        req: Request<hyper::Body>,
        state_manager: Arc<StateManager>,
        metrics_store: Arc<MetricsStore>,
    ) -> Result<Response<hyper::Body>> {
        let (response, ws_fut_opt) = hyper_ws_listener::create_ws(req)
            .map_err(|e| anyhow::anyhow!("WebSocket upgrade failed: {}", e))?;

        if let Some(ws_fut) = ws_fut_opt {
            tokio::spawn(async move {
                let ws_stream: WsStream = match ws_fut.await {
                    Ok(Ok(s)) => s,
                    Ok(Err(e)) => {
                        debug!("WebSocket handshake failed (hyper): {}", e);
                        return;
                    }
                    Err(e) => {
                        debug!("WebSocket task join failed: {}", e);
                        return;
                    }
                };
                metrics_store.increment_ws();
                info!("WebSocket /ws/db connected");

                let (mut tx, mut rx) = ws_stream.split();

                // Subscribe BEFORE taking the initial snapshot. Messages are full
                // data dumps keyed by a monotonic revision, so a broadcast that
                // races the snapshot is at worst a harmless duplicate/newer dump
                // delivered right after — but subscribing after the snapshot send
                // (the old order) let any update in that window vanish forever,
                // since a tokio broadcast channel never delivers sends that
                // predate subscribe(). That silently stuck clients (e.g. a fresh
                // install's post-boot container scan) on a stale initial snapshot
                // until a full page reload opened a new connection past the race.
                let mut state_rx = state_manager.subscribe();

                let initial_msg = state_manager.get_initial_message().await;
                if let Ok(json_msg) = serde_json::to_string(&initial_msg) {
                    if let Err(e) = tx.send(Message::Text(json_msg)).await {
                        debug!("Failed to send initial data: {}", e);
                        return;
                    }
                    debug!("Sent initial data dump at revision {}", initial_msg.rev);
                }
                let ping_interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
                tokio::pin!(ping_interval);
                let mut last_client_activity = Instant::now();
                const INACTIVITY_TIMEOUT_SECS: u64 = 300; // 5 minutes

                loop {
                    tokio::select! {
                        _ = ping_interval.tick() => {
                            // Check inactivity timeout
                            if last_client_activity.elapsed().as_secs() >= INACTIVITY_TIMEOUT_SECS {
                                info!("WebSocket client inactive for {}s, closing", INACTIVITY_TIMEOUT_SECS);
                                let _ = tx.send(Message::Close(None)).await;
                                break;
                            }
                            if tx.send(Message::Ping(vec![])).await.is_err() {
                                debug!("Failed to send ping, connection likely closed");
                                break;
                            }
                        }
                        update = state_rx.recv() => {
                            match update {
                                Ok(msg) => {
                                    if let Ok(json_msg) = serde_json::to_string(&msg) {
                                        if let Err(e) = tx.send(Message::Text(json_msg)).await {
                                            debug!("Failed to send state update: {}", e);
                                            break;
                                        }
                                        debug!("Sent state update at revision {}", msg.rev);
                                    }
                                }
                                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                                    debug!("Client lagged behind, skipped {} messages", skipped);
                                }
                                Err(broadcast::error::RecvError::Closed) => {
                                    debug!("Broadcast channel closed");
                                    break;
                                }
                            }
                        }
                        msg = rx.next() => {
                            match msg {
                                Some(Ok(Message::Close(_))) => break,
                                Some(Ok(Message::Pong(_))) => {
                                    last_client_activity = Instant::now();
                                    debug!("Received pong");
                                }
                                Some(Ok(Message::Ping(data))) => {
                                    last_client_activity = Instant::now();
                                    let _ = tx.send(Message::Pong(data)).await;
                                }
                                Some(Ok(Message::Text(text))) => {
                                    last_client_activity = Instant::now();
                                    // Handle JSON ping from frontend
                                    if text.contains("\"type\":\"ping\"") || text.contains("\"type\": \"ping\"") {
                                        let _ = tx.send(Message::Text(r#"{"type":"pong"}"#.to_string())).await;
                                    }
                                }
                                Some(Ok(_)) => {
                                    last_client_activity = Instant::now();
                                }
                                Some(Err(e)) => {
                                    debug!("WebSocket stream error: {}", e);
                                    break;
                                }
                                None => break,
                            }
                        }
                    }
                }
                metrics_store.decrement_ws();
                info!("WebSocket /ws/db disconnected");
            });
        }

        Ok(response)
    }
}
