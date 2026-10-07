//! Durable external-invoice operations. Browser storage is supplemental only.
//! An ambiguous AddInvoice is never replayed: lookup the saved hash instead.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Binding {
    pub id: String,
    pub buyer_did: String,
    pub seller_did: String,
    pub content_id: String,
    pub price_sats: u64,
}
impl Binding {
    pub(crate) fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            uuid::Uuid::parse_str(&self.id)?.to_string() == self.id,
            "Invalid invoice operation"
        );
        crate::identity::pubkey_bytes_from_did_key(&self.buyer_did)?;
        crate::identity::pubkey_bytes_from_did_key(&self.seller_did)?;
        anyhow::ensure!(
            !self.content_id.is_empty()
                && self.content_id.len() <= 128
                && self
                    .content_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "Invalid invoice content"
        );
        anyhow::ensure!(
            self.price_sats > 0 && self.price_sats <= i64::MAX as u64,
            "Invalid invoice price"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    Prepared,
    Dispatched,
    Issued,
    CancelRequested,
    CanceledUnpaid,
    Settled,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetainedFile {
    pub sha256: String,
    pub size: u64,
    pub filename: String,
    pub mime_type: String,
}
impl RetainedFile {
    pub(crate) fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.sha256.len() == 64
                && self.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                && self.size > 0
                && !self.filename.is_empty()
                && self.filename.len() <= 4096
                && !self.filename.chars().any(char::is_control)
                && !self.mime_type.is_empty()
                && self.mime_type.len() <= 256,
            "Invalid retained invoice source metadata"
        );
        hyper::header::HeaderValue::from_str(&self.mime_type)?;
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SellerRecord {
    pub binding: Binding,
    preimage: String,
    pub payment_hash: String,
    pub phase: Phase,
    pub source: Option<RetainedFile>,
    pub bolt11: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BuyerRecord {
    pub binding: Binding,
    pub seller_onion: String,
    pub external_exposure: bool,
    #[serde(default)]
    pub native_retired: bool,
    #[serde(default)]
    pub native_replacement: Option<String>,
    #[serde(default)]
    pub native_dispatched: bool,
    #[serde(default)]
    pub native_result: Option<String>,
    pub last: Option<Status>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Status {
    pub binding: Binding,
    pub payment_hash: String,
    pub bolt11: Option<String>,
    pub state: Phase,
    pub can_switch_method: bool,
    pub source: Option<RetainedFile>,
}
#[derive(Clone)]
pub(crate) struct Invoice {
    pub payment_hash: String,
    pub bolt11: String,
    pub price_sats: u64,
    pub state: String,
    pub paid_sats: Option<u64>,
    pub paid_msats: Option<u64>,
}
pub(crate) trait InvoiceNode {
    async fn prepare_creation(&self) -> Result<()>;
    async fn lookup(&self, hash: &str) -> Result<Option<Invoice>>;
    async fn add(&self, binding: &Binding, preimage_hex: &str) -> Result<()>;
    async fn cancel(&self, hash: &str) -> Result<()>;
}
#[derive(Serialize, Deserialize)]
struct Envelope {
    payload: String,
    checksum: String,
}
pub(crate) struct Journal {
    directory: PathBuf,
    _lock: fs::File,
}
impl Journal {
    pub async fn open(data_dir: &Path) -> Result<Self> {
        let data = data_dir.to_path_buf();
        tokio::task::spawn_blocking(move || {
            use std::os::{
                fd::AsRawFd,
                unix::fs::{OpenOptionsExt, PermissionsExt},
            };
            fs::create_dir_all(&data)?;
            let data = fs::canonicalize(data)?;
            let directory = data.join("content-lightning");
            fs::create_dir_all(&directory)?;
            anyhow::ensure!(
                fs::symlink_metadata(&directory)?.is_dir(),
                "Invoice journal is not a directory"
            );
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
            fs::File::open(&data)?.sync_all()?;
            let lock = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(directory.join(".lock"))?;
            anyhow::ensure!(lock.metadata()?.is_file(), "Invalid invoice lock");
            loop {
                if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } == 0 {
                    break;
                }
                let e = std::io::Error::last_os_error();
                if e.kind() != std::io::ErrorKind::Interrupted {
                    return Err(e.into());
                }
            }
            Ok(Self {
                directory,
                _lock: lock,
            })
        })
        .await?
    }
    fn path(&self, role: &str, id: &str) -> Result<PathBuf> {
        anyhow::ensure!(
            matches!(role, "buyer" | "seller") && uuid::Uuid::parse_str(id)?.to_string() == id,
            "Invalid invoice journal key"
        );
        Ok(self.directory.join(format!("{role}-{id}.json")))
    }
    fn read<T: serde::de::DeserializeOwned>(&self, role: &str, id: &str) -> Result<Option<T>> {
        use std::os::unix::fs::OpenOptionsExt;
        let file = match fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(self.path(role, id)?)
        {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        anyhow::ensure!(file.metadata()?.is_file(), "Invalid invoice record");
        let mut bytes = Vec::new();
        file.take(65537).read_to_end(&mut bytes)?;
        anyhow::ensure!(bytes.len() <= 65536, "Invoice record too large");
        let envelope: Envelope =
            serde_json::from_slice(&bytes).context("Invoice recovery damaged; do not pay again")?;
        anyhow::ensure!(
            hex::encode(Sha256::digest(envelope.payload.as_bytes())) == envelope.checksum,
            "Invoice recovery checksum changed"
        );
        Ok(Some(serde_json::from_str(&envelope.payload)?))
    }
    fn write<T: Serialize>(&self, role: &str, id: &str, value: &T) -> Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        let payload = serde_json::to_string(value)?;
        let bytes = serde_json::to_vec(&Envelope {
            checksum: hex::encode(Sha256::digest(payload.as_bytes())),
            payload,
        })?;
        anyhow::ensure!(bytes.len() <= 65536, "Invoice record too large");
        let temporary = self
            .directory
            .join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
            fs::rename(&temporary, self.path(role, id)?)?;
            fs::File::open(&self.directory)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
    pub fn seller(&self, binding: &Binding) -> Result<Option<SellerRecord>> {
        binding.validate()?;
        let record: Option<SellerRecord> = self.read("seller", &binding.id)?;
        if let Some(v) = &record {
            anyhow::ensure!(&v.binding == binding, "Invoice operation binding changed");
            let secret = hex::decode(&v.preimage)?;
            anyhow::ensure!(
                secret.len() == 32 && hex::encode(Sha256::digest(secret)) == v.payment_hash,
                "Invoice preimage binding damaged"
            );
        }
        Ok(record)
    }
    pub fn prepare_seller(&self, binding: Binding) -> Result<SellerRecord> {
        self.prepare_seller_source(binding, None)
    }
    pub fn prepare_seller_source(
        &self,
        binding: Binding,
        source: Option<RetainedFile>,
    ) -> Result<SellerRecord> {
        if let Some(source) = &source {
            source.validate()?;
        }
        if let Some(saved) = self.seller(&binding)? {
            anyhow::ensure!(saved.source == source, "Original invoice source changed");
            return Ok(saved);
        }
        use rand::RngCore;
        let mut secret = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut secret);
        let record = SellerRecord {
            payment_hash: hex::encode(Sha256::digest(secret)),
            preimage: hex::encode(secret),
            binding,
            phase: Phase::Prepared,
            source,
            bolt11: None,
        };
        self.save_seller(&record)?;
        Ok(record)
    }
    pub fn save_seller(&self, record: &SellerRecord) -> Result<()> {
        self.write("seller", &record.binding.id, record)
    }
    pub fn buyer(&self, id: &str) -> Result<Option<BuyerRecord>> {
        let record: Option<BuyerRecord> = self.read("buyer", id)?;
        if let Some(v) = &record {
            v.binding.validate()?;
            anyhow::ensure!(v.binding.id == id, "Invoice operation changed");
        }
        Ok(record)
    }
    pub fn buyer_for(
        &self,
        buyer: &str,
        seller: &str,
        content: &str,
    ) -> Result<Option<BuyerRecord>> {
        let mut found = None;
        for entry in fs::read_dir(&self.directory)? {
            let name = entry?
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Invalid invoice record name"))?;
            let Some(id) = name
                .strip_prefix("buyer-")
                .and_then(|s| s.strip_suffix(".json"))
            else {
                continue;
            };
            let record: BuyerRecord = self
                .read("buyer", id)?
                .context("Invoice record disappeared")?;
            record.binding.validate()?;
            let unfinished_replacement = if record.native_retired {
                if let Some(id) = &record.native_replacement {
                    self.buyer(id)?.is_none()
                } else {
                    false
                }
            } else {
                false
            };
            if record.binding.buyer_did == buyer
                && record.binding.seller_did == seller
                && record.binding.content_id == content
                && (!record.native_retired || unfinished_replacement)
                && (unfinished_replacement
                    || record.last.as_ref().is_none_or(|s| !s.can_switch_method))
            {
                anyhow::ensure!(
                    found.is_none(),
                    "Multiple unresolved invoice operations; recover them first"
                );
                found = Some(record)
            }
        }
        Ok(found)
    }
    /// Explicit retry only: retire a proven native-only failure and retain its
    /// replacement UUID before creating anything. Recovery reuses that UUID.
    pub fn retry_native(&self, id: &str) -> Result<BuyerRecord> {
        let mut old = self.buyer(id)?.context("Original native invoice missing")?;
        anyhow::ensure!(
            !old.external_exposure
                && old.native_dispatched
                && old.native_result.as_deref() == Some("failed")
                && old.last.as_ref().is_some_and(|s| s.state != Phase::Settled),
            "Only a confirmed native-only failure can be retried"
        );
        anyhow::ensure!(
            !old.native_retired || old.native_replacement.is_some(),
            "Original invoice was retired for another payment method"
        );
        let replacement = old
            .native_replacement
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let mut binding = old.binding.clone();
        binding.id = replacement.clone();
        if let Some(saved) = self.buyer(&replacement)? {
            anyhow::ensure!(
                saved.binding == binding && saved.seller_onion == old.seller_onion,
                "Native replacement binding changed"
            );
            return Ok(saved);
        }
        let current = self
            .buyer_for(
                &old.binding.buyer_did,
                &old.binding.seller_did,
                &old.binding.content_id,
            )?
            .context("Original native operation no longer owns this purchase")?;
        anyhow::ensure!(
            current.binding.id == old.binding.id,
            "Another operation owns this purchase"
        );
        old.native_retired = true;
        old.native_replacement = Some(replacement);
        self.save_buyer(&old)?;
        let new = BuyerRecord {
            binding,
            seller_onion: old.seller_onion,
            external_exposure: false,
            native_retired: false,
            native_replacement: None,
            native_dispatched: false,
            native_result: None,
            last: None,
        };
        self.save_buyer(&new)?;
        Ok(new)
    }
    pub fn save_buyer(&self, record: &BuyerRecord) -> Result<()> {
        record.binding.validate()?;
        anyhow::ensure!(
            matches!(
                record.native_result.as_deref(),
                None | Some("failed" | "succeeded")
            ),
            "Invalid native invoice outcome"
        );
        anyhow::ensure!(
            !record.native_retired
                || (!record.external_exposure && record.native_result.as_deref() == Some("failed")),
            "Retired native invoice cannot be exposed"
        );
        if let Some(id) = &record.native_replacement {
            anyhow::ensure!(
                record.native_retired
                    && uuid::Uuid::parse_str(id)?.to_string() == *id
                    && *id != record.binding.id,
                "Invalid native replacement identity"
            );
        }
        if let Some(old) = self.read::<BuyerRecord>("buyer", &record.binding.id)? {
            anyhow::ensure!(
                old.binding == record.binding
                    && old.seller_onion == record.seller_onion
                    && (!old.external_exposure || record.external_exposure)
                    && (!old.native_retired || record.native_retired)
                    && old
                        .native_replacement
                        .as_ref()
                        .is_none_or(|id| record.native_replacement.as_ref() == Some(id))
                    && (!old.native_dispatched || record.native_dispatched)
                    && (old.native_result.as_deref() != Some("succeeded")
                        || record.native_result.as_deref() == Some("succeeded")),
                "Invoice buyer binding changed"
            );
            if old.last.as_ref().is_some_and(|s| s.state == Phase::Settled) {
                anyhow::ensure!(
                    record
                        .last
                        .as_ref()
                        .is_some_and(|s| s.state == Phase::Settled),
                    "Settled invoice cannot regress"
                );
            }
        }
        if let Some(status) = &record.last {
            if let Some(source) = &status.source {
                source.validate()?;
            }
            anyhow::ensure!(
                status.binding == record.binding
                    && !(record.native_result.as_deref() == Some("succeeded")
                        && status.can_switch_method)
                    && status.can_switch_method == (status.state == Phase::CanceledUnpaid),
                "Invoice status binding changed"
            );
        }
        self.write("buyer", &record.binding.id, record)
    }
}
impl SellerRecord {
    pub fn status(&self) -> Status {
        Status {
            binding: self.binding.clone(),
            payment_hash: self.payment_hash.clone(),
            bolt11: self.bolt11.clone(),
            state: self.phase.clone(),
            can_switch_method: self.phase == Phase::CanceledUnpaid,
            source: self.source.clone(),
        }
    }
    fn observe(&mut self, invoice: Invoice) -> Result<()> {
        anyhow::ensure!(
            invoice.payment_hash == self.payment_hash
                && invoice.price_sats == self.binding.price_sats
                && !invoice.bolt11.is_empty(),
            "LND invoice binding changed"
        );
        if let Some(original) = &self.bolt11 {
            anyhow::ensure!(original == &invoice.bolt11, "Original invoice changed");
        }
        self.bolt11 = Some(invoice.bolt11);
        let settled = invoice.state == "SETTLED"
            && invoice
                .paid_sats
                .is_some_and(|v| v >= self.binding.price_sats)
            && invoice.paid_msats.is_none_or(|v| {
                self.binding
                    .price_sats
                    .checked_mul(1000)
                    .is_some_and(|required| v >= required)
            });
        if settled {
            self.phase = Phase::Settled
        } else if self.phase != Phase::Settled
            && invoice.state == "CANCELED"
            && invoice.paid_sats == Some(0)
            && invoice.paid_msats.is_none_or(|v| v == 0)
        {
            self.phase = Phase::CanceledUnpaid
        } else if self.phase != Phase::Settled
            && self.phase != Phase::CanceledUnpaid
            && self.phase != Phase::CancelRequested
        {
            self.phase = Phase::Issued
        }
        Ok(())
    }
}
/// Journal lock is retained through network calls. Persist dispatch BEFORE await.
/// Cancellation of this future leaves a recoverable record, never permission to
/// issue another invoice. A prepared operation can be canceled before dispatch.
pub(crate) async fn drive<N: InvoiceNode>(
    journal: &Journal,
    binding: &Binding,
    node: &N,
    cancel: bool,
) -> Result<Status> {
    let mut record = journal
        .seller(binding)?
        .context("Unknown invoice operation")?;
    if matches!(record.phase, Phase::Settled | Phase::CanceledUnpaid) {
        return Ok(record.status());
    }
    if cancel && record.phase == Phase::Prepared {
        record.phase = Phase::CanceledUnpaid;
        journal.save_seller(&record)?;
        return Ok(record.status());
    }
    if cancel {
        record.phase = Phase::CancelRequested;
        journal.save_seller(&record)?;
    }
    if record.phase == Phase::Prepared {
        node.prepare_creation().await?;
        record.phase = Phase::Dispatched;
        journal.save_seller(&record)?;
        // Exactly one AddInvoice attempt. A failed response may still have created
        // it; subsequent operations only look up the saved hash.
        let _ = node.add(binding, &record.preimage).await;
    }
    let Some(invoice) = node.lookup(&record.payment_hash).await? else {
        return Ok(record.status());
    };
    record.observe(invoice)?;
    journal.save_seller(&record)?;
    if cancel && record.phase != Phase::Settled && record.phase != Phase::CanceledUnpaid {
        let _ = node.cancel(&record.payment_hash).await;
        if let Some(invoice) = node.lookup(&record.payment_hash).await? {
            record.observe(invoice)?;
            journal.save_seller(&record)?;
        }
    }
    Ok(record.status())
}

/// Runtime adapters prepare a request without dispatching it, then consume it once.
pub(crate) trait PreparedPayment {
    async fn execute(self) -> Result<serde_json::Value>;
}
pub(crate) trait NativeInvoiceNode {
    type Prepared: PreparedPayment;
    async fn prepare(&self, invoice: &str, hash: &str, amount: u64) -> Result<Self::Prepared>;
    async fn lookup_payment(&self, hash: &str) -> Result<serde_json::Value>;
}
/// Caller retains the per-buyer/seller/item admission lock across this operation.
/// The journal lock is released during actual payment, so unrelated invoices can recover.
pub(crate) async fn drive_native<N: NativeInvoiceNode>(
    data_dir: &Path,
    journal: Journal,
    id: &str,
    node: &N,
) -> Result<serde_json::Value> {
    let mut record = journal
        .buyer(id)?
        .context("Original invoice operation missing")?;
    anyhow::ensure!(
        !record.native_retired,
        "Original native invoice was retired before changing methods"
    );
    let status = record
        .last
        .as_ref()
        .context("Original invoice has not been created")?;
    anyhow::ensure!(!status.can_switch_method, "Original invoice was canceled");
    if status.state == Phase::Settled || record.native_result.as_deref() == Some("succeeded") {
        return Ok(serde_json::json!({"status":"succeeded","payment_hash":status.payment_hash}));
    }
    anyhow::ensure!(
        status.source.is_some(),
        "Original invoice snapshot is not confirmed; no payment dispatched"
    );
    if record.native_dispatched {
        if record.native_result.as_deref() == Some("failed") {
            return Ok(serde_json::json!({"status":"failed","payment_hash":status.payment_hash}));
        }
        let payment = node.lookup_payment(&status.payment_hash).await?;
        if matches!(payment["status"].as_str(), Some("succeeded" | "failed")) {
            record.native_result = payment["status"].as_str().map(str::to_owned);
            journal.save_buyer(&record)?;
        }
        return Ok(payment);
    }
    let invoice = status
        .bolt11
        .as_ref()
        .context("Original invoice is unavailable")?;
    // Preparation validates identity/amount/network/expiry and builds the request;
    // deterministic preparation failures leave this same operation undispatched.
    let prepared = node
        .prepare(invoice, &status.payment_hash, record.binding.price_sats)
        .await?;
    record.native_dispatched = true;
    journal.save_buyer(&record)?;
    drop(journal);
    let payment = prepared.execute().await?;
    if matches!(payment["status"].as_str(), Some("succeeded" | "failed")) {
        record.native_result = payment["status"].as_str().map(str::to_owned);
        Journal::open(data_dir).await?.save_buyer(&record)?;
    }
    Ok(payment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };
    struct Node {
        invoice: Mutex<Option<Invoice>>,
        adds: AtomicUsize,
        lost_reply: bool,
        settle_on_cancel: bool,
        reject_preflight: std::sync::atomic::AtomicBool,
    }
    impl Node {
        fn new() -> Self {
            Self {
                invoice: Mutex::new(None),
                adds: AtomicUsize::new(0),
                lost_reply: true,
                settle_on_cancel: false,
                reject_preflight: std::sync::atomic::AtomicBool::new(false),
            }
        }
    }
    impl InvoiceNode for Node {
        async fn prepare_creation(&self) -> Result<()> {
            anyhow::ensure!(
                !self.reject_preflight.load(Ordering::SeqCst),
                "LND unavailable before invoice dispatch"
            );
            Ok(())
        }
        async fn lookup(&self, _: &str) -> Result<Option<Invoice>> {
            Ok(self.invoice.lock().unwrap().clone())
        }
        async fn add(&self, b: &Binding, p: &str) -> Result<()> {
            self.adds.fetch_add(1, Ordering::SeqCst);
            *self.invoice.lock().unwrap() = Some(Invoice {
                payment_hash: hex::encode(Sha256::digest(hex::decode(p)?)),
                bolt11: "ln-original".into(),
                price_sats: b.price_sats,
                state: "OPEN".into(),
                paid_sats: Some(0),
                paid_msats: Some(0),
            });
            if self.lost_reply {
                anyhow::bail!("reply lost after LND stored invoice")
            }
            Ok(())
        }
        async fn cancel(&self, _: &str) -> Result<()> {
            let mut guard = self.invoice.lock().unwrap();
            let v = guard.as_mut().unwrap();
            if self.settle_on_cancel {
                v.state = "SETTLED".into();
                v.paid_sats = Some(v.price_sats);
                v.paid_msats = Some(v.price_sats * 1000)
            } else {
                v.state = "CANCELED".into()
            };
            anyhow::bail!("cancel reply lost")
        }
    }
    fn binding() -> Binding {
        Binding {
            id: uuid::Uuid::new_v4().to_string(),
            buyer_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([7; 32])).unwrap(),
            seller_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([8; 32])).unwrap(),
            content_id: "file".into(),
            price_sats: 8,
        }
    }
    #[tokio::test]
    async fn lost_add_reply_and_process_restart_recover_original_invoice_once() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let node = Node::new();
        let j = Journal::open(root.path()).await.unwrap();
        j.prepare_seller(b.clone()).unwrap();
        let first = drive(&j, &b, &node, false).await.unwrap();
        drop(j);
        let j = Journal::open(root.path()).await.unwrap();
        let second = drive(&j, &b, &node, false).await.unwrap();
        assert_eq!(first, second);
        assert_eq!(node.adds.load(Ordering::SeqCst), 1);
        assert_eq!(second.state, Phase::Issued);
    }
    #[tokio::test]
    async fn ambiguous_dispatch_missing_lookup_cannot_reissue_or_cancel_as_unpaid() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let node = Node::new();
        let j = Journal::open(root.path()).await.unwrap();
        let mut r = j.prepare_seller(b.clone()).unwrap();
        r.phase = Phase::Dispatched;
        j.save_seller(&r).unwrap();
        drop(j);
        let j = Journal::open(root.path()).await.unwrap();
        let unknown = drive(&j, &b, &node, true).await.unwrap();
        assert_eq!(unknown.state, Phase::CancelRequested);
        assert!(!unknown.can_switch_method);
        assert_eq!(node.adds.load(Ordering::SeqCst), 0);
        // Original delayed AddInvoice arrives after the first cancel lookup.
        node.add(&b, &r.preimage).await.unwrap_err();
        let resolved = drive(&j, &b, &node, true).await.unwrap();
        assert_eq!(resolved.state, Phase::CanceledUnpaid);
        assert!(resolved.can_switch_method);
        assert_eq!(node.adds.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn prepared_cancel_has_no_remote_creation_and_cannot_be_reopened() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let node = Node::new();
        let j = Journal::open(root.path()).await.unwrap();
        j.prepare_seller(b.clone()).unwrap();
        assert!(drive(&j, &b, &node, true).await.unwrap().can_switch_method);
        assert!(drive(&j, &b, &node, false).await.unwrap().can_switch_method);
        assert_eq!(node.adds.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn settlement_wins_lost_cancel_response_and_survives_missing_lnd_record() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let mut node = Node::new();
        node.settle_on_cancel = true;
        let j = Journal::open(root.path()).await.unwrap();
        j.prepare_seller(b.clone()).unwrap();
        drive(&j, &b, &node, false).await.unwrap();
        let paid = drive(&j, &b, &node, true).await.unwrap();
        assert_eq!(paid.state, Phase::Settled);
        assert!(!paid.can_switch_method);
        *node.invoice.lock().unwrap() = None;
        assert_eq!(drive(&j, &b, &node, true).await.unwrap(), paid);
    }
    #[tokio::test]
    async fn browser_loss_finds_original_buyer_operation_and_exposure_is_monotonic() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let j = Journal::open(root.path()).await.unwrap();
        let mut record = BuyerRecord {
            binding: b.clone(),
            seller_onion: "original.onion".into(),
            external_exposure: true,
            native_retired: false,
            native_replacement: None,
            native_dispatched: false,
            native_result: None,
            last: None,
        };
        j.save_buyer(&record).unwrap();
        drop(j);
        let j = Journal::open(root.path()).await.unwrap();
        assert_eq!(
            j.buyer_for(&b.buyer_did, &b.seller_did, &b.content_id)
                .unwrap()
                .unwrap()
                .binding,
            b
        );
        record.external_exposure = false;
        assert!(j.save_buyer(&record).is_err());
        let mut changed = b.clone();
        changed.price_sats += 1;
        j.prepare_seller(b).unwrap();
        assert!(j.prepare_seller(changed).is_err());
    }
    #[tokio::test]
    async fn checksum_damage_blocks_replacement() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let j = Journal::open(root.path()).await.unwrap();
        j.prepare_seller(b.clone()).unwrap();
        let p = j.path("seller", &b.id).unwrap();
        fs::write(p, b"{}").unwrap();
        assert!(j.prepare_seller(b).is_err());
    }
    #[tokio::test]
    async fn native_success_cannot_be_replaced_by_contradictory_canceled_status() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let j = Journal::open(root.path()).await.unwrap();
        let mut last = j.prepare_seller(b.clone()).unwrap().status();
        last.state = Phase::Issued;
        let mut record = BuyerRecord {
            binding: b.clone(),
            seller_onion: "original.onion".into(),
            external_exposure: false,
            native_retired: false,
            native_replacement: None,
            native_dispatched: true,
            native_result: Some("succeeded".into()),
            last: Some(last),
        };
        j.save_buyer(&record).unwrap();
        record.last.as_mut().unwrap().state = Phase::CanceledUnpaid;
        record.last.as_mut().unwrap().can_switch_method = true;
        assert!(j.save_buyer(&record).is_err());
        drop(j);
        let j = Journal::open(root.path()).await.unwrap();
        assert_eq!(
            j.buyer_for(&b.buyer_did, &b.seller_did, &b.content_id)
                .unwrap()
                .unwrap()
                .native_result
                .as_deref(),
            Some("succeeded")
        );
    }
    #[tokio::test]
    async fn retired_native_failure_stays_retired_and_cannot_later_expose_invoice() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let j = Journal::open(root.path()).await.unwrap();
        let mut record = BuyerRecord {
            binding: b.clone(),
            seller_onion: "original.onion".into(),
            external_exposure: false,
            native_retired: false,
            native_replacement: None,
            native_dispatched: true,
            native_result: Some("failed".into()),
            last: None,
        };
        j.save_buyer(&record).unwrap();
        record.native_retired = true;
        j.save_buyer(&record).unwrap();
        drop(j);
        let j = Journal::open(root.path()).await.unwrap();
        assert!(j
            .buyer_for(&b.buyer_did, &b.seller_did, &b.content_id)
            .unwrap()
            .is_none());
        assert!(j.buyer(&b.id).unwrap().unwrap().native_retired);
        record.external_exposure = true;
        assert!(j.save_buyer(&record).is_err());
        record.external_exposure = false;
        record.native_retired = false;
        assert!(j.save_buyer(&record).is_err());
    }
    #[tokio::test]
    async fn inconsistent_paid_units_cannot_create_settlement_or_cancellation() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let node = Node::new();
        let j = Journal::open(root.path()).await.unwrap();
        j.prepare_seller(b.clone()).unwrap();
        drive(&j, &b, &node, false).await.unwrap();
        {
            let mut held = node.invoice.lock().unwrap();
            let invoice = held.as_mut().unwrap();
            invoice.state = "SETTLED".into();
            invoice.paid_sats = Some(b.price_sats);
            invoice.paid_msats = Some(0);
        }
        let result = drive(&j, &b, &node, false).await.unwrap();
        assert_ne!(result.state, Phase::Settled);
        assert!(!result.can_switch_method);
        {
            let mut held = node.invoice.lock().unwrap();
            let invoice = held.as_mut().unwrap();
            invoice.state = "CANCELED".into();
            invoice.paid_sats = Some(0);
            invoice.paid_msats = Some(1);
        }
        assert!(!drive(&j, &b, &node, false).await.unwrap().can_switch_method);
    }
    #[tokio::test]
    async fn snapshot_commit_rechecks_changed_terms_and_unshare_before_invoice_exists() {
        use crate::content_server::{
            self as catalog, AccessControl, Availability, ContentCatalog, ContentItem,
        };
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let original = ContentItem {
            id: b.content_id.clone(),
            filename: "file.txt".into(),
            mime_type: "text/plain".into(),
            size_bytes: 4,
            description: String::new(),
            access: AccessControl::Paid {
                price_sats: b.price_sats,
                accepted: vec!["lightning".into()],
            },
            availability: Availability::AllPeers,
            added_at: String::new(),
        };
        let retained = RetainedFile {
            sha256: "ab".repeat(32),
            size: 4,
            filename: original.filename.clone(),
            mime_type: original.mime_type.clone(),
        };
        let j = Journal::open(root.path()).await.unwrap();
        let mut changed = original.clone();
        changed.access = AccessControl::Paid {
            price_sats: b.price_sats + 1,
            accepted: vec![],
        };
        catalog::save_catalog(
            root.path(),
            &ContentCatalog {
                items: vec![changed],
            },
        )
        .await
        .unwrap();
        assert!(catalog::publish_snapshot_invoice(
            root.path(),
            &original,
            &j,
            b.clone(),
            retained.clone()
        )
        .await
        .is_err());
        assert!(j.seller(&b).unwrap().is_none());
        catalog::save_catalog(root.path(), &ContentCatalog { items: vec![] })
            .await
            .unwrap();
        assert!(catalog::publish_snapshot_invoice(
            root.path(),
            &original,
            &j,
            b.clone(),
            retained.clone()
        )
        .await
        .is_err());
        assert!(j.seller(&b).unwrap().is_none());
        catalog::save_catalog(
            root.path(),
            &ContentCatalog {
                items: vec![original.clone()],
            },
        )
        .await
        .unwrap();
        let prepared = catalog::publish_snapshot_invoice(
            root.path(),
            &original,
            &j,
            b.clone(),
            retained.clone(),
        )
        .await
        .unwrap();
        assert_eq!(prepared.phase, Phase::Prepared);
        assert_eq!(prepared.source, Some(retained));
    }
    struct Native {
        prepares: AtomicUsize,
        executions: std::sync::Arc<AtomicUsize>,
        lookups: AtomicUsize,
        reject_preflight: std::sync::atomic::AtomicBool,
        lose_reply: bool,
    }
    struct PreparedNative {
        executions: std::sync::Arc<AtomicUsize>,
        hash: String,
        lose_reply: bool,
    }
    impl PreparedPayment for PreparedNative {
        async fn execute(self) -> Result<serde_json::Value> {
            self.executions.fetch_add(1, Ordering::SeqCst);
            anyhow::ensure!(!self.lose_reply, "Response lost after dispatch");
            Ok(serde_json::json!({"status":"succeeded","payment_hash":self.hash}))
        }
    }
    impl NativeInvoiceNode for Native {
        type Prepared = PreparedNative;
        async fn prepare(&self, _: &str, hash: &str, _: u64) -> Result<Self::Prepared> {
            self.prepares.fetch_add(1, Ordering::SeqCst);
            anyhow::ensure!(
                !self.reject_preflight.load(Ordering::SeqCst),
                "Wrong configured network before dispatch"
            );
            Ok(PreparedNative {
                executions: self.executions.clone(),
                hash: hash.into(),
                lose_reply: self.lose_reply,
            })
        }
        async fn lookup_payment(&self, hash: &str) -> Result<serde_json::Value> {
            self.lookups.fetch_add(1, Ordering::SeqCst);
            Ok(serde_json::json!({"status":"succeeded","payment_hash":hash}))
        }
    }
    impl Native {
        fn new(lose_reply: bool) -> Self {
            Self {
                prepares: AtomicUsize::new(0),
                executions: std::sync::Arc::new(AtomicUsize::new(0)),
                lookups: AtomicUsize::new(0),
                reject_preflight: std::sync::atomic::AtomicBool::new(false),
                lose_reply,
            }
        }
    }
    fn prepared_native_buyer(journal: &Journal, b: &Binding) -> BuyerRecord {
        let mut seller = journal
            .prepare_seller_source(
                b.clone(),
                Some(RetainedFile {
                    sha256: "ab".repeat(32),
                    size: 4,
                    filename: "file.txt".into(),
                    mime_type: "text/plain".into(),
                }),
            )
            .unwrap();
        seller.phase = Phase::Issued;
        seller.bolt11 = Some("ln-fixture".into());
        journal.save_seller(&seller).unwrap();
        let record = BuyerRecord {
            binding: b.clone(),
            seller_onion: "original.onion".into(),
            external_exposure: false,
            native_retired: false,
            native_replacement: None,
            native_dispatched: false,
            native_result: None,
            last: Some(seller.status()),
        };
        journal.save_buyer(&record).unwrap();
        record
    }
    #[tokio::test]
    async fn native_preflight_failure_can_retry_original_operation_before_single_dispatch() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let node = Native::new(false);
        let journal = Journal::open(root.path()).await.unwrap();
        prepared_native_buyer(&journal, &b);
        node.reject_preflight.store(true, Ordering::SeqCst);
        assert!(drive_native(root.path(), journal, &b.id, &node)
            .await
            .is_err());
        let journal = Journal::open(root.path()).await.unwrap();
        assert!(!journal.buyer(&b.id).unwrap().unwrap().native_dispatched);
        node.reject_preflight.store(false, Ordering::SeqCst);
        assert_eq!(
            drive_native(root.path(), journal, &b.id, &node)
                .await
                .unwrap()["status"],
            "succeeded"
        );
        let journal = Journal::open(root.path()).await.unwrap();
        assert_eq!(
            drive_native(root.path(), journal, &b.id, &node)
                .await
                .unwrap()["status"],
            "succeeded"
        );
        assert_eq!(node.prepares.load(Ordering::SeqCst), 2);
        assert_eq!(node.executions.load(Ordering::SeqCst), 1);
        assert_eq!(node.lookups.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn native_lost_reply_restarts_with_original_hash_lookup_and_never_dispatches_twice() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let node = Native::new(true);
        let journal = Journal::open(root.path()).await.unwrap();
        let original = prepared_native_buyer(&journal, &b);
        assert!(drive_native(root.path(), journal, &b.id, &node)
            .await
            .is_err());
        let journal = Journal::open(root.path()).await.unwrap();
        assert!(journal.buyer(&b.id).unwrap().unwrap().native_dispatched);
        let recovered = drive_native(root.path(), journal, &b.id, &node)
            .await
            .unwrap();
        assert_eq!(
            recovered["payment_hash"],
            original.last.unwrap().payment_hash
        );
        assert_eq!(recovered["status"], "succeeded");
        let journal = Journal::open(root.path()).await.unwrap();
        assert_eq!(
            journal
                .buyer(&b.id)
                .unwrap()
                .unwrap()
                .native_result
                .as_deref(),
            Some("succeeded")
        );
        drive_native(root.path(), journal, &b.id, &node)
            .await
            .unwrap();
        assert_eq!(node.prepares.load(Ordering::SeqCst), 1);
        assert_eq!(node.executions.load(Ordering::SeqCst), 1);
        assert_eq!(node.lookups.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn retired_invoice_rejects_delayed_native_callback_without_preflight_or_payment() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let node = Native::new(false);
        let journal = Journal::open(root.path()).await.unwrap();
        let mut original = prepared_native_buyer(&journal, &b);
        original.native_dispatched = true;
        original.native_result = Some("failed".into());
        original.native_retired = true;
        journal.save_buyer(&original).unwrap();
        assert!(drive_native(root.path(), journal, &b.id, &node)
            .await
            .is_err());
        assert_eq!(node.prepares.load(Ordering::SeqCst), 0);
        assert_eq!(node.executions.load(Ordering::SeqCst), 0);
        assert_eq!(node.lookups.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn explicit_retry_links_one_fresh_uuid_and_rejects_old_callbacks() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let node = Native::new(false);
        let j = Journal::open(root.path()).await.unwrap();
        let mut old = prepared_native_buyer(&j, &b);
        old.native_dispatched = true;
        old.native_result = Some("failed".into());
        j.save_buyer(&old).unwrap();
        let new = j.retry_native(&b.id).unwrap();
        assert_ne!(new.binding.id, b.id);
        assert!(!new.native_dispatched);
        assert!(!new.external_exposure);
        assert_eq!(j.retry_native(&b.id).unwrap().binding, new.binding);
        assert_eq!(
            j.buyer_for(&b.buyer_did, &b.seller_did, &b.content_id)
                .unwrap()
                .unwrap()
                .binding,
            new.binding
        );
        assert!(j.save_buyer(&old).is_err());
        assert!(drive_native(root.path(), j, &b.id, &node).await.is_err());
        assert_eq!(node.executions.load(Ordering::SeqCst), 0);
        let j = Journal::open(root.path()).await.unwrap();
        assert_eq!(
            j.buyer(&new.binding.id).unwrap().unwrap().binding,
            new.binding
        );
    }
    #[tokio::test]
    async fn interrupted_retry_retirement_blocks_other_rails_and_recovers_same_uuid() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let replacement = uuid::Uuid::new_v4().to_string();
        let j = Journal::open(root.path()).await.unwrap();
        let mut old = prepared_native_buyer(&j, &b);
        old.native_dispatched = true;
        old.native_result = Some("failed".into());
        old.native_retired = true;
        old.native_replacement = Some(replacement.clone());
        j.save_buyer(&old).unwrap();
        drop(j);
        let j = Journal::open(root.path()).await.unwrap();
        assert_eq!(
            j.buyer_for(&b.buyer_did, &b.seller_did, &b.content_id)
                .unwrap()
                .unwrap()
                .native_replacement,
            Some(replacement.clone())
        );
        old.last.as_mut().unwrap().state = Phase::CanceledUnpaid;
        old.last.as_mut().unwrap().can_switch_method = true;
        j.save_buyer(&old).unwrap();
        assert!(j
            .buyer_for(&b.buyer_did, &b.seller_did, &b.content_id)
            .unwrap()
            .is_some());
        assert_eq!(j.retry_native(&b.id).unwrap().binding.id, replacement);
        assert_eq!(j.retry_native(&b.id).unwrap().binding.id, replacement);
    }
    #[tokio::test]
    async fn explicit_retry_never_replaces_success_pending_or_externally_exposed_invoice() {
        for (outcome, exposed) in [
            (Some("succeeded"), false),
            (None, false),
            (Some("failed"), true),
        ] {
            let root = tempfile::tempdir().unwrap();
            let b = binding();
            let j = Journal::open(root.path()).await.unwrap();
            let mut old = prepared_native_buyer(&j, &b);
            old.native_dispatched = true;
            old.native_result = outcome.map(str::to_owned);
            old.external_exposure = exposed;
            j.save_buyer(&old).unwrap();
            assert!(j.retry_native(&b.id).is_err());
            assert!(!j.buyer(&b.id).unwrap().unwrap().native_retired);
        }
    }
    #[tokio::test]
    async fn unavailable_seller_preflight_preserves_prepared_operation_for_retry() {
        let root = tempfile::tempdir().unwrap();
        let b = binding();
        let node = Node::new();
        let j = Journal::open(root.path()).await.unwrap();
        let original = j.prepare_seller(b.clone()).unwrap();
        node.reject_preflight.store(true, Ordering::SeqCst);
        assert!(drive(&j, &b, &node, false).await.is_err());
        assert_eq!(j.seller(&b).unwrap().unwrap().phase, Phase::Prepared);
        assert_eq!(node.adds.load(Ordering::SeqCst), 0);
        node.reject_preflight.store(false, Ordering::SeqCst);
        assert_eq!(
            drive(&j, &b, &node, false).await.unwrap().payment_hash,
            original.payment_hash
        );
        assert_eq!(node.adds.load(Ordering::SeqCst), 1);
    }
}
