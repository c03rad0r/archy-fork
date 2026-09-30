//! Binding the gate in front of apps, and telling the truth when it cannot.
//!
//! # The ordering problem
//!
//! A published container port is bound `0.0.0.0:<port>`, which claims *every*
//! host address. While the app holds that, the gate cannot bind
//! `<lan-ip>:<port>` at all — the kernel refuses the overlap. So the gate can
//! only stand in front of an app whose own publish has been pinned to
//! loopback (`bind: 127.0.0.1` in its manifest, which
//! `PortMapping::bind` has supported all along).
//!
//! That makes the rollout necessarily two-step, per app: pin the publish,
//! recreate the container, and the gate claims the external addresses. Doing
//! it the other way round — gate first — is not possible, and doing it in one
//! step for every app at once would recreate every container on the node
//! simultaneously.
//!
//! # Why the failure has to be loud
//!
//! The dangerous version of this module is the one that tries to bind, fails
//! because the app still holds the port, logs at debug, and moves on. The
//! node would then be running "the app gate" while every app remained exactly
//! as open as before — a security control that reports success and does
//! nothing, which is worse than no control at all because it stops anyone
//! looking.
//!
//! So an unclaimable port is recorded in [`GateStatus::unprotected`] and
//! logged at warn on every sweep. The same reasoning killed the nft-drop-in
//! design: `/etc/fips/fips.nft` is provisioned out-of-band and its absence is
//! a silent no-op, so a gate shipped that way would be absent on every node
//! without the hardening baseline and nobody would know.

use super::identity::GatedPort;
use super::AppGate;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// How often the sweep re-runs. Matches `app_port_v6_relay_loop`: addresses
/// come and go (DHCP, Tailscale up/down, the fips0 ULA appearing late) and
/// apps are installed while the daemon runs.
const SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// The gate's own loopback address, distinct from the app's `127.0.0.1`.
///
/// Tor cannot present a session cookie, so `HiddenServicePort → 127.0.0.1`
/// reaches the app around the gate. Instead torrc forwards gated ports to
/// this address (`api/rpc/tor`), where the gate — not the app — listens. A
/// second loopback address rather than a second port number, so no app needs
/// a port it did not declare.
pub const GATE_TOR_UPSTREAM: IpAddr = IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 2));

/// A port the gate should own but could not claim, and why.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UnprotectedPort {
    pub port: u16,
    pub app_id: String,
    pub app_name: String,
    /// Human-readable cause, e.g. that the app still publishes on all
    /// interfaces.
    pub reason: String,
}

/// What the gate is actually enforcing right now.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct GateStatus {
    /// (port, address) pairs the gate holds.
    pub claimed: Vec<(u16, String)>,
    /// Ports that should be gated but are not. **Non-empty means the node
    /// has unauthenticated app surface.**
    pub unprotected: Vec<UnprotectedPort>,
}

impl GateStatus {
    pub fn is_fully_enforced(&self) -> bool {
        self.unprotected.is_empty()
    }
}

/// Every non-loopback address currently on this host.
///
/// Shells out to `ip` rather than pulling in a `getifaddrs` binding: the
/// codebase already resolves addresses this way (`host_ip`), the result is
/// re-derived every sweep so a stale parse self-corrects, and a failure here
/// degrades to "claim nothing this round" rather than to a wrong claim.
async fn host_addresses() -> Vec<IpAddr> {
    let Ok(out) = tokio::process::Command::new("ip")
        .args(["-o", "addr", "show"])
        .output()
        .await
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut addrs = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        // `1: lo    inet 127.0.0.1/8 scope host lo`
        let Some(family) = fields.clone().nth(2) else {
            continue;
        };
        if family != "inet" && family != "inet6" {
            continue;
        }
        let Some(cidr) = fields.nth(3) else { continue };
        let Some(addr) = cidr.split('/').next() else {
            continue;
        };
        // Strip a zone index (`fe80::1%eth0`) — link-local addresses need a
        // scope to bind and are not how anyone reaches an app anyway.
        let addr = addr.split('%').next().unwrap_or(addr);
        let Ok(ip) = addr.parse::<IpAddr>() else {
            continue;
        };
        if ip.is_loopback() || ip.is_unspecified() {
            continue;
        }
        if let IpAddr::V6(v6) = ip {
            // Link-local v6 requires a scope id we do not carry.
            if (v6.segments()[0] & 0xffc0) == 0xfe80 {
                continue;
            }
        }
        addrs.push(ip);
    }
    addrs.sort();
    addrs.dedup();
    addrs
}

