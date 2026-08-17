//! Cashu ecash wallet for peer-to-peer micropayments and streaming data payments.
//!
//! Real Cashu protocol implementation using BDHKE blind signatures.
//! Connects to Cashu-compatible mints (local Fedimint or external) for
//! mint/melt/swap operations. Stores proofs locally in the data directory.

use super::cashu::{amount_to_denominations, CashuToken, Proof};
use super::mint_client::MintClient;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tracing::{debug, info, warn};

const WALLET_FILE: &str = "wallet/ecash.json";
const MINTS_FILE: &str = "wallet/accepted_mints.json";
const NETWORK_FILE: &str = "wallet/network.json";

/// Transaction type for history tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    Mint,
    Melt,
    Send,
    Receive,
    StreamingPayment,
    StreamingRevenue,
}

/// Transaction history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcashTransaction {
    pub id: String,
    pub tx_type: TransactionType,
    pub amount_sats: u64,
    pub timestamp: String,
    #[serde(default)]
    pub description: String,
    /// Mint URL involved in this transaction.
    #[serde(default)]
    pub mint_url: String,
    /// Peer identifier (DID, pubkey, or onion) if applicable.
    #[serde(default)]
    pub peer: String,
    /// Which ecash system this entry belongs to: "cashu" or "fedimint". Lets the
    /// unified history/UI label each transaction. Defaults to "cashu" so legacy
    /// stored entries (written before dual-ecash) read back correctly.
    #[serde(default = "default_tx_kind")]
    pub kind: String,
}

/// Default `kind` for transactions persisted before the dual-ecash split.
pub fn default_tx_kind() -> String {
    "cashu".to_string()
}

/// A stored proof with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredProof {
    /// The Cashu proof.
    #[serde(flatten)]
    pub proof: Proof,
    /// Mint URL this proof is from.
    pub mint_url: String,
    /// Whether this proof has been spent (sent to someone or melted).
    #[serde(default)]
    pub spent: bool,
    /// Whether this proof is reserved (allocated to an in-progress operation).
    #[serde(default)]
    pub reserved: bool,
    /// Timestamp when received.
    pub created_at: String,
}

/// Persistent wallet state.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WalletState {
    /// All proofs (spent and unspent).
    pub proofs: Vec<StoredProof>,
    /// Transaction history.
    pub transactions: Vec<EcashTransaction>,
    /// Primary mint URL.
    #[serde(default)]
    pub mint_url: String,

    // ── Legacy compatibility ──
    // Old wallet format had a `tokens` field. If present during deserialization,
    // we migrate to proofs. This field is never written.
    #[serde(default, skip_serializing)]
    tokens: Vec<serde_json::Value>,
}

/// Accepted mints configuration.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AcceptedMints {
    /// List of mint URLs we accept tokens from.
    pub mints: Vec<String>,
}

impl WalletState {
    /// Total balance of unspent, unreserved proofs.
    pub fn balance(&self) -> u64 {
        self.proofs
            .iter()
            .filter(|p| !p.spent && !p.reserved)
            .map(|p| p.proof.amount)
            .sum()
    }

    /// Balance from a specific mint.
    pub fn balance_for_mint(&self, mint_url: &str) -> u64 {
        self.proofs
            .iter()
            .filter(|p| !p.spent && !p.reserved && p.mint_url == mint_url)
            .map(|p| p.proof.amount)
            .sum()
    }

    /// Spendable (unspent, unreserved) balance grouped by mint URL.
    pub fn spendable_by_mint(&self) -> Vec<(String, u64)> {
        use std::collections::BTreeMap;
        let mut by_mint: BTreeMap<String, u64> = BTreeMap::new();
        for p in &self.proofs {
            if !p.spent && !p.reserved {
                *by_mint.entry(p.mint_url.clone()).or_default() += p.proof.amount;
            }
        }
        by_mint.into_iter().collect()
    }

    /// Select unspent proofs that cover at least `amount` sats from a specific mint.
    /// Returns selected proofs and any overpayment amount.
    pub fn select_proofs(&self, mint_url: &str, amount: u64) -> Option<(Vec<usize>, u64)> {
        let mut candidates: Vec<(usize, u64)> = self
            .proofs
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.spent && !p.reserved && p.mint_url == mint_url)
            .map(|(i, p)| (i, p.proof.amount))
            .collect();

        // Sort by amount ascending for efficient selection
        candidates.sort_by_key(|&(_, a)| a);

        let mut selected = Vec::new();
        let mut total = 0u64;

        for (idx, amt) in &candidates {
            if total >= amount {
                break;
            }
            selected.push(*idx);
            total += amt;
        }

        if total >= amount {
            Some((selected, total - amount))
        } else {
            None
        }
    }

    /// Mark proofs at given indices as spent.
    pub fn mark_spent(&mut self, indices: &[usize]) {
        for &i in indices {
            if i < self.proofs.len() {
                self.proofs[i].spent = true;
            }
        }
    }

    /// Add new proofs to the wallet.
    pub fn add_proofs(&mut self, mint_url: &str, proofs: Vec<Proof>) {
        let now = chrono::Utc::now().to_rfc3339();
        for proof in proofs {
            self.proofs.push(StoredProof {
                proof,
                mint_url: mint_url.to_string(),
                spent: false,
                reserved: false,
                created_at: now.clone(),
            });
        }
    }

    /// Record a transaction.
    pub fn record_tx(
        &mut self,
        tx_type: TransactionType,
        amount_sats: u64,
        description: &str,
        mint_url: &str,
        peer: &str,
    ) {
        self.transactions.push(EcashTransaction {
            id: uuid::Uuid::new_v4().to_string(),
            tx_type,
            amount_sats,
            timestamp: chrono::Utc::now().to_rfc3339(),
            description: description.to_string(),
            mint_url: mint_url.to_string(),
            peer: peer.to_string(),
            kind: "cashu".to_string(),
        });
    }

    /// Prune spent proofs older than 30 days to keep wallet file manageable.
    pub fn prune_old_spent(&mut self) {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(30);
        let cutoff_str = cutoff.to_rfc3339();
        self.proofs
            .retain(|p| !p.spent || p.created_at > cutoff_str);
    }
}

/// Which ecash network this node's wallet is operating on.
///
/// Cashu itself has no notion of a testnet — a "test" wallet is simply one
/// pointed at a mint that issues valueless coins (the public `testnut` mint).
/// Modelling it as a network setting rather than "just add a mint" matters
/// because the two must never share a purse: test proofs and real proofs in
/// one file would be spendable interchangeably, and a balance would be a lie.
///
/// So each network gets its own wallet and its own accepted-mints list.
/// **Mainnet keeps the original filenames**, so an existing node's funds file
/// is untouched by this feature and by switching back and forth.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EcashNetwork {
    #[default]
    Mainnet,
    Testnet,
}

impl EcashNetwork {
    fn wallet_file(&self) -> &'static str {
        match self {
            Self::Mainnet => WALLET_FILE,
            Self::Testnet => "wallet/ecash.testnet.json",
        }
    }

    fn mints_file(&self) -> &'static str {
        match self {
            Self::Mainnet => MINTS_FILE,
            Self::Testnet => "wallet/accepted_mints.testnet.json",
        }
    }

    /// The mint a fresh wallet on this network starts out trusting.
    pub fn default_mint(&self) -> String {
        match self {
            Self::Mainnet => default_mint_url(),
            // Public test mint: issues coins with no monetary value, and hands
            // them out freely, so every route (mint/melt/send/receive/swap)
            // can be exercised end to end without risking real sats.
            Self::Testnet => "https://testnut.cashu.space".to_string(),
        }
    }

    pub fn is_test(&self) -> bool {
        matches!(self, Self::Testnet)
    }
}

/// Read the node's ecash network. Absent file = mainnet, so nodes that never
/// touch this setting behave exactly as before.
pub async fn load_network(data_dir: &Path) -> EcashNetwork {
    let path = data_dir.join(NETWORK_FILE);
    let Ok(content) = fs::read_to_string(&path).await else {
        return EcashNetwork::Mainnet;
    };
    serde_json::from_str::<NetworkConfig>(&content)
        .map(|c| c.network)
        .unwrap_or_default()
}

