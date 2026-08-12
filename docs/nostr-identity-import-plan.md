# Add an existing Nostr identity to the node — UX & implementation plan

**Status:** plan only (2026-07-16), no code. Companion research: `docs/nostr-signer-login-research.md`.

## Where it lives

The **Nostr Identities** screen (`Web5Identities.vue`, backed by `identity.list` /
`identity.create`). Today every identity is **seed-derived** (`identity_manager.rs`
derives ed25519 + nostr keys from the BIP-39 master seed at an index). "Add existing"
introduces a second class of identity: one whose key material comes from *outside* the
seed.

## Two import kinds (both needed, different guarantees)

1. **Full import (nsec)** — the node holds the secret key. The identity behaves exactly
   like a seed-derived one (can sign in embedded apps, publish, encrypt). NOT covered by
   seed backup — flag it visibly and include it in the encrypted node backup.
2. **Linked signer (npub only)** — the node stores just the public key; signing is
   delegated to the user's own signer (browser extension NIP-07, or a NIP-46 remote
   signer later). Zero key custody; some features (background publishing) unavailable —
   the UI should badge what works.

## The UX (matching the house style)

**Entry point:** next to "Create identity" on Nostr Identities, an **"Add existing"**
glass-button. Opens a modal with three tabs (same tab pattern as the send/receive
modals):

1. **Browser extension** (default when `window.nostr` exists)
   - One button: "Connect with extension". Flow: `getPublicKey()` → show the npub +
     resolved profile (kind-0 fetched via the node's relays: avatar, name — instant
     recognition) → "Add this identity".
   - Creates a **linked signer** identity. A challenge signature
     (`signEvent` on a throwaway event) proves key possession before adding — never add
     an unverified npub as "yours".
2. **Secret key (nsec)**
   - Paste field (masked, `nsec1…` or hex), inline validation + derived npub preview
     with the same kind-0 profile card before confirming.
   - Scary-clear copy: "Your key will be stored on this node, encrypted at rest. It is
     NOT part of your seed backup — back it up separately." Confirm step requires the
     profile card to load or an explicit "add anyway".
   - Creates a **full** identity.
3. **Public key (npub)** — watch-only
   - Paste an npub for a linked identity without any signer attached yet (useful to
     reserve the profile, upgrade to extension/NIP-46 signing later).

**After adding:** the identity appears in the same grid with a small origin badge —
`seed` / `imported` / `linked` — and the imported profile picture/name pulled from
relays. Everything else (picker in apps, rename, avatar) behaves uniformly.

**Removal:** existing delete flow; for `imported` identities the confirm dialog warns
the key is destroyed unless exported first (offer "Export nsec" in the identity's detail
sheet, gated behind password re-entry).

## Backend work

- `identity_manager.rs`: identity records gain `origin: Seed { index } | Imported |
  Linked`, optional `nostr_secret_hex` absent for Linked. Storage: reuse the existing
  encrypted identity file; imported secrets included in node backup.
- New RPCs:
  - `identity.import-nostr` `{ nsec | npub, name?, verify_sig? }` → validates, derives
    npub, rejects duplicates (same pubkey as any existing identity), returns the new
    identity.
  - `identity.fetch-profile` `{ pubkey }` → kind-0 lookup via `nostr_relays.rs` for the
    preview card (frontend could also do this, but the node already has relay plumbing
    and avoids CORS).
- `identity.nostr-sign` (used by the iframe NIP-07 bridge): for `Linked` identities
  return a typed error the bridge translates into "ask the user's extension instead" —
  phase 2; phase 1 simply hides linked identities from the in-app signer picker.

## Demo mode

Mock `identity.import-nostr` + `identity.fetch-profile` in mock-backend.js (canned
profile: picture + name for any pasted npub) so the whole add-existing flow is
demoable without real relays.

## Phasing

1. **Phase 1 (small):** nsec + npub tabs, origin badges, backup inclusion, mock.
2. **Phase 2:** extension tab with possession-proof + kind-0 preview cards everywhere.
3. **Phase 3:** NIP-46 remote-signer identities + login integration (shares the QR
   plumbing from the signer-login work).
