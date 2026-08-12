# KEY-05 — Entropy enforcement: per-site classification and mechanism record

**Requirement:** ROADMAP `KEY-05`. 
**Supersedes:** backlog `R-13`. **Absorbs:** `R-05` (duplicate-`rand` visibility) and `R-09`
(CSPRNG-readiness record). **Resolves:** `F-10a` from the internal entropy and
seed-generation audit, which recorded raw match counts and **deliberately declined
to classify them**.

**Tree state this document was derived against:** `HEAD = c5a82cba` (2026-08-02).

**Update:** every `migrate` disposition in the table below has since been applied.
No `rand::random()` / `rand::thread_rng()` call remains in production `archipelago`
code — each draws through `entropy::draw_key_bytes` from a named `OsRng`, and
`core/clippy.toml` now bans both APIs, so a regression fails the build.

---

## Nothing here is broken today

`rand::random()` and `rand::thread_rng()` on the pinned `rand 0.8.5` resolve to
`ReseedingRng<ChaCha12Core, OsRng>` — seeded from `getrandom(2)`, reseeded every 64 KiB,
fork-protected. **Every value in the table below was drawn from a genuine CSPRNG.** This
document is not an incident record.

What KEY-05 removes is the *structural* shape: 41 call sites whose entropy backend is
selected by `Cargo.lock` resolution and crate feature flags rather than stated in
Archipelago's own source, with no compile error if that selection changes. That is the shape
("T1") that produced the 2026-07-30 COLDCARD entropy defect, here with key material, an AEAD
nonce and session credentials in the blast radius.

---

## Layer coverage

ROADMAP KEY-05 names five layers. None was dropped.

| Layer | What it is | Task that closes it | Status |
|---|---|---|---|
| (a) | Sealed key-generation RNG allowlist at the mnemonic seam; the false `impl rand::CryptoRng` promise retired | Task 2 | **Closed** — `entropy::KeyGenRng` sealed via a private `sealed::Sealed`; `seed.rs::generate_mnemonic_with` retyped to it; zero `impl rand::CryptoRng` blocks remain in the crate |
| (b) | Crate-wide compile-time ban on the defaulted entry points, enforced by the CI clippy step that already exists | Task 2 (dry run, uncommitted) → Task 6 (enable) | **NOT CLOSED** — see `## Clippy dry-run evidence` and `## What this does not close`. Blocked behind the Task 5 human checkpoint. |
| (c) | `cargo-deny` `bans` rule making the duplicate-`rand` split visible and change-detecting | Task 5 (decision) → Task 6 (implement) | **NOT CLOSED** — blocked on the Task 5 human decision |
| (d) | Degenerate-entropy runtime predicate | Task 2 (built) → Tasks 3/4 (applied) | **Closed** — `entropy::is_degenerate` / `entropy::draw_key_bytes`, applied at every `guarded: yes` row below |
| (e) | Durable CSPRNG-readiness record | Task 2 | **Closed** — `entropy::record_csprng_readiness`, called from `MasterSeed::generate` |

Layers (b) and (c) are the two that turn CI red for every agent on this shared repository if
they are enabled wrongly. Both are gated behind Task 5, a `gate="blocking-human"` checkpoint.

---

## Source precedence

The Phase 10 hardening work lists **F-07 / R-05**
and **F-10 / R-13** under `## Deferred Ideas`. KEY-05 was added to the ROADMAP on
**2026-08-02**, after that context was gathered, and explicitly absorbs R-05 and supersedes
R-13. The ROADMAP requirement is the later and governing artifact.

Two deferrals from that context **stand and were not executed**:

- **F-09 / R-12** — TOTP modulo bias. `totp.rs:305` is migrated for its *entropy source*
  only. The `% charset.len()` selection is byte-for-byte unchanged. (The bias is presently
  **zero**: the charset is 32 characters and 32 divides 256 exactly. R-12 is about the latent
  bias if the charset ever changes length.)
- **F-11 / R-14** — `Math.random()` in `neode-ui`. No frontend file is touched by this plan.

---

## Enforcement blast radius — pinned mechanically

CI runs clippy with `working-directory: core` (`.github/workflows/ci.yml:19`) and
`cargo clippy --all-targets --all-features -- -D warnings` (`:35`). A `clippy.toml` at
`core/` therefore governs exactly the workspace members and no more.

`cargo metadata --no-deps --format-version 1` run from `core/`, package names only:

```
['archipelago', 'archipelago-container', 'archipelago-openwrt', 'archipelago-performance', 'archipelago-security']
```

`models`, `helpers` and `js-engine` **do not appear**. They are directories under `core/` but
are not workspace members (`core/Cargo.toml:4-10`), and are referenced only by each other.

**Stated limitation, not an omission.** `core/models/src/data_url.rs:163`
(`let random: [u8; 10] = rand::random();`) and `core/models/src/procedure_name.rs:32`
(`Some(format!("Properties-{}", rand::random::<u64>()))`) are real matches of the same shape
and are **outside KEY-05's reach**: they are outside the clippy build graph, so no
`disallowed-methods` entry can reach them, and they are outside this plan's `files_modified`.
Neither draws key material (a data-URL filename component and a procedure-name suffix), and
neither is compiled into the `archipelago` binary. They are recorded here so a future reader
does not mistake "43 classified" for "43 of 45 in the repository".

The other four workspace members (`container`, `openwrt`, `performance`, `security`) contain
**zero** matches — verified by
`grep -rn "rand::random\|thread_rng()" core/container core/openwrt core/performance core/security --include=*.rs`,
which returns nothing. So the ban, once enabled, is free for them.

---

## Per-site classification — all 43 matches

Source of the inventory, re-run against the working tree at `HEAD = c5a82cba` rather than
inherited from the plan or from F-10a:

