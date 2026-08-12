use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContainerRuntime {
    Podman,
    Docker,
    Auto,
}

impl ContainerRuntime {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "podman" => ContainerRuntime::Podman,
            "docker" => ContainerRuntime::Docker,
            "auto" | _ => ContainerRuntime::Auto,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BitcoinSimulation {
    Mock,
    Testnet,
    Mainnet,
    None,
}

impl BitcoinSimulation {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "mock" => BitcoinSimulation::Mock,
            "testnet" => BitcoinSimulation::Testnet,
            "mainnet" => BitcoinSimulation::Mainnet,
            "none" | _ => BitcoinSimulation::None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub data_dir: PathBuf,
    pub bind_host: String,
    pub bind_port: u16,
    pub log_level: String,
    /// Host IP for container env vars (FM_API_URL, BACKEND_MAINNET_HTTP_HOST, etc.)
    pub host_ip: String,
    // Dev mode configuration
    pub dev_mode: bool,
    pub container_runtime: ContainerRuntime,
    pub port_offset: u16,
    pub bitcoin_simulation: BitcoinSimulation,
    pub dev_data_dir: PathBuf,
    /// Nostr discovery: opt-in only. When true + relays non-empty, publish node to relays.
    #[serde(default)]
    pub nostr_discovery_enabled: bool,
    /// Nostr relay URLs (comma-separated). Only used when nostr_discovery_enabled.
    #[serde(default)]
    pub nostr_relays: Vec<String>,
    /// Tor SOCKS5 proxy (e.g. 127.0.0.1:9050). When set, ALL Nostr traffic routes through Tor.
    #[serde(default)]
    pub nostr_tor_proxy: Option<String>,
    /// Phase 3.2 of v1.7.52: route orchestrator-managed backend installs
    /// through Quadlet (`.container` units in ~/.config/containers/systemd
    /// + systemctl --user start) instead of `podman create + start`. Default
    /// off so the legacy path stays the production path until the harness
    /// at tests/lifecycle/run-gate.sh has gone green against the new path
    /// on .228 + .198. See `project_v1_7_52_phase3_quadlet_design`.
    #[serde(default)]
    pub use_quadlet_backends: bool,
    /// DHT swarm-assist (Phase 3): when true AND the binary was built with the
    /// `iroh-swarm` feature, stand up an iroh-blobs provider that fetches release
    /// blobs peer-to-peer (origin always wins) and seeds them via signed Nostr
    /// adverts. Off by default; with the feature absent this is inert. Reuses
    /// `nostr_relays` + `nostr_tor_proxy` for discovery transport.
    #[serde(default)]
    pub swarm_enabled: bool,
}

impl Config {
    /// Detect primary host IP (default-route interface, not `hostname -I` order)
    async fn detect_host_ip() -> Result<String> {
        Ok(crate::host_ip::primary_host_ipv4()
            .await
            .unwrap_or_else(|| "127.0.0.1".to_string()))
    }

    pub async fn load() -> Result<Self> {
        // Default configuration
        let mut config = Self::default();

        // Detect if running from macOS app bundle
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_str) = exe_path.to_str() {
                if exe_str.contains(".app/Contents/MacOS") {
                    // Running from macOS bundle - use user's Library directory
                    if let Some(home) = std::env::var_os("HOME") {
                        let app_support =
                            PathBuf::from(home).join("Library/Application Support/Archipelago");
                        config.data_dir = app_support.join("data");
                        config.dev_data_dir = app_support.join("data");
                        tracing::info!(
                            "🍎 Detected macOS bundle, using: {}",
                            app_support.display()
                        );
                    }
                }
            }
        }

        // Try to load from config file
        let config_path = Path::new("/etc/archipelago/config.toml");
        if config_path.exists() {
            let content = fs::read_to_string(config_path)
                .await
                .context("Failed to read config file")?;
            let file_config: Config =
                toml::de::from_str(&content).context("Failed to parse config file")?;
            config = file_config;
        }

        // Override with environment variables
        if let Ok(data_dir) = std::env::var("ARCHIPELAGO_DATA_DIR") {
            config.data_dir = PathBuf::from(data_dir);
        }

