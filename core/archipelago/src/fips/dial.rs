//! Dial peers over the FIPS mesh.
//!
//! The FIPS daemon exposes a local DNS resolver on `127.0.0.1:5354` that
//! answers AAAA queries for `<npub>.fips` with the peer's ULA address on
//! the `fips0` TUN. Once resolved we speak plain HTTP to the peer on
//! [`PEER_PORT`] — the same port `127.0.0.1:5678` where the archipelago
//! backend serves the existing signed peer-to-peer endpoints
//! (`/rpc/v1`, `/archipelago/node-message`, `/content/{id}`, …). The
//! server-side binding to the `fips0` address is handled in `server.rs`.
//!
//! The module is deliberately dependency-free for DNS — one packet in,
//! one packet out, standard RFC 1035 wire format — to avoid pulling
//! hickory-resolver's transitive tree for a single AAAA query.
//!
//! On any failure (daemon down, peer not in the identity cache, TUN
//! unreachable) callers fall back to the Tor transport.
//!
//! # Examples
//! ```ignore
//! let base = crate::fips::dial::peer_base_url("npub1…").await?;
//! // base = "http://[fd9d:…]:5678"
//! let client = crate::fips::dial::client();
//! let resp = client.get(format!("{}/content/abc", base)).send().await?;
//! ```
#![allow(dead_code)]

use super::telemetry::{self, FallbackReason};
use anyhow::{Context, Result};
use std::net::{IpAddr, Ipv6Addr};
use std::time::Duration;
use tokio::net::UdpSocket;

/// Port the archipelago backend listens on for FIPS peer-to-peer traffic.
/// Separate from the localhost-only internal port (5678) so the per-listener
/// path filter can restrict the exposed surface.
pub const PEER_PORT: u16 = 5679;

/// Whether a FIPS-side HTTP status should trigger a fall-back to Tor in
/// `Auto` mode. A `404` over FIPS often means the peer's mesh listener
/// doesn't expose that path (e.g. a peer on an older build with a stricter
/// `is_peer_allowed_path`), and `5xx` is a server-side error — both are
/// worth retrying over Tor, which reaches a different (less-filtered) route.
/// Success, redirects, and other 4xx (auth / bad request) are authoritative
/// and are returned as-is so we neither mask real errors nor double latency.
fn fips_should_fall_back(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::NOT_FOUND || status.is_server_error()
}

/// Is this FIPS answer the final one, or should the request go again over
/// Tor? A single-delivery request already reached the peer, so any answer
/// is final: a Tor replay would carry the same (possibly spent) payload.
fn fips_answer_is_final(
    pref: crate::settings::transport::TransportPref,
    single_delivery: bool,
    status: reqwest::StatusCode,
) -> bool {
    pref == crate::settings::transport::TransportPref::Fips
        || single_delivery
        || !fips_should_fall_back(status)
}

/// May a failed FIPS attempt be sent again? Only a failed connect proves the
/// peer never saw it; a timeout can land after the request was delivered.
fn fips_retryable(single_delivery: bool, e: &reqwest::Error) -> bool {
    e.is_connect() || (!single_delivery && e.is_timeout())
}

/// DNS suffix appended to a peer's bech32 npub.
pub const FIPS_DNS_SUFFIX: &str = "fips";

/// FIPS daemon's local DNS resolver.
pub const FIPS_DNS_ADDR: &str = "127.0.0.1:5354";

/// Short DNS query timeout — FIPS DNS is a local process; a slow answer
/// almost certainly means the daemon is gone.
const DNS_TIMEOUT: Duration = Duration::from_secs(2);

/// DNS AAAA query type.
const QTYPE_AAAA: u16 = 28;

/// DNS IN class.
const QCLASS_IN: u16 = 1;

/// Resolve a peer's bech32 npub to their `fips0` ULA address via the local
/// FIPS DNS resolver.
pub async fn resolve(npub: &str) -> Result<Ipv6Addr> {
    let sock = UdpSocket::bind("127.0.0.1:0")
        .await
        .context("bind UDP socket for FIPS DNS")?;
    sock.connect(FIPS_DNS_ADDR)
        .await
        .context("connect to FIPS DNS")?;

    // KEY-05: source named. A 2-byte DNS transaction id, not key material, so it
    // is drawn unguarded — an "all bytes identical" predicate on two bytes
    // false-positives once in 256, which would be worse than the defect.
    let id: u16 = rand::RngCore::next_u32(&mut rand::rngs::OsRng) as u16;
    let query = encode_query(id, npub)?;
    tokio::time::timeout(DNS_TIMEOUT, sock.send(&query))
        .await
        .context("FIPS DNS query timed out on send")?
        .context("FIPS DNS send")?;

    let mut buf = [0u8; 512];
    let n = tokio::time::timeout(DNS_TIMEOUT, sock.recv(&mut buf))
        .await
        .context("FIPS DNS query timed out on recv")?
        .context("FIPS DNS recv")?;

    decode_response(id, &buf[..n], npub)
}

