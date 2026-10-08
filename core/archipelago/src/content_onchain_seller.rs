//! Buyer-bound seller address allocation. Unknown allocation never creates a replacement.
use crate::{
    content_lightning::{Binding, RetainedFile},
    content_onchain::{ChainNetwork, Quote},
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Allocation {
    Prepared,
    Dispatched,
    Ready { address: String },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub binding: Binding,
    pub source: RetainedFile,
    pub network: ChainNetwork,
    pub allocation: Allocation,
    pub paid: bool,
}
impl Record {
    fn validate(&self) -> Result<()> {
        self.binding.validate()?;
        self.source.validate()?;
        anyhow::ensure!(
            (546..=2_100_000_000_000_000).contains(&self.binding.price_sats),
            "On-chain price below supported minimum"
        );
        if let Allocation::Ready { address } = &self.allocation {
            self.quote()?
                .context("Missing original address")?
                .script()?;
            anyhow::ensure!(!address.is_empty(), "Missing address");
        }
        anyhow::ensure!(
            !self.paid || matches!(self.allocation, Allocation::Ready { .. }),
            "Paid operation lacks address"
        );
        Ok(())
    }
    pub fn offer(&self) -> Result<crate::content_onchain_plan::Offer> {
        let offer = crate::content_onchain_plan::Offer {
            binding: self.binding.clone(),
            source: self.source.clone(),
            network: self.network,
            recipient_script_type: "p2wpkh".into(),
        };
        offer.validate()?;
        Ok(offer)
    }
    pub fn quote(&self) -> Result<Option<Quote>> {
        Ok(match &self.allocation {
            Allocation::Ready { address } => Some(Quote {
                binding: self.binding.clone(),
                address: address.clone(),
                network: self.network,
                source: self.source.clone(),
            }),
            _ => None,
        })
    }
}
/// Terminal proof for an operation whose receive allocation was never dispatched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UnallocatedAck {
    pub binding: Binding,
    pub state: String,
    pub address: serde_json::Value,
    pub allocation_dispatched: bool,
    pub can_switch_method: bool,
}
impl UnallocatedAck {
    pub fn validate(&self, binding: &Binding) -> Result<()> {
        binding.validate()?;
        anyhow::ensure!(
            &self.binding == binding
                && self.state == "cancelled_unallocated"
                && self.address.is_null()
                && !self.allocation_dispatched
                && self.can_switch_method,
            "Invalid unallocated retirement acknowledgement"
        );
        Ok(())
    }
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
            let directory = data.join("content-onchain-seller");
            fs::create_dir_all(&directory)?;
            anyhow::ensure!(
                fs::symlink_metadata(&directory)?.is_dir(),
                "On-chain seller journal is not a directory"
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
            anyhow::ensure!(lock.metadata()?.is_file(), "Invalid on-chain seller lock");
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
            matches!(role, "seller" | "retired") && uuid::Uuid::parse_str(id)?.to_string() == id,
            "Invalid on-chain seller journal key"
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
        anyhow::ensure!(file.metadata()?.is_file(), "Invalid on-chain seller record");
        let mut bytes = Vec::new();
        file.take(65537).read_to_end(&mut bytes)?;
        anyhow::ensure!(bytes.len() <= 65536, "On-chain seller record too large");
        let envelope: Envelope = serde_json::from_slice(&bytes)
            .context("On-chain seller recovery damaged; do not pay again")?;
        anyhow::ensure!(
            hex::encode(Sha256::digest(envelope.payload.as_bytes())) == envelope.checksum,
            "On-chain seller recovery checksum changed"
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
        anyhow::ensure!(bytes.len() <= 65536, "On-chain seller record too large");
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

    pub fn retirement(&self, binding: &Binding) -> Result<Option<UnallocatedAck>> {
        binding.validate()?;
        let saved: Option<UnallocatedAck> = self.read("retired", &binding.id)?;
        if let Some(ack) = &saved {
            ack.validate(binding)?;
        }
        Ok(saved)
    }
    pub fn retire_unallocated(&self, binding: &Binding) -> Result<UnallocatedAck> {
        if let Some(ack) = self.retirement(binding)? {
            return Ok(ack);
        }
        if let Some(saved) = self.load(binding)? {
            anyhow::ensure!(saved.allocation==Allocation::Prepared && !saved.paid,"Original address allocation was dispatched or paid; recover it without another payment");
        }
        // Even an absent operation gets a durable tombstone. A delayed create
        // must observe retirement rather than allocating after the acknowledgement.
        let ack = UnallocatedAck {
            binding: binding.clone(),
            state: "cancelled_unallocated".into(),
            address: serde_json::Value::Null,
            allocation_dispatched: false,
            can_switch_method: true,
        };
        ack.validate(binding)?;
        self.write("retired", &binding.id, &ack)?;
        Ok(ack)
    }
    pub fn load(&self, binding: &Binding) -> Result<Option<Record>> {
        binding.validate()?;
        let saved: Option<Record> = self.read("seller", &binding.id)?;
        if let Some(record) = &saved {
            record.validate()?;
            anyhow::ensure!(
                &record.binding == binding,
                "Original seller operation binding changed"
            );
        }
        Ok(saved)
    }
    pub fn save(&self, record: &Record) -> Result<()> {
        record.validate()?;
        anyhow::ensure!(
            self.retirement(&record.binding)?.is_none(),
            "Original address operation is retired"
        );
        if let Some(old) = self.load(&record.binding)? {
            anyhow::ensure!(
                old.source == record.source && old.network == record.network,
                "Original file/network changed"
            );
            anyhow::ensure!(
                !old.paid || record.paid,
                "Confirmed purchase cannot become unpaid"
            );
            match old.allocation {
                Allocation::Ready { .. } => anyhow::ensure!(
                    old.allocation == record.allocation,
                    "Original receive address changed"
                ),
                Allocation::Dispatched => anyhow::ensure!(
                    record.allocation != Allocation::Prepared,
                    "Ambiguous address allocation cannot restart"
                ),
                Allocation::Prepared => {}
            }
        }
        self.write("seller", &record.binding.id, record)
    }
    pub fn prepare(
        &self,
        binding: Binding,
        source: RetainedFile,
        network: ChainNetwork,
    ) -> Result<Record> {
        anyhow::ensure!(
            self.retirement(&binding)?.is_none(),
            "Original address operation is retired"
        );
        if let Some(old) = self.load(&binding)? {
            anyhow::ensure!(
                old.source == source && old.network == network,
                "Original sale changed"
            );
            return Ok(old);
        }
        let record = Record {
            binding,
            source,
            network,
            allocation: Allocation::Prepared,
            paid: false,
        };
        self.save(&record)?;
        Ok(record)
    }
}
pub(crate) trait Wallet {
    async fn network(&self) -> Result<ChainNetwork> {
        anyhow::bail!("Seller wallet network unavailable")
    }
    async fn preflight(&self, network: ChainNetwork) -> Result<()>;
    async fn allocate(&self) -> Result<String>;
    async fn received(&self, address: &str, amount: u64) -> Result<bool>;
}
pub(crate) async fn drive<W: Wallet>(
    journal: &Journal,
    binding: &Binding,
    create: bool,
    wallet: &W,
) -> Result<Record> {
    anyhow::ensure!(
        journal.retirement(binding)?.is_none(),
        "Original address operation is retired"
    );
    let mut saved = journal
        .load(binding)?
        .context("Original on-chain sale missing")?;
    if saved.allocation == Allocation::Prepared && create {
        wallet.preflight(saved.network).await?;
        saved.allocation = Allocation::Dispatched;
        journal.save(&saved)?;
        let address = wallet.allocate().await?;
        saved.allocation = Allocation::Ready { address };
        journal.save(&saved)?;
    }
    if !saved.paid {
        if let Allocation::Ready { address } = &saved.allocation {
            if wallet.received(address, saved.binding.price_sats).await? {
                saved.paid = true;
                journal.save(&saved)?;
            }
        }
    }
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    };
    struct Mock {
        calls: AtomicUsize,
        lost: AtomicBool,
        paid: AtomicBool,
        preflight_fails: AtomicBool,
        observed: Mutex<Vec<String>>,
    }
    impl Wallet for Mock {
        async fn preflight(&self, _: ChainNetwork) -> Result<()> {
            anyhow::ensure!(
                !self.preflight_fails.load(Ordering::SeqCst),
                "Wallet unavailable before allocation"
            );
            Ok(())
        }
        async fn allocate(&self) -> Result<String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            anyhow::ensure!(
                !self.lost.swap(false, Ordering::SeqCst),
                "Lost address allocation reply"
            );
            Ok(address())
        }
        async fn received(&self, address: &str, _: u64) -> Result<bool> {
            self.observed.lock().unwrap().push(address.into());
            Ok(self.paid.load(Ordering::SeqCst))
        }
    }
    fn address() -> String {
        let mut script = vec![0, 20];
        script.extend([1; 20]);
        bitcoin::Address::from_script(
            &bitcoin::ScriptBuf::from_bytes(script),
            bitcoin::Network::Regtest,
        )
        .unwrap()
        .to_string()
    }
    fn mock() -> Mock {
        Mock {
            calls: AtomicUsize::new(0),
            lost: AtomicBool::new(false),
            paid: AtomicBool::new(false),
            preflight_fails: AtomicBool::new(false),
            observed: Mutex::new(vec![]),
        }
    }
    async fn fixture() -> (tempfile::TempDir, Journal, Binding) {
        let data = tempfile::tempdir().unwrap();
        let journal = Journal::open(data.path()).await.unwrap();
        let binding = Binding {
            id: uuid::Uuid::new_v4().to_string(),
            buyer_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([7; 32])).unwrap(),
            seller_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([8; 32])).unwrap(),
            content_id: "file".into(),
            price_sats: 546,
        };
        journal
            .prepare(
                binding.clone(),
                RetainedFile {
                    sha256: "a".repeat(64),
                    size: 4,
                    filename: "original.txt".into(),
                    mime_type: "text/plain".into(),
                },
                ChainNetwork::Regtest,
            )
            .unwrap();
        (data, journal, binding)
    }
    #[tokio::test]
    async fn lost_address_response_never_allocates_again_even_after_restart() {
        let (data, journal, binding) = fixture().await;
        let wallet = mock();
        wallet.lost.store(true, Ordering::SeqCst);
        assert!(drive(&journal, &binding, true, &wallet).await.is_err());
        drop(journal);
        let journal = Journal::open(data.path()).await.unwrap();
        let recovered = drive(&journal, &binding, true, &wallet).await.unwrap();
        assert_eq!(recovered.allocation, Allocation::Dispatched);
        assert!(!recovered.paid);
        assert_eq!(wallet.calls.load(Ordering::SeqCst), 1);
        assert!(wallet.observed.lock().unwrap().is_empty());
    }
    #[tokio::test]
    async fn preflight_retry_and_read_only_status_preserve_single_allocation() {
        let (_data, journal, binding) = fixture().await;
        let wallet = mock();
        drive(&journal, &binding, false, &wallet).await.unwrap();
        assert_eq!(wallet.calls.load(Ordering::SeqCst), 0);
        wallet.preflight_fails.store(true, Ordering::SeqCst);
        assert!(drive(&journal, &binding, true, &wallet).await.is_err());
        assert_eq!(
            journal.load(&binding).unwrap().unwrap().allocation,
            Allocation::Prepared
        );
        wallet.preflight_fails.store(false, Ordering::SeqCst);
        drive(&journal, &binding, true, &wallet).await.unwrap();
        drive(&journal, &binding, true, &wallet).await.unwrap();
        assert_eq!(wallet.calls.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn paid_source_survives_reload_and_stale_unpaid_callback_cannot_regress() {
        let (data, journal, binding) = fixture().await;
        let wallet = mock();
        let unpaid = drive(&journal, &binding, true, &wallet).await.unwrap();
        wallet.paid.store(true, Ordering::SeqCst);
        let paid = drive(&journal, &binding, false, &wallet).await.unwrap();
        assert!(paid.paid);
        assert!(journal.save(&unpaid).is_err());
        drop(journal);
        let journal = Journal::open(data.path()).await.unwrap();
        wallet.paid.store(false, Ordering::SeqCst);
        assert_eq!(
            drive(&journal, &binding, false, &wallet).await.unwrap(),
            paid
        );
        assert_eq!(paid.source.filename, "original.txt");
        assert_eq!(wallet.calls.load(Ordering::SeqCst), 1);
        let mut other = binding.clone();
        other.buyer_did = binding.seller_did.clone();
        assert!(journal.load(&other).is_err());
    }
    #[tokio::test]
    async fn cancel_before_allocation_survives_lost_ack_and_blocks_delayed_create() {
        let (data, journal, binding) = fixture().await;
        let wallet = mock();
        wallet.preflight_fails.store(true, Ordering::SeqCst);
        assert!(drive(&journal, &binding, true, &wallet).await.is_err());
        let original = journal.load(&binding).unwrap().unwrap();
        let ack = journal.retire_unallocated(&binding).unwrap();
        ack.validate(&binding).unwrap();
        assert!(ack.address.is_null());
        assert!(!ack.allocation_dispatched);
        drop(journal);
        let journal = Journal::open(data.path()).await.unwrap();
        assert_eq!(journal.retire_unallocated(&binding).unwrap(), ack);
        wallet.preflight_fails.store(false, Ordering::SeqCst);
        assert!(drive(&journal, &binding, true, &wallet).await.is_err());
        assert!(journal
            .prepare(binding.clone(), original.source, original.network)
            .is_err());
        assert_eq!(wallet.calls.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn absent_operation_retirement_is_a_tombstone_not_absence_evidence() {
        let (_data, journal, binding) = fixture().await;
        let mut unknown = binding.clone();
        unknown.id = uuid::Uuid::new_v4().to_string();
        let ack = journal.retire_unallocated(&unknown).unwrap();
        assert!(journal.load(&unknown).unwrap().is_none());
        let original = journal.load(&binding).unwrap().unwrap();
        assert!(journal
            .prepare(unknown.clone(), original.source, original.network)
            .is_err());
        assert_eq!(journal.retirement(&unknown).unwrap(), Some(ack));
    }
    #[tokio::test]
    async fn dispatched_unknown_and_issued_addresses_cannot_be_retired() {
        let (_data, journal, binding) = fixture().await;
        let wallet = mock();
        wallet.lost.store(true, Ordering::SeqCst);
        assert!(drive(&journal, &binding, true, &wallet).await.is_err());
        assert!(journal.retire_unallocated(&binding).is_err());
        assert!(journal.retirement(&binding).unwrap().is_none());
        drop(journal);
        let (_data, journal, binding) = fixture().await;
        let wallet = mock();
        drive(&journal, &binding, true, &wallet).await.unwrap();
        assert!(journal.retire_unallocated(&binding).is_err());
        assert_eq!(wallet.calls.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn retirement_ack_requires_explicit_null_address_and_all_terminal_fields() {
        let binding = Binding {
            id: uuid::Uuid::new_v4().to_string(),
            buyer_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([7; 32])).unwrap(),
            seller_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([8; 32])).unwrap(),
            content_id: "file".into(),
            price_sats: 546,
        };
        let complete = serde_json::json!({"binding":binding,"state":"cancelled_unallocated","address":null,"allocation_dispatched":false,"can_switch_method":true});
        for key in [
            "address",
            "state",
            "allocation_dispatched",
            "can_switch_method",
        ] {
            let mut partial = complete.clone();
            partial.as_object_mut().unwrap().remove(key);
            assert!(
                serde_json::from_value::<UnallocatedAck>(partial).is_err(),
                "missing {key}"
            );
        }
        serde_json::from_value::<UnallocatedAck>(complete)
            .unwrap()
            .validate(&binding)
            .unwrap();
    }
}
