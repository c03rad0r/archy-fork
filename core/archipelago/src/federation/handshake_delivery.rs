//! Durable, node-encrypted approval replies. Relay acknowledgement is not peer
//! acceptance: keep retrying the same invite until reciprocal membership exists.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::{fs, io::AsyncWriteExt};

const FILE: &str = "federation/handshake-delivery.enc";
const DOMAIN: &[u8] = b"archipelago-handshake-delivery-v1";
static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ApprovalReply {
    pub request_id: String,
    pub recipient: String,
    pub expected_did: String,
    pub invite_code: String,
    pub attempts: u32,
    pub next_attempt: i64,
}

async fn load(data_dir: &Path) -> Result<Vec<ApprovalReply>> {
    let bytes = match fs::read(data_dir.join(FILE)).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let key = crate::storage_crypto::derive_key(data_dir, DOMAIN).await?;
    let plaintext = crate::storage_crypto::open(&bytes, &key)?;
    serde_json::from_slice(&plaintext)
        .context("Invalid handshake delivery store; preserved for recovery")
}

async fn save(data_dir: &Path, entries: &[ApprovalReply]) -> Result<()> {
    let key = crate::storage_crypto::derive_key(data_dir, DOMAIN).await?;
    let bytes = crate::storage_crypto::seal(&serde_json::to_vec(entries)?, &key)?;
    let path = data_dir.join(FILE);
    let parent = path.parent().context("Delivery parent missing")?;
    fs::create_dir_all(parent).await?;
    let temporary = parent.join(format!(".delivery-{}.tmp", uuid::Uuid::new_v4()));
    let result = async {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .await?;
        file.write_all(&bytes).await?;
        file.sync_all().await?;
        drop(file);
        fs::rename(&temporary, &path).await?;
        fs::File::open(parent).await?.sync_all().await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(temporary).await;
    }
    result
}

pub(crate) async fn find(data_dir: &Path, request_id: &str) -> Result<Option<ApprovalReply>> {
    let _guard = LOCK.lock().await;
    Ok(load(data_dir)
        .await?
        .into_iter()
        .find(|entry| entry.request_id == request_id))
}

pub(crate) async fn stage(data_dir: &Path, reply: ApprovalReply) -> Result<ApprovalReply> {
    let _guard = LOCK.lock().await;
    let mut entries = load(data_dir).await?;
    if let Some(existing) = entries
        .iter()
        .find(|entry| entry.request_id == reply.request_id)
    {
        anyhow::ensure!(
            existing.recipient == reply.recipient && existing.expected_did == reply.expected_did,
            "Approval recipient changed; refusing delivery"
        );
        return Ok(existing.clone());
    }
    anyhow::ensure!(entries.len() < 1024, "Handshake delivery queue is full");
    entries.push(reply.clone());
    save(data_dir, &entries).await?;
    Ok(reply)
}

/// Claim before sending, including failed sends. Concurrent polls cannot create
/// retry storms; a crash after this write delays but never loses the reply.
pub(crate) async fn claim(
    data_dir: &Path,
    request_id: &str,
    now: i64,
) -> Result<Option<ApprovalReply>> {
    let _guard = LOCK.lock().await;
    let mut entries = load(data_dir).await?;
    let Some(entry) = entries
        .iter_mut()
        .find(|entry| entry.request_id == request_id)
    else {
        return Ok(None);
    };
    if entry.next_attempt > now {
        return Ok(None);
    }
    entry.attempts = entry.attempts.saturating_add(1);
    let delay = 30_i64
        .saturating_mul(1_i64 << entry.attempts.min(7))
        .min(3600);
    entry.next_attempt = now.saturating_add(delay);
    let claimed = entry.clone();
    save(data_dir, &entries).await?;
    Ok(Some(claimed))
}

pub(crate) async fn remove(data_dir: &Path, request_id: &str) -> Result<()> {
    let _guard = LOCK.lock().await;
    let mut entries = load(data_dir).await?;
    let before = entries.len();
    entries.retain(|entry| entry.request_id != request_id);
    if entries.len() != before {
        save(data_dir, &entries).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("identity"))
            .await
            .unwrap();
        fs::write(dir.path().join("identity/node_key"), [7; 32])
            .await
            .unwrap();
        dir
    }
    fn reply() -> ApprovalReply {
        ApprovalReply {
            request_id: "request-1".into(),
            recipient: "recipient".into(),
            expected_did: "did:key:peer".into(),
            invite_code: "secret-invite".into(),
            attempts: 0,
            next_attempt: 0,
        }
    }
    #[tokio::test]
    async fn encrypted_reply_survives_reload_and_retry_claim_is_exclusive() {
        let dir = fixture().await;
        stage(dir.path(), reply()).await.unwrap();
        let raw = fs::read(dir.path().join(FILE)).await.unwrap();
        assert!(!raw.windows(13).any(|bytes| bytes == b"secret-invite"));
        assert_eq!(
            find(dir.path(), "request-1")
                .await
                .unwrap()
                .unwrap()
                .invite_code,
            "secret-invite"
        );
        let (a, b) = tokio::join!(
            claim(dir.path(), "request-1", 100),
            claim(dir.path(), "request-1", 100)
        );
        assert_eq!(
            usize::from(a.unwrap().is_some()) + usize::from(b.unwrap().is_some()),
            1
        );
        assert!(claim(dir.path(), "request-1", 159).await.unwrap().is_none());
        assert_eq!(
            claim(dir.path(), "request-1", 160)
                .await
                .unwrap()
                .unwrap()
                .attempts,
            2
        );
        remove(dir.path(), "request-1").await.unwrap();
        assert!(find(dir.path(), "request-1").await.unwrap().is_none());
    }
    #[tokio::test]
    async fn corrupt_store_is_not_overwritten_and_recipient_cannot_change() {
        let dir = fixture().await;
        stage(dir.path(), reply()).await.unwrap();
        let mut other = reply();
        other.recipient = "different-recipient".into();
        assert!(stage(dir.path(), other).await.is_err());
        let path = dir.path().join(FILE);
        let mut raw = fs::read(&path).await.unwrap();
        raw[20] ^= 1;
        fs::write(&path, &raw).await.unwrap();
        assert!(stage(dir.path(), reply()).await.is_err());
        assert_eq!(fs::read(path).await.unwrap(), raw);
    }
}
