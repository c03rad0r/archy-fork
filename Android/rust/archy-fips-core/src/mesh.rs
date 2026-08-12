//! Mesh lifecycle: identity, config assembly, and the node task.
//!
//! Host-buildable (no JNI) so the config/identity logic is unit-testable;
//! only [`start`] needs a real TUN fd and therefore only runs on-device.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use fips::config::{PeerConfig, TransportInstances, UdpConfig};
use fips::{Config, Identity, Node};
use tokio::sync::Notify;

/// How long `start` waits for the node to come up (TUN attach + transports).
const START_TIMEOUT: Duration = Duration::from_secs(15);

/// How long `stop` waits for the node task to drain.
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

struct MeshHandle {
    runtime: tokio::runtime::Runtime,
    task: Option<tokio::task::JoinHandle<()>>,
    shutdown: Arc<Notify>,
    running: Arc<AtomicBool>,
    npub: String,
    address: String,
}

static MESH: Mutex<Option<MeshHandle>> = Mutex::new(None);

#[derive(Debug, Clone)]
pub struct IdentityInfo {
    pub secret_hex: String,
    pub npub: String,
    /// The phone's own ULA on the mesh (fd::/8), for VpnService.addAddress.
    pub address: String,
}

/// Generate a fresh mesh identity from the OS CSPRNG.
pub fn generate_identity() -> Result<IdentityInfo> {
    // ~1 in 2^128 chance a candidate is off the curve; loop regardless.
    loop {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes).context("OS RNG")?;
        if let Ok(id) = Identity::from_secret_bytes(&bytes) {
            return Ok(IdentityInfo {
                secret_hex: hex::encode(bytes),
                npub: id.npub(),
                address: id.address().to_ipv6().to_string(),
            });
        }
    }
}

/// Re-derive npub + ULA from a stored secret.
pub fn derive_identity(secret: &str) -> Result<IdentityInfo> {
    let id = Identity::from_secret_str(secret).map_err(|e| anyhow!("bad secret: {e}"))?;
    Ok(IdentityInfo {
        secret_hex: secret.to_string(),
        npub: id.npub(),
        address: id.address().to_ipv6().to_string(),
    })
}

/// Build the phone-side node config: leaf-only (never routes third-party
/// traffic — battery), no DNS responder, TUN enabled but attached to the
/// VpnService fd rather than created.
///
/// `listen_port` 0 = ephemeral UDP (outbound-only, the default posture).
/// Non-zero = fixed UDP bind so a nearby phone can dial us directly over a
/// local link (party mode); leaf_only still guarantees we never carry
/// third-party transit even while accepting an inbound link.
pub fn build_config(secret: &str, peers: Vec<PeerConfig>, listen_port: u16) -> Config {
    let mut cfg = Config::default();
    cfg.node.identity.nsec = Some(secret.to_string());
    cfg.node.identity.persistent = false;
    cfg.node.leaf_only = true;
    cfg.tun.enabled = true;
    cfg.tun.mtu = Some(1280);
    cfg.dns.enabled = false;
    cfg.transports.udp = TransportInstances::Single(UdpConfig {
        bind_addr: Some(format!("0.0.0.0:{listen_port}")),
        ..Default::default()
    });
    // TCP with no bind_addr = outbound-only (fallback when UDP is blocked).
    cfg.transports.tcp = TransportInstances::Single(Default::default());
    // Fast-connect profile — a phone opens the app and expects the node NOW.
    // Stock pacing is tuned for always-on routers: a failed discovery backs
    // off 30s, session resends gap out to 8-16s, dead links redial at
    // 5s→300s. Over 5G that stacked into a ~40s first connect (observed
    // 2026-07-24: anchor +10s, session +40s). Retries only fire while a
    // link/session is down, so steady-state traffic is unchanged.
    cfg.node.retry.base_interval_secs = 1; // dead-link redial 1s,2s,4s…
    cfg.node.retry.max_backoff_secs = 30; // …capped at 30s, not 5 min
    cfg.node.retry.max_retries = 30;
    cfg.node.rate_limit.handshake_resend_interval_ms = 400;
    cfg.node.rate_limit.handshake_resend_backoff = 1.5;
    cfg.node.rate_limit.handshake_max_resends = 10;
    cfg.node.discovery.backoff_base_secs = 1; // failed lookup retries fast
    cfg.node.discovery.backoff_max_secs = 30;
    cfg.node.discovery.retry_interval_secs = 2;
    cfg.node.discovery.max_attempts = 3;
    // Lookups launched before the tree position settles are doomed; a 10s
    // completion timeout made each one cost 10s before the 1s retry could
    // fire (observed: 19s to a route on a fresh join). Fail fast instead —
    // the resend-within-window above still gives each attempt two shots.
    cfg.node.discovery.timeout_secs = 5;
    cfg.peers = peers;
    cfg
}

/// Parse the peers JSON handed over from Kotlin. Shape = fips `PeerConfig`:
/// `[{"npub":"…","alias":"…","addresses":[{"transport":"udp","addr":"host:2121","priority":10},…]}]`
pub fn parse_peers(peers_json: &str) -> Result<Vec<PeerConfig>> {
    serde_json::from_str(peers_json).context("peers JSON")
}

