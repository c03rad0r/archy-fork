// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! CBOR delta encoding for federation state sync.
//!
//! Instead of sending a full NodeStateSnapshot (~500-2000 bytes JSON) on every
//! sync cycle, we compute a delta of only changed fields and encode it as CBOR.
//! A typical delta (CPU + memory change) is ~30-50 bytes — small enough to fit
//! in a single LoRa chunk after encryption.

use crate::federation::{AppStatus, NodeStateSnapshot};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Delta format version. Increment when fields change.
const DELTA_VERSION: u8 = 1;

/// Compact state delta — only changed fields, with short field names.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StateDelta {
    /// Timestamp of the snapshot this delta represents.
    pub ts: String,
    /// Delta format version for forward compatibility.
    pub v: u8,
    /// Apps that changed status (full entry for each changed app).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apps: Option<Vec<AppStatus>>,
    /// App IDs that were removed since last sync.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apps_rm: Option<Vec<String>>,
    /// CPU usage percent (only if changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<f64>,
    /// Memory used bytes (only if changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_u: Option<u64>,
    /// Memory total bytes (only if changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_t: Option<u64>,
    /// Disk used bytes (only if changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dsk_u: Option<u64>,
    /// Disk total bytes (only if changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dsk_t: Option<u64>,
    /// Uptime seconds (only if changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub up: Option<u64>,
    /// Tor active flag (only if changed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tor: Option<bool>,
    /// Shared location (only if changed) — same "None means unchanged"
    /// convention as the other scalar fields here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lat: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lon: Option<f64>,
}

/// Compute the delta between two state snapshots.
/// Returns only the fields that differ.
pub fn compute_delta(prev: &NodeStateSnapshot, curr: &NodeStateSnapshot) -> StateDelta {
    let mut delta = StateDelta {
        ts: curr.timestamp.clone(),
        v: DELTA_VERSION,
        ..Default::default()
    };

    // Compare apps
    let prev_apps: std::collections::HashMap<&str, &AppStatus> =
        prev.apps.iter().map(|a| (a.id.as_str(), a)).collect();
    let curr_apps: std::collections::HashMap<&str, &AppStatus> =
        curr.apps.iter().map(|a| (a.id.as_str(), a)).collect();

    let mut changed_apps = Vec::new();
    let mut removed_apps = Vec::new();

    for (id, curr_app) in &curr_apps {
        match prev_apps.get(id) {
            Some(prev_app) => {
                if prev_app.status != curr_app.status || prev_app.version != curr_app.version {
                    changed_apps.push((*curr_app).clone());
                }
            }
            None => changed_apps.push((*curr_app).clone()),
        }
    }

    for id in prev_apps.keys() {
        if !curr_apps.contains_key(id) {
            removed_apps.push(id.to_string());
        }
    }

    if !changed_apps.is_empty() {
        delta.apps = Some(changed_apps);
    }
    if !removed_apps.is_empty() {
        delta.apps_rm = Some(removed_apps);
    }

    // Compare scalar fields
    if curr.cpu_usage_percent != prev.cpu_usage_percent {
        delta.cpu = curr.cpu_usage_percent;
    }
    if curr.mem_used_bytes != prev.mem_used_bytes {
        delta.mem_u = curr.mem_used_bytes;
    }
    if curr.mem_total_bytes != prev.mem_total_bytes {
        delta.mem_t = curr.mem_total_bytes;
    }
    if curr.disk_used_bytes != prev.disk_used_bytes {
        delta.dsk_u = curr.disk_used_bytes;
    }
    if curr.disk_total_bytes != prev.disk_total_bytes {
        delta.dsk_t = curr.disk_total_bytes;
    }
    if curr.uptime_secs != prev.uptime_secs {
        delta.up = curr.uptime_secs;
    }
    if curr.tor_active != prev.tor_active {
        delta.tor = curr.tor_active;
    }
    if curr.lat != prev.lat {
        delta.lat = curr.lat;
    }
    if curr.lon != prev.lon {
        delta.lon = curr.lon;
    }

    delta
}