/// Switch the node's ecash network. The other network's wallet is left on
/// disk untouched, so switching is reversible and loses nothing.
pub async fn save_network(data_dir: &Path, network: EcashNetwork) -> Result<()> {
    let dir = data_dir.join("wallet");
    fs::create_dir_all(&dir)
        .await
        .context("Failed to create wallet dir")?;
    let content = serde_json::to_string_pretty(&NetworkConfig { network })
        .context("Failed to serialize ecash network")?;
    fs::write(data_dir.join(NETWORK_FILE), content)
        .await
        .context("Failed to write ecash network")?;
    Ok(())
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct NetworkConfig {
    #[serde(default)]
    network: EcashNetwork,
}

/// Load wallet state from disk.
pub async fn load_wallet(data_dir: &Path) -> Result<WalletState> {
    let network = load_network(data_dir).await;
    let path = data_dir.join(network.wallet_file());
    if !path.exists() {
        return Ok(WalletState {
            mint_url: network.default_mint(),
            ..Default::default()
        });
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read wallet file")?;
    let mut wallet: WalletState = serde_json::from_str(&content).unwrap_or_default();

    // Set default mint URL if empty
    if wallet.mint_url.is_empty() {
        wallet.mint_url = network.default_mint();
    }

    Ok(wallet)
}

/// Save wallet state to disk.
pub async fn save_wallet(data_dir: &Path, wallet: &WalletState) -> Result<()> {
    let dir = data_dir.join("wallet");
    fs::create_dir_all(&dir)
        .await
        .context("Failed to create wallet dir")?;
    let path = data_dir.join(load_network(data_dir).await.wallet_file());
    let content = serde_json::to_string_pretty(wallet).context("Failed to serialize wallet")?;
    fs::write(&path, content)
        .await
        .context("Failed to write wallet file")?;
    Ok(())
}

/// Load accepted mints list.
pub async fn load_accepted_mints(data_dir: &Path) -> Result<AcceptedMints> {
    let network = load_network(data_dir).await;
    let path = data_dir.join(network.mints_file());
    if !path.exists() {
        return Ok(AcceptedMints {
            mints: vec![network.default_mint()],
        });
    }
    let content = fs::read_to_string(&path)
        .await
        .context("Failed to read accepted mints")?;
    let mints: AcceptedMints = serde_json::from_str(&content).unwrap_or(AcceptedMints {
        mints: vec![network.default_mint()],
    });
    Ok(mints)
}

/// Save accepted mints list.
pub async fn save_accepted_mints(data_dir: &Path, mints: &AcceptedMints) -> Result<()> {
    let dir = data_dir.join("wallet");
    fs::create_dir_all(&dir)
        .await
        .context("Failed to create wallet dir")?;
    let path = data_dir.join(load_network(data_dir).await.mints_file());
    let content =
        serde_json::to_string_pretty(mints).context("Failed to serialize accepted mints")?;
    fs::write(&path, content)
        .await
        .context("Failed to write accepted mints")?;
    Ok(())
}

/// Request a mint quote — returns a Lightning invoice to pay.
pub async fn mint_quote(
    data_dir: &Path,
    amount_sats: u64,
) -> Result<super::mint_client::MintQuote> {
    let wallet = load_wallet(data_dir).await?;
    let client = MintClient::new(&wallet.mint_url)?;
    client.mint_quote(amount_sats).await
}

/// Mint new ecash tokens after a Lightning invoice has been paid.
pub async fn mint_tokens(data_dir: &Path, quote_id: &str, amount_sats: u64) -> Result<u64> {
    let mut wallet = load_wallet(data_dir).await?;
    let mint_url = wallet.mint_url.clone();
    let client = MintClient::new(&mint_url)?;

    let result = client.mint_tokens(quote_id, amount_sats).await?;
    let minted: u64 = result.proofs.iter().map(|p| p.amount).sum();

    wallet.add_proofs(&mint_url, result.proofs);
    wallet.record_tx(
        TransactionType::Mint,
        minted,
        &format!("Minted {} sats from Lightning", minted),
        &mint_url,
        "",
    );
    save_wallet(data_dir, &wallet).await?;

    debug!("Minted {} sats ecash", minted);
    Ok(minted)
}

/// Request a melt quote — how much to pay a Lightning invoice with ecash.
pub async fn melt_quote(data_dir: &Path, bolt11: &str) -> Result<super::mint_client::MeltQuote> {
    let wallet = load_wallet(data_dir).await?;
    let client = MintClient::new(&wallet.mint_url)?;
    client.melt_quote(bolt11).await
}

/// Melt ecash tokens to pay a Lightning invoice.
pub async fn melt_tokens(data_dir: &Path, quote_id: &str, bolt11: &str) -> Result<u64> {
    let mut wallet = load_wallet(data_dir).await?;
    let mint_url = wallet.mint_url.clone();
    let client = MintClient::new(&mint_url)?;

    // Get the melt quote to know the amount needed
    let quote = client.melt_quote(bolt11).await?;
    let total_needed = quote.amount + quote.fee_reserve;

    // Select proofs to cover the amount
    let (indices, _overpayment) =
        wallet
            .select_proofs(&mint_url, total_needed)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Insufficient balance: need {} sats, have {} sats",
                    total_needed,
                    wallet.balance_for_mint(&mint_url)
                )
            })?;

    let proofs: Vec<Proof> = indices
        .iter()
        .map(|&i| wallet.proofs[i].proof.clone())
        .collect();
    let spent_amount: u64 = proofs.iter().map(|p| p.amount).sum();

    // Execute melt
    let result = client.melt_tokens(quote_id, &proofs).await?;

    // Mark proofs as spent
    wallet.mark_spent(&indices);
    wallet.record_tx(
        TransactionType::Melt,
        quote.amount,
        &format!(
            "Melted {} sats to Lightning (fee: {})",
            quote.amount, quote.fee_reserve
        ),
        &mint_url,
        "",
    );

    // If there was overpayment and the melt returned change, we could handle it here.
    // For now, any overpayment beyond fee_reserve is lost (mints may return change in future NUTs).
    let _ = result;
    let _ = spent_amount;

    save_wallet(data_dir, &wallet).await?;
    debug!("Melted {} sats to Lightning", quote.amount);
    Ok(quote.amount)
}

// ── Cross-mint settlement (plan §2a / phasing F2) ──────────────────────────
//
// The wallet data model is already multi-mint (proofs, balances and selection
// are all keyed by mint URL). What was hardcoded to the home mint is the
// convenience layer. These `*_at` helpers parameterize that layer by target
// mint, and `swap_between_mints` moves value across mints over Lightning so a
// node holding tokens on mint A can pay a seeder that only accepts mint B.

/// How long to wait for a target mint's Lightning invoice to settle before
/// claiming the freshly-minted tokens.
const SWAP_CLAIM_TIMEOUT_SECS: u64 = 60;
/// Poll interval while waiting for the invoice to settle.
const SWAP_CLAIM_POLL_SECS: u64 = 2;

/// Whether we trust a mint enough to swap value *into* it (or accept its tokens).
///
/// The local Fedimint (home mint) is always trusted; any other mint must be on
/// the configured accepted-mints allow-list. Comparison ignores a trailing
/// slash so advertised URLs match stored ones. See plan §2a "Mint trust list".
pub async fn is_mint_trusted(data_dir: &Path, mint_url: &str) -> Result<bool> {
    let norm = |s: &str| s.trim_end_matches('/').to_string();
    let target = norm(mint_url);
    if target == norm(&default_mint_url()) {
        return Ok(true);
    }
    let accepted = load_accepted_mints(data_dir).await?;
    Ok(accepted.mints.iter().any(|m| norm(m) == target))
}

/// All-in cost of a swap, relative to the amount actually delivered.
///
/// `total_paid` is what the source mint charges (melt amount + LN fee reserve);
/// `amount_delivered` is what lands on the target mint. The difference is the
/// fee the user pays to move value across mints.
fn swap_fee(total_paid: u64, amount_delivered: u64) -> u64 {
    total_paid.saturating_sub(amount_delivered)
}

