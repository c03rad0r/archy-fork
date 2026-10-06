//! Private write-ahead records for recoverable sends. Kept separate from the
//! legacy purse so an older purse writer cannot discard recovery metadata.
//! This module does not itself spend, reserve proofs, or authorize a purchase.
use super::{
    cashu::Proof, ecash::EcashNetwork, mint_client::PreparedSwap, mutation::WalletMutation,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};

const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    pub id: String,
    pub network: EcashNetwork,
    pub mint_url: String,
    pub amount_sats: u64,
    /// Hash of immutable caller-owned purchase/transfer terms, never a token.
    pub context_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallet::{cashu::CashuToken, mutation};

    fn fixture() -> (Binding, Request, Outcome) {
        let binding = Binding {
            id: uuid::Uuid::new_v4().to_string(),
            network: EcashNetwork::Mainnet,
            mint_url: "https://mint.example".into(),
            amount_sats: 2,
            context_hash: "ab".repeat(32),
        };
        // Storage fixture only; mint signatures are exercised by payment_tests.
        let proofs = vec![Proof {
            amount: 2,
            id: "009a1f293253e41e".into(),
            secret: "private-journal-fixture".into(),
            c: "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798".into(),
        }];
        let outcome = Outcome {
            token: CashuToken::new(&binding.mint_url, proofs.clone())
                .serialize()
                .unwrap(),
            change: vec![],
        };
        (binding, Request::Exact { proofs }, outcome)
    }

    #[tokio::test]
    async fn restart_preserves_original_request_and_private_files() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let (binding, request, _) = fixture();
        {
            let held = mutation::guard(root.path()).await.unwrap();
            let journal = Journal::new(&held);
            assert!(journal.load(&binding.id).await.unwrap().is_none());
            let record = journal
                .prepare(binding.clone(), request.clone())
                .await
                .unwrap();
            assert!(!format!("{record:?}").contains("private-journal-fixture"));
            let path = journal.path(&binding.id).unwrap();
            assert_eq!(
                fs::metadata(&path).await.unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(path.parent().unwrap())
                    .await
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        let held = mutation::guard(root.path()).await.unwrap();
        let journal = Journal::new(&held);
        let original = journal.load(&binding.id).await.unwrap().unwrap();
        // A retry cannot replace material potentially already sent to the mint.
        let retry = journal
            .prepare(binding, Request::Exact { proofs: vec![] })
            .await
            .unwrap();
        assert_eq!(
            serde_json::to_value(original).unwrap(),
            serde_json::to_value(retry).unwrap()
        );
    }

    #[tokio::test]
    async fn changed_terms_never_replace_original_record() {
        let root = tempfile::tempdir().unwrap();
        let held = mutation::guard(root.path()).await.unwrap();
        let journal = Journal::new(&held);
        let (binding, request, _) = fixture();
        journal
            .prepare(binding.clone(), request.clone())
            .await
            .unwrap();
        let path = journal.path(&binding.id).unwrap();
        let before = fs::read(&path).await.unwrap();
        let mut variants = vec![binding.clone(); 4];
        variants[0].amount_sats = 4;
        variants[1].mint_url = "https://different.example".into();
        variants[2].context_hash = "cd".repeat(32);
        variants[3].network = EcashNetwork::Testnet;
        for changed in variants {
            assert!(journal.prepare(changed, request.clone()).await.is_err());
            assert_eq!(fs::read(&path).await.unwrap(), before);
        }
    }

    #[tokio::test]
    async fn phase_transitions_require_durable_result_and_are_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let (binding, request, outcome) = fixture();
        {
            let held = mutation::guard(root.path()).await.unwrap();
            let journal = Journal::new(&held);
            assert!(journal
                .record_result(&binding, outcome.clone())
                .await
                .is_err());
            journal.prepare(binding.clone(), request).await.unwrap();
            assert!(journal.mark_committed(&binding).await.is_err());
            journal
                .record_result(&binding, outcome.clone())
                .await
                .unwrap();
        }
        let held = mutation::guard(root.path()).await.unwrap();
        let journal = Journal::new(&held);
        assert!(matches!(
            journal.load(&binding.id).await.unwrap().unwrap().phase,
            Phase::Result(_)
        ));
        journal
            .record_result(&binding, outcome.clone())
            .await
            .unwrap();
        journal.mark_committed(&binding).await.unwrap();
        journal.mark_committed(&binding).await.unwrap();
        assert!(matches!(
            journal
                .record_result(&binding, outcome)
                .await
                .unwrap()
                .phase,
            Phase::Committed(_)
        ));
    }

    #[tokio::test]
    async fn invalid_outcomes_preserve_prepared_record() {
        let root = tempfile::tempdir().unwrap();
        let held = mutation::guard(root.path()).await.unwrap();
        let journal = Journal::new(&held);
        let (binding, request, outcome) = fixture();
        journal.prepare(binding.clone(), request).await.unwrap();
        let path = journal.path(&binding.id).unwrap();
        let before = fs::read(&path).await.unwrap();
        let token = CashuToken::deserialize(&outcome.token).unwrap();
        let mut variants = vec![outcome.clone(); 5];
        variants[0].token = "broken".into();
        variants[1].token = CashuToken::new("https://other.example", token.token[0].proofs.clone())
            .serialize()
            .unwrap();
        let mut wrong = token.token[0].proofs.clone();
        wrong[0].amount = 4;
        variants[2].token = CashuToken::new(&binding.mint_url, wrong)
            .serialize()
            .unwrap();
        variants[3].change = token.token[0].proofs.clone();
        let mut wrong_unit = token.clone();
        wrong_unit.unit = Some("usd".into());
        variants[4].token = wrong_unit.serialize().unwrap();
        for invalid in variants {
            assert!(journal.record_result(&binding, invalid).await.is_err());
            assert_eq!(fs::read(&path).await.unwrap(), before);
        }
    }

    #[tokio::test]
    async fn damaged_or_unsupported_record_cannot_be_treated_as_new_payment() {
        let root = tempfile::tempdir().unwrap();
        let held = mutation::guard(root.path()).await.unwrap();
        let journal = Journal::new(&held);
        let (binding, request, _) = fixture();
        journal
            .prepare(binding.clone(), request.clone())
            .await
            .unwrap();
        let path = journal.path(&binding.id).unwrap();
        let original = fs::read(&path).await.unwrap();
        let mut checksum: Envelope = serde_json::from_slice(&original).unwrap();
        checksum.checksum = "00".repeat(32);
        let mut version: Envelope = serde_json::from_slice(&original).unwrap();
        version.version = 2;
        for damaged in [
            vec![],
            b"{".to_vec(),
            serde_json::to_vec(&checksum).unwrap(),
            serde_json::to_vec(&version).unwrap(),
            vec![b' '; MAX_BYTES as usize + 1],
        ] {
            fs::write(&path, &damaged).await.unwrap();
            assert!(journal.load(&binding.id).await.is_err());
            assert!(journal
                .prepare(binding.clone(), request.clone())
                .await
                .is_err());
            assert_eq!(fs::read(&path).await.unwrap(), damaged);
        }
        assert!(journal.load("../../wallet").await.is_err());
    }

    #[tokio::test]
    async fn dangling_record_symlink_is_not_an_absent_operation() {
        let root = tempfile::tempdir().unwrap();
        let held = mutation::guard(root.path()).await.unwrap();
        let journal = Journal::new(&held);
        let (binding, request, _) = fixture();
        let path = journal.path(&binding.id).unwrap();
        fs::create_dir_all(path.parent().unwrap()).await.unwrap();
        std::os::unix::fs::symlink(root.path().join("missing"), &path).unwrap();
        assert!(journal.load(&binding.id).await.is_err());
        assert!(journal.prepare(binding, request).await.is_err());
        assert!(fs::symlink_metadata(path)
            .await
            .unwrap()
            .file_type()
            .is_symlink());
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) enum Request {
    Exact { proofs: Vec<Proof> },
    Swap(PreparedSwap),
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Outcome {
    pub token: String,
    pub change: Vec<Proof>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) enum Phase {
    Prepared,
    Result(Outcome),
    Committed(Outcome),
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub binding: Binding,
    pub request: Request,
    pub phase: Phase,
}

impl std::fmt::Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SendJournalRecord")
            .field("id", &self.binding.id)
            .field("amount_sats", &self.binding.amount_sats)
            .finish_non_exhaustive()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u8,
    payload: String,
    checksum: String,
}

