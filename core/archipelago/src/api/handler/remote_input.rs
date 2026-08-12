use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use hyper::{Request, Response};
use hyper_ws_listener::WsStream;
use serde::Deserialize;
use std::time::Instant;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

use super::ApiHandler;

/// Allowed xdotool key names. Only these pass validation.
const ALLOWED_KEYS: &[&str] = &[
    // Letters
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n",
    "o",
    "p",
    "q",
    "r",
    "s",
    "t",
    "u",
    "v",
    "w",
    "x",
    "y",
    "z",
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
    "G",
    "H",
    "I",
    "J",
    "K",
    "L",
    "M",
    "N",
    "O",
    "P",
    "Q",
    "R",
    "S",
    "T",
    "U",
    "V",
    "W",
    "X",
    "Y",
    "Z",
    // Numbers
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    // Navigation
    "Up",
    "Down",
    "Left",
    "Right",
    "Return",
    "Escape",
    "Tab",
    "BackSpace",
    "Delete",
    "Home",
    "End",
    "Prior",
    "Next", // Prior=PageUp, Next=PageDown
    // Modifiers (for combos like shift+a)
    "space",
    "minus",
    "equal",
    "bracketleft",
    "bracketright",
    "backslash",
    "semicolon",
    "apostrophe",
    "grave",
    "comma",
    "period",
    "slash",
    // Function keys
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
    // Symbols — xdotool names
    "exclam",
    "at",
    "numbersign",
    "dollar",
    "percent",
    "asciicircum",
    "ampersand",
    "asterisk",
    "parenleft",
    "parenright",
    "underscore",
    "plus",
    "braceleft",
    "braceright",
    "bar",
    "colon",
    "quotedbl",
    "less",
    "greater",
    "question",
    "asciitilde",
];

/// Validate a key name against the whitelist.
/// Also allows "shift+X" combos where X is in the whitelist.
fn validate_key(key: &str) -> bool {
    if ALLOWED_KEYS.contains(&key) {
        return true;
    }
    // Allow modifier combos: "shift+a", "ctrl+c", etc.
    if let Some((modifier, base)) = key.split_once('+') {
        let valid_modifiers = ["shift", "ctrl", "alt", "super"];
        return valid_modifiers.contains(&modifier) && ALLOWED_KEYS.contains(&base);
    }
    false
}

#[derive(Deserialize)]
#[serde(tag = "t")]
enum InputCommand {
    #[serde(rename = "k")]
    Key {
        k: String,
        /// Optional player ID (1 or 2) for multi-player arcade games.
        /// When absent, input is broadcast without player tagging.
        #[serde(default)]
        #[allow(dead_code)]
        p: Option<u8>,
    },
    #[serde(rename = "m")]
    MouseMove { x: i32, y: i32 },
    #[serde(rename = "c")]
    Click { b: u8 },
    #[serde(rename = "s")]
    Scroll { y: i32 },
    #[serde(rename = "p")]
    Ping,
}

