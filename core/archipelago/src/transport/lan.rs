// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! LAN transport — peer discovery via mDNS and direct HTTP messaging.
//!
//! Advertises this node as `_archipelago._tcp.local.` with TXT records
//! containing the node's DID and public key. Discovers other Archipelago
//! nodes on the same LAN segment. Sends messages via direct HTTP POST
//! to the discovered IP:port — same endpoint as Tor transport but without
//! the SOCKS5 proxy, for near-zero latency on local networks.

use super::{NodeTransport, PeerRegistry, PeerSource, TransportKind, TransportMessage};
use anyhow::{Context, Result};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

const SERVICE_TYPE: &str = "_archipelago._tcp.local.";
const DEFAULT_PORT: u16 = 5678;
const LAN_TIMEOUT: Duration = Duration::from_secs(10);

pub struct LanTransport {
    our_did: String,
    our_pubkey_hex: String,
    our_port: u16,
    /// This node's FIPS npub, advertised in the mDNS TXT record so
    /// co-located peers can form a direct FIPS link (`lan_fips_anchors`)
    /// without waiting for federation storage to sync.
    our_fips_npub: Option<String>,
    daemon: Option<ServiceDaemon>,
    available: AtomicBool,
}

impl LanTransport {
    /// Create a new LAN transport. Does not start discovery yet.
    pub fn new(
        our_did: &str,
        our_pubkey_hex: &str,
        port: u16,
        our_fips_npub: Option<String>,
    ) -> Self {
        Self {
            our_did: our_did.to_string(),
            our_pubkey_hex: our_pubkey_hex.to_string(),
            our_port: port,
            our_fips_npub,
            daemon: None,
            available: AtomicBool::new(false),
        }
    }

    /// Start the mDNS daemon, advertise our service, and begin browsing.
    /// Non-blocking — spawns background tasks for discovery.
    pub fn start(&mut self, registry: Arc<PeerRegistry>) -> Result<()> {
        let daemon = ServiceDaemon::new().context("Failed to create mDNS daemon")?;

        // Advertise our service
        let hostname = format!("archy-{}.local.", &self.our_pubkey_hex[..8]);
        let mut properties = vec![
            ("did".to_string(), self.our_did.clone()),
            ("pubkey".to_string(), self.our_pubkey_hex.clone()),
            ("version".to_string(), "0.1.0".to_string()),
        ];
        if let Some(npub) = &self.our_fips_npub {
            properties.push(("fips".to_string(), npub.clone()));
        }

        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            &format!("archy-{}", &self.our_pubkey_hex[..8]),
            &hostname,
            "",
            self.our_port,
            Some(properties.into_iter().collect()),
        )
        .context("Failed to create mDNS service info")?;

        daemon
            .register(service_info)
            .context("Failed to register mDNS service")?;

        // Browse for other Archipelago nodes
        let receiver = daemon
            .browse(SERVICE_TYPE)
            .context("Failed to browse mDNS services")?;

        self.daemon = Some(daemon);
        self.available.store(true, Ordering::Relaxed);

        info!("LAN transport started — advertising {}", SERVICE_TYPE);

        // Spawn background discovery listener
        let registry_clone = registry;
        tokio::spawn(async move {
            while let Ok(event) = receiver.recv() {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let did = info
                            .get_properties()
                            .get("did")
                            .map(|v| v.val_str().to_string());
                        let pubkey = info
                            .get_properties()
                            .get("pubkey")
                            .map(|v| v.val_str().to_string());
                        let addresses = info.get_addresses();

                        let fips_npub = info
                            .get_properties()
                            .get("fips")
                            .map(|v| v.val_str().to_string());

                        if let (Some(did), Some(pubkey)) = (did, pubkey) {
                            if let Some(scoped_ip) = addresses.iter().next() {
                                let ip: std::net::IpAddr = match scoped_ip.to_string().parse() {
                                    Ok(ip) => ip,
                                    Err(_) => continue,
                                };
                                let socket_addr = std::net::SocketAddr::new(ip, info.get_port());
                                info!(did = %did, addr = %socket_addr, "Discovered LAN peer via mDNS");
                                registry_clone
                                    .register_peer(&did, &pubkey, PeerSource::LanDiscovery)
                                    .await;
                                registry_clone.set_lan_address(&did, socket_addr).await;
                                registry_clone.set_name(&did, info.get_fullname()).await;
                                if let Some(npub) = fips_npub.as_deref() {
                                    registry_clone.set_fips_npub(&did, npub).await;
                                }
                            }
                        }
                    }
                    ServiceEvent::ServiceRemoved(_, name) => {
                        debug!(name = %name, "LAN peer removed");
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }

    async fn send_impl(&self, address: &str, message: &TransportMessage) -> Result<()> {
        // address is "ip:port" format
        let url = format!("http://{}/archipelago/node-message", address);
        let encoded_payload = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(&message.payload)
        };
        let body = serde_json::json!({
            "from_pubkey": self.our_pubkey_hex,
            "from_did": message.from_did,
            "message": encoded_payload,
            "message_type": message.message_type,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "transport": "lan",
        });

        let client = reqwest::Client::builder()
            .timeout(LAN_TIMEOUT)
            .build()
            .context("Failed to build LAN HTTP client")?;

        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("LAN send to {} failed: {}", address, e))?;

        if !resp.status().is_success() {
            anyhow::bail!(
                "LAN peer at {} returned {}",
                address,
                resp.status().as_u16()
            );
        }

        Ok(())
    }
}

impl Drop for LanTransport {
    // The mdns_sd daemon runs on its own OS thread and the browse
    // listener task blocks on a sync channel. Without this call both
    // keep the process alive past SIGTERM, long enough for systemd to
    // SIGKILL us — which makes a normal update look like a crash.
    fn drop(&mut self) {
        if let Some(daemon) = self.daemon.take() {
            let _ = daemon.shutdown();
        }
    }
}

impl NodeTransport for LanTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Lan
    }

    fn is_available(&self) -> bool {
        self.available.load(Ordering::Relaxed)
    }

    fn send<'a>(
        &'a self,
        address: &'a str,
        message: &'a TransportMessage,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move { self.send_impl(address, message).await })
    }
}
