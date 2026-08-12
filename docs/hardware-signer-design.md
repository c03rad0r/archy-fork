# Archipelago Hardware Signer — Design Notes (PSBT + Nostr)

> Status: **exploratory / spec stub** (2026-06-24). No code yet. This captures the
> hardware-selection reasoning and architecture for a small, air-gapped, super-secure
> signing device built around the Tropic Square **TROPIC01** secure element, intended
> to integrate with Archipelago as an external signer.

## 1. Goal

A small, super-secure, air-gapped handheld device that:

- Signs **Bitcoin PSBTs** for the Archipelago wallet.
- (Stretch / dual-function) Signs **Nostr events** for the node's sovereign identity.
- Communicates **only via QR** (camera in, screen out) — no USB data path, no radio in
  use. Pure air-gap, same threat model as SeedSigner but with a real audited secure element.
- Anchors key-at-rest security and RNG in the **TROPIC01** open-source secure element.

## 2. The critical curve caveat

**TROPIC01's signing engine supports P-256 (ECDSA) and Ed25519 (EdDSA) — NOT secp256k1.**
Bitcoin and Nostr both require secp256k1. Therefore:

- The secure element is the **vault + RNG + attestation**, not the signer.
- The seed lives encrypted inside TROPIC01 (tamper mesh, pairing, secure channel).
- The host MCU does the actual **secp256k1 ECDSA (Bitcoin)** and **Schnorr / BIP-340
  (Taproot + Nostr)** signing in software.
- TODO before committing: re-check whether a firmware revision adds secp256k1 — it's
  open RISC-V silicon and has been a community ask. If/when it lands, this design gets
  materially stronger (signing in-silicon).

## 3. Architecture (two chips)

```
   [ QR in ]  -->  Camera (OV2640)
                        |
                   Host MCU (ESP32-S3)  <--SPI-->  TROPIC01 (Mini Board)
                        |                              (seed vault, RNG,
                   Touch screen                         secure channel, attest)
                        |
   [ QR out ] <--  Display (signed PSBT / signed event)
```

- **Host MCU** drives camera, touch screen, QR parse/render, PSBT + Nostr logic, and
  the secp256k1/Schnorr signing.
- **TROPIC01** protects the seed at rest and supplies the TRNG + secure boot/attestation
  over an authenticated+encrypted SPI channel.

## 4. Hardware selection

### 4.1 MCU — the camera-ease vs radio-purity fork

| | **ESP32-S3** (recommended) | **RP2350** |
|---|---|---|
| Camera | Native DVP interface; huge QR-scan code ecosystem | No camera peripheral — bit-bang over PIO (harder) |
| Radios on die | WiFi + BLE present (con for air-gap purists) | **None** |
| Security | Secure boot, flash encryption | Cortex-M33 + TrustZone, signed boot, OTP |
| secp256k1 in SW | Fine (240 MHz dual-core) | Fine (150 MHz dual-core M33) |
| Price (chip / board) | ~$3 / ~$6 | ~$1.20 / ~$5 |

**Pick: ESP32-S3 (N16R8 — 16MB flash / 8MB PSRAM).** The camera is the hard part of the
build and the S3 is the only cheap MCU with a native camera interface. PSRAM matters for
holding camera frames during QR decode. The on-die radio is the one downside — acceptable
because trust is anchored in the TROPIC01, not the MCU. If radio-on-die is a hard no,
switch to RP2350 and accept harder camera bring-up. (SeedSigner deliberately chose a
no-WiFi Pi Zero 1.3 for exactly this reason — the concern is legitimate.)

### 4.2 Camera

- **OV2640** 2MP module — standard ESP32-cam sensor, code everywhere. ~$2–4.

### 4.3 Thin touch screen

Pick by review legibility (the whole security value is the human verifying address +
amount before tap-to-approve):

- **2.0" IPS ST7789 capacitive, 240×320 — recommended.** Easiest to read a full Bitcoin
  address/amount. ~$8–12.
- 1.69" rounded-rect IPS ST7789 + CST816 cap touch — best size/compactness balance.
  ~$7–10.
- 1.28" round (GC9A01 + CST816) — smallest/thinnest but **too cramped** for address
  verification; skip for a signer.

**Do not go below ~1.69".** Use capacitive (not resistive) touch for a thin glass-front
tap-to-confirm feel.