/// Return a peer's base URL on the FIPS overlay, e.g. `http://[fd9d:…]:5678`.
pub async fn peer_base_url(npub: &str) -> Result<String> {
    let ip = resolve(npub).await?;
    Ok(format!("http://[{}]:{}", ip, PEER_PORT))
}

/// Build an HTTP client tuned for FIPS peer-to-peer dialing. No proxy.
/// `connect_timeout` is generous enough to let NAT hole-punching complete on
/// the first dial (FIPS is UDP hole-punched; the path often isn't established
/// until the first packets flow), so a reachable-but-cold peer isn't abandoned
/// to Tor prematurely. Reliability over latency — FIPS is the preferred path.
pub fn client() -> reqwest::Client {
    client_with_timeout(Duration::from_secs(20))
}

/// FIPS client with a caller-chosen overall request timeout. The static 20s
/// `client()` budget is fine for catalog browses and short calls, but a large
/// content download (#38) needs the per-request timeout the caller asked for —
/// otherwise a 178MB transfer is aborted at 20s and the whole download fails
/// before the Tor fallback ever gets a chance. The generous `connect_timeout`
/// is preserved so a cold hole-punched path still gets time to establish.
pub fn client_with_timeout(timeout: Duration) -> reqwest::Client {
    client_with_delivery_policy(timeout, false)
}

fn delivery_redirect_policy(single: bool) -> reqwest::redirect::Policy {
    if single {
        reqwest::redirect::Policy::none()
    } else {
        reqwest::redirect::Policy::default()
    }
}

fn client_with_delivery_policy(timeout: Duration, single: bool) -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(delivery_redirect_policy(single))
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(8))
        .user_agent("archipelago-fips/1")
        .build()
        .expect("static reqwest client config")
}

/// Send a FIPS request with ONE retry on a connect/timeout error.
///
/// The first dial to a peer typically triggers NAT hole-punching and can time
/// out before the overlay path is established; a quick retry then lands on the
/// now-warm path. Without this, a single cold-path failure drops the call to
/// Tor even though the peer is FIPS-reachable — the main reason FIPS "isn't
/// robust". Only connect/timeout errors are retried (a real HTTP response,
/// including 4xx/5xx, is returned as-is for the caller to interpret).
async fn send_with_retry(rb: reqwest::RequestBuilder) -> Result<reqwest::Response, reqwest::Error> {
    send_with_retry_if(rb, |e| e.is_connect() || e.is_timeout()).await
}

/// [`send_with_retry`], retrying only on errors `retryable` accepts.
async fn send_with_retry_if(
    rb: reqwest::RequestBuilder,
    retryable: impl Fn(&reqwest::Error) -> bool,
) -> Result<reqwest::Response, reqwest::Error> {
    let retry = rb.try_clone();
    match rb.send().await {
        Ok(resp) => Ok(resp),
        Err(e) if retryable(&e) && retry.is_some() => {
            // Brief pause so the hole-punch packets from the first attempt can
            // traverse before we re-dial onto the warmed path.
            tokio::time::sleep(Duration::from_millis(600)).await;
            retry.expect("retry builder present").send().await
        }
        Err(e) => Err(e),
    }
}

/// Proactively warm the hole-punched FIPS path to a peer: resolve its overlay
/// address and open a short connection to its peer listener. Hole-punched
/// paths and NAT mappings go cold after ~30-60s of no traffic, after which the
/// next real dial pays the full re-punch cost and often falls back to Tor.
/// Keeping the path warm is what makes FIPS the transport that actually gets
/// used. Best-effort: any error (peer offline, UDP blocked) is ignored — the
/// connection attempt itself is what re-punches and refreshes the path.
pub async fn warm_path(npub: &str) {
    if !is_service_active().await {
        return;
    }
    warm_path_unchecked(npub).await
}

/// [`warm_path`] without the service-active check — for callers (the warm
/// tick) that already verified the daemon once for the whole batch.
pub async fn warm_path_unchecked(npub: &str) {
    let Ok(base) = peer_base_url(npub).await else {
        return;
    };
    let c = client();
    // The response status is irrelevant; establishing the connection warms it.
    let _ = tokio::time::timeout(Duration::from_secs(8), c.get(&base).send()).await;
}

// ── DNS wire-format helpers ─────────────────────────────────────────────