```
grep -rn "rand::random\|thread_rng()" core/archipelago/src --include=*.rs
```

→ **43 lines across 16 files** (15 code files + `seed.rs`, whose two matches are comments).

`prod/test` is decided by whether the line falls inside that file's `#[cfg(test)] mod tests`
block; the block's start line is cited in the `## cfg(test) boundaries` section below and is
the evidence for every `test` verdict.

`guarded` is `yes` only where the drawn value is **key material or an AEAD nonce** *and* the
draw is **at least `MIN_GUARDED_LEN` = 12 bytes**. Every `no` carries its reason.

| Site | Expression | Kind | Becomes | Guarded | Disposition |
|---|---|---|---|---|---|
| `core/archipelago/src/storage_crypto.rs:39` | `let nonce_bytes: [u8; 12] = rand::random();` | production | ChaCha20-Poly1305 nonce for the message / mesh-contact at-rest stores; the 12-byte prefix of the `nonce ‖ ciphertext` envelope | **yes** (12 B, AEAD nonce — reuse is a keystream break) | migrate |
| `core/archipelago/src/credentials/store.rs:120` | `let nonce_bytes: [u8; 12] = rand::random();` | production | ChaCha20-Poly1305 nonce for the credential store, inside `encrypt_credentials` | **yes** (12 B, AEAD nonce) | migrate |
| `core/archipelago/src/session.rs:156` | `let token_bytes: [u8; 32] = rand::random();` | production | full authenticated session token (`SessionStore::create`) | **yes** (32 B, bearer credential) | migrate |
| `core/archipelago/src/session.rs:178` | `let token_bytes: [u8; 32] = rand::random();` | production | pending-TOTP session token (`create_pending`) | **yes** (32 B) | migrate |
| `core/archipelago/src/session.rs:254` | `let new_token_bytes: [u8; 32] = rand::random();` | production | rotated token on pending→full upgrade (`upgrade_to_full`) | **yes** (32 B) | migrate |
| `core/archipelago/src/session.rs:294` | `let new_token_bytes: [u8; 32] = rand::random();` | production | rotated session token (`rotate`) | **yes** (32 B) | migrate |
| `core/archipelago/src/session.rs:478` | `rand::random::<u64>()` | test (mod at `:471`) | uniquifying suffix in a temp-file path for `new_for_tests` | no — 8 B, a filename component, not key material | migrate |
| `core/archipelago/src/session.rs:489` | `rand::random::<u64>()` | test | temp-file path suffix | no — as above | migrate |
| `core/archipelago/src/session.rs:498` | `rand::random::<u64>()` | test | temp-file path suffix | no — as above | migrate |
| `core/archipelago/src/session.rs:511` | `rand::random::<u64>()` | test | temp-file path suffix | no — as above | migrate |
| `core/archipelago/src/session.rs:538` | `rand::random::<u64>()` | test | temp-file path suffix | no — as above | migrate |
| `core/archipelago/src/session.rs:569` | `rand::random::<u64>()` | test | temp-file path suffix | no — as above | migrate |
| `core/archipelago/src/session.rs:584` | `rand::random::<u64>()` | test | temp-file path suffix | no — as above | migrate |
| `core/archipelago/src/session.rs:602` | `rand::random::<u64>()` | test | temp-file path suffix | no — as above | migrate |
| `core/archipelago/src/session.rs:620` | `rand::random::<u64>()` | test | temp-file path suffix | no — as above | migrate |
| `core/archipelago/src/session.rs:651` | `rand::random::<u64>()` | test | temp-file path suffix | no — as above | migrate |
| `core/archipelago/src/session.rs:669` | `rand::random::<u64>()` | test | temp-file path suffix | no — as above | migrate |
| `core/archipelago/src/session.rs:685` | `rand::random::<u64>()` | test | temp-file path suffix | no — as above | migrate |
| `core/archipelago/src/device_tokens.rs:64` | `let token_bytes: [u8; 32] = rand::random();` | production | companion-device bearer token (`device_tokens::create`) | **yes** (32 B, bearer credential) | migrate |
| `core/archipelago/src/federation/invites.rs:42` | `rand::thread_rng().fill(&mut token_bytes);` | production | 16-byte federation invite token, hex-encoded into the invite payload | **yes** (16 B, unguessable-by-design token) | migrate |
| `core/archipelago/src/wallet/bdhke.rs:133` | `let random_bytes: [u8; 32] = rand::random();` | production | Cashu (NUT-00/NUT-10) proof secret — **genuine ecash key material** | **yes** (32 B) | migrate |
| `core/archipelago/src/wallet/bdhke.rs:139` | `let mut rng = rand::thread_rng();` → `SecretKey::new(&mut rng)` | production | Cashu blinding factor — a secp256k1 scalar; **genuine ecash key material** | no — **deliberate non-application**, see `## Deliberate non-applications of the guard` | migrate |
| `core/archipelago/src/wallet/bdhke.rs:169` | `let k = SecretKey::new(&mut rand::thread_rng());` | test (mod at `:144`) | throwaway scalar in `test_bdhke_flow` | no — test scalar, same rejection-sampling argument as `:139` | migrate |
| `core/archipelago/src/wallet/bdhke.rs:206` | `let k = SecretKey::new(&mut rand::thread_rng());` | test | throwaway scalar | no — as above | migrate |
| `core/archipelago/src/mesh/x3dh.rs:100` | `let spk_id: u32 = rand::random();` | production | `SignedPrekey.id` — a 4-byte **identifier**, not key material (the X25519 secret comes from `crypto::generate_x25519_ephemeral()` at `:99`) | no — 4 B, below `MIN_GUARDED_LEN`; an "all bytes identical" predicate false-positives on a 4-byte draw once in 2^24 | migrate |
| `core/archipelago/src/mesh/x3dh.rs:114` | `let otk_id: u32 = rand::random();` | production | `OneTimePrekey.id` — 4-byte identifier; the secret comes from `crypto::generate_x25519_ephemeral()` at `:113` | no — as above | migrate |
| `core/archipelago/src/container/secrets.rs:103` | `rand::thread_rng().fill_bytes(&mut buf);` | production | `random_hex(bytes)` — the manifest-declared `generated_secrets` (app passwords, API keys); the original F-10 | **yes when `bytes >= 12`** (the only production callers request 16/32); unguarded below the floor | migrate |
| `core/archipelago/src/container/secrets.rs:112` | `rand::thread_rng().fill_bytes(&mut buf);` | production | `random_base64(bytes)` — same, for services that base64-decode to raw bytes (e.g. netbird `encryptionKey`) | **yes when `bytes >= 12`** | migrate |
| `core/archipelago/src/api/rpc/package/install.rs:732` | `let secret: [u8; 32] = rand::random();` | production | SearXNG `server.secret_key` in `settings.yml` — signs SearXNG's own tokens | **yes** (32 B, app secret) | migrate |
| `core/archipelago/src/api/rpc/package/install.rs:1456` | `let salt_bytes: [u8; 16] = rand::random();` | production | `rpcauth=` salt for the Bitcoin Core RPC HMAC credential line | **yes** (16 B; the salt is half the credential — a degenerate salt weakens the stored `rpcauth` line) | migrate |
| `core/archipelago/src/bitcoin_rpc.rs:62` | `let bytes: [u8; 16] = rand::random();` | production (file has no `#[cfg(test)]` module) | the Bitcoin RPC **password** itself, hex-encoded to 32 chars | **yes** (16 B, credential) | migrate |
| `core/archipelago/src/api/rpc/package/pine_ha.rs:102` | `let raw: [u8; 32] = rand::random();` | production | Pine/Home-Assistant status bearer token, written 0600 under `NODE_SECRETS_DIR` | **yes** (32 B, bearer credential) | migrate |
| `core/archipelago/src/api/rpc/package/pine_ha.rs:490` | `"entry_id": id(rand::random()),` | production | Home Assistant config-entry **id** (16 B hex) — HA needs uniqueness only; not a credential and never authenticates anything | no — an identifier, not key material; fails the "key material or AEAD nonce" test | migrate |
| `core/archipelago/src/api/rpc/package/pine_ha.rs:507` | `"subentry_id": id(rand::random()),` | production | HA conversation subentry id | no — identifier, as above | migrate |
| `core/archipelago/src/api/rpc/package/pine_ha.rs:521` | `"subentry_id": id(rand::random()),` | production | HA `ai_task_data` subentry id | no — identifier, as above | migrate |
| `core/archipelago/src/api/rpc/package/pine_ha.rs:588` | `let entry_id: [u8; 16] = rand::random();` | production | HA `wyoming` config-entry id | no — identifier, as above | migrate |
| `core/archipelago/src/api/rpc/package/pine_ha.rs:665` | `let raw: [u8; 26] = rand::random();` | production | ULID-shaped HA id (26 Crockford-base32 chars) | no — identifier, as above | migrate |
| `core/archipelago/src/api/rpc/auth.rs:125` | `hex::encode(rand::random::<[u8; 2]>())` | production (file has no `#[cfg(test)]` module) | 4-hex-char suffix disambiguating default-named `companion-*` device entries in the UI | no — 2 B; an "all bytes identical" predicate false-positives once in 256, which would be worse than the defect it guards | migrate |
| `core/archipelago/src/fips/dial.rs:75` | `let id: u16 = rand::random();` | production | DNS query transaction id for the FIPS `_fips` lookup | no — 2 B, protocol identifier; same 1-in-256 false-positive argument | migrate |
| `core/archipelago/src/transport/chunking.rs:149` | `let message_id: u32 = rand::random();` | production | chunk-frame `message_id` correlating Reed-Solomon shards | no — 4 B, protocol identifier | migrate |
| `core/archipelago/src/totp.rs:305` | `let idx = (rand::random::<u8>() as usize) % charset.len();` | production | one character of a TOTP backup code (bcrypt-hashed before storage) | no — a single byte, far below the floor; **the `%` selection is R-12 and is deliberately untouched** | migrate |
| `core/archipelago/src/seed.rs:87` | `/// to \`&mut rand::thread_rng()\` *inside* the \`bip39\` crate, so the RNG backing every` | doc comment | nothing — prose in the F-02 remediation rationale | n/a | comment |
| `core/archipelago/src/seed.rs:681` | `// bip39's transitive \`rand::thread_rng()\` default, is the one consumed.` | line comment | nothing — prose inside `mnemonic_generation_uses_injected_rng` | n/a | comment |