/// Start the mesh node on the given TUN fd (from `VpnService.establish()`,
/// detached — the node owns it from here). Returns (npub, ula) on success.
/// Any previously running node is stopped first.
pub fn start(secret: &str, peers_json: &str, tun_fd: i32, listen_port: u16) -> Result<(String, String)> {
    stop();

    // Android hands the VpnService TUN fd over in non-blocking mode on some
    // OS builds. The fips TUN reader is a dedicated blocking-read thread that
    // treats EAGAIN as fatal — the loop died at startup ("TUN read error …
    // Try again (os error 11)" on-device), so sessions came up but no packet
    // ever entered the mesh. Force the fd into the blocking mode the reader
    // is designed for.
    unsafe {
        let flags = libc::fcntl(tun_fd, libc::F_GETFL);
        if flags >= 0 && (flags & libc::O_NONBLOCK) != 0 {
            libc::fcntl(tun_fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
        }
    }

    let peers = parse_peers(peers_json)?;
    let config = build_config(secret, peers, listen_port);
    let mut node = Node::new(config).map_err(|e| anyhow!("node init: {e}"))?;
    let npub = node.npub();
    let address = node.identity().address().to_ipv6().to_string();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("archy-fips")
        .build()
        .context("tokio runtime")?;

    let shutdown = Arc::new(Notify::new());
    let running = Arc::new(AtomicBool::new(false));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel::<Result<()>>();

    let task = {
        let shutdown = shutdown.clone();
        let running = running.clone();
        runtime.spawn(async move {
            match node.start_with_tun_fd(tun_fd).await {
                Ok(()) => {
                    running.store(true, Ordering::SeqCst);
                    let _ = started_tx.send(Ok(()));
                }
                Err(e) => {
                    let _ = started_tx.send(Err(anyhow!("node start: {e}")));
                    return;
                }
            }
            tokio::select! {
                result = node.run_rx_loop() => {
                    if let Err(e) = result {
                        tracing::error!("mesh rx loop error: {e}");
                    }
                }
                _ = shutdown.notified() => {}
            }
            if let Err(e) = node.stop().await {
                tracing::warn!("mesh stop: {e}");
            }
            running.store(false, Ordering::SeqCst);
        })
    };

    let started = runtime
        .block_on(async { tokio::time::timeout(START_TIMEOUT, started_rx).await })
        .map_err(|_| anyhow!("node start timed out"))?
        .map_err(|_| anyhow!("node task died during start"))?;
    if let Err(e) = started {
        runtime.shutdown_background();
        return Err(e);
    }

    *MESH.lock().unwrap() = Some(MeshHandle {
        runtime,
        task: Some(task),
        shutdown,
        running,
        npub: npub.clone(),
        address: address.clone(),
    });
    Ok((npub, address))
}

/// Stop the mesh node if running. Idempotent.
pub fn stop() {
    let Some(mut handle) = MESH.lock().unwrap().take() else {
        return;
    };
    handle.shutdown.notify_waiters();
    if let Some(task) = handle.task.take() {
        let _ = handle
            .runtime
            .block_on(async { tokio::time::timeout(STOP_TIMEOUT, task).await });
    }
    handle.runtime.shutdown_background();
}

pub fn is_running() -> bool {
    MESH.lock()
        .unwrap()
        .as_ref()
        .map(|h| h.running.load(Ordering::SeqCst))
        .unwrap_or(false)
}

/// `{running, npub, address}` for the Kotlin status surface.
pub fn status_json() -> String {
    let guard = MESH.lock().unwrap();
    match guard.as_ref() {
        Some(h) => serde_json::json!({
            "running": h.running.load(Ordering::SeqCst),
            "npub": h.npub,
            "address": h.address,
        })
        .to_string(),
        None => serde_json::json!({ "running": false }).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_roundtrip() {
        let a = generate_identity().unwrap();
        let b = derive_identity(&a.secret_hex).unwrap();
        assert_eq!(a.npub, b.npub);
        assert_eq!(a.address, b.address);
        // ULA in fd::/8
        assert!(a.address.starts_with("fd"));
    }

    #[test]
    fn peers_json_parses_into_peer_config() {
        let peers = parse_peers(
            r#"[{
                "npub": "npub1abc",
                "alias": "My Archipelago",
                "addresses": [
                    {"transport": "udp", "addr": "192.0.2.10:2121", "priority": 10},
                    {"transport": "tcp", "addr": "192.0.2.10:8443", "priority": 20}
                ]
            }]"#,
        )
        .unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].addresses.len(), 2);
        assert!(peers[0].is_auto_connect());
    }

    #[test]
    fn config_is_leaf_only_with_tun() {
        let id = generate_identity().unwrap();
        let cfg = build_config(&id.secret_hex, vec![], 0);
        assert!(cfg.node.leaf_only);
        assert!(cfg.tun.enabled);
        assert_eq!(cfg.tun.mtu(), 1280);
        assert!(!cfg.dns.enabled);
        assert!(!cfg.transports.udp.is_empty());
        assert!(!cfg.transports.tcp.is_empty());
    }

    #[test]
    fn listen_port_sets_fixed_udp_bind() {
        let id = generate_identity().unwrap();
        let cfg = build_config(&id.secret_hex, vec![], 2121);
        // Party mode keeps leaf_only — accepting a link is not routing transit.
        assert!(cfg.node.leaf_only);
        let TransportInstances::Single(udp) = &cfg.transports.udp else {
            panic!("expected single UDP transport");
        };
        assert_eq!(udp.bind_addr.as_deref(), Some("0.0.0.0:2121"));
    }
}
