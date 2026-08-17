//! Cashu token format (NUT-00) — serialization and deserialization.
//!
//! Emits the cashuA (V3) token format:
//!   cashuA<base64url_encoded_json>
//!
//! Token JSON structure:
//! {
//!   "token": [{ "mint": "<url>", "proofs": [{ "amount": u64, "id": "<keyset>", "secret": "<str>", "C": "<hex>" }] }],
//!   "memo": "<optional>"
//! }
//!
//! Also accepts (decode-only) the cashuB (V4) CBOR format many wallets emit
//! by default now:
//!   cashuB<base64url_encoded_cbor>
//! CBOR map keys are the spec's single-letter names (t/i/p/a/s/c/m/u/d/w),
//! not the JSON names above. `i` (keyset id) and `c` (signature) are raw
//! bytes on the wire; we hex-encode them into `Proof` to match the V3
//! convention so the rest of the wallet doesn't need to know which version
//! a token arrived in.

use anyhow::{Context, Result};
use bitcoin::secp256k1::PublicKey;
// Protocol types from the reference implementation (`cashu`, the crate CDK
// itself is built on). Used for the parts of NUT-00/02 that move with the
// spec — token parsing and keyset ids — while the structs below stay ours
// because they are also the on-disk format (see docs/cashu-cdk-migration-plan.md).
use cashu::nuts::nut02::{
    Id as CdkId, KeySetInfo as CdkKeySetInfo, ShortKeysetId as CdkShortKeysetId,
};
use serde::{Deserialize, Serialize};

/// Prefix for V3 (JSON) tokens.
const CASHU_A_PREFIX: &str = "cashuA";
/// Prefix for V4 (CBOR) tokens.
const CASHU_B_PREFIX: &str = "cashuB";

/// Raw CBOR shape of a V4 proof — field names are the spec's map keys.
#[derive(Debug, Deserialize)]
struct ProofV4 {
    a: u64,
    s: String,
    #[serde(with = "serde_bytes")]
    c: Vec<u8>,
    // DLEQ proof ("d") and witness ("w") aren't verified or stored by this
    // wallet; accept and discard them rather than fail on the field.
}

/// Raw CBOR shape of a V4 token entry (one keyset's worth of proofs).
#[derive(Debug, Deserialize)]
struct TokenEntryV4 {
    #[serde(with = "serde_bytes")]
    i: Vec<u8>,
    p: Vec<ProofV4>,
}

/// Raw CBOR shape of a full V4 token — single mint per token, unlike V3.
#[derive(Debug, Deserialize)]
struct TokenV4 {
    t: Vec<TokenEntryV4>,
    m: String,
    #[serde(default)]
    u: Option<String>,
    #[serde(default, rename = "d")]
    memo: Option<String>,
}

/// A single Cashu proof (a signed token for a specific denomination).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proof {
    /// Denomination in the mint's unit (sats).
    pub amount: u64,
    /// Keyset ID (hex string, e.g. "009a1f293253e41e").
    pub id: String,
    /// The secret (random hex string or NUT-10 structured secret).
    pub secret: String,
    /// The unblinded signature C as hex-encoded compressed public key.
    #[serde(rename = "C")]
    pub c: String,
}

impl Proof {
    /// Parse the C field as a secp256k1 PublicKey.
    pub fn c_as_pubkey(&self) -> Result<PublicKey> {
        let bytes = hex::decode(&self.c).context("Invalid hex in proof C field")?;
        PublicKey::from_slice(&bytes).context("Invalid public key in proof C field")
    }
}

/// A group of proofs from a single mint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenEntry {
    /// Mint URL.
    pub mint: String,
    /// Proofs from this mint.
    pub proofs: Vec<Proof>,
}

/// The full cashuA token envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashuToken {
    /// Token entries grouped by mint.
    pub token: Vec<TokenEntry>,
    /// Optional memo.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memo: Option<String>,
    /// Optional unit (e.g. "sat").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
}

