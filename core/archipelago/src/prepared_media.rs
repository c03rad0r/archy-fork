//! File-backed peer responses with bounded buffers and private payment snapshots.
//! Snapshot construction finishes before bearer ecash is redeemed. Anonymous
//! temporary files are removed automatically when the response is dropped.
use anyhow::{Context, Result};
use hyper::{Body, Response, StatusCode};
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

const CHUNK: usize = 64 * 1024;
const DISK_RESERVE: u64 = 256 * 1024 * 1024;
static RESERVED: Mutex<u64> = Mutex::new(0);
static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();

struct Reservation {
    bytes: u64,
    _slot: OwnedSemaphorePermit,
}
impl Drop for Reservation {
    fn drop(&mut self) {
        let mut reserved = RESERVED.lock().unwrap_or_else(|e| e.into_inner());
        *reserved = reserved.saturating_sub(self.bytes);
    }
}

fn reserve(file: &File, length: u64) -> Result<Reservation> {
    use std::os::fd::AsRawFd;
    let slot = SLOTS
        .get_or_init(|| Arc::new(Semaphore::new(4)))
        .clone()
        .try_acquire_owned()
        .context("Content preparation is busy")?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    // The live descriptor supplies the filesystem; no attacker-controlled C path.
    if unsafe { libc::fstatvfs(file.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error()).context("Checking content staging space");
    }
    let stat = unsafe { stat.assume_init() };
    let available = (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64);
    let mut reserved = RESERVED.lock().unwrap_or_else(|e| e.into_inner());
    let next = reserved
        .checked_add(length)
        .context("Content size overflow")?;
    anyhow::ensure!(
        next.checked_add(DISK_RESERVE.max(available / 20))
            .is_some_and(|n| n <= available),
        "Insufficient private staging space; no payment was redeemed"
    );
    *reserved = next;
    Ok(Reservation {
        bytes: length,
        _slot: slot,
    })
}

pub struct PreparedMedia {
    file: File,
    length: u64,
    mime: String,
    range: Option<(u64, u64, u64)>,
    reservation: Option<Reservation>,
}

impl PreparedMedia {
    pub async fn direct(
        mut file: File,
        start: u64,
        length: u64,
        mime: String,
        range: Option<(u64, u64, u64)>,
    ) -> Result<Self> {
        anyhow::ensure!(
            file.metadata().await?.is_file(),
            "Content is not a regular file"
        );
        file.seek(std::io::SeekFrom::Start(start)).await?;
        Ok(Self {
            file,
            length,
            mime,
            range,
            reservation: None,
        })
    }

    pub async fn snapshot<R: AsyncRead + Unpin>(
        data_dir: &Path,
        mut source: R,
        length: u64,
        mime: String,
        range: Option<(u64, u64, u64)>,
    ) -> Result<Self> {
        let dir = data_dir.join("content-staging");
        tokio::fs::create_dir_all(&dir).await?;
        let temporary = tokio::task::spawn_blocking(move || tempfile::tempfile_in(dir)).await??;
        let mut file = File::from_std(temporary);
        let reservation = reserve(&file, length)?;
        let mut left = length;
        let mut buffer = vec![0; CHUNK];
        while left > 0 {
            let limit = left.min(CHUNK as u64) as usize;
            let count = source.read(&mut buffer[..limit]).await?;
            anyhow::ensure!(
                count > 0,
                "Content changed while preparing payment response"
            );
            file.write_all(&buffer[..count]).await?;
            left -= count as u64;
        }
        // Detect writeback errors before the caller attempts bearer redemption.
        file.flush().await?;
        file.sync_data().await?;
        file.seek(std::io::SeekFrom::Start(0)).await?;
        Ok(Self {
            file,
            length,
            mime,
            range,
            reservation: Some(reservation),
        })
    }

