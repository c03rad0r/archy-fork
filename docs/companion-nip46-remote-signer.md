# Companion NIP-46 remote signer — the phone side of Nostr Bunker (#139)

**Status:** shipped in companion 0.5.28 (vc48). Issue: #139 ("Remote signer
with companion app?"). Background research:
[`nostr-signer-login-research.md`](nostr-signer-login-research.md) — flow B of
that document is exactly the flow this implements, with the companion playing
the role the research assigned to Amber.

## What shipped: the phone IS the bunker (remote signer)

The companion holds a nostr key (generate or import an `nsec`) and speaks
NIP-46 as the **remote signer**:

1. A NIP-46 client — the node's login page, per the research doc's flow B,
   or any `nostrconnect://`-emitting app — shows its pairing QR.
2. The phone scans it (hub → **Remote Signer** → *Scan pairing QR*), or any
   QR-scanner app hands the `nostrconnect://` URI over as a deep link
   (registered in the manifest).
3. The phone connects to the client's relay(s), subscribes to kind-24133
   events p-tagged to its own key, and sends the `connect` request carrying
   the secret — the same handshake direction rust-nostr's reference bunker
   uses (`NostrConnectRemoteSigner::send_connect_ack`), which is what the
   node's eventual nostr-connect client will wait for.
4. Requests arrive NIP-44-encrypted. Handled methods:
   - `connect` → "ack" (validates our pubkey + the pairing secret)
   - `get_public_key` → our pubkey
   - `describe` → method list
   - `ping` → "pong"
   - **`sign_event` → an approve/deny card — kind label, content, tags,
     time. Nothing signs without a thumb on Approve.** Deny replies
     `"denied"`; a second request while one is pending replies `"busy"`
     instead of replacing the visible card.
   - anything else → `"not authorized"` (nip04/nip44 encrypt/decrypt are
     deliberately NOT granted in v1).
5. Responses go back over the same encrypted kind-24133 channel.

The session lives while the app does (the login handshake takes seconds);
remembered-session auto-reconnect is the research doc's deferred flow C, and
stays deferred. NIP-04 is accepted on receive as a fallback (deprecated but
still spoken by real clients); all sending is NIP-44 v2.

## Where the code lives

- **Crypto:** `Android/rust/archy-fips-core/src/nostr.rs` — nsec/npub bech32
  keys, BIP-340 schnorr event signing (NIP-01 id serialization), NIP-44 v2
  payloads, NIP-04 fallback, `nostrconnect://` parsing. Host `cargo test`
  runs the official NIP-44 vectors (conversation/message keys, padded
  lengths, byte-exact encrypt vectors), the official BIP-340 sign vectors,
  and round-trip/tamper/failure cases.
- **JNI:** `com.archipelago.app.NativeCore` (same .so as the FIPS mesh).
- **Session:** `nostr/BunkerManager.kt` — OkHttp WebSocket relay client,
  JSON-RPC dispatch, approve/deny state.
- **UI:** `ui/screens/SignerScreen.kt` — key setup, npub/nsec display,
  pairing scan, session status, the approve/deny card.
- **Deep link:** `nostrconnect://` intent filter → SignerScreen.

## Security notes (conscious deviations, reviewed)

- Incoming events are **not** signature-verified before decryption — the
  same choice rust-nostr's reference bunker makes. The NIP-44 MAC is the
  actual gate: forging content that decrypts with a valid MAC requires one
  of the two conversation secrets. A future hardening pass may add event
  verification first.
- The signer secret lives in app-private DataStore (same storage model as
  the FIPS secret and node login passwords). It can additionally be sealed
  inside an encrypted backup (see
  [`companion-backup-restore.md`](companion-backup-restore.md)).
- `sign_event` approval is per-request and per-screen; there is no
  "remember this client" auto-approve in v1.

## End-to-end test harness (the node side doesn't exist yet)

`Android/tools/nip46-test-client.py` plays the node's role: generates the
pairing QR in your terminal, runs the full handshake, requests
`get_public_key` + `sign_event`, and verifies the returned signature with an
independent pure-Python BIP-340 implementation (no code shared with the
phone's Rust core; both are pinned to the same official test vectors).

```bash
python3 -m venv /tmp/nip46env
/tmp/nip46env/bin/pip install websockets qrcode
/tmp/nip46env/bin/python Android/tools/nip46-test-client.py   # --relay to override
```

Then on the phone: hub → Remote Signer → Generate key (once) → Scan pairing
QR → point at the terminal QR → Approve the incoming request. The harness
prints `END-TO-END PASS` when the phone-signed event verifies.

## Test checklist (on-device)

- [ ] Generate key → npub shows, copy works; import nsec → same npub.
- [ ] Harness handshake: pair → ack → `get_public_key` returns the phone's npub.
- [ ] `sign_event` request shows a legible card (kind label, content, tags);
      Approve → harness verifies the schnorr signature; Deny → harness sees
      `"denied"`.
- [ ] Deep link: open a `nostrconnect://…` URI from a QR app → SignerScreen
      with the pairing already starting.
- [ ] Wrong/foreign QR → clear error, no state change.

## Roadmap (node-side, tracked separately)

The node-side bunker hosting/login flow (research doc flows A+B, the
`auth.login.nostr` slot, relay topology on the node's own strfry) is roadmap
territory via the `companion-agent`-labeled tracker issues; when it ships,
the phone side here already speaks its language.