impl CashuToken {
    /// Create a new token with proofs from a single mint.
    pub fn new(mint_url: &str, proofs: Vec<Proof>) -> Self {
        Self {
            token: vec![TokenEntry {
                mint: mint_url.to_string(),
                proofs,
            }],
            memo: None,
            unit: Some("sat".to_string()),
        }
    }

    /// Total value of all proofs across all mints.
    pub fn total_amount(&self) -> u64 {
        self.token
            .iter()
            .flat_map(|e| &e.proofs)
            .map(|p| p.amount)
            .sum()
    }

    /// All proofs across all mint entries.
    pub fn all_proofs(&self) -> Vec<&Proof> {
        self.token.iter().flat_map(|e| &e.proofs).collect()
    }

    /// All unique mint URLs in this token.
    pub fn mint_urls(&self) -> Vec<&str> {
        self.token.iter().map(|e| e.mint.as_str()).collect()
    }

    /// Encode as a cashuA token string.
    pub fn serialize(&self) -> Result<String> {
        let json = serde_json::to_string(self).context("Failed to serialize token JSON")?;
        use base64::Engine;
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes());
        Ok(format!("{}{}", CASHU_A_PREFIX, encoded))
    }

    /// Decode a cashuA (V3 JSON) or cashuB (V4 CBOR) token string.
    pub fn deserialize(token_str: &str) -> Result<Self> {
        if let Some(payload) = token_str.strip_prefix(CASHU_B_PREFIX) {
            return Self::deserialize_v4(payload);
        }

        let payload = token_str.strip_prefix(CASHU_A_PREFIX).ok_or_else(|| {
            anyhow::anyhow!(
                "Token must start with '{}' or '{}'",
                CASHU_A_PREFIX,
                CASHU_B_PREFIX
            )
        })?;

        let decoded = decode_token_base64(payload).context("Invalid base64 in cashuA token")?;
        let json_str = String::from_utf8(decoded).context("Invalid UTF-8 in decoded token")?;
        let token: CashuToken =
            serde_json::from_str(&json_str).context("Invalid JSON in cashuA token")?;

        token.validate()?;
        Ok(token)
    }

    /// Decode a cashuB (V4 CBOR) token payload (prefix already stripped).
    fn deserialize_v4(payload: &str) -> Result<Self> {
        let decoded = decode_token_base64(payload).context("Invalid base64 in cashuB token")?;

        let v4: TokenV4 =
            ciborium::from_reader(decoded.as_slice()).context("Invalid CBOR in cashuB token")?;

        let proofs =
            v4.t.into_iter()
                .flat_map(|entry| {
                    let keyset_id = hex::encode(&entry.i);
                    entry.p.into_iter().map(move |p| Proof {
                        amount: p.a,
                        id: keyset_id.clone(),
                        secret: p.s,
                        c: hex::encode(&p.c),
                    })
                })
                .collect();

        let token = CashuToken {
            token: vec![TokenEntry { mint: v4.m, proofs }],
            memo: v4.memo,
            unit: v4.u,
        };
        token.validate()?;
        Ok(token)
    }

    /// Structural validation shared by both token versions.
    fn validate(&self) -> Result<()> {
        if self.token.is_empty() {
            anyhow::bail!("Token has no entries");
        }
        for entry in &self.token {
            if entry.mint.is_empty() {
                anyhow::bail!("Token entry has empty mint URL");
            }
            if entry.proofs.is_empty() {
                anyhow::bail!("Token entry has no proofs");
            }
            for proof in &entry.proofs {
                if proof.amount == 0 {
                    anyhow::bail!("Proof has zero amount");
                }
                if proof.secret.is_empty() {
                    anyhow::bail!("Proof has empty secret");
                }
                if proof.c.is_empty() {
                    anyhow::bail!("Proof has empty C");
                }
                validate_keyset_id(&proof.id)?;
            }
        }
        Ok(())
    }
}

