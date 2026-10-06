//! Private incoming-proof claims and recoverable settlement. Incoming bearer
//! proofs never become spendable locally: only fresh, saved swap outputs do.
use super::{
    cashu::Proof, ecash::EcashNetwork, mint_client::PreparedSwap, mutation::WalletMutation,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::PathBuf};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};
const MAX_BYTES: u64 = 1024 * 1024;

pub(super) fn canonical_mint(value: &str) -> Result<String> {
    let url = reqwest::Url::parse(value).context("Invalid settlement mint")?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Invalid settlement mint URL"
    );
    Ok(url.to_string().trim_end_matches('/').to_owned())
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    pub id: String,
    pub network: EcashNetwork,
    pub mint_url: String,
    pub token_hash: String,
    pub context_hash: String,
    pub minimum_sats: u64,
}
impl Binding {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            uuid::Uuid::parse_str(&self.id)
                .ok()
                .is_some_and(|id| id.to_string() == self.id),
            "Invalid settlement identifier"
        );
        anyhow::ensure!(
            self.minimum_sats > 0 && is_hash(&self.token_hash) && is_hash(&self.context_hash),
            "Invalid settlement terms"
        );
        anyhow::ensure!(
            canonical_mint(&self.mint_url)? == self.mint_url,
            "Settlement mint is not canonical"
        );
        Ok(())
    }
    fn history_id(&self) -> String {
        format!("received:{}", self.id)
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) enum Phase {
    Prepared,
    Result(Vec<Proof>),
    Committing {
        proofs: Vec<Proof>,
        before: String,
    },
    Committed {
        amount_sats: u64,
        commitment: String,
    },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub binding: Binding,
    pub request: PreparedSwap,
    pub phase: Phase,
}
impl std::fmt::Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReceiveJournalRecord")
            .field("id", &self.binding.id)
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
    fn directory(&self) -> PathBuf {
        self.guard.data_dir.join("wallet/receive-operations")
    }
    fn path(&self, id: &str) -> Result<PathBuf> {
        let uuid = uuid::Uuid::parse_str(id).context("Invalid settlement identifier")?;
        anyhow::ensure!(
            uuid.to_string() == id,
            "Settlement identifier is not canonical"
        );
        Ok(self.directory().join(format!("{uuid}.json")))
    }
    fn validate(record: &Record) -> Result<()> {
        record.binding.validate()?;
        record.request.validate_for_mint(&record.binding.mint_url)?;
        anyhow::ensure!(
            record.request.covers_payment(record.binding.minimum_sats),
            "Settlement does not cover the agreed price"
        );
        match &record.phase {
            Phase::Prepared => (),
            Phase::Result(proofs) => {
                Self::validate_result(record, proofs)?;
            }
            Phase::Committing { proofs, before } => {
                Self::validate_result(record, proofs)?;
                anyhow::ensure!(is_hash(before), "Invalid settlement purse boundary");
            }
            Phase::Committed {
                amount_sats,
                commitment,
            } => {
                anyhow::ensure!(
                    *amount_sats >= record.binding.minimum_sats
                        && record.request.covers_payment(*amount_sats)
                        && (amount_sats
                            .checked_add(1)
                            .is_none_or(|next| !record.request.covers_payment(next)))
                        && is_hash(commitment),
                    "Invalid committed settlement"
                );
            }
        }
        Ok(())
    }
    fn validate_result(record: &Record, proofs: &[Proof]) -> Result<u64> {
        record.request.validate_result_proofs(proofs)?;
        let amount = proofs
            .iter()
            .try_fold(0u64, |sum, proof| sum.checked_add(proof.amount))
            .context("Settlement amount overflow")?;
        anyhow::ensure!(
            amount >= record.binding.minimum_sats,
            "Settlement does not cover the agreed price"
        );
        Ok(amount)
    }
    pub async fn load(&self, id: &str) -> Result<Option<Record>> {
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        let file = match options.open(self.path(id)?).await {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).context("Could not read settlement recovery"),
        };
        anyhow::ensure!(
            file.metadata().await?.is_file(),
            "Settlement record is not a regular file"
        );
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1).read_to_end(&mut bytes).await?;
        anyhow::ensure!(
            bytes.len() as u64 <= MAX_BYTES,
            "Settlement recovery exceeds its size limit"
        );
        let envelope: Envelope = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("Settlement recovery is damaged; do not redeem again"))?;
        anyhow::ensure!(
            envelope.version == 1 && envelope.checksum == digest(envelope.payload.as_bytes()),
            "Settlement recovery checksum/version failed"
        );
        let record: Record = serde_json::from_str(&envelope.payload)
            .map_err(|_| anyhow::anyhow!("Settlement recovery contents are damaged"))?;
        anyhow::ensure!(record.binding.id == id, "Settlement identity mismatch");
        Self::validate(&record)?;
        Ok(Some(record))
    }
    async fn records(&self) -> Result<Vec<Record>> {
        let mut directory = match fs::read_dir(self.directory()).await {
            Ok(directory) => directory,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e).context("Cannot inspect incoming settlement claims"),
        };
        let mut records = Vec::new();
        while let Some(entry) = directory.next_entry().await? {
            let name = entry.file_name();
            let name = name.to_str().context("Invalid settlement filename")?;
            if name
                .strip_prefix('.')
                .and_then(|name| name.strip_suffix(".tmp"))
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
            {
                continue;
            }
            let id = name
                .strip_suffix(".json")
                .context("Unexpected settlement recovery entry")?;
            records.push(
                self.load(id)
                    .await?
                    .context("Settlement recovery disappeared")?,
            );
        }
        Ok(records)
    }
    /// Claims are based on proof secrets, not token serialization/keyset aliases.
    /// A partial overlap must not be treated as a new payment or a refund.
    pub async fn ensure_unclaimed(
        &self,
        mint_url: &str,
        inputs: &[Proof],
        owner: Option<&str>,
    ) -> Result<()> {
        let mint = canonical_mint(mint_url)?;
        let secrets: HashSet<_> = inputs
            .iter()
            .map(|proof| digest(proof.secret.as_bytes()))
            .collect();
        anyhow::ensure!(
            secrets.len() == inputs.len() && !inputs.is_empty(),
            "Duplicate or missing settlement inputs"
        );
        for record in self.records().await? {
            if record.binding.mint_url != mint || owner == Some(record.binding.id.as_str()) {
                continue;
            }
            anyhow::ensure!(!record.request.inputs().iter().any(|proof| secrets.contains(&digest(proof.secret.as_bytes()))), "These incoming proofs already belong to another settlement; resume its original operation");
        }
        Ok(())
    }
    pub async fn ensure_restore_allowed(
        &self,
        network: EcashNetwork,
        mint_url: &str,
    ) -> Result<()> {
        let mint = canonical_mint(mint_url)?;
        for record in self.records().await? {
            if record.binding.network == network && record.binding.mint_url == mint {
                anyhow::ensure!(
                    matches!(record.phase, Phase::Committed { .. }),
                    "Recover pending receipts before restoring this mint from the backup phrase"
                );
            }
        }
        Ok(())
    }
    pub async fn prepare(&self, binding: Binding, request: PreparedSwap) -> Result<Record> {
        binding.validate()?;
        if let Some(previous) = self.load(&binding.id).await? {
            anyhow::ensure!(
                previous.binding == binding,
                "Settlement operation terms changed"
            );
            return Ok(previous);
        }
        self.ensure_unclaimed(&binding.mint_url, request.inputs(), Some(&binding.id))
            .await?;
        let record = Record {
            binding,
            request,
            phase: Phase::Prepared,
        };
        Self::validate(&record)?;
        self.write(&record).await?;
        Ok(record)
    }
    async fn bound(&self, binding: &Binding) -> Result<Record> {
        let record = self
            .load(&binding.id)
            .await?
            .context("Settlement recovery is missing")?;
        anyhow::ensure!(
            &record.binding == binding,
            "Settlement operation terms changed"
        );
        anyhow::ensure!(
            super::ecash::load_network(&self.guard.data_dir).await? == binding.network,
            "Switch back to the settlement's original network"
        );
        Ok(record)
    }
    pub async fn record_result(&self, binding: &Binding, proofs: Vec<Proof>) -> Result<()> {
        let mut record = self.bound(binding).await?;
        Self::validate_result(&record, &proofs)?;
        match &record.phase {
            Phase::Prepared => record.phase = Phase::Result(proofs),
            Phase::Result(saved) | Phase::Committing { proofs: saved, .. } => {
                anyhow::ensure!(
                    serde_json::to_vec(saved)? == serde_json::to_vec(&proofs)?,
                    "Settlement already has a different result"
                );
                return Ok(());
            }
            Phase::Committed { .. } => anyhow::bail!("Settlement already committed"),
        }
        self.write(&record).await
    }
    async fn purse_snapshot(&self, network: EcashNetwork) -> Result<String> {
        // Hash exact on-disk bytes, not a reserialized WalletState. Absence and
        // empty file are deliberately different (empty fails wallet loading).
        match fs::read(self.guard.data_dir.join(network.wallet_file())).await {
            Ok(bytes) => {
                let mut tagged = b"existing:".to_vec();
                tagged.extend(bytes);
                Ok(digest(&tagged))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(digest(b"missing")),
            Err(e) => Err(e).context("Cannot establish settlement purse boundary"),
        }
    }
    pub async fn commit_wallet(&self, binding: &Binding) -> Result<u64> {
        use super::ecash::{load_wallet, save_wallet, TransactionType};
        let mut record = self.bound(binding).await?;
        if let Phase::Committed { amount_sats, .. } = record.phase {
            return Ok(amount_sats);
        }
        let mut wallet = load_wallet(&self.guard.data_dir).await?;
        let (proofs, before) = match record.phase.clone() {
            Phase::Prepared => anyhow::bail!("Settlement result is not durable yet"),
            Phase::Result(proofs) => {
                let before = self.purse_snapshot(binding.network).await?;
                record.phase = Phase::Committing {
                    proofs: proofs.clone(),
                    before: before.clone(),
                };
                self.write(&record).await?;
                (proofs, before)
            }
            Phase::Committing { proofs, before } => (proofs, before),
            Phase::Committed { .. } => unreachable!(),
        };
        let amount = Self::validate_result(&record, &proofs)?;
        let commitment = digest(&serde_json::to_vec(&(binding, amount, &proofs))?);
        let history_id = binding.history_id();
        if let Some(marker) = wallet.receive_commits.get(&history_id) {
            anyhow::ensure!(
                is_hash(marker) && *marker == commitment,
                "Settlement purse marker does not match; manual recovery required"
            );
        } else {
            anyhow::ensure!(
                self.purse_snapshot(binding.network).await? == before,
                "Settlement purse changed without its commit marker; manual recovery required"
            );
            anyhow::ensure!(
                !wallet.transactions.iter().any(|tx| tx.id == history_id),
                "Settlement history exists without its commit marker; manual recovery required"
            );
            anyhow::ensure!(
                !proofs
                    .iter()
                    .any(|proof| wallet.proofs.iter().any(|stored| canonical_mint(
                        &stored.mint_url
                    )
                    .ok()
                    .as_deref()
                        == Some(binding.mint_url.as_str())
                        && stored.proof.secret == proof.secret)),
                "Settlement output exists without its commit marker; manual recovery required"
            );
            wallet.add_proofs(&binding.mint_url, proofs);
            wallet.record_tx(
                TransactionType::Receive,
                amount,
                "Received ecash",
                &binding.mint_url,
                "",
            );
            wallet
                .transactions
                .last_mut()
                .context("Could not record settlement history")?
                .id = history_id.clone();
            wallet
                .receive_commits
                .insert(history_id, commitment.clone());
            save_wallet(&self.guard.data_dir, &wallet).await?;
        }
        record.phase = Phase::Committed {
            amount_sats: amount,
            commitment,
        };
        self.write(&record).await?;
        Ok(amount)
    }
    async fn write(&self, record: &Record) -> Result<()> {
        Self::validate(record)?;
        let payload = serde_json::to_string(record)?;
        let bytes = serde_json::to_vec(&Envelope {
            version: 1,
            checksum: digest(payload.as_bytes()),
            payload,
        })?;
        anyhow::ensure!(
            bytes.len() as u64 <= MAX_BYTES,
            "Settlement recovery exceeds its size limit"
        );
        let parent = self.directory();
        fs::create_dir_all(&parent).await?;
        anyhow::ensure!(
            fs::symlink_metadata(&parent).await?.is_dir(),
            "Settlement directory is not a regular directory"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).await?;
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
        // No asynchronous commit may outlive the wallet mutation guard.
        std::fs::rename(&temporary.0, self.path(&record.binding.id)?)?;
        for directory in [
            parent,
            self.guard.data_dir.join("wallet"),
            self.guard.data_dir.clone(),
        ] {
            std::fs::File::open(directory)?.sync_all()?;
        }
        Ok(())
    }
}
