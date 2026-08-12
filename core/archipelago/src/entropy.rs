//! Entropy policy for key generation — the KEY-05 mechanism module.
//!
//! Three independent controls live here, each closing a different half of the
//! same structural defect recorded as **F-10a** in
//! `docs/security/ENTROPY-SEED-AUDIT-2026-07-31.md` and classified per-site in
//! `docs/security/KEY-05-ENTROPY-ENFORCEMENT.md`:
//!
//! - **The sealed allowlist** ([`KeyGenRng`], layer *a*). A key-generation seam
//!   typed `R: KeyGenRng` can only be driven by a type this module blessed. The
//!   marker's supertrait lives in a private module, so membership is unnameable
//!   — and therefore unaddable — from any other module of this crate, and from
//!   any downstream crate were this binary ever split into a library.
//! - **The degenerate-entropy predicate** ([`is_degenerate`], [`draw_key_bytes`],
//!   layer *d*). Key material and AEAD nonces are inspected before they are
//!   used, and a draw that is all-zero, all-identical or a wrapping ±1 counter
//!   is refused outright rather than retried.
//! - **The CSPRNG-readiness ledger** ([`record_csprng_readiness`], layer *e*).
//!   `seed::kernel_csprng_ready()` already computes whether the kernel pool was
//!   initialised at generation time; before this module that verdict was logged
//!   and discarded. It is now durable, so a node can answer the question after
//!   the fact.
//!
//! **Nothing here fixes a present defect.** `rand 0.8.5`'s `thread_rng()` is a
//! fork-protected ChaCha12 CSPRNG seeded from `getrandom(2)`; every key this
//! fleet has ever generated came from a genuine CSPRNG. What these controls
//! remove is the *future* failure mode in which a dependency bump, feature-flag
//! change or refactor rebinds the entropy backend with no compile error, no test
//! failure and no diff in Archipelago's own source — the shape ("T1") that
//! produced the 2026-07-30 COLDCARD entropy defect.

use rand::RngCore;
use std::path::PathBuf;
use zeroize::Zeroize;

// ─── Layer (a): the sealed key-generation RNG allowlist ─────────────────

/// Private supertrait module. This is the whole sealing mechanism: `Sealed` is
/// nameable only from inside `entropy`, so `impl KeyGenRng for MyType` cannot
/// compile anywhere else — the required `Sealed` bound is unsatisfiable and
/// unimplementable outside this file.
mod sealed {
    pub trait Sealed {}
}

/// The allowlist of RNGs permitted to drive key generation.
///
/// Deliberately **without** a `rand::CryptoRng` supertrait. `CryptoRng` is a
/// marker with no compiler-checked content — implementing it is a promise, and
/// a promise a caller can make about their own type is not a control. Sealed
/// membership is checkable: the compiler enforces that the set of members is
/// exactly the set written in this file. After KEY-05 the crate contains zero
/// `impl rand::CryptoRng` blocks, so there is one mechanism for this claim
/// rather than two, and the one that remains is the one the compiler verifies.
pub(crate) trait KeyGenRng: RngCore + sealed::Sealed {
    /// Whether draws from this source are subject to [`is_degenerate`].
    ///
    /// `true` for every member that exists in a production build, and not
    /// overridable outside this module because the trait is sealed.
    ///
    /// The single `#[cfg(test)]` member sets it `false`, and that is not a
    /// weakening of the guard — it is what makes the guard compatible with the
    /// crate's strongest existing proof. [`testing::CountingRng`] exists to emit
    /// the published test vector `0x00, 0x01, … 0x1f`, which is *by
    /// construction* exactly the ascending-counter pattern the predicate
    /// rejects. `seed.rs`'s `mnemonic_generation_uses_injected_rng` pins the
    /// 24-word mnemonic that vector produces, and that known-answer pin is the
    /// only evidence the crate has that the RNG named at the call site is the
    /// one `bip39` actually consumes. Guarding the counter would make that pin
    /// unrepresentable and delete the proof to satisfy the guard.
    ///
    /// The opt-out cannot reach a shipped binary: the only implementor that
    /// sets it `false` is itself `#[cfg(test)]`-gated and is not compiled into
    /// the `archipelago` binary at all.
    const GUARD_DRAWS: bool = true;
}

impl sealed::Sealed for rand::rngs::OsRng {}