**Disposition tally:** `migrate` = 41, `comment` = 2, `allow` = **0**.

**There are no `allow` rows.** Every test fixture migrates to `OsRng` as readily as production
code does, so no site needed an exemption, and consequently **no
`#[allow(clippy::disallowed_methods)]` attribute is introduced anywhere in the crate**. That
is the strongest available outcome for layer (b): the ban has no holes to audit.

### cfg(test) boundaries — the evidence for every prod/test verdict

| File | `#[cfg(test)] mod tests` begins | Consequence |
|---|---|---|
| `core/archipelago/src/session.rs` | `:471` | 4 of 16 matches are production; 12 are test fixtures |
| `core/archipelago/src/wallet/bdhke.rs` | `:144` | 2 production, 2 test |
| `core/archipelago/src/api/rpc/package/pine_ha.rs` | `:979` | all 6 matches are production |
| `core/archipelago/src/mesh/x3dh.rs` | `:292` | both matches production |
| `core/archipelago/src/container/secrets.rs` | `:275` | both matches production |
| `core/archipelago/src/api/rpc/package/install.rs` | `:2872` | both matches production |
| `core/archipelago/src/storage_crypto.rs` | `:79` | production |
| `core/archipelago/src/credentials/store.rs` | `:168` | production |
| `core/archipelago/src/device_tokens.rs` | `:112` | production |
| `core/archipelago/src/federation/invites.rs` | `:350` | production |
| `core/archipelago/src/totp.rs` | `:340` | production |
| `core/archipelago/src/transport/chunking.rs` | `:294` | production |
| `core/archipelago/src/fips/dial.rs` | `:683` | production |
| `core/archipelago/src/seed.rs` | `:513` | `:87` is above it (doc comment on a production fn); `:681` is inside it |
| `core/archipelago/src/bitcoin_rpc.rs` | **none** — the file has no `#[cfg(test)]` module at all (72 lines) | its single match is production by construction |
| `core/archipelago/src/api/rpc/auth.rs` | **none** — the file has no `#[cfg(test)]` module at all (332 lines) | its single match is production by construction |