/// Move `amount_sats` of value from mint `from_mint` to mint `to_mint` over
/// Lightning, returning the amount claimed on the target mint.
///
/// Flow (Cashu/Fedimint settle over BOLT11):
/// 1. `mint_quote` on the target → a Lightning invoice to pay.
/// 2. `melt_quote` on the source → the cost in source tokens (amount + fee).
/// 3. Fee-cap check: refuse if the all-in fee exceeds `max_fee_sats`.
/// 4. Select source proofs and `melt` them to pay the target's invoice.
/// 5. Once the invoice settles, `mint` (claim) the tokens on the target.
///
/// Crash-safety: the source spend is persisted *before* the claim, so a crash
/// between paying and claiming never double-spends — at worst the target tokens
/// are left unclaimed (reconcilable from the mint quote id). Idempotent resume
/// is phasing step 3 (deferred).
pub async fn swap_between_mints(
    data_dir: &Path,
    from_mint: &str,
    to_mint: &str,
    amount_sats: u64,
    max_fee_sats: u64,
) -> Result<u64> {
    if amount_sats == 0 {
        anyhow::bail!("swap amount must be greater than zero");
    }
    let norm = |s: &str| s.trim_end_matches('/').to_string();
    if norm(from_mint) == norm(to_mint) {
        anyhow::bail!("swap source and target mints are identical");
    }
    if !is_mint_trusted(data_dir, to_mint).await? {
        anyhow::bail!(
            "target mint '{}' is not in the trusted/accepted mint list",
            to_mint
        );
    }

    let from = MintClient::new(from_mint)?;
    let to = MintClient::new(to_mint)?;

    // 1. Mint quote on the target → invoice to pay.
    let mint_quote = to
        .mint_quote(amount_sats)
        .await
        .with_context(|| format!("requesting mint quote at target mint {}", to_mint))?;

    // 2. Melt quote on the source for that invoice → cost in source tokens.
    let melt_quote = from
        .melt_quote(&mint_quote.request)
        .await
        .with_context(|| format!("requesting melt quote at source mint {}", from_mint))?;
    let total_needed = melt_quote.amount + melt_quote.fee_reserve;
    let fee = swap_fee(total_needed, amount_sats);

    // 3. Fee-cap check — caller falls back to free origin if too expensive.
    if fee > max_fee_sats {
        anyhow::bail!(
            "swap fee {} sats exceeds cap {} sats (need {} on {} to deliver {} on {})",
            fee,
            max_fee_sats,
            total_needed,
            from_mint,
            amount_sats,
            to_mint
        );
    }

    // 4. Select source proofs and melt them to pay the target invoice.
    let mut wallet = load_wallet(data_dir).await?;
    let (indices, _overpayment) =
        wallet
            .select_proofs(from_mint, total_needed)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "insufficient balance on {}: need {} sats, have {} sats",
                    from_mint,
                    total_needed,
                    wallet.balance_for_mint(from_mint)
                )
            })?;
    let proofs: Vec<Proof> = indices
        .iter()
        .map(|&i| wallet.proofs[i].proof.clone())
        .collect();

    if let Err(e) = from.melt_tokens(&melt_quote.quote, &proofs).await {
        // The pay leg never completed — record the route failure so future
        // payments can prefer a route with a track record.
        record_swap_failure(data_dir, from_mint, to_mint).await;
        return Err(e).with_context(|| {
            format!(
                "melting source proofs at {} to pay target invoice",
                from_mint
            )
        });
    }

    // Persist the spend BEFORE claiming so a crash can't double-spend, and
    // journal the in-flight swap so the claim can be resumed after a crash.
    wallet.mark_spent(&indices);
    wallet.record_tx(
        TransactionType::Melt,
        total_needed,
        &format!(
            "Cross-mint swap {}→{}: paid {} sats (fee {})",
            from_mint, to_mint, total_needed, fee
        ),
        from_mint,
        to_mint,
    );
    save_wallet(data_dir, &wallet).await?;
    add_pending_swap(
        data_dir,
        PendingSwap {
            from_mint: from_mint.to_string(),
            to_mint: to_mint.to_string(),
            amount_sats,
            melt_quote_id: melt_quote.quote.clone(),
            mint_quote_id: mint_quote.quote.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    )
    .await?;

    // 5. Wait for the invoice to settle, then claim the minted tokens.
    wait_for_mint_quote_paid(&to, &mint_quote.quote).await?;
    let result = to
        .mint_tokens(&mint_quote.quote, amount_sats)
        .await
        .with_context(|| format!("claiming minted tokens at target mint {}", to_mint))?;
    let minted: u64 = result.proofs.iter().map(|p| p.amount).sum();

    let mut wallet = load_wallet(data_dir).await?;
    wallet.add_proofs(to_mint, result.proofs);
    wallet.record_tx(
        TransactionType::Mint,
        minted,
        &format!(
            "Cross-mint swap {}→{}: claimed {} sats",
            from_mint, to_mint, minted
        ),
        to_mint,
        from_mint,
    );
    save_wallet(data_dir, &wallet).await?;

    // Swap fully settled — clear the journal entry and credit the route.
    remove_pending_swap(data_dir, &mint_quote.quote).await?;
    record_swap_success(data_dir, from_mint, to_mint).await;

    debug!(
        "Cross-mint swap complete: {} → {} delivered {} sats (fee {})",
        from_mint, to_mint, minted, fee
    );
    Ok(minted)
}

/// Poll a mint quote until its Lightning invoice is paid (state `PAID`/`ISSUED`),
/// or time out. The melt above pays the invoice; the target mint sees it settle
/// shortly after.
async fn wait_for_mint_quote_paid(client: &MintClient, quote_id: &str) -> Result<()> {
    let deadline = SWAP_CLAIM_TIMEOUT_SECS / SWAP_CLAIM_POLL_SECS.max(1);
    for _ in 0..deadline.max(1) {
        let status = client.mint_quote_status(quote_id).await?;
        match status.state.as_str() {
            "PAID" | "ISSUED" => return Ok(()),
            _ => tokio::time::sleep(std::time::Duration::from_secs(SWAP_CLAIM_POLL_SECS)).await,
        }
    }
    anyhow::bail!(
        "target mint invoice for quote {} did not settle within {}s",
        quote_id,
        SWAP_CLAIM_TIMEOUT_SECS
    )
}

/// Create a cashuA token string to send to a peer, drawing from the home mint.
pub async fn send_token(data_dir: &Path, amount_sats: u64) -> Result<String> {
    let mint_url = load_wallet(data_dir).await?.mint_url;
    send_token_at(data_dir, &mint_url, amount_sats).await
}

/// Create a cashuA token denominated in a specific mint's tokens.
///
/// Used by the payer-side cross-mint flow: after `swap_between_mints` lands value
/// on the seeder's accepted mint, we send a token from *that* mint so the seeder
/// only ever receives its own mint's proofs (see plan §2a, payer-side swap).
pub async fn send_token_at(data_dir: &Path, mint_url: &str, amount_sats: u64) -> Result<String> {
    let mut wallet = load_wallet(data_dir).await?;
    let mint_url = mint_url.to_string();

    // Select proofs covering the amount
    let (indices, overpayment) = wallet
        .select_proofs(&mint_url, amount_sats)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Insufficient balance: need {} sats, have {} sats",
                amount_sats,
                wallet.balance_for_mint(&mint_url)
            )
        })?;

    let selected_proofs: Vec<Proof> = indices
        .iter()
        .map(|&i| wallet.proofs[i].proof.clone())
        .collect();

    // If there's overpayment, swap to get exact change
    let send_proofs = if overpayment > 0 {
        let client = MintClient::new(&mint_url)?;
        let send_denoms = amount_to_denominations(amount_sats);
        let change_denoms = amount_to_denominations(overpayment);

        let mut all_target: Vec<u64> = send_denoms.clone();
        all_target.extend(&change_denoms);

        let swap_result = client.swap(&selected_proofs, &all_target).await?;

        // Mark original proofs as spent
        wallet.mark_spent(&indices);

        // Separate send proofs from change proofs BY COUNT, not membership:
        // partition(contains) put EVERY proof whose denomination appeared in
        // send_denoms into the token — when change shared a denomination with
        // the send (worst case: spending from a proof worth exactly 2× the
        // amount, send [n] + change [n]), the change proofs rode along and the
        // receiver was credited double. Consume exactly one proof per needed
        // send denomination; everything else is change.
        let mut send_needed = send_denoms.clone();
        let mut send: Vec<Proof> = Vec::new();
        let mut change: Vec<Proof> = Vec::new();
        for p in swap_result.new_proofs {
            if let Some(pos) = send_needed.iter().position(|&d| d == p.amount) {
                send_needed.swap_remove(pos);
                send.push(p);
            } else {
                change.push(p);
            }
        }
        if !send_needed.is_empty() {
            anyhow::bail!(
                "Mint swap returned incomplete send denominations (missing {:?})",
                send_needed
            );
        }

        // Add change proofs back to wallet
        if !change.is_empty() {
            wallet.add_proofs(&mint_url, change);
        }

        send
    } else {
        // Exact amount — just mark as spent and use directly
        wallet.mark_spent(&indices);
        selected_proofs
    };

    // Serialize as cashuA token
    let token = CashuToken::new(&mint_url, send_proofs);
    let token_str = token.serialize()?;

    wallet.record_tx(
        TransactionType::Send,
        amount_sats,
        &format!("Sent {} sats ecash", amount_sats),
        &mint_url,
        "",
    );
    save_wallet(data_dir, &wallet).await?;

    debug!("Created send token for {} sats", amount_sats);
    Ok(token_str)
}