### 4.4 TROPIC01 board (from the Tropic Square order form)

All options speak SPI (wires to the S3 the same way). Two-board plan:

- **Development: TROPIC01 USB DevKit (€50)** — STM32 + USB-to-SPI stick. Bring up the
  secure-element stack (pairing, key gen, secure channel) on a PC first, independent of
  the camera/screen work.
- **Final device: TROPIC01 Mini Board (€9.50)** — small easy-to-solder module exposing
  SPI; solder straight to the S3's SPI bus inside the enclosure.
- Skip: Standalone Sample (€5, bare QFN — needs hot-air), Raspberry Pi / Arduino Shields
  (wrong host form factor), MIKROE Click (€20, only if you have a mikroBUS rig).

### 4.5 Rough BOM

| Item | ~Cost |
|---|---|
| ESP32-S3 N16R8 board | $6–8 |
| OV2640 camera | $2–4 |
| 2.0" cap-touch IPS | $8–12 |
| TROPIC01 Mini Board | €9.50 |
| (Dev only) TROPIC01 USB DevKit | €50 |

**Core device BOM ≈ $20–30** + TROPIC01 Mini Board, before enclosure/battery.

## 5. Dual-function: Nostr signer

Genuinely viable and a natural fit — **Nostr signs with Schnorr/BIP-340 over secp256k1,
the same scheme as Bitcoin Taproot.** So Nostr signing reuses the secp256k1+Schnorr code
already needed for Bitcoin — near-zero marginal firmware cost.

### 5.1 One seed → two separated keys

From the single seed in the TROPIC01:

- **Bitcoin:** BIP-32/39/84 HD derivation.
- **Nostr:** **NIP-06** deterministic derivation (`m/44'/1237'/…`) → `nsec`/`npub`.

One backup, two independent identities, no cross-contamination.

### 5.2 Cold vs hot tension

| | Bitcoin | Nostr |
|---|---|---|
| Frequency | Rare, high-value | Frequent, often interactive |
| Natural transport | QR / PSBT — air-gap perfect | Apps want real-time signing |
| Air-gap comfort | Excellent | Fine for occasional events, painful for chat |

Two possible modes:

1. **Air-gapped QR Nostr signer (recommended):** app shows unsigned-event QR → camera
   scan → touch approve → signed-event QR back. Great for high-value/infrequent events
   (root identity, profile/metadata, key rotation, announcements). Keeps 100% air-gap.
2. **Connected NIP-46 "bunker" over USB/serial:** enables interactive real-time signing
   but **breaks the air-gap** and reintroduces the USB/radio attack surface. Not
   recommended for this device.

### 5.3 Recommendation

Keep it **cold for both roles.** The device guards the Bitcoin spending key *and* the
high-value Nostr **identity** key — neither ever touches a network. Day-to-day Nostr
chatter uses a separate hot software key; the hardware device protects only the
identity-defining key you can't afford to leak. Avoids putting a hot key next to cold
Bitcoin funds.

## 6. Archipelago integration

- Slots in as an **external signer** path alongside the existing wallet flow — does not
  touch the orchestrator. Archipelago builds PSBT → renders QR (animated QR for large
  txs) → device scans → touch review → returns signed-PSBT QR → Archipelago broadcasts.
- Especially apt given Archipelago's Nostr/Blossom catalog + node-identity direction
  (see `dht-distribution-design.md`): the device becomes the **hardware root of trust**
  for both halves of a node's identity — its `npub`/DID and its Bitcoin keys — aligning
  with the sovereign/secure/rootless north star.

## 7. Open items / next steps

- [ ] **Pin budget:** confirm the S3 GPIO/SPI budget fits camera DVP + display SPI +
      TROPIC01 SPI simultaneously. (Biggest unknown before buying.)
- [ ] Confirm current TROPIC01 firmware secp256k1 status (could remove the §2 caveat).
- [ ] Define QR payload formats for both roles (PSBT vs unsigned Nostr-event JSON) so a
      single scan→approve→return firmware loop handles either transparently.
- [ ] Animated/multi-part QR strategy for large PSBTs.
- [ ] Seed provisioning ceremony into the TROPIC01 (gen on-device via its TRNG; never
      import in clear).
- [ ] Enclosure + power (battery vs USB-power-only-while-airgapped).
- [ ] Decide: ESP32-S3 (radio present) vs RP2350 (no radio, harder camera) — final call.