/// The sole production member. `OsRng` is a direct `getrandom(2)` wrapper with
/// no userspace state, no reseeding schedule and no fork hazard — the thing a
/// defaulted `thread_rng()` happens to be backed by today, named explicitly so
/// that it cannot stop being so silently.
impl KeyGenRng for rand::rngs::OsRng {}

// ─── Layer (d): the degenerate-entropy predicate ────────────────────────

/// The shortest draw the predicate is allowed to inspect.
///
/// Below twelve bytes the false-positive argument in
/// `docs/security/KEY-05-ENTROPY-ENFORCEMENT.md` does not hold: on a two-byte
/// draw, `AllIdentical` fires once in 256 on genuine CSPRNG output, which would
/// be a far worse defect than the one being guarded. Twelve is also exactly the
/// ChaCha20-Poly1305 nonce width, so every AEAD nonce in the crate is guardable
/// at the floor rather than above it.
pub(crate) const MIN_GUARDED_LEN: usize = 12;

/// The three — and only three — patterns [`is_degenerate`] recognises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DegenerateEntropy {
    /// Every byte is `0x00`.
    AllZero,
    /// Every byte equals the first byte (and the first byte is not `0x00`,
    /// which would be reported as the more specific [`Self::AllZero`]).
    AllIdentical,
    /// Every adjacent pair differs by a wrapping +1, or every adjacent pair by
    /// a wrapping −1.
    Counter,
}

impl std::fmt::Display for DegenerateEntropy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::AllZero => "all bytes zero",
            Self::AllIdentical => "all bytes identical",
            Self::Counter => "wrapping ±1 counter",
        };
        f.write_str(s)
    }
}

impl std::error::Error for DegenerateEntropy {}

/// Is this buffer one of the three exactly-analysable degenerate shapes?
///
/// **Nothing heuristic.** No entropy estimator, no chi-squared, no
/// "looks non-random" scoring. A predicate whose false-positive rate cannot be
/// computed in closed form cannot be argued safe, and refusing genuine CSPRNG
/// output on a key-generation path is strictly worse than the defect being
/// guarded against. These three cases are what a rebound-to-broken RNG actually
/// emits (a zeroed buffer, an uninitialised constant fill, a counter PRNG); each
/// has a false-positive probability computable exactly for any length.
pub(crate) fn is_degenerate(bytes: &[u8]) -> Option<DegenerateEntropy> {
    if bytes.is_empty() {
        return None;
    }

    if bytes.iter().all(|b| *b == 0) {
        return Some(DegenerateEntropy::AllZero);
    }

    // Checked after AllZero so the reported variant is always the more specific
    // one, even though AllZero is a strict subset of AllIdentical.
    if bytes.iter().all(|b| *b == bytes[0]) {
        return Some(DegenerateEntropy::AllIdentical);
    }

    // A single byte cannot form a counter; `windows(2)` is empty and `all`
    // would vacuously succeed, so guard the length explicitly.
    if bytes.len() >= 2 {
        let ascending = bytes.windows(2).all(|w| w[1] == w[0].wrapping_add(1));
        let descending = bytes.windows(2).all(|w| w[1] == w[0].wrapping_sub(1));
        if ascending || descending {
            return Some(DegenerateEntropy::Counter);
        }
    }

    None
}

/// Fill `out` with key material from an allowlisted RNG, refusing a degenerate
/// draw.
///
/// On a trip the buffer is **zeroized**, the variant and the buffer length are
/// logged, and the error is returned. There is deliberately **no retry**: a
/// retry would paper over a genuinely broken RNG, which is precisely the failure
/// this layer exists to surface. The bytes themselves are never logged.
///
/// # Panics
///
/// If `out.len() < MIN_GUARDED_LEN`. Calling the guard on a buffer too short for
/// its false-positive argument to hold is a programmer error, not an input
/// condition — a caller that legitimately needs fewer bytes must draw from
/// `OsRng` directly and unguarded, and say so.
pub(crate) fn draw_key_bytes<R: KeyGenRng>(
    rng: &mut R,
    out: &mut [u8],
) -> Result<(), DegenerateEntropy> {
    assert!(
        out.len() >= MIN_GUARDED_LEN,
        "draw_key_bytes called on a {}-byte buffer; the degenerate-entropy \
         predicate's false-positive bound only holds at {} bytes or more — draw \
         unguarded from OsRng instead (KEY-05)",
        out.len(),
        MIN_GUARDED_LEN
    );

    rng.fill_bytes(out);

    if !R::GUARD_DRAWS {
        return Ok(());
    }

    if let Some(kind) = is_degenerate(out) {
        out.zeroize();
        tracing::error!(
            "refusing degenerate entropy draw: {} over {} bytes — the RNG backing \
             this call site is not producing usable key material (KEY-05 layer d)",
            kind,
            out.len()
        );
        return Err(kind);
    }

    Ok(())
}