        if let Ok(bind) = std::env::var("ARCHIPELAGO_BIND") {
            let parts: Vec<&str> = bind.split(':').collect();
            if parts.len() == 2 {
                config.bind_host = parts[0].to_string();
                config.bind_port = parts[1]
                    .parse()
                    .context("Invalid port in ARCHIPELAGO_BIND")?;
            }
        }

        if let Ok(level) = std::env::var("ARCHIPELAGO_LOG_LEVEL") {
            config.log_level = level;
        }

        // Production binaries must not be switched into dev orchestration by
        // host environment. Several live nodes carried a stale systemd
        // ARCHIPELAGO_DEV_MODE override, which rewrote production volume
        // mounts into /tmp and prevented real installs from starting.
        if std::env::var("ARCHIPELAGO_DEV_MODE").is_ok() {
            tracing::warn!("Ignoring ARCHIPELAGO_DEV_MODE in production config");
        }

        if let Ok(runtime) = std::env::var("ARCHIPELAGO_CONTAINER_RUNTIME") {
            config.container_runtime = ContainerRuntime::from_str(&runtime);
        }

        if let Ok(offset) = std::env::var("ARCHIPELAGO_PORT_OFFSET") {
            config.port_offset = offset
                .parse()
                .context("Invalid port offset in ARCHIPELAGO_PORT_OFFSET")?;
        }

        if let Ok(sim) = std::env::var("ARCHIPELAGO_BITCOIN_SIMULATION") {
            config.bitcoin_simulation = BitcoinSimulation::from_str(&sim);
        }

        if let Ok(dev_data_dir) = std::env::var("ARCHIPELAGO_DEV_DATA_DIR") {
            config.dev_data_dir = PathBuf::from(dev_data_dir);
        }

        // Nostr discovery (opt-in, secure by default)
        if let Ok(v) = std::env::var("ARCHIPELAGO_NOSTR_DISCOVERY_ENABLED") {
            config.nostr_discovery_enabled = v.parse().unwrap_or(false);
        }
        if let Ok(v) = std::env::var("ARCHIPELAGO_NOSTR_RELAYS") {
            config.nostr_relays = v
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if let Ok(v) = std::env::var("ARCHIPELAGO_NOSTR_TOR_PROXY") {
            let s = v.trim().to_string();
            config.nostr_tor_proxy = if s.is_empty() { None } else { Some(s) };
        }

        // DHT swarm-assist (Phase 3). Opt-in: only takes effect when the binary
        // was also built with the `iroh-swarm` feature; otherwise inert.
        if let Ok(v) = std::env::var("ARCHIPELAGO_SWARM_ENABLED") {
            config.swarm_enabled = parse_truthy_env(&v);
        }

        // Phase 3.2 of v1.7.52. Truthy values (1, true, yes, on — case-insensitive)
        // route backend installs through the Quadlet path without requiring a
        // config.json edit + archipelago.service restart (which would trigger
        // FM3 cgroup cascade until 3.5 ships). Anything else (or unset) leaves
        // the config.json value untouched.
        if let Ok(v) = std::env::var("ARCHIPELAGO_USE_QUADLET_BACKENDS") {
            if parse_truthy_env(&v) {
                config.use_quadlet_backends = true;
            }
        }

        // Host IP for container env vars (detect if not set)
        if let Ok(ip) = std::env::var("ARCHIPELAGO_HOST_IP") {
            config.host_ip = ip;
        } else {
            config.host_ip = Self::detect_host_ip()
                .await
                .unwrap_or_else(|_| "127.0.0.1".to_string());
        }

        // Ensure data directory exists
        fs::create_dir_all(&config.data_dir)
            .await
            .context("Failed to create data directory")?;

        // Ensure dev data directory exists if in dev mode
        if config.dev_mode {
            fs::create_dir_all(&config.dev_data_dir)
                .await
                .context("Failed to create dev data directory")?;
        }

        Ok(config)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("/var/lib/archipelago"),
            bind_host: "127.0.0.1".to_string(),
            bind_port: 5678,
            log_level: "info".to_string(),
            host_ip: "127.0.0.1".to_string(),
            dev_mode: false,
            container_runtime: ContainerRuntime::Auto,
            port_offset: 10000,
            bitcoin_simulation: BitcoinSimulation::Mock,
            dev_data_dir: PathBuf::from("/tmp/archipelago-dev"),
            // Discoverability is opt-in. Until the user explicitly enables it
            // (Settings UI / `nostr_discovery_enabled = true` in config), no
            // presence event is ever published and `handshake.poll` never
            // contacts a relay. This is the sole knob that controls whether
            // we leak our DID + npub to the public Nostr relays.
            nostr_discovery_enabled: false,
            nostr_relays: vec![
                "wss://relay.damus.io".into(),
                "wss://relay.nostr.info".into(),
            ],
            nostr_tor_proxy: Some("127.0.0.1:9050".into()),
            use_quadlet_backends: false,
            swarm_enabled: false,
        }
    }
}

