# PSBT-First Signing Architecture

> ## ⚠️ Status update (2026-08-02): **§8 Phase 1 was superseded by deletion, not delivered**
>
> Phase 1 ("Descriptor watch-only read path", §8) planned to **rewrite**
> `handle_bitcoin_init_wallet_from_seed` so Bitcoin Core's wallet held only the xpub. That is not
> what happened. Under Phase 10 decision **D-07b**, the entire Bitcoin Core wallet path was
> **deleted**: `handle_bitcoin_init_wallet_from_seed` and its `bitcoin.init-wallet-from-seed`
> dispatch arm are gone. It had no caller, LND is the wallet the product drives, and the endpoint
> was authenticated *and* password-gated, so F-13 was key-at-rest duplication rather than an
> exposed endpoint.
>
> **Consequences for reading the rest of this document:**
>
> - **§0's "single highest-value change"** and **§2.1's invariant** now read against a code path
>   that no longer exists. Their goal — the BIP-84 private key existing in exactly one place —
>   is **achieved**, by removal rather than by conversion to watch-only.
> - **§1.1, §2.2, §3.1 and §7.3** describe a Core watch-only wallet and a wallet migration.
>   **There is no such wallet and no migration was performed or is planned.**
> - **§3.1's key-origin requirement** still holds, but it now applies to the **PSBT** rather than
>   to Archipelago-emitted descriptors, of which there are none left. `lnd.create-psbt` inspects
>   and reports it (`psbt_key_origin_report`, `core/archipelago/src/api/rpc/lnd/wallet.rs`).
> - **§5 (LND) is unaffected and remains accurate**, including **§5.4's honesty table**, which is
>   correct as written and unchanged.
>
> **Note:** the current signing-posture record is maintained internally. It records the
> deletion with its evidence, an honest per-step coverage map of the LND PSBT round trip, and the
> verdict on whether an external signer can sign a default node's PSBT today (it cannot: no fleet
> node is provisioned watch-only). Phases 2-7 below are unaffected as design targets.

> **Status: specification.** No implementation. This document defines a target architecture and
> a phased rollout that a future `/gsd-plan-phase` can consume directly. It deliberately
> contains no code, adds no dependencies, and changes no wallet or signing behaviour.
>
> **Companion document:** the internal entropy and seed-generation audit that
> motivated this spec. **Cross-linked design:**
> `docs/hardware-signer-design.md` — the exploratory TROPIC01 air-gapped signer, which this
> architecture treats as the future *first-party* signer, not as a competing design.

**Provenance rules used throughout.** Every architectural claim is grounded in either (a) a
`file:line` from this tree, or (b) RESEARCH.md Part C
(which cites Bitcoin Core `doc/psbt.md`, `doc/descriptors.md`, `doc/multisig-tutorial.md`, the
Core 30.0 release notes, LND `docs/remote-signing.md` and `docs/psbt.md`). Anything from
neither is marked `[UNVERIFIED]`.

---

## 0. Why this document exists

The 2026-07-30 Coinkite COLDCARD entropy incident swept ~1,082 BTC from ~1,195 addresses. The
Archipelago-specific reading is in the audit; the design-relevant lesson is narrower and is the
organising principle of this spec:

> **T1's survivors were the users who took the *optional* extra step.** Users who rolled dice
> contributed ≥128 bits independently of the broken RNG and were not at risk. The safe path
> existed the whole time; it just was not the default.

Everything below follows from that. The safe path (watch-only + external signer + PSBT) must be
the **default** and must feel like the normal way to use Archipelago, not an expert mode buried
behind a warning. The hot wallet is retained, deliberately, as an explicitly-secondary tier —
because a safe path users route around is not a safe path.

**Where the tree stands today (important, and not what the target says).**
`core/archipelago/src/api/rpc/bitcoin.rs:161-294` already creates a **descriptor** wallet
(`createwallet ... descriptors=true`, `:207`) — which is the right foundation — but it passes
`disable_private_keys = false` (`:203`) and imports `wpkh(xprv/0/*)` and `wpkh(xprv/1/*)`
(`:229-231`), i.e. **the BIP-84 account extended *private* key is imported into Bitcoin Core's
`wallet.dat`.** The node's spending key therefore lives in two places: the daemon's Argon2 +
ChaCha20-Poly1305 envelope (`core/archipelago/src/seed.rs:238-269`) *and* Core's wallet
database. The code is careful with the string in memory (`bitcoin.rs:189`, zeroized at `:222`
and `:284`), but the key itself is persisted by Core. Closing that gap is Phase 1 of the
rollout in §8, and it is the single highest-value change in this document.