    pub fn into_response(self) -> Result<Response<Body>> {
        let Self {
            file,
            length,
            mime,
            range,
            reservation,
        } = self;
        let chunks = futures_util::stream::try_unfold(
            (file, length, reservation),
            |(mut file, left, reservation)| async move {
                if left == 0 {
                    return Ok::<_, std::io::Error>(None);
                }
                let mut buffer = vec![0; left.min(CHUNK as u64) as usize];
                let count = file.read(&mut buffer).await?;
                if count == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "Content changed during transfer",
                    ));
                }
                buffer.truncate(count);
                Ok(Some((buffer, (file, left - count as u64, reservation))))
            },
        );
        let mut response = Response::builder()
            .status(if range.is_some() {
                StatusCode::PARTIAL_CONTENT
            } else {
                StatusCode::OK
            })
            .header("Content-Type", mime)
            .header("Content-Length", length)
            .header("Accept-Ranges", "bytes")
            .header("X-Content-Type-Options", "nosniff")
            .header("Cache-Control", "private, no-store");
        if let Some((start, end, total)) = range {
            response = response.header("Content-Range", format!("bytes {start}-{end}/{total}"));
        }
        Ok(response.body(Body::wrap_stream(crate::media_metrics::counted(chunks)))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::body::HttpBody;

    #[tokio::test]
    async fn direct_large_sparse_file_does_not_read_ahead_and_short_reads_fail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("film");
        let write = File::create(&path).await.unwrap();
        write.set_len(4 * 1024 * 1024 * 1024).await.unwrap();
        let mut response = PreparedMedia::direct(
            File::open(&path).await.unwrap(),
            0,
            4 * 1024 * 1024 * 1024,
            "video/mp4".into(),
            None,
        )
        .await
        .unwrap()
        .into_response()
        .unwrap();
        assert_eq!(response.headers()["content-length"], "4294967296");
        assert_eq!(
            response.body_mut().data().await.unwrap().unwrap().len(),
            CHUNK
        );
        write.set_len(0).await.unwrap();
        assert!(response.body_mut().data().await.unwrap().is_err());
        drop(response);
    }

    #[tokio::test]
    async fn snapshot_survives_original_removal_and_has_no_named_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("original");
        let bytes = vec![73; 3 * 1024 * 1024];
        tokio::fs::write(&path, &bytes).await.unwrap();
        let prepared = PreparedMedia::snapshot(
            dir.path(),
            File::open(&path).await.unwrap(),
            bytes.len() as u64,
            "application/octet-stream".into(),
            None,
        )
        .await
        .unwrap();
        tokio::fs::remove_file(path).await.unwrap();
        assert_eq!(
            std::fs::read_dir(dir.path().join("content-staging"))
                .unwrap()
                .count(),
            0
        );
        let mut response = prepared.into_response().unwrap();
        let mut received = Vec::new();
        while let Some(chunk) = response.body_mut().data().await {
            let chunk = chunk.unwrap();
            assert!(chunk.len() <= CHUNK);
            received.extend_from_slice(&chunk);
        }
        assert_eq!(received, bytes);
    }

    #[tokio::test]
    async fn incomplete_snapshot_fails_before_a_payment_can_be_attempted() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            PreparedMedia::snapshot(dir.path(), &b"short"[..], 100, "x".into(), None)
                .await
                .is_err()
        );
        assert_eq!(
            std::fs::read_dir(dir.path().join("content-staging"))
                .unwrap()
                .count(),
            0
        );
        // A subsequent snapshot still works; the failed preparation releases its slot.
        let body = PreparedMedia::snapshot(
            dir.path(),
            &b"ok"[..],
            2,
            "text/plain".into(),
            Some((4, 5, 10)),
        )
        .await
        .unwrap()
        .into_response()
        .unwrap();
        assert_eq!(body.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(body.headers()["content-range"], "bytes 4-5/10");
        assert_eq!(hyper::body::to_bytes(body.into_body()).await.unwrap(), "ok");
    }
}
