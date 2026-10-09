//! CDP input bridge — forwards companion remote input into the local kiosk
//! Chromium as *trusted*, browser-level input via the DevTools protocol.
//!
//! Why this exists: the web relay path (`remote-relay.ts`) synthesizes DOM
//! events in the top document, and synthetic events can never cross into a
//! cross-origin iframe — so on the kiosk, companion taps/keys/scrolls died at
//! the border of every containerized app's frame. CDP `Input.dispatch*`
//! events enter the browser's real input pipeline: they hit-test through any
//! frame, move focus, and insert text exactly like a physical device, which
//! is the only correct way to drive app iframes (tracked in the unified task
//! tracker; supersedes the earlier "no CDP" note in
//! `docs/tv-input-iframe-apps.md`, which was about gamepad *keys* only).
//!
//! The kiosk launcher opens Chromium with `--remote-debugging-port=9222`
//! bound to loopback. This keeper task discovers the page target, holds one
//! WebSocket to it, and reconnects whenever the kiosk restarts (the launcher
//! supervises Chromium in a loop, so the debugger URL changes under us).
//! When the bridge is not connected (non-kiosk installs, kiosk booting),
//! `is_active()` is false and callers fall back to the web relay unchanged.
//!
//! Security: the CDP port is loopback-only and Chromium's default origin
//! check stands (we deliberately do NOT pass `--remote-allow-origins`, so
//! browser pages can't open the debug socket; our raw client sends no
//! Origin header and is accepted).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info};

const CDP_HTTP: &str = "http://127.0.0.1:9222";
/// Marker whose presence means this node drives a local kiosk display.
const KIOSK_UNIT: &str = "/etc/systemd/system/archipelago-kiosk.service";
/// One relay scroll step ≈ this many CSS pixels (matches remote-relay.ts).
const SCROLL_STEP_PX: f64 = 100.0;

/// Cloneable handle the WS handlers use to feed validated relay JSON into
/// the keeper task.
#[derive(Clone)]
pub struct CdpBridge {
    tx: mpsc::Sender<String>,
    connected: Arc<AtomicBool>,
}

impl CdpBridge {
    /// Spawn the session keeper and return the shared handle.
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<String>(256);
        let connected = Arc::new(AtomicBool::new(false));
        tokio::spawn(run_keeper(rx, connected.clone()));
        Self { tx, connected }
    }

    /// True only while a live CDP session to the kiosk Chromium exists.
    pub fn is_active(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// Queue a validated relay input message (the exact JSON that goes to the
    /// broadcast channel) for CDP dispatch. Best-effort: if the keeper is
    /// behind or gone the message is dropped — input is transient by nature.
    pub fn send(&self, relay_json: &str) {
        let _ = self.tx.try_send(relay_json.to_string());
    }
}

