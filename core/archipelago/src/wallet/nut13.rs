//! NUT-13 deterministic secrets — what makes the ecash wallet restorable.
//!
//! Until this module existed, every Cashu proof this node held was backed by a
//! secret drawn from `OsRng` and written to exactly one file. Losing
//! `wallet/ecash.json` lost the coins outright: there was no phrase to write
//! down, and no amount of talking to the mint could reconstruct them. Ecash is
//! a bearer instrument, so "one file, no backup" was the sharpest edge in the
//! wallet.
//!
//! [NUT-13] fixes that by deriving each proof's secret and blinding factor
//! from `(wallet seed, keyset id, counter)` instead of from randomness. The
//! wallet is then a *phrase*, and the coins can be re-derived and re-claimed
//! from the mint — here, or in any other NUT-13 wallet.
//!
//! Three pieces live here:
//!
//! - **The wallet seed** (`wallet/cashu_seed.json`) — a 24-word BIP-39
//!   mnemonic derived from the node's master seed, so the node's own recovery
//!   phrase already covers the ecash. See [`crate::seed::derive_cashu_mnemonic`]
//!   for why it is a *separate* phrase rather than the node's own.
//! - **The counters** (`wallet/cashu_counters.json`) — the next unused counter
//!   per keyset. Recovery metadata, not funds: losing it costs a restore scan,
//!   never coins.
//! - **The derivation itself** — delegated to the reference implementation, so
//!   the secrets a third-party wallet re-derives from these words are the same
//!   ones we did.
//!
//! ## Why the seed sits on disk in the clear
//!
//! The node's master seed is encrypted at rest and needs the operator's
//! password to open, which no background mint/swap can ask for. This file is
//! not encrypted, and that is deliberate: it lives in the same directory as
//! `wallet/ecash.json`, which already holds spendable bearer secrets in
//! plaintext. A NUT-13 seed regenerates exactly those same secrets, so it is
//! the same sensitivity class as the file beside it — encrypting one and not
//! the other would buy nothing. It is written 0600, matching
//! `identity/nostr_secret`, which is derived and persisted the same way.
//!
//! [NUT-13]: https://github.com/cashubtc/nuts/blob/main/13.md

use anyhow::{Context, Result};
use bitcoin::secp256k1::SecretKey;
use cashu::nuts::nut01::SecretKey as CdkSecretKey;
use cashu::nuts::nut02::Id as CdkId;
use cashu::secret::Secret as CdkSecret;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tokio::fs;
use tracing::{debug, warn};

/// The wallet's BIP-39 phrase. One file for both networks: NUT-13 derivation
/// is keyed by keyset id, and a testnet mint's keysets never collide with a
/// real mint's, so the two purses cannot derive each other's secrets.
const SEED_FILE: &str = "wallet/cashu_seed.json";
/// Next-unused counter per keyset.
const COUNTER_FILE: &str = "wallet/cashu_counters.json";

/// Serialises counter reservation within this process. Reservation is a
/// read-modify-write of one small file, and two concurrent mints handing out
/// the same counter would mean two proofs with the same secret — the mint
/// signs both and only one is ever spendable.
static COUNTER_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// On-disk shape of `wallet/cashu_seed.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSeed {
    /// The 24-word BIP-39 phrase.
    mnemonic: String,
    /// How this wallet got its phrase — see [`SeedSource`].
    #[serde(default)]
    source: SeedSource,
    /// When it was first written, for the operator's benefit.
    #[serde(default)]
    created_at: String,
}

/// Where an ecash wallet's phrase came from, which decides what restoring the
/// *node* gets you back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SeedSource {
    /// Derived from the node's master seed. The node's 24 words restore this
    /// ecash wallet too — nothing extra to write down.
    #[default]
    NodeSeed,
    /// Generated independently of the node seed. Still a perfectly good
    /// NUT-13 wallet, but restoring the node from its recovery phrase will
    /// *not* bring it back — only these words will.
    Independent,
    /// Supplied by the operator, from another NUT-13 wallet. Same caveat as
    /// `Independent` — the node's recovery phrase does not cover it — but it
    /// is worth telling apart, because these words exist somewhere else too
    /// and the operator already knows where.
    Imported,
}

impl SeedSource {
    /// Does restoring the *node* from its recovery phrase bring this wallet
    /// back? Only a derived phrase can promise that.
    pub fn covered_by_node_seed(&self) -> bool {
        matches!(self, Self::NodeSeed)
    }
}

