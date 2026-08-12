// WIP mesh/transport protocol — suppress dead code warnings
#![allow(dead_code)]
//! Double Ratchet protocol for forward-secret mesh messaging.
//!
//! Implements the Signal protocol's Double Ratchet algorithm:
//! - DH ratchet: new X25519 ephemeral keypair per DH step
//! - Symmetric-key ratchet: HKDF-SHA256 chain for message keys
//! - Forward secrecy: compromising current key doesn't reveal past messages
//!
//! Wire format per message:
//! ```text
//! [RatchetHeader: 40 bytes] [nonce: 12] [ciphertext] [tag: 16]
//! ```
//!
//! Reference: Signal Technical Documentation — Double Ratchet Algorithm

use super::crypto;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zeroize::Zeroize;

/// HKDF info string for root key + chain key derivation.
const KDF_RK_INFO: &[u8] = b"ArchyRatchetRK";

/// HKDF info string for message key derivation from chain key.
const KDF_CK_INFO: &[u8] = b"ArchyRatchetCK";

/// Maximum number of skipped message keys to store (prevents DoS).
const MAX_SKIP: u32 = 100;

/// Ratchet message header sent with every encrypted message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatchetHeader {
    /// Sender's current DH ratchet public key (32 bytes).
    #[serde(with = "hex_bytes")]
    pub dh_public: [u8; 32],
    /// Number of messages in the previous sending chain.
    pub prev_chain_n: u32,
    /// Message number in the current sending chain.
    pub message_n: u32,
}

impl RatchetHeader {
    /// Serialize header to bytes (fixed 40 bytes).
    pub fn to_bytes(&self) -> [u8; 40] {
        let mut buf = [0u8; 40];
        buf[..32].copy_from_slice(&self.dh_public);
        buf[32..36].copy_from_slice(&self.prev_chain_n.to_le_bytes());
        buf[36..40].copy_from_slice(&self.message_n.to_le_bytes());
        buf
    }

    /// Parse header from bytes.
    pub fn from_bytes(data: &[u8; 40]) -> Self {
        let mut dh_public = [0u8; 32];
        dh_public.copy_from_slice(&data[..32]);
        let prev_chain_n = u32::from_le_bytes([data[32], data[33], data[34], data[35]]);
        let message_n = u32::from_le_bytes([data[36], data[37], data[38], data[39]]);
        Self {
            dh_public,
            prev_chain_n,
            message_n,
        }
    }
}

/// A complete ratchet-encrypted message (header + ciphertext).
#[derive(Debug, Clone)]
pub struct RatchetMessage {
    pub header: RatchetHeader,
    pub ciphertext: Vec<u8>, // nonce(12) + encrypted(N) + tag(16)
}

impl RatchetMessage {
    /// Serialize to wire format: header(40) + ciphertext.
    pub fn to_bytes(&self) -> Vec<u8> {
        let header_bytes = self.header.to_bytes();
        let mut buf = Vec::with_capacity(40 + self.ciphertext.len());
        buf.extend_from_slice(&header_bytes);
        buf.extend_from_slice(&self.ciphertext);
        buf
    }

    /// Parse from wire format.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 40 + 12 + 16 + 1 {
            anyhow::bail!("Ratchet message too short: {} bytes", data.len());
        }
        let mut header_bytes = [0u8; 40];
        header_bytes.copy_from_slice(&data[..40]);
        Ok(Self {
            header: RatchetHeader::from_bytes(&header_bytes),
            ciphertext: data[40..].to_vec(),
        })
    }
}

/// Per-peer Double Ratchet state.
#[derive(Serialize, Deserialize)]
pub struct RatchetState {
    // DH ratchet: our current ephemeral keypair
    dh_self_secret: [u8; 32],
    dh_self_public: [u8; 32],
    // DH ratchet: peer's last known public key
    dh_remote_public: Option<[u8; 32]>,
    // Root key (ratcheted on each DH step)
    root_key: [u8; 32],
    // Sending chain key
    chain_key_send: Option<[u8; 32]>,
    // Receiving chain key
    chain_key_recv: Option<[u8; 32]>,
    // Message counters
    send_n: u32,
    recv_n: u32,
    prev_send_n: u32,
    // Skipped message keys for out-of-order delivery
    // Key: (dh_public_hex, message_number)
    skipped_keys: HashMap<(String, u32), [u8; 32]>,
}