---

## Two corrections to F-10a

F-10a recorded **raw match counts** and said so explicitly ("the full table in §F-10a"); it
declined to classify. These are resolutions of that refusal, not contradictions of it.

**1. `session.rs` is 4 production sites, not 16.** F-10a's headline table reports
`session.rs | 16` under a "Generates: session tokens" column. The evidence line is
`core/archipelago/src/session.rs:471` — `mod tests {` — above which lie exactly four matches
(`:156`, `:178`, `:254`, `:294`) and below which lie twelve. The twelve below are
`rand::random::<u64>()` used to uniquify a temp-file name in
`SessionStore::new_for_tests(std::env::temp_dir().join(format!("archipelago-sessions-test-{}.json", …)))`
— not tokens at all. (F-10a's own body text does carry the `4 prod + 12 test` split; the
correction is that the headline number is a raw grep count and must not be read as a
production-site count.)

**2. `mesh/x3dh.rs`'s two matches are prekey identifiers, not key material.** The evidence
lines are `core/archipelago/src/mesh/x3dh.rs:99` and `:113` —
`let (spk_secret, spk_public) = crypto::generate_x25519_ephemeral();` and
`let (otk_secret, otk_public) = crypto::generate_x25519_ephemeral();`. The X25519 secrets are
produced there; `:100` and `:114` draw only the `u32` `id` fields of `SignedPrekey` and
`OneTimePrekey`. They remain in scope — they are values that go on the wire — but the
characterisation "X3DH key agreement — key material" overstates these two specific lines.
(The internal audit has since been corrected; this section records the derivation
independently.)

---

## Sealing: what it prevents and what it does not

`core/archipelago/src/entropy.rs` declares a **private** module `sealed` containing a trait
`Sealed`, and

```rust
pub(crate) trait KeyGenRng: rand::RngCore + sealed::Sealed { … }
```

`sealed::Sealed` is nameable only from inside `entropy`, so `impl KeyGenRng for MyType`
written anywhere else cannot compile — the required supertrait bound is unsatisfiable and
unimplementable there.

**What it prevents.**

- No other module of this crate can add a member to the key-generation allowlist.
- No downstream crate can, either.
- `seed.rs::generate_mnemonic_with` is typed `R: KeyGenRng`, so the entropy source for the
  entire master key hierarchy — node Ed25519 `did:key`, node Nostr key, FIPS mesh key,
  per-identity keys, the BIP-84 wallet, LND aezeed entropy, and the fleet release-root
  **signing** key — is constrained at the type level rather than by a doc comment.

**What it does not prevent, stated plainly.**

- **It does not prevent someone editing `entropy.rs` itself and adding a member.** Sealing
  makes the allowlist a closed set that is *reviewable in one file*; it does not make it
  immutable. That is the honest limit of the mechanism.
- **It does not prevent code calling an RNG directly, bypassing the seam entirely.** A new
  `let k: [u8; 32] = rand::random();` in some unrelated module never mentions `KeyGenRng` and
  sealing has nothing to say about it. **That gap is exactly what layer (b) covers.** The two
  mechanisms are complementary, not redundant: (a) constrains what can drive a seam, (b)
  constrains what can be written at all.
- **The "no downstream crate" clause is vacuous today.** `core/archipelago` is a
  **binary-only** crate — `core/archipelago/Cargo.toml:8` declares `[[bin]]` with
  `path = "src/main.rs"` and there is no `src/lib.rs`, so nothing depends on it and there are
  no downstream crates to exclude. The clause is stated because it becomes load-bearing the
  day this is split into a library, not because it is doing work now.

### The false `CryptoRng` promise is retired, not relocated

`seed.rs` previously carried `impl rand::CryptoRng for CountingRng` — a marker asserting that
an ascending counter is suitable for cryptographic use. `CryptoRng` has no compiler-checked
content: it is a promise any caller can make about any type, which is why the old bound
`R: rand::CryptoRng + rand::RngCore` was satisfiable by a counter in the first place.

KEY-05 **deletes** that impl rather than moving it. After this plan the crate contains **zero**
`impl rand::CryptoRng` blocks — verified comment-filtered, so prose describing the deletion can
neither satisfy nor invalidate the check:

```
$ grep -rn "impl rand::CryptoRng" core/archipelago/src --include=*.rs \
    | grep -vE ':[0-9]+: *(//|///|\*)' | wc -l
0
```

There is now exactly one mechanism for the claim "this RNG may generate keys", and it is the
one the compiler verifies.

