//! The Minibits `@minibits.cash` Lightning address (LUD-16) for the node's
//! ecash wallet.
//!
//! ## Why this exists next to the mint client
//!
//! The ecash wallet already talks to `mint.minibits.cash` as a plain Cashu
//! mint (`wallet::mint_client`): mint/melt quotes, swap, receive. That gives the
//! node ecash *from* Minibits, but not a *name* at Minibits. A human-readable
//! Lightning address like `braveharbor42@minibits.cash` is a separate service —
//! the Minibits profile API at `api.minibits.cash/v3` — and it is what lets any
//! Lightning wallet pay this node by typing an address, with the payment landing
//! as ecash.
//!
//! ## Identity: the ecash wallet *is* the Minibits wallet
//!
//! Minibits ties an address to a wallet by `seedHash`, and authenticates the
//! wallet with a NIP-06 Nostr keypair. Both come from the *existing* NUT-13
//! ecash phrase (`wallet::nut13`), so there is no second secret to back up:
//!
//! - `seedHash = sha256(mnemonic.to_seed(""))` — the exact bytes the Minibits
//!   app hashes, so restoring the same phrase in the Minibits app recovers the
//!   same address (and vice-versa).
//! - Nostr keys via NIP-06 at `m/44'/1237'/0'/0/0`. `nostr_sdk::Keys::from_mnemonic`
//!   uses that path with an empty BIP-39 passphrase — byte-for-byte the derivation
//!   the Minibits app (nostr-tools `accountFromSeedWords`) does, verified against
//!   the crate's own NIP-06 test vector.
//!
//! ## Flow (all verified against the live v3 API)
//!
//! 1. `POST /auth/challenge {pubkey}` → `{challenge, createdAt}`.
//! 2. Sign a NIP-42 kind-22242 event (`relay` + `challenge` tags, server's
//!    `createdAt`) with the Nostr key.
//! 3. `POST /auth/verify {pubkey, challenge, signature}` → JWT access token.
//! 4. `POST /profile {walletId, seedHash}` → the assigned `lud16`/`nip05`.
//!    Idempotent per pubkey: re-registering returns the existing address.
//! 5. `POST /claim {seedHash}` → NIP-04-encrypted Cashu tokens for Lightning
//!    payments sent to the address; decrypt with the Nostr key + the server's
//!    Nostr pubkey, then redeem through `ecash::receive_token`.
//!
//! Only runs on the mainnet ecash network — Minibits is a mainnet service, and a
//! testnet node must not register a profile or hit the production API.
//!
//! ## A claim can't be re-fetched — so nothing gets dropped
//!
//! `/claim` consumes a payment server-side the instant it's returned. A local
//! failure after that point (mint briefly unreachable, a stale cached server
//! key, a crash mid-loop) must not silently lose the coins, so every fetched
//! token is persisted to `MinibitsState::pending_claims` *before* decrypt/
//! redeem is attempted, and stays there — retried on every later poll — until
//! it succeeds. `ClaimOutcome::failed_count` reports how many are still
//! stuck so the caller can surface it instead of it being a log-only event.
//! Separately, `ensure_mint_accepted` keeps the Minibits mint on the node's
//! accepted-mints allow-list: the address is inherently backed by that one
//! mint, so an operator-edited allow-list must never be able to cause this
//! same kind of loss via `receive_token`'s mint check.

use super::ecash::{self, EcashNetwork};
use super::nut13;
use anyhow::{anyhow, Context, Result};
use base64::Engine;
use nostr_sdk::nips::{nip04, nip06::FromMnemonic};
use nostr_sdk::{Client, EventBuilder, Filter, Kind, RelayUrl, Tag, TagKind, Timestamp, ToBech32};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::fs;
use tracing::{debug, info, warn};

// Address registration, claim fetching and the state/wallet updates they
// trigger are one transaction from this module's point of view. Multiple
// dashboard tabs can call the RPC concurrently, while a relay fetch normally
// lasts longer than the UI's poll interval; serialise them so two polls cannot
// consume the same claim and race each other's state file writes.
static MINIBITS_STATE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
// A Companion WebView, its external browser tab, and a desktop dashboard can
// all watch the same address. Once one caller has completed the expensive
// relay fetch, callers already queued behind it should return the durable
// receipt immediately instead of each opening another relay subscription.
static LAST_MINIBITS_POLL_COMPLETED_AT: AtomicU64 = AtomicU64::new(0);
const MINIBITS_POLL_COALESCE_SECS: u64 = 2;

/// Minibits profile/LNURL API. Confirmed live: `/v3/auth/challenge`,
/// `/v3/profile`, `/v3/claim` (the older `/v2` host no longer serves profiles).
const API_BASE: &str = "https://api.minibits.cash/v3";
/// The relay named in the NIP-42 auth event. Matches the value the Minibits app
/// sends and the relay the service publishes in its NIP-05 record.
const RELAY_URL: &str = "wss://relay.minibits.cash";
/// Relays to check for incoming claim DMs (see `fetch_relay_dms`). Confirmed
/// live 2026-09-08: real Lightning payments to a `@minibits.cash` address are
/// delivered as a NIP-04 DM (kind 4, `#p`-tagged to the wallet's pubkey)
/// published to relays — *not* surfaced by `POST /claim`, which was the only
/// source this module fetched from until that gap stranded three real
/// payments. `RELAY_URL` first (it's the service's own relay and the one
/// most likely to have them), plus two large public relays as a fallback in
/// case that relay is ever unreachable or Minibits adds others.
const CLAIM_RELAY_URLS: &[&str] = &[RELAY_URL, "wss://relay.damus.io", "wss://nos.lol"];
/// NIP-42 client authentication event kind.
const AUTH_KIND: u16 = 22242;
/// The Minibits service Nostr pubkey that NIP-04-encrypts claimed tokens. Used
/// only as a fallback: the authoritative value is read from the address's own
/// LUD-16 metadata (`nostrPubkey`) at claim time, so a Minibits key rotation
/// does not strand claims.
const FALLBACK_SERVER_NOSTR_PUBKEY: &str =
    "beeb48407a6f087ea8f76dc384a5d88c67ced9bd9fb0cdba90930210df3d92e7";
/// Re-authenticate this long before the JWT actually expires, so a claim poll
/// never races the expiry boundary.
const TOKEN_EXPIRY_SKEP_SECS: i64 = 120;