pub(super) struct Journal<'a> {
    guard: &'a WalletMutation,
}
impl<'a> Journal<'a> {
    pub fn new(guard: &'a WalletMutation) -> Self {
        Self { guard }
    }

    fn path(&self, id: &str) -> Result<PathBuf> {
        let id = uuid::Uuid::parse_str(id).context("Invalid payment operation identifier")?;
        Ok(self
            .guard
            .data_dir
            .join("wallet/send-operations")
            .join(format!("{id}.json")))
    }

    fn validate_binding(binding: &Binding) -> Result<()> {
        let id =
            uuid::Uuid::parse_str(&binding.id).context("Invalid payment operation identifier")?;
        anyhow::ensure!(
            id.to_string() == binding.id,
            "Payment operation identifier is not canonical"
        );
        anyhow::ensure!(binding.amount_sats > 0, "Payment amount must be positive");
        anyhow::ensure!(
            binding.context_hash.len() == 64
                && binding
                    .context_hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Invalid payment context hash"
        );
        let url = reqwest::Url::parse(&binding.mint_url).context("Invalid payment mint")?;
        anyhow::ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "Invalid payment mint URL"
        );
        Ok(())
    }

    fn validate_request(binding: &Binding, request: &Request) -> Result<()> {
        match request {
            Request::Exact { proofs } => {
                let total = proofs
                    .iter()
                    .try_fold(0u64, |sum, proof| sum.checked_add(proof.amount))
                    .context("Payment input amount overflow")?;
                anyhow::ensure!(
                    total == binding.amount_sats && !proofs.is_empty(),
                    "Exact payment inputs do not match the amount"
                );
                let mut secrets = std::collections::HashSet::new();
                for proof in proofs {
                    anyhow::ensure!(
                        proof.amount.is_power_of_two() && secrets.insert(&proof.secret),
                        "Invalid or duplicate payment input"
                    );
                    proof.c_as_pubkey()?;
                }
            }
            Request::Swap(prepared) => prepared.validate_for_mint(&binding.mint_url)?,
        }
        Ok(())
    }

    fn validate_outcome(record: &Record, outcome: &Outcome) -> Result<()> {
        let token = super::cashu::CashuToken::deserialize(&outcome.token)
            .map_err(|_| anyhow::anyhow!("Payment result token is invalid"))?;
        anyhow::ensure!(
            token.unit.as_deref().unwrap_or("sat") == "sat",
            "Payment result is not denominated in sats"
        );
        anyhow::ensure!(
            token.token.len() == 1
                && token.token[0].mint.trim_end_matches('/')
                    == record.binding.mint_url.trim_end_matches('/'),
            "Payment result belongs to a different mint"
        );
        let proofs = &token.token[0].proofs;
        let amount = proofs
            .iter()
            .try_fold(0u64, |sum, proof| sum.checked_add(proof.amount))
            .context("Payment result amount overflow")?;
        anyhow::ensure!(
            amount == record.binding.amount_sats,
            "Payment result amount changed"
        );
        match &record.request {
            Request::Exact { proofs: expected } => {
                anyhow::ensure!(
                    outcome.change.is_empty()
                        && serde_json::to_value(proofs)? == serde_json::to_value(expected)?,
                    "Exact payment result changed its proofs"
                );
            }
            Request::Swap(prepared) => {
                let mut all = proofs.clone();
                all.extend(outcome.change.iter().cloned());
                prepared.validate_result_proofs(&all)?;
            }
        }
        Ok(())
    }

    pub async fn load(&self, id: &str) -> Result<Option<Record>> {
        let path = self.path(id)?;
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        let file = match options.open(path).await {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("Could not read payment recovery record"),
        };
        anyhow::ensure!(
            file.metadata().await?.is_file(),
            "Payment recovery record is not a regular file"
        );
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1).read_to_end(&mut bytes).await?;
        anyhow::ensure!(
            bytes.len() as u64 <= MAX_BYTES,
            "Payment recovery record exceeds its size limit"
        );
        let envelope: Envelope = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("Payment recovery record is damaged; do not pay again"))?;
        anyhow::ensure!(
            envelope.version == 1,
            "Unsupported payment recovery record version"
        );
        anyhow::ensure!(
            envelope.checksum == hex::encode(Sha256::digest(envelope.payload.as_bytes())),
            "Payment recovery record checksum failed; do not pay again"
        );
        let record: Record = serde_json::from_str(&envelope.payload).map_err(|_| {
            anyhow::anyhow!("Payment recovery record contents are damaged; do not pay again")
        })?;
        Self::validate_binding(&record.binding)?;
        Self::validate_request(&record.binding, &record.request)?;
        match &record.phase {
            Phase::Prepared => (),
            Phase::Result(outcome) | Phase::Committed(outcome) => {
                Self::validate_outcome(&record, outcome)?
            }
        }
        anyhow::ensure!(
            record.binding.id == id,
            "Payment recovery record identity mismatch"
        );
        Ok(Some(record))
    }

    /// Repeating the same binding returns the original immutable request.
    /// A new preparation must never replace a request that may already be sent.
    pub async fn prepare(&self, binding: Binding, request: Request) -> Result<Record> {
        Self::validate_binding(&binding)?;
        if let Some(record) = self.load(&binding.id).await? {
            anyhow::ensure!(
                record.binding == binding,
                "Payment operation terms changed; no new spend allowed"
            );
            return Ok(record);
        }
        Self::validate_request(&binding, &request)?;
        let record = Record {
            binding,
            request,
            phase: Phase::Prepared,
        };
        self.write(&record).await?;
        Ok(record)
    }

    pub async fn record_result(&self, binding: &Binding, outcome: Outcome) -> Result<Record> {
        let mut record = self
            .load(&binding.id)
            .await?
            .context("Payment recovery record is missing")?;
        anyhow::ensure!(
            &record.binding == binding,
            "Payment operation terms changed"
        );
        Self::validate_outcome(&record, &outcome)?;
        match &record.phase {
            Phase::Prepared => record.phase = Phase::Result(outcome),
            Phase::Result(previous) | Phase::Committed(previous) => {
                anyhow::ensure!(
                    serde_json::to_value(previous)? == serde_json::to_value(&outcome)?,
                    "Payment operation already has a different result"
                );
                return Ok(record);
            }
        }
        self.write(&record).await?;
        Ok(record)
    }

    /// Call only after the wallet commit is durable. This cannot skip Result.
    pub async fn mark_committed(&self, binding: &Binding) -> Result<Record> {
        let mut record = self
            .load(&binding.id)
            .await?
            .context("Payment recovery record is missing")?;
        anyhow::ensure!(
            &record.binding == binding,
            "Payment operation terms changed"
        );
        record.phase = match record.phase {
            Phase::Prepared => anyhow::bail!("Payment result is not durable yet"),
            Phase::Result(outcome) | Phase::Committed(outcome) => Phase::Committed(outcome),
        };
        self.write(&record).await?;
        Ok(record)
    }

    async fn write(&self, record: &Record) -> Result<()> {
        let payload = serde_json::to_string(record)?;
        let checksum = hex::encode(Sha256::digest(payload.as_bytes()));
        let bytes = serde_json::to_vec(&Envelope {
            version: 1,
            payload,
            checksum,
        })?;
        anyhow::ensure!(
            bytes.len() as u64 <= MAX_BYTES,
            "Payment recovery record exceeds its size limit"
        );
        let path = self.path(&record.binding.id)?;
        let parent = path
            .parent()
            .context("Payment recovery directory is missing")?;
        fs::create_dir_all(parent).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).await?;
        }
        struct Temporary(PathBuf);
        impl Drop for Temporary {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let temporary = Temporary(parent.join(format!(".{}.tmp", uuid::Uuid::new_v4())));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temporary.0).await?;
        file.write_all(&bytes).await?;
        file.sync_all().await?;
        drop(file);
        fs::rename(&temporary.0, &path).await?;
        // Persist every new directory entry down from the existing node root.
        for directory in [
            parent.to_path_buf(),
            self.guard.data_dir.join("wallet"),
            self.guard.data_dir.clone(),
        ] {
            fs::File::open(directory).await?.sync_all().await?;
        }
        Ok(())
    }
}
