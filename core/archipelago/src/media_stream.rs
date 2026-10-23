//! Bounded, seekable media responses. One file descriptor and at most 64 KiB
//! are retained per in-flight response; dropping the body closes the file.
use anyhow::Result;
use hyper::{Body, HeaderMap, Response, StatusCode};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
};

fn range(value: &str, total: u64) -> Option<(u64, u64)> {
    let (start, end) = value.strip_prefix("bytes=")?.split_once('-')?;
    if total == 0 || end.contains(',') {
        return None;
    }
    let number = |s: &str| {
        if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
            s.parse::<u64>().ok()
        } else {
            None
        }
    };
    if start.is_empty() {
        let length = number(end)?;
        return (length > 0).then_some((total.saturating_sub(length), total - 1));
    }
    let start = number(start)?;
    let end = if end.is_empty() {
        total - 1
    } else {
        number(end)?.min(total - 1)
    };
    (start <= end && start < total).then_some((start, end))
}

/// Validator for a media file, derived from its identity on disk.
///
/// **This is NOT a content hash, and the difference matters.** The plan requires cache keys
/// of `(content_sha256, content_size)` — that is a *cache-key* requirement for D1/D5, where
/// two replicas must agree on identity. An HTTP validator answers a different question:
/// "are the bytes I am about to send the same ones the client already has a prefix of?" For
/// that, size + mtime is the standard answer (nginx and Apache both do exactly this for
/// static files), costs one `stat` instead of reading the whole file per request, and is
/// sufficient for the property that matters: an object changed under a resuming client must
/// not be spliced into a partial mix.
///
/// Known limit, stated rather than hidden: a file replaced with byte-identical size AND
/// mtime would pass this check. Making it content-derived would mean hashing every byte
/// before the first chunk, which is a per-request full read — the exact cost P4 exists to
/// avoid, since resume is most valuable on objects too large to re-download.
///
/// Emitted in strong form (no `W/` prefix) because RFC 7232 §2.3.2 forbids using a weak
/// validator with `If-Range`, so a weak tag would make `If-Range` unusable — which is the
/// whole mechanism P4 needs.
fn etag_for(metadata: &std::fs::Metadata) -> String {
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("\"{}-{}\"", metadata.len(), mtime)
}

/// Should a `Range` request be honoured, given the client's validators?
///
/// RFC 7233 §3.2: if `If-Range` is present and does not match the current representation,
/// the server MUST ignore `Range` and send the whole body (200). That is precisely the
/// "object changed under the client" case: honouring the range would splice new bytes onto
/// an old prefix and hand back a corrupt file that looks like a successful resume.
fn range_is_honoured(headers: &HeaderMap, etag: &str) -> bool {
    match headers.get("if-range") {
        None => true,
        Some(value) => {
            let Ok(v) = value.to_str() else {
                // Unparseable validator: be conservative, send the whole body.
                return false;
            };
            // An HTTP-date If-Range is compared against Last-Modified, which this handler
            // does not emit, so an exact ETag match is the only accept case.
            v.trim() == etag
        }
    }
}