// ─── Layer (e): the CSPRNG-readiness ledger ─────────────────────────────

/// Schema version, so a later change does not orphan lines already written on
/// fleet nodes.
const READINESS_SCHEMA_VERSION: u8 = 1;

/// One ledger line. A struct rather than `serde_json::json!` so the field order
/// on disk is the declared order and the schema is a compile-time object rather
/// than a literal that can drift.
///
/// These four fields are the whole record. There is no field for entropy, key
/// bytes, seed material, mnemonic words or a hash of any of them — a readiness
/// ledger that carried any of those would be a new place to steal a key from,
/// sitting next to the identity directory.
#[derive(serde::Serialize)]
struct ReadinessRecord<'a> {
    v: u8,
    ts: String,
    ready: Option<bool>,
    event: &'a str,
}

/// Where the ledger lives.
///
/// Resolved from `ARCHIPELAGO_DATA_DIR` with the `/var/lib/archipelago`
/// fallback, matching `container/version_config.rs:36-39`, so this module needs
/// no wiring through `bootstrap.rs` or a system handler to know its own path.
///
/// Deliberately **outside** `identity/`: the KEY-02 rootfs identity sweep and
/// `backup.restore-identity` both operate on that directory wholesale, and
/// neither should ever have to reason about a file that is not key material.
fn readiness_ledger_path() -> PathBuf {
    let base = std::env::var("ARCHIPELAGO_DATA_DIR")
        .unwrap_or_else(|_| "/var/lib/archipelago".to_string());
    PathBuf::from(base)
        .join("security")
        .join("csprng-readiness.jsonl")
}

/// Append one readiness verdict to the ledger. Best-effort by design.
///
/// Every failure path warns and returns. `ceremony.rs` generates a master seed
/// **offline**, on a machine that need not have `/var/lib/archipelago` at all;
/// a ledger write that could fail key generation would be a availability defect
/// introduced by an audit feature, which is not a trade this is willing to make.
///
/// The file is created `0600` (matching `seed.rs`'s identity-blob pattern) and
/// only ever appended to, so a node accumulates its history rather than
/// overwriting it.
pub(crate) fn record_csprng_readiness(ready: Option<bool>, event: &str) {
    let path = readiness_ledger_path();

    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                "CSPRNG readiness ledger: cannot create {}: {e} — verdict not recorded",
                parent.display()
            );
            return;
        }
    }

    let record = ReadinessRecord {
        v: READINESS_SCHEMA_VERSION,
        ts: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        ready,
        event,
    };
    let line = match serde_json::to_string(&record) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("CSPRNG readiness ledger: serialisation failed: {e}");
            return;
        }
    };

    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }

    match opts.open(&path) {
        Ok(mut f) => {
            use std::io::Write;
            if let Err(e) = writeln!(f, "{line}") {
                tracing::warn!(
                    "CSPRNG readiness ledger: write to {} failed: {e}",
                    path.display()
                );
            }
        }
        Err(e) => tracing::warn!(
            "CSPRNG readiness ledger: cannot open {}: {e} — verdict not recorded",
            path.display()
        ),
    }
}

// ─── Test-only allowlist members ────────────────────────────────────────

#[cfg(test)]
pub(crate) mod testing {
    use super::{sealed, KeyGenRng};

    /// Deterministic test-only RNG emitting `0x00, 0x01, 0x02, …`.
    ///
    /// Relocated verbatim from `seed.rs` (the wrapping-add-1 `fill_bytes` and
    /// therefore the emitted byte sequence are unchanged, so the known-answer
    /// mnemonic it produces is unchanged). What did **not** move is
    /// `impl rand::CryptoRng for CountingRng`: that marker was a false promise —
    /// a counter is not a cryptographic source — and KEY-05 retires it rather
    /// than relocating it. Sealed membership replaces it, and unlike a marker it
    /// is a closed set the compiler enforces.
    pub(crate) struct CountingRng(pub u8);