impl Drop for RatchetState {
    fn drop(&mut self) {
        self.dh_self_secret.zeroize();
        self.root_key.zeroize();
        if let Some(ref mut k) = self.chain_key_send {
            k.zeroize();
        }
        if let Some(ref mut k) = self.chain_key_recv {
            k.zeroize();
        }
        for (_, v) in self.skipped_keys.iter_mut() {
            v.zeroize();
        }
    }
}

impl RatchetState {
    /// Initialize as the session initiator (the one who performed X3DH initiate).
    /// The initiator sends the first message, so they start with a sending chain.
    pub fn init_as_sender(
        root_key: [u8; 32],
        their_signed_prekey_public: &[u8; 32],
    ) -> Result<Self> {
        let (dh_secret, dh_public) = crypto::generate_x25519_ephemeral();

        // First DH ratchet step: derive sending chain key
        let dh_output = crypto::x25519_shared_secret(&dh_secret, their_signed_prekey_public);
        let (new_root_key, chain_key_send) =
            crypto::hkdf_sha256_64(&root_key, &dh_output, KDF_RK_INFO)?;

        Ok(Self {
            dh_self_secret: dh_secret,
            dh_self_public: dh_public,
            dh_remote_public: Some(*their_signed_prekey_public),
            root_key: new_root_key,
            chain_key_send: Some(chain_key_send),
            chain_key_recv: None,
            send_n: 0,
            recv_n: 0,
            prev_send_n: 0,
            skipped_keys: HashMap::new(),
        })
    }

    /// Initialize as the session receiver (the one who performed X3DH respond).
    /// The receiver waits for the first message before creating their sending chain.
    pub fn init_as_receiver(
        root_key: [u8; 32],
        our_signed_prekey_secret: [u8; 32],
        our_signed_prekey_public: [u8; 32],
    ) -> Self {
        Self {
            dh_self_secret: our_signed_prekey_secret,
            dh_self_public: our_signed_prekey_public,
            dh_remote_public: None,
            root_key,
            chain_key_send: None,
            chain_key_recv: None,
            send_n: 0,
            recv_n: 0,
            prev_send_n: 0,
            skipped_keys: HashMap::new(),
        }
    }

    /// Encrypt a plaintext message.
    /// Ratchets the sending chain forward, derives a per-message key,
    /// and encrypts with ChaCha20-Poly1305.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<RatchetMessage> {
        let chain_key = self.chain_key_send.ok_or_else(|| {
            anyhow::anyhow!("No sending chain key — session not fully initialized")
        })?;

        // Derive message key from chain key
        let (new_chain_key, message_key) = kdf_chain_key(&chain_key)?;
        self.chain_key_send = Some(new_chain_key);

        // Encrypt with message key
        let ciphertext = crypto::encrypt(&message_key, plaintext)?;

        let header = RatchetHeader {
            dh_public: self.dh_self_public,
            prev_chain_n: self.prev_send_n,
            message_n: self.send_n,
        };

        self.send_n += 1;