/// Process-wide gate status, so any RPC handler can report what the gate is
/// actually enforcing without threading a handle through every caller.
///
/// A single shared cell rather than a value returned from `run`: "is my node
/// actually protected?" has to be answerable from the RPC layer, and the
/// listener that knows the answer runs in a detached task.
pub fn shared_status() -> Arc<RwLock<GateStatus>> {
    static STATUS: std::sync::OnceLock<Arc<RwLock<GateStatus>>> = std::sync::OnceLock::new();
    STATUS
        .get_or_init(|| Arc::new(RwLock::new(GateStatus::default())))
        .clone()
}

static REFRESH_KICK: std::sync::LazyLock<tokio::sync::Notify> =
    std::sync::LazyLock::new(tokio::sync::Notify::new);
static REFRESH_REV: std::sync::LazyLock<tokio::sync::watch::Sender<u64>> =
    std::sync::LazyLock::new(|| tokio::sync::watch::channel(0).0);

/// Installation must not wait for the minute sweep before becoming reachable.
/// Wait for a completed sweep, bounded if shutdown/startup prevents one.
pub async fn refresh_now() {
    let mut completed = REFRESH_REV.subscribe();
    REFRESH_KICK.notify_one();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), completed.changed()).await;
}

pub fn port_claimed(status: &GateStatus, port: u16) -> bool {
    let mut external = false;
    let mut tor = false;
    for (claimed_port, address) in &status.claimed {
        if *claimed_port != port {
            continue;
        }
        if let Ok(ip) = address.parse::<IpAddr>() {
            tor |= ip == GATE_TOR_UPSTREAM;
            external |= !ip.is_loopback();
        }
    }
    external && tor
}

/// Run the gate. Returns only on shutdown.
pub async fn run(
    gate: Arc<AppGate>,
    status: Arc<RwLock<GateStatus>>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    // (port, addr) pairs already served, so a sweep does not rebind what it
    // already holds. The accept-loop handle is kept so a claim can be
    // RELEASED when its port leaves the gated set — a catalog refresh
    // declaring a port `local`/`none` must make the gate let go without a
    // daemon restart, or the stale bind keeps republishing a port the
    // catalog just withdrew (nbxplorer 32838, a test node 2026-08-04).
    let mut held: HashMap<(u16, IpAddr), tokio::task::JoinHandle<()>> = HashMap::new();
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = REFRESH_KICK.notified() => {}
            _ = shutdown_rx.changed() => return,
        }
        sweep(&gate, &status, &mut held, &shutdown_rx).await;
        REFRESH_REV.send_modify(|revision| *revision = revision.wrapping_add(1));
    }
}

