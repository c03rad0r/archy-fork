//! Local browser playback. Only opaque local handles cross the browser boundary.
use super::{build_response, ApiHandler};
use crate::{content_purchase::Journal, content_server::ByteRange, identity::NodeIdentity};
use anyhow::{Context, Result};
use hyper::{Body, HeaderMap, Method, Response, StatusCode};
use std::{
    io,
    sync::Arc,
    time::{Duration, Instant},
};

fn requested_bounds(headers: &HeaderMap, total: u64) -> Result<Option<(u64, u64)>> {
    anyhow::ensure!(total > 0, "Empty purchased media");
    anyhow::ensure!(
        headers.get_all("range").iter().count() <= 1,
        "Ambiguous playback ranges"
    );
    let Some(header) = headers.get("range") else {
        return Ok(None);
    };
    let range = crate::content_server::parse_range_header(header.to_str()?)
        .context("Invalid playback byte range")?;
    let last = total - 1;
    let (start, end) = match range {
        ByteRange::From { start, end } => (start, end.unwrap_or(last).min(last)),
        ByteRange::Suffix(count) => {
            anyhow::ensure!(count > 0, "Invalid byte range");
            (total.saturating_sub(count), last)
        }
    };
    anyhow::ensure!(start <= end && start < total, "Invalid playback byte range");
    Ok(Some((start, end)))
}
fn validate_upstream(
    status: u16,
    headers: &HeaderMap,
    total: u64,
    bounds: Option<(u64, u64)>,
    now: u64,
) -> Result<(u16, u64, String, u64)> {
    let expected_status = if bounds.is_some() { 206 } else { 200 };
    anyhow::ensure!(
        status == expected_status,
        "Seller returned another range status"
    );
    let length = bounds.map_or(total, |(start, end)| end - start + 1);
    anyhow::ensure!(
        headers
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            == Some(length),
        "Seller changed purchased byte length"
    );
    if let Some((start, end)) = bounds {
        let expected = format!("bytes {start}-{end}/{total}");
        anyhow::ensure!(
            headers.get("content-range").and_then(|v| v.to_str().ok()) == Some(expected.as_str()),
            "Seller changed purchased byte range"
        );
    }
    let mime = headers
        .get("content-type")
        .context("Missing media type")?
        .to_str()?
        .to_owned();
    anyhow::ensure!(
        mime.starts_with("video/") || mime.starts_with("audio/"),
        "Unsupported rental media type"
    );
    let expires = headers
        .get("x-rental-expires-at")
        .context("Missing rental expiry")?
        .to_str()?
        .parse::<u64>()?;
    anyhow::ensure!(expires > now, "Rental viewing window ended");
    Ok((expected_status, length, mime, expires))
}

fn installed_playback_origin(
    origin: &str,
    expected: &str,
    host: &str,
    gated_tls_port: bool,
) -> bool {
    let (Ok(actual), Ok(expected), Ok(request)) = (
        reqwest::Url::parse(origin),
        reqwest::Url::parse(expected),
        reqwest::Url::parse(&format!("http://{host}")),
    ) else {
        return false;
    };
    if !matches!(actual.scheme(), "http" | "https")
        || actual.origin().ascii_serialization() != origin
        || request.path() != "/"
        || !request.username().is_empty()
        || request.password().is_some()
        || request.query().is_some()
        || request.fragment().is_some()
    {
        return false;
    }
    if actual.origin() == expected.origin() {
        return true;
    }
    let same_port = actual.port_or_known_default() == expected.port_or_known_default();
    let scheme = actual.scheme() == expected.scheme()
        || (gated_tls_port && actual.scheme() == "https" && expected.scheme() == "http");
    let expected_loopback = matches!(
        expected.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]")
    );
    same_port
        && scheme
        && actual.host_str() == request.host_str()
        && (expected_loopback || actual.host_str() == expected.host_str())
}

fn unix_now() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs())
}
fn stream_error(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}