/// A loaded ecash wallet seed, ready to derive secrets from.
#[derive(Clone)]
pub struct EcashSeed {
    /// BIP-39 seed bytes — the NUT-13 input.
    seed: [u8; 64],
    mnemonic: bip39::Mnemonic,
    source: SeedSource,
}

impl std::fmt::Debug for EcashSeed {
    /// Never let the phrase or the seed bytes reach a log line.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EcashSeed")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

impl EcashSeed {
    fn from_mnemonic(mnemonic: bip39::Mnemonic, source: SeedSource) -> Self {
        Self {
            seed: mnemonic.to_seed(""),
            mnemonic,
            source,
        }
    }

    /// The 24 words, for the backup screen. Everything else about this type
    /// keeps them out of reach.
    pub fn words(&self) -> Vec<String> {
        self.mnemonic.words().map(|w| w.to_string()).collect()
    }

    /// The phrase as a single string — the input to NUT-13 *and* to the NIP-06
    /// Nostr derivation the Minibits profile flow needs (`crate::wallet::minibits`).
    pub fn phrase(&self) -> String {
        self.mnemonic.to_string()
    }

    /// The 64-byte BIP-39 seed. Same bytes Minibits hashes with SHA-256 to get
    /// its `seedHash`, so the two wallets agree on wallet identity.
    pub fn seed_bytes(&self) -> [u8; 64] {
        self.seed
    }

    pub fn source(&self) -> SeedSource {
        self.source
    }

    /// Derive the NUT-13 secret and blinding factor for one output.
    ///
    /// Delegated to the reference implementation rather than reimplemented:
    /// NUT-13 uses BIP-32 for v1 keyset ids and an HMAC-SHA256 KDF for v2, and
    /// getting either subtly wrong yields a wallet whose words restore
    /// *nothing* — a failure that only shows up on the day it matters.
    pub fn derive_output(&self, keyset_id: &str, counter: u32) -> Result<(Vec<u8>, SecretKey)> {
        let id = CdkId::from_str(keyset_id)
            .with_context(|| format!("Keyset id {keyset_id} is not one NUT-13 can derive for"))?;

        let secret = CdkSecret::from_seed(&self.seed, id, counter)
            .context("NUT-13 secret derivation failed")?;
        let blinding = CdkSecretKey::from_seed(&self.seed, id, counter)
            .context("NUT-13 blinding-factor derivation failed")?;
        let blinding = SecretKey::from_slice(&blinding.to_secret_bytes())
            .context("NUT-13 produced a blinding factor secp256k1 rejects")?;

        Ok((secret.to_bytes(), blinding))
    }
}

impl Drop for EcashSeed {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.seed.zeroize();
    }
}

fn seed_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SEED_FILE)
}

/// Is this wallet backed by a phrase yet?
pub fn seed_exists(data_dir: &Path) -> bool {
    seed_path(data_dir).exists()
}

/// Load the wallet seed, or `None` if this node has never established one.
///
/// A *damaged* seed file is an error, not a `None`: silently treating it as
/// "no seed" would send the wallet back to unrecoverable random secrets while
/// telling the operator their backup was fine.
pub async fn load_seed(data_dir: &Path) -> Result<Option<EcashSeed>> {
    let path = seed_path(data_dir);
    let content = match fs::read_to_string(&path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("Could not read the existing ecash backup seed"),
    };
    let stored: StoredSeed = serde_json::from_str(&content)
        .with_context(|| format!("The ecash seed file is damaged: {}", path.display()))?;
    let mnemonic: bip39::Mnemonic = stored
        .mnemonic
        .parse()
        .map_err(|e| anyhow::anyhow!("The stored ecash phrase is not valid BIP-39: {e}"))?;
    Ok(Some(EcashSeed::from_mnemonic(mnemonic, stored.source)))
}