async fn sweep(
    gate: &Arc<AppGate>,
    status: &Arc<RwLock<GateStatus>>,
    held: &mut HashMap<(u16, IpAddr), tokio::task::JoinHandle<()>>,
    shutdown_rx: &tokio::sync::watch::Receiver<bool>,
) {
    // Re-read the manifests every sweep rather than trusting the map built
    // at construction. An app installed while the daemon is running would
    // otherwise never be gated until the next restart — and it would not
    // appear in `unprotected` either, so the node would report itself fully
    // enforced while serving a brand-new app to anyone who asked.
    gate.refresh().await;
    let port_map = gate.port_map().await;

    // Release claims whose port left the gated set (or whose Tor-upstream
    // claim lost its declaration). Aborting the accept loop drops the
    // listener, freeing the address for whoever now legitimately owns it —
    // the app itself, or nobody.
    held.retain(|(port, addr), handle| {
        let keep = match port_map.gated(*port) {
            None => false,
            Some(app) => *addr != GATE_TOR_UPSTREAM || app.declared,
        };
        if !keep {
            handle.abort();
            info!(port, %addr, "app gate released a claim: port is no longer gated here");
        }
        keep
    });

    let addresses = host_addresses().await;
    if addresses.is_empty() {
        debug!("app gate: no external addresses yet");
        return;
    }

    let mut claimed = Vec::new();
    let mut unprotected = Vec::new();

    for app in port_map.gated_ports() {
        // Nothing is listening on this port, so there is no app to protect
        // and binding would steal the port from an install that has not
        // happened yet. The relay loop learned this the hard way: binding a
        // port for an app that is not installed makes its later install hit
        // "address already in use", and the install's port-free step then
        // kills the daemon holding it.
        if !app_is_listening(app.port).await {
            continue;
        }

        let mut claimed_any = false;
        let mut blocked = false;
        // External addresses first, then the gate's Tor upstream. 127.0.0.2
        // deliberately does NOT count toward `claimed_any`: the warning below
        // is about external exposure, and a port whose only claim is the Tor
        // loopback is still wide open on the LAN.
        for &addr in &addresses {
            let key = (app.port, addr);
            if held.contains_key(&key) {
                claimed.push((app.port, addr.to_string()));
                claimed_any = true;
                continue;
            }
            match TcpListener::bind(SocketAddr::new(addr, app.port)).await {
                Ok(listener) => {
                    let handle =
                        spawn_accept_loop(listener, gate.clone(), app.clone(), shutdown_rx.clone());
                    held.insert(key, handle);
                    claimed.push((app.port, addr.to_string()));
                    claimed_any = true;
                    info!(
                        port = app.port, %addr, app = %app.app_id,
                        "app gate claimed an app port"
                    );
                }
                // Almost always the app itself holding 0.0.0.0:<port>.
                Err(_) => blocked = true,
            }
        }
        // The Tor upstream is bound for DECLARED gated ports only: torrc only
        // repoints an onion at 127.0.0.2 for a declared port, and standing a
        // challenge on an undeclared port's would-be upstream would change
        // where its traffic goes on nothing but a default.
        if app.declared {
            let tor_key = (app.port, GATE_TOR_UPSTREAM);
            if held.contains_key(&tor_key) {
                claimed.push((app.port, GATE_TOR_UPSTREAM.to_string()));
            } else {
                match TcpListener::bind(SocketAddr::new(GATE_TOR_UPSTREAM, app.port)).await {
                    Ok(listener) => {
                        let handle = spawn_accept_loop(
                            listener,
                            gate.clone(),
                            app.clone(),
                            shutdown_rx.clone(),
                        );
                        held.insert(tor_key, handle);
                        claimed.push((app.port, GATE_TOR_UPSTREAM.to_string()));
                        info!(
                            port = app.port, app = %app.app_id,
                            "app gate claimed the Tor upstream (127.0.0.2)"
                        );
                    }
                    Err(_) => blocked = true,
                }
            }
        }

        if blocked && !claimed_any {
            warn!(
                port = app.port, app = %app.app_id,
                "APP GATE CANNOT PROTECT THIS PORT — the app still publishes on all \
                 interfaces. Pin its manifest port to bind: 127.0.0.1 and recreate the \
                 container, or it stays reachable without authentication."
            );
            unprotected.push(UnprotectedPort {
                port: app.port,
                app_id: app.app_id.clone(),
                app_name: app.app_name.clone(),
                reason: "app publishes on all interfaces; manifest port needs bind: 127.0.0.1"
                    .to_string(),
            });
        }
    }

    claimed.sort();
    unprotected.sort_by_key(|u| u.port);
    let mut guard = status.write().await;
    guard.claimed = claimed;
    guard.unprotected = unprotected;
}

/// Is anything answering on loopback for this port?
async fn app_is_listening(port: u16) -> bool {
    tokio::time::timeout(
        std::time::Duration::from_millis(300),
        tokio::net::TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .ok()
    .and_then(|r| r.ok())
    .is_some()
}

/// Returns the accept-loop task handle so the sweep can release the claim
/// (abort → listener drops → address freed) when the port leaves the gated
/// set. In-flight connections finish on their own tasks.
fn spawn_accept_loop(
    listener: TcpListener,
    gate: Arc<AppGate>,
    app: GatedPort,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let Ok((stream, peer)) = accepted else { break };
                    let gate = gate.clone();
                    let app = app.clone();
                    tokio::spawn(async move {
                        serve_connection(stream, peer, gate, app).await;
                    });
                }
                _ = shutdown_rx.changed() => break,
            }
        }
    })
}