// ── Payer-side payment builder (plan §2a step 2) ───────────────────────────
//
// Given a seeder's advertised `accepted_mints`, pick the cheapest way to pay:
// spend tokens we already hold on an accepted mint (no fee), else swap value
// into a *trusted* accepted mint and pay from there. If neither is possible
// within budget, decline so the caller falls back to free origin.

/// How a payment of a given amount can be satisfied across our mints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaymentPlan {
    /// We already hold enough on this accepted mint — pay directly, no swap fee.
    Direct { mint_url: String },
    /// Swap value from `from_mint` into the trusted accepted `to_mint`, then pay.
    Swap { from_mint: String, to_mint: String },
    /// No single mint can cover the amount (caller should use free origin).
    Insufficient,
}

/// Decide how to pay `amount` sats to a seeder, given what we hold and which
/// mints the seeder accepts.
///
/// - `holdings`: our spendable `(mint_url, balance)` pairs (verbatim URLs).
/// - `accepted`: the seeder's `(mint_url, trusted)` pairs, where `trusted`
///   means the mint is on our swap-into allow-list (`is_mint_trusted`).
/// - Direct beats Swap (no fee). A Direct target needs no trust (we already
///   hold those tokens); a Swap target must be trusted. The home mint is
///   preferred as a tie-break for both legs (lowest friction).
///
/// Pure and synchronous so it can be unit-tested without a live mint. It does
/// not know swap fees; `swap_between_mints` enforces the fee cap and bails (→
/// origin fallback) if the chosen source can't cover amount + fee.
fn plan_payment(
    holdings: &[(String, u64)],
    accepted: &[(String, bool)],
    amount: u64,
) -> PaymentPlan {
    let norm = |s: &str| s.trim_end_matches('/').to_string();
    let home = norm(&default_mint_url());
    let held = |mint: &str| -> u64 {
        holdings
            .iter()
            .filter(|(m, _)| norm(m) == norm(mint))
            .map(|(_, b)| *b)
            .sum()
    };

    // 1. Direct: any accepted mint we already hold enough on. Prefer home.
    let mut direct: Vec<&(String, bool)> =
        accepted.iter().filter(|(m, _)| held(m) >= amount).collect();
    direct.sort_by_key(|(m, _)| norm(m) != home); // home (false) sorts first
    if let Some((mint, _)) = direct.first() {
        return PaymentPlan::Direct {
            mint_url: mint.clone(),
        };
    }

    // 2. Swap: a trusted accepted target + a source we hold that covers `amount`.
    let mut targets: Vec<&(String, bool)> =
        accepted.iter().filter(|(_, trusted)| *trusted).collect();
    targets.sort_by_key(|(m, _)| norm(m) != home);
    if let Some((to_mint, _)) = targets.first() {
        // Largest source we hold that isn't the target itself.
        let from = holdings
            .iter()
            .filter(|(m, b)| norm(m) != norm(to_mint) && *b >= amount)
            .max_by_key(|(_, b)| *b);
        if let Some((from_mint, _)) = from {
            return PaymentPlan::Swap {
                from_mint: from_mint.clone(),
                to_mint: to_mint.clone(),
            };
        }
    }

    PaymentPlan::Insufficient
}

/// Build a cashuA token to pay a seeder `amount_sats`, denominated in one of the
/// seeder's `accepted_mints`. Auto-swaps across mints (up to `max_fee_sats`) when
/// we don't already hold the right mint. Returns the token string ready to send.
///
/// Errors (caller should fall back to free origin) when no accepted mint is
/// reachable within balance, no trusted swap target exists, or the swap exceeds
/// the fee cap.
pub async fn build_payment_token(
    data_dir: &Path,
    accepted_mints: &[String],
    amount_sats: u64,
    max_fee_sats: u64,
) -> Result<String> {
    if amount_sats == 0 {
        anyhow::bail!("payment amount must be greater than zero");
    }
    if accepted_mints.is_empty() {
        anyhow::bail!("seeder advertised no accepted mints");
    }

    // Annotate each accepted mint with whether we trust swapping into it.
    let mut accepted: Vec<(String, bool)> = Vec::with_capacity(accepted_mints.len());
    for m in accepted_mints {
        let trusted = is_mint_trusted(data_dir, m).await?;
        accepted.push((m.clone(), trusted));
    }

    // Prefer swap targets with a liquidity track record. plan_payment's stable
    // sort keeps the home mint first; within the rest, this orders by how
    // reliably we've reached each target before (best routes first).
    let liq = load_swap_liquidity(data_dir).await;
    accepted.sort_by_key(|(m, _)| std::cmp::Reverse(target_liquidity_score(&liq, m)));

    let holdings = load_wallet(data_dir).await?.spendable_by_mint();

    match plan_payment(&holdings, &accepted, amount_sats) {
        PaymentPlan::Direct { mint_url } => {
            debug!(
                "Payment plan: direct from {} for {} sats",
                mint_url, amount_sats
            );
            send_token_at(data_dir, &mint_url, amount_sats).await
        }
        PaymentPlan::Swap { from_mint, to_mint } => {
            debug!(
                "Payment plan: swap {}→{} then pay {} sats (fee cap {})",
                from_mint, to_mint, amount_sats, max_fee_sats
            );
            swap_between_mints(data_dir, &from_mint, &to_mint, amount_sats, max_fee_sats).await?;
            send_token_at(data_dir, &to_mint, amount_sats).await
        }
        PaymentPlan::Insufficient => anyhow::bail!(
            "cannot pay {} sats: no accepted mint covers it within balance/trust",
            amount_sats
        ),
    }
}

// ── F2 step 3 — hardening: idempotent swap resume + liquidity cache ─────────

const PENDING_SWAPS_FILE: &str = "wallet/pending_swaps.json";
const SWAP_LIQUIDITY_FILE: &str = "wallet/swap_liquidity.json";

/// An in-flight cross-mint swap, journaled the moment the source proofs are
/// melted (paid) so a crash before the target claim can be resumed instead of
/// silently losing the value. Removed once the target tokens are claimed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingSwap {
    pub from_mint: String,
    pub to_mint: String,
    pub amount_sats: u64,
    /// Source-mint melt quote id (the leg already paid).
    pub melt_quote_id: String,
    /// Target-mint mint quote id (the leg to claim).
    pub mint_quote_id: String,
    pub created_at: String,
}

async fn load_pending_swaps(data_dir: &Path) -> Result<Vec<PendingSwap>> {
    let path = data_dir.join(PENDING_SWAPS_FILE);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path).await?;
    Ok(serde_json::from_str(&content).unwrap_or_default())
}

async fn save_pending_swaps(data_dir: &Path, swaps: &[PendingSwap]) -> Result<()> {
    let dir = data_dir.join("wallet");
    fs::create_dir_all(&dir).await?;
    let path = data_dir.join(PENDING_SWAPS_FILE);
    fs::write(&path, serde_json::to_string_pretty(swaps)?).await?;
    Ok(())
}

async fn add_pending_swap(data_dir: &Path, swap: PendingSwap) -> Result<()> {
    let mut all = load_pending_swaps(data_dir).await?;
    all.push(swap);
    save_pending_swaps(data_dir, &all).await
}

async fn remove_pending_swap(data_dir: &Path, mint_quote_id: &str) -> Result<()> {
    let mut all = load_pending_swaps(data_dir).await?;
    all.retain(|s| s.mint_quote_id != mint_quote_id);
    save_pending_swaps(data_dir, &all).await
}