fn encode_query(id: u16, npub: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(64 + npub.len());
    // Header
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&0x0100u16.to_be_bytes()); // RD=1, std query
    out.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    out.extend_from_slice(&0u16.to_be_bytes()); // ANCOUNT
    out.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
    out.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT

    // QNAME — two labels: "<npub>" and "fips".
    encode_label(&mut out, npub)?;
    encode_label(&mut out, FIPS_DNS_SUFFIX)?;
    out.push(0); // root
                 // QTYPE + QCLASS
    out.extend_from_slice(&QTYPE_AAAA.to_be_bytes());
    out.extend_from_slice(&QCLASS_IN.to_be_bytes());
    Ok(out)
}

fn encode_label(out: &mut Vec<u8>, label: &str) -> Result<()> {
    if label.is_empty() || label.len() > 63 {
        anyhow::bail!("invalid DNS label length: {}", label.len());
    }
    out.push(label.len() as u8);
    out.extend_from_slice(label.as_bytes());
    Ok(())
}

fn decode_response(expected_id: u16, buf: &[u8], npub: &str) -> Result<Ipv6Addr> {
    if buf.len() < 12 {
        anyhow::bail!("DNS response too short");
    }
    let id = u16::from_be_bytes([buf[0], buf[1]]);
    if id != expected_id {
        anyhow::bail!("DNS response id mismatch");
    }
    let rcode = buf[3] & 0x0F;
    if rcode != 0 {
        anyhow::bail!("DNS rcode {} resolving {}.fips", rcode, npub);
    }
    let qdcount = u16::from_be_bytes([buf[4], buf[5]]) as usize;
    let ancount = u16::from_be_bytes([buf[6], buf[7]]) as usize;
    if ancount == 0 {
        anyhow::bail!("no AAAA record for {}.fips", npub);
    }

    let mut pos = 12;
    // Skip question section(s)
    for _ in 0..qdcount {
        pos = skip_name(buf, pos)?;
        pos = pos
            .checked_add(4)
            .ok_or_else(|| anyhow::anyhow!("qsection overflow"))?;
        if pos > buf.len() {
            anyhow::bail!("qsection past end");
        }
    }

    // Walk answers; return the first valid AAAA rdata.
    for _ in 0..ancount {
        pos = skip_name(buf, pos)?;
        if pos + 10 > buf.len() {
            anyhow::bail!("answer RR past end");
        }
        let rtype = u16::from_be_bytes([buf[pos], buf[pos + 1]]);
        let rclass = u16::from_be_bytes([buf[pos + 2], buf[pos + 3]]);
        let rdlength = u16::from_be_bytes([buf[pos + 8], buf[pos + 9]]) as usize;
        pos += 10;
        if pos + rdlength > buf.len() {
            anyhow::bail!("rdata past end");
        }
        if rtype == QTYPE_AAAA && rclass == QCLASS_IN && rdlength == 16 {
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&buf[pos..pos + 16]);
            return Ok(Ipv6Addr::from(octets));
        }
        pos += rdlength;
    }
    anyhow::bail!("no AAAA answer for {}.fips", npub)
}

/// Advance past a DNS name (handles compressed pointers). Returns the
/// position immediately after the name.
fn skip_name(buf: &[u8], mut pos: usize) -> Result<usize> {
    loop {
        if pos >= buf.len() {
            anyhow::bail!("name past end");
        }
        let len = buf[pos];
        if len == 0 {
            return Ok(pos + 1);
        }
        if len & 0xC0 == 0xC0 {
            // Compressed pointer — 2 bytes total, no further labels.
            if pos + 2 > buf.len() {
                anyhow::bail!("pointer past end");
            }
            return Ok(pos + 2);
        }
        if len & 0xC0 != 0 {
            anyhow::bail!("reserved label type");
        }
        pos = pos
            .checked_add(1 + len as usize)
            .ok_or_else(|| anyhow::anyhow!("name overflow"))?;
    }
}

/// Treat `IpAddr::V6` as the raw address for ergonomic callers.
pub fn as_ip_addr(v6: Ipv6Addr) -> IpAddr {
    IpAddr::V6(v6)
}

// ── High-level peer request helpers ────────────────────────────────────

/// TTL for the [`is_service_active`] cache. Every FIPS dial attempt and
/// every warm-tick peer used to spawn up to two `systemctl` subprocesses;
/// service state changes on human timescales, so 10s staleness is free.
const SERVICE_ACTIVE_TTL_MS: u64 = 10_000;
static SERVICE_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static SERVICE_PROBED_AT_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Quick poll: is the FIPS daemon (archipelago-supervised OR upstream)
/// currently `systemctl is-active`? Cached for [`SERVICE_ACTIVE_TTL_MS`];
/// concurrent refreshes are harmless (idempotent probe, last write wins).
pub async fn is_service_active() -> bool {
    use std::sync::atomic::Ordering;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let probed_at = SERVICE_PROBED_AT_MS.load(Ordering::Relaxed);
    if probed_at != 0 && now_ms.saturating_sub(probed_at) < SERVICE_ACTIVE_TTL_MS {
        return SERVICE_ACTIVE.load(Ordering::Relaxed);
    }
    let mut active = false;
    for unit in [
        crate::fips::SERVICE_UNIT,
        crate::fips::UPSTREAM_SERVICE_UNIT,
    ] {
        if crate::fips::service::unit_state(unit).await == "active" {
            active = true;
            break;
        }
    }
    SERVICE_ACTIVE.store(active, Ordering::Relaxed);
    SERVICE_PROBED_AT_MS.store(now_ms, Ordering::Relaxed);
    active
}

