//! Durable purchase intent and seller receipt primitives. No transport or wallet
//! mutations happen here. Callers must authenticate both peers, negotiate this
//! protocol and obtain a stable content offer before creating the contract.
//!
//! A caller must retain the same purchase UUID across retries. A saved token is
//! not settlement evidence: only a successful, correlated recoverable wallet
//! result may advance seller settlement. Received receipts must come from the
//! authenticated seller; this local journal is not a wire-signature verifier.
use anyhow::{Context, Result};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};

use crate::wallet::{cashu::CashuToken, ecash::EcashNetwork};
const VERSION: u8 = 1;
const MAX_RECORD_BYTES: u64 = 2 * 1024 * 1024;
const MAX_TOKEN_BYTES: usize = 512 * 1024;

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn validate_id(id: &str) -> Result<()> {
    anyhow::ensure!(
        uuid::Uuid::parse_str(id)
            .ok()
            .is_some_and(|v| v.to_string() == id),
        "Invalid purchase identifier"
    );
    Ok(())
}
fn canonical_mint(value: &str) -> Result<String> {
    let url = reqwest::Url::parse(value).context("Invalid purchase mint")?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Invalid purchase mint"
    );
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

/// Verified identities are supplied by the authenticated offer/request layer.
/// Parsing a DID here checks its form; it does not authenticate its presenter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Contract {
    pub version: u8,
    pub id: String,
    pub buyer_did: String,
    pub seller_did: String,
    pub content_id: String,
    pub content_sha256: String,
    pub content_size: u64,
    pub terms_sha256: String,
    pub network: EcashNetwork,
    pub mint_url: String,
    pub gross_token_sats: u64,
    pub minimum_net_sats: u64,
    pub offered_at: i64,
    pub expires_at: i64,
}
impl Contract {
    pub fn validate(&self) -> Result<()> {
        validate_id(&self.id)?;
        anyhow::ensure!(self.version == VERSION, "Unsupported purchase protocol");
        crate::identity::pubkey_bytes_from_did_key(&self.buyer_did)?;
        crate::identity::pubkey_bytes_from_did_key(&self.seller_did)?;
        anyhow::ensure!(
            self.buyer_did != self.seller_did,
            "Purchase peers must be distinct"
        );
        anyhow::ensure!(
            !self.content_id.is_empty()
                && self.content_id.len() <= 256
                && self
                    .content_id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)),
            "Invalid purchase content identifier"
        );
        anyhow::ensure!(
            valid_hash(&self.content_sha256)
                && valid_hash(&self.terms_sha256)
                && self.content_size > 0,
            "Invalid purchase content or terms"
        );
        anyhow::ensure!(
            self.minimum_net_sats > 0 && self.gross_token_sats >= self.minimum_net_sats,
            "Invalid gross/net purchase amounts"
        );
        anyhow::ensure!(
            canonical_mint(&self.mint_url)? == self.mint_url,
            "Purchase mint is not canonical"
        );
        anyhow::ensure!(
            self.offered_at > 0 && self.expires_at > self.offered_at,
            "Invalid purchase offer lifetime"
        );
        Ok(())
    }
    /// Stable context passed to both recoverable wallet operations.
    pub fn context_hash(&self) -> Result<String> {
        self.validate()?;
        Ok(hash(&serde_json::to_vec(&(
            "archipelago-content-purchase-v1",
            self,
        ))?))
    }
    fn validate_new_at(&self, now: i64) -> Result<()> {
        self.validate()?;
        anyhow::ensure!(
            now >= self.offered_at && now < self.expires_at,
            "Purchase offer is not current"
        );
        Ok(())
    }
}

/// Accepted seller liability has no automatic expiry or garbage collection.
/// The seller must retain immutable snapshot eligibility until settlement or an
/// explicit future cancellation/refund protocol resolves this commitment.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Acceptance {
    pub contract_hash: String,
    pub accepted_at: i64,
}
impl Acceptance {
    fn validate(&self, contract: &Contract) -> Result<()> {
        anyhow::ensure!(
            self.contract_hash == contract.context_hash()?
                && self.accepted_at >= contract.offered_at
                && self.accepted_at < contract.expires_at,
            "Seller acceptance does not match the offer"
        );
        Ok(())
    }
}