/// Recognise the canonical "the user meant true" forms for boolean env
/// vars: 1, true, yes, on (case-insensitive, surrounding whitespace
/// trimmed). Anything else — including the typo'd "ture" or the empty
/// string — counts as false. Centralised so future env flags stay
/// consistent with each other.
fn parse_truthy_env(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_values() {
        let config = Config::default();
        assert_eq!(config.data_dir, PathBuf::from("/var/lib/archipelago"));
        assert_eq!(config.bind_host, "127.0.0.1");
        assert_eq!(config.bind_port, 5678);
        assert_eq!(config.log_level, "info");
        assert_eq!(config.host_ip, "127.0.0.1");
        assert!(!config.dev_mode);
        assert_eq!(config.port_offset, 10000);
        assert!(!config.nostr_discovery_enabled);
        assert_eq!(config.nostr_relays.len(), 2);
        assert_eq!(config.nostr_tor_proxy, Some("127.0.0.1:9050".to_string()));
    }

    #[test]
    fn test_container_runtime_from_str_podman() {
        assert!(matches!(
            ContainerRuntime::from_str("podman"),
            ContainerRuntime::Podman
        ));
        assert!(matches!(
            ContainerRuntime::from_str("Podman"),
            ContainerRuntime::Podman
        ));
        assert!(matches!(
            ContainerRuntime::from_str("PODMAN"),
            ContainerRuntime::Podman
        ));
    }

    #[test]
    fn test_container_runtime_from_str_docker() {
        assert!(matches!(
            ContainerRuntime::from_str("docker"),
            ContainerRuntime::Docker
        ));
        assert!(matches!(
            ContainerRuntime::from_str("Docker"),
            ContainerRuntime::Docker
        ));
        assert!(matches!(
            ContainerRuntime::from_str("DOCKER"),
            ContainerRuntime::Docker
        ));
    }

    #[test]
    fn test_container_runtime_from_str_auto() {
        assert!(matches!(
            ContainerRuntime::from_str("auto"),
            ContainerRuntime::Auto
        ));
        assert!(matches!(
            ContainerRuntime::from_str("Auto"),
            ContainerRuntime::Auto
        ));
        // Unknown strings default to Auto
        assert!(matches!(
            ContainerRuntime::from_str("unknown"),
            ContainerRuntime::Auto
        ));
        assert!(matches!(
            ContainerRuntime::from_str(""),
            ContainerRuntime::Auto
        ));
    }

    #[test]
    fn test_bitcoin_simulation_from_str() {
        assert!(matches!(
            BitcoinSimulation::from_str("mock"),
            BitcoinSimulation::Mock
        ));
        assert!(matches!(
            BitcoinSimulation::from_str("Mock"),
            BitcoinSimulation::Mock
        ));
        assert!(matches!(
            BitcoinSimulation::from_str("testnet"),
            BitcoinSimulation::Testnet
        ));
        assert!(matches!(
            BitcoinSimulation::from_str("Testnet"),
            BitcoinSimulation::Testnet
        ));
        assert!(matches!(
            BitcoinSimulation::from_str("mainnet"),
            BitcoinSimulation::Mainnet
        ));
        assert!(matches!(
            BitcoinSimulation::from_str("Mainnet"),
            BitcoinSimulation::Mainnet
        ));
        assert!(matches!(
            BitcoinSimulation::from_str("none"),
            BitcoinSimulation::None
        ));
    }

    #[test]
    fn test_bitcoin_simulation_unknown_defaults_to_none() {
        assert!(matches!(
            BitcoinSimulation::from_str(""),
            BitcoinSimulation::None
        ));
        assert!(matches!(
            BitcoinSimulation::from_str("signet"),
            BitcoinSimulation::None
        ));
        assert!(matches!(
            BitcoinSimulation::from_str("garbage"),
            BitcoinSimulation::None
        ));
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = Config::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.bind_host, config.bind_host);
        assert_eq!(deserialized.bind_port, config.bind_port);
        assert_eq!(deserialized.data_dir, config.data_dir);
        assert_eq!(deserialized.log_level, config.log_level);
        assert_eq!(deserialized.dev_mode, config.dev_mode);
        assert_eq!(deserialized.port_offset, config.port_offset);
        assert_eq!(
            deserialized.nostr_discovery_enabled,
            config.nostr_discovery_enabled
        );
        assert_eq!(deserialized.nostr_relays, config.nostr_relays);
    }

    #[test]
    fn test_config_toml_parsing() {
        let toml_str = r#"
            data_dir = "/tmp/test-data"
            bind_host = "127.0.0.1"
            bind_port = 9999
            log_level = "debug"
            host_ip = "192.0.2.100"
            dev_mode = true
            container_runtime = "Podman"
            port_offset = 20000
            bitcoin_simulation = "Testnet"
            dev_data_dir = "/tmp/dev-test"
            nostr_discovery_enabled = false
            nostr_relays = ["wss://example.com"]
        "#;
        let config: Config = toml::de::from_str(toml_str).unwrap();
        assert_eq!(config.data_dir, PathBuf::from("/tmp/test-data"));
        assert_eq!(config.bind_host, "127.0.0.1");
        assert_eq!(config.bind_port, 9999);
        assert_eq!(config.log_level, "debug");
        assert_eq!(config.host_ip, "192.0.2.100");
        assert!(config.dev_mode);
        assert_eq!(config.port_offset, 20000);
        assert!(!config.nostr_discovery_enabled);
        assert_eq!(config.nostr_relays, vec!["wss://example.com"]);
    }

    #[test]
    fn test_config_data_dir_is_pathbuf() {
        let config = Config::default();
        assert!(config.data_dir.is_absolute());
        assert_eq!(config.data_dir, PathBuf::from("/var/lib/archipelago"));
    }

    #[test]
    fn test_config_host_ip_default() {
        let config = Config::default();
        assert_eq!(config.host_ip, "127.0.0.1");
    }

    #[test]
    fn test_config_dev_mode_defaults_off() {
        let config = Config::default();
        assert!(!config.dev_mode);
        assert_eq!(config.dev_data_dir, PathBuf::from("/tmp/archipelago-dev"));
    }

    #[test]
    fn test_config_nostr_discovery_disabled_by_default() {
        // Discoverability is opt-in: nothing is published to public relays
        // until the user explicitly turns it on. Flipping this back to
        // `true` would silently start leaking the local DID + npub on every
        // boot — guard rail.
        let config = Config::default();
        assert!(!config.nostr_discovery_enabled);
        assert!(config.nostr_tor_proxy.is_some());
    }

    #[test]
    fn test_config_nostr_relays_default_not_empty() {
        let config = Config::default();
        assert!(!config.nostr_relays.is_empty());
        assert!(config.nostr_relays.iter().all(|r| r.starts_with("wss://")));
    }

    #[test]
    fn parse_truthy_env_recognises_canonical_forms() {
        for t in ["1", "true", "TRUE", "yes", "Yes", "on", "ON", "  true  "] {
            assert!(parse_truthy_env(t), "{t:?} should parse truthy");
        }
        for f in ["", "0", "false", "no", "off", "ture", "anything else", " "] {
            assert!(!parse_truthy_env(f), "{f:?} should NOT parse truthy");
        }
    }

    #[test]
    fn test_config_use_quadlet_backends_defaults_off() {
        // Phase 3.2 of v1.7.52 — the new path stays gated until the 5×
        // harness goes green on .228 and .198. Flipping this default
        // ahead of that would route every backend install through code
        // we haven't fleet-validated yet.
        let config = Config::default();
        assert!(!config.use_quadlet_backends);
    }
}