/// Establish the wallet seed from the node's master seed, writing it if this
/// node does not have one yet.
///
/// Idempotent, and deliberately **never overwrites**: an existing phrase is
/// the only thing that can re-derive the proofs already minted under it, so a
/// re-derivation that disagreed (a different master seed after a restore from
/// different words, say) must not be allowed to replace it. The existing seed
/// is returned instead, and the mismatch is logged.
pub async fn establish_from_master(
    data_dir: &Path,
    master: &crate::seed::MasterSeed,
) -> Result<EcashSeed> {
    let _mutation = super::mutation::guard(data_dir).await?;
    let derived = crate::seed::derive_cashu_mnemonic(master)?;

    if let Some(existing) = load_seed(data_dir).await? {
        if existing.mnemonic != derived {
            warn!(
                "The ecash wallet's phrase does not match the one this node's master seed \
                 derives — keeping the existing phrase, because it is what the current \
                 proofs were minted under. Back it up from Settings; the node's own \
                 recovery phrase does not cover this wallet."
            );
        }
        return Ok(existing);
    }

    write_seed(data_dir, &derived, SeedSource::NodeSeed).await?;
    debug!("Established the ecash wallet seed from the node master seed");
    Ok(EcashSeed::from_mnemonic(derived, SeedSource::NodeSeed))
}

/// Establish a wallet seed that is **not** derived from the node's master
/// seed, for a node that has no encrypted master seed to derive from.
///
/// Plenty of nodes are in that position: `identity/master_seed.enc` is written
/// during onboarding, and any node onboarded before that step existed simply
/// does not have one. The choice there is not "derived phrase or independent
/// phrase" — it is "independent phrase or **no backup at all**", and a wallet
/// whose coins can be restored from words the operator holds is strictly
/// better than one whose coins die with a single file.
///
/// The cost is stated plainly rather than hidden: the phrase is recorded as
/// [`SeedSource::Independent`], and every surface that shows it says that
/// restoring the node will *not* bring this wallet back — only these words
/// will. That is a real obligation on the operator, so it must never be the
/// silent default when derivation was possible; [`establish_from_master`] is
/// what a node with a master seed gets.
pub async fn establish_independent(data_dir: &Path) -> Result<EcashSeed> {
    let _mutation = super::mutation::guard(data_dir).await?;
    if let Some(existing) = load_seed(data_dir).await? {
        return Ok(existing);
    }
    // Same guarded generation path as the node's own seed: a named CSPRNG and
    // the degenerate-entropy check, not a dependency's default (KEY-05).
    let (mnemonic, _seed) = crate::seed::MasterSeed::generate()?;
    write_seed(data_dir, &mnemonic, SeedSource::Independent).await?;
    warn!(
        "Established an INDEPENDENT ecash backup phrase: this node has no encrypted \
         master seed to derive one from, so restoring the node will not restore this \
         ecash wallet — only the phrase itself will."
    );
    Ok(EcashSeed::from_mnemonic(mnemonic, SeedSource::Independent))
}

/// Adopt a phrase the operator supplies, from another NUT-13 wallet.
///
/// This is the "bring your own" path: it points the wallet at someone else's
/// derivation, which is what makes coins held in Minibits, Nutstash or
/// `cdk-cli` restorable here.
///
/// Replacing a phrase is the one genuinely lossy thing this module can do.
/// Coins already in `wallet/ecash.json` stay spendable — they are proofs, not
/// derivations, and nothing here touches them — but they were minted under
/// the *old* phrase, so a future restore will no longer find them. The old
/// phrase is therefore archived rather than overwritten, and replacing an
/// established one needs `confirm`. An operator who imports by mistake must
/// not lose the only copy of the words their balance was minted under.
///
/// Counters are deliberately left alone. They are per-keyset and seed-
/// relative, so under a new seed they merely start high — which costs nothing,
/// since a restore scans from zero regardless. Resetting them would be the
/// dangerous choice if the imported phrase turned out to be the one already
/// in use.
pub async fn import_mnemonic(data_dir: &Path, words: &str, confirm: bool) -> Result<EcashSeed> {
    let _mutation = super::mutation::guard(data_dir).await?;
    let mnemonic: bip39::Mnemonic = words
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .parse()
        .map_err(|e| {
            anyhow::anyhow!(
                "That is not a valid BIP-39 recovery phrase: {e}. Check for typos — \
             every word must come from the BIP-39 word list, and the phrase as \
             a whole carries a checksum."
            )
        })?;

    if let Some(existing) = load_seed(data_dir).await? {
        if existing.mnemonic == mnemonic {
            // Importing the phrase already in use: nothing to do, and
            // certainly nothing to archive.
            return Ok(existing);
        }
        if !confirm {
            anyhow::bail!(
                "This wallet already has a backup phrase. Importing a different one \
                 means coins minted under the current phrase will no longer be \
                 restorable from words — they stay spendable, but a restore will \
                 not find them. Reveal and write down the current phrase first, \
                 then confirm to replace it."
            );
        }
        archive_seed(data_dir).await?;
    }

    write_seed(data_dir, &mnemonic, SeedSource::Imported).await?;
    warn!("Ecash backup phrase REPLACED by an imported one (the previous phrase, if any, was archived)");
    Ok(EcashSeed::from_mnemonic(mnemonic, SeedSource::Imported))
}