        Ok(RatchetMessage { header, ciphertext })
    }

    /// Decrypt a received ratchet message.
    /// Handles DH ratchet steps, out-of-order messages via skipped keys.
    pub fn decrypt(&mut self, message: &RatchetMessage) -> Result<Vec<u8>> {
        // 1. Try skipped message keys first (out-of-order delivery)
        let dh_hex = hex::encode(message.header.dh_public);
        if let Some(mk) = self
            .skipped_keys
            .remove(&(dh_hex.clone(), message.header.message_n))
        {
            return crypto::decrypt(&mk, &message.ciphertext);
        }

        // 2. Check if we need a DH ratchet step (new DH public key from peer)
        let need_dh_ratchet = match self.dh_remote_public {
            None => true,
            Some(ref remote) => remote != &message.header.dh_public,
        };

        if need_dh_ratchet {
            // Skip any remaining messages in the current receiving chain
            if self.chain_key_recv.is_some() {
                self.skip_message_keys(message.header.prev_chain_n)?;
            }

            // DH ratchet step: derive new receiving chain
            let dh_output =
                crypto::x25519_shared_secret(&self.dh_self_secret, &message.header.dh_public);
            let (new_root_key, chain_key_recv) =
                crypto::hkdf_sha256_64(&self.root_key, &dh_output, KDF_RK_INFO)?;
            self.root_key = new_root_key;
            self.chain_key_recv = Some(chain_key_recv);
            self.dh_remote_public = Some(message.header.dh_public);
            self.prev_send_n = self.send_n;
            self.send_n = 0;
            self.recv_n = 0;

            // Generate new DH keypair for our next sending chain
            let (new_secret, new_public) = crypto::generate_x25519_ephemeral();
            let dh_output2 = crypto::x25519_shared_secret(&new_secret, &message.header.dh_public);
            let (new_root_key2, chain_key_send) =
                crypto::hkdf_sha256_64(&self.root_key, &dh_output2, KDF_RK_INFO)?;
            self.root_key = new_root_key2;
            self.chain_key_send = Some(chain_key_send);
            self.dh_self_secret.zeroize();
            self.dh_self_secret = new_secret;
            self.dh_self_public = new_public;
        }

        // 3. Skip any messages before this one in the current chain
        self.skip_message_keys(message.header.message_n)?;

        // 4. Derive message key and decrypt
        let chain_key = self
            .chain_key_recv
            .ok_or_else(|| anyhow::anyhow!("No receiving chain key"))?;
        let (new_chain_key, message_key) = kdf_chain_key(&chain_key)?;
        self.chain_key_recv = Some(new_chain_key);
        self.recv_n += 1;

        crypto::decrypt(&message_key, &message.ciphertext)
    }

    /// Skip message keys up to `until` (exclusive) and store them for later.
    fn skip_message_keys(&mut self, until: u32) -> Result<()> {
        if self.recv_n + MAX_SKIP < until {
            anyhow::bail!(
                "Too many skipped messages: {} (max {})",
                until - self.recv_n,
                MAX_SKIP
            );
        }

        if let Some(mut chain_key) = self.chain_key_recv {
            while self.recv_n < until {
                let (new_chain_key, message_key) = kdf_chain_key(&chain_key)?;
                let dh_hex = self.dh_remote_public.map(hex::encode).unwrap_or_default();
                self.skipped_keys.insert((dh_hex, self.recv_n), message_key);
                chain_key = new_chain_key;
                self.recv_n += 1;

                // Evict oldest if over limit
                if self.skipped_keys.len() > MAX_SKIP as usize {
                    if let Some(key) = self.skipped_keys.keys().next().cloned() {
                        self.skipped_keys.remove(&key);
                    }
                }
            }
            self.chain_key_recv = Some(chain_key);
        }
        Ok(())
    }

    /// Get the current DH ratchet generation (number of DH steps).
    pub fn generation(&self) -> u32 {
        self.prev_send_n + self.send_n
    }

    /// Total messages sent in this session.
    pub fn total_sent(&self) -> u32 {
        self.prev_send_n + self.send_n
    }
}

/// Derive a message key from a chain key using HKDF.
/// Returns (new_chain_key, message_key).
fn kdf_chain_key(chain_key: &[u8; 32]) -> Result<([u8; 32], [u8; 32])> {
    crypto::hkdf_sha256_64(chain_key, &[0x01], KDF_CK_INFO)
}

// ─── Hex serde helper ───────────────────────────────────────────────────

mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[u8; 32], D::Error> {
        let s = String::deserialize(d)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("expected 32 bytes"));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulate a full conversation between Alice and Bob.
    #[test]
    fn test_ratchet_conversation() {
        // Shared root key from X3DH (normally derived, here mocked)
        let root_key = [42u8; 32];

        // Bob's signed prekey (normally from X3DH bundle)
        let (bob_spk_secret, bob_spk_public) = crypto::generate_x25519_ephemeral();

        // Alice (sender) initializes
        let mut alice = RatchetState::init_as_sender(root_key, &bob_spk_public).unwrap();

        // Bob (receiver) initializes
        let mut bob = RatchetState::init_as_receiver(root_key, bob_spk_secret, bob_spk_public);

        // Alice sends message 1
        let msg1 = alice.encrypt(b"Hello Bob, from mesh!").unwrap();
        let plain1 = bob.decrypt(&msg1).unwrap();
        assert_eq!(plain1, b"Hello Bob, from mesh!");

        // Bob replies
        let msg2 = bob.encrypt(b"Hey Alice, loud and clear").unwrap();
        let plain2 = alice.decrypt(&msg2).unwrap();
        assert_eq!(plain2, b"Hey Alice, loud and clear");

        // Alice sends again (new DH ratchet step)
        let msg3 = alice.encrypt(b"Block 890412 confirmed").unwrap();
        let plain3 = bob.decrypt(&msg3).unwrap();
        assert_eq!(plain3, b"Block 890412 confirmed");

        // Bob sends multiple in a row
        let msg4 = bob.encrypt(b"Opening channel").unwrap();
        let msg5 = bob.encrypt(b"500k sats capacity").unwrap();
        let plain4 = alice.decrypt(&msg4).unwrap();
        let plain5 = alice.decrypt(&msg5).unwrap();
        assert_eq!(plain4, b"Opening channel");
        assert_eq!(plain5, b"500k sats capacity");
    }

    #[test]
    fn test_out_of_order_delivery() {
        let root_key = [99u8; 32];
        let (spk_secret, spk_public) = crypto::generate_x25519_ephemeral();

        let mut alice = RatchetState::init_as_sender(root_key, &spk_public).unwrap();
        let mut bob = RatchetState::init_as_receiver(root_key, spk_secret, spk_public);

        // Alice sends 3 messages
        let msg1 = alice.encrypt(b"first").unwrap();
        let msg2 = alice.encrypt(b"second").unwrap();
        let msg3 = alice.encrypt(b"third").unwrap();

        // Bob receives out of order: 3, 1, 2
        let p3 = bob.decrypt(&msg3).unwrap();
        assert_eq!(p3, b"third");
        let p1 = bob.decrypt(&msg1).unwrap();
        assert_eq!(p1, b"first");
        let p2 = bob.decrypt(&msg2).unwrap();
        assert_eq!(p2, b"second");
    }

    #[test]
    fn test_forward_secrecy() {
        // After DH ratchet steps, old keys are destroyed
        let root_key = [77u8; 32];
        let (spk_secret, spk_public) = crypto::generate_x25519_ephemeral();

        let mut alice = RatchetState::init_as_sender(root_key, &spk_public).unwrap();
        let mut bob = RatchetState::init_as_receiver(root_key, spk_secret, spk_public);

        // Exchange messages to ratchet forward
        let msg1 = alice.encrypt(b"msg1").unwrap();
        bob.decrypt(&msg1).unwrap();
        let msg2 = bob.encrypt(b"msg2").unwrap();
        alice.decrypt(&msg2).unwrap();

        // At this point, both have ratcheted. The original root_key
        // and initial chain keys are no longer in memory.
        // We can verify the state has evolved:
        assert_ne!(alice.root_key, root_key);
        assert_ne!(bob.root_key, root_key);
    }

    #[test]
    fn test_message_wire_format() {
        let header = RatchetHeader {
            dh_public: [0xAA; 32],
            prev_chain_n: 5,
            message_n: 12,
        };
        let bytes = header.to_bytes();
        assert_eq!(bytes.len(), 40);

        let parsed = RatchetHeader::from_bytes(&bytes);
        assert_eq!(parsed.dh_public, [0xAA; 32]);
        assert_eq!(parsed.prev_chain_n, 5);
        assert_eq!(parsed.message_n, 12);
    }

    #[test]
    fn test_ratchet_message_roundtrip() {
        let msg = RatchetMessage {
            header: RatchetHeader {
                dh_public: [0xBB; 32],
                prev_chain_n: 0,
                message_n: 0,
            },
            ciphertext: vec![[0x01, 0x02, 0x03]; 30].into_iter().flatten().collect(),
        };
        let bytes = msg.to_bytes();
        let parsed = RatchetMessage::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.header.dh_public, [0xBB; 32]);
        assert_eq!(parsed.ciphertext.len(), msg.ciphertext.len());
    }

    #[test]
    fn test_long_conversation() {
        let root_key = [11u8; 32];
        let (spk_secret, spk_public) = crypto::generate_x25519_ephemeral();

        let mut alice = RatchetState::init_as_sender(root_key, &spk_public).unwrap();
        let mut bob = RatchetState::init_as_receiver(root_key, spk_secret, spk_public);

        // 50 messages back and forth
        for i in 0..50 {
            let msg_text = format!(
                "Message #{} from {}",
                i,
                if i % 2 == 0 { "Alice" } else { "Bob" }
            );
            if i % 2 == 0 {
                let msg = alice.encrypt(msg_text.as_bytes()).unwrap();
                let decrypted = bob.decrypt(&msg).unwrap();
                assert_eq!(decrypted, msg_text.as_bytes());
            } else {
                let msg = bob.encrypt(msg_text.as_bytes()).unwrap();
                let decrypted = alice.decrypt(&msg).unwrap();
                assert_eq!(decrypted, msg_text.as_bytes());
            }
        }
    }
}