/// Resume any swaps that were interrupted between paying the source mint and
/// claiming the target tokens. For each pending swap, ask the target mint about
/// the mint quote:
/// - `PAID`   → claim now (the value was paid but never claimed). Reclaimed.
/// - `ISSUED` → already claimed on a prior run; just drop the journal entry.
/// - else     → leave it (the invoice hasn't settled yet; retry next time).
///
/// Returns the total sats reclaimed. Safe to call repeatedly (idempotent): a
/// quote is only minted once, and `ISSUED` quotes are never re-claimed.
pub async fn resume_pending_swaps(data_dir: &Path) -> Result<u64> {
    let pending = load_pending_swaps(data_dir).await?;
    let mut reclaimed = 0u64;
    for swap in pending {
        let to = match MintClient::new(&swap.to_mint) {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    "resume_pending_swaps: bad target mint {}: {}",
                    swap.to_mint, e
                );
                continue;
            }
        };
        let status = match to.mint_quote_status(&swap.mint_quote_id).await {
            Ok(s) => s,
            Err(e) => {
                debug!(
                    "resume_pending_swaps: status check failed for {}: {} — leaving pending",
                    swap.mint_quote_id, e
                );
                continue;
            }
        };
        match status.state.as_str() {
            "PAID" => match to.mint_tokens(&swap.mint_quote_id, swap.amount_sats).await {
                Ok(result) => {
                    let minted: u64 = result.proofs.iter().map(|p| p.amount).sum();
                    let mut wallet = load_wallet(data_dir).await?;
                    wallet.add_proofs(&swap.to_mint, result.proofs);
                    wallet.record_tx(
                        TransactionType::Mint,
                        minted,
                        &format!(
                            "Resumed cross-mint swap {}→{}: claimed {} sats",
                            swap.from_mint, swap.to_mint, minted
                        ),
                        &swap.to_mint,
                        &swap.from_mint,
                    );
                    save_wallet(data_dir, &wallet).await?;
                    remove_pending_swap(data_dir, &swap.mint_quote_id).await?;
                    record_swap_success(data_dir, &swap.from_mint, &swap.to_mint).await;
                    reclaimed += minted;
                    info!(
                        "Resumed interrupted swap {}→{}: reclaimed {} sats",
                        swap.from_mint, swap.to_mint, minted
                    );
                }
                Err(e) => warn!(
                    "resume_pending_swaps: claim failed for {}: {}",
                    swap.mint_quote_id, e
                ),
            },
            "ISSUED" => {
                // Already claimed on a previous run — drop the journal entry.
                remove_pending_swap(data_dir, &swap.mint_quote_id).await?;
            }
            other => debug!(
                "resume_pending_swaps: quote {} state {} — leaving pending",
                swap.mint_quote_id, other
            ),
        }
    }
    Ok(reclaimed)
}

/// Success/failure counts for a single (from → to) swap route.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RouteStat {
    successes: u64,
    failures: u64,
}

/// Per-mint-pair liquidity cache: which swap routes have actually worked, so the
/// payer can prefer routes with a track record over ones that keep failing.
#[derive(Debug, Default, Serialize, Deserialize)]
struct SwapLiquidity {
    /// Keyed by `"<from>|<to>"` (normalized URLs).
    routes: std::collections::BTreeMap<String, RouteStat>,
}

fn route_key(from_mint: &str, to_mint: &str) -> String {
    format!(
        "{}|{}",
        from_mint.trim_end_matches('/'),
        to_mint.trim_end_matches('/')
    )
}

async fn load_swap_liquidity(data_dir: &Path) -> SwapLiquidity {
    let path = data_dir.join(SWAP_LIQUIDITY_FILE);
    match fs::read_to_string(&path).await {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => SwapLiquidity::default(),
    }
}

async fn save_swap_liquidity(data_dir: &Path, liq: &SwapLiquidity) {
    let dir = data_dir.join("wallet");
    let _ = fs::create_dir_all(&dir).await;
    if let Ok(content) = serde_json::to_string_pretty(liq) {
        let _ = fs::write(data_dir.join(SWAP_LIQUIDITY_FILE), content).await;
    }
}

/// Record that a swap route succeeded (best-effort; never fails the caller).
async fn record_swap_success(data_dir: &Path, from_mint: &str, to_mint: &str) {
    let mut liq = load_swap_liquidity(data_dir).await;
    liq.routes
        .entry(route_key(from_mint, to_mint))
        .or_default()
        .successes += 1;
    save_swap_liquidity(data_dir, &liq).await;
}

/// Record that a swap route failed (best-effort; never fails the caller).
async fn record_swap_failure(data_dir: &Path, from_mint: &str, to_mint: &str) {
    let mut liq = load_swap_liquidity(data_dir).await;
    liq.routes
        .entry(route_key(from_mint, to_mint))
        .or_default()
        .failures += 1;
    save_swap_liquidity(data_dir, &liq).await;
}

/// Liquidity score for reaching `to_mint` from any source: net successes across
/// all routes ending at this target. Higher = a more reliable destination.
fn target_liquidity_score(liq: &SwapLiquidity, to_mint: &str) -> i64 {
    let suffix = format!("|{}", to_mint.trim_end_matches('/'));
    liq.routes
        .iter()
        .filter(|(k, _)| k.ends_with(&suffix))
        .map(|(_, s)| s.successes as i64 - s.failures as i64)
        .sum()
}

/// Receive a cashuA token from a peer — swaps proofs at the mint for fresh ones.
pub async fn receive_token(data_dir: &Path, token_str: &str) -> Result<u64> {
    // Handle legacy format for backwards compatibility
    if token_str.starts_with("cashuSend_") {
        return receive_legacy_token(data_dir, token_str).await;
    }

    let token = CashuToken::deserialize(token_str)?;
    let total_amount = token.total_amount();

    if total_amount == 0 {
        anyhow::bail!("Token has zero value");
    }

    // Verify all mints in the token are accepted
    let accepted = load_accepted_mints(data_dir).await?;
    for mint_url in token.mint_urls() {
        if !accepted.mints.iter().any(|m| m == mint_url) {
            anyhow::bail!("Mint '{}' is not in accepted mints list", mint_url);
        }
    }

    let mut wallet = load_wallet(data_dir).await?;
    let mut received_total = 0u64;
    // MintClient translates the mint's NUT error code into plain language and
    // puts it at the top of the error chain (see `mint_error` in
    // mint_client.rs); `{}` surfaces that, `{:#}` keeps the raw status/body
    // for the log. Remember the last one so a total failure can tell the user
    // *why* instead of just "nothing was received".
    let mut last_reason: Option<String> = None;

    // Swap proofs at each mint
    for entry in &token.token {
        let client = MintClient::new(&entry.mint)?;
        match client.receive_token(&token).await {
            Ok(new_proofs) => {
                let amount: u64 = new_proofs.iter().map(|p| p.amount).sum();
                wallet.add_proofs(&entry.mint, new_proofs);
                received_total += amount;
            }
            Err(e) => {
                warn!("Failed to swap proofs from mint {}: {:#}", entry.mint, e);
                last_reason = Some(e.to_string());
                // Continue with other mints if any
            }
        }
    }

    if received_total == 0 {
        match last_reason {
            Some(reason) => anyhow::bail!("Could not receive this ecash: {}", reason),
            None => anyhow::bail!("Failed to receive any proofs from token"),
        }
    }

    wallet.record_tx(
        TransactionType::Receive,
        received_total,
        &format!("Received {} sats ecash", received_total),
        token.token.first().map(|e| e.mint.as_str()).unwrap_or(""),
        "",
    );
    save_wallet(data_dir, &wallet).await?;

    debug!("Received {} sats ecash token", received_total);
    Ok(received_total)
}

/// Receive a legacy format token (cashuSend_{amount}_{uuid}_{timestamp}).
/// For backwards compatibility during migration period.
async fn receive_legacy_token(data_dir: &Path, token_str: &str) -> Result<u64> {
    let amount_sats = token_str
        .split('_')
        .nth(1)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    if amount_sats == 0 {
        anyhow::bail!("Invalid legacy ecash token");
    }

    let mut wallet = load_wallet(data_dir).await?;

    // Store as a synthetic proof (legacy tokens can't be verified cryptographically)
    let proof = Proof {
        amount: amount_sats,
        id: "legacy".to_string(),
        secret: token_str.to_string(),
        c: "0000000000000000000000000000000000000000000000000000000000000000ff".to_string(),
    };
    wallet.add_proofs(&wallet.mint_url.clone(), vec![proof]);

    wallet.record_tx(
        TransactionType::Receive,
        amount_sats,
        &format!("Received {} sats (legacy format)", amount_sats),
        &wallet.mint_url.clone(),
        "",
    );
    save_wallet(data_dir, &wallet).await?;

    debug!("Received {} sats legacy ecash token", amount_sats);
    Ok(amount_sats)
}

