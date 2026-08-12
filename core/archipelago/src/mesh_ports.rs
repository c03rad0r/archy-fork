//! Mirror IPv4-published app ports onto IPv6 for mesh access.
//!
//! The companion (and anything else on the FIPS mesh) reaches this node at
//! its fips0 ULA. The web UI builds app links from the current host + each
//! app's DIRECT port (Direct Port Rule) — but rootless-podman published
//! ports bind 0.0.0.0 only, so `http://[<ULA>]:8334` was refused for every
//! catalog app even after nginx :80 learned IPv6 (2026-07-23, third
//! "the node wasn't listening on v6" bug of the night).
//!
//! Instead of touching the container layer (port-publish changes would
//! recreate every container), a reconcile loop mirrors the host's public
//! IPv4 listeners: any port ≥1024 listening on 0.0.0.0 without an IPv6
//! any-address listener gets a v6-only `[::]:<port>` forwarder to
//! `127.0.0.1:<port>`. Ports appear/disappear with app install/remove, so
//! the mirror follows `/proc/net/tcp*` rather than the catalog — companion
//! containers with hardcoded ports (bitcoin-ui :8334) are covered the same
//! as manifest-declared ones. Nothing is exposed that IPv4 didn't already
//! expose to the LAN; the ULA is the established remote-access surface.

use std::collections::{HashMap, HashSet};
use std::net::{Ipv6Addr, SocketAddrV6};
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

const RECONCILE_INTERVAL: Duration = Duration::from_secs(15);
/// Never mirror system ports; nginx (80/443) handles its own v6 listeners.
const MIN_PORT: u16 = 1024;

/// Entry point — spawned from main; runs for the process lifetime.
pub async fn run_mesh_port_mirror() {
    let mut active: HashMap<u16, JoinHandle<()>> = HashMap::new();
    loop {
        match reconcile(&mut active).await {
            Ok(()) => {}
            Err(e) => debug!("mesh-ports: reconcile skipped: {e:#}"),
        }
        tokio::time::sleep(RECONCILE_INTERVAL).await;
    }
}

async fn reconcile(active: &mut HashMap<u16, JoinHandle<()>>) -> Result<()> {
    let v4 = listening_ports("/proc/net/tcp", 8).await?;
    let v6 = listening_ports("/proc/net/tcp6", 32).await?;

    // Drop mirrors whose backing IPv4 listener vanished (app removed/stopped).
    active.retain(|port, handle| {
        if v4.contains(port) && !handle.is_finished() {
            true
        } else {
            handle.abort();
            info!("mesh-ports: released [::]:{port} (backing 0.0.0.0 listener gone)");
            false
        }
    });

    for port in v4 {
        if port < MIN_PORT || active.contains_key(&port) {
            continue;
        }
        // A foreign IPv6 any-listener already serves this port (our own
        // mirrors are excluded above via `active`).
        if v6.contains(&port) {
            continue;
        }
        match spawn_forwarder(port) {
            Ok(handle) => {
                info!("mesh-ports: mirroring [::]:{port} -> 127.0.0.1:{port}");
                active.insert(port, handle);
            }
            Err(e) => debug!("mesh-ports: cannot mirror port {port}: {e:#}"),
        }
    }
    Ok(())
}

/// Ports in LISTEN state bound to the any-address, parsed from /proc/net/tcp
/// (`addr_hex_len` 8) or /proc/net/tcp6 (32).
async fn listening_ports(path: &str, addr_hex_len: usize) -> Result<HashSet<u16>> {
    let raw = tokio::fs::read_to_string(path)
        .await
        .with_context(|| format!("read {path}"))?;
    let any_addr = "0".repeat(addr_hex_len);
    let mut ports = HashSet::new();
    for line in raw.lines().skip(1) {
        let mut cols = line.split_whitespace();
        let (Some(_sl), Some(local), Some(_rem), Some(state)) =
            (cols.next(), cols.next(), cols.next(), cols.next())
        else {
            continue;
        };
        if state != "0A" {
            continue; // not LISTEN
        }
        let Some((addr, port_hex)) = local.split_once(':') else {
            continue;
        };
        if addr != any_addr {
            continue;
        }
        if let Ok(port) = u16::from_str_radix(port_hex, 16) {
            ports.insert(port);
        }
    }
    Ok(ports)
}

/// A v6-only listener on [::]:port forwarding each connection to 127.0.0.1:port.
fn spawn_forwarder(port: u16) -> Result<JoinHandle<()>> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket =
        Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP)).context("create v6 socket")?;
    // v6only so we coexist with the app's own 0.0.0.0:<port> bind.
    socket.set_only_v6(true).context("set v6only")?;
    socket.set_reuse_address(true).ok();
    socket.set_nonblocking(true).context("set nonblocking")?;
    let addr = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, port, 0, 0);
    socket.bind(&addr.into()).context("bind [::]")?;
    socket.listen(128).context("listen")?;
    let listener =
        tokio::net::TcpListener::from_std(socket.into()).context("register with tokio")?;

    Ok(tokio::spawn(async move {
        loop {
            let (mut inbound, _peer) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    warn!("mesh-ports: accept failed on [::]:{port}: {e}");
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                }
            };
            tokio::spawn(async move {
                match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
                    Ok(mut upstream) => {
                        let _ = tokio::io::copy_bidirectional(&mut inbound, &mut upstream).await;
                    }
                    Err(e) => debug!("mesh-ports: upstream 127.0.0.1:{port} refused: {e}"),
                }
            });
        }
    }))
}