/// Private delivery capability; do not log or expose it to another buyer.
/// The wire layer must authenticate this receipt before a buyer stores it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Receipt {
    pub contract_hash: String,
    pub amount_received: u64,
    pub capability: String,
}
impl Receipt {
    fn validate(&self, contract: &Contract) -> Result<()> {
        anyhow::ensure!(
            self.contract_hash == contract.context_hash()?
                && self.amount_received >= contract.minimum_net_sats
                && self.amount_received <= contract.gross_token_sats
                && valid_hash(&self.capability),
            "Receipt does not match the purchase"
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedToken {
    encoded: String,
    sha256: String,
}
impl PreparedToken {
    fn new(contract: &Contract, encoded: String) -> Result<Self> {
        let value = Self {
            sha256: hash(encoded.as_bytes()),
            encoded,
        };
        value.validate(contract)?;
        Ok(value)
    }
    fn validate(&self, contract: &Contract) -> Result<()> {
        anyhow::ensure!(
            self.encoded.len() <= MAX_TOKEN_BYTES && self.sha256 == hash(self.encoded.as_bytes()),
            "Prepared purchase token is damaged"
        );
        let token =
            CashuToken::deserialize(&self.encoded).context("Invalid prepared purchase token")?;
        anyhow::ensure!(
            token.unit.as_deref().unwrap_or("sat") == "sat" && token.token.len() == 1,
            "Purchase requires one sat-denominated mint"
        );
        let entry = &token.token[0];
        anyhow::ensure!(
            canonical_mint(&entry.mint)? == contract.mint_url,
            "Purchase token mint changed"
        );
        let mut secrets = std::collections::HashSet::new();
        let mut total = 0u64;
        for proof in &entry.proofs {
            anyhow::ensure!(
                proof.amount.is_power_of_two()
                    && !proof.secret.is_empty()
                    && secrets.insert(&proof.secret),
                "Invalid or duplicate purchase proof"
            );
            proof.c_as_pubkey()?;
            total = total
                .checked_add(proof.amount)
                .context("Purchase amount overflow")?;
        }
        anyhow::ensure!(
            total == contract.gross_token_sats,
            "Purchase token amount changed"
        );
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum BuyerPhase {
    Intent,
    AcceptanceSaved,
    TokenPrepared,
    ReceiptSaved,
    Delivered,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BuyerRecord {
    pub contract: Contract,
    pub phase: BuyerPhase,
    acceptance: Option<Acceptance>,
    token: Option<PreparedToken>,
    receipt: Option<Receipt>,
}
impl BuyerRecord {
    pub fn public_status(&self) -> serde_json::Value {
        let (state, settlement_confirmed, delivered) = match self.phase {
            BuyerPhase::Intent => ("intent", false, false),
            BuyerPhase::AcceptanceSaved => ("accepted_payment_unconfirmed", false, false),
            BuyerPhase::TokenPrepared => ("token_prepared_settlement_unconfirmed", false, false),
            BuyerPhase::ReceiptSaved => ("settled_delivery_pending", true, false),
            BuyerPhase::Delivered => ("delivered", true, true),
        };
        serde_json::json!({
            "operation_id": self.contract.id,
            "content_id": self.contract.content_id,
            "seller_did": self.contract.seller_did,
            "state": state,
            "gross_sats": self.contract.gross_token_sats,
            "minimum_net_sats": self.contract.minimum_net_sats,
            "settlement_confirmed": settlement_confirmed,
            "amount_received": self.receipt().map(|receipt| receipt.amount_received),
            "delivered": delivered,
            "recovery_required": !delivered,
            "can_start_new_payment": false,
        })
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_ref().map(|token| token.encoded.as_str())
    }
    pub fn receipt(&self) -> Option<&Receipt> {
        self.receipt.as_ref()
    }
    fn validate(&self) -> Result<()> {
        self.contract.validate()?;
        if let Some(acceptance) = &self.acceptance {
            acceptance.validate(&self.contract)?;
        }
        if let Some(token) = &self.token {
            token.validate(&self.contract)?;
        }
        if let Some(receipt) = &self.receipt {
            receipt.validate(&self.contract)?;
        }
        anyhow::ensure!(
            match self.phase {
                BuyerPhase::Intent =>
                    self.acceptance.is_none() && self.token.is_none() && self.receipt.is_none(),
                BuyerPhase::AcceptanceSaved =>
                    self.acceptance.is_some() && self.token.is_none() && self.receipt.is_none(),
                BuyerPhase::TokenPrepared =>
                    self.acceptance.is_some() && self.token.is_some() && self.receipt.is_none(),
                BuyerPhase::ReceiptSaved | BuyerPhase::Delivered =>
                    self.acceptance.is_some() && self.token.is_some() && self.receipt.is_some(),
            },
            "Invalid buyer purchase transition"
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum SellerPhase {
    Intent,
    Settled { amount_received: u64 },
    ReceiptSaved(Receipt),
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SellerRecord {
    pub contract: Contract,
    pub accepted_at: i64,
    token_hash: Option<String>,
    pub phase: SellerPhase,
}
impl SellerRecord {
    pub fn acceptance(&self) -> Result<Acceptance> {
        Ok(Acceptance {
            contract_hash: self.contract.context_hash()?,
            accepted_at: self.accepted_at,
        })
    }
    fn validate(&self) -> Result<()> {
        self.contract.validate()?;
        self.acceptance()?.validate(&self.contract)?;
        if let Some(token_hash) = &self.token_hash {
            anyhow::ensure!(valid_hash(token_hash), "Invalid seller token hash");
        }
        anyhow::ensure!(
            matches!(self.phase, SellerPhase::Intent) || self.token_hash.is_some(),
            "Seller token was not durably bound"
        );
        match &self.phase {
            SellerPhase::Intent => (),
            SellerPhase::Settled { amount_received } => {
                anyhow::ensure!(
                    *amount_received >= self.contract.minimum_net_sats
                        && *amount_received <= self.contract.gross_token_sats,
                    "Invalid seller settlement amount"
                );
            }
            SellerPhase::ReceiptSaved(receipt) => receipt.validate(&self.contract)?,
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u8,
    payload: String,
    checksum: String,
}

/// Exclusive journal access across tasks and processes. Hold this only while
/// changing local purchase state; release it before transport/wallet calls.
/// A later caller reopens and revalidates the immutable contract before advancing.
pub(crate) struct Journal {
    directory: PathBuf,
    _lock: std::fs::File,
    #[cfg(test)]
    before_commit: Option<(
        std::sync::Arc<tokio::sync::Notify>,
        std::sync::Arc<tokio::sync::Notify>,
    )>,
}
impl Journal {
    pub async fn open(data_dir: &Path) -> Result<Self> {
        fs::create_dir_all(data_dir).await?;
        let data_dir = fs::canonicalize(data_dir).await?;
        let directory = data_dir.join("content-purchases");
        fs::create_dir_all(&directory).await?;
        anyhow::ensure!(
            fs::symlink_metadata(&directory).await?.is_dir(),
            "Purchase journal directory is not regular"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).await?;
        }
        let path = directory.join(".lock");
        let lock = tokio::task::spawn_blocking(move || -> Result<std::fs::File> {
            let mut options = std::fs::OpenOptions::new();
            options.read(true).write(true).create(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            let file = options.open(path)?;
            anyhow::ensure!(
                file.metadata()?.is_file(),
                "Purchase lock is not a regular file"
            );
            #[cfg(unix)]
            {
                use std::os::fd::AsRawFd;
                loop {
                    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0 {
                        break;
                    }
                    let error = std::io::Error::last_os_error();
                    if error.kind() != std::io::ErrorKind::Interrupted {
                        return Err(error.into());
                    }
                }
            }
            #[cfg(not(unix))]
            anyhow::bail!("Purchase journal locking requires Unix");
            Ok(file)
        })
        .await??;
        Ok(Self {
            directory,
            _lock: lock,
            #[cfg(test)]
            before_commit: None,
        })
    }
    fn path(&self, role: &str, id: &str) -> Result<PathBuf> {
        validate_id(id)?;
        anyhow::ensure!(
            matches!(role, "buyer" | "seller"),
            "Invalid purchase journal role"
        );
        Ok(self.directory.join(format!("{role}-{id}.json")))
    }
    async fn read<T: serde::de::DeserializeOwned>(
        &self,
        role: &str,
        id: &str,
    ) -> Result<Option<T>> {
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        let file = match options.open(self.path(role, id)?).await {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).context("Cannot read purchase recovery"),
        };
        anyhow::ensure!(
            file.metadata().await?.is_file(),
            "Purchase record is not a regular file"
        );
        let mut bytes = Vec::new();
        file.take(MAX_RECORD_BYTES + 1)
            .read_to_end(&mut bytes)
            .await?;
        anyhow::ensure!(
            bytes.len() as u64 <= MAX_RECORD_BYTES,
            "Purchase recovery exceeds size limit"
        );
        let envelope: Envelope = serde_json::from_slice(&bytes)
            .context("Purchase recovery is damaged; do not pay again")?;
        anyhow::ensure!(
            envelope.version == VERSION && envelope.checksum == hash(envelope.payload.as_bytes()),
            "Purchase recovery checksum/version failed; do not pay again"
        );
        Ok(Some(
            serde_json::from_str(&envelope.payload)
                .context("Purchase recovery contents are damaged")?,
        ))
    }
    async fn write<T: Serialize>(&self, role: &str, id: &str, value: &T) -> Result<()> {
        let payload = serde_json::to_string(value)?;
        let bytes = serde_json::to_vec(&Envelope {
            version: VERSION,
            checksum: hash(payload.as_bytes()),
            payload,
        })?;
        anyhow::ensure!(
            bytes.len() as u64 <= MAX_RECORD_BYTES,
            "Purchase recovery exceeds size limit"
        );
        struct Temporary(PathBuf);
        impl Drop for Temporary {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let temporary = Temporary(
            self.directory
                .join(format!(".{}.tmp", uuid::Uuid::new_v4())),
        );
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temporary.0).await?;
        file.write_all(&bytes).await?;
        file.sync_all().await?;
        drop(file);
        #[cfg(test)]
        if let Some((reached, resume)) = &self.before_commit {
            reached.notify_one();
            resume.notified().await;
        }
        // No await after the commit begins: a cancelled future must not release
        // the journal lock while an async rename can still overwrite new state.
        std::fs::rename(&temporary.0, self.path(role, id)?)?;
        std::fs::File::open(&self.directory)?.sync_all()?;
        std::fs::File::open(
            self.directory
                .parent()
                .context("Purchase journal has no parent")?,
        )?
        .sync_all()?;
        Ok(())
    }
    pub async fn buyer(&self, id: &str) -> Result<Option<BuyerRecord>> {
        let result: Option<BuyerRecord> = self.read("buyer", id).await?;
        if let Some(record) = &result {
            record.validate()?;
            anyhow::ensure!(record.contract.id == id, "Buyer journal identity changed");
        }
        Ok(result)
    }
    pub async fn seller(&self, id: &str) -> Result<Option<SellerRecord>> {
        let result: Option<SellerRecord> = self.read("seller", id).await?;
        if let Some(record) = &result {
            record.validate()?;
            anyhow::ensure!(record.contract.id == id, "Seller journal identity changed");
        }
        Ok(result)
    }
    /// Discover existing node-owned intent after browser storage loss. The
    /// journal lock makes this lookup and prepare_buyer's duplicate guard one
    /// serialized decision; caller-supplied fresh UUIDs cannot bypass it.
    pub async fn find_buyers(
        &self,
        buyer_did: &str,
        seller_did: &str,
        content_id: &str,
    ) -> Result<Vec<BuyerRecord>> {
        crate::identity::pubkey_bytes_from_did_key(buyer_did)?;
        crate::identity::pubkey_bytes_from_did_key(seller_did)?;
        let mut entries = fs::read_dir(&self.directory).await?;
        let mut result = Vec::new();
        let mut count = 0usize;
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(id) = name
                .strip_prefix("buyer-")
                .and_then(|v| v.strip_suffix(".json"))
            else {
                continue;
            };
            count += 1;
            anyhow::ensure!(
                count <= 10000,
                "Purchase recovery index requires maintenance; do not pay again"
            );
            let record = self
                .buyer(id)
                .await?
                .context("Purchase recovery disappeared")?;
            if record.contract.buyer_did == buyer_did
                && record.contract.seller_did == seller_did
                && record.contract.content_id == content_id
            {
                result.push(record);
            }
        }
        result.sort_by(|a, b| a.contract.id.cmp(&b.contract.id));
        Ok(result)
    }

    pub async fn prepare_buyer(&self, contract: &Contract, now: i64) -> Result<BuyerRecord> {
        contract.validate()?;
        if let Some(record) = self.buyer(&contract.id).await? {
            anyhow::ensure!(&record.contract == contract, "Buyer purchase terms changed");
            return Ok(record);
        }
        let pending = self
            .find_buyers(
                &contract.buyer_did,
                &contract.seller_did,
                &contract.content_id,
            )
            .await?;
        anyhow::ensure!(
            pending
                .iter()
                .all(|record| record.phase == BuyerPhase::Delivered),
            "An existing purchase must be recovered before a new operation is created"
        );
        contract.validate_new_at(now)?;
        let record = BuyerRecord {
            contract: contract.clone(),
            phase: BuyerPhase::Intent,
            acceptance: None,
            token: None,
            receipt: None,
        };
        self.write("buyer", &contract.id, &record).await?;
        Ok(record)
    }
    pub async fn prepare_seller(&self, contract: &Contract, now: i64) -> Result<SellerRecord> {
        contract.validate()?;
        if let Some(record) = self.seller(&contract.id).await? {
            anyhow::ensure!(
                &record.contract == contract,
                "Seller purchase terms changed"
            );
            return Ok(record);
        }
        contract.validate_new_at(now)?;
        let record = SellerRecord {
            contract: contract.clone(),
            accepted_at: now,
            token_hash: None,
            phase: SellerPhase::Intent,
        };
        self.write("seller", &contract.id, &record).await?;
        Ok(record)
    }
    async fn bound_buyer(&self, contract: &Contract) -> Result<BuyerRecord> {
        let record = self
            .buyer(&contract.id)
            .await?
            .context("Buyer intent is not durable")?;
        anyhow::ensure!(&record.contract == contract, "Buyer purchase terms changed");
        Ok(record)
    }
    async fn bound_seller(&self, contract: &Contract) -> Result<SellerRecord> {
        let record = self
            .seller(&contract.id)
            .await?
            .context("Seller intent is not durable")?;
        anyhow::ensure!(
            &record.contract == contract,
            "Seller purchase terms changed"
        );
        Ok(record)
    }
    /// The transport layer must verify response provenance before passing the
    /// verified seller DID. A claimed DID or client mint timestamp is insufficient.
    pub async fn record_acceptance(
        &self,
        contract: &Contract,
        acceptance: &Acceptance,
        verified_seller_did: &str,
    ) -> Result<BuyerRecord> {
        anyhow::ensure!(
            verified_seller_did == contract.seller_did,
            "Acceptance is from another seller"
        );
        let mut record = self.bound_buyer(contract).await?;
        acceptance.validate(contract)?;
        if let Some(previous) = &record.acceptance {
            anyhow::ensure!(previous == acceptance, "Seller acceptance changed");
            return Ok(record);
        }
        anyhow::ensure!(
            record.phase == BuyerPhase::Intent,
            "Buyer acceptance phase changed"
        );
        record.acceptance = Some(acceptance.clone());
        record.phase = BuyerPhase::AcceptanceSaved;
        record.validate()?;
        self.write("buyer", &contract.id, &record).await?;
        Ok(record)
    }
    /// Bind exact incoming bytes before wallet settlement. Receipt/status replay
    /// remains possible without bearer token bytes, but settlement cannot change them.
    pub async fn record_incoming_token(
        &self,
        contract: &Contract,
        encoded: &str,
    ) -> Result<SellerRecord> {
        let mut record = self.bound_seller(contract).await?;
        let token = PreparedToken::new(contract, encoded.into())?;
        if let Some(previous) = &record.token_hash {
            anyhow::ensure!(previous == &token.sha256, "Seller incoming token changed");
            return Ok(record);
        }
        anyhow::ensure!(
            matches!(record.phase, SellerPhase::Intent),
            "Seller settlement phase changed"
        );
        record.token_hash = Some(token.sha256);
        record.validate()?;
        self.write("seller", &contract.id, &record).await?;
        Ok(record)
    }
    /// Call only with the original correlated recoverable-send result.
    pub async fn record_token(&self, contract: &Contract, encoded: &str) -> Result<BuyerRecord> {
        let mut record = self.bound_buyer(contract).await?;
        let token = PreparedToken::new(contract, encoded.into())?;
        if let Some(previous) = &record.token {
            anyhow::ensure!(
                previous.encoded == token.encoded,
                "Buyer already has a different prepared token"
            );
            return Ok(record);
        }
        anyhow::ensure!(
            record.phase == BuyerPhase::AcceptanceSaved,
            "Authenticated seller acceptance is not durable"
        );
        record.token = Some(token);
        record.phase = BuyerPhase::TokenPrepared;
        record.validate()?;
        self.write("buyer", &contract.id, &record).await?;
        Ok(record)
    }
    /// Call only after receive_token_recoverable succeeds for this UUID/context.
    /// A missing response, client's claim or balance delta is not settlement.
    pub async fn record_settlement(
        &self,
        contract: &Contract,
        amount_received: u64,
    ) -> Result<SellerRecord> {
        let mut record = self.bound_seller(contract).await?;
        match &record.phase {
            SellerPhase::Intent => record.phase = SellerPhase::Settled { amount_received },
            SellerPhase::Settled {
                amount_received: saved,
            } => {
                anyhow::ensure!(
                    *saved == amount_received,
                    "Seller settlement result changed"
                );
                return Ok(record);
            }
            SellerPhase::ReceiptSaved(receipt) => {
                anyhow::ensure!(
                    receipt.amount_received == amount_received,
                    "Seller settlement result changed"
                );
                return Ok(record);
            }
        }
        record.validate()?;
        self.write("seller", &contract.id, &record).await?;
        Ok(record)
    }
    /// Generate once and save before returning the capability to the wire layer.
    pub async fn issue_receipt(&self, contract: &Contract) -> Result<Receipt> {
        let mut record = self.bound_seller(contract).await?;
        let amount_received = match &record.phase {
            SellerPhase::Intent => anyhow::bail!("Seller settlement is not durable"),
            SellerPhase::Settled { amount_received } => *amount_received,
            SellerPhase::ReceiptSaved(receipt) => return Ok(receipt.clone()),
        };
        let mut capability = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut capability);
        let receipt = Receipt {
            contract_hash: contract.context_hash()?,
            amount_received,
            capability: hex::encode(capability),
        };
        receipt.validate(contract)?;
        record.phase = SellerPhase::ReceiptSaved(receipt.clone());
        self.write("seller", &contract.id, &record).await?;
        Ok(receipt)
    }
    /// The caller must first authenticate the seller's response and contract.
    pub async fn record_receipt(
        &self,
        contract: &Contract,
        receipt: &Receipt,
    ) -> Result<BuyerRecord> {
        let mut record = self.bound_buyer(contract).await?;
        receipt.validate(contract)?;
        if let Some(previous) = &record.receipt {
            anyhow::ensure!(previous == receipt, "Buyer already has a different receipt");
            return Ok(record);
        }
        anyhow::ensure!(
            record.phase == BuyerPhase::TokenPrepared,
            "Buyer token is not durable"
        );
        record.receipt = Some(receipt.clone());
        record.phase = BuyerPhase::ReceiptSaved;
        record.validate()?;
        self.write("buyer", &contract.id, &record).await?;
        Ok(record)
    }
    /// Call only after complete downloaded bytes and metadata have been flushed.
    pub async fn record_delivery(
        &self,
        contract: &Contract,
        sha256: &str,
        size: u64,
    ) -> Result<BuyerRecord> {
        let mut record = self.bound_buyer(contract).await?;
        anyhow::ensure!(
            matches!(
                record.phase,
                BuyerPhase::ReceiptSaved | BuyerPhase::Delivered
            ),
            "Buyer receipt is not durable"
        );
        anyhow::ensure!(
            sha256 == contract.content_sha256 && size == contract.content_size,
            "Delivered content changed"
        );
        if record.phase != BuyerPhase::Delivered {
            record.phase = BuyerPhase::Delivered;
            self.write("buyer", &contract.id, &record).await?;
        }
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn contract() -> Contract {
        Contract {
            version: VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            buyer_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([1; 32])).unwrap(),
            seller_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([2; 32])).unwrap(),
            content_id: "film-1".into(),
            content_sha256: "ab".repeat(32),
            content_size: 1024,
            terms_sha256: "cd".repeat(32),
            network: EcashNetwork::Mainnet,
            mint_url: "https://mint.invalid".into(),
            gross_token_sats: 8,
            minimum_net_sats: 7,
            offered_at: 1000,
            expires_at: 2000,
        }
    }
    fn token(contract: &Contract, secret: &str) -> String {
        let key = bitcoin::secp256k1::SecretKey::from_slice(&[7; 32]).unwrap();
        let c = bitcoin::secp256k1::PublicKey::from_secret_key(
            &bitcoin::secp256k1::Secp256k1::new(),
            &key,
        )
        .to_string();
        CashuToken::new(
            &contract.mint_url,
            vec![crate::wallet::cashu::Proof {
                id: "0011223344556677".into(),
                amount: contract.gross_token_sats,
                secret: secret.into(),
                c,
            }],
        )
        .serialize()
        .unwrap()
    }
    #[test]
    fn strict_contract_binds_identity_content_terms_network_and_fee_amounts() {
        let original = contract();
        original.validate().unwrap();
        let context = original.context_hash().unwrap();
        let mut changed = original.clone();
        changed.version = 2;
        assert!(changed.validate().is_err());
        let mut value = serde_json::to_value(&original).unwrap();
        value["unreviewed_field"] = serde_json::json!(true);
        assert!(serde_json::from_value::<Contract>(value).is_err());
        let mut changed = original.clone();
        changed.minimum_net_sats = 9;
        assert!(changed.validate().is_err());
        changed = original.clone();
        changed.buyer_did = "claimed".into();
        assert!(changed.validate().is_err());
        changed = original.clone();
        changed.mint_url.push('/');
        assert!(changed.validate().is_err());
        changed = original.clone();
        changed.content_sha256 = "ef".repeat(32);
        assert_ne!(changed.context_hash().unwrap(), context);
        changed = original.clone();
        changed.network = EcashNetwork::Testnet;
        assert_ne!(changed.context_hash().unwrap(), context);
        changed = original.clone();
        changed.minimum_net_sats = 8;
        assert_ne!(changed.context_hash().unwrap(), context);
    }
    #[tokio::test]
    async fn buyer_seller_resume_original_token_receipt_and_delivery_after_reopen() {
        let root = tempfile::tempdir().unwrap();
        let contract = contract();
        let encoded = token(&contract, "original");
        let journal = Journal::open(root.path()).await.unwrap();
        assert!(journal.record_token(&contract, &encoded).await.is_err());
        assert!(journal.record_settlement(&contract, 7).await.is_err());
        journal.prepare_buyer(&contract, 1500).await.unwrap();
        let seller = journal.prepare_seller(&contract, 1500).await.unwrap();
        journal
            .record_acceptance(
                &contract,
                &&seller.acceptance().unwrap(),
                &contract.seller_did,
            )
            .await
            .unwrap();
        journal
            .record_incoming_token(&contract, &token(&contract, "seller-original"))
            .await
            .unwrap();
        assert!(journal.issue_receipt(&contract).await.is_err());
        journal.record_token(&contract, &encoded).await.unwrap();
        journal.record_settlement(&contract, 7).await.unwrap();
        drop(journal);
        let journal = Journal::open(root.path()).await.unwrap();
        // Expiry prevents a new sale, not recovery of the original durable one.
        journal.prepare_buyer(&contract, 3000).await.unwrap();
        journal.prepare_seller(&contract, 3000).await.unwrap();
        assert_eq!(
            journal.buyer(&contract.id).await.unwrap().unwrap().token(),
            Some(encoded.as_str())
        );
        let receipt = journal.issue_receipt(&contract).await.unwrap();
        journal.record_receipt(&contract, &receipt).await.unwrap();
        let before = fs::read(journal.path("buyer", &contract.id).unwrap())
            .await
            .unwrap();
        journal.record_token(&contract, &encoded).await.unwrap();
        journal.record_receipt(&contract, &receipt).await.unwrap();
        assert_eq!(
            fs::read(journal.path("buyer", &contract.id).unwrap())
                .await
                .unwrap(),
            before
        );
        assert!(journal
            .record_delivery(&contract, &"ef".repeat(32), contract.content_size)
            .await
            .is_err());
        journal
            .record_delivery(&contract, &contract.content_sha256, contract.content_size)
            .await
            .unwrap();
        drop(journal);
        let journal = Journal::open(root.path()).await.unwrap();
        assert!(journal.issue_receipt(&contract).await.unwrap() == receipt);
        assert_eq!(
            journal.buyer(&contract.id).await.unwrap().unwrap().phase,
            BuyerPhase::Delivered
        );
        assert!(
            journal
                .buyer(&contract.id)
                .await
                .unwrap()
                .unwrap()
                .receipt()
                .unwrap()
                == &receipt
        );
        assert!(journal.record_settlement(&contract, 8).await.is_err());
    }
    #[tokio::test]
    async fn changed_terms_tokens_and_foreign_receipts_preserve_original_record() {
        let root = tempfile::tempdir().unwrap();
        let contract = contract();
        let journal = Journal::open(root.path()).await.unwrap();
        journal.prepare_buyer(&contract, 1500).await.unwrap();
        let seller = journal.prepare_seller(&contract, 1500).await.unwrap();
        journal
            .record_acceptance(
                &contract,
                &&seller.acceptance().unwrap(),
                &contract.seller_did,
            )
            .await
            .unwrap();
        journal
            .record_incoming_token(&contract, &token(&contract, "seller-original"))
            .await
            .unwrap();
        journal
            .record_token(&contract, &token(&contract, "first"))
            .await
            .unwrap();
        let before = fs::read(journal.path("buyer", &contract.id).unwrap())
            .await
            .unwrap();
        let mut changed = contract.clone();
        changed.terms_sha256 = "ef".repeat(32);
        assert!(journal.prepare_buyer(&changed, 1500).await.is_err());
        assert!(journal.prepare_seller(&changed, 1500).await.is_err());
        assert!(journal
            .record_token(&contract, &token(&contract, "other"))
            .await
            .is_err());
        assert!(journal.record_settlement(&contract, 6).await.is_err());
        journal.record_settlement(&contract, 7).await.unwrap();
        let mut receipt = journal.issue_receipt(&contract).await.unwrap();
        receipt.contract_hash = changed.context_hash().unwrap();
        assert!(journal.record_receipt(&contract, &receipt).await.is_err());
        assert_eq!(
            fs::read(journal.path("buyer", &contract.id).unwrap())
                .await
                .unwrap(),
            before
        );
        let mut expired = contract.clone();
        expired.id = uuid::Uuid::new_v4().to_string();
        assert!(journal.prepare_buyer(&expired, 2000).await.is_err());
        assert!(journal.prepare_seller(&expired, 999).await.is_err());
        assert!(journal.buyer(&expired.id).await.unwrap().is_none());
    }
    #[tokio::test]
    async fn damaged_records_and_nonregular_targets_are_preserved() {
        let root = tempfile::tempdir().unwrap();
        let contract = contract();
        let journal = Journal::open(root.path()).await.unwrap();
        let path = journal.path("buyer", &contract.id).unwrap();
        fs::write(&path, b"damaged").await.unwrap();
        assert!(journal.prepare_buyer(&contract, 1500).await.is_err());
        assert_eq!(fs::read(&path).await.unwrap(), b"damaged");
        fs::remove_file(&path).await.unwrap();
        fs::create_dir(&path).await.unwrap();
        assert!(journal.prepare_buyer(&contract, 1500).await.is_err());
        assert!(path.is_dir());
        #[cfg(unix)]
        {
            fs::remove_dir(&path).await.unwrap();
            let target = root.path().join("untouched");
            fs::write(&target, b"preserve").await.unwrap();
            std::os::unix::fs::symlink(&target, &path).unwrap();
            assert!(journal.prepare_buyer(&contract, 1500).await.is_err());
            assert_eq!(fs::read(&target).await.unwrap(), b"preserve");
        }
    }
    #[tokio::test]
    async fn private_records_reject_checksum_damage_and_skipped_phases() {
        let root = tempfile::tempdir().unwrap();
        let contract = contract();
        let journal = Journal::open(root.path()).await.unwrap();
        journal.prepare_buyer(&contract, 1500).await.unwrap();
        let path = journal.path("buyer", &contract.id).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(&journal.directory)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        let mut envelope: Envelope =
            serde_json::from_slice(&fs::read(&path).await.unwrap()).unwrap();
        envelope.checksum = "00".repeat(32);
        fs::write(&path, serde_json::to_vec(&envelope).unwrap())
            .await
            .unwrap();
        assert!(journal.buyer(&contract.id).await.is_err());
        let mut record: BuyerRecord = serde_json::from_str(&envelope.payload).unwrap();
        record.phase = BuyerPhase::Delivered;
        envelope.payload = serde_json::to_string(&record).unwrap();
        envelope.checksum = hash(envelope.payload.as_bytes());
        fs::write(&path, serde_json::to_vec(&envelope).unwrap())
            .await
            .unwrap();
        assert!(journal.buyer(&contract.id).await.is_err());
    }
    #[tokio::test]
    async fn cancellation_before_commit_cannot_overwrite_the_next_writer() {
        let root = tempfile::tempdir().unwrap();
        let contract = contract();
        let mut journal = Journal::open(root.path()).await.unwrap();
        journal.prepare_buyer(&contract, 1500).await.unwrap();
        let seller = journal.prepare_seller(&contract, 1500).await.unwrap();
        journal
            .record_acceptance(
                &contract,
                &&seller.acceptance().unwrap(),
                &contract.seller_did,
            )
            .await
            .unwrap();
        let reached = std::sync::Arc::new(tokio::sync::Notify::new());
        let resume = std::sync::Arc::new(tokio::sync::Notify::new());
        journal.before_commit = Some((reached.clone(), resume.clone()));
        let original_contract = contract.clone();
        let stale_token = token(&contract, "cancelled");
        let task =
            tokio::spawn(
                async move { journal.record_token(&original_contract, &stale_token).await },
            );
        reached.notified().await;
        task.abort();
        let result = task.await;
        assert!(matches!(result, Err(error) if error.is_cancelled()));
        let journal = Journal::open(root.path()).await.unwrap();
        assert_eq!(
            journal.buyer(&contract.id).await.unwrap().unwrap().phase,
            BuyerPhase::AcceptanceSaved
        );
        let current_token = token(&contract, "current");
        journal
            .record_token(&contract, &current_token)
            .await
            .unwrap();
        // Resuming the old hook cannot enqueue a stale rename: its future and
        // private temporary file were already dropped before the lock released.
        resume.notify_one();
        tokio::task::yield_now().await;
        assert_eq!(
            journal.buyer(&contract.id).await.unwrap().unwrap().token(),
            Some(current_token.as_str())
        );
        let mut directory = fs::read_dir(&journal.directory).await.unwrap();
        while let Some(entry) = directory.next_entry().await.unwrap() {
            assert!(!entry.file_name().to_string_lossy().ends_with(".tmp"));
        }
    }
    #[tokio::test]
    async fn node_lookup_retains_pending_purchase_when_client_generates_a_fresh_id() {
        let root = tempfile::tempdir().unwrap();
        let first = contract();
        let journal = Journal::open(root.path()).await.unwrap();
        journal.prepare_buyer(&first, 1100).await.unwrap();
        let mut duplicate = first.clone();
        duplicate.id = uuid::Uuid::new_v4().to_string();
        assert!(journal.prepare_buyer(&duplicate, 1100).await.is_err());
        assert!(journal.buyer(&duplicate.id).await.unwrap().is_none());
        let records = journal
            .find_buyers(&first.buyer_did, &first.seller_did, &first.content_id)
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].contract, first);
        duplicate.content_id = "other-file".into();
        journal.prepare_buyer(&duplicate, 1100).await.unwrap();
        let records = journal
            .find_buyers(&first.buyer_did, &first.seller_did, &first.content_id)
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        let mut bytes = fs::read(journal.path("buyer", &first.id).unwrap())
            .await
            .unwrap();
        bytes[0] = b'!';
        fs::write(journal.path("buyer", &first.id).unwrap(), bytes)
            .await
            .unwrap();
        assert!(journal
            .find_buyers(&first.buyer_did, &first.seller_did, &first.content_id)
            .await
            .is_err());
    }
    #[tokio::test]
    async fn public_purchase_status_never_exposes_token_or_delivery_capability() {
        let root = tempfile::tempdir().unwrap();
        let contract = contract();
        let journal = Journal::open(root.path()).await.unwrap();
        let buyer = journal.prepare_buyer(&contract, 1100).await.unwrap();
        assert_eq!(buyer.public_status()["state"], "intent");
        let seller = journal.prepare_seller(&contract, 1100).await.unwrap();
        journal
            .record_acceptance(
                &contract,
                &seller.acceptance().unwrap(),
                &contract.seller_did,
            )
            .await
            .unwrap();
        let encoded = token(&contract, "private-status-proof");
        let buyer = journal.record_token(&contract, &encoded).await.unwrap();
        let status = buyer.public_status();
        assert_eq!(status["state"], "token_prepared_settlement_unconfirmed");
        assert_eq!(status["settlement_confirmed"], false);
        assert!(!status.to_string().contains(&encoded));
        journal
            .record_incoming_token(&contract, &encoded)
            .await
            .unwrap();
        journal.record_settlement(&contract, 8).await.unwrap();
        let receipt = journal.issue_receipt(&contract).await.unwrap();
        let buyer = journal.record_receipt(&contract, &receipt).await.unwrap();
        let status = buyer.public_status();
        assert_eq!(status["state"], "settled_delivery_pending");
        assert_eq!(status["amount_received"], 8);
        assert_eq!(status["can_start_new_payment"], false);
        assert!(!status.to_string().contains(&receipt.capability));
    }
}