/// Verify a payment token and receive it into the wallet.
/// Returns the verified amount, or error if verification fails.
/// Used by the content server and streaming gate to verify incoming payments.
pub async fn verify_and_receive_payment(
    data_dir: &Path,
    token_str: &str,
    required_sats: u64,
) -> Result<u64> {
    // Handle legacy tokens
    if token_str.starts_with("cashuSend_") {
        let amount = token_str
            .split('_')
            .nth(1)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        if amount < required_sats {
            anyhow::bail!(
                "Insufficient payment: {} sats, need {} sats",
                amount,
                required_sats
            );
        }
        let received = receive_legacy_token(data_dir, token_str).await?;
        return Ok(received);
    }

    // Fedimint notes (#3): a buyer whose balance is in Fedimint pays with notes
    // rather than a Cashu token. Cashu tokens all start with "cashu" (cashuA/
    // cashuB, or the legacy form handled above), so anything else is treated as
    // Fedimint notes and redeemed into a joined federation. reissue_into_any
    // verifies the notes are unspent and credits this node's wallet.
    if !token_str.starts_with("cashu") {
        let (received, _fed_id) =
            crate::wallet::fedimint_client::reissue_into_any(data_dir, token_str).await?;
        if received < required_sats {
            anyhow::bail!(
                "Insufficient payment: {} sats, need {} sats",
                received,
                required_sats
            );
        }
        return Ok(received);
    }

    // Parse and validate cashuA token
    let token = CashuToken::deserialize(token_str)?;
    let total = token.total_amount();

    if total < required_sats {
        anyhow::bail!(
            "Insufficient payment: {} sats, need {} sats",
            total,
            required_sats
        );
    }

    // Verify mints are accepted
    let accepted = load_accepted_mints(data_dir).await?;
    for mint_url in token.mint_urls() {
        if !accepted.mints.iter().any(|m| m == mint_url) {
            anyhow::bail!("Mint '{}' not accepted", mint_url);
        }
    }

    // Swap proofs at mint (this verifies they're unspent and gives us fresh proofs)
    let mut wallet = load_wallet(data_dir).await?;
    let mut received_total = 0u64;

    for entry in &token.token {
        let client = MintClient::new(&entry.mint)?;
        let entry_total: u64 = entry.proofs.iter().map(|p| p.amount).sum();
        let target_amounts = amount_to_denominations(entry_total);

        match client.swap(&entry.proofs, &target_amounts).await {
            Ok(result) => {
                let amount: u64 = result.new_proofs.iter().map(|p| p.amount).sum();
                wallet.add_proofs(&entry.mint, result.new_proofs);
                received_total += amount;
            }
            Err(e) => {
                warn!("Payment verification failed at mint {}: {}", entry.mint, e);
            }
        }
    }

    if received_total < required_sats {
        anyhow::bail!(
            "Payment verification failed: only {} of {} sats verified",
            received_total,
            required_sats
        );
    }

    wallet.record_tx(
        TransactionType::Receive,
        received_total,
        &format!("Payment received: {} sats", received_total),
        token.token.first().map(|e| e.mint.as_str()).unwrap_or(""),
        "",
    );
    save_wallet(data_dir, &wallet).await?;

    Ok(received_total)
}

/// Check the wallet balance.
pub async fn get_balance(data_dir: &Path) -> Result<u64> {
    let wallet = load_wallet(data_dir).await?;
    Ok(wallet.balance())
}