/// Builder for a peer request that may be sent over FIPS (preferred) or
/// Tor (fallback). The call sites migrating off direct-Tor dialing build
/// one of these and call [`send_json`] / [`send_get`]; the helper handles
/// dial, timeout, fallback, and cross-transport auth headers.
///
/// The optional `service` field ties the request to a user-configurable
/// transport preference (see `crate::settings::transport`). Leaving it
/// unset picks Auto (FIPS preferred, Tor fallback) — the same default as
/// before the Settings UI landed.
pub struct PeerRequest<'a> {
    pub fips_npub: Option<&'a str>,
    pub onion_host: &'a str,
    pub path: &'a str,
    pub headers: Vec<(&'a str, String)>,
    pub timeout: std::time::Duration,
    /// Optional shorter cap on the FIPS *attempt* only. When set, a cold or hung
    /// FIPS overlay fails fast within this budget so the Tor fallback still gets
    /// its full `timeout` — without it, a stuck FIPS dial can consume the whole
    /// caller budget (e.g. a 60s frontend RPC) and the request "times out" even
    /// though Tor would have answered (#6, the Pay-with-QR invoice request).
    /// `None` keeps the legacy behavior (FIPS uses the full `timeout`), which a
    /// large content download needs so its long FIPS transfer isn't truncated.
    pub fips_timeout: Option<std::time::Duration>,
    pub service: Option<crate::settings::transport::PeerService>,
    /// When set, the transport that actually served this request is written
    /// to federation storage (`record_peer_transport`, matched by onion) so
    /// the per-peer FIPS/Tor badge reflects reality. Opt-in because not
    /// every caller has a data dir in scope.
    pub record_data_dir: Option<std::path::PathBuf>,
    /// The request carries something that must reach the peer at most once
    /// (a bearer ecash token). See [`PeerRequest::single_delivery`].
    pub single_delivery: bool,
}

impl<'a> PeerRequest<'a> {
    pub fn new(fips_npub: Option<&'a str>, onion_host: &'a str, path: &'a str) -> Self {
        Self {
            fips_npub,
            onion_host,
            path,
            headers: Vec::new(),
            timeout: std::time::Duration::from_secs(30),
            fips_timeout: None,
            service: None,
            record_data_dir: None,
            single_delivery: false,
        }
    }

    /// Never send this request twice. A paid download carries a bearer ecash
    /// token that the seller redeems on first sight; replaying it over Tor
    /// after FIPS already delivered it hands the seller a spent token, so the
    /// buyer is charged and gets a 402 instead of the file (2026-09-29: FIPS
    /// answered 404 after the seller redeemed, the Tor retry got 402).
    ///
    /// With this set, whatever FIPS answers is final, the FIPS retry fires
    /// only when the first attempt never connected, and Tor is used only when
    /// FIPS could not have delivered the request. An attempt that may have
    /// been delivered but timed out is an error, not a fallback.
    pub fn single_delivery(mut self) -> Self {
        self.single_delivery = true;
        self
    }

    /// Record the transport that serves this request into federation storage
    /// (matched by this request's onion host). Best-effort, off the hot path.
    pub fn record_transport(mut self, data_dir: impl Into<std::path::PathBuf>) -> Self {
        self.record_data_dir = Some(data_dir.into());
        self
    }

    fn spawn_record(&self, kind: crate::transport::TransportKind) {
        if let Some(dir) = &self.record_data_dir {
            let dir = dir.clone();
            let onion = self.onion_host.to_string();
            let transport = kind.to_string();
            tokio::spawn(async move {
                let _ =
                    crate::federation::record_peer_transport(&dir, None, Some(&onion), &transport)
                        .await;
            });
        }
    }

    /// Cap the FIPS attempt to a shorter budget than the overall `timeout`, so a
    /// cold/hung overlay path fails fast and the Tor fallback keeps its full
    /// budget. Use on short request/response calls (invoice, status); leave
    /// unset for large downloads that legitimately need a long FIPS transfer.
    pub fn fips_timeout(mut self, t: std::time::Duration) -> Self {
        self.fips_timeout = Some(t);
        self
    }

