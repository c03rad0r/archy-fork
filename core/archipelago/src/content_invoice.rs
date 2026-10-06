//! Durable seller-side entitlements for peer-file invoices and on-chain sales.
//! Payment records must outlive browser polling, process restarts and invoice
//! expiry: an invoice can settle while the buyer is disconnected.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncWriteExt, sync::Mutex};

static WRITES: Mutex<()> = Mutex::const_new(());

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaymentMethod {
    Lightning,
    Onchain,
}

impl PaymentMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lightning => "lightning",
            Self::Onchain => "onchain",
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Entitlement {
    content_id: String,
    price_sats: u64,
    paid: bool,
    #[serde(default)]
    method: Option<PaymentMethod>,
}

impl Entitlement {
    fn payment_method(&self, token: &str) -> PaymentMethod {
        self.method.unwrap_or_else(|| {
            if token
                .parse::<bitcoin::Address<bitcoin::address::NetworkUnchecked>>()
                .is_ok()
            {
                PaymentMethod::Onchain
            } else {
                PaymentMethod::Lightning
            }
        })
    }
}

fn path(data_dir: &Path, token: &str) -> PathBuf {
    data_dir.join("content-entitlements").join(format!(
        "{}.json",
        hex::encode(Sha256::digest(token.as_bytes()))
    ))
}

async fn read(data_dir: &Path, token: &str) -> Result<Option<Entitlement>> {
    match fs::read(path(data_dir, token)).await {
        Ok(bytes) => Ok(Some(
            serde_json::from_slice(&bytes).context("Invalid payment entitlement")?,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("Reading payment entitlement"),
    }
}

async fn write(data_dir: &Path, token: &str, entry: &Entitlement) -> Result<()> {
    let target = path(data_dir, token);
    let dir = target.parent().unwrap();
    fs::create_dir_all(dir).await?;
    let tmp = target.with_extension("tmp");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .await?;
    file.write_all(&serde_json::to_vec(entry)?).await?;
    file.sync_all().await?;
    drop(file);
    fs::rename(&tmp, &target).await?;
    fs::File::open(dir).await?.sync_all().await?;
    Ok(())
}

/// Save before exposing an invoice/address to the buyer. Never overwrite an
/// existing payment or silently rebind its token to another item or price.
pub async fn record_pending(
    data_dir: &Path,
    token: &str,
    content_id: &str,
    price_sats: u64,
) -> Result<()> {
    record_pending_method(
        data_dir,
        token,
        content_id,
        price_sats,
        PaymentMethod::Lightning,
    )
    .await
}

pub async fn record_pending_method(
    data_dir: &Path,
    token: &str,
    content_id: &str,
    price_sats: u64,
    method: PaymentMethod,
) -> Result<()> {
    let _lock = WRITES.lock().await;
    if let Some(existing) = read(data_dir, token).await? {
        anyhow::ensure!(
            existing.content_id == content_id
                && existing.price_sats == price_sats
                && existing.payment_method(token) == method,
            "Payment entitlement mismatch"
        );
        return Ok(());
    }
    write(
        data_dir,
        token,
        &Entitlement {
            content_id: content_id.into(),
            price_sats,
            paid: false,
            method: Some(method),
        },
    )
    .await
}

pub async fn mark_paid(data_dir: &Path, token: &str) -> Result<()> {
    let _lock = WRITES.lock().await;
    let mut entry = read(data_dir, token)
        .await?
        .context("Unknown payment entitlement")?;
    entry.paid = true;
    write(data_dir, token, &entry).await
}

pub async fn lookup(data_dir: &Path, token: &str) -> Result<Option<(String, u64)>> {
    Ok(read(data_dir, token)
        .await?
        .map(|e| (e.content_id, e.price_sats)))
}

pub async fn is_paid_for(data_dir: &Path, token: &str, content_id: &str) -> bool {
    read(data_dir, token)
        .await
        .ok()
        .flatten()
        .map(|e| e.paid && e.content_id == content_id)
        .unwrap_or(false)
}

/// Read the method from the seller's durable record, never a buyer header.
pub async fn paid_method_for(
    data_dir: &Path,
    token: &str,
    content_id: &str,
) -> Option<PaymentMethod> {
    let entry = read(data_dir, token).await.ok().flatten()?;
    (entry.paid && entry.content_id == content_id).then(|| entry.payment_method(token))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn paid_entitlement_survives_reload_and_cannot_be_rebound() {
        let dir = tempfile::tempdir().unwrap();
        record_pending(dir.path(), "hash", "file", 12)
            .await
            .unwrap();
        assert!(!is_paid_for(dir.path(), "hash", "file").await);
        mark_paid(dir.path(), "hash").await.unwrap();
        // All reads reopen disk; no process-local entitlement map exists.
        assert!(is_paid_for(dir.path(), "hash", "file").await);
        assert!(!is_paid_for(dir.path(), "hash", "other").await);
        record_pending(dir.path(), "hash", "file", 12)
            .await
            .unwrap();
        assert!(is_paid_for(dir.path(), "hash", "file").await);
        assert!(record_pending(dir.path(), "hash", "other", 12)
            .await
            .is_err());
        assert!(record_pending(dir.path(), "hash", "file", 13)
            .await
            .is_err());
        let other = tempfile::tempdir().unwrap();
        assert!(!is_paid_for(other.path(), "hash", "file").await);
        assert!(mark_paid(dir.path(), "unknown").await.is_err());
    }
    #[tokio::test]
    async fn payment_method_survives_reload_and_legacy_addresses_remain_onchain() {
        let dir = tempfile::tempdir().unwrap();
        record_pending_method(dir.path(), "new", "file", 1, PaymentMethod::Onchain)
            .await
            .unwrap();
        assert!(paid_method_for(dir.path(), "new", "file").await.is_none());
        mark_paid(dir.path(), "new").await.unwrap();
        assert!(paid_method_for(dir.path(), "new", "file").await == Some(PaymentMethod::Onchain));
        assert!(record_pending(dir.path(), "new", "file", 1).await.is_err());
        assert!(paid_method_for(dir.path(), "new", "other").await.is_none());
        let address = "1BoatSLRHtKNngkdXEeobR76b53LETtpyT";
        let legacy = br#"{"content_id":"file","price_sats":1,"paid":true}"#;
        fs::write(path(dir.path(), address), legacy).await.unwrap();
        assert!(paid_method_for(dir.path(), address, "file").await == Some(PaymentMethod::Onchain));
        let hash = "a".repeat(64);
        fs::write(path(dir.path(), &hash), legacy).await.unwrap();
        assert!(paid_method_for(dir.path(), &hash, "file").await == Some(PaymentMethod::Lightning));
    }

    #[tokio::test]
    async fn corrupt_or_unwritable_records_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        record_pending(dir.path(), "../../token", "file", 1)
            .await
            .unwrap();
        fs::write(path(dir.path(), "../../token"), b"broken")
            .await
            .unwrap();
        assert!(lookup(dir.path(), "../../token").await.is_err());
        assert!(!is_paid_for(dir.path(), "../../token", "file").await);
        assert!(record_pending(dir.path(), "../../token", "file", 1)
            .await
            .is_err());
        let file = dir.path().join("not-directory");
        fs::write(&file, b"x").await.unwrap();
        assert!(record_pending(&file, "hash", "file", 1).await.is_err());
    }
}