/// Durably copy the current seed before replacing it; keep the live seed on failure.
///
/// Never deleted and never overwritten: this file may be the last copy of the
/// words a balance was minted under, and the whole point of the module is that
/// such a thing is not casually destroyed.
async fn archive_seed(data_dir: &Path) -> Result<()> {
    let from = seed_path(data_dir);
    let content = fs::read(&from)
        .await
        .context("Could not read the previous ecash phrase for backup")?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let to = data_dir.join(format!(
        "wallet/cashu_seed.replaced-{stamp}-{}.json",
        uuid::Uuid::new_v4()
    ));
    persist_private_file(&to, &content)
        .await
        .context("Could not durably archive the previous ecash phrase")?;
    warn!("Previous ecash phrase archived to {}", to.display());
    Ok(())
}

/// Write the seed file at 0600, creating the wallet directory if needed.
async fn write_seed(data_dir: &Path, mnemonic: &bip39::Mnemonic, source: SeedSource) -> Result<()> {
    let path = seed_path(data_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .context("Failed to create the wallet directory")?;
    }
    let stored = StoredSeed {
        mnemonic: mnemonic.to_string(),
        source,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let content =
        serde_json::to_string_pretty(&stored).context("Failed to serialize the ecash seed")?;
    persist_private_file(&path, content.as_bytes()).await?;
    Ok(())
}

// ── Counters ───────────────────────────────────────────────────────────────

/// On-disk shape of `wallet/cashu_counters.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct StoredCounters {
    /// keyset id → next unused counter.
    counters: BTreeMap<String, u32>,
}

/// Reserve `count` consecutive counters for `keyset_id` and return the first.
///
/// Written to disk **before** the outputs are used, and never rolled back on
/// failure. A gap in the sequence costs a restore scan a few extra probes; a
/// *reused* counter costs a coin, because two proofs with the same secret can
/// only ever be spent once. So the asymmetry is resolved in favour of gaps.
pub async fn reserve_counters(data_dir: &Path, keyset_id: &str, count: usize) -> Result<u32> {
    let _guard = COUNTER_LOCK.lock().await;
    let path = data_dir.join(COUNTER_FILE);

    let mut state: StoredCounters = match fs::read_to_string(&path).await {
        Ok(content) => serde_json::from_str(&content)
            .with_context(|| format!("The ecash counter file is damaged: {}", path.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => StoredCounters::default(),
        Err(error) => return Err(error).context("Could not read the ecash counter file"),
    };

    let start = *state.counters.get(keyset_id).unwrap_or(&0);
    let next = start
        .checked_add(u32::try_from(count).context("Absurd output count")?)
        .context("NUT-13 counter space exhausted for this keyset")?;
    state.counters.insert(keyset_id.to_string(), next);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .context("Failed to create the wallet directory")?;
    }
    let content =
        serde_json::to_string_pretty(&state).context("Failed to serialize ecash counters")?;
    persist_private_file(&path, content.as_bytes()).await?;

    Ok(start)
}

/// Never truncate the active reservation file. A reservation is not usable
/// until both its replacement file and directory entry have reached storage.
async fn persist_private_file(path: &Path, content: &[u8]) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    struct PendingPrivateFile(PathBuf);
    impl Drop for PendingPrivateFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let parent = path
        .parent()
        .context("Private wallet file has no directory")?;
    let temporary =
        PendingPrivateFile(parent.join(format!(".cashu-private-{}.tmp", uuid::Uuid::new_v4())));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary.0)
        .await
        .context("Could not create the private ecash state")?;
    file.write_all(content)
        .await
        .context("Could not write the private ecash state")?;
    file.sync_all()
        .await
        .context("Could not flush the private ecash state")?;
    drop(file);
    fs::rename(&temporary.0, path)
        .await
        .context("Could not replace the private ecash state")?;
    fs::File::open(parent)
        .await?
        .sync_all()
        .await
        .context("Could not flush the private ecash directory")?;
    Ok(())
}

