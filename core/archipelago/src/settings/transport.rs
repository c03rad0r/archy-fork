//! Per-service transport preferences.
//!
//! The user picks, for each peer-to-peer service, whether to prefer
//! FIPS (mesh overlay), Tor (hidden service fallback), or leave it on
//! Auto (FIPS preferred, Tor fallback — the default). Preferences are
//! persisted to `<data_dir>/settings/transport_preferences.json` and
//! cached in a process-wide handle so that calls from the transport,
//! RPC, and ingest stacks can consult them without threading data_dir
//! through every signature.
//!
//! Services covered (matching the Settings UI):
//!   - `federation`       — state sync, invites, peer notifications
//!   - `peers`            — address/DID rotation broadcast
//!   - `peer_files`       — content download / browse / preview
//!   - `messaging`        — archipelago channel + mesh-typed relay
//!   - `mesh_file_sharing`— content_ref blob fetches over onion
//!
//! Unknown files parse as `default()` so a missing or corrupt
//! preferences file is equivalent to "Auto everywhere."

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::OnceLock;
use tokio::sync::RwLock;

const FILE_PATH: &str = "settings/transport_preferences.json";

/// Which transport to use for a given service.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransportPref {
    /// FIPS preferred, Tor fallback. The default.
    #[default]
    Auto,
    /// FIPS only. If FIPS is unavailable or the peer has no npub,
    /// requests fail rather than leaking over Tor.
    Fips,
    /// Tor only. Useful when the user explicitly wants onion anonymity
    /// for a given surface (e.g., first-contact messaging).
    Tor,
}

/// Enum of peer-facing services that have a preference knob.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerService {
    Federation,
    Peers,
    PeerFiles,
    Messaging,
    MeshFileSharing,
}

impl PeerService {
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Federation => "federation",
            Self::Peers => "peers",
            Self::PeerFiles => "peer_files",
            Self::Messaging => "messaging",
            Self::MeshFileSharing => "mesh_file_sharing",
        }
    }
}

/// Persisted shape. One field per service so the on-disk file is
/// self-describing and trivially diff-able.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TransportPreferences {
    pub federation: TransportPref,
    pub peers: TransportPref,
    pub peer_files: TransportPref,
    pub messaging: TransportPref,
    pub mesh_file_sharing: TransportPref,
}

impl TransportPreferences {
    pub fn for_service(&self, s: PeerService) -> TransportPref {
        match s {
            PeerService::Federation => self.federation,
            PeerService::Peers => self.peers,
            PeerService::PeerFiles => self.peer_files,
            PeerService::Messaging => self.messaging,
            PeerService::MeshFileSharing => self.mesh_file_sharing,
        }
    }

    pub fn set_for_service(&mut self, s: PeerService, pref: TransportPref) {
        match s {
            PeerService::Federation => self.federation = pref,
            PeerService::Peers => self.peers = pref,
            PeerService::PeerFiles => self.peer_files = pref,
            PeerService::Messaging => self.messaging = pref,
            PeerService::MeshFileSharing => self.mesh_file_sharing = pref,
        }
    }
}

// ── Process-wide handle ─────────────────────────────────────────────────

static HANDLE: OnceLock<RwLock<TransportPreferences>> = OnceLock::new();

/// Initialise the handle from `<data_dir>/settings/transport_preferences.json`.
/// Must be called early in startup — before any call that reads `get()`.
/// Idempotent: second call reloads.
pub async fn init(data_dir: &Path) -> Result<()> {
    let prefs = load_from_disk(data_dir).await;
    match HANDLE.get() {
        Some(lock) => {
            *lock.write().await = prefs;
        }
        None => {
            HANDLE
                .set(RwLock::new(prefs))
                .map_err(|_| anyhow::anyhow!("transport prefs already initialised"))?;
        }
    }
    Ok(())
}

/// Read the current preference for a service. Returns `Auto` if the
/// handle wasn't initialised (tests, fallbacks).
pub async fn get(service: PeerService) -> TransportPref {
    match HANDLE.get() {
        Some(lock) => lock.read().await.for_service(service),
        None => TransportPref::Auto,
    }
}

/// Read the whole preferences block for the Settings UI.
pub async fn snapshot() -> TransportPreferences {
    match HANDLE.get() {
        Some(lock) => lock.read().await.clone(),
        None => TransportPreferences::default(),
    }
}

/// Update a single service preference, persist to disk, and update the
/// handle. Callers must pass `data_dir` because the on-disk file lives
/// under it — the handle alone doesn't know where to write.
pub async fn set(data_dir: &Path, service: PeerService, pref: TransportPref) -> Result<()> {
    let new_prefs = {
        let lock = HANDLE.get_or_init(|| RwLock::new(TransportPreferences::default()));
        let mut w = lock.write().await;
        w.set_for_service(service, pref);
        w.clone()
    };
    save_to_disk(data_dir, &new_prefs).await
}

// ── On-disk I/O ─────────────────────────────────────────────────────────

async fn load_from_disk(data_dir: &Path) -> TransportPreferences {
    let path = data_dir.join(FILE_PATH);
    let s = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(_) => return TransportPreferences::default(),
    };
    serde_json::from_str(&s).unwrap_or_default()
}

async fn save_to_disk(data_dir: &Path, prefs: &TransportPreferences) -> Result<()> {
    let path = data_dir.join(FILE_PATH);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("create {}", parent.display()))?;
    }
    let body = serde_json::to_string_pretty(prefs).context("serialize TransportPreferences")?;
    tokio::fs::write(&path, body)
        .await
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_all_auto() {
        let p = TransportPreferences::default();
        for s in [
            PeerService::Federation,
            PeerService::Peers,
            PeerService::PeerFiles,
            PeerService::Messaging,
            PeerService::MeshFileSharing,
        ] {
            assert_eq!(p.for_service(s), TransportPref::Auto);
        }
    }

    #[test]
    fn set_then_read_for_service() {
        let mut p = TransportPreferences::default();
        p.set_for_service(PeerService::Federation, TransportPref::Fips);
        assert_eq!(p.for_service(PeerService::Federation), TransportPref::Fips);
        assert_eq!(p.for_service(PeerService::Peers), TransportPref::Auto);
    }

    #[test]
    fn json_round_trips() {
        let mut p = TransportPreferences::default();
        p.set_for_service(PeerService::Messaging, TransportPref::Tor);
        let s = serde_json::to_string(&p).unwrap();
        let back: TransportPreferences = serde_json::from_str(&s).unwrap();
        assert_eq!(back.for_service(PeerService::Messaging), TransportPref::Tor);
    }

    #[test]
    fn service_names_round_trip() {
        for s in [
            PeerService::Federation,
            PeerService::Peers,
            PeerService::PeerFiles,
            PeerService::Messaging,
            PeerService::MeshFileSharing,
        ] {
            let json = serde_json::to_value(s).unwrap();
            assert_eq!(json.as_str().unwrap(), s.as_str());
        }
    }
}