    impl rand::RngCore for CountingRng {
        fn next_u32(&mut self) -> u32 {
            let mut b = [0u8; 4];
            self.fill_bytes(&mut b);
            u32::from_le_bytes(b)
        }

        fn next_u64(&mut self) -> u64 {
            let mut b = [0u8; 8];
            self.fill_bytes(&mut b);
            u64::from_le_bytes(b)
        }

        fn fill_bytes(&mut self, dest: &mut [u8]) {
            for byte in dest.iter_mut() {
                *byte = self.0;
                self.0 = self.0.wrapping_add(1);
            }
        }

        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> std::result::Result<(), rand::Error> {
            self.fill_bytes(dest);
            Ok(())
        }
    }

    impl sealed::Sealed for CountingRng {}

    impl KeyGenRng for CountingRng {
        // See `KeyGenRng::GUARD_DRAWS`. This type's entire purpose is to emit
        // the ascending counter the predicate rejects.
        const GUARD_DRAWS: bool = false;
    }

    /// A guarded test RNG that emits a constant byte, so the guard itself can be
    /// observed tripping through `draw_key_bytes` rather than only through the
    /// pure predicate.
    pub(crate) struct ConstantRng(pub u8);

    impl rand::RngCore for ConstantRng {
        fn next_u32(&mut self) -> u32 {
            u32::from_le_bytes([self.0; 4])
        }

        fn next_u64(&mut self) -> u64 {
            u64::from_le_bytes([self.0; 8])
        }

        fn fill_bytes(&mut self, dest: &mut [u8]) {
            dest.fill(self.0);
        }

        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> std::result::Result<(), rand::Error> {
            self.fill_bytes(dest);
            Ok(())
        }
    }

    impl sealed::Sealed for ConstantRng {}

    // Deliberately keeps the default `GUARD_DRAWS = true`.
    impl KeyGenRng for ConstantRng {}
}

#[cfg(test)]
mod tests {
    use super::testing::{ConstantRng, CountingRng};
    use super::*;

    // ─── Layer (a) ──────────────────────────────────────────────────────

    #[test]
    fn sealed_allowlist_has_one_production_member() {
        // `OsRng` is a member and is guarded. The assertion that it is the
        // *only* production member is enforced by the compiler plus the sealing
        // — `sealed::Sealed` is unnameable outside this module, so no impl can
        // exist elsewhere — and is checked mechanically by the plan's grep
        // criterion over `impl KeyGenRng for` in this file. What is asserted
        // here is the property that must hold of every production member.
        fn assert_member<R: KeyGenRng>() -> bool {
            R::GUARD_DRAWS
        }
        assert!(
            assert_member::<rand::rngs::OsRng>(),
            "the production allowlist member must be guarded"
        );
        assert!(
            !CountingRng::GUARD_DRAWS,
            "the deterministic test vector member is the one documented opt-out"
        );
        assert!(
            ConstantRng::GUARD_DRAWS,
            "the constant test RNG must stay guarded so the guard is observable"
        );
    }

    #[test]
    fn osrng_draws_through_the_seam() {
        let mut buf = [0u8; 32];
        draw_key_bytes(&mut rand::rngs::OsRng, &mut buf).expect("OsRng draw must be accepted");
        assert!(
            buf.iter().any(|b| *b != 0),
            "draw produced an unfilled buffer"
        );
    }

    // ─── Layer (d): the predicate ───────────────────────────────────────

    #[test]
    fn degenerate_rejects_all_zero() {
        assert_eq!(is_degenerate(&[0u8; 32]), Some(DegenerateEntropy::AllZero));
        assert_eq!(is_degenerate(&[0u8; 12]), Some(DegenerateEntropy::AllZero));
    }

    #[test]
    fn degenerate_rejects_all_identical() {
        assert_eq!(
            is_degenerate(&[0xABu8; 32]),
            Some(DegenerateEntropy::AllIdentical)
        );
        assert_eq!(
            is_degenerate(&[0xABu8; 12]),
            Some(DegenerateEntropy::AllIdentical)
        );
    }