/// Apply a delta to a base snapshot, producing an updated snapshot.
pub fn apply_delta(base: &NodeStateSnapshot, delta: &StateDelta) -> NodeStateSnapshot {
    let mut result = base.clone();
    result.timestamp = delta.ts.clone();

    // Apply app changes
    if let Some(changed) = &delta.apps {
        for app in changed {
            if let Some(existing) = result.apps.iter_mut().find(|a| a.id == app.id) {
                existing.status = app.status.clone();
                existing.version = app.version.clone();
            } else {
                result.apps.push(app.clone());
            }
        }
    }

    // Apply app removals
    if let Some(removed) = &delta.apps_rm {
        result.apps.retain(|a| !removed.contains(&a.id));
    }

    // Apply scalar fields
    if let Some(cpu) = delta.cpu {
        result.cpu_usage_percent = Some(cpu);
    }
    if let Some(mem_u) = delta.mem_u {
        result.mem_used_bytes = Some(mem_u);
    }
    if let Some(mem_t) = delta.mem_t {
        result.mem_total_bytes = Some(mem_t);
    }
    if let Some(dsk_u) = delta.dsk_u {
        result.disk_used_bytes = Some(dsk_u);
    }
    if let Some(dsk_t) = delta.dsk_t {
        result.disk_total_bytes = Some(dsk_t);
    }
    if let Some(up) = delta.up {
        result.uptime_secs = Some(up);
    }
    if let Some(tor) = delta.tor {
        result.tor_active = Some(tor);
    }
    if let Some(lat) = delta.lat {
        result.lat = Some(lat);
    }
    if let Some(lon) = delta.lon {
        result.lon = Some(lon);
    }

    result
}

/// Encode a delta as CBOR bytes.
pub fn encode_cbor(delta: &StateDelta) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    ciborium::into_writer(delta, &mut buf).context("CBOR encode failed")?;
    Ok(buf)
}

/// Decode a delta from CBOR bytes.
pub fn decode_cbor(data: &[u8]) -> Result<StateDelta> {
    ciborium::from_reader(data).context("CBOR decode failed")
}

/// Encode a full state snapshot as CBOR (for initial sync or Tor transport).
pub fn encode_snapshot_cbor(snapshot: &NodeStateSnapshot) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    ciborium::into_writer(snapshot, &mut buf).context("CBOR snapshot encode failed")?;
    Ok(buf)
}

