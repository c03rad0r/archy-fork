//! Scheduled / queued mesh messages (issue #50, phase 1.7).
//!
//! A small persisted queue of messages to send at a future time. A background
//! task fires due messages via the listener. A message addressed to a peer that
//! isn't currently in the contact table stays queued and retries on later ticks
//! — i.e. it sends itself when the peer comes back in range.

use super::listener::{MeshCommand, MeshState};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::{watch, RwLock};
use tracing::warn;

const SCHEDULER_FILE: &str = "mesh-scheduled.json";
/// Wake interval for firing due messages.
const TICK_SECS: u64 = 10;
/// Drop a still-undeliverable message after this many attempts (~1h at 10s).
const MAX_ATTEMPTS: u32 = 360;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledMessage {
    pub id: u64,
    /// Direct-message target (peer contact_id), or None for a channel broadcast.
    #[serde(default)]
    pub contact_id: Option<u32>,
    /// Channel to broadcast on, or None for a direct message.
    #[serde(default)]
    pub channel: Option<u8>,
    pub body: String,
    /// Unix seconds when the message becomes due.
    pub fire_at: i64,
    #[serde(default)]
    pub attempts: u32,
}

pub struct MeshScheduler {
    path: PathBuf,
    queue: RwLock<Vec<ScheduledMessage>>,
    next_id: RwLock<u64>,
}

impl MeshScheduler {
    pub async fn load(data_dir: &Path) -> Self {
        let path = data_dir.join(SCHEDULER_FILE);
        let queue: Vec<ScheduledMessage> = match fs::read_to_string(&path).await {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        let next = queue.iter().map(|m| m.id).max().unwrap_or(0) + 1;
        Self {
            path,
            queue: RwLock::new(queue),
            next_id: RwLock::new(next),
        }
    }

    async fn save(&self) -> Result<()> {
        let json = {
            let q = self.queue.read().await;
            serde_json::to_string_pretty(&*q).context("serialize scheduled queue")?
        };
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await.ok();
        }
        fs::write(&self.path, json)
            .await
            .context("write scheduled queue")?;
        Ok(())
    }

    pub async fn add(
        &self,
        contact_id: Option<u32>,
        channel: Option<u8>,
        body: String,
        fire_at: i64,
    ) -> Result<ScheduledMessage> {
        let id = {
            let mut n = self.next_id.write().await;
            let id = *n;
            *n += 1;
            id
        };
        let msg = ScheduledMessage {
            id,
            contact_id,
            channel,
            body,
            fire_at,
            attempts: 0,
        };
        self.queue.write().await.push(msg.clone());
        self.save().await?;
        Ok(msg)
    }

    pub async fn list(&self) -> Vec<ScheduledMessage> {
        let mut v = self.queue.read().await.clone();
        v.sort_by_key(|m| m.fire_at);
        v
    }

    pub async fn cancel(&self, id: u64) -> Result<bool> {
        let removed = {
            let mut q = self.queue.write().await;
            let before = q.len();
            q.retain(|m| m.id != id);
            q.len() != before
        };
        if removed {
            self.save().await?;
        }
        Ok(removed)
    }
}

/// Background loop: every `TICK_SECS`, fire any due messages.
pub async fn run_scheduler(
    scheduler: Arc<MeshScheduler>,
    state: Arc<MeshState>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(TICK_SECS));
    loop {
        tokio::select! {
            _ = interval.tick() => fire_due(&scheduler, &state).await,
            _ = shutdown.changed() => {
                if *shutdown.borrow() { return; }
            }
        }
    }
}

async fn fire_due(scheduler: &Arc<MeshScheduler>, state: &Arc<MeshState>) {
    let now = chrono::Utc::now().timestamp();
    let due: Vec<ScheduledMessage> = scheduler
        .queue
        .read()
        .await
        .iter()
        .filter(|m| m.fire_at <= now)
        .cloned()
        .collect();
    if due.is_empty() {
        return;
    }

    let mut delivered: Vec<u64> = Vec::new();
    let mut failed: Vec<u64> = Vec::new();
    for msg in &due {
        if try_send(state, msg).await {
            delivered.push(msg.id);
        } else {
            failed.push(msg.id);
        }
    }

    let mut to_remove = delivered;
    {
        let mut q = scheduler.queue.write().await;
        for m in q.iter_mut() {
            if failed.contains(&m.id) {
                m.attempts += 1;
                if m.attempts >= MAX_ATTEMPTS {
                    warn!(
                        id = m.id,
                        attempts = m.attempts,
                        "Dropping undeliverable scheduled message"
                    );
                    to_remove.push(m.id);
                }
            }
        }
        q.retain(|m| !to_remove.contains(&m.id));
    }
    if let Err(e) = scheduler.save().await {
        warn!("Failed to persist mesh outbox after sweep: {e:#}");
    }
}

/// Hand a due message to the radio. Returns true if it was sent (or should be
/// dropped); false to keep it queued for a later retry (peer not in range yet).
async fn try_send(state: &Arc<MeshState>, msg: &ScheduledMessage) -> bool {
    let payload = msg.body.clone().into_bytes();
    if let Some(channel) = msg.channel {
        return state
            .send_cmd(MeshCommand::BroadcastChannel { channel, payload })
            .await
            .is_ok();
    }
    if let Some(contact_id) = msg.contact_id {
        let pubkey = {
            let peers = state.peers.read().await;
            peers.get(&contact_id).and_then(|p| p.pubkey_hex.clone())
        };
        if let Some(pk) = pubkey {
            if let Ok(bytes) = hex::decode(&pk) {
                if bytes.len() >= 6 {
                    let mut dest = [0u8; 6];
                    dest.copy_from_slice(&bytes[..6]);
                    return state
                        .send_cmd(MeshCommand::SendText {
                            dest_pubkey_prefix: dest,
                            payload,
                        })
                        .await
                        .is_ok();
                }
            }
        }
        // Peer unknown / not in range yet — keep queued, retry next tick.
        return false;
    }
    warn!("Scheduled message has neither channel nor contact_id — dropping");
    true
}