---

## 1. Target architecture

### 1.1 Watch-only descriptor wallet on the node

The node runs a Bitcoin Core wallet that is **structurally incapable of signing**:

- Created with `createwallet` passing **`disable_private_keys = true`** and
  `descriptors = true`. Note the ordering already used at
  `core/archipelago/src/api/rpc/bitcoin.rs:200-208` — the second positional argument is
  `disable_private_keys`, currently `false`.
- Populated with `importdescriptors`, using **public** descriptors only
  (`wpkh([fingerprint/84h/0h/0h]xpub.../0/*)` and `.../1/*`).

Unsignability comes from the *absence of private key material*, not from a flag that could be
flipped. That is the correct construction and is why "watch-only" here means "descriptor wallet
with no private keys", not "a wallet we promise not to sign with".

**Descriptor-only from day one.** Bitcoin Core 30.0 removed the ability to create *or load* BDB
legacy wallets (RESEARCH §C.1). Nothing in this design may depend on a legacy wallet, on
`importmulti`, or on any of the 11 removed legacy RPCs. Archipelago is already descriptor-based
(`bitcoin.rs:207`), so this costs nothing to preserve and would be expensive to lose.

### 1.2 The loop, with the actual RPCs

| Step | RPC | Scope | Notes |
|---|---|---|---|
| 1. Construct + fund | `walletcreatefundedpsbt` | **wallet** | Runs on the watch-only wallet. Selects inputs, adds change, attaches the metadata the signer needs. |
| 2. Fill UTXO data (optional) | `utxoupdatepsbt` | node | Useful when the PSBT was built elsewhere or is missing witness UTXO data. |
| 3. Inspect | `analyzepsbt` | node | **Drive all UI state from this** — see §1.3. |
| 4. Export | — | — | Serialise to base64 / file / QR (§4). |
| 5. Sign (offline) | external signer | — | Hardware device, or `descriptorprocesspsbt` on an offline machine holding the descriptors. |
| 6. Import | — | — | Scan / upload the signed PSBT back. |
| 7. Merge signatures | `combinepsbt` | node | Multisig only: merges signatures for the **same** transaction from multiple signers. |
| 8. Merge transactions | `joinpsbts` | node | Different transactions into one. **Not** the multisig merge — a common and expensive confusion. |
| 9. Finalize | `finalizepsbt` | node | Produces the network-serialized transaction. |
| 10. Broadcast | `sendrawtransaction` | node | Except for LND channel funding — see §5. |

`walletprocesspsbt` (wallet-scoped) and `descriptorprocesspsbt` (node-scoped, takes a descriptor
list, **needs no wallet**) are the two signing entry points. `descriptorprocesspsbt` is the
right primitive for an offline signing machine that has descriptors but no wallet.

**Wallet-scoped vs node-scoped matters operationally**: wallet-scoped RPCs must be addressed to
the specific wallet endpoint (`/wallet/<name>`), node-scoped ones must not. Archipelago's
existing `bitcoin_rpc_call` helper (`core/archipelago/src/api/rpc/bitcoin.rs:191-210` usage)
will need an explicit wallet-scoping parameter rather than one global endpoint.

### 1.3 `analyzepsbt` drives the UI — do not infer state

`analyzepsbt` reports, per input, what is still missing and **which role must act next**
(updater / signer / finalizer). The UI must render from that, not from Archipelago's own guess
about how many signatures a 2-of-3 needs. Rationale: role inference is where coordinators get
multisig wrong, and the node already has an authoritative answer one RPC away. It also makes
the "what do I do now" screen correct for free in partial-signature states.

### 1.4 Versions this runs against

From the manifests, so the spec is not written against an imaginary node:

| App | Manifest version | Image |
|---|---|---|
| Bitcoin Core | `28.4.0` (`apps/bitcoin-core/manifest.yml:4`) | `bitcoin:28.4` (`:10`) |
| Bitcoin Knots | `28.1.0` (`apps/bitcoin-knots/manifest.yml:4`) | **`bitcoin-knots:latest`** (`:10`) |
| LND | `0.18.4` (`apps/lnd/manifest.yml:4`) | `lnd:v0.18.4-beta` (`:8`), requires Bitcoin `>=26.0` (`:25`) |

