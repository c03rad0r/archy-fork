//! DWN (Decentralized Web Node) sync protocol.
//!
//! Manages syncing DWN data between the local node and connected peers.
//! Communicates with the DWN server container via its HTTP API.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::debug;

const DWN_SYNC_FILE: &str = "dwn/sync_state.json";

/// DWN sync status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum SyncStatus {
    #[default]
    Idle,
    Syncing,
    Synced,
    Error,
}

/// DWN sync state persisted to disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DwnSyncState {
    pub status: SyncStatus,
    pub last_sync: Option<String>,
    pub messages_synced: u64,
    pub storage_bytes: u64,
    pub registered_protocols: Vec<String>,
    pub peer_sync_targets: Vec<String>,
}

/// Load DWN sync state from disk.
pub async fn load_sync_state(data_dir: &Path) -> Result<DwnSyncState> {
    let path = data_dir.join(DWN_SYNC_FILE);
    if !path.exists() {
        return Ok(DwnSyncState::default());
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read DWN sync state")?;
    let state: DwnSyncState = serde_json::from_str(&content).unwrap_or_default();
    Ok(state)
}

/// Save DWN sync state to disk.
pub async fn save_sync_state(data_dir: &Path, state: &DwnSyncState) -> Result<()> {
    let dir = data_dir.join("dwn");
    fs::create_dir_all(&dir)
        .await
        .context("Failed to create dwn dir")?;
    let path = data_dir.join(DWN_SYNC_FILE);
    let content = serde_json::to_string_pretty(state).context("Failed to serialize DWN state")?;
    fs::write(&path, content)
        .await
        .context("Failed to write DWN state")?;
    Ok(())
}

/// Query the local DWN server for status information.
pub async fn get_dwn_status() -> Result<DwnStatusResponse> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .context("Failed to build HTTP client")?;

    let res = client
        .get(crate::constants::DWN_HEALTH_URL)
        .send()
        .await
        .context("DWN server not reachable")?;

    if res.status().is_success() {
        Ok(DwnStatusResponse {
            running: true,
            version: "0.4.0".to_string(),
        })
    } else {
        Ok(DwnStatusResponse {
            running: false,
            version: String::new(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DwnStatusResponse {
    pub running: bool,
    pub version: String,
}

/// Trigger a sync with connected peers.
/// For each peer that has a DWN endpoint, we pull their messages
/// and push our local messages, deduplicating by record_id.
pub async fn sync_with_peers(data_dir: &Path, peer_onions: &[String]) -> Result<DwnSyncState> {
    use crate::network::dwn_store::{DwnStore, MessageQuery};
    use std::collections::HashSet;

    let mut state = load_sync_state(data_dir).await?;
    state.status = SyncStatus::Syncing;
    save_sync_state(data_dir, &state).await?;

    let store = DwnStore::new(data_dir).await?;
    let mut synced_count = 0u64;

    // Get local messages since last sync (or all if first sync, capped at 200)
    let local_messages = store
        .query_messages(&MessageQuery {
            date_from: state.last_sync.clone(),
            limit: Some(200),
            ..Default::default()
        })
        .await?;

    // Deduplicate peer onion addresses
    let mut seen = HashSet::new();
    let unique_onions: Vec<&String> = peer_onions
        .iter()
        .filter(|o| !o.is_empty() && seen.insert(o.as_str().to_string()))
        .collect();

    debug!(
        peers = unique_onions.len(),
        local_msgs = local_messages.len(),
        "Starting DWN sync"
    );

    // Overall sync timeout: 90 seconds
    let sync_future = async {
        for onion in &unique_onions {
            let fips_npub = crate::federation::fips_npub_for_onion(data_dir, onion).await;
            match sync_single_peer(
                data_dir,
                fips_npub.as_deref(),
                &store,
                onion,
                &local_messages,
                &state.last_sync,
            )
            .await
            {
                Ok(count) => {
                    debug!(peer = %onion, messages = count, "Peer sync complete");
                    synced_count += count;
                }
                Err(e) => {
                    debug!(peer = %onion, error = %e, "Peer sync failed");
                }
            }
        }
    };

    match tokio::time::timeout(std::time::Duration::from_secs(90), sync_future).await {
        Ok(()) => {
            debug!(count = synced_count, "DWN sync complete");
        }
        Err(_) => {
            debug!("DWN sync timed out after 90s");
        }
    }

    state.status = SyncStatus::Synced;
    state.last_sync = Some(chrono::Utc::now().to_rfc3339());
    state.messages_synced += synced_count;
    save_sync_state(data_dir, &state).await?;

    Ok(state)
}

/// Sync with a single peer: pull their messages and push ours.
/// Each HTTP call picks FIPS when a npub is known, otherwise Tor.
async fn sync_single_peer(
    data_dir: &Path,
    fips_npub: Option<&str>,
    store: &crate::network::dwn_store::DwnStore,
    onion: &str,
    local_messages: &[crate::network::dwn_store::DwnMessage],
    last_sync: &Option<String>,
) -> Result<u64> {
    use crate::fips::dial::PeerRequest;
    let mut imported = 0u64;

    // Step 1: Check peer health
    let (health_resp, _) = PeerRequest::new(fips_npub, onion, "/dwn/health")
        .service(crate::settings::transport::PeerService::Federation)
        .timeout(std::time::Duration::from_secs(30))
        .fips_timeout(std::time::Duration::from_secs(6))
        .record_transport(data_dir)
        .send_get()
        .await
        .context("Peer DWN unreachable")?;
    if !health_resp.status().is_success() {
        return Err(anyhow::anyhow!("Peer DWN not healthy"));
    }

    // Step 2: Pull — query peer for messages since our last sync
    let mut query_filter = serde_json::json!({});
    if let Some(ref since) = last_sync {
        query_filter = serde_json::json!({ "dateSort": "createdAscending", "dateFrom": since });
    }
    let pull_body = serde_json::json!({
        "messages": [{
            "descriptor": {
                "interface": "Records",
                "method": "Query",
                "filter": query_filter,
            }
        }]
    });

    let (pull_res, _) = PeerRequest::new(fips_npub, onion, "/dwn")
        .service(crate::settings::transport::PeerService::Federation)
        .timeout(std::time::Duration::from_secs(30))
        .fips_timeout(std::time::Duration::from_secs(6))
        .record_transport(data_dir)
        .send_json(&pull_body)
        .await
        .context("Failed to query peer DWN")?;

    if pull_res.status().is_success() {
        let pull_data: serde_json::Value = pull_res.json().await.unwrap_or_default();
        if let Some(entries) = pull_data["entries"].as_array() {
            for entry in entries {
                let record_id = entry["record_id"].as_str().unwrap_or_default();
                if record_id.is_empty() {
                    continue;
                }
                // Skip if we already have this message
                if store.read_message(record_id).await?.is_some() {
                    continue;
                }
                // Import the message
                let author = entry["author"].as_str().unwrap_or("unknown");
                let protocol = entry["descriptor"]["protocol"].as_str();
                let schema = entry["descriptor"]["schema"].as_str();
                let data_format = entry["descriptor"]["dataFormat"].as_str();
                let data = entry.get("data").cloned();

                store
                    .write_message(author, protocol, schema, data_format, data)
                    .await?;
                imported += 1;
            }
        }
    }

    // Step 3: Push — send local messages to peer in batches
    let batch_size = 50;
    for chunk in local_messages.chunks(batch_size) {
        let messages: Vec<serde_json::Value> = chunk
            .iter()
            .map(|msg| {
                serde_json::json!({
                    "descriptor": {
                        "interface": "Records",
                        "method": "Write",
                        "protocol": msg.descriptor.protocol,
                        "schema": msg.descriptor.schema,
                        "dataFormat": msg.descriptor.data_format,
                    },
                    "recordId": msg.record_id,
                    "author": msg.author,
                    "data": msg.data,
                })
            })
            .collect();

        let push_body = serde_json::json!({ "messages": messages });

        // Best-effort push — don't fail the whole sync if a batch fails.
        match PeerRequest::new(fips_npub, onion, "/dwn")
            .service(crate::settings::transport::PeerService::Federation)
            .timeout(std::time::Duration::from_secs(30))
            .fips_timeout(std::time::Duration::from_secs(6))
            .record_transport(data_dir)
            .send_json(&push_body)
            .await
        {
            Ok((_, t)) => {
                debug!(count = chunk.len(), transport = %t, "Pushed message batch to peer");
            }
            Err(e) => {
                debug!(error = %e, "Failed to push message batch to peer");
            }
        }
    }

    Ok(imported)
}