/// Resolve a token's (possibly short) keyset id against the mint's keyset
/// list, using the reference implementation's NUT-02 rules.
///
/// A v1 id is complete at 8 bytes; a v2 id is 33 bytes and may legitimately
/// travel in a token as a shorter prefix, which only the mint's keyset list
/// can expand. Upstream `Id::from_short_keyset_id` implements exactly that,
/// including the "8 bytes but `0x01`-versioned" case that a wallet written
/// against the old format produces (framework-pt, 2026-08-17).
///
/// Returns the full hex id to send to the mint, or `None` when the id is
/// already complete or cannot be resolved — the caller passes those through
/// untouched so the mint's own error still reaches the operator.
pub fn resolve_keyset_id(id_hex: &str, mint_keysets: &[CdkKeySetInfo]) -> Option<String> {
    let bytes = hex::decode(id_hex).ok()?;
    let short = CdkShortKeysetId::from_bytes(&bytes).ok()?;
    let full = CdkId::from_short_keyset_id(&short, mint_keysets).ok()?;
    let full_hex = hex::encode(full.to_bytes());
    (full_hex != id_hex).then_some(full_hex)
}

/// NUT-02 keyset ID: hex for either 8 bytes (v1, the `00…` short form) or
/// 33 bytes (v2, version-byte + hash).
///
/// Checked when a token is decoded rather than left to the mint. Forwarding
/// an out-of-spec id produced a swap the mint rejected with a bare
/// `422 Unprocessable Entity`, which reached the operator as "check server
/// logs" with nothing actionable in the UI (framework-pt, 2026-08-17,
/// mint.minibits.cash: `inputs[0].id: NUT02: ID length invalid`). A local
/// check can say which keyset format the token uses and that this wallet
/// cannot spend it, before any network call.
fn validate_keyset_id(id: &str) -> Result<()> {
    let bytes = hex::decode(id).map_err(|_| {
        anyhow::anyhow!(
            "Token uses a keyset id that is not hex ({id:?}) — this wallet supports \
             NUT-02 v1 (8-byte) and v2 (33-byte) hex keyset ids"
        )
    })?;
    match bytes.len() {
        // 8 bytes is v1's whole id, and also what a wallet that predates v2
        // leaves behind when it truncates one. Both are accepted here; the
        // truncated case is repaired against the mint's keyset list at swap
        // time (see MintClient::resolve_truncated_keyset_ids).
        8 | 33 => Ok(()),
        n => anyhow::bail!(
            "Token uses an unsupported keyset id format: {n}-byte id {id:?}. NUT-02 \
             defines 8-byte (v1) and 33-byte (v2) ids; the mint will reject a swap \
             carrying this one"
        ),
    }
}

/// Is this the first 8 bytes of a NUT-02 **v2** keyset id rather than a
/// complete v1 one?
///
/// A v1 id is 8 bytes beginning with the version byte `0x00`; a v2 id is 33
/// bytes beginning with `0x01`. So an 8-byte id that starts with `0x01` is a
/// v2 id some wallet cut to the old length — it cannot be spent as-is, but
/// the full id can be recovered from the mint because the short form is a
/// prefix of it.
pub fn is_truncated_v2_keyset_id(id: &str) -> bool {
    id.len() == 16 && id.starts_with("01") && hex::decode(id).is_ok()
}

/// Decode a token's base64 payload, trying URL-safe-no-pad first (the spec
/// default) and falling back to other alphabets some implementations use.
fn decode_token_base64(payload: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(payload))
}

/// Keyset info returned by a mint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeysetInfo {
    pub id: String,
    pub unit: String,
    pub active: bool,
    /// NUT-02 input fee, in parts-per-thousand of a proof. A mint charges
    /// this per *input* on a swap/melt; zero at fee-free mints, which is why
    /// ignoring it went unnoticed against Minibits.
    #[serde(default)]
    pub input_fee_ppk: u64,
}