**Flagged, in scope to name and out of scope to fix:** `bitcoin-knots:latest`
(`apps/bitcoin-knots/manifest.yml:10`) is an **unpinned tag**, at odds with ADR-009's
pinned-tag mandate and with every other image in these three manifests. For a wallet-bearing
component, an unpinned tag means the descriptor/PSBT RPC surface underneath a user's funds can
change on a `podman pull`. Fixing it belongs to whoever owns ADR-009 enforcement.

**PSBTv2 / BIP-370** is merged into Bitcoin Core (RESEARCH §C.1). **`[UNVERIFIED]`** — which
released version first exposes it at the RPC surface, and how broadly hardware signers accept
it, was not confirmed. **Build against PSBTv1 as the interop baseline**; treat v2 as
opportunistic and never as a requirement for a user to spend their money.

---

## 2. Where each step lives

Three surfaces, one non-negotiable invariant.

### 2.1 The invariant

> **The BIP-84 private key stays in the daemon's encrypted store. Only the xpub goes into the
> Core descriptor wallet. The private key is never imported into Core.**

Today this is violated (`core/archipelago/src/api/rpc/bitcoin.rs:229-231`, §0). The at-rest
envelope that should hold it exclusively already exists and is sound: Argon2 + ChaCha20-Poly1305
with per-blob salt and nonce from `OsRng`, written `0600`
(`core/archipelago/src/seed.rs:238-269`, `:243-246`, `:318-324`).

### 2.2 Rust orchestrator — `core/archipelago`

Owns everything that touches keys or Core:

- Derives the BIP-84 account key (`core/archipelago/src/seed.rs:207-224`, path `m/84'/0'/0'`)
  and exports **only** the account-level xpub plus its key-origin fingerprint into descriptors.
- Creates and maintains the watch-only wallet (rewrite of
  `handle_bitcoin_init_wallet_from_seed`, `core/archipelago/src/api/rpc/bitcoin.rs:161-294`).
- Owns the PSBT lifecycle RPCs: construct, analyze, combine, finalize, broadcast.
- Owns the *internal* software-signer path used by the hot tier (§6), which decrypts the seed
  under the user's password exactly as `bitcoin.rs:182-185` does today, signs, and zeroizes.
- Enforces spend limits server-side (§6). **Limits enforced in the UI are not limits.**

### 2.3 `neode-ui`

Owns presentation and transport only. It must never see a private key, an xprv, or a mnemonic
outside the onboarding flow the audit already scopes (F-04, F-08).

- Renders the PSBT review screen: inputs, outputs, fee, change, and the `analyzepsbt` "next
  role" state.
- Renders the export payload as animated QR (§4) and offers file download.
- Accepts the signed PSBT by camera scan or file upload.
- Renders the cold / warm / hot tier badges (§6) and the honest Lightning copy (§5.4).

### 2.4 Companion app

Owns the air-gap camera path. It already has the two pieces this needs:

- A working QR scanner (project memory: native scan shipped in companion 0.5.22; dense-QR fix
  `07772b56`).
- SeedQR encode/decode (`neode-ui/src/utils/seedqr.ts:11`), with a correct, honest note at
  `:9` that the LND aezeed is **not** BIP-39 and must never be SeedQR-encoded.

The companion is the natural home for scan-heavy multi-frame PSBT transport, because the node's
own browser may be a TV kiosk with no camera.

---

## 3. Tiers

### 3.1 Tier 1 — single-sig with an external hardware signer

- Descriptor: `wpkh([<fingerprint>/84h/0h/0h]xpub.../0/*)` and `.../1/*`.
- **Key-origin annotation `[fingerprint/derivation]` is mandatory, not cosmetic.** Without it a
  hardware signer cannot locate its own key in the PSBT and will refuse to sign (RESEARCH §C.2).
  Every descriptor Archipelago emits must carry it. The current code emits descriptors with **no
  key-origin prefix** (`core/archipelago/src/api/rpc/bitcoin.rs:230-231`) — a second concrete
  reason Phase 1 must rewrite that function.
- Descriptor checksums: obtain via `getdescriptorinfo` before `importdescriptors`, as the
  existing code correctly already does (`bitcoin.rs:234-259`). Core rejects a wrong checksum.