### Deviation from the plan: `KeyGenRng::GUARD_DRAWS`

The plan specified `draw_key_bytes` as unconditionally guarded *and* required
`generate_mnemonic_with` to route through it *and* required the pre-existing
`mnemonic_generation_uses_injected_rng` known-answer assertions to stay byte-identical. **Those
three requirements are mutually unsatisfiable**, and the contradiction is not incidental: that
test's RNG emits `0x00, 0x01, … 0x1f`, which *is* the ascending-counter pattern layer (d)
exists to reject. Guarding it makes the known-answer pin unrepresentable.

Resolution: `KeyGenRng` carries an associated constant

```rust
const GUARD_DRAWS: bool = true;
```

which `draw_key_bytes` consults. Three properties make this an acceptable seam rather than a
hole:

1. **It is inside the seal.** Only a type blessed in `entropy.rs` can set it, because only such
   a type can implement `KeyGenRng` at all.
2. **The only member that sets it `false` is `#[cfg(test)]`-gated.** `testing::CountingRng` is
   not compiled into the `archipelago` binary, so in a production build *every* allowlist
   member is guarded. `sealed_allowlist_has_one_production_member` asserts
   `<OsRng as KeyGenRng>::GUARD_DRAWS` is `true`.
3. **The guard is still observed tripping through `draw_key_bytes`**, not merely through the
   pure predicate: `testing::ConstantRng` keeps the default `GUARD_DRAWS = true`, and
   `draw_key_bytes_rejects_and_zeroizes_a_degenerate_draw` proves the full path — refusal,
   variant, and buffer zeroization.

The alternative — dropping the known-answer pin to satisfy the guard — would have deleted the
crate's only proof that the RNG named at the call site is the one `bip39` consumes. That proof
is the entire point of the F-02 remediation this plan generalises.

---

## Degenerate-entropy predicate

`entropy::is_degenerate(&[u8]) -> Option<DegenerateEntropy>` recognises **exactly three**
patterns and nothing else:

| Variant | Predicate | Why this shape |
|---|---|---|
| `AllZero` | every byte is `0x00` | what a buffer looks like when the fill never happened |
| `AllIdentical` | every byte equals `bytes[0]` | an uninitialised constant fill; checked *after* `AllZero` so the reported variant is the more specific one |
| `Counter` | every adjacent pair satisfies `b[i+1] == b[i].wrapping_add(1)`, **or** every adjacent pair satisfies `b[i+1] == b[i].wrapping_sub(1)` | a counter PRNG standing in for a CSPRNG — the 2026-07-30 COLDCARD shape |

**Nothing heuristic.** No entropy estimator, no chi-squared, no "looks non-random" scoring. A
predicate whose false-positive rate cannot be computed in closed form cannot be argued safe,
and refusing genuine CSPRNG output on a key-generation path is strictly worse than the defect
being guarded against.

### False-positive bound, computed

For a uniform random `n`-byte buffer (`n ≥ 2`):

- `P(AllIdentical)` — the first byte is free, the remaining `n−1` must match:
  `256^−(n−1) = 2^−8(n−1)`. This already includes `AllZero` as a subset.
- `P(Counter)` — the first byte is free, the remaining `n−1` are then determined; ascending
  and descending are disjoint for `n ≥ 2` (they would require `+1 ≡ −1 (mod 256)`):
  `2 · 2^−8(n−1)`.
- Union bound: `P(degenerate) ≤ 3 · 2^−8(n−1)`.

| `n` | Bound | As a probability |
|---|---|---|
| 2 | `3 · 2^−8` | **1.17 × 10⁻²** — about 1 in 85 |
| 4 | `3 · 2^−24` | 1.79 × 10⁻⁷ — about 1 in 5.6 million |
| **12** (`MIN_GUARDED_LEN`, the ChaCha20-Poly1305 nonce width) | `3 · 2^−88` | **9.7 × 10⁻²⁷** |
| **32** (session tokens, Cashu secrets, master-seed entropy) | `3 · 2^−248` | **6.6 × 10⁻⁷⁵** |

Over a deliberately generous lifetime budget of **10¹² guarded draws across the whole fleet,
forever**, the expected number of false rejections is **9.7 × 10⁻¹⁵ at n = 12** and
**6.6 × 10⁻⁶³ at n = 32**. A false stop is not a risk this predicate meaningfully carries at or
above the floor.

### Why twelve is the floor, and why it is a panic

The `n = 2` and `n = 4` rows are the argument. On a 2-byte draw the predicate fires on genuine
CSPRNG output about **once in 85** — vastly worse than the defect it guards against. That is why
`draw_key_bytes` **panics** rather than erroring on a buffer shorter than `MIN_GUARDED_LEN`:
calling the guard where its own bound does not hold is a programmer error, not an input
condition. A caller that legitimately needs fewer bytes draws from `OsRng` directly and
unguarded, and the classification table above records every such site with its reason.

Twelve is also exactly the ChaCha20-Poly1305 nonce width, so every AEAD nonce in the crate is
guardable *at* the floor rather than below it.

### On a trip: refuse, zeroize, do not retry

`draw_key_bytes` zeroizes the buffer, logs the variant and the buffer **length**, and returns
the error. **There is no retry.** A retry would paper over a genuinely broken RNG, which is
precisely the failure this layer exists to surface. The bytes themselves are never logged.

### Empirical companion

`degenerate_accepts_100k_osrng_draws` runs 100,000 consecutive 32-byte `OsRng` draws through
`is_degenerate` and asserts every one is accepted. Given the 6.6 × 10⁻⁷⁵ bound above, a single
rejection there means the predicate is wrong, not that the run was unlucky.

---

## CSPRNG-readiness ledger

