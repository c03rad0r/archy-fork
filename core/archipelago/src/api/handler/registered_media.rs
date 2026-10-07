//! Authenticated immutable rental streaming, separate from legacy mutable shares.
use super::{build_response, ApiHandler};
use crate::{content_server::ByteRange, identity::NodeIdentity, registered_media::OpenedMedia};
use anyhow::{Context, Result};
use hyper::{Body, HeaderMap, Response, StatusCode};
use std::sync::Arc;

fn route(path: &str) -> Result<(&str, &str)> {
    let (content, purchase) = path
        .strip_prefix("/content/")
        .and_then(|value| value.split_once("/rental/"))
        .context("Invalid rental route")?;
    anyhow::ensure!(
        content.starts_with("registered_") && !content.contains('/') && !purchase.contains('/'),
        "Invalid rental identifiers"
    );
    let id = uuid::Uuid::parse_str(purchase)?;
    anyhow::ensure!(
        id.to_string() == purchase && id.get_version_num() == 4,
        "Invalid purchase identifier"
    );
    Ok((content, purchase))
}
fn bounds(range: Option<ByteRange>, total: u64) -> Result<Option<(u64, u64)>> {
    let Some(range) = range else {
        anyhow::ensure!(total > 0, "Registered media is empty");
        return Ok(None);
    };
    let last = total.checked_sub(1).context("Registered media is empty")?;
    let (start, end) = match range {
        ByteRange::From { start, end } => (start, end.unwrap_or(last).min(last)),
        ByteRange::Suffix(count) => {
            anyhow::ensure!(count > 0, "Invalid suffix range");
            (total.saturating_sub(count), last)
        }
    };
    anyhow::ensure!(start <= end && start < total, "Invalid rental byte range");
    Ok(Some((start, end)))
}
fn clock() -> u64 {
    u64::try_from(chrono::Utc::now().timestamp()).unwrap_or(0)
}
fn denied(message: &'static str) -> Response<Body> {
    build_response(StatusCode::FORBIDDEN, "text/plain", Body::from(message))
}

impl ApiHandler {
    pub(super) async fn handle_rental_control(
        &self,
        mut request: hyper::Request<Body>,
    ) -> Result<Response<Body>> {
        use hyper::body::HttpBody;
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Control {
            capability: String,
            ready_id: Option<String>,
            #[serde(default)]
            retry: bool,
        }
        let path = request.uri().path().to_owned();
        let (base, action) = path.rsplit_once('/').context("Invalid rental action")?;
        anyhow::ensure!(
            matches!(action, "prepare" | "start") && request.method() == hyper::Method::POST,
            "Invalid rental action"
        );
        let (content, purchase) = route(base)?;
        let bytes = tokio::time::timeout(std::time::Duration::from_secs(15), async {
            let mut bytes = Vec::new();
            while let Some(chunk) = request.body_mut().data().await {
                let chunk = chunk?;
                anyhow::ensure!(
                    bytes
                        .len()
                        .checked_add(chunk.len())
                        .is_some_and(|n| n <= 16 * 1024),
                    "Rental request too large"
                );
                bytes.extend_from_slice(&chunk);
            }
            Ok::<_, anyhow::Error>(bytes)
        })
        .await
        .context("Rental request timed out")??;
        let audience = crate::identity::did_key_from_pubkey_hex(&self.self_pubkey_hex)?;
        let buyer = crate::content_auth::authenticate_request(
            request.headers(),
            &audience,
            &hyper::Method::POST,
            &path,
            &bytes,
            chrono::Utc::now().timestamp(),
        )?;
        let input: Control = serde_json::from_slice(&bytes)?;
        let identity =
            Arc::new(NodeIdentity::load_existing(&self.config.data_dir.join("identity")).await?);
        anyhow::ensure!(identity.did_key()? == audience, "Node identity changed");
        let result = if action == "prepare" {
            anyhow::ensure!(input.ready_id.is_none(), "Prepare does not start a rental");
            let prior = crate::registered_media::paid_window(
                &self.config.data_dir,
                &identity,
                content,
                purchase,
                &buyer,
                &input.capability,
            )
            .await?;
            let metadata = crate::registered_media::registered_metadata(
                &self.config.data_dir,
                &identity,
                content,
            )?;
            anyhow::ensure!(
                prior
                    .as_ref()
                    .is_none_or(|window| clock() >= window.started_at),
                "Rental clock moved backwards"
            );
            if let Some(window) = prior.as_ref().filter(|window| clock() >= window.expires_at) {
                serde_json::json!({"state":"expired", "viewing_seconds":metadata.0.viewing_seconds,"started_at":window.started_at,"expires_at":window.expires_at})
            } else {
                let state = crate::registered_media::prepare_paid(
                    self.config.data_dir.clone(),
                    identity,
                    content.into(),
                    purchase.into(),
                    buyer,
                    input.capability,
                    input.retry,
                )
                .await?;
                let mut result = serde_json::to_value(state)?;
                result["viewing_seconds"] = serde_json::json!(metadata.0.viewing_seconds);
                result["started_at"] =
                    serde_json::json!(prior.as_ref().map(|window| window.started_at));
                result["expires_at"] =
                    serde_json::json!(prior.as_ref().map(|window| window.expires_at));
                result
            }
        } else {
            anyhow::ensure!(!input.retry, "Start cannot retry verification");
            let ready_id = input
                .ready_id
                .context("Media must be ready before explicit Start")?;
            let window = crate::registered_media::start_paid(
                self.config.data_dir.clone(),
                identity,
                content.into(),
                purchase.into(),
                buyer,
                input.capability,
                ready_id,
            )
            .await?;
            anyhow::ensure!(clock() >= window.started_at, "Rental clock moved backwards");
            serde_json::json!({"state": if clock() >= window.expires_at {"expired"} else {"started"},
                "started_at":window.started_at,"expires_at":window.expires_at})
        };
        Ok(build_response(
            StatusCode::OK,
            "application/json",
            Body::from(serde_json::to_vec(&result)?),
        ))
    }