### 3.2 Tier 2 — `wsh(sortedmulti(k, ...))` multisig

- Script: `wsh(sortedmulti(k, xpub1/…, xpub2/…, xpub3/…))`.
- **Why `sortedmulti` over ordered `multi`:** `sortedmulti` (BIP-67) lexicographically sorts the
  keys in the resulting script, so the wallet can be **recreated without preserving xpub order**.
  With ordered `multi`, losing the order loses the wallet even though every key survives — a
  recovery failure mode that is entirely avoidable. Use `sortedmulti` unless a specific
  cosigner demands ordered `multi`.
- **BIP-48 derivation** for multisig accounts: `m/48'/<coin>'/<account>'/<script_type>'`, with
  `2'` = P2WSH. Every coordinator (Sparrow, Nunchuk, Caravan, Specter) expects this path; using
  anything else means users cannot import their Archipelago multisig anywhere else.
- Descriptor exchange: each cosigner contributes an xpub **with key origin**; the coordinator
  assembles the descriptor and every participant imports the identical descriptor string. All
  participants must be able to export the descriptor for backup — a multisig backup is the
  descriptor plus each seed, and users who back up only seeds lose funds.
- Reference to copy rather than re-derive: Bitcoin Core's `doc/multisig-tutorial.md` and the
  functional test `test/functional/wallet_multisig_descriptor_psbt.py`, which is the exact RPC
  sequence in executable form (RESEARCH §C.3).

### 3.3 Taproot / MuSig2 multisig — future work, deliberately

`tr(...)` descriptors exist, but **`[UNVERIFIED]`** — the 2026 state of MuSig2 key-aggregation
support in Core's descriptor wallets and across hardware signers was not confirmed (RESEARCH
§C.3, Open Question 4). Shipping a multisig scheme whose recovery depends on unconfirmed
signer support is how users lose money years later. **Ship `wsh(sortedmulti(...))`.** Revisit
taproot multisig when Core's support and at least two independent hardware signers can be
verified against a real device.

---

## 4. Air-gapped transport

### 4.1 The format decision

| Format | Mechanism | Verdict |
|---|---|---|
| **BC-UR v2** (Blockchain Commons) | **Fountain-coded** (rateless erasure). Any sufficient subset of frames reconstructs the payload; order-independent. | **Recommended primary.** |
| **BBQr** (Coinkite) | Payload split across sequential frames; receiver accumulates and must obtain each missing frame. | Support for Coldcard interop; not the primary. |
| microSD / file (`.psbt`) | Plain file exchange. | **Mandatory fallback, always offered.** |
| SeedQR | Static QR of mnemonic word indices. | **Seed transport only, not PSBT.** Already shipped (`neode-ui/src/utils/seedqr.ts:11`). |

**Recommendation: BC-UR v2 as primary, BBQr for Coldcard interop, file always available.**

The justification is specific to Archipelago's hardware reality rather than generic. The
companion app scans QR from a phone camera, frequently at a TV or in a rack cupboard, in poor
light. BBQr's sequential model means a single missed frame stalls the user until that exact
frame comes round again — the failure mode is "keep pointing the camera and hope". BC-UR's
fountain coding means *any* sufficient number of frames reconstructs the payload, so a bad
scanning environment degrades into "takes longer" instead of "gets stuck". That difference is
what makes an air-gap workflow tolerable enough that users keep using it — which, per §0, is
the whole point.

**`[UNVERIFIED]`** — device support matrix. Confirmed from RESEARCH §C.4: Coldcard → BBQr
(native) + microSD + NFC; Foundation Passport and Keystone → UR; SeedSigner → BC-UR v2. Jade,
Krux, BitBox, Ledger and Trezor support was **not** confirmed and must be verified against real
hardware before any of them is listed as supported in the UI.

### 4.2 QR density — animated is mandatory, not a nice-to-have

A QR code maxes out around ~2,953 bytes at the largest version with the lowest error correction,
and far less at densities a phone camera can actually read across a room. **A real multi-input
multisig PSBT routinely exceeds that.** Therefore:

- **Multi-frame animated QR is mandatory.** Single-QR PSBT export must not be the only path.
- **A file fallback must always be offered**, on every export screen, with equal visual weight.
  microSD/file has no density limit and is the most reliable route for large PSBTs.
- The UI must show frame progress (e.g. "142 of 210 frames received") so a stalled scan is
  visibly stalled rather than mysteriously slow.

