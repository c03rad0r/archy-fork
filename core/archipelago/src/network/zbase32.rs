//! z-base-32 encoding, as used by `did:dht` identifiers.
//!
//! [z-base-32](https://philzimmermann.com/docs/human-oriented-base-32-encoding.txt)
//! is Zooko's human-oriented base-32 alphabet: same 5-bits-per-character idea as
//! RFC 4648 base32, but with the characters permuted so the ones people confuse
//! (`0`/`O`, `1`/`l`/`I`, `2`/`Z`, `v`/`u`) are either absent or arranged to
//! minimise transcription errors, and with no `=` padding.
//!
//! # Why this exists rather than a crate
//!
//! This replaces the `zbase32` crate, which is **LGPL-3.0+** — the only hard
//! copyleft dependency in the Rust graph and a blocker for the MIT release
//! (`docs/LICENSE-COMPLIANCE-AUDIT.md` §2). Statically linking LGPL code into a
//! Rust binary obliges us to ship relinkable objects, which is impractical for
//! a node image. The encoding itself is an alphabet substitution over a bit
//! stream, so an original implementation is a few dozen lines and adds no
//! dependency at all.
//!
//! # Compatibility
//!
//! Output is **byte-identical** to `zbase32 0.1.2`'s `encode_full_bytes` /
//! `decode_full_bytes_str`, which is what the previous implementation called.
//! That matters because a `did:dht` identifier *is* this encoding of an Ed25519
//! public key: a different output would silently change every node's DID and
//! break already-published DHT records. The tests below pin the crate's own
//! documented vectors, the canonical vectors from Zimmermann's spec, and
//! several 32-byte keys.
//!
//! # Bit layout
//!
//! Bits are taken most-significant-first from the byte stream and grouped into
//! 5-bit chunks. When the bit count is not a multiple of 5 the final chunk is
//! padded on the right (low side) with zero bits. A 32-byte key is 256 bits →
//! 52 characters (260 bits), so the last character carries 4 padding bits.

/// The z-base-32 alphabet. Index = 5-bit value.
const ALPHABET: &[u8; 32] = b"ybndrfg8ejkmcpqxot1uwisza345h769";

/// Reverse of [`ALPHABET`]: ASCII byte → 5-bit value, `None` if not a digit.
/// Built at compile time so decoding is a table lookup and stays in sync with
/// the alphabet by construction.
const DECODE_TABLE: [Option<u8>; 256] = {
    let mut table = [None; 256];
    let mut i = 0;
    while i < 32 {
        table[ALPHABET[i] as usize] = Some(i as u8);
        i += 1;
    }
    table
};

/// Encode every bit of `data` as z-base-32.
///
/// Equivalent to the `zbase32` crate's `encode_full_bytes`.
pub fn encode_full_bytes(data: &[u8]) -> String {
    let bits = data.len() * 8;
    // ceil(bits / 5)
    let out_len = bits.div_ceil(5);
    let mut out = String::with_capacity(out_len);

    // `acc` holds the not-yet-emitted low `acc_bits` bits, MSB-first.
    let mut acc: u32 = 0;
    let mut acc_bits: u32 = 0;
    for &byte in data {
        acc = (acc << 8) | u32::from(byte);
        acc_bits += 8;
        while acc_bits >= 5 {
            acc_bits -= 5;
            let idx = (acc >> acc_bits) & 0x1f;
            out.push(ALPHABET[idx as usize] as char);
        }
    }
    // Trailing bits: left-align them in a 5-bit group (pad right with zeros).
    if acc_bits > 0 {
        let idx = (acc << (5 - acc_bits)) & 0x1f;
        out.push(ALPHABET[idx as usize] as char);
    }

    debug_assert_eq!(out.len(), out_len);
    out
}