    pub(super) async fn handle_registered_rental(
        &self,
        path: &str,
        headers: &HeaderMap,
    ) -> Result<Response<Body>> {
        let (content, purchase) = match route(path) {
            Ok(ids) => ids,
            Err(_) => {
                return Ok(build_response(
                    StatusCode::BAD_REQUEST,
                    "text/plain",
                    Body::from("Invalid rental route"),
                ))
            }
        };
        let audience = crate::identity::did_key_from_pubkey_hex(&self.self_pubkey_hex)?;
        let buyer = match crate::content_auth::incoming(
            headers,
            &audience,
            path,
            chrono::Utc::now().timestamp(),
        ) {
            Ok(Some(buyer)) => buyer,
            _ => return Ok(denied("Authenticated node proof is required")),
        };
        let capability = match headers
            .get("x-content-capability")
            .and_then(|v| v.to_str().ok())
        {
            Some(value) if value.len() == 64 => value.to_owned(),
            _ => return Ok(denied("Original purchase capability is required")),
        };
        let requested_range = match headers.get("range") {
            None => None,
            Some(value) => match value
                .to_str()
                .ok()
                .and_then(crate::content_server::parse_range_header)
            {
                Some(range) => Some(range),
                None => {
                    return Ok(build_response(
                        StatusCode::RANGE_NOT_SATISFIABLE,
                        "text/plain",
                        Body::from("Invalid byte range"),
                    ))
                }
            },
        };
        let identity =
            Arc::new(NodeIdentity::load_existing(&self.config.data_dir.join("identity")).await?);
        anyhow::ensure!(identity.did_key()? == audience, "Node identity changed");
        let data = self.config.data_dir.clone();
        let selected = content.to_owned();
        let key = identity.clone();
        let metadata = tokio::task::spawn_blocking(move || {
            crate::registered_media::registered_metadata(&data, &key, &selected)
        })
        .await?;
        let (receipt, _) = match metadata {
            Ok(value) => value,
            Err(_) => {
                return Ok(build_response(
                    StatusCode::NOT_FOUND,
                    "text/plain",
                    Body::from("Registered content is unavailable"),
                ))
            }
        };
        let total = receipt.size_bytes.parse::<u64>()?;
        let range = match bounds(requested_range, total) {
            Ok(value) => value,
            Err(_) => {
                return Ok(Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header("Content-Range", format!("bytes */{total}"))
                    .body(Body::from("Invalid byte range"))?)
            }
        };
        // All malformed/out-of-bounds requests are rejected before first-open
        // rental creation. No payment or new receipt is attempted by this route.
        let opened = match crate::registered_media::open_paid(
            self.config.data_dir.clone(),
            identity,
            content.into(),
            purchase.into(),
            buyer,
            capability,
        )
        .await
        {
            Ok(opened) => opened,
            Err(_) => {
                return Ok(denied(
                    "This purchase is not settled, does not match, or its rental has expired",
                ))
            }
        };
        rental_response(opened, range, Arc::new(clock)).await
    }
}
async fn rental_response(
    opened: OpenedMedia,
    range: Option<(u64, u64)>,
    now: Arc<dyn Fn() -> u64 + Send + Sync>,
) -> Result<Response<Body>> {
    anyhow::ensure!(
        opened.still_authorized(now()),
        "Rental expired before streaming"
    );
    let total = opened.size_bytes;
    let started = opened.started_at;
    let expires = opened.expires_at;
    let (start, length) = range.map_or((0, total), |(start, end)| (start, end - start + 1));
    let chunks = futures_util::stream::try_unfold(
        (opened.file, opened.verification, start, length, now),
        move |(mut file, verification, position, left, now)| async move {
            if left == 0 {
                return Ok::<_, std::io::Error>(None);
            }
            let instant = now();
            if instant < started || instant >= expires {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Rental window ended",
                ));
            }
            let read = tokio::task::spawn_blocking(move || {
                let bytes = verification
                    .index
                    .read_slice(&mut file, position, left.min(64 * 1024) as usize)
                    .map_err(std::io::Error::other)?;
                Ok::<_, std::io::Error>((file, verification, bytes))
            });
            let (file, verification, bytes) =
                tokio::time::timeout(std::time::Duration::from_secs(expires - instant), read)
                    .await
                    .map_err(|_| {
                        std::io::Error::new(std::io::ErrorKind::TimedOut, "Rental window ended")
                    })?
                    .map_err(std::io::Error::other)??;
            let instant = now();
            if instant < started || instant >= expires {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "Rental window ended",
                ));
            }
            let count = bytes.len() as u64;
            Ok(Some((
                bytes,
                (file, verification, position + count, left - count, now),
            )))
        },
    );
    let mut response = Response::builder()
        .status(if range.is_some() {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header("Content-Type", opened.mime_type)
        .header("Content-Length", length)
        .header("Accept-Ranges", "bytes")
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "private, no-store")
        .header("X-Rental-Expires-At", expires);
    if let Some((start, end)) = range {
        response = response.header("Content-Range", format!("bytes {start}-{end}/{total}"));
    }
    Ok(response.body(Body::wrap_stream(chunks))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::body::HttpBody;
    use std::sync::atomic::{AtomicU64, Ordering};
    #[test]
    fn invalid_routes_and_ranges_cannot_reach_rental_creation() {
        let id = uuid::Uuid::new_v4();
        assert!(route(&format!("/content/registered_{id}/rental/{id}")).is_ok());
        for path in [
            format!("/content/registered_{id}/rental/{id}/extra"),
            format!("/content/../rental/{id}"),
            format!("/content/registered_{id}/rental/not-a-purchase"),
        ] {
            assert!(route(&path).is_err());
        }
        assert!(bounds(
            Some(ByteRange::From {
                start: 20,
                end: None
            }),
            20
        )
        .is_err());
        assert!(bounds(
            Some(ByteRange::From {
                start: 9,
                end: Some(8)
            }),
            20
        )
        .is_err());
        assert_eq!(
            bounds(Some(ByteRange::Suffix(5)), 20).unwrap(),
            Some((15, 19))
        );
    }
    fn opened(size: u64) -> OpenedMedia {
        use sha2::{Digest, Sha256};
        let mut file = tempfile::tempfile().unwrap();
        file.set_len(size).unwrap();
        let binding = crate::rental_chunk_index::Binding {
            content_id: format!("registered_{}", uuid::Uuid::new_v4()),
            receipt_sha256: "ab".repeat(32),
            full_sha256: hex::encode(Sha256::digest(vec![0; size as usize])),
            size,
        };
        let index = crate::rental_chunk_index::Index::scan(&mut file, binding, |_| Ok(())).unwrap();
        OpenedMedia {
            file,
            verification: crate::rental_readiness::Ready::fixture(index),
            size_bytes: size,
            mime_type: "video/mp4".into(),
            started_at: 1000,
            expires_at: 1060,
        }
    }
    #[tokio::test]
    async fn bounded_stream_stops_at_persisted_deadline_without_restarting_window() {
        let clock = Arc::new(AtomicU64::new(1000));
        let read_clock = clock.clone();
        let mut response = rental_response(
            opened(200_000),
            None,
            Arc::new(move || read_clock.load(Ordering::SeqCst)),
        )
        .await
        .unwrap();
        assert_eq!(response.headers()["x-rental-expires-at"], "1060");
        assert_eq!(
            response.body_mut().data().await.unwrap().unwrap().len(),
            64 * 1024
        );
        clock.store(1060, Ordering::SeqCst);
        assert!(response.body_mut().data().await.unwrap().is_err());
    }
    #[tokio::test]
    async fn suffix_response_has_exact_length_and_expired_or_rollback_stream_denies() {
        let mut response = rental_response(opened(20), Some((15, 19)), Arc::new(|| 1000))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()["content-range"], "bytes 15-19/20");
        assert_eq!(
            hyper::body::to_bytes(response.body_mut())
                .await
                .unwrap()
                .len(),
            5
        );
        assert!(rental_response(opened(20), None, Arc::new(|| 1060))
            .await
            .is_err());
        assert!(rental_response(opened(20), None, Arc::new(|| 999))
            .await
            .is_err());
    }
}