/// Read the next-unused counter for a keyset without reserving anything.
pub async fn counter_for(data_dir: &Path, keyset_id: &str) -> u32 {
    let path = data_dir.join(COUNTER_FILE);
    let Ok(content) = fs::read_to_string(&path).await else {
        return 0;
    };
    serde_json::from_str::<StoredCounters>(&content)
        .ok()
        .and_then(|s| s.counters.get(keyset_id).copied())
        .unwrap_or(0)
}

/// Move a keyset's counter forward to at least `next`, so a restore that found
/// coins beyond the recorded point cannot hand the same counters out again.
pub async fn advance_counter_to(data_dir: &Path, keyset_id: &str, next: u32) -> Result<()> {
    let current = counter_for(data_dir, keyset_id).await;
    if next > current {
        reserve_counters(data_dir, keyset_id, (next - current) as usize).await?;
    }
    Ok(())
}

// ── The source handed to the mint client ───────────────────────────────────

/// Supplies NUT-13 outputs to [`crate::wallet::mint_client::MintClient`].
///
/// Holds the data directory as well as the seed because reserving a counter is
/// a disk write that has to happen before the outputs are handed out.
#[derive(Clone, Debug)]
pub struct RecoverySource {
    seed: EcashSeed,
    data_dir: PathBuf,
}

impl RecoverySource {
    /// Build a recovery source for this node, or `None` when the wallet has no
    /// seed yet. Callers fall back to random secrets in that case, which is
    /// exactly the pre-NUT-13 behaviour — correct, just not restorable.
    pub async fn load(data_dir: &Path) -> Result<Option<Self>> {
        match load_seed(data_dir).await? {
            Some(seed) => Ok(Some(Self {
                seed,
                data_dir: data_dir.to_path_buf(),
            })),
            None => Ok(None),
        }
    }

    /// Reserve and derive `count` outputs for `keyset_id`.
    pub async fn next_outputs(
        &self,
        keyset_id: &str,
        count: usize,
    ) -> Result<Vec<(Vec<u8>, SecretKey)>> {
        // Fail the derivation *before* burning counters if this keyset id is
        // one NUT-13 cannot address.
        self.seed.derive_output(keyset_id, 0)?;
        let start = reserve_counters(&self.data_dir, keyset_id, count).await?;
        (0..count)
            .map(|i| self.seed.derive_output(keyset_id, start + i as u32))
            .collect()
    }