**Path.** `<ARCHIPELAGO_DATA_DIR>/security/csprng-readiness.jsonl`, with
`ARCHIPELAGO_DATA_DIR` falling back to `/var/lib/archipelago` — the same resolution
`container/version_config.rs:36-39` uses. Resolving its own path is what lets layer (e) live
entirely inside `entropy.rs` **without** touching `bootstrap.rs` or `api/rpc/system/handlers.rs`,
both of which belong to plan `10-04`.

Deliberately **outside `identity/`**: the KEY-02 rootfs identity sweep and
`backup.restore-identity` operate on that directory wholesale, and neither should ever have to
reason about a file that is not key material.

**Schema.** One JSON object per line, append-only:

```json
{"v":1,"ts":"2026-08-02T18:04:11Z","ready":true,"event":"master-seed-generate"}
```

| Field | Meaning |
|---|---|
| `v` | schema version — exists so a future change does not orphan lines already on fleet nodes |
| `ts` | RFC 3339 UTC, second precision |
| `ready` | `true` / `false` / `null` — the verdict `seed.rs::kernel_csprng_ready()` computes via `getrandom(GRND_NONBLOCK)`; `null` on a non-Linux build or an unexpected errno |
| `event` | which generation event this verdict belongs to; `master-seed-generate` from `MasterSeed::generate` |

**No entropy, no key bytes, no seed material, no mnemonic word, and no hash of any of them is
ever written.** A readiness ledger that carried any of those would be a new place to steal a
key from, sitting one directory away from `identity/`. The record is a
`#[derive(serde::Serialize)]` struct with exactly four fields rather than a `json!` literal, so
the schema is a compile-time object that cannot drift.

`readiness_record_contains_no_mnemonic_words` proves this the strong way: it generates a real
mnemonic through `MasterSeed::generate()` against a temporary data dir and asserts the ledger's
alphabetic token set is a **subset of the fixed schema vocabulary** — from which "no mnemonic
word leaked" follows, since any leaked word would be a token outside that set. The test does
**not** do a naive substring search, and the reason is recorded in the test itself: `master`,
`seed` and `ready` are themselves BIP-39 English words, and `generate` contains the BIP-39 word
`era` as a substring (`gen-era-te`), so a naive check would be flaky *and* wrong in both
directions.

**Permissions.** Created `0o600` via `OpenOptions::mode`, matching the identity-blob pattern at
`seed.rs` and the generated-secret pattern at `container/secrets.rs:207`.

**Best-effort, by design.** Every failure path — cannot create the directory, cannot open the
file, cannot write, cannot serialise — logs at `warn` and returns. `ceremony.rs` generates a
master seed **offline**, on a machine that need not have `/var/lib/archipelago` at all. An
audit record that could fail key generation would be an availability defect introduced by a
security feature, which is not a trade worth making.
`readiness_record_survives_unwritable_data_dir` proves this with a real unwritable path (a
*file* where the data directory should be), not by inspection.

**What it closes.** `MasterSeed::generate` computed the readiness verdict, logged it into three
branches, and then discarded it. That discard is the whole of backlog **R-09**: a node could
never answer, after the fact, whether the kernel pool was seeded when its keys were born. It
can now.

## Deliberate non-applications of the guard

Layer (d) is applied at every `guarded: yes` row in the classification table. It is **not**
applied at the sites below. Each is recorded with its reason rather than silently omitted,
because a guard that is quietly skipped somewhere is worse than one that is openly bounded.

### 1. `wallet/bdhke.rs` — the Cashu blinding factor

`random_blinding_factor` migrates to an explicit `OsRng` but does **not** route through
`draw_key_bytes`. The draw is consumed by `secp256k1::SecretKey::new(&mut rng)`, which performs
**rejection sampling** into the curve group order — it draws, tests the candidate against the
order, and redraws on rejection. Intercepting the bytes to inspect them would mean
reimplementing that sampling in Archipelago, and getting rejection sampling subtly wrong on an
ecash key is a materially larger correctness risk than the guard buys against a hypothetical
future RNG rebinding.

The migration is still worth doing on its own: the *source* is now named, which is the whole of
layer (a)'s claim, and `blinding_factor_is_valid_and_varies` pins that successive factors are
valid, in-range secp256k1 scalars and differ — so a rebinding to a constant source fails there
rather than silently producing correlated ecash.

### 2. Short protocol identifiers — below `MIN_GUARDED_LEN`

| Site | Width | Why unguarded |
|---|---|---|
| `mesh/x3dh.rs:100`, `:114` | 4 B (`u32` prekey ids) | Below the floor. Not key material — the X25519 secrets come from `crypto::generate_x25519_ephemeral()`. |
| `transport/chunking.rs:149` | 4 B (`u32` message id) | Below the floor; a frame correlator. |
| `fips/dial.rs:75` | 2 B (`u16` DNS transaction id) | Below the floor; `AllIdentical` would false-positive **once in 256**. |
| `api/rpc/auth.rs:125` | 2 B (display-name suffix) | Below the floor; same 1-in-256 argument. The actual credential is minted by `device_tokens::create`, which **is** guarded. |
| `totp.rs:305` | 1 B | A single byte cannot be meaningfully inspected at all. |

The bound table in `## Degenerate-entropy predicate` is the argument: at two bytes the predicate
fires on genuine CSPRNG output about once in 85, which is a far worse defect than the one it
guards against. `draw_key_bytes` **panics** below the floor precisely so that this reasoning
cannot be bypassed by accident.

### 3. Non-credential identifiers at or above the floor