### 4.3 Consistency with the first-party signer

`docs/hardware-signer-design.md` specifies a QR-only, camera-in/screen-out air-gapped signer
(TROPIC01 + ESP32-S3), and lists "Animated/multi-part QR strategy for large PSBTs" as an open
item (`docs/hardware-signer-design.md:167`) and "Define QR payload formats for both roles" at
`:165`. **This document answers both for Bitcoin: BC-UR v2 primary, BBQr for Coldcard interop.**
That signer, when built, should implement the same format so the same node-side transport code
serves third-party signers and the first-party device identically. Its dual Nostr-signing role
(`docs/hardware-signer-design.md:110-148`) is out of scope here but shares the transport layer,
which is an argument for implementing transport as a payload-agnostic module.

---

## 5. LND — what is and is not achievable

### 5.1 Decision table

| Capability | Achievable? | Detail |
|---|---|---|
| Watch-only `lnd` + separate signer instance | **Yes** | `remotesigner.*` on the watch-only node; the signer needs no chain backend (`bitcoin.node=nochainbackend`). |
| Signer fully offline | **No** | The signer must accept a **live inbound gRPC connection**. "Offline except for one connection" is not an air-gap. |
| Air-gap channel / revocation / HTLC keys | **No** | These live in the signer and must sign **on demand, at protocol speed**. A routing node cannot tolerate human-in-the-loop signing. **This is the hard limit of the entire design.** |
| PSBT funding of channels | **Yes** | `lncli openchannel --psbt`; `PsbtShim` via `FundingStateStep`; batch by passing the returned PSBT as `base_psbt`. |
| Open a channel with zero LND wallet balance | **Yes** | The `--psbt` flow explicitly supports funding from an external wallet. |
| **Self-broadcast the funding transaction** | **NEVER** | LND must publish it "in the proper funding flow order **or the funds can be lost**". Encode as a hard UI rule — see §5.3. |
| Sign arbitrary messages / on-chain txs externally | **Yes** | `signrpc` / `walletrpc` (`signer:generate`, `onchain:write`). |
| Move private keys between instances after init | **No** | Not supported. |
| Add accounts dynamically without wallet reconstruction | **No** | Not supported. |

Source: RESEARCH §C.5, from LND `docs/remote-signing.md` and `docs/psbt.md`.

### 5.2 Required accounts and the taproot gotcha

Remote signing requires xpubs for level-3 derivation accounts: purpose **49** (NP2WKH), **84**
(P2WKH), **86** (P2TR), and **1017** accounts 0-255 (node identity, channels, watchtower,
HTLCs). Setup is `lncli wallet accounts list > accounts-signer.json` on the signer, then
`lncli createwatchonly accounts-signer.json` on the watch-only node. A minimal signer macaroon
is `lncli bakemacaroon --save_to signer.custom.macaroon message:write signer:generate
address:read onchain:write`.

**Taproot gotcha:** requires LND v0.15.3-beta+ and a manual
`lncli wallet accounts import --address_type p2tr <xpub> default` on upgrade, or the node fails
with `"account 0 not found"`. Archipelago pins LND `0.18.4` (`apps/lnd/manifest.yml:4`), so the
version floor is satisfied; the manual import step is not automatic and must be part of any
migration runbook.

Migrating an existing node is `remotesigner.migrate-wallet-to-watch-only=true`, which **purges
private key material in place** — one-way, and therefore gated behind a verified backup.

### 5.3 The self-broadcast rule is a hard UI constraint

Archipelago already exposes `lnd.create-psbt` and `lnd.finalize-psbt`
(`core/archipelago/src/api/rpc/dispatcher.rs:136-137`,
implemented in `core/archipelago/src/api/rpc/lnd/wallet.rs:605` and `:711`), and the finalize
handler already broadcasts (`core/archipelago/src/api/rpc/lnd/wallet.rs:757`). That is correct
for an **on-chain** send and **catastrophic** for a channel-funding PSBT.

**Rule:** any PSBT produced by the channel-funding flow must be tagged as such end-to-end, and
every broadcast path must refuse to broadcast a channel-funding PSBT. The refusal belongs in the
Rust orchestrator, not in the UI, and it should be a type-level distinction (a distinct
`ChannelFundingPsbt` wrapper) rather than a boolean anyone can forget to check. This is the one
place in this document where a mistake destroys funds rather than exposing them.