impl ApiHandler {
    pub(super) async fn handle_local_rental_request(
        &self,
        method: &Method,
        path: &str,
        headers: &HeaderMap,
    ) -> Result<Response<Body>> {
        // HEAD is deliberately not GET: a browser probe must never open a lease.
        if method != Method::GET && method != Method::OPTIONS {
            return Ok(Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .header("Allow", "GET, OPTIONS")
                .header("Cache-Control", "no-store")
                .body(Body::empty())?);
        }
        let origin = headers.get("origin").map(|v| v.to_str()).transpose()?;
        if let Some(origin) = origin {
            let identity =
                NodeIdentity::load_existing(&self.config.data_dir.join("identity")).await?;
            let (state, _) = self.state_manager.get_snapshot().await;
            let root = self.config.data_dir.clone();
            let context = tokio::task::spawn_blocking(move || {
                crate::container::registration_pin::installed_context(&root, &identity, &state)
            })
            .await??;
            let host = headers
                .get("host")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            let port = reqwest::Url::parse(origin)
                .ok()
                .and_then(|url| url.port_or_known_default());
            let ports = self.rpc_handler.app_gate.port_map().await;
            let gated_tls_port = port
                .and_then(|port| ports.gated(port))
                .is_some_and(|gate| gate.app_id == context.app_id && gate.declared);
            if !context
                .app_origins
                .iter()
                .any(|allowed| installed_playback_origin(origin, allowed, host, gated_tls_port))
            {
                return Ok(build_response(
                    StatusCode::FORBIDDEN,
                    "text/plain",
                    Body::from("Playback origin is not the installed app"),
                ));
            }
        }
        let mut response = if method == Method::OPTIONS {
            Response::builder()
                .status(StatusCode::NO_CONTENT)
                .body(Body::empty())?
        } else {
            self.handle_local_rental(path, headers).await?
        };
        response
            .headers_mut()
            .insert("Cache-Control", "private, no-store".parse()?);
        response.headers_mut().insert("Vary", "Origin".parse()?);
        if let Some(origin) = origin {
            response
                .headers_mut()
                .insert("Access-Control-Allow-Origin", origin.parse()?);
            response
                .headers_mut()
                .insert("Access-Control-Allow-Credentials", "true".parse()?);
            response
                .headers_mut()
                .insert("Access-Control-Allow-Methods", "GET, OPTIONS".parse()?);
            response
                .headers_mut()
                .insert("Access-Control-Allow-Headers", "Range".parse()?);
            response.headers_mut().insert(
                "Access-Control-Expose-Headers",
                "Content-Length, Content-Range, Accept-Ranges, X-Rental-Expires-At".parse()?,
            );
        }
        Ok(response)
    }