const STATE_FILE: &str = "wallet/minibits.json";

/// Small word lists for the generated address name. Uniqueness comes from the
/// numeric suffix plus the retry-on-collision below — the Minibits server rejects
/// a name already taken by another wallet and we simply draw another, so these do
/// not need to be exhaustive (the Minibits app ships lists hundreds long).
const ADJECTIVES: &[&str] = &[
    "calm", "brave", "quiet", "solar", "rapid", "noble", "lunar", "vivid", "amber", "crisp",
    "eager", "fancy", "gentle", "happy", "jolly", "keen", "lucky", "mellow", "nimble", "proud",
    "quick", "rusty", "sunny", "tidy", "urban", "vital", "warm", "zesty", "bold", "clever",
    "daring", "epic", "fiery", "grand", "humble", "iron", "merry", "polar", "sleek", "wild",
];
const NOUNS: &[&str] = &[
    "harbor", "meadow", "canyon", "summit", "river", "forest", "island", "comet", "nebula",
    "orbit", "quartz", "maple", "willow", "falcon", "otter", "badger", "salmon", "crane", "ridge",
    "creek", "glade", "grove", "prairie", "delta", "cobalt", "onyx", "topaz", "ember", "anchor",
    "lantern", "beacon", "cabin", "drift", "signal", "thunder", "zephyr", "marble", "pebble",
    "sequoia", "tundra",
];

/// Persistent state for the node's Minibits address.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MinibitsState {
    /// The chosen wallet name (the `name` in `name@minibits.cash`).
    pub wallet_id: String,
    /// The full LUD-16 Lightning address, e.g. `braveharbor42@minibits.cash`.
    pub lud16: String,
    /// NIP-05 address (Minibits sets this equal to `lud16`).
    pub nip05: String,
    /// This node's NIP-06 Nostr pubkey (hex) the profile is bound to.
    pub nostr_pubkey: String,
    /// `sha256(seed)` — the wallet identifier Minibits keys claims on.
    pub seed_hash: String,
    /// Cached JWT access token.
    #[serde(default)]
    pub access_token: String,
    /// Access-token expiry (unix seconds); 0 when unknown/expired.
    #[serde(default)]
    pub access_expires: i64,
    /// Server Nostr pubkey used to decrypt claims, discovered from LUD-16.
    #[serde(default)]
    pub server_nostr_pubkey: String,
    #[serde(default)]
    pub created_at: String,
    /// Raw NIP-04-encrypted claim tokens fetched from `/claim` but not yet
    /// successfully redeemed. A claim is consumed server-side the instant
    /// `/claim` returns it, so it is stashed here *before* decrypt/redeem is
    /// attempted — a local failure (mint briefly down, bad cached server key,
    /// process crash mid-loop) then retries next poll instead of losing the
    /// coins outright.
    #[serde(default)]
    pub pending_claims: Vec<PendingClaim>,
    /// Unix timestamp of the newest Nostr DM we've already pulled into
    /// `pending_claims` (see `fetch_relay_dms`). Nostr events never expire
    /// from relays, so without this watermark every poll would re-fetch and
    /// re-attempt every claim ever sent — harmless (the mint rejects an
    /// already-spent token) but wasteful and noisy.
    #[serde(default)]
    pub last_dm_seen_at: u64,
    /// Event ids already queued from the relay. `created_at` has only
    /// one-second resolution, so a strict `since = last + 1` watermark can
    /// permanently miss a second payment published later in the same second.
    /// We query the boundary second inclusively and deduplicate by event id.
    #[serde(default)]
    pub seen_dm_ids: Vec<String>,
    /// Monotonic id for the latest successfully redeemed claim batch. Claim
    /// polling may come from several browser/Companion contexts; keeping the
    /// latest receipt here lets every caller observe the result instead of
    /// only whichever request happened to acquire the claim lock first.
    #[serde(default)]
    pub last_receipt_id: u64,
    #[serde(default)]
    pub last_receipt_sats: u64,
    #[serde(default)]
    pub last_receipt_at: u64,
}

/// A retryable encrypted token and the server key that encrypted it. The
/// legacy string form is accepted for state written by the original PR; new
/// entries retain their author so a server-key rotation does not make an older
/// pending claim undecryptable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PendingClaim {
    Attributed {
        content: String,
        sender_pubkey: String,
    },
    Legacy(String),
}

impl PendingClaim {
    fn content(&self) -> &str {
        match self {
            Self::Attributed { content, .. } | Self::Legacy(content) => content,
        }
    }

    fn sender_pubkey<'a>(&'a self, fallback: &'a str) -> &'a str {
        match self {
            Self::Attributed { sender_pubkey, .. } => sender_pubkey,
            Self::Legacy(_) => fallback,
        }
    }
}

/// A fresh Nostr keypair + seedHash derived from the node's ecash phrase.
struct MinibitsIdentity {
    keys: nostr_sdk::Keys,
    seed_hash: String,
}

/// Derive the Minibits identity (NIP-06 Nostr keys + seedHash) from the node's
/// ecash mnemonic. Both are deterministic, so the address and claims are
/// recoverable from the same 24 words the ecash already lives on.
fn derive_identity(phrase: &str, seed: &[u8; 64]) -> Result<MinibitsIdentity> {
    let keys = nostr_sdk::Keys::from_mnemonic(phrase, None::<&str>)
        .map_err(|e| anyhow!("NIP-06 derivation failed: {e}"))?;
    let seed_hash = hex::encode(Sha256::digest(seed));
    Ok(MinibitsIdentity { keys, seed_hash })
}

fn state_matches_identity(state: &MinibitsState, identity: &MinibitsIdentity) -> bool {
    state.seed_hash == identity.seed_hash
        && state.nostr_pubkey == identity.keys.public_key().to_hex()
}

/// A Minibits profile record — the fields we read off every profile response.
#[derive(Debug, Deserialize)]
struct ProfileRecord {
    #[serde(rename = "walletId")]
    wallet_id: String,
    #[serde(default)]
    nip05: String,
    #[serde(default)]
    lud16: Option<String>,
}