    /// Derive one output at an explicit counter, without reserving — the
    /// restore scan's probe, which must be able to re-derive the past.
    pub fn derive_at(&self, keyset_id: &str, counter: u32) -> Result<(Vec<u8>, SecretKey)> {
        self.seed.derive_output(keyset_id, counter)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed::MasterSeed;

    const TEST_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
    /// A real NUT-02 v1 keyset id (the one in the NUT test vectors).
    const V1_KEYSET: &str = "009a1f293253e41e";
    /// A NUT-02 v2 keyset id — 33 bytes, version byte 0x01. The two versions
    /// take different derivation paths in the spec, so both need covering.
    const V2_KEYSET: &str = "01fc0ec0e59cd6fa01b7a88f8cd77fce81fd1e64bca67d752e984992b7a3c3a821";

    fn seed() -> EcashSeed {
        let (_, master) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let mnemonic = crate::seed::derive_cashu_mnemonic(&master).unwrap();
        EcashSeed::from_mnemonic(mnemonic, SeedSource::NodeSeed)
    }

    /// The whole promise of NUT-13: the same phrase and counter must give back
    /// the same secret, or a restore finds nothing.
    #[test]
    fn the_same_phrase_and_counter_rederive_the_same_output() {
        let a = seed();
        let b = seed();
        for keyset in [V1_KEYSET, V2_KEYSET] {
            let (s1, r1) = a.derive_output(keyset, 7).unwrap();
            let (s2, r2) = b.derive_output(keyset, 7).unwrap();
            assert_eq!(s1, s2, "secret must be reproducible ({keyset})");
            assert_eq!(
                r1.secret_bytes(),
                r2.secret_bytes(),
                "blinding factor must be reproducible ({keyset})"
            );
        }
    }

    /// Different counters — and different keysets — must not collide, or two
    /// proofs would share a secret and only one could ever be spent.
    #[test]
    fn different_counters_and_keysets_give_different_outputs() {
        let s = seed();
        let (a, _) = s.derive_output(V1_KEYSET, 0).unwrap();
        let (b, _) = s.derive_output(V1_KEYSET, 1).unwrap();
        let (c, _) = s.derive_output(V2_KEYSET, 0).unwrap();
        assert_ne!(a, b, "counter must separate secrets");
        assert_ne!(a, c, "keyset must separate secrets");
    }

    /// The secret must look like the one the rest of the wallet expects: a
    /// 32-byte value, hex-encoded, carried as ASCII bytes — the same shape
    /// `bdhke::generate_secret` produces.
    #[test]
    fn a_derived_secret_has_the_shape_the_wallet_already_uses() {
        let (secret, _) = seed().derive_output(V1_KEYSET, 0).unwrap();
        assert_eq!(secret.len(), 64, "32 bytes, hex-encoded");
        let text = String::from_utf8(secret).expect("secret must be ASCII hex");
        assert!(hex::decode(&text).is_ok(), "{text}");
    }

    /// A truncated v2 id cannot address a keyset, and must fail loudly rather
    /// than deriving from a prefix that means nothing.
    #[test]
    fn an_unaddressable_keyset_id_is_refused() {
        let err = seed()
            .derive_output("01fc0ec0e59cd6fa", 0)
            .expect_err("short v2 id must not derive");
        assert!(err.to_string().contains("NUT-13"), "{err}");
    }

    #[tokio::test]
    async fn counters_are_reserved_in_order_and_never_reused() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();

        assert_eq!(reserve_counters(d, V1_KEYSET, 3).await.unwrap(), 0);
        assert_eq!(reserve_counters(d, V1_KEYSET, 2).await.unwrap(), 3);
        assert_eq!(counter_for(d, V1_KEYSET).await, 5);

        // A second keyset counts independently.
        assert_eq!(reserve_counters(d, V2_KEYSET, 1).await.unwrap(), 0);
        assert_eq!(counter_for(d, V1_KEYSET).await, 5);
    }