### 5.4 On-chain vs Lightning — two genuinely different tiers

The design splits cleanly, and the split must be visible to users:

| | **On-chain balance** | **Lightning balance** |
|---|---|---|
| Key exposure | Can be fully cold — key never on the node | **Necessarily hot** — channel/revocation/HTLC keys must sign at protocol speed |
| Protection mechanism | Watch-only descriptors + PSBT + external signer | Remote signing *relocates* keys to a hardened host; it does not remove hot exposure |
| Honest claim | "Cold storage" is accurate | "Cold storage" is **false** |

**The exact sentence the UI should use:**

> *A Lightning routing node's channel keys are necessarily hot. Remote signing moves them to a
> hardened machine; it does not make them cold. Only your on-chain balance can be genuinely
> protected by an offline signer.*

**Any copy implying a routing node's channel keys are cold is misleading and must not ship.**
This is not pedantry: a user who believes their Lightning balance is cold will keep more in it
than they would otherwise, which is precisely the miscalibration that turns an incident into a
loss. The Coldcard incident is a good reason to be conservative in this copy rather than
optimistic.

---

## 6. The hot wallet as the explicitly-secondary option

The hot wallet stays. Removing it would push users to worse tools. It is framed, limited, and
labelled as secondary.

1. **Hard separation of on-chain and Lightning balances** in the data model and in the UI.
   **Never one blended number.** They have different key exposure (§5.4), different recovery
   stories, and different risk. A single "balance" figure silently averages a cold number with a
   hot one, which is a lie of composition.
2. **Server-enforced spend limits.** Per-transaction and rolling-daily, enforced in the Rust
   orchestrator. Anything above the limit is **forced onto the PSBT path** — not blocked, not
   warned-and-allowed: routed. Archipelago already rate-limits financial RPCs
   (`core/archipelago/src/rate_limit.rs:62-69`: `wallet.send` 5/300s, `lnd.sendcoins` 5/300s,
   `lnd.openchannel` 3/300s), so the enforcement point exists; value limits are the addition.
3. **Reuse the existing at-rest envelope.** Argon2 + ChaCha20-Poly1305, per-blob salt and nonce
   from `OsRng`, `0600` (`core/archipelago/src/seed.rs:238-269`, `:318-324`). Do not invent a
   second envelope. See audit finding **F-05** on aligning the Argon2 parameters with ADR-005
   before this tier carries meaningful value.
4. **Zeroization on every path.** The existing code is the standard to match:
   `core/archipelago/src/seed.rs:262`, `:292`, `:384`, `:401`;
   `core/archipelago/src/api/rpc/bitcoin.rs:222`, `:284`.
5. **Explicit tiering in the UI**, named rather than hidden:
   - **Cold** — watch-only + external signer. On-chain only. The default for new wallets.
   - **Warm** — hot on-chain key in the daemon's envelope, under spend limits.
   - **Hot** — Lightning. Unavoidably hot; labelled as such.

### 6.1 Nudging toward PSBT without punishing the hot path

The failure mode to avoid is a safe path so tedious that users disable it, and a hot path so
nagged-at that users stop reading warnings. Concretely:

- **Default new wallets to cold.** Do not make the user opt in to safety. This is the direct
  lesson of §0.
- **One-time framing, not per-transaction nagging.** Explain the tiers once, at setup, and then
  show a small persistent tier badge. Repeated modal warnings train users to dismiss modals.
- **Make the limit the teacher.** When a spend exceeds the warm limit, route it to the PSBT
  flow with a neutral explanation ("this amount uses your signing device") rather than an error.
  The user learns the tier boundary by using it.
- **Never make the hot path feel broken.** A small Lightning payment should be one tap. If
  everyday use is painful, users move their funds to software that does not have any of this.
- **Let the user raise limits, deliberately.** A limit the user cannot adjust gets worked around
  entirely; a limit they must consciously raise is a decision they remember making.

---

## 7. Migration for existing users

### 7.1 What the incident does and does not imply here

**Be precise, because both errors are costly.**

- **A software fix does not repair an already-generated seed.** If a seed was produced by a
  defective RNG, updating the software leaves it exactly as guessable. This is why Coinkite told
  users to migrate rather than merely update.
- **The audit found no such defect in Archipelago.** The internal entropy audit's
  §2 and §4 record that every first-party key-generation call site draws from a genuine CSPRNG,
  that the mnemonic is a real 256-bit value, and that `[ARCHY-1]` is a *structural* risk with no
  present exploitability.