    #[test]
    fn degenerate_rejects_ascending_counter() {
        let ascending: Vec<u8> = (0u8..32).collect();
        assert_eq!(
            is_degenerate(&ascending),
            Some(DegenerateEntropy::Counter),
            "0x00..0x1f is the canonical broken-counter output"
        );
        // Wrapping, not merely ascending: 0xFE, 0xFF, 0x00, 0x01, … is the same
        // defect and must not escape through the wrap.
        let wrapping: Vec<u8> = (0..32u32).map(|i| (0xFEu8).wrapping_add(i as u8)).collect();
        assert_eq!(is_degenerate(&wrapping), Some(DegenerateEntropy::Counter));
    }

    #[test]
    fn degenerate_rejects_descending_counter() {
        let descending: Vec<u8> = (0..32u32).map(|i| (0x80u8).wrapping_sub(i as u8)).collect();
        assert_eq!(is_degenerate(&descending), Some(DegenerateEntropy::Counter));
    }

    #[test]
    fn degenerate_accepts_100k_osrng_draws() {
        // The false-positive claim in KEY-05-ENTROPY-ENFORCEMENT.md is a
        // calculation; this is the empirical companion to it. At 32 bytes the
        // predicted expected count over 100,000 draws is ~1e-71, so a single
        // rejection here means the predicate is wrong, not that we were unlucky.
        let mut buf = [0u8; 32];
        for i in 0..100_000u32 {
            rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut buf);
            assert_eq!(
                is_degenerate(&buf),
                None,
                "genuine OsRng draw #{i} was rejected — the predicate has a false positive"
            );
        }
    }

    #[test]
    fn degenerate_accepts_ordinary_material() {
        // Two bytes equal, and a run of three ascending, must not be enough.
        let sample: [u8; 16] = [
            0x9f, 0x9f, 0x01, 0x02, 0x03, 0xd4, 0x00, 0x00, 0x71, 0x8c, 0x8c, 0xff, 0x10, 0x22,
            0x35, 0xae,
        ];
        assert_eq!(is_degenerate(&sample), None);
    }

    #[test]
    fn draw_key_bytes_rejects_and_zeroizes_a_degenerate_draw() {
        let mut buf = [0xFFu8; 32];
        let err = draw_key_bytes(&mut ConstantRng(0xAB), &mut buf)
            .expect_err("a constant fill must be refused");
        assert_eq!(err, DegenerateEntropy::AllIdentical);
        assert_eq!(
            buf, [0u8; 32],
            "a refused draw must leave the buffer zeroized"
        );
    }

    #[test]
    fn draw_key_bytes_reports_all_zero_specifically() {
        let mut buf = [0xFFu8; 16];
        let err = draw_key_bytes(&mut ConstantRng(0x00), &mut buf).expect_err("zeros are refused");
        assert_eq!(err, DegenerateEntropy::AllZero);
    }

    #[test]
    #[should_panic(expected = "draw_key_bytes called on a 11-byte buffer")]
    fn draw_key_bytes_panics_below_min_guarded_len() {
        let mut buf = [0u8; MIN_GUARDED_LEN - 1];
        let _ = draw_key_bytes(&mut rand::rngs::OsRng, &mut buf);
    }

    // ─── Layer (e): the ledger ──────────────────────────────────────────

    // `ARCHIPELAGO_DATA_DIR` is process-global, so these tests must not run
    // concurrently — serialize them and give each a unique dir. Same pattern and
    // same reasoning as `container/version_config.rs:163-181` (poisoning is fine:
    // a panicking test still releases a usable guard).
    static ENV_LOCK: std::sync::Mutex<u64> = std::sync::Mutex::new(0);

    fn with_tmp_data_dir<F: FnOnce(&std::path::Path)>(f: F) {
        let mut counter = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *counter += 1;
        let dir = std::env::temp_dir().join(format!(
            "archy-entropy-test-{}-{}",
            std::process::id(),
            *counter
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("ARCHIPELAGO_DATA_DIR", &dir);
        f(&dir);
        std::env::remove_var("ARCHIPELAGO_DATA_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn readiness_ledger_is_0600_and_append_only() {
        with_tmp_data_dir(|dir| {
            let path = dir.join("security").join("csprng-readiness.jsonl");

            record_csprng_readiness(Some(true), "unit-test");
            let first = std::fs::read_to_string(&path).unwrap();
            assert_eq!(first.lines().count(), 1, "one call must write one line");

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
                assert_eq!(mode, 0o600, "ledger must be owner-only");
            }

            record_csprng_readiness(Some(false), "unit-test-2");
            let second = std::fs::read_to_string(&path).unwrap();
            assert_eq!(second.lines().count(), 2, "second call must append");
            assert!(
                second.starts_with(first.trim_end()),
                "append must not rewrite the first line"
            );
        });
    }

    #[test]
    fn readiness_record_schema_is_exactly_four_keys() {
        with_tmp_data_dir(|dir| {
            record_csprng_readiness(None, "unit-test-schema");
            let path = dir.join("security").join("csprng-readiness.jsonl");
            let text = std::fs::read_to_string(&path).unwrap();
            let line = text.lines().next().unwrap();
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            let obj = value.as_object().unwrap();

            let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
            keys.sort_unstable();
            assert_eq!(keys, vec!["event", "ready", "ts", "v"]);

            assert_eq!(obj["v"], serde_json::json!(1));
            assert_eq!(obj["event"], serde_json::json!("unit-test-schema"));
            assert!(obj["ready"].is_null(), "an unknown verdict records as null");
            assert!(
                obj["ts"].as_str().unwrap().ends_with('Z'),
                "timestamp must be RFC3339 UTC"
            );
        });
    }

    /// The fixed vocabulary a ledger line can contain: the schema keys plus the
    /// literal values the master-seed call site writes.
    ///
    /// Three of these — `master`, `seed`, `ready` — are themselves BIP-39
    /// English words. A naive "no mnemonic word appears in the file" substring
    /// check would therefore fail on roughly 3% of runs purely because a random
    /// 24-word mnemonic happened to contain one of them, and would *also* false
    /// positive on substrings (`gen-era-te` contains the BIP-39 word `era`).
    /// Subtracting the fixed vocabulary and comparing whole tokens makes the
    /// assertion exact instead of flaky: any alphabetic token in the ledger that
    /// is not schema is, by construction, a leak.
    ///
    /// `t` and `z` are the RFC 3339 date/time separator and the UTC designator
    /// from the `ts` value. They are single characters and every BIP-39 English
    /// word is at least three, so they cannot mask a leaked word.
    const LEDGER_FIXED_VOCABULARY: &[&str] = &[
        "v", "ts", "ready", "event", "master", "seed", "generate", "true", "false", "null", "t",
        "z",
    ];

    #[test]
    fn readiness_record_contains_no_mnemonic_words() {
        with_tmp_data_dir(|dir| {
            let (mnemonic, _seed) = crate::seed::MasterSeed::generate().unwrap();
            let path = dir.join("security").join("csprng-readiness.jsonl");
            let text = std::fs::read_to_string(&path)
                .expect("MasterSeed::generate must have written a readiness line");

            let unexpected: Vec<String> = text
                .split(|c: char| !c.is_ascii_alphabetic())
                .filter(|t| !t.is_empty())
                .map(|t| t.to_ascii_lowercase())
                .filter(|t| !LEDGER_FIXED_VOCABULARY.contains(&t.as_str()))
                .collect();
            assert!(
                unexpected.is_empty(),
                "ledger contains tokens outside the fixed schema vocabulary: {unexpected:?}"
            );

            for word in mnemonic.to_string().split_whitespace() {
                assert!(
                    !unexpected.iter().any(|t| t == word),
                    "mnemonic word {word:?} leaked into the readiness ledger"
                );
            }
        });
    }

    #[test]
    fn readiness_record_survives_unwritable_data_dir() {
        let mut counter = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *counter += 1;
        let dir = std::env::temp_dir().join(format!(
            "archy-entropy-unwritable-{}-{}",
            std::process::id(),
            *counter
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // A *file* where the data dir should be, so `create_dir_all` of the
        // `security/` child cannot succeed.
        let blocker = dir.join("not-a-directory");
        std::fs::write(&blocker, b"x").unwrap();
        std::env::set_var("ARCHIPELAGO_DATA_DIR", &blocker);

        // The contract is that this returns normally. A panic or an unwind here
        // fails the test, which is the whole assertion: a ledger write must
        // never be able to fail key generation on the offline ceremony path.
        record_csprng_readiness(Some(true), "unit-test-unwritable");

        std::env::remove_var("ARCHIPELAGO_DATA_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