/// NUT-02 swap fee for a set of inputs: the summed per-proof parts-per-
/// thousand, rounded **up** to whole units. Inputs whose keyset the mint
/// didn't list contribute nothing — the mint is the authority, and guessing
/// high would silently burn the sender's coins.
pub fn swap_fee_for(proofs: &[Proof], keysets: &[KeysetInfo]) -> u64 {
    let ppk: u64 = proofs
        .iter()
        .map(|p| {
            keysets
                .iter()
                .find(|k| k.id == p.id)
                .map(|k| k.input_fee_ppk)
                .unwrap_or(0)
        })
        .sum();
    ppk.div_ceil(1000)
}

/// Mint keyset: maps denomination amounts to public keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintKeyset {
    pub id: String,
    /// Currency unit this keyset signs for ("sat", "usd", "eur", "msat"…).
    ///
    /// Defaulted rather than required: a mint that omits it is sat-only in
    /// practice, and refusing to parse would break wallets against mints that
    /// predate multi-unit support.
    #[serde(default = "default_unit")]
    pub unit: String,
    /// Whether the mint will still sign with this keyset.
    #[serde(default = "default_true")]
    pub active: bool,
    /// Map of amount (as string) to hex-encoded public key.
    pub keys: std::collections::HashMap<String, String>,
}

fn default_unit() -> String {
    "sat".to_string()
}

fn default_true() -> bool {
    true
}

impl MintKeyset {
    /// Get the mint's public key for a given denomination amount.
    pub fn key_for_amount(&self, amount: u64) -> Result<PublicKey> {
        let amount_str = amount.to_string();
        let hex_key = self
            .keys
            .get(&amount_str)
            .ok_or_else(|| anyhow::anyhow!("No key for amount {} in keyset {}", amount, self.id))?;
        let bytes = hex::decode(hex_key).context("Invalid hex in mint pubkey")?;
        PublicKey::from_slice(&bytes).context("Invalid pubkey in mint keyset")
    }
}

/// Blinded message sent to the mint during mint/swap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindedMessageRequest {
    /// Amount for this output.
    pub amount: u64,
    /// Keyset ID to use.
    pub id: String,
    /// Blinded secret B_ as hex-encoded compressed pubkey.
    #[serde(rename = "B_")]
    pub b_prime: String,
}

/// Blind signature returned by the mint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlindSignature {
    /// Amount signed.
    pub amount: u64,
    /// Keyset ID.
    pub id: String,
    /// Blind signature C_ as hex-encoded compressed pubkey.
    #[serde(rename = "C_")]
    pub c_prime: String,
}

impl BlindSignature {
    /// Parse C_ as a secp256k1 PublicKey.
    pub fn c_prime_as_pubkey(&self) -> Result<PublicKey> {
        let bytes = hex::decode(&self.c_prime).context("Invalid hex in blind signature C_")?;
        PublicKey::from_slice(&bytes).context("Invalid pubkey in blind signature C_")
    }
}