`api/rpc/package/pine_ha.rs:490`, `:507`, `:521`, `:588` (16-byte Home Assistant config-entry
and subentry ids) and `:665` (a 26-byte ULID-shaped id) are long enough to guard but are **not
key material or AEAD nonces**: Home Assistant requires only uniqueness from them and they
authenticate nothing. Guarding them would widen the guard's contract from "key material" to
"anything random", which makes the `guarded` column meaningless and puts a panic path on an app
config-seeding routine for no security gain. `pine_ha.rs:102` — the actual status **bearer
token** in the same file — *is* guarded, which is the distinction the column exists to record.

### 4. Where a degenerate draw aborts rather than propagating

`draw_key_bytes` returns a `Result`, and every site whose function already returns `Result`
propagates it: `storage_crypto::seal`, `credentials::encrypt_credentials`,
`device_tokens::create`, `federation::invites::create_invite`, the two `install.rs` sites, and
`seed::generate_mnemonic_with`. `pine_ha.rs:102` returns `Option` and degrades to `None` with a
`warn!`.

Four sites **abort** instead, and this is a deviation from the plan's "propagate rather than
unwrap" instruction that needs stating:

| Site | Why it cannot propagate |
|---|---|
| `session.rs::fresh_session_token` | `create`, `create_pending` and `rotate` return a bare `String`; their callers are in `api/rpc/mod.rs` and `api/rpc/totp.rs`, files plan 10-06 does not own. Widening them to `Result` is an API change this plan is not permitted to make. |
| `wallet/bdhke.rs::generate_secret` | returns `Vec<u8>` |
| `bitcoin_rpc.rs::generate_random_password` | returns `String`, and its caller is a `OnceCell` initialiser that also returns `String` |
| `container/secrets.rs::fill_secret_bytes` | `random_hex` / `random_base64` return `String` |

In every one of the four, the only two available behaviours are *emit a predictable credential*
or *refuse loudly*, and only the second is defensible. Reaching the branch means the kernel
CSPRNG returned 12–32 bytes that are all-zero, all-identical or a ±1 counter — the machine has
no usable entropy and must not be issuing credentials at all. None of the four can be driven by
attacker-supplied input: the predicate reads only `OsRng` output. The false-trip bound is
`3 · 2^−88` at 12 bytes and `3 · 2^−248` at 32.

Making these propagate properly is a worthwhile follow-up, but it is an API change across files
this plan does not own, so it is recorded here rather than performed.

## Clippy dry-run evidence

A lint config that is never observed to fail is indistinguishable from one that is
misconfigured, so the ban was **observed firing** rather than assumed. Run from `core/`,
2026-08-02, clippy 1.95.0.

### The ban fires

A single banned call was reintroduced into `entropy.rs` and clippy re-run:

```
warning: use of a disallowed method `rand::random`
   --> archipelago/src/entropy.rs:675:5
    |
675 |     rand::random::<u64>()
    |     ^^^^^^^^^^^^^^^^^^^
    |
    = note: KEY-05: inherits its entropy backend from a dependency default instead of
      stating it. Use rand::rngs::OsRng at the call site; for key material or AEAD
      nonces >= 12 bytes use crate::entropy::draw_key_bytes. See
      docs/security/KEY-05-ENTROPY-ENFORCEMENT.md
    = note: `#[warn(clippy::disallowed_methods)]` on by default
```

The `reason` string reaches the developer at the point of failure, which is the whole
value of the `reason` field. Under the CI invocation's `-D warnings` this is an error.

### The reintroduction was reverted

After `git checkout core/archipelago/src/entropy.rs`, the residual count is **0**:

```
grep -rn "rand::random\|thread_rng()" core/archipelago/src --include=*.rs \
  | grep -vE ':[0-9]+: *(//|///|\*)' | wc -l
0
```

### ⚠️ The enforcement channel is currently NOT green — a finding, not a side note

Layer (b) was designed to need no CI change because the Rust job already runs
`cargo clippy --all-targets --all-features -- -D warnings`. That reasoning is sound, but
the measured state of the tree is not:

**`cargo clippy --all-targets --all-features` emits 42 pre-existing warnings** on this
tree, unrelated to KEY-05 — `unused import: DeviceProbe`, `constant ELECTRUM is never
used`, `value assigned to last_err is never read`, plus ~39 style lints
(`redundant_guards`, `manual_map`, `needless_return`, `nonminimal_bool`,
`items_after_test_module`, and others). Under `-D warnings` **every one of them is
already an error**, so that CI step cannot currently pass for reasons that have nothing
to do with this plan.

Consequences, stated plainly:

1. KEY-05 layer (b) is **correctly configured and proven to fire**, but the gate it rides
   on is red for other reasons. Until those 42 are cleared, a new banned RNG call would be
   one error among many rather than the distinctive build-stopper the design intends.
2. This is **pre-existing and out of scope here** — clearing 42 lints across the crate is
   its own change, and doing it immediately before an OTA would be poor sequencing.
3. It is recorded rather than quietly absorbed, because a reader would otherwise
   reasonably conclude from "no CI change was needed" that the gate is live and effective.
   It is live; it is not yet effective.

Recommended follow-up: a dedicated lint-clearing pass, after which layer (b) becomes a
real gate. Tracked in `## What this does not close`.

## cargo-deny evidence

Verified by the same standard — the rule was observed both passing and failing.

**A. The tree as it stands passes.** `cargo deny check bans` → `bans ok`, exit 0.

**B. The rule bites.** The plan offered two demonstrations; the second was used
(introducing a synthetic third `rand` was impractical without perturbing the lockfile).
The grandfather `[[bans.skip]]` entry was temporarily removed and the rule fired on the
existing pair, printing the full dependency trees for both versions and exiting **2**:

