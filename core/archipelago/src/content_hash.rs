//! Content hashing for the DHT distribution plan's *integrity & addressing*
//! tier (`docs/dht-distribution-design.md` §4).
//!
//! SHA-256 is the incumbent: it keys `blobs.rs` and verifies OTA components
//! today. BLAKE3 is introduced **alongside** it because iroh-blobs addresses
//! and *range-verifies* content by BLAKE3 — essential for resumable downloads
//! and HLS streaming. During the migration window both may be present; SHA-256
//! stays mandatory and BLAKE3 is verified when supplied.
//!
//! Digests are written multihash-style as `"<alg>:<hex>"`, e.g.
//! `"blake3:ab12…"` / `"sha256:cd34…"`, matching the app-catalog `digest` field.
//! Both algorithms emit 32-byte (64-hex-char) digests.

use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};

const DIGEST_LEN: usize = 32;

/// Supported content-hash algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlg {
    Sha256,
    Blake3,
}

impl HashAlg {
    pub fn as_str(self) -> &'static str {
        match self {
            HashAlg::Sha256 => "sha256",
            HashAlg::Blake3 => "blake3",
        }
    }
}

/// Hex-encoded SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Hex-encoded BLAKE3 of `bytes`.
pub fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// A parsed `"<alg>:<hex>"` content digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentDigest {
    pub alg: HashAlg,
    /// Lowercase hex, validated to the algorithm's length.
    pub hex: String,
}

impl ContentDigest {
    /// Parse a multihash-style `"<alg>:<hex>"` string.
    pub fn parse(s: &str) -> Result<Self> {
        let (alg_part, hex_part) = s
            .split_once(':')
            .ok_or_else(|| anyhow!("digest must be '<alg>:<hex>', got: {}", s))?;
        let alg = match alg_part {
            "sha256" => HashAlg::Sha256,
            "blake3" => HashAlg::Blake3,
            other => bail!("unsupported hash algorithm: {}", other),
        };
        let raw = hex::decode(hex_part).context("digest hex is invalid")?;
        if raw.len() != DIGEST_LEN {
            bail!(
                "{} digest must be {} bytes, got {}",
                alg.as_str(),
                DIGEST_LEN,
                raw.len()
            );
        }
        Ok(Self {
            alg,
            hex: hex_part.to_ascii_lowercase(),
        })
    }

    /// Compute the digest of `bytes` under this digest's algorithm.
    pub fn compute_hex(&self, bytes: &[u8]) -> String {
        match self.alg {
            HashAlg::Sha256 => sha256_hex(bytes),
            HashAlg::Blake3 => blake3_hex(bytes),
        }
    }

    /// Verify `bytes` hash to this digest. Errors (does not panic) on mismatch.
    pub fn verify(&self, bytes: &[u8]) -> Result<()> {
        let actual = self.compute_hex(bytes);
        if actual.eq_ignore_ascii_case(&self.hex) {
            Ok(())
        } else {
            bail!(
                "{} mismatch: expected {}, got {}",
                self.alg.as_str(),
                self.hex,
                actual
            )
        }
    }
}

impl std::fmt::Display for ContentDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.alg.as_str(), self.hex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_lengths_are_32_bytes() {
        assert_eq!(sha256_hex(b"hi").len(), 64);
        assert_eq!(blake3_hex(b"hi").len(), 64);
    }

    #[test]
    fn blake3_known_answer() {
        // BLAKE3 of the empty input — RFC/reference vector.
        assert_eq!(
            blake3_hex(b""),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn parse_roundtrip() {
        let d = ContentDigest::parse(&format!("blake3:{}", blake3_hex(b"x"))).unwrap();
        assert_eq!(d.alg, HashAlg::Blake3);
        assert_eq!(d.to_string(), format!("blake3:{}", blake3_hex(b"x")));
    }

    #[test]
    fn verify_accepts_and_rejects() {
        let d = ContentDigest::parse(&format!("sha256:{}", sha256_hex(b"payload"))).unwrap();
        assert!(d.verify(b"payload").is_ok());
        assert!(d.verify(b"tampered").is_err());
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert!(ContentDigest::parse("nocolon").is_err());
        assert!(ContentDigest::parse("md5:abcd").is_err());
        assert!(ContentDigest::parse("blake3:nothex").is_err());
        assert!(ContentDigest::parse("blake3:ab").is_err()); // too short
    }
}
