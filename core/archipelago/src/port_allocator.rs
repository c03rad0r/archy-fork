//! Smart port allocation to prevent conflicts between apps.
//!
//! Tracks which host ports are in use and allocates free ports for new apps.
//! Persists allocations to disk so they survive restarts.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Ports reserved by system/deploy services (LND UI, Mempool, etc.).
/// These are never allocated to user-installed apps.
const RESERVED_PORTS: &[u16] = &[
    80, 443, 81, // HTTP/HTTPS
    8332, 8333, 8334, // Bitcoin RPC/P2P
    9735, 10009, 8080,  // LND P2P, gRPC, REST
    18083, // LND UI (archy-lnd-ui)
    4080, 8999, 50001, // Mempool stack
    23000, // BTCPay
    8173, 8174, 8175,  // Fedimint
    8178,  // Fedimint client daemon (fedimint-clientd REST)
    3535,  // Ark wallet daemon (barkd REST)
    8123,  // Home Assistant
    3000,  // Grafana
    11434, // Ollama
    9980, 9001, // OnlyOffice, Penpot
    8240, // Tailscale
    9000, // Portainer
    3001, 3002, // Gitea, Uptime Kuma
    8888, // SearXNG
    8096, 2342, 2283, // Jellyfin, Photoprism, Immich
    8443, // FIPS TCP fallback
    8336, // FIPS UI (fips-ui)
];