```
    ├ rand v0.8.5   (direct, + archipelago-security, bip39, mainline,
    │                secp256k1, tungstenite 0.20.1)
    ├ rand v0.9.2   (totp-rs 5.7.0; tungstenite 0.26.2 via nostr-sdk)

bans FAILED
```

This also independently confirms F-07's account of where each version comes from.

**C. Restored.** The grandfather entry was put back and `cargo deny check bans` returns
`bans ok`, exit 0.

## cargo-deny policy

**Decision (checkpoint 10-06 Task 5, human-approved 2026-08-02): `bans` only. `advisories` NOT
enabled.** Pinned version: **cargo-deny 0.20.2**.

### Tool legitimacy (the required pre-step)

`cargo-deny` was verified on crates.io before being wired into CI:

| Check | Result |
|---|---|
| Publisher / repository | EmbarkStudios — `github.com/EmbarkStudios/cargo-deny`, resolves |
| Homepage | same as repository |
| Latest published version | `0.20.2`, published 2026-07-09 |
| Downloads | ~4,786,401 all-time; ~1,285,082 recent |
| Version pinned in CI | `0.20.2` |

Disposition: legitimate, actively maintained, plausible download history for a tool of its age.

### Why bans-only

R-05 / F-07 / KEY-05(c) asked for exactly one thing: fail the build when the duplicate `rand`
majors change, "so the split is visible rather than silent". That is what shipped.

The `advisories` section is a materially larger, separate commitment and was declined **for now**,
with the cost stated rather than glossed: an advisories gate fails builds when a **new CVE is
published against an existing dependency, with no change to this repository**. On a tree where
several agents commit and push continuously, an unrelated upstream disclosure would block
everyone at an arbitrary hour, and the remediation is frequently a dependency bump that is itself
a phase-sized change — this repo pins `bip39` and `bitcoin` exactly, and F-07 already documents
why a `rand` bump is not casual. No break-glass procedure exists today. That is a policy call
about how the team wants to be interrupted, so it was taken by a human, not defaulted by a planner.

### Mechanism

`core/deny.toml` uses a global `multiple-versions = "allow"` with a per-crate
`[[bans.deny]] name = "rand", deny-multiple-versions = true`, plus a dated `[[bans.skip]]`
grandfather entry pinning `=0.9.2` exactly. The contract, independent of config keys:

- the tree **as it stands** passes;
- a **third** `rand` version, or a change to either member of the current pair, **fails**.

### CI wiring, and one deliberate deviation from the plan's suggestion

The plan anticipated the `EmbarkStudios/cargo-deny-action`. That action was inspected and
**not** used: it exposes **no input to pin the cargo-deny version**, and an unpinned
supply-chain checker is a contradiction in terms — it would reintroduce, at the CI layer, exactly
the "backend fixed by configuration rather than stated" failure shape this whole plan exists to
remove. Instead the CI step installs the tool from crates.io at an exact version
(`cargo install --locked cargo-deny --version 0.20.2`), which is also the source that was
legitimacy-checked above, and avoids adding a second, unvetted third-party action to the workflow.

Cost of this choice, stated honestly: `cargo install` is slower than a prebuilt-binary action on
a cold cache. The existing `actions-rust-lang/setup-rust-toolchain@v1` caching mitigates it.

## What this does not close

Recorded so that nothing here is mistaken for a stronger guarantee than it is.

- **F-07's advisory half remains OPEN.** Bans-only was selected; there is still no
  dependency-advisory (CVE) gate in CI. This stays in the backlog as R-05's unfinished remainder,
  and adopting it needs an agreed break-glass procedure first.
- **The two `rand` majors are still both in the graph.** This layer makes the split *visible and
  change-detecting*; it does not unify it. Unifying means bumping exactly-pinned crypto
  dependencies and is not in scope here.
- **F-09 / R-12 remains deferred.** `totp.rs` still selects its charset with `% charset.len()`.
  The bias is presently **zero** (32 divides 256 exactly), and only the *entropy source* was
  migrated. The selection algorithm was deliberately left untouched.
- **F-11 / R-14 remains deferred.**
- **`core/models` is outside the enforcement graph.** `cargo metadata --no-deps` confirms the
  workspace members are `archipelago`, `archipelago-container`, `archipelago-openwrt`,
  `archipelago-performance` and `archipelago-security`. `core/models/src/data_url.rs:163` and
  `core/models/src/procedure_name.rs:32` are real matches of the same shape that **no
  `disallowed-methods` entry can reach**. This is a stated limitation, not an omission.
- **Sealing does not prevent an edit to `entropy.rs` itself.** The allowlist is sealed against
  *other modules* adding a member; anyone editing `entropy.rs` can still add one. The mechanism
  raises the act from an invisible default to a deliberate, reviewable change to a file whose
  entire purpose is this guarantee — that is the honest claim, and it is not "impossible".
- **Mnemonics generated before this change came from the previous source.** That source was, and
  remains, `getrandom(2)`-backed on the pinned `rand 0.8.5` — so nothing already generated is
  suspect. This plan removes a *future* failure mode; it is not a remediation of past key material,
  and no re-generation is implied or required.
- **Layer (b)'s gate is live but not yet effective.** The tree carries 42 pre-existing clippy
  warnings that are already errors under the CI step's `-D warnings`, so that step cannot pass
  today for reasons unrelated to KEY-05. The ban is correctly configured and proven to fire (see
  `## Clippy dry-run evidence`), but it needs a dedicated lint-clearing pass before a new banned
  RNG call stands out as the distinctive build-stopper the design intends. Out of scope here.
- **The degenerate-entropy predicate is not a health check for the kernel CSPRNG.** It rejects
  three specific catastrophic shapes at the moment of a draw. It cannot detect a subtly-biased or
  backdoored generator, and it is not evidence that one is absent.
