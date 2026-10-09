//! Owner-authenticated terminal sessions backed by private tmux processes.
//!
//! Browser connections are disposable attachments. The tmux process and its
//! metadata remain on the node so a reconnect resumes the same workspace.

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use hyper::{Request, Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tokio_tungstenite::tungstenite::Message;

fn tmux_key_for_input(data: &str) -> Option<&'static str> {
    match data {
        "\u{3}" => Some("C-c"),
        "\u{4}" => Some("C-d"),
        "\r" | "\n" => Some("Enter"),
        "\u{7f}" => Some("BSpace"),
        "\t" => Some("Tab"),
        "\u{1b}[A" => Some("Up"),
        "\u{1b}[B" => Some("Down"),
        "\u{1b}[C" => Some("Right"),
        "\u{1b}[D" => Some("Left"),
        "\u{1b}[H" => Some("Home"),
        "\u{1b}[F" => Some("End"),
        "\u{1b}[3~" => Some("DC"),
        _ => None,
    }
}
use uuid::Uuid;

use super::{build_response, ApiHandler};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SessionRecord {
    pub schema: u8,
    pub id: String,
    pub name: String,
    pub workspace: String,
    pub state: String,
    pub tmux: String,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
struct CreateRequest {
    name: Option<String>,
    workspace: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClientMessage {
    #[serde(rename = "type")]
    kind: String,
    data: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
}

pub(crate) fn state_dir() -> PathBuf {
    std::env::var_os("ARCHY_SESSION_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
                PathBuf::from(xdg).join("archipelago/sessions")
            } else if let Some(home) = std::env::var_os("HOME") {
                let user_dir = PathBuf::from(home).join(".local/state/archipelago/sessions");
                if user_dir.exists() {
                    user_dir
                } else {
                    PathBuf::from("/var/lib/archipelago/sessions")
                }
            } else {
                PathBuf::from("/var/lib/archipelago/sessions")
            }
        })
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

async fn read_record(dir: &Path, id: &str) -> Result<SessionRecord> {
    if !valid_id(id) {
        return Err(anyhow!("invalid session id"));
    }
    let bytes = tokio::fs::read(dir.join(format!("{id}.json"))).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn tmux_alive(name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", name])
        .output()
        .await
        .map(|out| out.status.success())
        .unwrap_or(false)
}

async fn write_record(dir: &Path, record: &SessionRecord) -> Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    let tmp = dir.join(format!(".{}.tmp-{}", record.id, Uuid::new_v4()));
    let final_path = dir.join(format!("{}.json", record.id));
    tokio::fs::write(&tmp, serde_json::to_vec_pretty(record)?).await?;
    tokio::fs::rename(tmp, final_path).await?;
    Ok(())
}

pub(crate) async fn list(dir: &Path) -> Result<Vec<SessionRecord>> {
    let mut out = Vec::new();
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(error) => return Err(error.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = tokio::fs::read(entry.path()).await else {
            continue;
        };
        let Ok(mut record) = serde_json::from_slice::<SessionRecord>(&bytes) else {
            continue;
        };
        record.state = if tmux_alive(&record.tmux).await {
            "detached".into()
        } else if record.state == "running" {
            "interrupted".into()
        } else {
            record.state.clone()
        };
        out.push(record);
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(out)
}

pub(crate) async fn list_response() -> Result<Response<hyper::Body>> {
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(hyper::Body::from(serde_json::to_vec(
            &list(&state_dir()).await?,
        )?))?)
}

pub(crate) async fn create(body: &[u8]) -> Result<Response<hyper::Body>> {
    let request: CreateRequest = serde_json::from_slice(body).unwrap_or(CreateRequest {
        name: None,
        workspace: None,
    });
    let workspace_was_requested = request.workspace.is_some();
    let workspace = request.workspace.unwrap_or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("Work")
            .to_string_lossy()
            .into_owned()
    });
    let workspace_path = PathBuf::from(&workspace);
    if !workspace_was_requested {
        tokio::fs::create_dir_all(&workspace_path).await?;
    }
    if !workspace_path.is_absolute() || !workspace_path.is_dir() || workspace_path == Path::new("/")
    {
        return Ok(build_response(
            StatusCode::BAD_REQUEST,
            "application/json",
            hyper::Body::from(r#"{"error":"workspace must be an existing non-root directory"}"#),
        ));
    }
    let id = format!("s-{}", Uuid::new_v4().simple());
    let tmux_name = format!("archy-{id}");
    let output = Command::new("tmux")
        .args(["new-session", "-d", "-s", &tmux_name, "-c", &workspace])
        .output()
        .await?;
    if !output.status.success() {
        return Ok(build_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "application/json",
            hyper::Body::from(r#"{"error":"tmux could not start the session"}"#),
        ));
    }
    let name = request
        .name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "Work".into());
    let record = SessionRecord {
        schema: 1,
        id,
        name,
        workspace,
        state: "running".into(),
        tmux: tmux_name,
        updated_at: chrono::Utc::now().timestamp(),
    };
    write_record(&state_dir(), &record).await?;
    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/json")
        .body(hyper::Body::from(serde_json::to_vec(&record)?))?)
}