pub async fn file_response(
    mut file: File,
    mime: &str,
    headers: &HeaderMap,
) -> Result<Response<Body>> {
    let metadata = file.metadata().await?;
    anyhow::ensure!(metadata.is_file(), "Media source is not a regular file");
    let total = metadata.len();
    let etag = etag_for(&metadata);

    // Conditional GET: the client already has this exact representation.
    if let Some(inm) = headers.get("if-none-match").and_then(|v| v.to_str().ok()) {
        if inm.split(',').any(|t| t.trim() == etag) {
            return Ok(Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .header("ETag", &etag)
                .header("Accept-Ranges", "bytes")
                .header("Cache-Control", "private, no-store")
                .body(Body::empty())?);
        }
    }

    // A Range is only served when the client's validator still matches. On mismatch the
    // whole body is returned so a changed object can never be resumed into a partial mix.
    let may_range = range_is_honoured(headers, &etag);
    let selected = if may_range {
        match headers.get("range") {
            None => None,
            Some(value) => match value.to_str().ok().and_then(|value| range(value, total)) {
                Some(range) => Some(range),
                None => {
                    return Ok(Response::builder()
                        .status(StatusCode::RANGE_NOT_SATISFIABLE)
                        .header("Content-Range", format!("bytes */{total}"))
                        .header("Accept-Ranges", "bytes")
                        .header("ETag", &etag)
                        .header("Cache-Control", "private, no-store")
                        .body(Body::empty())?)
                }
            },
        }
    } else {
        None
    };
    let (start, length) = selected
        .map(|(start, end)| (start, end - start + 1))
        .unwrap_or((0, total));
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let chunks = futures_util::stream::try_unfold((file, length), |(mut file, left)| async move {
        if left == 0 {
            return Ok::<_, std::io::Error>(None);
        }
        let mut chunk = vec![0; left.min(64 * 1024) as usize];
        let read = file.read(&mut chunk).await?;
        if read == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Media changed during playback",
            ));
        }
        chunk.truncate(read);
        Ok(Some((chunk, (file, left - read as u64))))
    });
    let mut response = Response::builder()
        .status(if selected.is_some() {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header("Content-Type", mime)
        .header("Content-Length", length)
        .header("Accept-Ranges", "bytes")
        .header("ETag", &etag)
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "private, no-store")
        .header("X-Archipelago-Transport", "local-cache");
    if let Some((start, end)) = selected {
        response = response.header("Content-Range", format!("bytes {start}-{end}/{total}"));
    }
    Ok(response.body(Body::wrap_stream(crate::media_metrics::counted(chunks)))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::body::HttpBody;

    /// P4: a Range is honoured only when the client's validator still matches. This is the
    /// property that stops a changed object being resumed into a corrupt partial mix.
    #[test]
    fn if_range_mismatch_disables_range_and_matching_if_range_keeps_it() {
        let etag = "\"143360-1234567890\"";
        let mut headers = HeaderMap::new();

        // No If-Range: a plain Range request is honoured (today's behaviour).
        headers.insert("range", "bytes=100-".parse().unwrap());
        assert!(
            range_is_honoured(&headers, etag),
            "plain Range must still work"
        );

        // If-Range matching the current ETag: honour the range.
        headers.insert("if-range", etag.parse().unwrap());
        assert!(
            range_is_honoured(&headers, etag),
            "a matching If-Range must keep the range — this is the resume path"
        );

        // If-Range from an OLDER representation: refuse the range, send the whole body.
        headers.insert("if-range", "\"143360-9999999999\"".parse().unwrap());
        assert!(
            !range_is_honoured(&headers, etag),
            "a stale If-Range must disable the range, or a changed object gets spliced \
             onto an old prefix and the client receives a corrupt file that looks like a \
             successful resume"
        );

        // Garbage validator: conservative failure, not an accidental resume.
        headers.insert("if-range", "not-a-validator".parse().unwrap());
        assert!(!range_is_honoured(&headers, etag));

        // A weak validator must not be accepted: RFC 7232 forbids weak validators with
        // If-Range, and accepting one would resume across a changed body.
        headers.insert("if-range", "W/\"143360-1234567890\"".parse().unwrap());
        assert!(
            !range_is_honoured(&headers, etag),
            "weak validators must not resume"
        );
    }

    /// The ETag must be a STRONG validator (no W/ prefix) or If-Range is unusable, and it
    /// must change when the file changes — otherwise a resume splices two objects.
    #[test]
    fn etag_is_strong_and_tracks_size() {
        let dir = std::env::temp_dir().join(format!("archy-etag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let a = dir.join("a.bin");
        std::fs::write(&a, b"hello").unwrap();
        let meta_a = std::fs::metadata(&a).unwrap();
        let tag_a = etag_for(&meta_a);

        assert!(
            tag_a.starts_with('"') && tag_a.ends_with('"'),
            "must be quoted"
        );
        assert!(
            !tag_a.starts_with("W/"),
            "must be a STRONG validator for If-Range"
        );

        // Different size => different tag.
        let b = dir.join("b.bin");
        std::fs::write(&b, b"hello world").unwrap();
        let tag_b = etag_for(&std::fs::metadata(&b).unwrap());
        assert_ne!(tag_a, tag_b, "a changed size must change the validator");

        // Same size, different content written at a different time. Asserted via a real
        // mtime change (write, sleep past filesystem granularity, rewrite) rather than a
        // stub: a stubbed setter would make this assertion vacuous.
        let c = dir.join("c.bin");
        std::fs::write(&c, b"HELLO").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        std::fs::write(&c, b"WORLD").unwrap();
        let tag_c = etag_for(&std::fs::metadata(&c).unwrap());
        assert_ne!(
            tag_a, tag_c,
            "same size but a different mtime must change the validator, or a replaced \
             object resumes into a splice"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn ranges_cover_suffix_open_ended_clamping_and_rejection() {
        assert_eq!(range("bytes=2-5", 10), Some((2, 5)));
        assert_eq!(range("bytes=2-", 10), Some((2, 9)));
        assert_eq!(range("bytes=2-100", 10), Some((2, 9)));
        assert_eq!(range("bytes=-4", 10), Some((6, 9)));
        assert_eq!(range("bytes=-100", 10), Some((0, 9)));
        for value in [
            "bytes=-0",
            "bytes=10-",
            "bytes=8-3",
            "bytes=0-1,4-5",
            "bytes=+1-4",
            "bytes=18446744073709551616-",
            "nope",
        ] {
            assert_eq!(range(value, 10), None, "{value}");
        }
        assert_eq!(range("bytes=0-", 0), None);
    }
    #[tokio::test]
    async fn sparse_large_file_is_streamed_in_bounded_chunks_and_ranges_are_exact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("video");
        let file = File::create(&path).await.unwrap();
        file.set_len(4 * 1024 * 1024 * 1024).await.unwrap();
        drop(file);
        let mut full = file_response(
            File::open(&path).await.unwrap(),
            "video/mp4",
            &HeaderMap::new(),
        )
        .await
        .unwrap();
        assert_eq!(full.headers()["content-length"], "4294967296");
        assert_eq!(full.body_mut().data().await.unwrap().unwrap().len(), 65536);
        drop(full); // Cancellation must not read the remainder.
        let mut headers = HeaderMap::new();
        headers.insert("range", "bytes=-3".parse().unwrap());
        let response = file_response(File::open(&path).await.unwrap(), "video/mp4", &headers)
            .await
            .unwrap();
        assert_eq!(response.status(), 206);
        assert_eq!(
            response.headers()["content-range"],
            "bytes 4294967293-4294967295/4294967296"
        );
        assert_eq!(
            hyper::body::to_bytes(response.into_body())
                .await
                .unwrap()
                .as_ref(),
            &[0, 0, 0]
        );
        headers.insert("range", "bytes=4294967296-".parse().unwrap());
        assert_eq!(
            file_response(File::open(&path).await.unwrap(), "video/mp4", &headers)
                .await
                .unwrap()
                .status(),
            416
        );
    }
}