/// Turn a non-2xx Minibits response into a readable error, surfacing the
/// server's `error.name`/`error.message` when present.
fn minibits_error(status: reqwest::StatusCode, body: &str) -> anyhow::Error {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(err) = v.get("error") {
            let name = err.get("name").and_then(|n| n.as_str()).unwrap_or("ERROR");
            let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("");
            return anyhow!("Minibits API error {status}: {name} {msg}");
        }
    }
    let excerpt: String = body.chars().take(180).collect();
    anyhow!("Minibits API error {status}: {excerpt}")
}

fn state_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(STATE_FILE)
}

async fn archive_state(data_dir: &Path, reason: &str) -> Result<Option<std::path::PathBuf>> {
    let path = state_path(data_dir);
    if !path.exists() {
        return Ok(None);
    }
    let stamp = chrono::Utc::now().timestamp_millis();
    let archived = path.with_file_name(format!("minibits.recovery-{stamp}.json"));
    fs::rename(&path, &archived).await.with_context(|| {
        format!(
            "Failed to preserve {} as {}",
            path.display(),
            archived.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&archived, std::fs::Permissions::from_mode(0o600))
            .await
            .with_context(|| format!("Failed to protect {}", archived.display()))?;
    }
    warn!(
        "Minibits: preserved prior state at {} before recovery ({reason})",
        archived.display()
    );
    Ok(Some(archived))
}

async fn load_state(data_dir: &Path) -> Result<Option<MinibitsState>> {
    let path = state_path(data_dir);
    match fs::read_to_string(&path).await {
        Ok(s) if s.trim().is_empty() => Ok(None),
        Ok(s) => match serde_json::from_str::<MinibitsState>(&s) {
            Ok(st) if st.wallet_id.is_empty() => Ok(None),
            Ok(st) => Ok(Some(st)),
            // Unlike the accepted-mints file, nothing here is a user-editable
            // security setting — it's a pure mirror of state Minibits already
            // holds server-side, and registration is idempotent per pubkey
            // (§ module docs), so re-registering after a corrupt/truncated
            // read always recovers the *same* address. A node whose disk
            // filled up mid-write (observed on archy-x250-pa3, 2026-09-08:
            // this file truncated to 0 bytes) must self-heal on the next open
            // rather than permanently show "Lightning address unavailable".
            Err(e) => {
                warn!(
                    "Minibits: {} is corrupt/unreadable ({e}); treating as no profile yet and re-registering",
                    path.display()
                );
                archive_state(data_dir, "state file was not valid JSON").await?;
                Ok(None)
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("Failed to read {}", path.display())),
    }
}

/// Write the state file 0600 — it holds a bearer JWT. Same sensitivity class as
/// the ecash files it sits beside, so it gets the same owner-only mode.
async fn save_state(data_dir: &Path, state: &MinibitsState) -> Result<()> {
    let path = state_path(data_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .context("Failed to create the wallet directory")?;
    }
    let content =
        serde_json::to_string_pretty(state).context("Failed to serialize the Minibits profile")?;

    // This file contains both a bearer token and already-consumed claims.
    // Truncating it in place can lose retryable value on a crash or full disk.
    // Write and fsync a 0600 sibling, then atomically rename it over the state.
    let tmp = path.with_extension("json.tmp");
    let mut options = fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
    }
    let mut file = options
        .open(&tmp)
        .await
        .with_context(|| format!("Failed to create {}", tmp.display()))?;
    use tokio::io::AsyncWriteExt;
    file.write_all(content.as_bytes())
        .await
        .with_context(|| format!("Failed to write {}", tmp.display()))?;
    file.sync_all()
        .await
        .with_context(|| format!("Failed to flush {}", tmp.display()))?;
    drop(file);
    fs::rename(&tmp, &path)
        .await
        .with_context(|| format!("Failed to replace {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .await
            .with_context(|| format!("Failed to chmod 0600 {}", path.display()))?;
    }
    Ok(())
}

/// Read the `exp` claim (unix seconds) from a JWT without verifying it — the
/// token comes straight from Minibits over TLS; we only use the expiry to decide
/// when to refresh.
fn jwt_expiry(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("exp")?.as_i64()
}

/// Run the NIP-42 challenge/verify dance and return the access token plus its
/// expiry. Idempotent and cheap enough to redo whenever the cached token lapses.
async fn authenticate(client: &reqwest::Client, keys: &nostr_sdk::Keys) -> Result<(String, i64)> {
    let ch: serde_json::Value = client
        .post(format!("{API_BASE}/auth/challenge"))
        .json(&serde_json::json!({ "pubkey": keys.public_key().to_hex() }))
        .send()
        .await
        .context("Minibits auth challenge request failed")?
        .error_for_status()
        .context("Minibits auth challenge rejected")?
        .json()
        .await
        .context("Minibits auth challenge was not JSON")?;

    let challenge = ch["challenge"]
        .as_str()
        .ok_or_else(|| anyhow!("Minibits challenge response missing 'challenge'"))?
        .to_string();
    let created_at = ch["createdAt"]
        .as_u64()
        .ok_or_else(|| anyhow!("Minibits challenge response missing 'createdAt'"))?;

    // Sign a NIP-42 auth event, stamping the server's own createdAt so the
    // signature lines up with the challenge it was issued for.
    let unsigned = EventBuilder::new(Kind::from(AUTH_KIND), "")
        .tag(Tag::relay(
            RelayUrl::parse(RELAY_URL).context("Invalid Minibits relay URL")?,
        ))
        .tag(Tag::custom(
            TagKind::custom("challenge"),
            vec![challenge.clone()],
        ))
        .custom_created_at(Timestamp::from(created_at))
        .build(keys.public_key());
    let signed = unsigned
        .sign_with_keys(keys)
        .map_err(|e| anyhow!("Failed to sign the Minibits auth challenge: {e}"))?;

    let tok: serde_json::Value = client
        .post(format!("{API_BASE}/auth/verify"))
        .json(&serde_json::json!({
            "pubkey": keys.public_key().to_hex(),
            "challenge": challenge,
            "signature": hex::encode(signed.sig.serialize()),
        }))
        .send()
        .await
        .context("Minibits auth verify request failed")?
        .error_for_status()
        .context("Minibits auth verify rejected (bad challenge signature)")?
        .json()
        .await
        .context("Minibits auth verify was not JSON")?;

    let access = tok["accessToken"]
        .as_str()
        .ok_or_else(|| anyhow!("Minibits verify response missing 'accessToken'"))?
        .to_string();
    let expires = jwt_expiry(&access).unwrap_or_else(|| {
        chrono::Utc::now().timestamp() + 3600 // conservative fallback
    });
    Ok((access, expires))
}

/// True when the cached access token is missing or about to lapse.
fn token_is_stale(state: &MinibitsState) -> bool {
    let now = chrono::Utc::now().timestamp();
    state.access_token.is_empty() || now + TOKEN_EXPIRY_SKEP_SECS >= state.access_expires
}

/// Ensure we hold a valid access token, re-authenticating as needed and folding
/// the fresh token back into `state` (which the caller persists).
async fn ensure_token(
    client: &reqwest::Client,
    state: &mut MinibitsState,
    keys: &nostr_sdk::Keys,
) -> Result<()> {
    if token_is_stale(state) {
        let (access, expires) = authenticate(client, keys).await?;
        state.access_token = access;
        state.access_expires = expires;
        debug!("Minibits: authenticated (token valid to {})", expires);
    }
    Ok(())
}

/// Draw a fresh readable wallet name, Minibits-style: adjective + noun + number.
fn generate_wallet_id() -> String {
    let mut rng = rand::thread_rng();
    let adj = ADJECTIVES.choose(&mut rng).copied().unwrap_or("quiet");
    let noun = NOUNS.choose(&mut rng).copied().unwrap_or("harbor");
    let num = rand::Rng::gen_range(&mut rng, 1..=999);
    format!("{adj}{noun}{num}")
}

/// Register the profile, returning the assigned address. Retries with a new name
/// a handful of times if the generated name is already taken by another wallet.
async fn register_profile(
    client: &reqwest::Client,
    access: &str,
    seed_hash: &str,
) -> Result<ProfileRecord> {
    let mut last_err = None;
    for attempt in 0..6 {
        let wallet_id = generate_wallet_id();
        let resp = client
            .post(format!("{API_BASE}/profile"))
            .bearer_auth(access)
            .json(&serde_json::json!({ "walletId": wallet_id, "seedHash": seed_hash }))
            .send()
            .await
            .context("Minibits profile registration request failed")?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .context("Minibits profile response body read failed")?;
        if status.is_success() {
            let rec: ProfileRecord = serde_json::from_str(&body)
                .context("Minibits profile response was not the expected shape")?;
            return Ok(rec);
        }
        // Name collision → draw another. Anything else is fatal.
        let is_taken = body.contains("ALREADY_EXISTS") || body.contains("already");
        if is_taken {
            warn!("Minibits name '{wallet_id}' taken, retrying (attempt {attempt})");
            last_err = Some(minibits_error(status, &body));
            continue;
        }
        return Err(minibits_error(status, &body));
    }
    Err(last_err.unwrap_or_else(|| anyhow!("Could not register a free Minibits name")))
}

/// Fetch the LUD-16 metadata for our own address and read the service's
/// `nostrPubkey` — the key that NIP-04-encrypts claimed tokens.
async fn discover_server_nostr_pubkey(client: &reqwest::Client, lud16: &str) -> Result<String> {
    let (name, domain) = lud16
        .split_once('@')
        .ok_or_else(|| anyhow!("Malformed Minibits address '{lud16}'"))?;
    let url = format!("https://{domain}/.well-known/lnurlp/{name}");
    let md: serde_json::Value = client
        .get(&url)
        .send()
        .await
        .context("Minibits LUD-16 metadata request failed")?
        .json()
        .await
        .context("Minibits LUD-16 metadata was not JSON")?;
    md.get("nostrPubkey")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("Minibits LUD-16 metadata missing 'nostrPubkey'"))
}

/// Load the ecash phrase, or establish it from the node master seed when this
/// node has not materialised one yet — but never fail an address request just
/// because a phrase is not on disk; report that clearly instead.
async fn ecash_phrase(data_dir: &Path) -> Result<(String, [u8; 64])> {
    let seed = nut13::load_seed(data_dir)
        .await?
        .ok_or_else(|| anyhow!("The ecash wallet has no seed yet — restore or reveal it first"))?;
    Ok((seed.phrase(), seed.seed_bytes()))
}

/// Get (registering on first use) the node's Minibits Lightning address.
///
/// On mainnet this registers a profile with the Minibits server the first time
/// and caches it in `wallet/minibits.json`; later calls return the cached address
/// and refresh the access token as needed. Registration is idempotent per pubkey,
/// so a node that restores the same ecash phrase recovers the same address.
pub async fn lnaddress(data_dir: &Path) -> Result<serde_json::Value> {
    let _state_guard = MINIBITS_STATE_LOCK.lock().await;
    let network = ecash::load_network(data_dir).await;
    if network == EcashNetwork::Testnet {
        return Err(anyhow!(
            "Minibits Lightning addresses are mainnet-only — switch the ecash network to mainnet to set one up"
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .context("Failed to build the Minibits HTTP client")?;

    let (phrase, seed) = ecash_phrase(data_dir).await?;
    let identity = derive_identity(&phrase, &seed)?;

    let mut state = match load_state(data_dir).await? {
        Some(st) if state_matches_identity(&st, &identity) => st,
        Some(_) => {
            warn!("Minibits: cached profile belongs to a different ecash seed; registering the restored wallet identity");
            archive_state(data_dir, "profile belonged to a different ecash seed").await?;
            register_new_state(&client, &identity).await?
        }
        None => {
            info!("Minibits: no profile yet, registering a new @minibits.cash address");
            register_new_state(&client, &identity).await?
        }
    };

    ensure_token(&client, &mut state, &identity.keys).await?;
    save_state(data_dir, &state).await?;

    Ok(serde_json::json!({
        "address": state.lud16,
        "nip05": state.nip05,
        "wallet_id": state.wallet_id,
        "nostr_pubkey": state.nostr_pubkey,
        "npub": identity.keys.public_key().to_bech32().unwrap_or_default(),
    }))
}

async fn register_new_state(
    client: &reqwest::Client,
    identity: &MinibitsIdentity,
) -> Result<MinibitsState> {
    let (access, expires) = authenticate(client, &identity.keys).await?;
    let rec = register_profile(client, &access, &identity.seed_hash).await?;
    let lud16 = rec
        .lud16
        .clone()
        .unwrap_or_else(|| format!("{}@minibits.cash", rec.wallet_id));
    let (name, domain) = lud16
        .split_once('@')
        .ok_or_else(|| anyhow!("Minibits returned malformed Lightning address '{lud16}'"))?;
    if name.is_empty() || !domain.eq_ignore_ascii_case("minibits.cash") {
        return Err(anyhow!(
            "Minibits returned an unexpected Lightning-address domain '{domain}'"
        ));
    }
    Ok(MinibitsState {
        wallet_id: rec.wallet_id,
        lud16,
        nip05: rec.nip05,
        // Bind local state to the key we actually derived and authenticated,
        // rather than trusting an optional echo in the remote response.
        nostr_pubkey: identity.keys.public_key().to_hex(),
        seed_hash: identity.seed_hash.clone(),
        access_token: access,
        access_expires: expires,
        server_nostr_pubkey: String::new(),
        created_at: chrono::Utc::now().to_rfc3339(),
        pending_claims: Vec::new(),
        last_dm_seen_at: 0,
        seen_dm_ids: Vec::new(),
        last_receipt_id: 0,
        last_receipt_sats: 0,
        last_receipt_at: 0,
    })
}

/// Outcome of a claim poll.
#[derive(Debug, Serialize)]
pub struct ClaimOutcome {
    pub claimed_count: usize,
    pub received_sats: u64,
    /// Claims that were fetched (and so already consumed server-side) but
    /// still haven't been redeemed after this poll — decrypt/redeem failed
    /// and they are queued in `pending_claims` for the next poll rather than
    /// dropped. Non-zero here means real, unswept value the operator should
    /// know about.
    pub failed_count: usize,
    /// Most recent successful receipt, including one redeemed by another
    /// concurrent UI poll. Zero means this wallet has no recorded receipt.
    pub receipt_id: u64,
    pub receipt_sats: u64,
    pub receipt_at: u64,
}

const NO_CLAIMS: ClaimOutcome = ClaimOutcome {
    claimed_count: 0,
    received_sats: 0,
    failed_count: 0,
    receipt_id: 0,
    receipt_sats: 0,
    receipt_at: 0,
};

fn outcome_with_latest_receipt(
    state: &MinibitsState,
    claimed_count: usize,
    received_sats: u64,
    failed_count: usize,
) -> ClaimOutcome {
    ClaimOutcome {
        claimed_count,
        received_sats,
        failed_count,
        receipt_id: state.last_receipt_id,
        receipt_sats: state.last_receipt_sats,
        receipt_at: state.last_receipt_at,
    }
}

/// Make sure the Minibits mint is on the accepted-mints allow-list.
///
/// `ecash::receive_token` checks the raw accepted-mints file directly (not
/// the more lenient `ecash::is_mint_trusted`, which always trusts the default
/// mint) — so an operator who edited their accepted-mints list (e.g. via the
/// `streaming.configure-mints` RPC) and dropped the default mint would
/// otherwise cause every Minibits claim to fail *after* the claim was already
/// consumed server-side, permanently losing those coins with nothing but a
/// log line to show for it. The Minibits Lightning address is inherently
/// backed by this one mint — registering it already implies trusting the
/// mint — so self-heal the allow-list here rather than let that combination
/// silently strand funds.
/// Fetch NIP-04 DM (kind 4) events addressed to `our_pubkey` newer than
/// `since`, from `CLAIM_RELAY_URLS`. Returns each event's raw (still
/// encrypted) content plus its `created_at`, newest last. This — not
/// `POST /claim` — is how Minibits actually delivers a Lightning payment
/// made to a `@minibits.cash` address: confirmed live 2026-09-08 against
/// three real payments that `/claim` never surfaced. Best-effort: a relay
/// error here must not abort the poll, since `pending_claims` may still hold
/// earlier fetches worth retrying.
async fn fetch_relay_dms(
    our_pubkey: nostr_sdk::PublicKey,
    server_pubkey: nostr_sdk::PublicKey,
    since: u64,
) -> Vec<(String, u64, String, String)> {
    let client = Client::default();
    for url in CLAIM_RELAY_URLS {
        if let Err(e) = client.add_relay(*url).await {
            warn!("Minibits: could not add relay {url}: {e}");
        }
    }
    client.connect().await;
    // Give relays a moment to finish the WebSocket handshake before the
    // fetch's own timeout starts consuming that time.
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    // Nostr timestamps have one-second resolution. Query the boundary second
    // inclusively: a later-published payment may legitimately share that
    // timestamp. `seen_dm_ids` performs the exact deduplication locally.
    let filter = Filter::new()
        .author(server_pubkey)
        .pubkey(our_pubkey)
        .kind(Kind::from(4u16))
        .since(Timestamp::from(since))
        .limit(200);

    let result = match client
        .fetch_events(filter, std::time::Duration::from_secs(5))
        .await
    {
        Ok(events) => {
            let mut out: Vec<(String, u64, String, String)> = events
                .into_iter()
                .map(|e| {
                    (
                        e.content,
                        e.created_at.as_secs(),
                        e.pubkey.to_hex(),
                        e.id.to_hex(),
                    )
                })
                .collect();
            out.sort_by_key(|(_, created_at, _, _)| *created_at);
            out
        }
        Err(e) => {
            warn!("Minibits: relay fetch for claim DMs failed: {e}");
            Vec::new()
        }
    };

    client.shutdown().await;
    result
}

fn queue_relay_dm(
    state: &mut MinibitsState,
    content: String,
    created_at: u64,
    event_id: String,
    sender_pubkey: String,
) -> bool {
    if state.seen_dm_ids.iter().any(|seen| seen == &event_id) {
        return false;
    }
    state.pending_claims.push(PendingClaim::Attributed {
        content,
        sender_pubkey,
    });
    state.last_dm_seen_at = state.last_dm_seen_at.max(created_at);
    state.seen_dm_ids.push(event_id);
    // This is only a boundary-second dedupe window, not transaction history.
    // Keep it bounded while retaining ample overlap for delayed relay delivery.
    if state.seen_dm_ids.len() > 512 {
        let excess = state.seen_dm_ids.len() - 512;
        state.seen_dm_ids.drain(..excess);
    }
    true
}

async fn ensure_mint_accepted(data_dir: &Path, mint_url: &str) -> Result<()> {
    let mut accepted = ecash::load_accepted_mints(data_dir).await?;
    if !accepted.mints.iter().any(|m| m == mint_url) {
        accepted.mints.push(mint_url.to_string());
        ecash::save_accepted_mints(data_dir, &accepted).await?;
        info!("Minibits: added {mint_url} to accepted mints (needed to redeem LN-address claims)");
    }
    Ok(())
}

/// Poll Minibits for Lightning payments sent to the node's address and redeem
/// each into the ecash wallet.
///
/// Each claim is a NUT-00 token NIP-04-encrypted by the Minibits service to
/// this wallet's Nostr key; decrypting it needs the service pubkey
/// (discovered from our LUD-16 metadata, falling back to the known
/// constant). A claim is consumed server-side the instant `/claim` returns
/// it, so newly-fetched tokens are persisted to `state.pending_claims`
/// *before* decrypt/redeem is attempted; a token that fails to decrypt or
/// redeem stays in `pending_claims` and is retried on the next poll instead
/// of being dropped, and `failed_count` tells the caller when that happened
/// so it isn't purely a log-line event.
pub async fn claim_and_redeem(data_dir: &Path) -> Result<ClaimOutcome> {
    let _state_guard = MINIBITS_STATE_LOCK.lock().await;
    let network = ecash::load_network(data_dir).await;
    if network == EcashNetwork::Testnet {
        return Ok(NO_CLAIMS);
    }
    ensure_mint_accepted(data_dir, &network.default_mint()).await?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("Failed to build the Minibits HTTP client")?;

    let (phrase, seed) = ecash_phrase(data_dir).await?;
    let identity = derive_identity(&phrase, &seed)?;

    let mut state = match load_state(data_dir).await? {
        Some(st) if state_matches_identity(&st, &identity) => st,
        Some(_) => {
            return Err(anyhow!(
                "The cached Minibits profile belongs to a different ecash seed; open Receive → Ecash to register the restored wallet first"
            ));
        }
        // Nothing is addressable until a profile exists; registering lazily here
        // means a payment could not have arrived, so claiming is a no-op.
        None => return Ok(NO_CLAIMS),
    };

    // The global lock serialises claim redemption, but without this fast path
    // every browser waiting on that lock performed its own five-to-ten-second
    // relay fetch in turn. Reuse the just-completed durable result for the
    // short coalescing window; a later UI poll performs the next real fetch.
    let now = chrono::Utc::now().timestamp().max(0) as u64;
    let last_completed = LAST_MINIBITS_POLL_COMPLETED_AT.load(Ordering::Acquire);
    if last_completed > 0 && now.saturating_sub(last_completed) <= MINIBITS_POLL_COALESCE_SECS {
        return Ok(outcome_with_latest_receipt(
            &state,
            0,
            0,
            state.pending_claims.len(),
        ));
    }
    ensure_token(&client, &mut state, &identity.keys).await?;

    // Refresh the service key that authors and encrypts claim DMs. Keeping the
    // first discovered key forever would make a legitimate Minibits rotation
    // invisible: the relay filter would exclude every event from the new key.
    // A metadata outage retains the last known valid key; only a fresh profile
    // with no cached key needs the compiled fallback.
    match discover_server_nostr_pubkey(&client, &state.lud16).await {
        Ok(candidate) if nostr_sdk::PublicKey::from_hex(&candidate).is_ok() => {
            state.server_nostr_pubkey = candidate;
        }
        Ok(candidate) => {
            warn!("Minibits: LUD-16 metadata returned invalid Nostr pubkey {candidate:?}; retaining the last valid key");
        }
        Err(e) => {
            warn!("Minibits: could not refresh service Nostr pubkey ({e}); retaining the last valid key");
        }
    }
    if state.server_nostr_pubkey.is_empty() {
        state.server_nostr_pubkey = FALLBACK_SERVER_NOSTR_PUBKEY.to_string();
    }

    let server_pk = nostr_sdk::PublicKey::from_hex(&state.server_nostr_pubkey)
        .context("Service Nostr pubkey was not valid hex")?;

    // Fetch anything new. A failure here is *not* fatal to the poll — the
    // operator may still have earlier claims sitting in `pending_claims` that
    // are worth retrying — so log and fall through instead of bailing out.
    let resp = client
        .post(format!("{API_BASE}/claim"))
        .bearer_auth(&state.access_token)
        .json(&serde_json::json!({ "seedHash": state.seed_hash }))
        .send()
        .await;
    match resp {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if status.is_success() {
                match serde_json::from_str::<Vec<serde_json::Value>>(&body) {
                    Ok(claims) => {
                        for claim in &claims {
                            match claim.get("token").and_then(|t| t.as_str()) {
                                Some(t) => state.pending_claims.push(PendingClaim::Attributed {
                                    content: t.to_string(),
                                    sender_pubkey: state.server_nostr_pubkey.clone(),
                                }),
                                None => warn!("Minibits claim had no 'token' field; skipping"),
                            }
                        }
                    }
                    Err(e) => warn!("Minibits claim response was not the expected shape: {e}"),
                }
            } else {
                warn!("{}", minibits_error(status, &body));
            }
        }
        Err(e) => {
            warn!("Minibits claim request failed ({e}); retrying only previously-pending claims")
        }
    }

    // The actual delivery channel: real Lightning payments arrive as a
    // NIP-04 DM on relays, not via `/claim` above. `since` is our own
    // watermark (Nostr events never expire off a relay, so without it we'd
    // re-fetch and re-attempt every claim ever sent on every poll).
    let dms = fetch_relay_dms(identity.keys.public_key(), server_pk, state.last_dm_seen_at).await;
    for (content, created_at, author, event_id) in dms {
        if author != state.server_nostr_pubkey {
            warn!("Minibits: ignoring claim DM from unexpected pubkey {author}");
            continue;
        }
        queue_relay_dm(&mut state, content, created_at, event_id, author);
    }

    // Persist immediately: everything in `pending_claims` right now has
    // already been consumed server-side (or, for relay DMs, is public and
    // can't be un-sent), whether it came from this fetch or survived from an
    // earlier failed attempt.
    save_state(data_dir, &state).await?;

    if state.pending_claims.is_empty() {
        LAST_MINIBITS_POLL_COMPLETED_AT.store(
            chrono::Utc::now().timestamp().max(0) as u64,
            Ordering::Release,
        );
        return Ok(outcome_with_latest_receipt(&state, 0, 0, 0));
    }

    let to_process = std::mem::take(&mut state.pending_claims);
    let mut redeemed = 0usize;
    let mut sats = 0u64;
    let mut still_pending = Vec::new();
    for claim in &to_process {
        let claim_server_pk =
            match nostr_sdk::PublicKey::from_hex(claim.sender_pubkey(&state.server_nostr_pubkey)) {
                Ok(pubkey) => pubkey,
                Err(e) => {
                    warn!(
                        "Minibits claim has an invalid sender pubkey ({e}); will retry next poll"
                    );
                    still_pending.push(claim.clone());
                    continue;
                }
            };
        let decoded = match nip04::decrypt(
            identity.keys.secret_key(),
            &claim_server_pk,
            claim.content(),
        ) {
            Ok(d) => d,
            Err(e) => {
                warn!("Minibits claim could not be decrypted ({e}); will retry next poll");
                still_pending.push(claim.clone());
                continue;
            }
        };
        match ecash::receive_token(data_dir, &decoded).await {
            Ok(got) => {
                redeemed += 1;
                sats += got;
                info!("Minibits: redeemed a claimed payment ({got} sats)");
            }
            Err(e) => {
                warn!("Minibits claim decrypted but failed to redeem ({e}); will retry next poll");
                still_pending.push(claim.clone());
            }
        }
    }

    let failed_count = still_pending.len();
    state.pending_claims = still_pending;
    if sats > 0 {
        state.last_receipt_id = state.last_receipt_id.saturating_add(1).max(1);
        state.last_receipt_sats = sats;
        state.last_receipt_at = chrono::Utc::now().timestamp().max(0) as u64;
    }
    save_state(data_dir, &state).await?;
    LAST_MINIBITS_POLL_COMPLETED_AT.store(
        chrono::Utc::now().timestamp().max(0) as u64,
        Ordering::Release,
    );

    Ok(outcome_with_latest_receipt(
        &state,
        redeemed,
        sats,
        failed_count,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_nostr_key_matches_the_nip06_vector() {
        // The Minibits app derives its Nostr key at m/44'/1237'/0'/0/0 with an
        // empty BIP-39 passphrase (nostr-tools accountFromSeedWords). Lock to the
        // crate's own NIP-06 secret-key vector so a nostr-sdk bump cannot silently
        // move our derivation and orphan the registered address.
        let phrase =
            "leader monkey parrot ring guide accident before fence cannon height naive bean";
        let keys = nostr_sdk::Keys::from_mnemonic(phrase, None::<&str>).unwrap();
        assert_eq!(
            hex::encode(keys.secret_key().as_secret_bytes()),
            "7f7ff03d123792d6ac594bfa67bf6d0c0ab55b6b1fdb6249303fe861f1ccba9a"
        );
    }

    #[test]
    fn seed_hash_is_sha256_of_the_bip39_seed() {
        // Minibits hashes the *seed*, not the phrase — a regression here would
        // make the node register a profile that the Minibits app cannot recover.
        let phrase =
            "leader monkey parrot ring guide accident before fence cannon height naive bean";
        let m: bip39::Mnemonic = phrase.parse().unwrap();
        let seed = m.to_seed("");
        let want = hex::encode(Sha256::digest(seed));
        let id = derive_identity(phrase, &seed).unwrap();
        assert_eq!(id.seed_hash, want);
    }

    #[test]
    fn cached_profile_is_bound_to_the_current_ecash_identity() {
        let phrase =
            "leader monkey parrot ring guide accident before fence cannon height naive bean";
        let mnemonic: bip39::Mnemonic = phrase.parse().unwrap();
        let seed = mnemonic.to_seed("");
        let identity = derive_identity(phrase, &seed).unwrap();
        let mut state = MinibitsState {
            seed_hash: identity.seed_hash.clone(),
            nostr_pubkey: identity.keys.public_key().to_hex(),
            ..Default::default()
        };
        assert!(state_matches_identity(&state, &identity));
        state.seed_hash = "restored-different-seed".into();
        assert!(!state_matches_identity(&state, &identity));
    }

    #[test]
    fn generated_names_are_readable_and_bounded() {
        for _ in 0..200 {
            let n = generate_wallet_id();
            assert!(!n.is_empty());
            assert!(n
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
            // ends in at least one digit (the 1..=999 suffix)
            assert!(n
                .chars()
                .last()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false));
        }
    }

    #[test]
    fn stale_token_when_missing_or_near_expiry() {
        let now = chrono::Utc::now().timestamp();
        assert!(token_is_stale(&MinibitsState::default()));
        assert!(token_is_stale(&MinibitsState {
            access_token: "x".into(),
            access_expires: now + 10, // inside the skew window
            ..Default::default()
        }));
        assert!(!token_is_stale(&MinibitsState {
            access_token: "x".into(),
            access_expires: now + 3600,
            ..Default::default()
        }));
    }

    #[test]
    fn api_error_excerpt_is_utf8_safe() {
        let body = "é".repeat(181);
        let error = minibits_error(reqwest::StatusCode::BAD_GATEWAY, &body);
        assert!(error.to_string().contains("Minibits API error 502"));
    }

    #[tokio::test]
    async fn ensure_mint_accepted_heals_a_dropped_default_mint() {
        // Regression guard: `ecash::receive_token` checks the raw accepted-mints
        // file, not the more lenient `is_mint_trusted` — so an operator-edited
        // allow-list that dropped the default mint must not be able to make
        // Minibits claims (already consumed server-side by the time redeem
        // runs) fail permanently and silently.
        let tmp = tempfile::TempDir::new().unwrap();
        let mint = "https://mint.minibits.cash/Bitcoin";
        ecash::save_accepted_mints(
            tmp.path(),
            &ecash::AcceptedMints {
                mints: vec!["https://mint.example.com".to_string()],
            },
        )
        .await
        .unwrap();

        ensure_mint_accepted(tmp.path(), mint).await.unwrap();

        let accepted = ecash::load_accepted_mints(tmp.path()).await.unwrap();
        assert!(accepted.mints.iter().any(|m| m == mint));
        assert!(accepted
            .mints
            .iter()
            .any(|m| m == "https://mint.example.com"));
    }

    #[tokio::test]
    async fn ensure_mint_accepted_does_not_duplicate() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mint = "https://mint.minibits.cash/Bitcoin";
        ensure_mint_accepted(tmp.path(), mint).await.unwrap();
        ensure_mint_accepted(tmp.path(), mint).await.unwrap();
        let accepted = ecash::load_accepted_mints(tmp.path()).await.unwrap();
        assert_eq!(accepted.mints.iter().filter(|m| *m == mint).count(), 1);
    }

    #[test]
    fn dm_watermark_advances_but_never_rewinds() {
        // `claim_and_redeem` does `state.last_dm_seen_at.max(created_at)` per
        // event. Events from `fetch_relay_dms` are sorted ascending, but the
        // watermark must still be safe against an out-of-order relay
        // response (or a future refactor) — it must never move backward, or
        // an already-queued DM gets re-fetched and its now-spent token
        // re-attempted forever.
        let mut watermark = 100u64;
        for created_at in [105, 103, 110, 108] {
            watermark = watermark.max(created_at);
        }
        assert_eq!(watermark, 110);
    }

    #[test]
    fn relay_dedupe_keeps_distinct_payments_from_the_same_second() {
        let mut state = MinibitsState::default();
        assert!(queue_relay_dm(
            &mut state,
            "first".into(),
            100,
            "id-1".into(),
            "server-1".into(),
        ));
        assert!(queue_relay_dm(
            &mut state,
            "second".into(),
            100,
            "id-2".into(),
            "server-2".into(),
        ));
        assert!(!queue_relay_dm(
            &mut state,
            "duplicate".into(),
            100,
            "id-1".into(),
            "server-1".into(),
        ));
        assert_eq!(state.pending_claims.len(), 2);
        assert_eq!(state.pending_claims[0].content(), "first");
        assert_eq!(
            state.pending_claims[0].sender_pubkey("fallback"),
            "server-1"
        );
        assert_eq!(state.pending_claims[1].content(), "second");
        assert_eq!(
            state.pending_claims[1].sender_pubkey("fallback"),
            "server-2"
        );
        assert_eq!(state.last_dm_seen_at, 100);
    }

    #[test]
    fn legacy_pending_claims_remain_readable() {
        let state: MinibitsState = serde_json::from_value(serde_json::json!({
            "wallet_id": "legacy-wallet",
            "lud16": "legacy-wallet@minibits.cash",
            "nip05": "legacy-wallet@minibits.cash",
            "nostr_pubkey": "node-key",
            "seed_hash": "seed-hash",
            "pending_claims": ["legacy-encrypted-token"]
        }))
        .unwrap();

        assert_eq!(state.pending_claims.len(), 1);
        assert_eq!(state.pending_claims[0].content(), "legacy-encrypted-token");
        assert_eq!(
            state.pending_claims[0].sender_pubkey("cached-server-key"),
            "cached-server-key"
        );
        assert_eq!(state.last_receipt_id, 0);
        assert_eq!(state.last_receipt_sats, 0);
        assert_eq!(state.last_receipt_at, 0);
    }

    #[test]
    fn latest_receipt_survives_a_zero_claim_poll() {
        let state = MinibitsState {
            last_receipt_id: 9,
            last_receipt_sats: 1_000,
            last_receipt_at: 1_789_000_000,
            ..Default::default()
        };
        let outcome = outcome_with_latest_receipt(&state, 0, 0, 0);
        assert_eq!(outcome.received_sats, 0);
        assert_eq!(outcome.receipt_id, 9);
        assert_eq!(outcome.receipt_sats, 1_000);
        assert_eq!(outcome.receipt_at, 1_789_000_000);
    }

    #[tokio::test]
    async fn load_state_treats_empty_file_as_no_profile() {
        // Reproduces archy-x250-pa3, 2026-09-08: a disk-full write truncated
        // wallet/minibits.json to 0 bytes, which then made every
        // wallet.ecash-lnaddress call fail with "EOF while parsing a value"
        // instead of just re-registering (idempotent per pubkey, so safe).
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join(STATE_FILE);
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&path, b"").await.unwrap();

        let st = load_state(tmp.path()).await.unwrap();
        assert!(st.is_none());
    }

    #[tokio::test]
    async fn load_state_treats_corrupt_json_as_no_profile() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join(STATE_FILE);
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&path, b"{ not valid json").await.unwrap();

        let st = load_state(tmp.path()).await.unwrap();
        assert!(st.is_none());
        assert!(!path.exists());
        let backups = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("minibits.recovery-")
            })
            .count();
        assert_eq!(backups, 1);
    }

    #[tokio::test]
    async fn save_state_is_atomic_private_and_round_trips() {
        let tmp = tempfile::TempDir::new().unwrap();
        let state = MinibitsState {
            wallet_id: "quietisland7".into(),
            lud16: "quietisland7@minibits.cash".into(),
            nostr_pubkey: "abc".into(),
            seed_hash: "def".into(),
            pending_claims: vec![PendingClaim::Attributed {
                content: "encrypted-value".into(),
                sender_pubkey: "server-key".into(),
            }],
            ..Default::default()
        };
        save_state(tmp.path(), &state).await.unwrap();
        let loaded = load_state(tmp.path()).await.unwrap().unwrap();
        assert_eq!(loaded.pending_claims, state.pending_claims);
        assert!(!state_path(tmp.path()).with_extension("json.tmp").exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(state_path(tmp.path()))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600,
            );
        }
    }

    /// Live end-to-end against the production Minibits API: register a throwaway
    /// profile with a random ecash phrase and claim (nothing pending → 0). Run
    /// with `cargo test -- --ignored --nocapture`. It creates one disposable
    /// profile on the public service and holds no funds.
    #[tokio::test]
    #[ignore]
    async fn registers_and_claims_against_live_minibits() {
        let dir = std::env::temp_dir().join(format!("mbtest-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // A node has an ecash phrase before it has a Minibits profile; stand up
        // a fresh random one so registration derives a real identity.
        nut13::establish_independent(&dir).await.unwrap();

        let info = lnaddress(&dir).await.expect("live registration failed");
        let addr = info["address"].as_str().unwrap().to_string();
        assert!(addr.ends_with("@minibits.cash"), "bad address {addr}");
        println!("registered live address: {addr}");

        // A second call must return the same cached address, not register again.
        let again = lnaddress(&dir).await.unwrap();
        assert_eq!(again["address"].as_str().unwrap(), addr.as_str());

        let out = claim_and_redeem(&dir)
            .await
            .expect("live claim poll failed");
        println!("claim poll: {out:?}");
        assert_eq!(out.claimed_count, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