    /// Timeout to apply to the FIPS attempt — the explicit cap if set, else the
    /// overall request timeout.
    fn fips_attempt_timeout(&self) -> std::time::Duration {
        self.fips_timeout.unwrap_or(self.timeout)
    }

    /// Tie this request to a user-configurable service preference. If
    /// the user has set that service to `Fips` or `Tor`, the builder
    /// respects it.
    pub fn service(mut self, s: crate::settings::transport::PeerService) -> Self {
        self.service = Some(s);
        self
    }

    pub fn header(mut self, name: &'a str, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }

    pub fn timeout(mut self, t: std::time::Duration) -> Self {
        self.timeout = t;
        self
    }

    /// Resolved preference: user setting if `service` was set, else Auto.
    async fn preference(&self) -> crate::settings::transport::TransportPref {
        match self.service {
            Some(s) => crate::settings::transport::get(s).await,
            None => crate::settings::transport::TransportPref::Auto,
        }
    }

    /// POST a JSON body. Returns the `reqwest::Response` — caller decides
    /// how to interpret the status code.
    pub async fn send_json<B: serde::Serialize>(
        &self,
        body: &B,
    ) -> Result<(reqwest::Response, crate::transport::TransportKind)> {
        use crate::settings::transport::TransportPref;
        let pref = self.preference().await;
        // FIPS-only or Auto: try FIPS first.
        if matches!(pref, TransportPref::Auto | TransportPref::Fips) {
            match self.try_fips_post_json(body).await? {
                Some(resp) => {
                    // Use the FIPS reply unless it's one a Tor retry could
                    // fix (404 path-not-served / 5xx) and we're allowed to
                    // fall back. FIPS-only never falls back.
                    if fips_answer_is_final(pref, self.single_delivery, resp.status()) {
                        telemetry::record_fips_ok();
                        self.spawn_record(crate::transport::TransportKind::Fips);
                        return Ok((resp, crate::transport::TransportKind::Fips));
                    }
                    let reason = if resp.status() == reqwest::StatusCode::NOT_FOUND {
                        FallbackReason::Http404
                    } else {
                        FallbackReason::Http5xx
                    };
                    telemetry::record_fallback(reason);
                    tracing::info!(
                        reason = reason.key(),
                        status = %resp.status(),
                        "FIPS POST {} answered but status triggers Tor fallback",
                        self.path
                    );
                }
                None => {
                    if pref == TransportPref::Fips {
                        anyhow::bail!(
                            "User set transport preference to FIPS only, but peer is unreachable over FIPS"
                        );
                    }
                }
            }
        }
        let resp = self.send_tor_post_json(body).await?;
        self.spawn_record(crate::transport::TransportKind::Tor);
        Ok((resp, crate::transport::TransportKind::Tor))
    }

    /// GET with optional header-based auth.
    pub async fn send_get(&self) -> Result<(reqwest::Response, crate::transport::TransportKind)> {
        use crate::settings::transport::TransportPref;
        let pref = self.preference().await;
        if matches!(pref, TransportPref::Auto | TransportPref::Fips) {
            match self.try_fips_get().await? {
                Some(resp) => {
                    if fips_answer_is_final(pref, self.single_delivery, resp.status()) {
                        telemetry::record_fips_ok();
                        self.spawn_record(crate::transport::TransportKind::Fips);
                        return Ok((resp, crate::transport::TransportKind::Fips));
                    }
                    let reason = if resp.status() == reqwest::StatusCode::NOT_FOUND {
                        FallbackReason::Http404
                    } else {
                        FallbackReason::Http5xx
                    };
                    telemetry::record_fallback(reason);
                    tracing::info!(
                        reason = reason.key(),
                        status = %resp.status(),
                        "FIPS GET {} answered but status triggers Tor fallback",
                        self.path
                    );
                }
                None => {
                    if pref == TransportPref::Fips {
                        anyhow::bail!(
                            "User set transport preference to FIPS only, but peer is unreachable over FIPS"
                        );
                    }
                }
            }
        }
        let resp = self.send_tor_get().await?;
        self.spawn_record(crate::transport::TransportKind::Tor);
        Ok((resp, crate::transport::TransportKind::Tor))
    }

    async fn try_fips_post_json<B: serde::Serialize>(
        &self,
        body: &B,
    ) -> Result<Option<reqwest::Response>> {
        let Some(npub) = self.fips_npub else {
            telemetry::record_fallback(FallbackReason::NoNpub);
            return Ok(None);
        };
        if !is_service_active().await {
            telemetry::record_fallback(FallbackReason::ServiceInactive);
            return Ok(None);
        }
        let base = match peer_base_url(npub).await {
            Ok(b) => b,
            Err(e) => {
                telemetry::record_fallback(FallbackReason::DnsFail);
                tracing::info!(
                    reason = FallbackReason::DnsFail.key(),
                    "FIPS resolve for {} failed: {}, falling back to Tor",
                    npub,
                    e
                );
                return Ok(None);
            }
        };
        let url = format!("{}{}", base, self.path);
        let budget = self.fips_attempt_timeout();
        // With an explicit fast-fail cap, halve the per-attempt client
        // timeout so the one-retry path in send_with_retry fits inside the
        // budget instead of silently doubling it ("fips_timeout(6s)" used
        // to really mean ~12.6s). Without one (long streaming downloads),
        // keep the full budget per attempt — the client timeout also
        // governs body streaming and must not truncate a real transfer.
        let per_attempt = if self.fips_timeout.is_some() {
            budget / 2
        } else {
            budget
        };
        let c = client_with_delivery_policy(per_attempt, self.single_delivery);
        let mut rb = c.post(&url).json(body);
        for (k, v) in &self.headers {
            rb = rb.header(*k, v);
        }
        let single = self.single_delivery;
        let attempt = send_with_retry_if(rb, |e| fips_retryable(single, e));
        match tokio::time::timeout(budget, attempt).await {
            Ok(Ok(r)) => Ok(Some(r)),
            Ok(Err(e)) if single && !e.is_connect() => Err(anyhow::anyhow!(
                "FIPS POST failed after possible delivery; not replaying: {e}"
            )),
            Err(_) if single => Err(anyhow::anyhow!(
                "FIPS POST exceeded its budget after possible delivery; not replaying"
            )),
            Ok(Err(e)) => {
                telemetry::record_fallback(FallbackReason::ConnectFail);
                tracing::info!(
                    reason = FallbackReason::ConnectFail.key(),
                    "FIPS POST {} failed after retry: {}, falling back to Tor",
                    url,
                    e
                );
                Ok(None)
            }
            Err(_) => {
                telemetry::record_fallback(FallbackReason::ConnectFail);
                tracing::info!(
                    reason = FallbackReason::ConnectFail.key(),
                    "FIPS POST {} exceeded attempt budget {:?}, falling back to Tor",
                    url,
                    budget
                );
                Ok(None)
            }
        }
    }

    async fn try_fips_get(&self) -> Result<Option<reqwest::Response>> {
        let Some(npub) = self.fips_npub else {
            telemetry::record_fallback(FallbackReason::NoNpub);
            return Ok(None);
        };
        if !is_service_active().await {
            telemetry::record_fallback(FallbackReason::ServiceInactive);
            return Ok(None);
        }
        let base = match peer_base_url(npub).await {
            Ok(b) => b,
            Err(e) => {
                telemetry::record_fallback(FallbackReason::DnsFail);
                tracing::info!(
                    reason = FallbackReason::DnsFail.key(),
                    "FIPS resolve for {} failed: {}, falling back to Tor",
                    npub,
                    e
                );
                return Ok(None);
            }
        };
        let url = format!("{}{}", base, self.path);
        let budget = self.fips_attempt_timeout();
        // Same budget discipline as the POST path: halve per attempt only
        // under an explicit fast-fail cap; hard-cap the retry sequence.
        let per_attempt = if self.fips_timeout.is_some() {
            budget / 2
        } else {
            budget
        };
        let c = client_with_delivery_policy(per_attempt, self.single_delivery);
        let mut rb = c.get(&url);
        for (k, v) in &self.headers {
            rb = rb.header(*k, v);
        }
        let single = self.single_delivery;
        let attempt = send_with_retry_if(rb, |e| fips_retryable(single, e));
        match tokio::time::timeout(budget, attempt).await {
            Ok(Ok(r)) => Ok(Some(r)),
            // Anything but a failed connect may have reached the peer.
            Ok(Err(e)) if single && !e.is_connect() => Err(anyhow::anyhow!(
                "FIPS GET {} failed after the request may have been delivered \
                 (not retrying over Tor): {}",
                self.path,
                e
            )),
            Err(_) if single => Err(anyhow::anyhow!(
                "FIPS GET {} exceeded its {:?} budget after the request may have \
                 been delivered (not retrying over Tor)",
                self.path,
                budget
            )),
            Ok(Err(e)) => {
                telemetry::record_fallback(FallbackReason::ConnectFail);
                tracing::info!(
                    reason = FallbackReason::ConnectFail.key(),
                    "FIPS GET {} failed after retry: {}, falling back to Tor",
                    url,
                    e
                );
                Ok(None)
            }
            Err(_) => {
                telemetry::record_fallback(FallbackReason::ConnectFail);
                tracing::info!(
                    reason = FallbackReason::ConnectFail.key(),
                    "FIPS GET {} exceeded attempt budget {:?}, falling back to Tor",
                    url,
                    budget
                );
                Ok(None)
            }
        }
    }

    async fn send_tor_post_json<B: serde::Serialize>(&self, body: &B) -> Result<reqwest::Response> {
        let url = self.tor_url();
        let client = self.tor_client()?;
        let mut rb = client.post(&url).json(body);
        for (k, v) in &self.headers {
            rb = rb.header(*k, v);
        }
        rb.send().await.with_context(|| format!("Tor POST {}", url))
    }

    async fn send_tor_get(&self) -> Result<reqwest::Response> {
        let url = self.tor_url();
        let client = self.tor_client()?;
        let mut rb = client.get(&url);
        for (k, v) in &self.headers {
            rb = rb.header(*k, v);
        }
        rb.send().await.with_context(|| format!("Tor GET {}", url))
    }

    fn tor_url(&self) -> String {
        let host = if self.onion_host.ends_with(".onion") {
            self.onion_host.to_string()
        } else {
            format!("{}.onion", self.onion_host)
        };
        format!("http://{}{}", host, self.path)
    }

    fn tor_client(&self) -> Result<reqwest::Client> {
        let proxy = reqwest::Proxy::all(crate::constants::TOR_SOCKS_PROXY)
            .context("Invalid Tor SOCKS proxy URL")?;
        reqwest::Client::builder()
            .proxy(proxy)
            .redirect(delivery_redirect_policy(self.single_delivery))
            .timeout(self.timeout)
            .build()
            .context("Build Tor HTTP client")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_query_round_trip_header_is_correct() {
        let q = encode_query(0x1234, "npub1abc").unwrap();
        assert_eq!(&q[0..2], &[0x12, 0x34]);
        assert_eq!(&q[2..4], &[0x01, 0x00]); // flags RD=1
        assert_eq!(&q[4..6], &[0x00, 0x01]); // QDCOUNT=1
                                             // Tail: QTYPE=28, QCLASS=1
        assert_eq!(&q[q.len() - 4..], &[0x00, 0x1C, 0x00, 0x01]);
    }

    #[test]
    fn encode_query_includes_both_labels() {
        let q = encode_query(0, "npub1xyz").unwrap();
        assert!(q.windows(9).any(|w| w == b"\x08npub1xyz"));
        assert!(q.windows(5).any(|w| w == b"\x04fips"));
    }

    #[test]
    fn decode_response_returns_aaaa_rdata() {
        // Minimal crafted response: header + qsection + one AAAA answer.
        let id = 0xBEEFu16;
        let mut r = Vec::new();
        r.extend_from_slice(&id.to_be_bytes());
        r.extend_from_slice(&0x8180u16.to_be_bytes()); // QR=1, RD=1, RA=1, rcode=0
        r.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
        r.extend_from_slice(&1u16.to_be_bytes()); // ANCOUNT
        r.extend_from_slice(&0u16.to_be_bytes()); // NSCOUNT
        r.extend_from_slice(&0u16.to_be_bytes()); // ARCOUNT
                                                  // Question: 1 label "a" + "fips"
        r.extend_from_slice(b"\x01a\x04fips\x00");
        r.extend_from_slice(&QTYPE_AAAA.to_be_bytes());
        r.extend_from_slice(&QCLASS_IN.to_be_bytes());
        // Answer: compressed name pointing at question offset 12
        r.extend_from_slice(&[0xC0, 0x0C]);
        r.extend_from_slice(&QTYPE_AAAA.to_be_bytes());
        r.extend_from_slice(&QCLASS_IN.to_be_bytes());
        r.extend_from_slice(&300u32.to_be_bytes()); // TTL
        r.extend_from_slice(&16u16.to_be_bytes()); // RDLENGTH
        let ip: Ipv6Addr = "fd9d:1192:e800:bad0:eed3:4b0e:b273:8e0e".parse().unwrap();
        r.extend_from_slice(&ip.octets());
        let got = decode_response(id, &r, "a").unwrap();
        assert_eq!(got, ip);
    }

    #[test]
    fn decode_rejects_id_mismatch() {
        let r = vec![0u8; 12];
        let err = decode_response(0x1234, &r, "x").unwrap_err();
        assert!(err.to_string().contains("id mismatch"));
    }

    #[test]
    fn decode_rejects_rcode() {
        let mut r = vec![0u8; 12];
        r[0] = 0xAA;
        r[1] = 0xBB;
        r[3] = 3; // NXDOMAIN
        let err = decode_response(0xAABB, &r, "x").unwrap_err();
        assert!(err.to_string().contains("rcode 3"));
    }

    #[test]
    fn decode_rejects_empty_answer_section() {
        let mut r = vec![0u8; 12];
        r[0] = 0xAA;
        r[1] = 0xBB;
        r[4] = 0;
        r[5] = 0; // QDCOUNT=0
        r[6] = 0;
        r[7] = 0; // ANCOUNT=0
        let err = decode_response(0xAABB, &r, "x").unwrap_err();
        assert!(err.to_string().contains("no AAAA"));
    }

    #[test]
    fn a_single_delivery_answer_is_final_whatever_its_status() {
        use crate::settings::transport::TransportPref;
        use reqwest::StatusCode;
        // Regression (2026-09-29): the seller redeemed a paid download's
        // token, answered 404, and the Tor fallback replayed the spent token.
        for status in [
            StatusCode::NOT_FOUND,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::OK,
        ] {
            assert!(fips_answer_is_final(TransportPref::Auto, true, status));
        }
        // Everything else keeps the existing fallback rules.
        assert!(!fips_answer_is_final(
            TransportPref::Auto,
            false,
            StatusCode::NOT_FOUND
        ));
        assert!(!fips_answer_is_final(
            TransportPref::Auto,
            false,
            StatusCode::BAD_GATEWAY
        ));
        assert!(fips_answer_is_final(
            TransportPref::Auto,
            false,
            StatusCode::PAYMENT_REQUIRED
        ));
        assert!(fips_answer_is_final(
            TransportPref::Fips,
            false,
            StatusCode::NOT_FOUND
        ));
    }

    /// A listener that accepts connections and never answers, counting them.
    async fn silent_peer() -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = seen.clone();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                held.push(stream); // keep it open, never reply
            }
        });
        (format!("http://{addr}/content/x"), seen)
    }

    #[tokio::test]
    async fn a_single_delivery_request_is_not_resent_after_a_timeout() {
        let (url, seen) = silent_peer().await;
        let c = client_with_timeout(Duration::from_millis(300));
        let err = send_with_retry_if(c.get(&url), |e| fips_retryable(true, e))
            .await
            .expect_err("peer never answers");
        assert!(err.is_timeout());
        assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn an_ordinary_request_is_still_retried_once_after_a_timeout() {
        let (url, seen) = silent_peer().await;
        let c = client_with_timeout(Duration::from_millis(300));
        let _ = send_with_retry_if(c.get(&url), |e| fips_retryable(false, e)).await;
        assert_eq!(seen.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_single_delivery_request_still_retries_a_refused_connect() {
        // Nothing listening: the peer provably never saw the request.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let c = client_with_timeout(Duration::from_millis(500));
        let err = send_with_retry_if(c.get(format!("http://{addr}/")), |e| {
            fips_retryable(true, e)
        })
        .await
        .expect_err("nothing listening");
        assert!(err.is_connect());
        assert!(fips_retryable(true, &err));
    }
}

#[cfg(test)]
mod delivery_redirect_tests {
    use super::*;
    use hyper::{
        service::{make_service_fn, service_fn},
        Body, Response, Server,
    };
    use std::{
        convert::Infallible,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };

    #[tokio::test]
    async fn paid_bearer_request_does_not_follow_redirects_but_normal_get_does() {
        let seen = Arc::new(AtomicUsize::new(0));
        let counter = seen.clone();
        let server = Server::bind(&([127, 0, 0, 1], 0).into());
        let address = server.local_addr();
        let service = make_service_fn(move |_| {
            let counter = counter.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |request: hyper::Request<Body>| {
                    let counter = counter.clone();
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        let response = if request.uri().path() == "/first" {
                            Response::builder()
                                .status(302)
                                .header("Location", "/replay")
                                .body(Body::empty())
                                .unwrap()
                        } else {
                            Response::new(Body::from("replayed"))
                        };
                        Ok::<_, Infallible>(response)
                    }
                }))
            }
        });
        let task = tokio::spawn(server.serve(service));
        let url = format!("http://{address}/first");
        let response = client_with_delivery_policy(Duration::from_secs(2), true)
            .get(&url)
            .header("X-Payment-Token", "dummy-test-token")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::FOUND);
        assert_eq!(seen.load(Ordering::SeqCst), 1);
        let response = client_with_delivery_policy(Duration::from_secs(2), false)
            .get(url)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(seen.load(Ordering::SeqCst), 3);
        task.abort();
    }

    #[tokio::test]
    async fn paid_request_is_not_resent_when_peer_disconnects_after_reading_it() {
        use tokio::io::AsyncReadExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Arc::new(AtomicUsize::new(0));
        let counter = seen.clone();
        let task = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0; 4096];
                let _ = stream.read(&mut buf).await;
                counter.fetch_add(1, Ordering::SeqCst);
                drop(stream);
            }
        });
        let c = client_with_delivery_policy(Duration::from_secs(2), true);
        let error = send_with_retry_if(c.get(format!("http://{address}/")), |e| {
            fips_retryable(true, e)
        })
        .await
        .unwrap_err();
        assert!(!error.is_connect());
        assert_eq!(seen.load(Ordering::SeqCst), 1);
        task.abort();
    }
}
