//! Process-local media handles; recovery reissues a handle for the same receipt.
//! No seller capability, wallet token or peer proof is sent to the browser.
use crate::{
    container::registration_pin::InstalledAppContext,
    content_purchase::{Contract, Journal},
};
use anyhow::{Context, Result};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

const MAX_HANDLES: usize = 1024;
const HANDLE_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Binding {
    pub context: InstalledAppContext,
    pub seller_onion: String,
    pub contract: Contract,
    session: [u8; 32],
}
struct Entry {
    binding: Binding,
    until: Instant,
    lease_expires: Option<u64>,
}
#[derive(Default)]
pub(crate) struct PlaybackHandles {
    entries: Mutex<HashMap<String, Entry>>,
}

fn session_fingerprint(session: &str) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"archipelago-local-playback-session-v1\0");
    digest.update(session.as_bytes());
    digest.finalize().into()
}
fn valid_handle(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
impl PlaybackHandles {
    /// Invoked only after normal owner-session/CSRF checks and the native broker's
    /// verified installed-app/account authorization for this saved purchase.
    /// It does not contact the seller, spend, open bytes, or start a rental lease.
    pub async fn issue(
        &self,
        data_dir: &Path,
        context: InstalledAppContext,
        session: &str,
        purchase_id: &str,
    ) -> Result<String> {
        anyhow::ensure!(!session.is_empty(), "Owner session required");
        let record = Journal::open(data_dir)
            .await?
            .buyer(purchase_id)
            .await?
            .context("Original purchase is missing; recover it without paying again")?;
        let seller_onion = crate::content_purchase_transport::seller_onion_for_did(
            data_dir,
            &record.contract.seller_did,
        )
        .await?;
        let peer = crate::federation::load_unique_payment_peer(data_dir, &seller_onion).await?;
        anyhow::ensure!(
            record.receipt().is_some(),
            "Original purchase is not settled"
        );
        anyhow::ensure!(
            record.contract.buyer_did == context.node_did
                && record.contract.seller_did == peer.did
                && record.contract.content_id.starts_with("registered_"),
            "Purchase does not match the current node and seller"
        );
        record.contract.validate()?;
        self.insert(
            Binding {
                context,
                seller_onion: seller_onion.trim_end_matches(".onion").to_owned(),
                contract: record.contract,
                session: session_fingerprint(session),
            },
            Instant::now(),
        )
    }
    fn insert(&self, binding: Binding, now: Instant) -> Result<String> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| anyhow::anyhow!("Playback handles unavailable"))?;
        entries.retain(|_, entry| now < entry.until);
        if let Some((handle, _)) = entries.iter().find(|(_, entry)| entry.binding == binding) {
            return Ok(handle.clone());
        }
        anyhow::ensure!(
            entries.len() < MAX_HANDLES,
            "Too many active playback handles"
        );
        let until = now
            .checked_add(HANDLE_LIFETIME)
            .context("Playback clock overflow")?;
        let handle = loop {
            let mut bytes = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut bytes);
            let handle = hex::encode(bytes);
            if !entries.contains_key(&handle) {
                break handle;
            }
        };
        entries.insert(
            handle.clone(),
            Entry {
                binding,
                until,
                lease_expires: None,
            },
        );
        Ok(handle)
    }
    /// Session validity is checked independently on GET and during streaming.
    /// Context must be freshly loaded from installed runtime state and its pin.
    pub fn lookup(
        &self,
        handle: &str,
        session: &str,
        context: &InstalledAppContext,
    ) -> Result<Binding> {
        anyhow::ensure!(valid_handle(handle), "Invalid playback handle");
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| anyhow::anyhow!("Playback handles unavailable"))?;
        let now = Instant::now();
        entries.retain(|_, entry| now < entry.until);
        let entry = entries
            .get(handle)
            .context("Playback handle expired; reopen the original purchase")?;
        anyhow::ensure!(
            entry.binding.session == session_fingerprint(session)
                && &entry.binding.context == context,
            "Playback session or installed app changed"
        );
        Ok(entry.binding.clone())
    }
    /// Called only after the authenticated seller response matches receipt/size/range.
    pub fn note_expiry(&self, handle: &str, binding: &Binding, expires: u64) -> Result<()> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| anyhow::anyhow!("Playback handles unavailable"))?;
        let entry = entries.get_mut(handle).context("Playback handle expired")?;
        anyhow::ensure!(
            &entry.binding == binding && Instant::now() < entry.until && expires > 0,
            "Playback handle changed"
        );
        anyhow::ensure!(
            entry.lease_expires.is_none_or(|old| old == expires),
            "Seller changed the original viewing window"
        );
        entry.lease_expires = Some(expires);
        Ok(())
    }
    /// Owner-session/native-broker status lookup; only public expiry leaves node.
    pub fn expiry(
        &self,
        handle: &str,
        session: &str,
        context: &InstalledAppContext,
    ) -> Result<Option<u64>> {
        self.lookup(handle, session, context)?;
        let entries = self
            .entries
            .lock()
            .map_err(|_| anyhow::anyhow!("Playback handles unavailable"))?;
        Ok(entries
            .get(handle)
            .context("Playback handle expired")?
            .lease_expires)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn binding() -> Binding {
        let buyer = crate::identity::did_key_from_pubkey_hex(&hex::encode([1; 32])).unwrap();
        Binding {
            context: InstalledAppContext {
                app_id: "indeedhub".into(),
                backend_id: "indeedhub-api".into(),
                app_audience: "installed-audience".into(),
                node_did: buyer.clone(),
                node_public_key: hex::encode([1; 32]),
                app_origins: vec!["http://node:7777".into()],
            },
            seller_onion: "seller".into(),
            session: session_fingerprint("original-session"),
            contract: Contract {
                version: 1,
                id: uuid::Uuid::new_v4().to_string(),
                buyer_did: buyer,
                seller_did: crate::identity::did_key_from_pubkey_hex(&hex::encode([2; 32]))
                    .unwrap(),
                content_id: "registered_media".into(),
                content_sha256: "ab".repeat(32),
                content_size: 1024,
                terms_sha256: "cd".repeat(32),
                network: crate::wallet::ecash::EcashNetwork::Mainnet,
                mint_url: "https://mint.invalid".into(),
                gross_token_sats: 8,
                minimum_net_sats: 7,
                offered_at: 1000,
                expires_at: 2000,
            },
        }
    }
    #[test]
    fn reissue_keeps_original_contract_and_fixed_expiry_without_extending_handle_lifetime() {
        let handles = PlaybackHandles::default();
        let binding = binding();
        let now = Instant::now();
        let handle = handles.insert(binding.clone(), now).unwrap();
        handles.note_expiry(&handle, &binding, 12345).unwrap();
        assert_eq!(
            handles
                .insert(binding.clone(), now + Duration::from_secs(3))
                .unwrap(),
            handle
        );
        assert_eq!(
            handles
                .expiry(&handle, "original-session", &binding.context)
                .unwrap(),
            Some(12345)
        );
        assert!(handles.note_expiry(&handle, &binding, 12346).is_err());
        let refreshed = handles
            .insert(binding.clone(), now + HANDLE_LIFETIME)
            .unwrap();
        assert_ne!(refreshed, handle);
        assert!(handles
            .lookup(&handle, "original-session", &binding.context)
            .is_err());
        assert_eq!(
            handles
                .lookup(&refreshed, "original-session", &binding.context)
                .unwrap()
                .contract,
            binding.contract
        );
        // No new seller lease is implied by a process-local handle: expiry stays
        // unknown until the original receipt's authenticated GET reports it.
        assert_eq!(
            handles
                .expiry(&refreshed, "original-session", &binding.context)
                .unwrap(),
            None
        );
    }
    #[test]
    fn handles_reject_other_sessions_installations_and_malformed_tokens() {
        let handles = PlaybackHandles::default();
        let binding = binding();
        let handle = handles.insert(binding.clone(), Instant::now()).unwrap();
        assert!(handles
            .lookup(&handle, "other-session", &binding.context)
            .is_err());
        let mut changed = binding.context.clone();
        changed.app_audience = "reinstalled".into();
        assert!(handles
            .lookup(&handle, "original-session", &changed)
            .is_err());
        for value in ["", "../secret", &"A".repeat(64), &"a".repeat(63)] {
            assert!(handles
                .lookup(value, "original-session", &binding.context)
                .is_err());
        }
        assert!(handles
            .lookup(&handle, "original-session", &binding.context)
            .is_ok());
    }
    #[tokio::test]
    async fn owner_session_revocation_does_not_become_a_new_handle_authorization() {
        let root = tempfile::tempdir().unwrap();
        let sessions =
            crate::session::SessionStore::new_for_tests(root.path().join("sessions.json"));
        let token = sessions.create().await;
        let mut binding = binding();
        binding.session = session_fingerprint(&token);
        let handles = PlaybackHandles::default();
        let handle = handles.insert(binding.clone(), Instant::now()).unwrap();
        assert!(sessions.validate(&token).await);
        assert!(handles.lookup(&handle, &token, &binding.context).is_ok());
        sessions.remove(&token).await;
        assert!(!sessions.validate(&token).await);
        let replacement = sessions.create().await;
        assert!(handles
            .lookup(&handle, &replacement, &binding.context)
            .is_err());
    }
}