/// Validate and acknowledge input — relay-only, no xdotool.
/// All input is forwarded to browser clients via the broadcast channel;
/// the browser's remote-relay.ts dispatches DOM events from there.
async fn handle_input(msg: &str) -> Result<Option<String>> {
    let cmd: InputCommand = serde_json::from_str(msg).context("invalid input command")?;

    match cmd {
        InputCommand::Key { ref k, .. } => {
            if !validate_key(k) {
                warn!("rejected key: {}", k);
                return Ok(Some(r#"{"t":"e","m":"invalid key"}"#.to_string()));
            }
        }
        InputCommand::MouseMove { x, y } => {
            let _x = x.clamp(-50, 50);
            let _y = y.clamp(-50, 50);
        }
        InputCommand::Click { b } => {
            let _b = b.clamp(1, 3);
        }
        InputCommand::Scroll { y } => {
            let _y = y.clamp(-10, 10);
        }
        InputCommand::Ping => {
            return Ok(Some(r#"{"t":"p"}"#.to_string()));
        }
    }

    Ok(None)
}

impl ApiHandler {
    pub(super) async fn handle_remote_input(
        req: Request<hyper::Body>,
        relay_tx: broadcast::Sender<String>,
        mut external_open_rx: broadcast::Receiver<String>,
    ) -> Result<Response<hyper::Body>> {
        // Extract optional player ID from query string: /ws/remote-input?p=1
        let player_id: Option<u8> = req
            .uri()
            .query()
            .and_then(|q| q.split('&').find(|s| s.starts_with("p=")))
            .and_then(|s| s.get(2..))
            .and_then(|v| v.parse().ok())
            .filter(|&p: &u8| p == 1 || p == 2);

        let (response, ws_fut_opt) = hyper_ws_listener::create_ws(req)
            .map_err(|e| anyhow::anyhow!("WebSocket upgrade failed: {}", e))?;

        if let Some(ws_fut) = ws_fut_opt {
            tokio::spawn(async move {
                let ws_stream: WsStream = match ws_fut.await {
                    Ok(Ok(s)) => s,
                    Ok(Err(e)) => {
                        debug!("Remote input WS handshake failed: {}", e);
                        return;
                    }
                    Err(e) => {
                        debug!("Remote input WS task join failed: {}", e);
                        return;
                    }
                };

                info!("Remote input connected");

                let (mut tx, mut rx) = ws_stream.split();

                // Send ready message
                let _ = tx.send(Message::Text(r#"{"t":"ok"}"#.to_string())).await;

                let ping_interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
                tokio::pin!(ping_interval);
                let mut last_activity = Instant::now();
                let mut msg_count: u64 = 0;
                let mut rate_window_start = Instant::now();
                let mut rate_count: u32 = 0;
                const MAX_RATE: u32 = 120; // messages per second
                const INACTIVITY_TIMEOUT: u64 = 300;

                loop {
                    tokio::select! {
                        _ = ping_interval.tick() => {
                            if last_activity.elapsed().as_secs() >= INACTIVITY_TIMEOUT {
                                info!("Remote input inactive, closing");
                                let _ = tx.send(Message::Close(None)).await;
                                break;
                            }
                            if tx.send(Message::Ping(vec![])).await.is_err() {
                                break;
                            }
                        }
                        // Forward kiosk "open this URL externally" requests down to
                        // the companion so the link opens in the phone's browser.
                        ext = external_open_rx.recv() => {
                            match ext {
                                Ok(text) => {
                                    if tx.send(Message::Text(text)).await.is_err() {
                                        break;
                                    }
                                }
                                Err(broadcast::error::RecvError::Lagged(_)) => {}
                                Err(broadcast::error::RecvError::Closed) => {}
                            }
                        }
                        msg = rx.next() => {
                            match msg {
                                Some(Ok(Message::Text(text))) => {
                                    last_activity = Instant::now();
                                    msg_count += 1;

                                    // Rate limiting
                                    if rate_window_start.elapsed().as_millis() >= 1000 {
                                        rate_window_start = Instant::now();
                                        rate_count = 0;
                                    }
                                    rate_count += 1;
                                    if rate_count > MAX_RATE {
                                        continue; // silently drop
                                    }

                                    // Relay to browser clients. If this connection has a
                                    // player ID from query string and the message is a key
                                    // event without a player field, inject it so the browser
                                    // can route input to the correct player.
                                    let relay_text = if let Some(pid) = player_id {
                                        if text.contains(r#""t":"k""#) && !text.contains(r#""p":"#) {
                                            // Insert "p":N before the closing brace
                                            if let Some(pos) = text.rfind('}') {
                                                let mut tagged = text[..pos].to_string();
                                                tagged.push_str(&format!(r#","p":{}"#, pid));
                                                tagged.push('}');
                                                tagged
                                            } else {
                                                text.clone()
                                            }
                                        } else {
                                            text.clone()
                                        }
                                    } else {
                                        text.clone()
                                    };
                                    let _ = relay_tx.send(relay_text);

                                    match handle_input(&text).await {
                                        Ok(Some(reply)) => {
                                            let _ = tx.send(Message::Text(reply)).await;
                                        }
                                        Ok(None) => {}
                                        Err(e) => {
                                            debug!("Input error: {}", e);
                                            let err = format!(r#"{{"t":"e","m":"{}"}}"#,
                                                e.to_string().replace('"', "'"));
                                            let _ = tx.send(Message::Text(err)).await;
                                        }
                                    }
                                }
                                Some(Ok(Message::Pong(_))) => {
                                    last_activity = Instant::now();
                                }
                                Some(Ok(Message::Ping(data))) => {
                                    last_activity = Instant::now();
                                    let _ = tx.send(Message::Pong(data)).await;
                                }
                                Some(Ok(Message::Close(_))) | None => break,
                                Some(Ok(_)) => { last_activity = Instant::now(); }
                                Some(Err(e)) => {
                                    debug!("Remote input stream error: {}", e);
                                    break;
                                }
                            }
                        }
                    }
                }

                info!(
                    "Remote input disconnected ({} messages processed)",
                    msg_count
                );
            });
        }

        Ok(response)
    }
}