/// Virtual cursor state, server-side. The web relay keeps this in the kiosk
/// page (`cursorX`/`cursorY`); CDP needs its own copy because trusted mouse
/// events carry absolute viewport coordinates.
struct Cursor {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

async fn run_keeper(mut rx: mpsc::Receiver<String>, connected: Arc<AtomicBool>) {
    loop {
        // Cheap gate: no kiosk unit on this node → nothing to drive. Keep
        // draining queued input so the channel never backs up.
        if tokio::fs::metadata(KIOSK_UNIT).await.is_err() {
            drain_for(&mut rx, Duration::from_secs(60)).await;
            continue;
        }
        let Some(ws_url) = discover_page_target().await else {
            // Kiosk configured but Chromium not up (or CDP flag not rolled
            // out yet) — retry gently.
            drain_for(&mut rx, Duration::from_secs(15)).await;
            continue;
        };
        match drive_session(&ws_url, &mut rx, &connected).await {
            Ok(()) => info!("CDP kiosk input session ended cleanly"),
            Err(e) => debug!(error = %e, "CDP kiosk input session dropped — will re-discover"),
        }
        connected.store(false, Ordering::Relaxed);
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

/// Discard queued input for `d` — used while no kiosk session exists so the
/// bounded channel can't fill with stale events.
async fn drain_for(rx: &mut mpsc::Receiver<String>, d: Duration) {
    let _ = tokio::time::timeout(d, async { while rx.recv().await.is_some() {} }).await;
}

/// Find the kiosk page target's WebSocket debugger URL. Prefers the page on
/// localhost (the kiosk app) over e.g. devtools/extension targets.
async fn discover_page_target() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .ok()?;
    let list: Vec<Value> = client
        .get(format!("{CDP_HTTP}/json/list"))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let pages: Vec<&Value> = list
        .iter()
        .filter(|t| t.get("type").and_then(Value::as_str) == Some("page"))
        .collect();
    let preferred = pages
        .iter()
        .find(|t| {
            t.get("url")
                .and_then(Value::as_str)
                .is_some_and(|u| u.contains("localhost") || u.contains("127.0.0.1"))
        })
        .or_else(|| pages.first());
    preferred?
        .get("webSocketDebuggerUrl")
        .and_then(Value::as_str)
        .map(str::to_string)
}

async fn drive_session(
    ws_url: &str,
    rx: &mut mpsc::Receiver<String>,
    connected: &Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let (ws, _) = tokio_tungstenite::connect_async(ws_url).await?;
    let (mut sink, mut stream) = ws.split();
    let mut next_id: u64 = 0;
    let mut id = move || {
        next_id += 1;
        next_id
    };

    // Viewport size for cursor clamping. Best-effort: fall back to 1080p if
    // the metrics call fails — clamping is a nicety, not a correctness need.
    sink.send(Message::Text(
        json!({"id": id(), "method": "Page.getLayoutMetrics"}).to_string(),
    ))
    .await?;
    let (mut vw, mut vh) = (1920.0_f64, 1080.0_f64);
    if let Ok(Some(Ok(Message::Text(txt)))) =
        tokio::time::timeout(Duration::from_secs(3), stream.next()).await
    {
        if let Ok(v) = serde_json::from_str::<Value>(&txt) {
            if let Some(vp) = v.pointer("/result/cssLayoutViewport") {
                vw = vp.get("clientWidth").and_then(Value::as_f64).unwrap_or(vw);
                vh = vp.get("clientHeight").and_then(Value::as_f64).unwrap_or(vh);
            }
        }
    }
    let mut cursor = Cursor {
        x: vw / 2.0,
        y: vh / 2.0,
        w: vw,
        h: vh,
    };

    connected.store(true, Ordering::Relaxed);
    info!(viewport = %format!("{vw}x{vh}"), "CDP kiosk input bridge connected");

    loop {
        tokio::select! {
            cmd = rx.recv() => {
                let Some(cmd) = cmd else { return Ok(()) };
                for frame in translate(&cmd, &mut cursor, &mut id) {
                    sink.send(Message::Text(frame.to_string())).await?;
                }
            }
            msg = stream.next() => {
                match msg {
                    // Responses/events — nothing to correlate, but a read
                    // error or close means Chromium restarted.
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e.into()),
                    None => anyhow::bail!("CDP socket closed"),
                }
            }
        }
    }
}

/// Translate one validated relay input message into CDP command frames.
fn translate(raw: &str, cursor: &mut Cursor, id: &mut impl FnMut() -> u64) -> Vec<Value> {
    let Ok(msg) = serde_json::from_str::<Value>(raw) else {
        return vec![];
    };
    match msg.get("t").and_then(Value::as_str) {
        Some("m") => {
            let dx = msg.get("x").and_then(Value::as_i64).unwrap_or(0) as f64;
            let dy = msg.get("y").and_then(Value::as_i64).unwrap_or(0) as f64;
            cursor.x = (cursor.x + dx).clamp(0.0, cursor.w - 1.0);
            cursor.y = (cursor.y + dy).clamp(0.0, cursor.h - 1.0);
            vec![mouse_event(id(), "mouseMoved", cursor, "none", 0, 1)]
        }
        Some("c") => {
            let b = msg
                .get("b")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .clamp(1, 3);
            let (button, buttons) = match b {
                2 => ("middle", 4),
                3 => ("right", 2),
                _ => ("left", 1),
            };
            vec![
                // Hover first so the press lands on current hit-test state.
                mouse_event(id(), "mouseMoved", cursor, "none", 0, 1),
                mouse_event(id(), "mousePressed", cursor, button, buttons, 1),
                mouse_event(id(), "mouseReleased", cursor, button, 0, 1),
            ]
        }
        Some("s") => {
            let dy = msg.get("y").and_then(Value::as_i64).unwrap_or(0) as f64 * SCROLL_STEP_PX;
            vec![json!({
                "id": id(),
                "method": "Input.dispatchMouseEvent",
                "params": {
                    "type": "mouseWheel",
                    "x": cursor.x, "y": cursor.y,
                    "deltaX": 0.0, "deltaY": dy,
                    "pointerType": "mouse",
                }
            })]
        }
        Some("k") => {
            let Some(k) = msg.get("k").and_then(Value::as_str) else {
                return vec![];
            };
            key_events(k, id)
        }
        _ => vec![],
    }
}

fn mouse_event(
    id: u64,
    kind: &str,
    cursor: &Cursor,
    button: &str,
    buttons: u32,
    clicks: u32,
) -> Value {
    json!({
        "id": id,
        "method": "Input.dispatchMouseEvent",
        "params": {
            "type": kind,
            "x": cursor.x, "y": cursor.y,
            "button": button,
            "buttons": buttons,
            "clickCount": if kind == "mousePressed" || kind == "mouseReleased" { clicks } else { 0 },
            "pointerType": "mouse",
        }
    })
}

/// xdotool named key → (DOM key, DOM code, Windows virtual-key code).
fn named_key(k: &str) -> Option<(&'static str, &'static str, i32)> {
    Some(match k {
        "Return" => ("Enter", "Enter", 13),
        "BackSpace" => ("Backspace", "Backspace", 8),
        "Escape" => ("Escape", "Escape", 27),
        "Tab" => ("Tab", "Tab", 9),
        "Delete" => ("Delete", "Delete", 46),
        "Up" => ("ArrowUp", "ArrowUp", 38),
        "Down" => ("ArrowDown", "ArrowDown", 40),
        "Left" => ("ArrowLeft", "ArrowLeft", 37),
        "Right" => ("ArrowRight", "ArrowRight", 39),
        "Home" => ("Home", "Home", 36),
        "End" => ("End", "End", 35),
        "Prior" => ("PageUp", "PageUp", 33),
        "Next" => ("PageDown", "PageDown", 34),
        "F1" => ("F1", "F1", 112),
        "F2" => ("F2", "F2", 113),
        "F3" => ("F3", "F3", 114),
        "F4" => ("F4", "F4", 115),
        "F5" => ("F5", "F5", 116),
        "F6" => ("F6", "F6", 117),
        "F7" => ("F7", "F7", 118),
        "F8" => ("F8", "F8", 119),
        "F9" => ("F9", "F9", 120),
        "F10" => ("F10", "F10", 121),
        "F11" => ("F11", "F11", 122),
        "F12" => ("F12", "F12", 123),
        _ => return None,
    })
}

/// xdotool symbol name → printable char (the relay whitelist speaks xdotool).
fn symbol_char(k: &str) -> Option<char> {
    Some(match k {
        "space" => ' ',
        "exclam" => '!',
        "at" => '@',
        "numbersign" => '#',
        "dollar" => '$',
        "percent" => '%',
        "asciicircum" => '^',
        "ampersand" => '&',
        "asterisk" => '*',
        "parenleft" => '(',
        "parenright" => ')',
        "underscore" => '_',
        "plus" => '+',
        "braceleft" => '{',
        "braceright" => '}',
        "bar" => '|',
        "colon" => ':',
        "quotedbl" => '"',
        "less" => '<',
        "greater" => '>',
        "question" => '?',
        "asciitilde" => '~',
        "minus" => '-',
        "equal" => '=',
        "bracketleft" => '[',
        "bracketright" => ']',
        "backslash" => '\\',
        "semicolon" => ';',
        "apostrophe" => '\'',
        "grave" => '`',
        "comma" => ',',
        "period" => '.',
        "slash" => '/',
        _ => return None,
    })
}

/// Build the CDP frame pair (keyDown, keyUp) for one relay key name,
/// including `modifier+base` combos. A keyDown that carries `text` both
/// fires real keydown/keypress AND inserts the character — exactly how a
/// physical keystroke behaves, so games see the key and fields get the text.
fn key_events(k: &str, id: &mut impl FnMut() -> u64) -> Vec<Value> {
    let (modifiers, base) = match k.split_once('+') {
        Some((m, b)) => (
            match m {
                "alt" => 1,
                "ctrl" => 2,
                "super" => 4,
                "shift" => 8,
                _ => 0,
            },
            b,
        ),
        None => (0, k),
    };

    let (key, code, vk, text): (String, Option<&str>, i32, Option<String>) =
        if let Some((key, code, vk)) = named_key(base) {
            // Enter carries "\r" like a real keyboard so single-line inputs
            // submit and textareas newline.
            let text = (key == "Enter").then(|| "\r".to_string());
            (key.to_string(), Some(code), vk, text)
        } else {
            let ch = if base.chars().count() == 1 {
                base.chars().next()
            } else {
                symbol_char(base)
            };
            let Some(mut ch) = ch else {
                return vec![];
            };
            if modifiers == 8 && ch.is_ascii_alphabetic() {
                ch = ch.to_ascii_uppercase();
            }
            let vk = ch.to_ascii_uppercase() as i32;
            // Ctrl/Alt/Super chords are shortcuts, not typing — no text.
            let text = (modifiers & !8 == 0).then(|| ch.to_string());
            (ch.to_string(), None, vk, text)
        };

    let mut down = json!({
        "id": id(),
        "method": "Input.dispatchKeyEvent",
        "params": {
            "type": "keyDown",
            "key": key,
            "modifiers": modifiers,
            "windowsVirtualKeyCode": vk,
            "nativeVirtualKeyCode": vk,
        }
    });
    if let Some(code) = code {
        down["params"]["code"] = json!(code);
    }
    if let Some(t) = &text {
        down["params"]["text"] = json!(t);
        down["params"]["unmodifiedText"] = json!(t);
    }
    let mut up = json!({
        "id": id(),
        "method": "Input.dispatchKeyEvent",
        "params": {
            "type": "keyUp",
            "key": key,
            "modifiers": modifiers,
            "windowsVirtualKeyCode": vk,
            "nativeVirtualKeyCode": vk,
        }
    });
    if let Some(code) = code {
        up["params"]["code"] = json!(code);
    }
    vec![down, up]
}
