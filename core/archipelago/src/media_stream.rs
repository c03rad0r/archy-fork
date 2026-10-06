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

pub async fn file_response(
    mut file: File,
    mime: &str,
    headers: &HeaderMap,
) -> Result<Response<Body>> {
    let metadata = file.metadata().await?;
    anyhow::ensure!(metadata.is_file(), "Media source is not a regular file");
    let total = metadata.len();
    let selected = match headers.get("range") {
        None => None,
        Some(value) => match value.to_str().ok().and_then(|value| range(value, total)) {
            Some(range) => Some(range),
            None => {
                return Ok(Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header("Content-Range", format!("bytes */{total}"))
                    .header("Accept-Ranges", "bytes")
                    .header("Cache-Control", "private, no-store")
                    .body(Body::empty())?)
            }
        },
    };
    let (start, length) = selected
        .map(|(start, end)| (start, end - start + 1))
        .unwrap_or((0, total));
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let chunks = futures::stream::try_unfold((file, length), |(mut file, left)| async move {
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
        .header("X-Content-Type-Options", "nosniff")
        .header("Cache-Control", "private, no-store")
        .header("X-Archipelago-Transport", "local-cache");
    if let Some((start, end)) = selected {
        response = response.header("Content-Range", format!("bytes {start}-{end}/{total}"));
    }
    Ok(response.body(Body::wrap_stream(chunks))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::body::HttpBody;
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