    /// Reservation must survive a process restart — the file is the state.
    #[tokio::test]
    async fn reserved_counters_persist_across_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        reserve_counters(d, V1_KEYSET, 4).await.unwrap();
        // Nothing cached in memory: read it back cold.
        assert_eq!(counter_for(d, V1_KEYSET).await, 4);
        assert_eq!(reserve_counters(d, V1_KEYSET, 1).await.unwrap(), 4);
    }

    #[tokio::test]
    async fn establishing_the_seed_is_idempotent_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let (_, master) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();

        assert!(!seed_exists(d));
        let first = establish_from_master(d, &master).await.unwrap();
        assert!(seed_exists(d));
        assert_eq!(first.source(), SeedSource::NodeSeed);

        let second = establish_from_master(d, &master).await.unwrap();
        assert_eq!(first.words(), second.words());

        // A *different* master seed must not replace the phrase the existing
        // proofs were minted under.
        let (other_words, _) = MasterSeed::generate().unwrap();
        let (_, other_master) = MasterSeed::from_mnemonic_words(&other_words.to_string()).unwrap();
        let third = establish_from_master(d, &other_master).await.unwrap();
        assert_eq!(
            first.words(),
            third.words(),
            "an established ecash phrase must never be silently replaced"
        );
    }

    /// A phrase from another wallet must derive that wallet's secrets — that
    /// is the entire point of importing one.
    #[tokio::test]
    async fn an_imported_phrase_derives_the_other_wallets_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();

        // Stand in for the other wallet: a known phrase and what it derives.
        let theirs: bip39::Mnemonic = TEST_MNEMONIC.parse().unwrap();
        let expected = EcashSeed::from_mnemonic(theirs.clone(), SeedSource::Imported)
            .derive_output(V1_KEYSET, 3)
            .unwrap();

        let imported = import_mnemonic(d, TEST_MNEMONIC, false).await.unwrap();
        assert_eq!(imported.source(), SeedSource::Imported);
        assert!(!imported.source().covered_by_node_seed());
        assert_eq!(imported.derive_output(V1_KEYSET, 3).unwrap().0, expected.0);

        // And it is what the wallet uses from now on.
        let reloaded = load_seed(d).await.unwrap().expect("persisted");
        assert_eq!(reloaded.words(), imported.words());
    }

    #[tokio::test]
    async fn importing_over_an_established_phrase_needs_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let (_, master) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        let original = establish_from_master(d, &master).await.unwrap();
        let original_words = original.words();

        // Refused without confirmation — replacing a phrase silently would
        // orphan every coin minted under it.
        let (other, _) = MasterSeed::generate().unwrap();
        let err = import_mnemonic(d, &other.to_string(), false)
            .await
            .expect_err("must not replace without confirmation");
        assert!(
            err.to_string().contains("already has a backup phrase"),
            "{err}"
        );
        assert_eq!(
            load_seed(d).await.unwrap().unwrap().words(),
            original_words,
            "a refused import must change nothing"
        );

        // Confirmed: replaced, and the old phrase archived rather than lost.
        import_mnemonic(d, &other.to_string(), true).await.unwrap();
        assert_eq!(
            load_seed(d).await.unwrap().unwrap().words(),
            other.words().map(|w| w.to_string()).collect::<Vec<_>>()
        );
        let archived: Vec<_> = std::fs::read_dir(d.join("wallet"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("cashu_seed.replaced-")
            })
            .collect();
        assert_eq!(archived.len(), 1, "the replaced phrase must be kept");
    }

    /// Re-importing the phrase already in use is a no-op, not a replacement —
    /// it must not archive anything or churn the file.
    #[tokio::test]
    async fn importing_the_current_phrase_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let first = import_mnemonic(d, TEST_MNEMONIC, false).await.unwrap();
        let again = import_mnemonic(d, TEST_MNEMONIC, false).await.unwrap();
        assert_eq!(first.words(), again.words());
        let archived = std::fs::read_dir(d.join("wallet"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("cashu_seed.replaced-")
            })
            .count();
        assert_eq!(archived, 0);
    }

    #[tokio::test]
    async fn a_malformed_phrase_is_refused_with_something_actionable() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        // Right shape, wrong checksum — the commonest real mistake.
        let bad = TEST_MNEMONIC.replace(" art", " abandon");
        let err = import_mnemonic(d, &bad, false).await.expect_err("checksum");
        assert!(err.to_string().contains("BIP-39"), "{err}");
        assert!(!seed_exists(d), "a rejected phrase must not be written");

        assert!(import_mnemonic(d, "not a phrase", false).await.is_err());
        assert!(import_mnemonic(d, "", false).await.is_err());
    }

    /// Whitespace and casing vary wildly in what people paste out of other
    /// wallets; the words are what matter.
    #[tokio::test]
    async fn a_pasted_phrase_survives_untidy_whitespace() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let messy = format!("  {}  ", TEST_MNEMONIC.replace(' ', "\n  "));
        let imported = import_mnemonic(d, &messy, false).await.unwrap();
        assert_eq!(imported.words().len(), 24);
        assert_eq!(imported.words().join(" "), TEST_MNEMONIC);
    }

    #[tokio::test]
    async fn the_seed_file_is_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let (_, master) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        establish_from_master(d, &master).await.unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(seed_path(d))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "the ecash phrase must be owner-only");
        }
    }

    /// A damaged seed file must not read back as "this wallet has no backup" —
    /// that would quietly return the wallet to unrecoverable random secrets.
    #[tokio::test]
    async fn a_damaged_seed_file_is_an_error_not_an_absence() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::create_dir_all(d.join("wallet")).await.unwrap();
        fs::write(seed_path(d), "{ truncated").await.unwrap();

        assert!(load_seed(d).await.is_err());
        assert!(
            RecoverySource::load(d).await.is_err(),
            "an unusable seed must not downgrade to an unbacked wallet"
        );
    }

    #[tokio::test]
    async fn the_recovery_source_hands_out_consecutive_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let (_, master) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        establish_from_master(d, &master).await.unwrap();

        let source = RecoverySource::load(d)
            .await
            .unwrap()
            .expect("seed was established");
        let first = source.next_outputs(V1_KEYSET, 2).await.unwrap();
        let second = source.next_outputs(V1_KEYSET, 2).await.unwrap();

        assert_eq!(first.len(), 2);
        // Counters advanced, so no secret repeats across the two batches.
        let secrets: std::collections::HashSet<_> = first
            .iter()
            .chain(second.iter())
            .map(|(s, _)| s.clone())
            .collect();
        assert_eq!(secrets.len(), 4, "counters must not be handed out twice");

        // And the batch is exactly what re-deriving counters 0..4 gives.
        for (i, (secret, _)) in first.iter().chain(second.iter()).enumerate() {
            let (expected, _) = source.derive_at(V1_KEYSET, i as u32).unwrap();
            assert_eq!(secret, &expected);
        }
    }

    #[tokio::test]
    async fn missing_seed_is_distinct_from_a_seed_read_failure() {
        let dir = tempfile::tempdir().unwrap();
        assert!(RecoverySource::load(dir.path()).await.unwrap().is_none());
        fs::create_dir_all(seed_path(dir.path())).await.unwrap();
        assert!(load_seed(dir.path()).await.is_err());
        assert!(RecoverySource::load(dir.path()).await.is_err());
        assert!(seed_path(dir.path()).is_dir());
    }

    #[tokio::test]
    async fn empty_or_corrupt_counters_never_reset_a_reservation() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("wallet")).await.unwrap();
        let path = dir.path().join(COUNTER_FILE);
        for damaged in ["", "   ", "{ truncated", "{}", "{\"counters\":null}"] {
            fs::write(&path, damaged).await.unwrap();
            assert!(reserve_counters(dir.path(), V1_KEYSET, 1).await.is_err());
            assert_eq!(fs::read_to_string(&path).await.unwrap(), damaged);
        }
        fs::remove_file(&path).await.unwrap();
        fs::create_dir(&path).await.unwrap();
        assert!(reserve_counters(dir.path(), V1_KEYSET, 1).await.is_err());
        assert!(path.is_dir());
    }

    #[tokio::test]
    async fn concurrent_counter_reservations_survive_reload_and_leave_no_temporary_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut tasks = Vec::new();
        for _ in 0..24 {
            let path = dir.path().to_path_buf();
            tasks.push(tokio::spawn(async move {
                reserve_counters(&path, V1_KEYSET, 2).await.unwrap()
            }));
        }
        let mut starts = std::collections::HashSet::new();
        for task in tasks {
            assert!(starts.insert(task.await.unwrap()));
        }
        assert_eq!(counter_for(dir.path(), V1_KEYSET).await, 48);
        assert_eq!(
            reserve_counters(dir.path(), V1_KEYSET, 1).await.unwrap(),
            48
        );
        let entries = std::fs::read_dir(dir.path().join("wallet"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(
            entries,
            vec![std::ffi::OsString::from("cashu_counters.json")]
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(dir.path().join(COUNTER_FILE))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[tokio::test]
    async fn invalid_derivation_does_not_reserve_counters() {
        let dir = tempfile::tempdir().unwrap();
        let (_, master) = MasterSeed::from_mnemonic_words(TEST_MNEMONIC).unwrap();
        establish_from_master(dir.path(), &master).await.unwrap();
        let source = RecoverySource::load(dir.path()).await.unwrap().unwrap();
        assert!(source.next_outputs("01fc0ec0e59cd6fa", 1).await.is_err());
        assert!(!dir.path().join(COUNTER_FILE).exists());
    }
    #[tokio::test]
    async fn archiving_keeps_the_live_seed_and_never_overwrites_an_earlier_backup() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        establish_independent(dir.path()).await.unwrap();
        let live = fs::read(seed_path(dir.path())).await.unwrap();
        archive_seed(dir.path()).await.unwrap();
        archive_seed(dir.path()).await.unwrap();
        assert_eq!(fs::read(seed_path(dir.path())).await.unwrap(), live);
        let backups: Vec<_> = std::fs::read_dir(dir.path().join("wallet"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("cashu_seed.replaced-")
            })
            .collect();
        assert_eq!(backups.len(), 2);
        for backup in backups {
            assert_eq!(std::fs::read(&backup).unwrap(), live);
            assert_eq!(
                std::fs::metadata(backup).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[tokio::test]
    async fn simultaneous_seed_establishment_keeps_one_identity() {
        let dir = tempfile::tempdir().unwrap();
        let (first, second) = tokio::join!(
            establish_independent(dir.path()),
            establish_independent(dir.path())
        );
        assert_eq!(first.unwrap().phrase(), second.unwrap().phrase());
    }
}