**Therefore: no Archipelago user needs to rotate their seed because of the COLDCARD incident.**
Do not ship a banner implying otherwise. Over-alarming has a real cost — it triggers unnecessary
fund movements, which have their own fee, privacy, and fat-finger risks, and it burns the
credibility needed for a real advisory later.

**Who this section *does* apply to:**

1. **Users whose seed was generated on a Coldcard and imported into Archipelago**, on affected
   firmware. Their seed is at risk from T1, independent of Archipelago's own code quality. They
   should follow Coinkite's guidance and the sequence in §7.2.
2. **Every user, at the point Phase 1 lands** — because the account xprv is currently imported
   into Bitcoin Core (`core/archipelago/src/api/rpc/bitcoin.rs:229-231`, §0). Moving to
   watch-only does not require a new seed; it requires re-creating the Core wallet without
   private keys. That is a *wallet* migration, not a *key* migration, and it must be presented
   as such — see §7.3.

### 7.2 Seed-rotation sequence (only when a seed is actually suspect)

Order matters; each step de-risks the next.

1. **Generate a new key** on trusted, fixed hardware or software.
2. **Verify the backup** — restore it into a second wallet and confirm it reproduces the same
   first receive address before sending anything.
3. **Verify a receive address** on the signing device's own screen, not only on the host.
4. **Send a small test transaction** to the new wallet and confirm it arrives and is spendable.
5. **Migrate the funds** from the old wallet to the new one.
6. **Retain the old backup** until every output is confirmed spent and the new wallet's balance
   is verified. Destroying the old backup early is the most common way this sequence loses money.

If Lightning is in use, closing channels is part of step 5 and is slow (force-closes carry
timelocks). Budget for it; do not present channel migration as instantaneous.

### 7.3 Wallet migration to watch-only (Phase 1) — *not* a seed rotation

For every existing user, when Phase 1 lands:

1. Confirm the encrypted seed backup exists and is decryptable
   (`core/archipelago/src/seed.rs:341-357`, `seed_exists` at `:360-362`).
2. Derive the account xpub and build the key-origin-annotated descriptors.
3. Create a **new** wallet with `disable_private_keys = true` and import the public descriptors.
4. Rescan, and confirm the new watch-only wallet reports the **same balance and the same UTXO
   set** as the old one. Do not proceed on any mismatch.
5. Only then unload and remove the private-key-bearing wallet from Core.

**The user's seed does not change and their funds do not move.** Say that plainly in the UI —
the natural user fear on seeing any wallet-migration prompt is that their money is being touched.

---

## 8. Phased rollout

Each phase names a goal, its dependencies, candidate requirements, and whether it needs real
hardware. This section is the input a future `/gsd-plan-phase` consumes.

### Phase 1 — Descriptor watch-only read path

**Goal:** the node's Bitcoin Core wallet holds no private keys; the daemon's encrypted store is
the only place the BIP-84 key exists.

**Dependencies:** none. **This is the highest-value change in the document and it unblocks
everything else** — no external-signer flow is meaningful while Core holds the xprv.

**Candidate requirements:**
- `createwallet` is called with `disable_private_keys = true` (currently `false`,
  `core/archipelago/src/api/rpc/bitcoin.rs:203`).
- Imported descriptors carry the **xpub** and a key-origin annotation
  `[fingerprint/84h/0h/0h]` (currently a bare xprv with no origin, `bitcoin.rs:229-231`).
- A migration path re-creates the wallet watch-only and verifies balance/UTXO parity before
  removing the old wallet (§7.3).
- The account xprv is never written to Core and never leaves the Argon2 envelope except in
  memory, zeroized.
- Regression test: the wallet cannot sign — a signing attempt against it fails structurally.

**Real hardware:** yes, for the migration — verify on a node with real UTXO history (`.228`).

### Phase 2 — PSBT construct and export

**Goal:** the node can build a funded PSBT from the watch-only wallet and hand it out.

**Dependencies:** Phase 1.

**Candidate requirements:**
- `walletcreatefundedpsbt` wired with explicit fee control, reusing the existing fee-preset UI.
- `analyzepsbt` exposed and used as the single source of UI state (§1.3).
- Export as base64 and as a `.psbt` file download.
- A PSBT review screen showing inputs, outputs, fee, change, and destination — the human check
  the whole air-gap model depends on.

