use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The main data model that mirrors the frontend's DataModel type.
/// This is sent via WebSocket as the initial state and updated via patches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DataModel {
    #[serde(rename = "server-info")]
    pub server_info: ServerInfo,
    #[serde(rename = "package-data")]
    pub package_data: HashMap<String, PackageDataEntry>,
    #[serde(
        rename = "peer-health",
        default,
        skip_serializing_if = "HashMap::is_empty"
    )]
    pub peer_health: HashMap<String, bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notifications: Vec<Notification>,
    pub ui: UIData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Notification {
    pub id: String,
    pub level: NotificationLevel,
    pub title: String,
    pub message: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerInfo {
    pub id: String,
    pub version: String,
    pub name: Option<String>,
    pub pubkey: String,
    #[serde(rename = "status-info")]
    pub status_info: StatusInfo,
    #[serde(rename = "lan-address")]
    pub lan_address: Option<String>,
    #[serde(rename = "tor-address")]
    pub tor_address: Option<String>,
    /// Is the Tor daemon actually answering — NOT "was an onion provisioned".
    ///
    /// `tor_address` is read from the hidden-service hostname file on disk and
    /// survives Tor being dead, so the dashboard's Network card reported
    /// "Connected" on three fleet nodes whose Tor had been down for days
    /// (2026-08-09). Liveness has to come from a probe; the address is a
    /// separate fact and must not be used as a proxy for it.
    #[serde(rename = "tor-running")]
    pub tor_running: bool,
    #[serde(rename = "node-address", skip_serializing_if = "Option::is_none")]
    pub node_address: Option<String>,
    pub unread: u32,
    #[serde(rename = "wifi-ssids")]
    pub wifi_ssids: Vec<String>,
    #[serde(rename = "zram-enabled")]
    pub zram_enabled: bool,
    /// True if this node's keys are derived from a BIP-39 seed.
    #[serde(rename = "seed-backed", default)]
    pub seed_backed: bool,
    /// This node's own physical location, for the Mesh Map — opt-in only
    /// (see `share_location`), set via `server.set-location`. `None` until
    /// the user sets one, regardless of `share_location`.
    #[serde(default)]
    pub lat: Option<f64>,
    #[serde(default)]
    pub lon: Option<f64>,
    /// Whether `lat`/`lon` should be included in the state snapshot we send
    /// to trusted federation peers (so they can plot us on their Mesh Map).
    /// Defaults to false — never shared unless explicitly turned on.
    #[serde(rename = "share-location", default)]
    pub share_location: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusInfo {
    pub restarting: bool,
    #[serde(rename = "shutting-down")]
    pub shutting_down: bool,
    pub updated: bool,
    #[serde(rename = "backup-progress")]
    pub backup_progress: Option<f32>,
    #[serde(rename = "update-progress")]
    pub update_progress: Option<f32>,
    /// True after the first container scan completes. Frontend should
    /// not show install buttons until this is true.
    #[serde(rename = "containers-scanned", default)]
    pub containers_scanned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UIData {
    pub name: Option<String>,
    #[serde(rename = "ack-welcome")]
    pub ack_welcome: String,
    pub marketplace: UIMarketplaceData,
    pub theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UIMarketplaceData {
    #[serde(rename = "selected-hosts")]
    pub selected_hosts: Vec<String>,
    #[serde(rename = "known-hosts")]
    pub known_hosts: HashMap<String, MarketplaceHost>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketplaceHost {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum PackageState {
    Installing,
    Installed,
    Stopping,
    Stopped,
    Exited,
    Starting,
    Running,
    Restarting,
    #[serde(rename = "creating-backup")]
    CreatingBackup,
    #[serde(rename = "restoring-backup")]
    RestoringBackup,
    Removing,
    #[serde(rename = "backing-up")]
    BackingUp,
    Updating,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackageDataEntry {
    /// Whether the app's HTTP upstream answered this scan (independent of
    /// container health and blockchain sync). Missing on older nodes.
    #[serde(rename = "ui-ready", default, skip_serializing_if = "Option::is_none")]
    pub ui_ready: Option<bool>,
    pub state: PackageState,
    /// Container health: "healthy", "unhealthy", "starting", or null
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<String>,
    /// Container exit code (only set when state is Exited): 0 = clean, non-zero = crash
    #[serde(rename = "exit-code", skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(rename = "static-files")]
    pub static_files: StaticFiles,
    pub manifest: Manifest,
    pub installed: Option<InstalledPackageDataEntry>,
    #[serde(rename = "install-progress")]
    pub install_progress: Option<InstallProgress>,
    /// Live label describing the current uninstall step ("Stopping
    /// containers (2/5)", "Removing data", …). Set by the uninstall
    /// pipeline so the UI can show real progress instead of a generic
    /// "Uninstalling…" spinner. Cleared after the package entry is
    /// removed.
    #[serde(
        rename = "uninstall-stage",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub uninstall_stage: Option<String>,
    /// Pinned image version from image-versions.sh when it differs from running version
    #[serde(rename = "available-update", skip_serializing_if = "Option::is_none")]
    pub available_update: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StaticFiles {
    pub license: String,
    pub instructions: String,
    pub icon: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub title: String,
    pub version: String,
    pub description: Description,
    #[serde(rename = "release-notes")]
    pub release_notes: String,
    pub license: String,
    #[serde(rename = "wrapper-repo")]
    pub wrapper_repo: String,
    #[serde(rename = "upstream-repo")]
    pub upstream_repo: String,
    #[serde(rename = "support-site")]
    pub support_site: String,
    #[serde(rename = "marketing-site")]
    pub marketing_site: String,
    #[serde(rename = "donation-url")]
    pub donation_url: Option<String>,
    pub author: Option<String>,
    pub website: Option<String>,
    pub interfaces: Option<Interfaces>,
    /// App tier: "core", "recommended", or "optional"
    #[serde(default)]
    pub tier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Description {
    pub short: String,
    pub long: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Interfaces {
    pub main: Option<MainInterface>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MainInterface {
    pub ui: Option<String>,
    #[serde(rename = "tor-config")]
    pub tor_config: Option<String>,
    #[serde(rename = "lan-config")]
    pub lan_config: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InstalledPackageDataEntry {
    #[serde(rename = "current-dependents")]
    pub current_dependents: HashMap<String, CurrentDependencyInfo>,
    #[serde(rename = "current-dependencies")]
    pub current_dependencies: HashMap<String, CurrentDependencyInfo>,
    #[serde(rename = "last-backup")]
    pub last_backup: Option<String>,
    #[serde(rename = "interface-addresses")]
    pub interface_addresses: HashMap<String, InterfaceAddress>,
    pub status: ServiceStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CurrentDependencyInfo {
    #[serde(rename = "health-checks")]
    pub health_checks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InterfaceAddress {
    #[serde(rename = "tor-address")]
    pub tor_address: String,
    #[serde(rename = "lan-address")]
    pub lan_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Restarting,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InstallProgress {
    pub size: u64,
    pub downloaded: u64,
    /// High-level pipeline phase. Preferred by the UI over the byte
    /// counters (podman pull doesn't emit parseable progress on a piped
    /// stderr, so `size`/`downloaded` are often 0). Each phase maps to
    /// a fixed UI percentage and a descriptive label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<InstallPhase>,
    /// Optional explicit message — used to surface install failures so
    /// the UI can keep the app card visible with an error description
    /// instead of silently removing the entry on fail. UI's PHASE_INFO
    /// label takes precedence when phase is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Phases of the install / update pipeline, surfaced to the UI so users
/// see where the pipeline is rather than a stuck 0% bar.
///
/// Ordered so each variant's index roughly corresponds to pipeline time.
/// Serialized as kebab-case: "preparing", "pulling-image", …
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum InstallPhase {
    /// Validating params, checking deps, writing dynamic configs.
    Preparing,
    /// `podman pull` in progress (the longest phase — up to several
    /// minutes for large images on slow networks).
    PullingImage,
    /// Orchestrator owns download/build and startup as one operation.
    PreparingApp,
    /// Creating data directories, writing app-specific configs
    /// (bitcoin.conf, lnd.conf, searxng settings.yml, chown).
    CreatingContainer,
    /// `podman run` has returned; container is transitioning to running.
    StartingContainer,
    /// Post-start loop waiting up to 60s for `State.Status == running`.
    WaitingHealthy,
    /// Running post-install hooks (chain init, wallet setup, …).
    PostInstall,
    /// Pipeline finished successfully. Terminal phase, UI clears entry.
    Done,
}

/// WebSocket message sent to clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketMessage {
    pub rev: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<DataModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<Vec<PatchOperation>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchOperation {
    pub op: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

impl DataModel {
    /// Read build version from /opt/archipelago/build-info.txt if available,
    /// falling back to Cargo.toml version. This allows sequential CI build
    /// numbers to be reflected in the UI without recompiling the binary.
    fn detect_build_version() -> String {
        // Always use the binary's compiled-in version. The ISO installer
        // writes /opt/archipelago/build-info.txt at install time, but that
        // file is never rewritten by OTA or sideload, so trusting it made
        // the sidebar permanently advertise whatever the ISO shipped with
        // even after the running binary had moved on. CARGO_PKG_VERSION is
        // baked into the binary at compile time, so it always matches what
        // is actually running.
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// Create a new empty data model with default values
    pub fn new() -> Self {
        Self {
            server_info: ServerInfo {
                id: uuid::Uuid::new_v4().to_string(),
                version: Self::detect_build_version(),
                name: Some("Archipelago".to_string()),
                pubkey: String::new(),
                status_info: StatusInfo {
                    restarting: false,
                    shutting_down: false,
                    updated: false,
                    backup_progress: None,
                    update_progress: None,
                    containers_scanned: false,
                },
                lan_address: Some("http://localhost:8100".to_string()),
                tor_address: None,
                tor_running: false,
                node_address: None,
                unread: 0,
                wifi_ssids: vec![],
                zram_enabled: false,
                seed_backed: false,
                lat: None,
                lon: None,
                share_location: false,
            },
            package_data: HashMap::new(),
            peer_health: HashMap::new(),
            notifications: Vec::new(),
            ui: UIData {
                name: None,
                ack_welcome: String::new(),
                marketplace: UIMarketplaceData {
                    selected_hosts: vec![],
                    known_hosts: HashMap::new(),
                },
                theme: "dark".to_string(),
            },
        }
    }
}

impl Default for DataModel {
    fn default() -> Self {
        Self::new()
    }
}