    /// Dispatcher accepts GET only after normal session handling. HEAD and other
    /// methods never reach upstream, so metadata probes cannot start a lease.
    pub(super) async fn handle_local_rental(
        &self,
        path: &str,
        headers: &HeaderMap,
    ) -> Result<Response<Body>> {
        let token = match crate::session::extract_session_cookie(headers) {
            Some(token) if self.session_store.validate(&token).await => token,
            _ => return Ok(Self::unauthorized()),
        };
        let handle = path
            .strip_prefix("/api/rental-playback/")
            .context("Invalid playback route")?;
        let identity =
            Arc::new(NodeIdentity::load_existing(&self.config.data_dir.join("identity")).await?);
        let (state, _) = self.state_manager.get_snapshot().await;
        let root = self.config.data_dir.clone();
        let key = identity.clone();
        let context = tokio::task::spawn_blocking(move || {
            crate::container::registration_pin::installed_context(&root, &key, &state)
        })
        .await??;
        let binding = self
            .rpc_handler
            .playback_handles()
            .lookup(handle, &token, &context)?;
        let capability = {
            let journal = Journal::open(&self.config.data_dir).await?;
            let record = journal
                .buyer(&binding.contract.id)
                .await?
                .context("Original purchase is missing")?;
            anyhow::ensure!(
                record.contract == binding.contract,
                "Original purchase changed"
            );
            record
                .receipt()
                .context("Original purchase is not settled")?
                .capability
                .clone()
        };
        let peer = crate::federation::load_unique_payment_peer(
            &self.config.data_dir,
            &binding.seller_onion,
        )
        .await?;
        anyhow::ensure!(
            peer.did == binding.contract.seller_did,
            "Purchased seller identity changed"
        );
        let mesh = peer
            .fips_npub
            .context("Seller mesh binding is unavailable")?;
        let total = binding.contract.content_size;
        let bounds = match requested_bounds(headers, total) {
            Ok(bounds) => bounds,
            Err(_) => {
                return Ok(Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header("Content-Range", format!("bytes */{total}"))
                    .body(Body::empty())?)
            }
        };
        let remote_path = format!(
            "/content/{}/rental/{}",
            binding.contract.content_id, binding.contract.id
        );
        let mut request =
            crate::fips::dial::PeerRequest::new(Some(&mesh), &binding.seller_onion, &remote_path)
                .require_fips()
                .single_delivery()
                .timeout(Duration::from_secs(24 * 60 * 60))
                .header("X-Content-Capability", capability);
        if let Some((start, end)) = bounds {
            request = request.header("Range", format!("bytes={start}-{end}"));
        }
        let (response, transport) = tokio::time::timeout(
            Duration::from_secs(20),
            request.send_content_get(&self.config.data_dir),
        )
        .await
        .context("Seller did not begin the original rental stream")??;
        if !response.status().is_success() {
            // Never forward arbitrary upstream bodies, redirects, cookies or private headers.
            let status = if response.status().as_u16() == 403 {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::BAD_GATEWAY
            };
            return Ok(build_response(
                status,
                "text/plain",
                Body::from(
                    "Original rental is unavailable; recover this purchase without paying again",
                ),
            ));
        }
        let (expected_status, length, mime, expires) = validate_upstream(
            response.status().as_u16(),
            response.headers(),
            total,
            bounds,
            unix_now()?,
        )?;
        self.rpc_handler
            .playback_handles()
            .note_expiry(handle, &binding, expires)?;
        let sessions = self.session_store.clone();
        let state_manager = self.state_manager.clone();
        let data_dir = self.config.data_dir.clone();
        let chunks = futures_util::stream::try_unfold(
            (response, length, None::<Instant>),
            move |(mut response, left, mut checked)| {
                let sessions = sessions.clone();
                let token = token.clone();
                let state_manager = state_manager.clone();
                let data_dir = data_dir.clone();
                let identity = identity.clone();
                let context = context.clone();
                async move {
                    if unix_now().map_err(|_| stream_error("Playback clock unavailable"))?
                        >= expires
                    {
                        return Err(stream_error("Rental viewing window ended"));
                    }
                    if left == 0 {
                        return Ok::<_, io::Error>(None);
                    }
                    let waiting_since = Instant::now();
                    loop {
                        if waiting_since.elapsed() >= Duration::from_secs(30) {
                            return Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "Rental stream stalled; reopen the original purchase",
                            ));
                        }
                        if unix_now().map_err(|_| stream_error("Playback clock unavailable"))?
                            >= expires
                        {
                            return Err(stream_error("Rental viewing window ended"));
                        }
                        if checked.is_none_or(|at| at.elapsed() >= Duration::from_secs(1)) {
                            if !sessions.validate(&token).await {
                                return Err(stream_error("Playback session ended"));
                            }
                            let (state, _) = state_manager.get_snapshot().await;
                            let root = data_dir.clone();
                            let key = identity.clone();
                            let actual = tokio::task::spawn_blocking(move || {
                                crate::container::registration_pin::installed_context(
                                    &root, &key, &state,
                                )
                            })
                            .await
                            .map_err(|_| stream_error("Playback app context unavailable"))?
                            .map_err(|_| stream_error("Playback app context unavailable"))?;
                            if actual != context {
                                return Err(stream_error("Playback app context changed"));
                            }
                            checked = Some(Instant::now());
                        }
                        // Keep checking revocation while the peer stalls; no local media cache.
                        let chunk = tokio::select! {
                            chunk=response.chunk() => chunk.map_err(|_|io::Error::new(io::ErrorKind::ConnectionAborted,"Rental stream interrupted; reopen the original purchase"))?,
                            _=tokio::time::sleep(Duration::from_secs(1)) => continue,
                        };
                        let bytes = chunk.ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "Purchased media ended early",
                            )
                        })?;
                        if bytes.len() as u64 > left {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Purchased media exceeded its declared length",
                            ));
                        }
                        let remaining = left - bytes.len() as u64;
                        return Ok(Some((bytes, (response, remaining, checked))));
                    }
                }
            },
        );
        let mut result = Response::builder()
            .status(expected_status)
            .header("Content-Type", mime)
            .header("Content-Length", length)
            .header("Accept-Ranges", "bytes")
            .header("Cache-Control", "private, no-store")
            .header("X-Content-Type-Options", "nosniff")
            .header("X-Rental-Expires-At", expires)
            .header("X-Archipelago-Transport", transport.to_string());
        if let Some((start, end)) = bounds {
            result = result.header("Content-Range", format!("bytes {start}-{end}/{total}"));
        }
        // D3: accumulate these bytes against the BUYER, so the byte cap has something to
        // charge. The buyer is the rental contract's holder, not a mesh peer — this is the
        // buyer proxy, so the viewer is identified by the entitlement they presented.
        // `counted` (no peer) is kept for the local/unauthenticated sites, which have
        // nobody to bill.
        Ok(
            result.body(Body::wrap_stream(crate::media_metrics::counted_for(
                chunks,
                &binding.contract.id,
                crate::media_meter::SERVICE_MEDIA_STREAM,
            )))?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installed_origin_maps_only_current_host_and_verified_app_port() {
        assert!(installed_playback_origin(
            "http://192.168.1.5:7778",
            "http://127.0.0.1:7778",
            "192.168.1.5",
            false
        ));
        assert!(installed_playback_origin(
            "https://192.168.1.5:7778",
            "http://127.0.0.1:7778",
            "192.168.1.5",
            true
        ));
        assert!(installed_playback_origin(
            "https://[fd00::5]:7778",
            "https://[::1]:7778",
            "[fd00::5]:443",
            false
        ));
        for (actual, host, tls) in [
            ("https://192.168.1.5:7778", "192.168.1.5", false),
            ("http://evil.test:7778", "192.168.1.5", true),
            ("http://192.168.1.5:7779", "192.168.1.5", true),
            ("http://192.168.1.5:7778", "evil.test", true),
            ("http://192.168.1.5:7778/path", "192.168.1.5", true),
            ("http://192.168.1.5:7778", "user@192.168.1.5", true),
        ] {
            assert!(
                !installed_playback_origin(actual, "http://127.0.0.1:7778", host, tls),
                "{actual} {host}"
            );
        }
    }

    #[test]
    fn range_bounds_follow_purchased_size_and_reject_ambiguous_ranges() {
        let check = |range: &str| {
            let mut h = HeaderMap::new();
            h.insert("range", range.parse().unwrap());
            requested_bounds(&h, 100)
        };
        assert_eq!(check("bytes=20-39").unwrap(), Some((20, 39)));
        assert_eq!(check("bytes=90-").unwrap(), Some((90, 99)));
        assert_eq!(check("bytes=-10").unwrap(), Some((90, 99)));
        assert_eq!(check("bytes=-200").unwrap(), Some((0, 99)));
        assert_eq!(check("bytes=90-500").unwrap(), Some((90, 99)));
        for range in [
            "bytes=100-",
            "bytes=20-10",
            "bytes=-0",
            "bytes=0-1,4-6",
            "other=0-1",
        ] {
            assert!(check(range).is_err(), "{range}");
        }
        assert_eq!(requested_bounds(&HeaderMap::new(), 100).unwrap(), None);
        assert!(requested_bounds(&HeaderMap::new(), 0).is_err());
    }
    #[test]
    fn upstream_range_expiry_and_media_headers_are_bound_before_bytes_escape() {
        let mut headers = HeaderMap::new();
        for (name, value) in [
            ("content-length", "20"),
            ("content-range", "bytes 20-39/100"),
            ("content-type", "video/mp4"),
            ("x-rental-expires-at", "200"),
        ] {
            headers.insert(name, value.parse().unwrap());
        }
        assert_eq!(
            validate_upstream(206, &headers, 100, Some((20, 39)), 100).unwrap(),
            (206, 20, "video/mp4".into(), 200)
        );
        assert!(validate_upstream(200, &headers, 100, Some((20, 39)), 100).is_err());
        assert!(validate_upstream(206, &headers, 100, Some((20, 39)), 200).is_err());
        for (name, bad) in [
            ("content-length", "21"),
            ("content-range", "bytes 21-40/100"),
            ("content-type", "text/html"),
            ("x-rental-expires-at", "0"),
        ] {
            let mut changed = headers.clone();
            changed.insert(name, bad.parse().unwrap());
            assert!(
                validate_upstream(206, &changed, 100, Some((20, 39)), 100).is_err(),
                "{name}"
            );
        }
        headers.remove("content-range");
        headers.insert("content-length", "100".parse().unwrap());
        assert!(validate_upstream(200, &headers, 100, None, 100).is_ok());
    }

    #[tokio::test]
    async fn metadata_probes_and_unauthenticated_get_do_not_touch_purchase_or_identity() {
        let root = tempfile::tempdir().unwrap();
        let mut config = crate::config::Config::default();
        config.data_dir = root.path().to_path_buf();
        let handler = ApiHandler::new(
            config,
            Arc::new(crate::state::StateManager::new()),
            Arc::new(crate::monitoring::MetricsStore::new()),
            None,
            None,
        )
        .await
        .unwrap();
        // ApiHandler initialization may establish its own node identity, but the
        // denied route must not need installed apps, saved receipts or any peer.
        for method in [Method::HEAD, Method::POST] {
            let result = handler
                .handle_request(
                    hyper::Request::builder()
                        .method(method)
                        .uri("/api/rental-playback/invalid")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(result.status(), StatusCode::METHOD_NOT_ALLOWED);
        }
        let result = handler
            .handle_request(
                hyper::Request::builder()
                    .uri("/api/rental-playback/invalid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(result.status(), StatusCode::UNAUTHORIZED);
        assert!(!root.path().join("content-purchases").exists());
    }
}