/// Default mint URL (local Fedimint).
/// Default Cashu mint. Minibits is a well-known public Cashu mint — note this
/// is a CASHU mint, distinct from the local Fedimint guardian (:8175), which is
/// a separate ecash protocol managed under the Fedimint Federations tab. The
/// old default pointed at :8175, which incorrectly surfaced the Fedimint URL in
/// the Cashu mints list.
fn default_mint_url() -> String {
    "https://mint.minibits.cash/Bitcoin".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_wallet_state_balance_empty() {
        let wallet = WalletState::default();
        assert_eq!(wallet.balance(), 0);
    }

    #[test]
    fn test_wallet_state_balance_with_proofs() {
        let mut wallet = WalletState::default();
        wallet.add_proofs(
            "http://mint",
            vec![
                Proof {
                    amount: 100,
                    id: "ks1".into(),
                    secret: "s1".into(),
                    c: "c1".into(),
                },
                Proof {
                    amount: 200,
                    id: "ks1".into(),
                    secret: "s2".into(),
                    c: "c2".into(),
                },
            ],
        );
        assert_eq!(wallet.balance(), 300);
    }

    #[test]
    fn test_wallet_state_balance_excludes_spent() {
        let mut wallet = WalletState::default();
        wallet.add_proofs(
            "http://mint",
            vec![
                Proof {
                    amount: 100,
                    id: "ks1".into(),
                    secret: "s1".into(),
                    c: "c1".into(),
                },
                Proof {
                    amount: 200,
                    id: "ks1".into(),
                    secret: "s2".into(),
                    c: "c2".into(),
                },
            ],
        );
        wallet.proofs[0].spent = true;
        assert_eq!(wallet.balance(), 200);
    }

    #[test]
    fn test_wallet_state_balance_excludes_reserved() {
        let mut wallet = WalletState::default();
        wallet.add_proofs(
            "http://mint",
            vec![Proof {
                amount: 100,
                id: "ks1".into(),
                secret: "s1".into(),
                c: "c1".into(),
            }],
        );
        wallet.proofs[0].reserved = true;
        assert_eq!(wallet.balance(), 0);
    }

    #[test]
    fn test_select_proofs_sufficient() {
        let mut wallet = WalletState::default();
        wallet.add_proofs(
            "http://mint",
            vec![
                Proof {
                    amount: 1,
                    id: "ks1".into(),
                    secret: "s1".into(),
                    c: "c1".into(),
                },
                Proof {
                    amount: 4,
                    id: "ks1".into(),
                    secret: "s2".into(),
                    c: "c2".into(),
                },
                Proof {
                    amount: 8,
                    id: "ks1".into(),
                    secret: "s3".into(),
                    c: "c3".into(),
                },
            ],
        );

        let (indices, overpayment) = wallet.select_proofs("http://mint", 5).unwrap();
        let total: u64 = indices.iter().map(|&i| wallet.proofs[i].proof.amount).sum();
        assert!(total >= 5);
        assert_eq!(total - 5, overpayment);
    }

    #[test]
    fn test_select_proofs_insufficient() {
        let mut wallet = WalletState::default();
        wallet.add_proofs(
            "http://mint",
            vec![Proof {
                amount: 1,
                id: "ks1".into(),
                secret: "s1".into(),
                c: "c1".into(),
            }],
        );

        assert!(wallet.select_proofs("http://mint", 100).is_none());
    }

    #[test]
    fn test_select_proofs_wrong_mint() {
        let mut wallet = WalletState::default();
        wallet.add_proofs(
            "http://mint-a",
            vec![Proof {
                amount: 100,
                id: "ks1".into(),
                secret: "s1".into(),
                c: "c1".into(),
            }],
        );

        assert!(wallet.select_proofs("http://mint-b", 100).is_none());
    }

    #[test]
    fn test_balance_for_mint() {
        let mut wallet = WalletState::default();
        wallet.add_proofs(
            "http://mint-a",
            vec![Proof {
                amount: 100,
                id: "ks1".into(),
                secret: "s1".into(),
                c: "c1".into(),
            }],
        );
        wallet.add_proofs(
            "http://mint-b",
            vec![Proof {
                amount: 200,
                id: "ks2".into(),
                secret: "s2".into(),
                c: "c2".into(),
            }],
        );

        assert_eq!(wallet.balance_for_mint("http://mint-a"), 100);
        assert_eq!(wallet.balance_for_mint("http://mint-b"), 200);
        assert_eq!(wallet.balance(), 300);
    }

    #[tokio::test]
    async fn test_load_wallet_creates_default_when_missing() {
        let tmp = TempDir::new().unwrap();
        let wallet = load_wallet(tmp.path()).await.unwrap();
        assert_eq!(wallet.balance(), 0);
        assert!(wallet.proofs.is_empty());
        assert!(wallet.transactions.is_empty());
        assert_eq!(wallet.mint_url, default_mint_url());
    }

    #[tokio::test]
    async fn test_save_and_load_wallet_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut wallet = WalletState {
            mint_url: "https://mint.minibits.cash/Bitcoin".into(),
            ..Default::default()
        };
        wallet.add_proofs(
            "https://mint.minibits.cash/Bitcoin",
            vec![Proof {
                amount: 42,
                id: "ks1".into(),
                secret: "test_secret".into(),
                c: "test_c".into(),
            }],
        );
        wallet.record_tx(
            TransactionType::Mint,
            42,
            "Test mint",
            "https://mint.minibits.cash/Bitcoin",
            "",
        );

        save_wallet(tmp.path(), &wallet).await.unwrap();
        let loaded = load_wallet(tmp.path()).await.unwrap();
        assert_eq!(loaded.proofs.len(), 1);
        assert_eq!(loaded.proofs[0].proof.amount, 42);
        assert!(!loaded.proofs[0].spent);
        assert_eq!(loaded.transactions.len(), 1);
        assert_eq!(loaded.balance(), 42);
    }

    #[tokio::test]
    async fn test_save_wallet_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let wallet_dir = tmp.path().join("wallet");
        assert!(!wallet_dir.exists());

        let wallet = WalletState::default();
        save_wallet(tmp.path(), &wallet).await.unwrap();
        assert!(wallet_dir.exists());
    }

    #[tokio::test]
    async fn test_receive_legacy_token() {
        let tmp = TempDir::new().unwrap();
        let amount = receive_token(tmp.path(), "cashuSend_500_abc123_1700000000")
            .await
            .unwrap();
        assert_eq!(amount, 500);

        let wallet = load_wallet(tmp.path()).await.unwrap();
        assert_eq!(wallet.balance(), 500);
        assert_eq!(wallet.proofs.len(), 1);
        assert!(!wallet.proofs[0].spent);
    }

    #[tokio::test]
    async fn test_receive_token_invalid_format() {
        let tmp = TempDir::new().unwrap();
        let result = receive_token(tmp.path(), "invalid_token_string").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_receive_legacy_token_zero_amount() {
        let tmp = TempDir::new().unwrap();
        let result = receive_token(tmp.path(), "cashuSend_0_abc_1700000000").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_prune_old_spent() {
        let mut wallet = WalletState::default();
        // Add an old spent proof
        wallet.proofs.push(StoredProof {
            proof: Proof {
                amount: 100,
                id: "ks1".into(),
                secret: "old".into(),
                c: "c".into(),
            },
            mint_url: "http://mint".into(),
            spent: true,
            reserved: false,
            created_at: "2020-01-01T00:00:00Z".into(),
        });
        // Add a recent unspent proof
        wallet.proofs.push(StoredProof {
            proof: Proof {
                amount: 200,
                id: "ks1".into(),
                secret: "new".into(),
                c: "c".into(),
            },
            mint_url: "http://mint".into(),
            spent: false,
            reserved: false,
            created_at: chrono::Utc::now().to_rfc3339(),
        });

        wallet.prune_old_spent();
        assert_eq!(wallet.proofs.len(), 1);
        assert_eq!(wallet.proofs[0].proof.amount, 200);
    }

    #[tokio::test]
    async fn test_accepted_mints_default() {
        let tmp = TempDir::new().unwrap();
        let mints = load_accepted_mints(tmp.path()).await.unwrap();
        assert_eq!(mints.mints.len(), 1);
        assert_eq!(mints.mints[0], default_mint_url());
    }

    #[tokio::test]
    async fn test_accepted_mints_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mints = AcceptedMints {
            mints: vec!["http://mint-a".into(), "http://mint-b".into()],
        };
        save_accepted_mints(tmp.path(), &mints).await.unwrap();
        let loaded = load_accepted_mints(tmp.path()).await.unwrap();
        assert_eq!(loaded.mints.len(), 2);
    }

    #[test]
    fn test_transaction_type_serialization() {
        let tx = EcashTransaction {
            id: "tx-test".into(),
            tx_type: TransactionType::StreamingPayment,
            amount_sats: 100,
            timestamp: "2025-01-01T00:00:00Z".into(),
            description: "test".into(),
            mint_url: String::new(),
            peer: String::new(),
            kind: default_tx_kind(),
        };
        let json = serde_json::to_string(&tx).unwrap();
        assert!(json.contains("\"streamingpayment\""));
    }

    #[test]
    fn test_default_mint_url() {
        assert_eq!(default_mint_url(), "https://mint.minibits.cash/Bitcoin");
    }

    #[test]
    fn test_swap_fee() {
        assert_eq!(swap_fee(105, 100), 5);
        // Defensive: never underflow if mint quotes oddly.
        assert_eq!(swap_fee(100, 100), 0);
        assert_eq!(swap_fee(90, 100), 0);
    }

    #[tokio::test]
    async fn test_is_mint_trusted_home_always() {
        let tmp = TempDir::new().unwrap();
        // Home mint is trusted even with no accepted-mints file.
        assert!(is_mint_trusted(tmp.path(), &default_mint_url())
            .await
            .unwrap());
        // Trailing slash on the home URL still matches.
        assert!(
            is_mint_trusted(tmp.path(), "https://mint.minibits.cash/Bitcoin/")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_is_mint_trusted_respects_accepted_list() {
        let tmp = TempDir::new().unwrap();
        save_accepted_mints(
            tmp.path(),
            &AcceptedMints {
                mints: vec![default_mint_url(), "https://mint.example.com".into()],
            },
        )
        .await
        .unwrap();

        assert!(is_mint_trusted(tmp.path(), "https://mint.example.com")
            .await
            .unwrap());
        // Normalized comparison ignores a trailing slash.
        assert!(is_mint_trusted(tmp.path(), "https://mint.example.com/")
            .await
            .unwrap());
        // A mint not on the list is not trusted.
        assert!(!is_mint_trusted(tmp.path(), "https://evil.example.com")
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn test_swap_between_mints_rejects_identical() {
        let tmp = TempDir::new().unwrap();
        let err = swap_between_mints(
            tmp.path(),
            &default_mint_url(),
            "https://mint.minibits.cash/Bitcoin/",
            100,
            10,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("identical"));
    }

    #[tokio::test]
    async fn test_swap_between_mints_rejects_untrusted_target() {
        let tmp = TempDir::new().unwrap();
        let err = swap_between_mints(
            tmp.path(),
            &default_mint_url(),
            "https://untrusted.example.com",
            100,
            10,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("trusted"));
    }

    #[tokio::test]
    async fn test_swap_between_mints_rejects_zero_amount() {
        let tmp = TempDir::new().unwrap();
        let err = swap_between_mints(
            tmp.path(),
            &default_mint_url(),
            "https://mint.example.com",
            0,
            10,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("greater than zero"));
    }

    #[test]
    fn test_spendable_by_mint_groups_and_excludes() {
        let mut wallet = WalletState::default();
        wallet.add_proofs(
            "http://mint-a",
            vec![
                Proof {
                    amount: 10,
                    id: "k".into(),
                    secret: "s1".into(),
                    c: "c".into(),
                },
                Proof {
                    amount: 5,
                    id: "k".into(),
                    secret: "s2".into(),
                    c: "c".into(),
                },
            ],
        );
        wallet.add_proofs(
            "http://mint-b",
            vec![Proof {
                amount: 7,
                id: "k".into(),
                secret: "s3".into(),
                c: "c".into(),
            }],
        );
        wallet.proofs[1].spent = true; // exclude the 5 on mint-a
        let by_mint = wallet.spendable_by_mint();
        assert_eq!(
            by_mint,
            vec![
                ("http://mint-a".to_string(), 10),
                ("http://mint-b".to_string(), 7)
            ]
        );
    }

    #[test]
    fn test_plan_payment_direct_prefers_home() {
        let home = default_mint_url();
        let holdings = vec![(home.clone(), 100), ("https://other".into(), 100)];
        // Both accepted; home should win the tie-break.
        let accepted = vec![("https://other".into(), true), (home.clone(), true)];
        assert_eq!(
            plan_payment(&holdings, &accepted, 50),
            PaymentPlan::Direct { mint_url: home }
        );
    }

    #[test]
    fn test_plan_payment_direct_only_accepted_mint() {
        let holdings = vec![("https://a".into(), 100), ("https://b".into(), 100)];
        // We hold both, but the seeder only accepts b.
        let accepted = vec![("https://b".into(), true)];
        assert_eq!(
            plan_payment(&holdings, &accepted, 50),
            PaymentPlan::Direct {
                mint_url: "https://b".into()
            }
        );
    }

    #[test]
    fn test_plan_payment_swaps_into_trusted_target() {
        // We hold value on A; seeder accepts only B (trusted) which we don't hold.
        let holdings = vec![("https://a".into(), 100)];
        let accepted = vec![("https://b".into(), true)];
        assert_eq!(
            plan_payment(&holdings, &accepted, 50),
            PaymentPlan::Swap {
                from_mint: "https://a".into(),
                to_mint: "https://b".into()
            }
        );
    }

    #[test]
    fn test_plan_payment_refuses_untrusted_swap_target() {
        // Seeder accepts only B, but B is not trusted → no swap, insufficient.
        let holdings = vec![("https://a".into(), 100)];
        let accepted = vec![("https://b".into(), false)];
        assert_eq!(
            plan_payment(&holdings, &accepted, 50),
            PaymentPlan::Insufficient
        );
    }

    #[test]
    fn test_plan_payment_insufficient_when_no_single_source_covers() {
        // Total 60 across two mints, but neither alone covers 50+ for a swap and
        // we hold neither accepted mint directly.
        let holdings = vec![("https://a".into(), 30), ("https://c".into(), 30)];
        let accepted = vec![("https://b".into(), true)];
        assert_eq!(
            plan_payment(&holdings, &accepted, 50),
            PaymentPlan::Insufficient
        );
    }

    #[test]
    fn test_plan_payment_direct_beats_swap() {
        // We hold the accepted mint directly AND could swap — Direct must win.
        let home = default_mint_url();
        let holdings = vec![("https://b".into(), 100), (home.clone(), 100)];
        let accepted = vec![("https://b".into(), true)];
        assert_eq!(
            plan_payment(&holdings, &accepted, 50),
            PaymentPlan::Direct {
                mint_url: "https://b".into()
            }
        );
    }

    #[tokio::test]
    async fn test_build_payment_token_rejects_empty_mints() {
        let tmp = TempDir::new().unwrap();
        let err = build_payment_token(tmp.path(), &[], 100, 10)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("no accepted mints"));
    }

    #[tokio::test]
    async fn test_build_payment_token_insufficient_falls_through() {
        let tmp = TempDir::new().unwrap();
        // Empty wallet, untrusted seeder mint → cannot pay (caller uses origin).
        let err = build_payment_token(tmp.path(), &["https://seeder.example.com".into()], 100, 10)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("cannot pay"));
    }

    #[test]
    fn test_route_key_normalizes_trailing_slash() {
        assert_eq!(route_key("https://a/", "https://b/"), "https://a|https://b");
        assert_eq!(route_key("https://a", "https://b"), "https://a|https://b");
    }

    #[tokio::test]
    async fn test_pending_swaps_roundtrip_and_remove() {
        let tmp = TempDir::new().unwrap();
        assert!(load_pending_swaps(tmp.path()).await.unwrap().is_empty());

        add_pending_swap(
            tmp.path(),
            PendingSwap {
                from_mint: "https://a".into(),
                to_mint: "https://b".into(),
                amount_sats: 100,
                melt_quote_id: "melt-1".into(),
                mint_quote_id: "mint-1".into(),
                created_at: "2026-06-17T00:00:00Z".into(),
            },
        )
        .await
        .unwrap();
        let loaded = load_pending_swaps(tmp.path()).await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].mint_quote_id, "mint-1");

        remove_pending_swap(tmp.path(), "mint-1").await.unwrap();
        assert!(load_pending_swaps(tmp.path()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_resume_pending_swaps_empty_is_noop() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(resume_pending_swaps(tmp.path()).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_liquidity_cache_records_and_scores() {
        let tmp = TempDir::new().unwrap();
        // Two routes into B succeed; one into C fails.
        record_swap_success(tmp.path(), "https://a", "https://b").await;
        record_swap_success(tmp.path(), "https://x", "https://b").await;
        record_swap_failure(tmp.path(), "https://a", "https://c").await;

        let liq = load_swap_liquidity(tmp.path()).await;
        // B reached successfully from two sources → net +2; trailing slash tolerant.
        assert_eq!(target_liquidity_score(&liq, "https://b/"), 2);
        // C only failed → net -1.
        assert_eq!(target_liquidity_score(&liq, "https://c"), -1);
        // Unknown target → neutral 0.
        assert_eq!(target_liquidity_score(&liq, "https://unknown"), 0);
    }

    #[tokio::test]
    async fn test_build_payment_token_prefers_liquid_target() {
        let tmp = TempDir::new().unwrap();
        // Trust two non-home mints; hold value on a source mint for both.
        save_accepted_mints(
            tmp.path(),
            &AcceptedMints {
                mints: vec![
                    default_mint_url(),
                    "https://liquid".into(),
                    "https://dry".into(),
                ],
            },
        )
        .await
        .unwrap();
        // Give "https://liquid" a track record so it should be preferred.
        record_swap_success(tmp.path(), "https://src", "https://liquid").await;

        // Seeder accepts both non-home mints; we only hold "https://src".
        let accepted = vec![
            ("https://dry".into(), true),
            ("https://liquid".into(), true),
        ];
        let holdings = vec![("https://src".to_string(), 1000u64)];

        // Mirror build_payment_token's ordering step, then plan.
        let liq = load_swap_liquidity(tmp.path()).await;
        let mut ordered = accepted.clone();
        ordered.sort_by_key(|(m, _): &(String, bool)| {
            std::cmp::Reverse(target_liquidity_score(&liq, m))
        });
        match plan_payment(&holdings, &ordered, 100) {
            PaymentPlan::Swap { to_mint, .. } => assert_eq!(to_mint, "https://liquid"),
            other => panic!("expected swap into liquid target, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn ecash_network_defaults_to_mainnet_and_leaves_files_alone() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        assert_eq!(load_network(dir).await, EcashNetwork::Mainnet);
        // A node that never touches this setting has no new file.
        assert!(!dir.join(NETWORK_FILE).exists());
        assert_eq!(
            load_wallet(dir).await.unwrap().mint_url,
            default_mint_url(),
            "mainnet must keep the original default mint"
        );
    }

    #[tokio::test]
    async fn switching_to_testnet_uses_a_separate_purse_and_test_mint() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // Put real coins in the mainnet wallet.
        let mut real = load_wallet(dir).await.unwrap();
        real.proofs.push(StoredProof {
            proof: Proof {
                amount: 1000,
                id: "009a1f293253e41e".into(),
                secret: "real".into(),
                c: "02".into(),
            },
            mint_url: default_mint_url(),
            spent: false,
            reserved: false,
            created_at: "2026-01-01T00:00:00Z".into(),
        });
        save_wallet(dir, &real).await.unwrap();
        assert_eq!(load_wallet(dir).await.unwrap().balance(), 1000);

        // Switching to testnet must show an EMPTY purse pointed at the test
        // mint — never the real coins.
        save_network(dir, EcashNetwork::Testnet).await.unwrap();
        let test_wallet = load_wallet(dir).await.unwrap();
        assert_eq!(test_wallet.balance(), 0, "test wallet must not see real coins");
        assert!(test_wallet.mint_url.contains("testnut"));
        assert!(load_accepted_mints(dir).await.unwrap().mints[0].contains("testnut"));

        // Test coins are written to their own file...
        let mut t = test_wallet;
        t.proofs.push(StoredProof {
            proof: Proof {
                amount: 7,
                id: "009a1f293253e41e".into(),
                secret: "test".into(),
                c: "02".into(),
            },
            mint_url: EcashNetwork::Testnet.default_mint(),
            spent: false,
            reserved: false,
            created_at: "2026-01-01T00:00:00Z".into(),
        });
        save_wallet(dir, &t).await.unwrap();
        assert!(dir.join("wallet/ecash.testnet.json").exists());

        // ...and switching back finds the real balance exactly as it was.
        save_network(dir, EcashNetwork::Mainnet).await.unwrap();
        let back = load_wallet(dir).await.unwrap();
        assert_eq!(back.balance(), 1000, "real funds must survive a round trip");
        assert_eq!(back.proofs.len(), 1);
        assert_eq!(back.proofs[0].proof.secret, "real");
    }
}