/// How long a freshly-accepted connection has to send its first byte.
///
/// The peek below blocks until *something* arrives, so without this an
/// unauthenticated caller could hold a task open indefinitely by connecting and
/// saying nothing — the same slowloris shape the header-read timeout guards
/// against, one step earlier in the handshake.
const FIRST_BYTE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Serve one connection, as TLS or plain HTTP depending on what the client
/// actually sent.
///
/// The first byte decides: `peek` inspects it *without consuming it*, so a TLS
/// client's ClientHello reaches the acceptor whole. This is what lets one port
/// serve an HTTP dashboard's frames and an HTTPS dashboard's frames on the same
/// node without a second port number or a per-node build.
async fn serve_connection(
    stream: tokio::net::TcpStream,
    peer: SocketAddr,
    gate: Arc<AppGate>,
    app: GatedPort,
) {
    let mut first = [0u8; 1];
    let peeked = tokio::time::timeout(FIRST_BYTE_TIMEOUT, stream.peek(&mut first)).await;

    let is_tls = match peeked {
        Ok(Ok(1)) => super::tls::looks_like_tls(first[0]),
        // 0 bytes is a clean close before any request; anything else is a
        // read error or the timeout. Nothing to serve either way.
        _ => {
            debug!(%peer, "app gate connection closed before sending anything");
            return;
        }
    };

    if is_tls {
        match gate.tls.acceptor().await {
            Some(acceptor) => match acceptor.accept(stream).await {
                Ok(tls_stream) => serve_http(tls_stream, peer, gate, app).await,
                Err(e) => {
                    // Routine: a browser probing a cert it does not trust, or a
                    // scanner. Not operator-actionable, so debug.
                    debug!(%peer, error = %e, "app gate TLS handshake failed");
                }
            },
            None => {
                // The client speaks TLS and this node has no certificate.
                // Replying in plain HTTP would be unreadable garbage to it, so
                // close and let the browser report the connection failure.
                debug!(
                    %peer,
                    "app gate got a TLS connection but has no certificate — closing"
                );
            }
        }
    } else {
        serve_http(stream, peer, gate, app).await;
    }
}

/// The HTTP half, generic over the transport so TLS and plain share one path —
/// the gate's authentication, proxying and upgrade handling must not differ by
/// scheme, and generics make that structural rather than a thing to remember.
async fn serve_http<S>(stream: S, peer: SocketAddr, gate: Arc<AppGate>, app: GatedPort)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let service = hyper::service::service_fn(move |req| {
        let gate = gate.clone();
        let app = app.clone();
        async move { Ok::<_, std::convert::Infallible>(gate.handle(req, &app, peer.ip()).await) }
    });
    let _ = hyper::server::conn::Http::new()
        // Same slowloris guard as the main listener: an unauthenticated caller
        // must not be able to hold a connection open by never sending headers.
        .http1_header_read_timeout(std::time::Duration::from_secs(30))
        .serve_connection(stream, service)
        .with_upgrades()
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn host_addresses_excludes_loopback() {
        for addr in host_addresses().await {
            assert!(!addr.is_loopback(), "{addr} is loopback");
            assert!(!addr.is_unspecified());
        }
    }

    #[tokio::test]
    async fn app_is_listening_is_false_for_a_dead_port() {
        // Bind and immediately drop, so the port is known-free.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(!app_is_listening(port).await);
    }

    #[tokio::test]
    async fn app_is_listening_is_true_for_a_live_port() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(app_is_listening(port).await);
    }

    #[test]
    fn a_status_with_unprotected_ports_is_not_fully_enforced() {
        let mut status = GateStatus::default();
        assert!(status.is_fully_enforced());
        status.unprotected.push(UnprotectedPort {
            port: 8090,
            app_id: "strfry".into(),
            app_name: "Strfry".into(),
            reason: "test".into(),
        });
        assert!(!status.is_fully_enforced());
    }
}

#[cfg(test)]
mod readiness_tests {
    use super::*;
    #[test]
    fn readiness_requires_external_and_tor_claims_for_the_same_port() {
        let mut status = GateStatus::default();
        assert!(!port_claimed(&status, 3001));
        status.claimed.push((3001, "127.0.0.2".into()));
        assert!(!port_claimed(&status, 3001));
        status.claimed.push((3002, "192.0.2.10".into()));
        assert!(!port_claimed(&status, 3001));
        status.claimed.push((3001, "192.0.2.10".into()));
        assert!(port_claimed(&status, 3001));
    }
}