pub(crate) async fn websocket(req: Request<hyper::Body>) -> Result<Response<hyper::Body>> {
    let id = req
        .uri()
        .query()
        .and_then(|query| {
            query
                .split('&')
                .find_map(|part| part.strip_prefix("session="))
        })
        .unwrap_or("")
        .to_string();
    let record = read_record(&state_dir(), &id).await?;
    if !tmux_alive(&record.tmux).await {
        return Ok(build_response(
            StatusCode::CONFLICT,
            "application/json",
            hyper::Body::from(r#"{"error":"session is not running"}"#),
        ));
    }
    let (response, ws_fut) =
        hyper_ws_listener::create_ws(req).map_err(|e| anyhow!("WebSocket upgrade failed: {e}"))?;
    if let Some(ws_fut) = ws_fut {
        tokio::spawn(async move {
            let Ok(Ok(stream)) = ws_fut.await else { return };
            let (mut tx, mut rx) = stream.split();
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(150));
            let mut last = String::new();
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Capture the visible pane only. Asking tmux for a large
                        // historical range injects hundreds of blank rows into
                        // xterm on every reconnect, producing a misleading
                        // giant initial scrollbar. xterm owns live scrollback.
                        let output = Command::new("tmux").args(["capture-pane", "-p", "-e", "-t", &record.tmux]).output().await;
                        if let Ok(output) = output {
                            let text = String::from_utf8_lossy(&output.stdout).into_owned();
                            if text != last { last = text.clone(); if tx.send(Message::Text(serde_json::json!({"type":"output", "data":text}).to_string())).await.is_err() { break; } }
                        } else { break; }
                    }
                    message = rx.next() => match message {
                        Some(Ok(Message::Text(text))) => {
                            let Ok(message) = serde_json::from_str::<ClientMessage>(&text) else { continue };
                            match message.kind.as_str() {
                                "input" => if let Some(data) = message.data { let mut command = Command::new("tmux"); command.args(["send-keys", "-t", &record.tmux]); if let Some(key) = tmux_key_for_input(&data) { command.arg(key); } else { command.args(["-l", "--", &data]); } let _ = command.output().await; },
                                "resize" => if let (Some(cols), Some(rows)) = (message.cols, message.rows) { let _ = Command::new("tmux").args(["resize-window", "-t", &record.tmux, "-x", &cols.to_string(), "-y", &rows.to_string()]).output().await; },
                                "ping" => { let _ = tx.send(Message::Text(r#"{"type":"pong"}"#.into())).await; },
                                _ => {}
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Err(_)) => break,
                        _ => {}
                    }
                }
            }
        });
    }
    Ok(response)
}

impl ApiHandler {
    pub(super) async fn handle_terminal_websocket(
        req: Request<hyper::Body>,
    ) -> Result<Response<hyper::Body>> {
        websocket(req).await
    }
}
