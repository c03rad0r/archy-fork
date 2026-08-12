# Sign in to the node with a Nostr signer — research & recommendation

**Status:** research only (2026-07-16), no code. Companion plan: `docs/nostr-identity-import-plan.md`.

## What's already in the tree (and what it isn't)

The IndeeHub "sign in with signer" work is the *inverse* of this feature: the node acts
as a NIP-07 **provider** for embedded iframe apps, signing with node-held keys
(`useNostrBridge.ts` postMessage bridge → `identity.nostr-sign` etc., picker UI in
`NostrIdentityPicker.vue`). It never verifies an external signer — but the UI patterns
(picker modal, QR rendering) and the backend crypto are reusable:

- **`nostr-sdk 0.44` is already a core dependency** (`nostr_handshake.rs` runs a real
  relay client) — schnorr event verification and NIP-46 client support are essentially
  free on the Rust side.
- Auth today is single-password + optional TOTP, and TOTP already uses a **two-step
  login** (`auth.login` → `auth.login.totp`) — the exact slot where a parallel
  `auth.login.nostr.*` path fits.
- The node can host its own relay (strfry app), and the frontend already bundles `qrcode`.

## Candidate flows, ranked by friction

### A. Browser extension (NIP-07) — lowest friction on desktop (2 clicks)
Login page shows "Sign in with extension" when `window.nostr` exists. Server issues a
random challenge → extension signs a **kind 22242** auth event carrying the challenge →
server verifies signature + challenge + `created_at` freshness + that the pubkey is
enrolled → normal session cookie. ~50 lines of frontend, ~80 lines of Rust. No relay
involved at all.

### B. QR scan with a mobile signer (NIP-46 `nostrconnect://`) — the headline UX (scan + 1 tap)
1. Backend generates an ephemeral client keypair and renders a
   `nostrconnect://<pubkey>?relay=<url>&secret=<rand>&perms=sign_event:22242&name=Archipelago` QR.
2. User scans with **Amber** (Android reference signer; Aegis/Nowser also scan;
   nsec.app is paste-based; Alby is *not* a NIP-46 signer).
3. Phone connects to the relay, acks the secret; backend requests one
   `sign_event:22242` over the encrypted NIP-46 channel, verifies, issues the session.

**Key architectural choice:** make the **Rust backend the NIP-46 client** (rust-nostr's
`nostr-connect` crate), talking to the relay over localhost — the browser only polls our
own RPC for "signer connected". No websocket/mixed-content issues in the Vue app.

**Relay topology:** no public relay is required by the spec — and public relays often
rate-limit ephemeral NIP-46 traffic. The node's own strfry is the ideal relay (private,
LAN-fast); the QR should carry a relay URL derived from the Host the browser used
(LAN IP / Tailscale IP — not `.local`, which Android often can't resolve).
**One empirical blocker to test first: does Amber accept plain `ws://` LAN relays?**
(Self-signed `wss://` will likely fail cert validation.) If not, route `wss://` through
the existing nginx/HTTPS cert story.

### C. Remembered NIP-46 session (persisted bunker pointer) — zero-tap repeat logins
Same as B but persists the pairing so future logins auto-approve. Adds state,
revocation surface, and "bunker offline = silent hang" failure modes. **Defer** — B
re-scans in ~5 seconds anyway.

## Recommendation

Ship **A + B behind one "Sign in with Nostr" button**; skip C for now. Password (+TOTP)
stays the permanent fallback — exactly as the user proposed, the signer is enrolled in a
step *after* password creation, never instead of it. The verification core is one shared
Rust function (sig + challenge + freshness + enrolled-pubkey → session).

- **Onboarding:** after the password (and seed) steps, an optional "Connect a signer"
  card: QR (nostrconnect) + "Use browser extension" + Skip. Success enrolls the npub as
  a login key.
- **Settings (next to TOTP):** list enrolled npubs (added date + method), "Add npub"
  (paste, becomes usable after a challenge-verify), "Connect another signer" (same
  QR/extension modal), "Remove" (requires password confirm; removing the last npub never
  locks the account — password always works).
- **Libraries:** hand-roll the 22242 event for NIP-07 (window.nostr is a browser global);
  rust-nostr `nostr-connect` for NIP-46. Avoid the 2.4 MB `nostr-login` JS bundle —
  wrong fit for a self-hosted box (defaults to public bunkers); it's UX prior art only.

## Security notes

- Only pubkeys enrolled **while authenticated** (or during onboarding) may log in —
  a simple `login_npubs` list next to the TOTP data in `auth.rs`.
- Challenge: 32-byte random, single-use, 2–5 min TTL, `created_at` ±60 s, deleted on
  first verify attempt; pin an origin/host tag. Rate-limit like password attempts.
- The `secret` in the nostrconnect URI is a bearer token — one QR per attempt, expires
  with the challenge.
- Policy call: signer approval should count as the second factor for TOTP accounts
  (possession of phone/extension key), so nostr login doesn't silently bypass TOTP.

## Open questions

1. Amber + `ws://` LAN relay — needs a 10-minute on-device test before committing.
2. Which relay URL to embed (LAN vs Tailscale vs onion) — derive from browser Host.
3. NIP-46 encryption: spec says NIP-44, some signers still NIP-04 — rust-nostr handles
   both; verify against current Amber.
4. Track draft **NIP-97 "Login with Nostr"** (matches this UX exactly, unmerged) —
   align, don't depend.

**Prior art:** no mainstream self-hosted node OS (Umbrel, Start9, Alby Hub) ships Nostr
QR login for its own UI — this would be genuinely differentiating, and every building
block is already in the tree.