**Real hardware:** no (regtest/testnet sufficient).

### Phase 3 — External-signer import and finalize

**Goal:** a signed PSBT from a third-party signer completes the loop and broadcasts.

**Dependencies:** Phase 2.

**Candidate requirements:**
- Import a signed PSBT by file upload; `combinepsbt` where multiple parts arrive.
- `finalizepsbt` + `sendrawtransaction`, with the channel-funding refusal of §5.3 in place from
  day one — not retrofitted.
- Clear error surfacing when `analyzepsbt` says signatures are still missing.

**Real hardware:** **yes** — must be verified end-to-end against at least one real signer
(Coldcard or Passport) before it is offered to users.

### Phase 4 — Air-gap transport (BC-UR v2 + BBQr)

**Goal:** the loop closes over QR, with a file fallback, in the companion app.

**Dependencies:** Phase 3.

**Candidate requirements:**
- BC-UR v2 encode (node) and decode (companion), fountain-coded, with visible frame progress.
- BBQr decode for Coldcard interop.
- File fallback offered with equal weight on every export and import screen (§4.2).
- Payload-agnostic transport module, so `docs/hardware-signer-design.md`'s Nostr role can reuse
  it later without a rewrite.

**Real hardware:** **yes** — QR density and scan reliability cannot be evaluated in an emulator.
Verify at realistic distance and lighting, including the TV-kiosk case.

### Phase 5 — Multisig

**Goal:** `wsh(sortedmulti(k, ...))` wallets with BIP-48 paths and descriptor exchange.

**Dependencies:** Phase 4 (large multisig PSBTs are exactly the case that needs robust transport).

**Candidate requirements:**
- Create/import a `wsh(sortedmulti(...))` descriptor with per-key origin annotations.
- BIP-48 `m/48'/0'/<account>'/2'` derivation for Archipelago's own key.
- Descriptor export/backup UX that states plainly that the descriptor is part of the backup.
- `combinepsbt` across N signers with `analyzepsbt`-driven progress.
- Interop test against at least one external coordinator (Sparrow or Nunchuk).

**Real hardware:** **yes** — two independent signers minimum.

### Phase 6 — LND remote signing

**Goal:** LND runs watch-only with a separate signer instance, with honest UI copy.

**Dependencies:** Phase 1 (the on-chain story must be settled first; doing Lightning first would
teach users the wrong mental model).

**Candidate requirements:**
- Signer instance provisioning (`bitcoin.node=nochainbackend`, minimal macaroon) and watch-only
  setup via `createwatchonly`.
- Explicit p2tr account import step (§5.2), or a documented failure with a fix-it action.
- `remotesigner.migrate-wallet-to-watch-only=true` migration, gated behind a verified backup —
  it purges key material in place and is one-way.
- UI copy carrying the §5.4 sentence verbatim, and no copy anywhere claiming Lightning funds are
  cold.

**Real hardware:** **yes** — two hosts, and a real channel.

### Phase 7 — Hot-wallet limits and tiering

**Goal:** the hot path is bounded, labelled, and routes large spends to PSBT.

**Dependencies:** Phase 3 (there must be a PSBT path to route *to*).

**Candidate requirements:**
- Server-enforced per-transaction and rolling-daily limits, with over-limit spends routed to the
  PSBT flow rather than rejected (§6.1).
- On-chain and Lightning balances separated in the data model and never summed in the UI.
- Cold / warm / hot tier badges.
- New wallets default to cold.

**Real hardware:** no, beyond normal on-node verification.

### Sequencing note

Phases 1-4 are the spine and should run in order. Phase 6 (LND) and Phase 7 (limits) can run in
parallel with Phase 5 (multisig) once Phase 3 lands. Phase 1 alone materially improves the
current security posture and should not wait for the rest.

---

## 9. Related documents

- The internal entropy and seed-generation audit — motivating this spec; see F-05
  (Argon2 parameters) and the F-13 addendum on the xprv-in-Core issue.
- `docs/hardware-signer-design.md` — the first-party TROPIC01 air-gapped signer; §4.3 above
  answers two of its open items.
- `docs/adr/005-chacha20-backup-encryption.md` — the at-rest envelope §6 reuses.
  — Part C is the source for the Core RPC table, the LND capability matrix, and the air-gap
  format comparison.