/// Start of range for allocating web app ports when preferred is taken.
const WEB_PORT_RANGE_START: u16 = 8085;
const WEB_PORT_RANGE_END: u16 = 9999;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PortAllocations {
    /// app_id -> (host_port, container_port)
    allocations: HashMap<String, PortMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PortMapping {
    host_port: u16,
    container_port: u16,
}

pub struct PortAllocator {
    data_dir: std::path::PathBuf,
    allocations: PortAllocations,
}

impl PortAllocator {
    pub async fn new(data_dir: impl AsRef<Path>) -> Result<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        let path = data_dir.join("port_allocations.json");
        let allocations = if path.exists() {
            let s = tokio::fs::read_to_string(&path)
                .await
                .context("Failed to read port allocations")?;
            serde_json::from_str(&s).unwrap_or_default()
        } else {
            PortAllocations::default()
        };
        Ok(Self {
            data_dir,
            allocations,
        })
    }

    async fn save(&self) -> Result<()> {
        let path = self.data_dir.join("port_allocations.json");
        tokio::fs::create_dir_all(&self.data_dir)
            .await
            .context("Failed to create data dir for port allocations")?;
        let s = serde_json::to_string_pretty(&self.allocations)
            .context("Failed to serialize port allocations")?;
        tokio::fs::write(&path, s)
            .await
            .context("Failed to write port allocations")?;
        Ok(())
    }

    fn is_reserved(&self, port: u16) -> bool {
        RESERVED_PORTS.contains(&port)
    }

    fn is_allocated(&self, port: u16) -> bool {
        self.allocations
            .allocations
            .values()
            .any(|m| m.host_port == port)
    }

    fn can_bind(port: u16) -> bool {
        std::net::TcpListener::bind(("0.0.0.0", port)).is_ok()
    }

    fn is_available(&self, port: u16) -> bool {
        !self.is_reserved(port) && !self.is_allocated(port) && Self::can_bind(port)
    }

    /// Allocate a host port for an app. Uses preferred_port if available, else finds next free.
    pub async fn allocate(
        &mut self,
        app_id: &str,
        preferred_host_port: u16,
        container_port: u16,
    ) -> Result<u16> {
        let host_port = if self.is_available(preferred_host_port) {
            preferred_host_port
        } else {
            (WEB_PORT_RANGE_START..=WEB_PORT_RANGE_END)
                .find(|&p| self.is_available(p))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "No free port in range {}-{}",
                        WEB_PORT_RANGE_START,
                        WEB_PORT_RANGE_END
                    )
                })?
        };

        self.allocations.allocations.insert(
            app_id.to_string(),
            PortMapping {
                host_port,
                container_port,
            },
        );
        self.save().await?;
        Ok(host_port)
    }

    /// Get existing allocation for an app, if any.
    pub fn get(&self, app_id: &str) -> Option<(u16, u16)> {
        self.allocations
            .allocations
            .get(app_id)
            .map(|m| (m.host_port, m.container_port))
    }

    /// Allocate or return existing. Use when installing/starting an app.
    pub async fn allocate_or_get(
        &mut self,
        app_id: &str,
        preferred_host_port: u16,
        container_port: u16,
    ) -> Result<u16> {
        if let Some((host, _)) = self.get(app_id) {
            return Ok(host);
        }
        self.allocate(app_id, preferred_host_port, container_port)
            .await
    }

    /// Release port when app is uninstalled.
    pub async fn release(&mut self, app_id: &str) -> Result<()> {
        self.allocations.allocations.remove(app_id);
        self.save().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_allocations_default_is_empty() {
        let allocs = PortAllocations::default();
        assert!(allocs.allocations.is_empty());
    }

    #[tokio::test]
    async fn test_new_allocator_from_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let alloc = PortAllocator::new(dir.path()).await.unwrap();
        assert!(alloc.allocations.allocations.is_empty());
    }

    #[tokio::test]
    async fn test_allocate_preferred_port_when_available() {
        let dir = tempfile::tempdir().unwrap();
        let mut alloc = PortAllocator::new(dir.path()).await.unwrap();
        let port = alloc.allocate("my-app", 8500, 80).await.unwrap();
        assert_eq!(port, 8500);
    }

    #[tokio::test]
    async fn test_allocate_fallback_when_preferred_is_reserved() {
        let dir = tempfile::tempdir().unwrap();
        let mut alloc = PortAllocator::new(dir.path()).await.unwrap();
        // Port 80 is in RESERVED_PORTS, so it should allocate from the range instead
        let port = alloc.allocate("web-app", 80, 80).await.unwrap();
        assert_ne!(port, 80);
        assert!((WEB_PORT_RANGE_START..=WEB_PORT_RANGE_END).contains(&port));
    }

    #[tokio::test]
    async fn test_allocate_fallback_when_preferred_is_taken() {
        let dir = tempfile::tempdir().unwrap();
        let mut alloc = PortAllocator::new(dir.path()).await.unwrap();
        let port1 = alloc.allocate("app-1", 8500, 80).await.unwrap();
        assert_eq!(port1, 8500);

        // Second app requesting the same preferred port gets a different one
        let port2 = alloc.allocate("app-2", 8500, 80).await.unwrap();
        assert_ne!(port2, 8500);
        assert!((WEB_PORT_RANGE_START..=WEB_PORT_RANGE_END).contains(&port2));
    }

    #[tokio::test]
    async fn test_get_returns_existing_allocation() {
        let dir = tempfile::tempdir().unwrap();
        let mut alloc = PortAllocator::new(dir.path()).await.unwrap();
        alloc.allocate("test-app", 8600, 3000).await.unwrap();

        let result = alloc.get("test-app");
        assert!(result.is_some());
        let (host, container) = result.unwrap();
        assert_eq!(host, 8600);
        assert_eq!(container, 3000);
    }

    #[tokio::test]
    async fn test_get_returns_none_for_unknown_app() {
        let dir = tempfile::tempdir().unwrap();
        let alloc = PortAllocator::new(dir.path()).await.unwrap();
        assert!(alloc.get("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_allocate_or_get_returns_existing() {
        let dir = tempfile::tempdir().unwrap();
        let mut alloc = PortAllocator::new(dir.path()).await.unwrap();
        let port1 = alloc.allocate("my-app", 8700, 80).await.unwrap();

        // Calling allocate_or_get with a different preferred port should return the existing one
        let port2 = alloc.allocate_or_get("my-app", 9999, 80).await.unwrap();
        assert_eq!(port1, port2);
        assert_eq!(port2, 8700);
    }

    #[tokio::test]
    async fn test_allocate_or_get_allocates_when_new() {
        let dir = tempfile::tempdir().unwrap();
        let mut alloc = PortAllocator::new(dir.path()).await.unwrap();
        let port = alloc.allocate_or_get("new-app", 8800, 443).await.unwrap();
        assert_eq!(port, 8800);

        // Verify it's now stored
        assert!(alloc.get("new-app").is_some());
    }

    #[tokio::test]
    async fn test_release_removes_allocation() {
        let dir = tempfile::tempdir().unwrap();
        let mut alloc = PortAllocator::new(dir.path()).await.unwrap();
        alloc.allocate("removable", 8900, 80).await.unwrap();
        assert!(alloc.get("removable").is_some());

        alloc.release("removable").await.unwrap();
        assert!(alloc.get("removable").is_none());
    }

    #[tokio::test]
    async fn test_released_port_becomes_available() {
        let dir = tempfile::tempdir().unwrap();
        let mut alloc = PortAllocator::new(dir.path()).await.unwrap();
        alloc.allocate("app-a", 8500, 80).await.unwrap();
        alloc.release("app-a").await.unwrap();

        // Port 8500 should now be available again
        let port = alloc.allocate("app-b", 8500, 80).await.unwrap();
        assert_eq!(port, 8500);
    }

    #[tokio::test]
    async fn test_reserved_ports_are_never_allocated() {
        let dir = tempfile::tempdir().unwrap();
        let alloc = PortAllocator::new(dir.path()).await.unwrap();
        for &port in RESERVED_PORTS {
            assert!(alloc.is_reserved(port), "Port {} should be reserved", port);
            assert!(
                !alloc.is_available(port),
                "Port {} should not be available",
                port
            );
        }
    }

    #[tokio::test]
    async fn test_persistence_across_instances() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut alloc = PortAllocator::new(dir.path()).await.unwrap();
            alloc.allocate("persistent-app", 8555, 80).await.unwrap();
        }
        // Create a new allocator from the same directory
        let alloc2 = PortAllocator::new(dir.path()).await.unwrap();
        let result = alloc2.get("persistent-app");
        assert!(result.is_some());
        let (host, container) = result.unwrap();
        assert_eq!(host, 8555);
        assert_eq!(container, 80);
    }
}