/// Decode a full state snapshot from CBOR.
pub fn decode_snapshot_cbor(data: &[u8]) -> Result<NodeStateSnapshot> {
    ciborium::from_reader(data).context("CBOR snapshot decode failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot_a() -> NodeStateSnapshot {
        NodeStateSnapshot {
            timestamp: "2026-03-16T12:00:00Z".to_string(),
            node_name: Some("Test Node A".to_string()),
            apps: vec![
                AppStatus {
                    id: "bitcoin-knots".to_string(),
                    status: "running".to_string(),
                    version: Some("27.1".to_string()),
                },
                AppStatus {
                    id: "lnd".to_string(),
                    status: "running".to_string(),
                    version: Some("0.18.0".to_string()),
                },
                AppStatus {
                    id: "mempool".to_string(),
                    status: "stopped".to_string(),
                    version: Some("3.0".to_string()),
                },
            ],
            cpu_usage_percent: Some(23.5),
            mem_used_bytes: Some(4_000_000_000),
            mem_total_bytes: Some(16_000_000_000),
            disk_used_bytes: Some(500_000_000_000),
            disk_total_bytes: Some(1_800_000_000_000),
            uptime_secs: Some(86400),
            tor_active: Some(true),
            nostr_npub: None,
            own_fips_npub: None,
            federated_peers: Vec::new(),
            lat: None,
            lon: None,
        }
    }

    fn sample_snapshot_b() -> NodeStateSnapshot {
        NodeStateSnapshot {
            timestamp: "2026-03-16T12:05:00Z".to_string(),
            node_name: Some("Test Node A".to_string()),
            apps: vec![
                AppStatus {
                    id: "bitcoin-knots".to_string(),
                    status: "running".to_string(),
                    version: Some("27.1".to_string()),
                },
                AppStatus {
                    id: "lnd".to_string(),
                    status: "running".to_string(),
                    version: Some("0.18.0".to_string()),
                },
                AppStatus {
                    id: "mempool".to_string(),
                    status: "running".to_string(), // Changed: stopped -> running
                    version: Some("3.0".to_string()),
                },
            ],
            cpu_usage_percent: Some(35.2),       // Changed
            mem_used_bytes: Some(4_500_000_000), // Changed
            mem_total_bytes: Some(16_000_000_000),
            disk_used_bytes: Some(500_000_000_000),
            disk_total_bytes: Some(1_800_000_000_000),
            uptime_secs: Some(86700), // Changed
            tor_active: Some(true),
            nostr_npub: None,
            own_fips_npub: None,
            federated_peers: Vec::new(),
            lat: None,
            lon: None,
        }
    }

    #[test]
    fn test_compute_delta_detects_changes() {
        let a = sample_snapshot_a();
        let b = sample_snapshot_b();
        let delta = compute_delta(&a, &b);

        assert_eq!(delta.v, DELTA_VERSION);
        assert_eq!(delta.ts, "2026-03-16T12:05:00Z");

        // Mempool status changed
        assert!(delta.apps.is_some());
        let apps = delta.apps.as_ref().unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].id, "mempool");
        assert_eq!(apps[0].status, "running");

        // No apps removed
        assert!(delta.apps_rm.is_none());

        // Scalar changes
        assert_eq!(delta.cpu, Some(35.2));
        assert_eq!(delta.mem_u, Some(4_500_000_000));
        assert_eq!(delta.up, Some(86700));

        // Unchanged fields should be None
        assert!(delta.mem_t.is_none());
        assert!(delta.dsk_u.is_none());
        assert!(delta.dsk_t.is_none());
        assert!(delta.tor.is_none());
    }

    #[test]
    fn test_apply_delta_reconstructs() {
        let a = sample_snapshot_a();
        let b = sample_snapshot_b();
        let delta = compute_delta(&a, &b);
        let reconstructed = apply_delta(&a, &delta);

        assert_eq!(reconstructed.timestamp, b.timestamp);
        assert_eq!(reconstructed.cpu_usage_percent, b.cpu_usage_percent);
        assert_eq!(reconstructed.mem_used_bytes, b.mem_used_bytes);
        assert_eq!(reconstructed.uptime_secs, b.uptime_secs);

        // Check mempool status was updated
        let mempool = reconstructed
            .apps
            .iter()
            .find(|a| a.id == "mempool")
            .unwrap();
        assert_eq!(mempool.status, "running");
    }

    #[test]
    fn test_delta_with_app_removal() {
        let a = sample_snapshot_a();
        let mut b = sample_snapshot_b();
        // Remove mempool from b
        b.apps.retain(|app| app.id != "mempool");

        let delta = compute_delta(&a, &b);
        assert!(delta.apps_rm.is_some());
        assert_eq!(delta.apps_rm.as_ref().unwrap(), &["mempool".to_string()]);

        let reconstructed = apply_delta(&a, &delta);
        assert!(reconstructed.apps.iter().all(|a| a.id != "mempool"));
    }

    #[test]
    fn test_delta_with_new_app() {
        let a = sample_snapshot_a();
        let mut b = sample_snapshot_b();
        b.apps.push(AppStatus {
            id: "electrs".to_string(),
            status: "running".to_string(),
            version: Some("0.10.0".to_string()),
        });

        let delta = compute_delta(&a, &b);
        let apps = delta.apps.as_ref().unwrap();
        assert!(apps.iter().any(|a| a.id == "electrs"));

        let reconstructed = apply_delta(&a, &delta);
        assert!(reconstructed.apps.iter().any(|a| a.id == "electrs"));
    }

    #[test]
    fn test_cbor_roundtrip() {
        let a = sample_snapshot_a();
        let b = sample_snapshot_b();
        let delta = compute_delta(&a, &b);

        let encoded = encode_cbor(&delta).unwrap();
        let decoded = decode_cbor(&encoded).unwrap();

        assert_eq!(decoded.ts, delta.ts);
        assert_eq!(decoded.cpu, delta.cpu);
        assert_eq!(decoded.mem_u, delta.mem_u);
        assert_eq!(decoded.up, delta.up);
    }

    #[test]
    fn test_cbor_size_vs_json() {
        let a = sample_snapshot_a();
        let b = sample_snapshot_b();
        let delta = compute_delta(&a, &b);

        let cbor_bytes = encode_cbor(&delta).unwrap();
        let json_bytes = serde_json::to_vec(&b).unwrap();

        // CBOR delta should be significantly smaller than full JSON snapshot
        assert!(
            cbor_bytes.len() < json_bytes.len(),
            "CBOR delta ({} bytes) should be smaller than full JSON ({} bytes)",
            cbor_bytes.len(),
            json_bytes.len()
        );
    }

    #[test]
    fn test_snapshot_cbor_roundtrip() {
        let snapshot = sample_snapshot_a();
        let encoded = encode_snapshot_cbor(&snapshot).unwrap();
        let decoded = decode_snapshot_cbor(&encoded).unwrap();

        assert_eq!(decoded.timestamp, snapshot.timestamp);
        assert_eq!(decoded.apps.len(), snapshot.apps.len());
        assert_eq!(decoded.cpu_usage_percent, snapshot.cpu_usage_percent);
    }

    #[test]
    fn test_no_changes_produces_minimal_delta() {
        let a = sample_snapshot_a();
        let mut b = a.clone();
        b.timestamp = "2026-03-16T12:01:00Z".to_string();

        let delta = compute_delta(&a, &b);

        // Only timestamp should differ
        assert!(delta.apps.is_none());
        assert!(delta.apps_rm.is_none());
        assert!(delta.cpu.is_none());
        assert!(delta.mem_u.is_none());
        assert!(delta.tor.is_none());

        let cbor_bytes = encode_cbor(&delta).unwrap();
        // Minimal delta should be very small (just timestamp + version)
        assert!(
            cbor_bytes.len() < 50,
            "Minimal delta should be <50 bytes, got {}",
            cbor_bytes.len()
        );
    }
}
