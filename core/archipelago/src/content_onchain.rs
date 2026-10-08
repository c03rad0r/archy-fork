//! Durable native on-chain purchase engine. Not yet connected to owner RPC/UI.
//! One operation owns one address and one funded transaction. Funding ambiguity
//! never invokes coin selection again; broadcast retries reuse saved bytes only.
use crate::content_lightning::{Binding, RetainedFile};
use anyhow::{Context, Result};
use base64::Engine;
use bitcoin::{consensus, psbt::Psbt, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

fn valid_onion(value: &str) -> bool {
    value.len() == 62
        && value.ends_with(".onion")
        && value.as_bytes()[..56]
            .iter()
            .copied()
            .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b))
}
const MAX_RECORD: usize = 2 * 1024 * 1024;
const MAX_SATS: u64 = 2_100_000_000_000_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChainNetwork {
    Mainnet,
    Testnet,
    Signet,
    Regtest,
}
impl ChainNetwork {
    pub(crate) fn bitcoin(self) -> bitcoin::Network {
        match self {
            Self::Mainnet => bitcoin::Network::Bitcoin,
            Self::Testnet => bitcoin::Network::Testnet,
            Self::Signet => bitcoin::Network::Signet,
            Self::Regtest => bitcoin::Network::Regtest,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Quote {
    pub binding: Binding,
    pub address: String,
    pub network: ChainNetwork,
    pub source: RetainedFile,
}
impl Quote {
    fn validate(&self) -> Result<()> {
        self.binding.validate()?;
        anyhow::ensure!(
            (546..=MAX_SATS).contains(&self.binding.price_sats),
            "Invalid on-chain price"
        );
        self.address
            .parse::<bitcoin::Address<bitcoin::address::NetworkUnchecked>>()?
            .require_network(self.network.bitcoin())?;
        self.source.validate()?;
        Ok(())
    }
    pub(crate) fn script(&self) -> Result<bitcoin::ScriptBuf> {
        Ok(self
            .address
            .parse::<bitcoin::Address<bitcoin::address::NetworkUnchecked>>()?
            .require_network(self.network.bitcoin())?
            .script_pubkey())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FeePolicy {
    /// Locally owned change script verified by the wallet adapter, never seller supplied.
    pub change_script: String,
    pub max_fee_sats: u64,
    pub max_fee_rate_sat_vbyte: u64,
}
impl FeePolicy {
    fn validate(&self) -> Result<()> {
        let script = bitcoin::ScriptBuf::from_bytes(hex::decode(&self.change_script)?);
        anyhow::ensure!(
            script.is_p2wpkh() || script.is_p2tr(),
            "Unsupported wallet change script"
        );
        anyhow::ensure!(
            self.max_fee_sats > 0
                && self.max_fee_sats <= MAX_SATS
                && self.max_fee_rate_sat_vbyte > 0
                && self.max_fee_rate_sat_vbyte <= MAX_SATS,
            "Invalid original fee limit"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lease {
    pub lock_id: String,
    pub txid: String,
    pub vout: u32,
    pub value_sats: u64,
    pub script: String,
    pub expires_at: u64,
}
impl Lease {
    pub(crate) fn outpoint(&self) -> Result<bitcoin::OutPoint> {
        Ok(bitcoin::OutPoint {
            txid: self.txid.parse()?,
            vout: self.vout,
        })
    }
    pub(crate) fn validate(&self, lock_id: &str) -> Result<()> {
        anyhow::ensure!(
            self.lock_id == lock_id && self.value_sats > 0 && self.value_sats <= MAX_SATS,
            "Foreign or invalid wallet lease"
        );
        self.outpoint()?;
        let script = bitcoin::ScriptBuf::from_bytes(hex::decode(&self.script)?);
        anyhow::ensure!(
            script.is_p2wpkh() || script.is_p2tr(),
            "Unsupported leased input script"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Funded {
    pub psbt_base64: String,
    pub leases: Vec<Lease>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Signed {
    pub raw_hex: String,
    pub txid: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    AddressRequested,
    OfferPrepared,
    PlanPrepared,
    PlanLeaseDispatched,
    InputsLeased,
    AddressAllocationDispatched,
    Quoted,
    TemplatePrepared,
    LeaseDispatched,
    FundingDispatched,
    Funded,
    SigningDispatched,
    Signed,
    BroadcastDispatched,
    Published,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ChangeAddress {
    Dispatched,
    Ready { address: String },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub binding: Binding,
    pub seller_onion: String,
    pub quote: Option<Quote>,
    #[serde(default)]
    pub offer: Option<crate::content_onchain_plan::Offer>,
    #[serde(default)]
    pub plan: Option<crate::content_onchain_plan::FundingPlan>,
    pub externally_exposed: bool,
    pub phase: Phase,
    pub lock_id: String,
    pub policy: Option<FeePolicy>,
    #[serde(default)]
    pub change_address: Option<ChangeAddress>,
    #[serde(default)]
    pub template: Option<Funded>,
    pub funded: Option<Funded>,
    pub signed: Option<Signed>,
    pub observed_leases: Vec<Lease>,
    pub mutations: u64,
    pub settled: bool,
    #[serde(default)]
    pub retirement: Option<crate::content_onchain_seller::UnallocatedAck>,
}
impl Record {
    pub fn new(binding: Binding, seller_onion: String) -> Result<Self> {
        binding.validate()?;
        anyhow::ensure!(
            (546..=MAX_SATS).contains(&binding.price_sats),
            "Invalid on-chain price"
        );
        anyhow::ensure!(valid_onion(&seller_onion), "Invalid seller address");
        use rand::RngCore;
        let mut lock_id = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut lock_id);
        Ok(Self {
            binding,
            seller_onion,
            quote: None,
            offer: None,
            plan: None,
            externally_exposed: false,
            phase: Phase::AddressRequested,
            lock_id: hex::encode(lock_id),
            policy: None,
            change_address: None,
            template: None,
            funded: None,
            signed: None,
            observed_leases: vec![],
            mutations: 0,
            settled: false,
            retirement: None,
        })
    }
    pub(crate) fn can_retire_unallocated(&self) -> bool {
        matches!(
            self.phase,
            Phase::AddressRequested | Phase::OfferPrepared | Phase::PlanPrepared
        ) && self.quote.is_none()
            && !self.externally_exposed
            && !self.settled
            && !matches!(self.change_address, Some(ChangeAddress::Dispatched))
            && self.policy.is_none()
            && self.template.is_none()
            && self.funded.is_none()
            && self.signed.is_none()
            && self.observed_leases.is_empty()
            && self.mutations
                == u64::from(matches!(
                    self.change_address,
                    Some(ChangeAddress::Ready { .. })
                ))
    }
    fn validate(&self) -> Result<()> {
        self.binding.validate()?;
        if let Some(ack) = &self.retirement {
            ack.validate(&self.binding)?;
            anyhow::ensure!(
                matches!(
                    self.phase,
                    Phase::AddressRequested | Phase::OfferPrepared | Phase::PlanPrepared
                ) && self.quote.is_none()
                    && !self.externally_exposed
                    && !self.settled
                    && !matches!(self.change_address, Some(ChangeAddress::Dispatched))
                    && self.policy.is_none()
                    && self.template.is_none()
                    && self.funded.is_none()
                    && self.signed.is_none()
                    && self.observed_leases.is_empty()
                    && self.mutations
                        == u64::from(matches!(
                            self.change_address,
                            Some(ChangeAddress::Ready { .. })
                        )),
                "Mutated or allocated purchase cannot be retired"
            );
        }
        anyhow::ensure!(
            hex::decode(&self.lock_id)?.len() == 32,
            "Invalid original lease identifier"
        );
        anyhow::ensure!(valid_onion(&self.seller_onion), "Invalid seller address");
        if let Some(change) = &self.change_address {
            let network = self
                .network()
                .context("Change allocation requires original offer")?;
            if let ChangeAddress::Ready { address } = change {
                let script = address
                    .parse::<bitcoin::Address<bitcoin::address::NetworkUnchecked>>()?
                    .require_network(network.bitcoin())?
                    .script_pubkey();
                anyhow::ensure!(
                    (script.is_p2wpkh() || script.is_p2tr())
                        && self
                            .quote
                            .as_ref()
                            .map(|q| q.script().map(|s| s != script))
                            .transpose()?
                            .unwrap_or(true),
                    "Invalid original change address"
                );
                if let Some(policy) = &self.policy {
                    anyhow::ensure!(
                        policy.change_script == hex::encode(script.as_bytes()),
                        "Original change address changed"
                    );
                }
            } else {
                anyhow::ensure!(
                    matches!(self.phase, Phase::OfferPrepared | Phase::Quoted)
                        && self.template.is_none()
                        && self.plan.is_none(),
                    "Ambiguous change address cannot fund payment"
                );
            }
        }
        if let Some(offer) = &self.offer {
            offer.validate()?;
            anyhow::ensure!(
                offer.binding == self.binding,
                "Original offer binding changed"
            );
        }
        if let Some(plan) = &self.plan {
            plan.validate(self)?;
        }
        if let Some(quote) = &self.quote {
            quote.validate()?;
            if let Some(offer) = &self.offer {
                offer.check_quote(quote)?;
            }
            anyhow::ensure!(quote.binding == self.binding, "Changed purchase quote");
        }
        if let Some(policy) = &self.policy {
            policy.validate()?;
        }
        anyhow::ensure!(
            self.phase <= Phase::AddressAllocationDispatched || self.quote.is_some(),
            "Original address is missing"
        );
        match self.phase {
            Phase::AddressRequested => anyhow::ensure!(
                self.quote.is_none()
                    && self.policy.is_none()
                    && self.funded.is_none()
                    && self.signed.is_none(),
                "Invalid address preparation state"
            ),
            Phase::OfferPrepared
            | Phase::PlanPrepared
            | Phase::PlanLeaseDispatched
            | Phase::InputsLeased
            | Phase::AddressAllocationDispatched => {
                anyhow::ensure!(
                    self.offer.is_some()
                        && self.quote.is_none()
                        && self.template.is_none()
                        && self.funded.is_none()
                        && self.signed.is_none()
                        && self.policy.is_none(),
                    "Invalid offer/plan state"
                );
                if matches!(
                    self.phase,
                    Phase::PlanPrepared | Phase::PlanLeaseDispatched | Phase::InputsLeased
                ) {
                    anyhow::ensure!(self.plan.is_some(), "Original funding plan missing");
                }
            }
            Phase::Quoted => anyhow::ensure!(
                self.policy.is_none() && self.funded.is_none() && self.signed.is_none(),
                "Unexpected native payment material"
            ),
            Phase::TemplatePrepared | Phase::LeaseDispatched => anyhow::ensure!(
                self.template.is_some() && self.funded.is_none() && self.signed.is_none(),
                "Invalid saved template state"
            ),
            Phase::FundingDispatched => anyhow::ensure!(
                self.funded.is_none() && self.signed.is_none(),
                "Unexpected signing material before funding recovery"
            ),
            Phase::Funded | Phase::SigningDispatched => {
                anyhow::ensure!(self.signed.is_none(), "Signed result has incorrect phase")
            }
            _ => {}
        }
        if self.externally_exposed {
            anyhow::ensure!(
                self.phase == Phase::Quoted
                    && self.policy.is_none()
                    && self.plan.is_none()
                    && self.observed_leases.is_empty()
                    && self.template.is_none()
                    && self.funded.is_none()
                    && self.signed.is_none(),
                "Exposed address cannot also start native payment"
            );
        }
        if matches!(
            self.phase,
            Phase::TemplatePrepared
                | Phase::LeaseDispatched
                | Phase::FundingDispatched
                | Phase::Funded
                | Phase::SigningDispatched
                | Phase::Signed
                | Phase::BroadcastDispatched
                | Phase::Published
        ) {
            anyhow::ensure!(self.policy.is_some(), "Original fee policy is missing");
        }
        let mut observed = std::collections::HashSet::new();
        for lease in &self.observed_leases {
            lease.validate(&self.lock_id)?;
            anyhow::ensure!(
                observed.insert(lease.outpoint()?),
                "Duplicate lease evidence"
            );
        }
        if let Some(template) = &self.template {
            validate_funded(self, template)?;
            if let (Some(plan), Some(quote)) = (&self.plan, &self.quote) {
                let (policy, expected) = plan.bind(self, quote)?;
                anyhow::ensure!(
                    template == &expected && self.policy.as_ref() == Some(&policy),
                    "Actual transaction differs from reviewed funding plan"
                );
            }
        }
        if let Some(funded) = &self.funded {
            validate_funded(self, funded)?;
            if let Some(template) = &self.template {
                anyhow::ensure!(
                    template.psbt_base64 == funded.psbt_base64,
                    "Original unsigned template changed"
                );
                validate_lease_identity(&template.leases, &funded.leases)?;
            }
        }
        if matches!(
            self.phase,
            Phase::Funded
                | Phase::SigningDispatched
                | Phase::Signed
                | Phase::BroadcastDispatched
                | Phase::Published
        ) {
            anyhow::ensure!(
                self.funded.is_some(),
                "Original funded transaction is missing"
            );
        }
        if let Some(signed) = &self.signed {
            validate_signed(self, signed)?;
        }
        if matches!(
            self.phase,
            Phase::Signed | Phase::BroadcastDispatched | Phase::Published
        ) {
            anyhow::ensure!(
                self.signed.is_some(),
                "Original signed transaction is missing"
            );
        }
        Ok(())
    }
    /// Even an unconfirmed external address remains payable. There is no
    /// timeout/empty-wallet transition that grants another rail admission.
    pub(crate) fn network(&self) -> Option<ChainNetwork> {
        self.quote
            .as_ref()
            .map(|q| q.network)
            .or_else(|| self.offer.as_ref().map(|o| o.network))
    }
    pub fn blocks_other_rails(&self) -> bool {
        self.retirement.is_none()
    }
}
#[derive(Serialize, Deserialize)]
struct Envelope {
    payload: String,
    checksum: String,
}
pub(crate) struct Journal {
    directory: PathBuf,
    id: String,
    _lock: fs::File,
}
impl Journal {
    /// Caller holds content_payment_admission before this operation lock.
    pub async fn open(data: &Path, id: &str) -> Result<Self> {
        anyhow::ensure!(
            uuid::Uuid::parse_str(id)?.to_string() == id,
            "Invalid purchase operation"
        );
        let data = data.to_path_buf();
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || {
            use std::os::{
                fd::AsRawFd,
                unix::fs::{OpenOptionsExt, PermissionsExt},
            };
            fs::create_dir_all(&data)?;
            let data = fs::canonicalize(data)?;
            let directory = data.join("content-onchain");
            fs::create_dir_all(&directory)?;
            anyhow::ensure!(
                fs::symlink_metadata(&directory)?.is_dir(),
                "Invalid on-chain journal"
            );
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
            fs::File::open(&data)?.sync_all()?;
            let lock = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(directory.join(format!("{id}.lock")))?;
            anyhow::ensure!(
                lock.metadata()?.is_file(),
                "Invalid on-chain operation lock"
            );
            loop {
                if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } == 0 {
                    break;
                }
                let error = std::io::Error::last_os_error();
                if error.kind() != std::io::ErrorKind::Interrupted {
                    return Err(error.into());
                }
            }
            Ok(Self {
                directory,
                id,
                _lock: lock,
            })
        })
        .await?
    }
    pub fn load(&self) -> Result<Option<Record>> {
        read_record(&self.directory, &self.id)
    }
    /// Caller holds the per-buyer/seller/item admission lock. Atomic record reads
    /// avoid holding another item's operation lock while finding this item.
    pub fn find_for(
        data: &Path,
        buyer: &str,
        seller: &str,
        content: &str,
    ) -> Result<Option<Record>> {
        let directory = data.join("content-onchain");
        let entries = match fs::read_dir(&directory) {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        anyhow::ensure!(
            fs::symlink_metadata(&directory)?.is_dir(),
            "Invalid on-chain recovery directory"
        );
        let mut found = None;
        for entry in entries {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Invalid recovery filename"))?;
            let Some(id) = name.strip_suffix(".json") else {
                continue;
            };
            anyhow::ensure!(
                uuid::Uuid::parse_str(id)?.to_string() == id,
                "Invalid recovery filename"
            );
            let record =
                read_record(&directory, id)?.context("Original recovery record disappeared")?;
            if record.binding.buyer_did == buyer
                && record.binding.seller_did == seller
                && record.binding.content_id == content
                && record.blocks_other_rails()
            {
                anyhow::ensure!(
                    found.is_none(),
                    "Multiple original on-chain attempts require recovery; do not pay again"
                );
                found = Some(record);
            }
        }
        Ok(found)
    }
    pub fn save(&self, record: &Record) -> Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        record.validate()?;
        anyhow::ensure!(record.binding.id == self.id, "Wrong journal operation");
        if let Some(old) = self.load()? {
            anyhow::ensure!(
                old.retirement
                    .as_ref()
                    .is_none_or(|ack| record.retirement.as_ref() == Some(ack)),
                "Retired operation cannot be revived"
            );
            anyhow::ensure!(
                old.binding == record.binding
                    && old.seller_onion == record.seller_onion
                    && old.lock_id == record.lock_id,
                "Original payment binding changed"
            );
            anyhow::ensure!(
                !old.externally_exposed || record.externally_exposed,
                "Address exposure cannot be undone"
            );
            anyhow::ensure!(
                !old.settled || record.settled,
                "Confirmed purchase cannot become unpaid"
            );
            anyhow::ensure!(
                old.offer
                    .as_ref()
                    .is_none_or(|o| record.offer.as_ref() == Some(o)),
                "Original offer changed"
            );
            anyhow::ensure!(
                old.plan
                    .as_ref()
                    .is_none_or(|p| record.plan.as_ref() == Some(p)),
                "Original funding plan changed; cancel and review again before any input mutation"
            );
            anyhow::ensure!(
                old.quote
                    .as_ref()
                    .is_none_or(|q| record.quote.as_ref() == Some(q)),
                "Original quote changed"
            );
            match &old.change_address {
                Some(ChangeAddress::Ready { .. }) => anyhow::ensure!(
                    old.change_address == record.change_address,
                    "Original change allocation changed"
                ),
                Some(ChangeAddress::Dispatched) => anyhow::ensure!(
                    record.change_address.is_some(),
                    "Ambiguous change allocation cannot be forgotten"
                ),
                None => {}
            }
            anyhow::ensure!(
                old.policy
                    .as_ref()
                    .is_none_or(|p| record.policy.as_ref() == Some(p)),
                "Original fee limit changed"
            );
            anyhow::ensure!(
                old.template
                    .as_ref()
                    .is_none_or(|t| record.template.as_ref() == Some(t)),
                "Original transaction template changed"
            );
            anyhow::ensure!(
                old.funded
                    .as_ref()
                    .is_none_or(|p| record.funded.as_ref() == Some(p)),
                "Original funded transaction changed"
            );
            anyhow::ensure!(
                old.signed
                    .as_ref()
                    .is_none_or(|p| record.signed.as_ref() == Some(p)),
                "Original signed transaction changed"
            );
            anyhow::ensure!(
                record.phase >= old.phase,
                "Original transaction phase regressed"
            );
            anyhow::ensure!(
                record.mutations >= old.mutations,
                "Mutation sequence regressed"
            );
        }
        let payload = serde_json::to_string(record)?;
        let bytes = serde_json::to_vec(&Envelope {
            checksum: hex::encode(Sha256::digest(payload.as_bytes())),
            payload,
        })?;
        anyhow::ensure!(bytes.len() <= MAX_RECORD, "Recovery record is too large");
        let temporary = self
            .directory
            .join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<()> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, self.directory.join(format!("{}.json", self.id)))?;
            fs::File::open(&self.directory)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
}
fn read_record(directory: &Path, id: &str) -> Result<Option<Record>> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = match fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(directory.join(format!("{id}.json")))
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    anyhow::ensure!(file.metadata()?.is_file(), "Invalid recovery record");
    let mut bytes = vec![];
    file.take((MAX_RECORD + 1) as u64).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() <= MAX_RECORD, "Recovery record is too large");
    let envelope: Envelope = serde_json::from_slice(&bytes)
        .context("Original payment recovery is damaged; do not pay again")?;
    anyhow::ensure!(
        hex::encode(Sha256::digest(envelope.payload.as_bytes())) == envelope.checksum,
        "Original payment checksum changed"
    );
    let record: Record = serde_json::from_str(&envelope.payload)?;
    record.validate()?;
    anyhow::ensure!(
        record.binding.id == id,
        "Original operation identifier changed"
    );
    Ok(Some(record))
}

pub(crate) fn decode_psbt(funded: &Funded) -> Result<Psbt> {
    anyhow::ensure!(funded.psbt_base64.len() <= MAX_RECORD / 2, "PSBT too large");
    Ok(Psbt::deserialize(
        &base64::engine::general_purpose::STANDARD.decode(&funded.psbt_base64)?,
    )?)
}
pub(crate) fn validate_funded(record: &Record, funded: &Funded) -> Result<u64> {
    use std::collections::{HashMap, HashSet};
    let quote = record.quote.as_ref().context("Missing original quote")?;
    let policy = record
        .policy
        .as_ref()
        .context("Missing original fee limit")?;
    let psbt = decode_psbt(funded)?;
    let tx = &psbt.unsigned_tx;
    anyhow::ensure!(
        !tx.input.is_empty() && tx.input.len() == psbt.inputs.len(),
        "Invalid funded inputs"
    );
    let mut leases = HashMap::new();
    for lease in &funded.leases {
        lease.validate(&record.lock_id)?;
        anyhow::ensure!(
            leases.insert(lease.outpoint()?, lease).is_none(),
            "Duplicate wallet lease"
        );
    }
    anyhow::ensure!(
        leases.len() == tx.input.len(),
        "Funding selected unexpected inputs"
    );
    let mut seen = HashSet::new();
    let mut input_sats = 0u64;
    for (input, metadata) in tx.input.iter().zip(&psbt.inputs) {
        anyhow::ensure!(
            seen.insert(input.previous_output),
            "Duplicate transaction input"
        );
        let lease = leases
            .get(&input.previous_output)
            .context("Input is not leased to this operation")?;
        let utxo = metadata
            .witness_utxo
            .as_ref()
            .context("Input value is not verifiable")?;
        anyhow::ensure!(
            utxo.value.to_sat() == lease.value_sats
                && utxo.script_pubkey.as_bytes() == hex::decode(&lease.script)?,
            "Leased input metadata changed"
        );
        anyhow::ensure!(
            input.script_sig.is_empty() && input.witness.is_empty(),
            "Funded PSBT unexpectedly signed"
        );
        input_sats = input_sats
            .checked_add(lease.value_sats)
            .filter(|v| *v <= MAX_SATS)
            .context("Invalid input sum")?;
    }
    let recipient = quote.script()?;
    let change = bitcoin::ScriptBuf::from_bytes(hex::decode(&policy.change_script)?);
    anyhow::ensure!(
        recipient != change,
        "Recipient and change scripts must differ"
    );
    let mut found = false;
    let mut change_found = false;
    let mut output_sats = 0u64;
    for output in &tx.output {
        if output.script_pubkey == recipient {
            anyhow::ensure!(
                !found && output.value.to_sat() == quote.binding.price_sats,
                "Recipient amount changed"
            );
            found = true;
        } else {
            anyhow::ensure!(
                !change_found && output.script_pubkey == change,
                "Unknown or duplicate change output"
            );
            change_found = true;
        }
        output_sats = output_sats
            .checked_add(output.value.to_sat())
            .filter(|v| *v <= MAX_SATS)
            .context("Invalid output sum")?;
    }
    anyhow::ensure!(found, "Missing original purchase output");
    let fee = input_sats
        .checked_sub(output_sats)
        .context("Outputs exceed inputs")?;
    anyhow::ensure!(
        fee > 0 && fee <= policy.max_fee_sats,
        "Original fee budget exceeded"
    );
    Ok(fee)
}
pub(crate) fn validate_signed(record: &Record, signed: &Signed) -> Result<()> {
    let funded = record.funded.as_ref().context("Missing saved PSBT")?;
    let fee = validate_funded(record, funded)?;
    let expected = decode_psbt(funded)?.unsigned_tx;
    anyhow::ensure!(
        signed.raw_hex.len() <= MAX_RECORD / 2,
        "Signed transaction too large"
    );
    let tx: Transaction = consensus::deserialize(&hex::decode(&signed.raw_hex)?)?;
    anyhow::ensure!(
        tx.compute_txid().to_string() == signed.txid,
        "Signed transaction identifier changed"
    );
    anyhow::ensure!(
        tx.version == expected.version
            && tx.lock_time == expected.lock_time
            && tx.output == expected.output
            && tx.input.len() == expected.input.len(),
        "Signed transaction terms changed"
    );
    for (actual, original) in tx.input.iter().zip(&expected.input) {
        anyhow::ensure!(
            actual.previous_output == original.previous_output
                && actual.sequence == original.sequence
                && actual.script_sig.is_empty()
                && !actual.witness.is_empty(),
            "Signed transaction input changed or incomplete"
        );
    }
    let policy = record
        .policy
        .as_ref()
        .context("Missing original fee policy")?;
    let limit = policy
        .max_fee_rate_sat_vbyte
        .checked_mul(tx.vsize() as u64)
        .context("Fee rate overflow")?;
    anyhow::ensure!(fee <= limit, "Original fee rate exceeded");
    Ok(())
}

fn validate_lease_identity(expected: &[Lease], actual: &[Lease]) -> Result<()> {
    use std::collections::HashMap;
    let mut map = HashMap::new();
    for lease in actual {
        anyhow::ensure!(
            map.insert(lease.outpoint()?, lease).is_none(),
            "Duplicate input lease"
        );
    }
    anyhow::ensure!(
        expected.len() == map.len(),
        "Original leased input set changed"
    );
    for lease in expected {
        let found = map
            .get(&lease.outpoint()?)
            .context("Original input lease missing")?;
        anyhow::ensure!(
            found.lock_id == lease.lock_id
                && found.script == lease.script
                && found.value_sats == lease.value_sats,
            "Original lease binding changed"
        );
    }
    Ok(())
}
/// Adapter can install only a read-only prepared, validated transaction. Every
/// byte and input owner is durable before any LeaseOutput request can be made.
pub(crate) fn save_exact_template(
    journal: &Journal,
    policy: FeePolicy,
    template: Funded,
) -> Result<Record> {
    let mut record = journal.load()?.context("Missing original purchase")?;
    anyhow::ensure!(
        record.phase == Phase::Quoted && !record.externally_exposed && !record.settled,
        "Original address is already exposed or payment started"
    );
    policy.validate()?;
    record.policy = Some(policy);
    validate_funded(&record, &template)?;
    record.template = Some(template);
    record.phase = Phase::TemplatePrepared;
    journal.save(&record)?;
    Ok(record)
}

pub(crate) fn validate_current_leases(
    record: &Record,
    original: &[Lease],
    current: &[Lease],
) -> Result<()> {
    use std::collections::HashMap;
    let mut lookup = HashMap::new();
    let now = u64::try_from(chrono::Utc::now().timestamp()).context("Invalid clock")?;
    for lease in current {
        lease.validate(&record.lock_id)?;
        anyhow::ensure!(
            lease.expires_at > now,
            "Original input lease expired; recover its lease before signing"
        );
        anyhow::ensure!(
            lookup.insert(lease.outpoint()?, lease).is_none(),
            "Duplicate lease evidence"
        );
    }
    anyhow::ensure!(
        lookup.len() == original.len(),
        "Original input leases are missing or changed"
    );
    for lease in original {
        let actual = lookup
            .get(&lease.outpoint()?)
            .context("Original input lease is missing")?;
        anyhow::ensure!(
            actual.value_sats == lease.value_sats && actual.script == lease.script,
            "Original input lease changed"
        );
    }
    Ok(())
}

async fn reconcile_template_leases<W: Wallet>(
    journal: &Journal,
    wallet: &W,
    record: &mut Record,
) -> Result<()> {
    let template = record
        .template
        .clone()
        .context("Original template missing")?;
    validate_funded(record, &template)?;
    for expected in &template.leases {
        let current = wallet.leases(&record.lock_id).await?;
        let now = u64::try_from(chrono::Utc::now().timestamp()).context("Invalid clock")?;
        if let Some(owned) = current
            .iter()
            .find(|l| l.txid == expected.txid && l.vout == expected.vout)
        {
            validate_lease_identity(std::slice::from_ref(expected), std::slice::from_ref(owned))?;
            if owned.expires_at > now.saturating_add(60) {
                continue;
            }
        }
        // Preserve Funded/SigningDispatched intent and immutable original PSBT.
        // Lost renewal replies never allow a replacement input or payment.
        let phase = if record.phase < Phase::Funded {
            Phase::LeaseDispatched
        } else {
            record.phase
        };
        mark_mutation(journal, record, phase)?;
        wallet.lease(expected).await?;
    }
    let current = wallet.leases(&record.lock_id).await?;
    validate_current_leases(record, &template.leases, &current)?;
    record.observed_leases = current;
    journal.save(record)?;
    Ok(())
}

/// Legacy funding recovery reads lease diagnostics only; it cannot reconstruct
/// a missing transaction. The exact-template adapter instead persists all inputs
/// and outputs before individual leases. Neither path releases unknown leases.
pub(crate) trait Wallet {
    /// Read-only capability/network/change-ownership/fee preflight.
    async fn prepare_funding(&self, record: &Record) -> Result<()>;
    async fn fund(&self, record: &Record) -> Result<Funded>;
    async fn leases(&self, lock_id: &str) -> Result<Vec<Lease>>;
    /// Lease/renew exactly this saved outpoint to its saved owner; no selection.
    async fn lease(&self, _lease: &Lease) -> Result<()> {
        anyhow::bail!("Explicit input leasing is unsupported")
    }

    async fn sign(&self, funded: &Funded) -> Result<Signed>;
    async fn publish(&self, signed: &Signed) -> Result<()>;
    async fn transaction_known(&self, txid: &str) -> Result<bool>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Fund,
    Lease,
    RecoverFunding,
    Sign,
    Publish,
    Status,
}
fn mark_mutation(journal: &Journal, record: &mut Record, phase: Phase) -> Result<()> {
    record.phase = phase;
    record.mutations = record
        .mutations
        .checked_add(1)
        .context("Mutation sequence exhausted")?;
    journal.save(record)
}
pub(crate) fn accept_offer(
    journal: &Journal,
    offer: crate::content_onchain_plan::Offer,
) -> Result<Record> {
    offer.validate()?;
    let mut record = journal.load()?.context("Original operation missing")?;
    anyhow::ensure!(
        record.binding == offer.binding && record.retirement.is_none(),
        "Original offer changed or retired"
    );
    if let Some(old) = &record.offer {
        anyhow::ensure!(old == &offer, "Original offer changed");
        return Ok(record);
    }
    anyhow::ensure!(
        record.phase == Phase::AddressRequested && record.quote.is_none(),
        "Original address already requested"
    );
    record.offer = Some(offer);
    record.phase = Phase::OfferPrepared;
    journal.save(&record)?;
    Ok(record)
}
pub(crate) fn save_plan(
    journal: &Journal,
    plan: crate::content_onchain_plan::FundingPlan,
) -> Result<Record> {
    let mut record = journal.load()?.context("Original operation missing")?;
    anyhow::ensure!(
        record.phase == Phase::OfferPrepared
            && record.retirement.is_none()
            && !record.externally_exposed,
        "Original operation cannot prepare new funding"
    );
    plan.validate(&record)?;
    record.plan = Some(plan);
    record.phase = Phase::PlanPrepared;
    journal.save(&record)?;
    Ok(record)
}
pub(crate) async fn lease_plan<W: Wallet>(journal: &Journal, wallet: &W) -> Result<Record> {
    let mut record = journal.load()?.context("Original operation missing")?;
    anyhow::ensure!(
        matches!(
            record.phase,
            Phase::PlanPrepared
                | Phase::PlanLeaseDispatched
                | Phase::InputsLeased
                | Phase::AddressAllocationDispatched
        ) && record.retirement.is_none()
            && !record.externally_exposed,
        "Original funding plan cannot lease inputs"
    );
    let plan = record
        .plan
        .clone()
        .context("Original funding plan missing")?;
    plan.validate(&record)?;
    wallet.prepare_funding(&record).await?;
    for input in &plan.inputs {
        let current = wallet.leases(&record.lock_id).await?;
        let now = u64::try_from(chrono::Utc::now().timestamp())?;
        if let Some(owned) = current
            .iter()
            .find(|l| l.txid == input.lease.txid && l.vout == input.lease.vout)
        {
            validate_lease_identity(
                std::slice::from_ref(&input.lease),
                std::slice::from_ref(owned),
            )?;
            if owned.expires_at > now.saturating_add(60) {
                continue;
            }
        }
        let phase = if record.phase < Phase::InputsLeased {
            Phase::PlanLeaseDispatched
        } else {
            record.phase
        };
        mark_mutation(journal, &mut record, phase)?;
        wallet.lease(&input.lease).await?;
    }
    let current = wallet.leases(&record.lock_id).await?;
    let expected: Vec<_> = plan.inputs.iter().map(|i| i.lease.clone()).collect();
    validate_current_leases(&record, &expected, &current)?;
    record.observed_leases = current;
    if record.phase < Phase::InputsLeased {
        record.phase = Phase::InputsLeased;
    }
    journal.save(&record)?;
    Ok(record)
}
pub(crate) fn mark_address_allocation(journal: &Journal, external: bool) -> Result<Record> {
    let mut record = journal.load()?.context("Original operation missing")?;
    anyhow::ensure!(
        record.retirement.is_none() && record.quote.is_none(),
        "Original operation cannot allocate address"
    );
    if record.phase == Phase::AddressAllocationDispatched {
        return Ok(record);
    }
    anyhow::ensure!(
        if external {
            record.phase == Phase::OfferPrepared
                && record.plan.is_none()
                && record.observed_leases.is_empty()
        } else {
            record.phase == Phase::InputsLeased && record.plan.is_some()
        },
        "Review and confirm original plan before address allocation"
    );
    mark_mutation(journal, &mut record, Phase::AddressAllocationDispatched)?;
    Ok(record)
}
pub(crate) fn bind_plan(journal: &Journal) -> Result<Record> {
    let record = journal.load()?.context("Original operation missing")?;
    if record.template.is_some() {
        return Ok(record);
    }
    let plan = record.plan.as_ref().context("Original plan missing")?;
    let quote = record
        .quote
        .as_ref()
        .context("Original allocated address missing")?;
    let (policy, template) = plan.bind(&record, quote)?;
    save_exact_template(journal, policy, template)
}
pub(crate) fn retire_unallocated(
    journal: &Journal,
    ack: crate::content_onchain_seller::UnallocatedAck,
) -> Result<Record> {
    let mut record = journal.load()?.context("Original operation missing")?;
    ack.validate(&record.binding)?;
    record.retirement = Some(ack);
    record.validate()?;
    journal.save(&record)?;
    Ok(record)
}
pub(crate) fn accept_quote(journal: &Journal, quote: Quote) -> Result<Record> {
    quote.validate()?;
    let mut record = journal.load()?.context("Missing original operation")?;
    anyhow::ensure!(
        record.binding == quote.binding,
        "Original address request changed"
    );
    if let Some(original) = &record.quote {
        anyhow::ensure!(original == &quote, "Original address or source changed");
        return Ok(record);
    }
    anyhow::ensure!(
        matches!(
            record.phase,
            Phase::AddressRequested | Phase::AddressAllocationDispatched
        ),
        "Invalid address operation state"
    );
    record.quote = Some(quote);
    record.phase = Phase::Quoted;
    journal.save(&record)?;
    Ok(record)
}
pub(crate) fn expose_address(journal: &Journal) -> Result<String> {
    let mut record = journal.load()?.context("Missing original operation")?;
    anyhow::ensure!(
        record.phase == Phase::Quoted
            && !record.settled
            && record.plan.is_none()
            && record.observed_leases.is_empty()
            && record.template.is_none()
            && record.funded.is_none(),
        "Native payment already started or purchase paid"
    );
    record.externally_exposed = true;
    journal.save(&record)?;
    Ok(record.quote.context("Missing original address")?.address)
}
pub(crate) async fn drive<W: Wallet>(
    journal: &Journal,
    wallet: &W,
    action: Action,
    policy: Option<FeePolicy>,
) -> Result<Record> {
    let mut record = journal.load()?.context("Missing original operation")?;
    if action == Action::Status {
        if let Some(signed) = &record.signed {
            if matches!(record.phase, Phase::BroadcastDispatched | Phase::Published)
                && wallet.transaction_known(&signed.txid).await?
            {
                record.phase = Phase::Published;
                journal.save(&record)?;
            }
        }
        return Ok(record);
    }
    anyhow::ensure!(
        !record.externally_exposed,
        "Original external address remains payable; recover it without another send"
    );
    anyhow::ensure!(
        !record.settled,
        "Original purchase is paid; recover its download"
    );
    if let (Some(original), Some(requested)) = (&record.policy, &policy) {
        anyhow::ensure!(original == requested, "Original fee policy changed");
    }
    match action {
        Action::Fund => {
            anyhow::ensure!(
                record.phase == Phase::Quoted,
                "Original funding already attempted; recover its leases"
            );
            let policy = policy.context("Explicit original fee policy required")?;
            policy.validate()?;
            record.policy = Some(policy);
            wallet.prepare_funding(&record).await?;
            mark_mutation(journal, &mut record, Phase::FundingDispatched)?;
            let funded = wallet.fund(&record).await?;
            validate_funded(&record, &funded)?;
            record.funded = Some(funded);
            record.phase = Phase::Funded;
            journal.save(&record)?;
        }
        Action::Lease => {
            anyhow::ensure!(
                matches!(
                    record.phase,
                    Phase::TemplatePrepared
                        | Phase::LeaseDispatched
                        | Phase::Funded
                        | Phase::SigningDispatched
                ),
                "Original template is not awaiting leases"
            );
            wallet.prepare_funding(&record).await?;
            let template = record
                .template
                .clone()
                .context("Original template missing")?;
            validate_funded(&record, &template)?;
            reconcile_template_leases(journal, wallet, &mut record).await?;
            if record.funded.is_none() {
                let owned = wallet.leases(&record.lock_id).await?;
                record.funded = Some(Funded {
                    psbt_base64: template.psbt_base64,
                    leases: owned,
                });
                record.phase = Phase::Funded;
                journal.save(&record)?;
            }
        }
        Action::RecoverFunding => {
            anyhow::ensure!(
                record.phase == Phase::FundingDispatched,
                "Original funding is not ambiguous"
            );
            let leases = wallet.leases(&record.lock_id).await?;
            for lease in &leases {
                lease.validate(&record.lock_id)?;
            }
            record.observed_leases = leases.clone();
            journal.save(&record)?;
            // Leases do not contain the original transaction/PSBT. They are
            // diagnostic evidence only, never authority to synthesize another
            // transaction or retry FundPsbt after an ambiguous response.
        }
        Action::Sign => {
            anyhow::ensure!(
                matches!(record.phase, Phase::Funded | Phase::SigningDispatched),
                "Original funding must be recovered first"
            );
            let funded = record
                .funded
                .clone()
                .context("Missing original funded PSBT")?;
            validate_funded(&record, &funded)?;
            if record.template.is_some() {
                wallet.prepare_funding(&record).await?;
                reconcile_template_leases(journal, wallet, &mut record).await?;
            }
            let current = wallet.leases(&record.lock_id).await?;
            validate_current_leases(&record, &funded.leases, &current)?;
            mark_mutation(journal, &mut record, Phase::SigningDispatched)?;
            let signed = wallet.sign(&funded).await?;
            validate_signed(&record, &signed)?;
            record.signed = Some(signed);
            record.phase = Phase::Signed;
            journal.save(&record)?;
        }
        Action::Publish => {
            anyhow::ensure!(
                matches!(
                    record.phase,
                    Phase::Signed | Phase::BroadcastDispatched | Phase::Published
                ),
                "No original signed transaction"
            );
            let signed = record
                .signed
                .clone()
                .context("Missing original signed transaction")?;
            validate_signed(&record, &signed)?;
            if wallet.transaction_known(&signed.txid).await? {
                record.phase = Phase::Published;
                journal.save(&record)?;
                return Ok(record);
            }
            let phase = if record.phase == Phase::Published {
                Phase::Published
            } else {
                Phase::BroadcastDispatched
            };
            mark_mutation(journal, &mut record, phase)?;
            wallet.publish(&signed).await?;
            record.phase = Phase::Published;
            journal.save(&record)?;
        }
        Action::Status => unreachable!(),
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::{
        absolute::LockTime, transaction::Version, Amount, ScriptBuf, Sequence, TxIn, TxOut, Witness,
    };
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    };
    fn script(byte: u8) -> ScriptBuf {
        let mut bytes = vec![0, 20];
        bytes.extend([byte; 20]);
        ScriptBuf::from_bytes(bytes)
    }
    fn policy() -> FeePolicy {
        FeePolicy {
            change_script: hex::encode(script(2).as_bytes()),
            max_fee_sats: 1000,
            max_fee_rate_sat_vbyte: 20,
        }
    }
    fn binding() -> Binding {
        Binding {
            id: uuid::Uuid::new_v4().to_string(),
            buyer_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([7; 32])).unwrap(),
            seller_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([8; 32])).unwrap(),
            content_id: "paid-file".into(),
            price_sats: 546,
        }
    }
    async fn prepared() -> (tempfile::TempDir, Journal) {
        let data = tempfile::tempdir().unwrap();
        let binding = binding();
        let journal = Journal::open(data.path(), &binding.id).await.unwrap();
        let record = Record::new(binding.clone(), format!("{}.onion", "a".repeat(56))).unwrap();
        journal.save(&record).unwrap();
        accept_quote(
            &journal,
            Quote {
                binding,
                address: bitcoin::Address::from_script(&script(1), bitcoin::Network::Regtest)
                    .unwrap()
                    .to_string(),
                network: ChainNetwork::Regtest,
                source: RetainedFile {
                    sha256: "a".repeat(64),
                    size: 4,
                    filename: "bought.txt".into(),
                    mime_type: "text/plain".into(),
                },
            },
        )
        .unwrap();
        (data, journal)
    }
    fn funded(record: &Record) -> Funded {
        let lease = Lease {
            lock_id: record.lock_id.clone(),
            txid: "b".repeat(64),
            vout: 0,
            value_sats: 2000,
            script: hex::encode(script(3).as_bytes()),
            expires_at: 4_000_000_000,
        };
        let tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: lease.outpoint().unwrap(),
                script_sig: ScriptBuf::new(),
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: Witness::new(),
            }],
            output: vec![
                TxOut {
                    value: Amount::from_sat(546),
                    script_pubkey: record.quote.as_ref().unwrap().script().unwrap(),
                },
                TxOut {
                    value: Amount::from_sat(954),
                    script_pubkey: script(2),
                },
            ],
        };
        let mut psbt = Psbt::from_unsigned_tx(tx).unwrap();
        psbt.inputs[0].witness_utxo = Some(TxOut {
            value: Amount::from_sat(lease.value_sats),
            script_pubkey: script(3),
        });
        Funded {
            psbt_base64: base64::engine::general_purpose::STANDARD.encode(psbt.serialize()),
            leases: vec![lease],
        }
    }
    fn signed(funded: &Funded) -> Signed {
        let mut tx = decode_psbt(funded).unwrap().unsigned_tx;
        tx.input[0].witness = Witness::from_slice(&[vec![1; 72], vec![2; 33]]);
        Signed {
            txid: tx.compute_txid().to_string(),
            raw_hex: hex::encode(consensus::serialize(&tx)),
        }
    }
    struct MockWallet {
        fail_preflight: AtomicBool,
        lost_fund: AtomicBool,
        lost_sign: AtomicBool,
        lost_publish: AtomicBool,
        funds: AtomicUsize,
        signs: AtomicUsize,
        publishes: AtomicUsize,
        known: AtomicBool,
        leases: Mutex<Vec<Lease>>,
        signed_inputs: Mutex<Vec<String>>,
        published: Mutex<Vec<String>>,
    }
    impl MockWallet {
        fn new() -> Self {
            Self {
                fail_preflight: AtomicBool::new(false),
                lost_fund: AtomicBool::new(false),
                lost_sign: AtomicBool::new(false),
                lost_publish: AtomicBool::new(false),
                funds: AtomicUsize::new(0),
                signs: AtomicUsize::new(0),
                publishes: AtomicUsize::new(0),
                known: AtomicBool::new(false),
                leases: Mutex::new(vec![]),
                signed_inputs: Mutex::new(vec![]),
                published: Mutex::new(vec![]),
            }
        }
    }
    impl Wallet for MockWallet {
        async fn prepare_funding(&self, _: &Record) -> Result<()> {
            anyhow::ensure!(
                !self.fail_preflight.load(Ordering::SeqCst),
                "wallet unavailable before funding"
            );
            Ok(())
        }
        async fn fund(&self, record: &Record) -> Result<Funded> {
            self.funds.fetch_add(1, Ordering::SeqCst);
            let funded = funded(record);
            *self.leases.lock().unwrap() = funded.leases.clone();
            anyhow::ensure!(
                !self.lost_fund.load(Ordering::SeqCst),
                "funding reply lost after leasing"
            );
            Ok(funded)
        }
        async fn leases(&self, _: &str) -> Result<Vec<Lease>> {
            Ok(self.leases.lock().unwrap().clone())
        }
        async fn sign(&self, funded: &Funded) -> Result<Signed> {
            self.signs.fetch_add(1, Ordering::SeqCst);
            self.signed_inputs
                .lock()
                .unwrap()
                .push(funded.psbt_base64.clone());
            anyhow::ensure!(!self.lost_sign.load(Ordering::SeqCst), "signing reply lost");
            Ok(signed(funded))
        }
        async fn publish(&self, signed: &Signed) -> Result<()> {
            self.publishes.fetch_add(1, Ordering::SeqCst);
            self.published.lock().unwrap().push(signed.raw_hex.clone());
            self.known.store(true, Ordering::SeqCst);
            anyhow::ensure!(
                !self.lost_publish.load(Ordering::SeqCst),
                "publish reply lost after broadcast"
            );
            Ok(())
        }
        async fn transaction_known(&self, _: &str) -> Result<bool> {
            Ok(self.known.load(Ordering::SeqCst))
        }
    }
    #[tokio::test]
    async fn funding_preflight_failure_is_definitely_undispatched() {
        let (_data, j) = prepared().await;
        let wallet = MockWallet::new();
        wallet.fail_preflight.store(true, Ordering::SeqCst);
        assert!(drive(&j, &wallet, Action::Fund, Some(policy()))
            .await
            .is_err());
        let record = j.load().unwrap().unwrap();
        assert_eq!(record.phase, Phase::Quoted);
        assert_eq!(record.mutations, 0);
        assert_eq!(wallet.funds.load(Ordering::SeqCst), 0);
        wallet.fail_preflight.store(false, Ordering::SeqCst);
        assert_eq!(
            drive(&j, &wallet, Action::Fund, Some(policy()))
                .await
                .unwrap()
                .phase,
            Phase::Funded
        );
    }
    #[tokio::test]
    async fn lost_funding_reply_preserves_original_lease_diagnostics_without_refunding() {
        let (data, j) = prepared().await;
        let id = j.id.clone();
        let wallet = MockWallet::new();
        wallet.lost_fund.store(true, Ordering::SeqCst);
        assert!(drive(&j, &wallet, Action::Fund, Some(policy()))
            .await
            .is_err());
        drop(j);
        let j = Journal::open(data.path(), &id).await.unwrap();
        assert_eq!(j.load().unwrap().unwrap().phase, Phase::FundingDispatched);
        assert!(drive(&j, &wallet, Action::Fund, Some(policy()))
            .await
            .is_err());
        assert_eq!(
            drive(&j, &wallet, Action::Status, None)
                .await
                .unwrap()
                .phase,
            Phase::FundingDispatched
        );
        let recovered = drive(&j, &wallet, Action::RecoverFunding, None)
            .await
            .unwrap();
        assert_eq!(recovered.phase, Phase::FundingDispatched);
        assert_eq!(recovered.observed_leases, *wallet.leases.lock().unwrap());
        assert!(recovered.funded.is_none());
        assert!(drive(&j, &wallet, Action::Sign, None).await.is_err());
        assert_eq!(wallet.funds.load(Ordering::SeqCst), 1);
        assert_eq!(wallet.signs.load(Ordering::SeqCst), 0);
        assert_eq!(wallet.publishes.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn missing_or_foreign_funding_leases_never_allow_fresh_coin_selection() {
        let (_data, j) = prepared().await;
        let wallet = MockWallet::new();
        wallet.lost_fund.store(true, Ordering::SeqCst);
        assert!(drive(&j, &wallet, Action::Fund, Some(policy()))
            .await
            .is_err());
        let original = wallet.leases.lock().unwrap().clone();
        wallet.leases.lock().unwrap().clear();
        assert_eq!(
            drive(&j, &wallet, Action::RecoverFunding, None)
                .await
                .unwrap()
                .phase,
            Phase::FundingDispatched
        );
        let mut foreign = original;
        foreign[0].lock_id = "f".repeat(64);
        *wallet.leases.lock().unwrap() = foreign;
        assert!(drive(&j, &wallet, Action::RecoverFunding, None)
            .await
            .is_err());
        assert!(drive(&j, &wallet, Action::Fund, Some(policy()))
            .await
            .is_err());
        assert_eq!(wallet.funds.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn duplicate_lease_diagnostics_cannot_replace_original_funding() {
        let (_data, j) = prepared().await;
        let wallet = MockWallet::new();
        wallet.lost_fund.store(true, Ordering::SeqCst);
        assert!(drive(&j, &wallet, Action::Fund, Some(policy()))
            .await
            .is_err());
        {
            let mut leases = wallet.leases.lock().unwrap();
            let duplicate = leases[0].clone();
            leases.push(duplicate);
        }
        assert!(drive(&j, &wallet, Action::RecoverFunding, None)
            .await
            .is_err());
        assert_eq!(j.load().unwrap().unwrap().phase, Phase::FundingDispatched);
        assert_eq!(wallet.signs.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn signing_and_broadcast_reply_loss_reuse_original_psbt_and_raw_transaction() {
        let (data, j) = prepared().await;
        let id = j.id.clone();
        let wallet = MockWallet::new();
        drive(&j, &wallet, Action::Fund, Some(policy()))
            .await
            .unwrap();
        wallet.lost_sign.store(true, Ordering::SeqCst);
        assert!(drive(&j, &wallet, Action::Sign, None).await.is_err());
        drop(j);
        let j = Journal::open(data.path(), &id).await.unwrap();
        assert_eq!(j.load().unwrap().unwrap().phase, Phase::SigningDispatched);
        wallet.lost_sign.store(false, Ordering::SeqCst);
        let signed = drive(&j, &wallet, Action::Sign, None).await.unwrap();
        {
            let attempts = wallet.signed_inputs.lock().unwrap();
            assert_eq!(attempts[0], attempts[1]);
        }
        wallet.lost_publish.store(true, Ordering::SeqCst);
        assert!(drive(&j, &wallet, Action::Publish, None).await.is_err());
        assert_eq!(j.load().unwrap().unwrap().signed, signed.signed);
        drop(j);
        let j = Journal::open(data.path(), &id).await.unwrap();
        assert_eq!(
            drive(&j, &wallet, Action::Status, None)
                .await
                .unwrap()
                .phase,
            Phase::Published
        );
        drive(&j, &wallet, Action::Publish, None).await.unwrap();
        assert_eq!(wallet.publishes.load(Ordering::SeqCst), 1);
        // Eviction is not permission to make a new transaction. An explicit
        // rebroadcast sends the exact bytes whose txid is already durable.
        wallet.known.store(false, Ordering::SeqCst);
        wallet.lost_publish.store(false, Ordering::SeqCst);
        drive(&j, &wallet, Action::Publish, None).await.unwrap();
        let sent = wallet.published.lock().unwrap();
        assert_eq!(sent[0], sent[1]);
        assert_eq!(wallet.funds.load(Ordering::SeqCst), 1);
    }
    #[tokio::test]
    async fn expired_or_missing_input_leases_stop_before_signing() {
        let (_data, j) = prepared().await;
        let wallet = MockWallet::new();
        drive(&j, &wallet, Action::Fund, Some(policy()))
            .await
            .unwrap();
        wallet.leases.lock().unwrap()[0].expires_at = 1;
        assert!(drive(&j, &wallet, Action::Sign, None).await.is_err());
        assert_eq!(wallet.signs.load(Ordering::SeqCst), 0);
        assert_eq!(j.load().unwrap().unwrap().phase, Phase::Funded);
    }
    #[tokio::test]
    async fn exposed_address_cannot_expire_into_new_native_payment_or_another_rail() {
        let (data, j) = prepared().await;
        let id = j.id.clone();
        let address = expose_address(&j).unwrap();
        drop(j);
        let j = Journal::open(data.path(), &id).await.unwrap();
        let wallet = MockWallet::new();
        assert_eq!(expose_address(&j).unwrap(), address);
        assert!(drive(&j, &wallet, Action::Fund, Some(policy()))
            .await
            .is_err());
        let record = drive(&j, &wallet, Action::Status, None).await.unwrap();
        assert!(record.blocks_other_rails());
        assert!(record.externally_exposed);
        assert_eq!(wallet.funds.load(Ordering::SeqCst), 0);
    }
    #[tokio::test]
    async fn fee_output_input_and_signed_mutations_fail_before_remote_sign_or_broadcast() {
        let (_data, j) = prepared().await;
        let mut record = j.load().unwrap().unwrap();
        record.policy = Some(policy());
        record.phase = Phase::FundingDispatched;
        let original = funded(&record);
        for variant in 0..4 {
            let mut altered = original.clone();
            let mut psbt = decode_psbt(&altered).unwrap();
            match variant {
                0 => psbt.unsigned_tx.output[0].value = Amount::from_sat(547),
                1 => psbt.unsigned_tx.output[1].script_pubkey = script(9),
                2 => psbt.unsigned_tx.output[1].value = Amount::from_sat(1),
                _ => psbt.inputs[0].witness_utxo.as_mut().unwrap().value = Amount::from_sat(9000),
            }
            altered.psbt_base64 =
                base64::engine::general_purpose::STANDARD.encode(psbt.serialize());
            assert!(validate_funded(&record, &altered).is_err());
        }
        record.funded = Some(original.clone());
        record.phase = Phase::Funded;
        let good = signed(&original);
        validate_signed(&record, &good).unwrap();
        let mut tx: Transaction =
            consensus::deserialize(&hex::decode(&good.raw_hex).unwrap()).unwrap();
        tx.output[0].value = Amount::from_sat(547);
        let bad = Signed {
            txid: tx.compute_txid().to_string(),
            raw_hex: hex::encode(consensus::serialize(&tx)),
        };
        assert!(validate_signed(&record, &bad).is_err());
    }
    #[tokio::test]
    async fn journal_damage_and_late_state_regression_never_restore_spending_permission() {
        let (data, j) = prepared().await;
        let wallet = MockWallet::new();
        let old = j.load().unwrap().unwrap();
        drive(&j, &wallet, Action::Fund, Some(policy()))
            .await
            .unwrap();
        assert!(j.save(&old).is_err());
        let target = data
            .path()
            .join("content-onchain")
            .join(format!("{}.json", j.id));
        fs::write(&target, b"broken original operation").unwrap();
        assert!(drive(&j, &wallet, Action::Fund, Some(policy()))
            .await
            .is_err());
        assert_eq!(wallet.funds.load(Ordering::SeqCst), 1);
    }
}