/// Split a target amount into powers of 2 (Cashu denomination scheme).
/// E.g., 13 -> [1, 4, 8]
pub fn amount_to_denominations(mut amount: u64) -> Vec<u64> {
    let mut denoms = Vec::new();
    let mut bit = 0;
    while amount > 0 {
        if amount & 1 == 1 {
            denoms.push(1u64 << bit);
        }
        amount >>= 1;
        bit += 1;
    }
    denoms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let token = CashuToken {
            token: vec![TokenEntry {
                mint: "http://127.0.0.1:8175".to_string(),
                proofs: vec![Proof {
                    amount: 8,
                    id: "009a1f293253e41e".to_string(),
                    secret: "abcdef1234567890".to_string(),
                    c: "02a9acc1e48c25eeeb9289b5031cc57da9fe72f3fe2861d94ec4da0e7f6c2b4e24"
                        .to_string(),
                }],
            }],
            memo: Some("test token".to_string()),
            unit: Some("sat".to_string()),
        };

        let encoded = token.serialize().unwrap();
        assert!(encoded.starts_with("cashuA"));

        let decoded = CashuToken::deserialize(&encoded).unwrap();
        assert_eq!(decoded.total_amount(), 8);
        assert_eq!(decoded.token[0].mint, "http://127.0.0.1:8175");
        assert_eq!(decoded.token[0].proofs[0].secret, "abcdef1234567890");
        assert_eq!(decoded.memo, Some("test token".to_string()));
    }

    #[test]
    fn test_total_amount_multi_proof() {
        let token = CashuToken {
            token: vec![TokenEntry {
                mint: "http://mint".to_string(),
                proofs: vec![
                    Proof {
                        amount: 1,
                        id: "id1".into(),
                        secret: "s1".into(),
                        c: "02a9acc1e48c25eeeb9289b5031cc57da9fe72f3fe2861d94ec4da0e7f6c2b4e24"
                            .into(),
                    },
                    Proof {
                        amount: 4,
                        id: "id1".into(),
                        secret: "s2".into(),
                        c: "02a9acc1e48c25eeeb9289b5031cc57da9fe72f3fe2861d94ec4da0e7f6c2b4e24"
                            .into(),
                    },
                    Proof {
                        amount: 8,
                        id: "id1".into(),
                        secret: "s3".into(),
                        c: "02a9acc1e48c25eeeb9289b5031cc57da9fe72f3fe2861d94ec4da0e7f6c2b4e24"
                            .into(),
                    },
                ],
            }],
            memo: None,
            unit: None,
        };
        assert_eq!(token.total_amount(), 13);
    }

    #[test]
    fn test_deserialize_rejects_empty_token() {
        let bad = CashuToken {
            token: vec![],
            memo: None,
            unit: None,
        };
        let encoded = bad.serialize().unwrap();
        let result = CashuToken::deserialize(&encoded);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_rejects_unknown_prefix() {
        let result = CashuToken::deserialize("cashuZabc123");
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_rejects_malformed_v4_cbor() {
        let result = CashuToken::deserialize("cashuBabc123");
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_v4_cbor_token() {
        // Hand-built (not via our own encoder) to verify we actually match
        // the NUT-00 V4 wire format: single-letter CBOR map keys, raw-byte
        // keyset id ("i") and signature ("c").
        use base64::Engine;
        use ciborium::value::Value;

        let keyset_id = vec![0x00u8, 0x9a, 0x1f, 0x29, 0x32, 0x53, 0xe4, 0x1e];
        let sig = hex::decode("02a9acc1e48c25eeeb9289b5031cc57da9fe72f3fe2861d94ec4da0e7f6c2b4e24")
            .unwrap();

        let proof = Value::Map(vec![
            (Value::from("a"), Value::from(8u64)),
            (Value::from("s"), Value::from("abcdef1234567890")),
            (Value::from("c"), Value::from(sig.clone())),
        ]);
        let entry = Value::Map(vec![
            (Value::from("i"), Value::from(keyset_id.clone())),
            (Value::from("p"), Value::Array(vec![proof])),
        ]);
        let token = Value::Map(vec![
            (Value::from("t"), Value::Array(vec![entry])),
            (Value::from("m"), Value::from("http://127.0.0.1:8175")),
            (Value::from("u"), Value::from("sat")),
            (Value::from("d"), Value::from("test token")),
        ]);

        let mut buf = Vec::new();
        ciborium::into_writer(&token, &mut buf).unwrap();
        let encoded = format!(
            "cashuB{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&buf)
        );

        let decoded = CashuToken::deserialize(&encoded).unwrap();
        assert_eq!(decoded.total_amount(), 8);
        assert_eq!(decoded.token[0].mint, "http://127.0.0.1:8175");
        assert_eq!(decoded.token[0].proofs[0].secret, "abcdef1234567890");
        assert_eq!(decoded.token[0].proofs[0].id, hex::encode(&keyset_id));
        assert_eq!(decoded.token[0].proofs[0].c, hex::encode(&sig));
        assert_eq!(decoded.memo, Some("test token".to_string()));
    }

    #[test]
    fn test_amount_to_denominations() {
        assert_eq!(amount_to_denominations(0), Vec::<u64>::new());
        assert_eq!(amount_to_denominations(1), vec![1]);
        assert_eq!(amount_to_denominations(13), vec![1, 4, 8]);
        assert_eq!(amount_to_denominations(21), vec![1, 4, 16]);
        assert_eq!(amount_to_denominations(64), vec![64]);
        assert_eq!(
            amount_to_denominations(255),
            vec![1, 2, 4, 8, 16, 32, 64, 128]
        );
    }

    #[test]
    fn test_amount_to_denominations_large() {
        let denoms = amount_to_denominations(1_000_000);
        let sum: u64 = denoms.iter().sum();
        assert_eq!(sum, 1_000_000);
    }

    #[test]
    fn test_proof_c_as_pubkey() {
        let proof = Proof {
            amount: 1,
            id: "test".into(),
            secret: "s".into(),
            // Generator point G of secp256k1, compressed form. Always a
            // valid pubkey, so c_as_pubkey() must succeed.
            c: "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798".to_string(),
        };
        assert!(proof.c_as_pubkey().is_ok());
    }

    #[test]
    fn keyset_ids_of_both_nut02_versions_are_accepted() {
        // v1: 8 bytes / 16 hex chars.
        assert!(validate_keyset_id("009a1f293253e41e").is_ok());
        // v2: 33 bytes / 66 hex chars (version byte + 32-byte hash).
        let v2 = format!("01{}", "ab".repeat(32));
        assert!(validate_keyset_id(&v2).is_ok());
    }

    #[test]
    fn a_truncated_v2_keyset_id_is_recognised_as_repairable() {
        // The real case: a Minibits token carried the first 8 bytes of the
        // mint's 33-byte v2 keyset id (2026-08-17).
        let short = "01fc0ec0e59cd6fa";
        let full = "01fc0ec0e59cd6fa01b7a88f8cd77fce81fd1e64bca67d752e984992b7a3c3a821";
        assert!(is_truncated_v2_keyset_id(short));
        assert!(full.starts_with(short), "short form must prefix the full id");
        // It must survive token validation so the swap path can repair it,
        // rather than being rejected as malformed.
        assert!(validate_keyset_id(short).is_ok());

        // A genuine v1 id (version byte 00) is not "truncated".
        assert!(!is_truncated_v2_keyset_id("009a1f293253e41e"));
        // Neither is a complete v2 id.
        assert!(!is_truncated_v2_keyset_id(full));
    }

    #[test]
    fn an_out_of_spec_keyset_id_is_rejected_locally_with_its_length() {
        // 9 bytes — what a legacy base64 keyset id decodes to, and neither
        // NUT-02 length. The mint answers this with a bare 422, so the
        // message has to come from here.
        let err = validate_keyset_id("00112233445566778899")
            .expect_err("9-byte keyset id must be rejected");
        let msg = err.to_string();
        assert!(msg.contains("10-byte") || msg.contains("unsupported keyset id"), "{msg}");

        // Non-hex ids (the original base64 keyset format) are named as such
        // rather than reported as a length problem.
        let err = validate_keyset_id("I2yN+iRYfkzT").expect_err("base64 id must be rejected");
        assert!(err.to_string().contains("not hex"), "{err}");
    }

    #[test]
    fn a_token_carrying_an_unsupported_keyset_id_fails_to_decode() {
        let token = CashuToken {
            token: vec![TokenEntry {
                mint: "https://mint.example.com".to_string(),
                proofs: vec![Proof {
                    amount: 1,
                    id: "I2yN+iRYfkzT".to_string(),
                    secret: "s".to_string(),
                    c: "02".to_string(),
                }],
            }],
            memo: None,
            unit: None,
        };
        // The whole point: this must fail here, not at the mint.
        assert!(token.validate().is_err());
    }
}