/// Decode a z-base-32 string, keeping only whole bytes.
///
/// Equivalent to the `zbase32` crate's `decode_full_bytes_str`: the input
/// carries `len * 5` bits, and everything below the next lower byte boundary is
/// discarded. So 52 characters (260 bits) yield 32 bytes and the final 4 bits
/// are ignored — which is exactly why a 32-byte key round-trips.
///
/// Returns `Err` with the offending character if the input is not z-base-32.
pub fn decode_full_bytes_str(s: &str) -> Result<Vec<u8>, String> {
    let total_bits = s.len() * 5;
    let keep_bits = total_bits / 8 * 8;
    let mut out = Vec::with_capacity(keep_bits / 8);

    let mut acc: u32 = 0;
    let mut acc_bits: u32 = 0;
    let mut emitted_bits = 0usize;
    for ch in s.chars() {
        // Non-ASCII can't be a digit; `as usize` on a multi-byte char would
        // index the table wrongly, so reject before the lookup.
        let value = u8::try_from(ch as u32)
            .ok()
            .and_then(|b| DECODE_TABLE[b as usize])
            .ok_or_else(|| format!("not a z-base-32 digit: {ch:?}"))?;
        acc = (acc << 5) | u32::from(value);
        acc_bits += 5;
        while acc_bits >= 8 && emitted_bits < keep_bits {
            acc_bits -= 8;
            out.push(((acc >> acc_bits) & 0xff) as u8);
            emitted_bits += 8;
        }
    }

    debug_assert_eq!(out.len(), keep_bits / 8);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three doc-test vectors from `zbase32 0.1.2` itself. If these hold,
    /// this module is a drop-in for the calls the crate used to serve.
    #[test]
    fn matches_the_replaced_crates_own_doctests() {
        assert_eq!(
            encode_full_bytes("Just an arbitrary sentence.".as_bytes()),
            "jj4zg7bycfznyam1cjwzehubqjh1yh5fp34gk5udcwzy"
        );
        assert_eq!(decode_full_bytes_str("qb1ze3m1").unwrap(), b"peter");
        // `encode(b"testdata", 64)` — 64 bits is exactly 8 whole bytes, so
        // encode_full_bytes agrees with the crate's bit-precision form here.
        assert_eq!(encode_full_bytes(b"testdata"), "qt1zg7drcf4gn");
    }

    /// Canonical vectors from Zimmermann's z-base-32 spec (the whole-byte
    /// subset — the spec's sub-byte cases exercise an API we deliberately
    /// don't expose).
    #[test]
    fn matches_the_spec_vectors() {
        assert_eq!(encode_full_bytes(&[0xf0, 0xbf, 0xc7]), "6n9hq");
        assert_eq!(encode_full_bytes(&[0xd4, 0x7a, 0x04]), "4t7ye");
    }

    /// A `did:dht` identifier is this encoding of a 32-byte Ed25519 key, so
    /// these pin the exact strings that must not drift. Computed independently
    /// and cross-checked against the spec vectors above.
    #[test]
    fn known_answers_for_32_byte_keys() {
        let seq: Vec<u8> = (0u8..32).collect();
        assert_eq!(
            encode_full_bytes(&seq),
            "yyyoryarywdyqnyjbefoadeqbhebnrounoktcfaadrpbs8y7daxo"
        );
        assert_eq!(
            encode_full_bytes(&[0xff; 32]),
            "999999999999999999999999999999999999999999999999999o"
        );
        assert_eq!(
            encode_full_bytes(&[0x00; 32]),
            "yyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyyy"
        );
        // The node Ed25519 public key derived from seed.rs's TEST_MNEMONIC.
        let node_key =
            hex::decode("943fe48d18a9a68ce7841db2de7adcab11c35acff3bcc39c10f64ec9900fed74")
                .unwrap();
        assert_eq!(
            encode_full_bytes(&node_key),
            "1o96jdeaigue33hrds3ph6shicehgssx6q6c88yo638curyx7i4y"
        );
    }

    #[test]
    fn a_32_byte_key_is_52_chars_and_round_trips() {
        for seed in 0u8..64 {
            let key: Vec<u8> = (0u8..32)
                .map(|i| i.wrapping_mul(7).wrapping_add(seed))
                .collect();
            let encoded = encode_full_bytes(&key);
            assert_eq!(encoded.len(), 52, "256 bits must encode to 52 characters");
            assert_eq!(decode_full_bytes_str(&encoded).unwrap(), key);
        }
    }

    #[test]
    fn round_trips_every_length_up_to_a_block() {
        for len in 0..40usize {
            let data: Vec<u8> = (0..len)
                .map(|i| (i as u8).wrapping_mul(31) ^ 0x5a)
                .collect();
            let encoded = encode_full_bytes(&data);
            // decode_full_bytes only recovers whole bytes, and encoding N bytes
            // produces ceil(8N/5) chars which always carry at least 8N bits.
            assert_eq!(decode_full_bytes_str(&encoded).unwrap(), data, "len {len}");
        }
    }

    #[test]
    fn empty_input() {
        assert_eq!(encode_full_bytes(&[]), "");
        assert_eq!(decode_full_bytes_str("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn rejects_non_alphabet_characters() {
        // `l`, `v`, `2`, `0` are deliberately absent from the z-base-32
        // alphabet — they're the characters it exists to avoid.
        for bad in ["l", "v", "2", "0", "A", "yyy!", "yyyé"] {
            assert!(
                decode_full_bytes_str(bad).is_err(),
                "{bad:?} must not decode"
            );
        }
    }

    /// The alphabet must stay a permutation of 32 distinct ASCII characters, or
    /// the compile-time decode table silently loses entries.
    #[test]
    fn alphabet_is_32_distinct_ascii_characters() {
        let mut seen = std::collections::HashSet::new();
        for &c in ALPHABET.iter() {
            assert!(c.is_ascii(), "non-ASCII in alphabet");
            assert!(seen.insert(c), "duplicate character in alphabet: {c}");
        }
        assert_eq!(seen.len(), 32);
        for (i, &c) in ALPHABET.iter().enumerate() {
            assert_eq!(DECODE_TABLE[c as usize], Some(i as u8));
        }
    }
}
