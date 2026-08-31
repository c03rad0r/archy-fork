# Companion backup & restore — the phone side of a border crossing (#128)

**Status:** shipped in companion 0.5.28 (vc48). Issue: #128 ("Graphene phone
backup/restore — part of the companion app or passport prime combo").

## The problem

The companion holds real secrets: node addresses and login passwords, the
phone's FIPS mesh identity (which nodes peer with), and — since 0.5.28 — the
remote-signer key. Losing the phone, or wiping it to cross a border, loses all
of it. On GrapheneOS there is no cloud backup and there should be none here
either: the export is a plain file the user saves wherever they choose (USB
drive, computer, a folder synced their way), sealed with a passphrase.

## The envelope — the node's, not a second format

Backups use the node's ADR-005 encrypted-backup envelope
(`core/archipelago/src/backup/identity.rs`), byte-for-byte:

- Argon2id key derivation (RustCrypto `argon2`, default params — same as the
  node's `Argon2::default()`), passphrase in, 16-byte random salt.
- ChaCha20-Poly1305 AEAD with a 12-byte random nonce.
- Envelope JSON:
  `{"version": 1, "kind": "companion", "encrypted": true, "blob": "<base64(salt‖nonce‖ct)>", "timestamp": "<rfc3339>"}`
- The native code (`Android/rust/archy-fips-core/src/backup.rs`) is the same
  crate family as the node's backup code; `decrypt` ignores unknown envelope
  fields, so a **node** identity backup (which carries `did`/`pubkey`/`kid`)
  also decrypts here — one envelope, two producers.

The encrypted payload is the companion's own JSON:

```json
{
  "app": "archipelago-companion",
  "payloadVersion": 1,
  "appVersion": "0.5.28",
  "createdAt": 1725100000,
  "servers": ["<serialized ServerEntry>", …],
  "active": "<serialized ServerEntry or null>",
  "fips": {"secret","npub","address","peers","partyPeers","partyName","partyListen"},
  "signer": {"secret": "<hex>"},
  "flags": {"introSeen": true}
}
```

## Where the code lives

- **Crypto:** `Android/rust/archy-fips-core/src/backup.rs` (+ JNI
  `NativeCore.backupEncrypt/Decrypt`). Host `cargo test` covers round-trip,
  wrong-passphrase, tampered-blob, node-shape envelopes, and salt/nonce
  freshness.
- **Payload/merge:** `BackupManager` (`Android/app/src/main/java/com/archipelago/app/data/BackupManager.kt`).
- **UI:** hub menu (three-finger) → **Backup & Restore** → SAF file picker
  (`CreateDocument` for export, `OpenDocument` for import), passphrase fields,
  verified-backup preview, result summary. The suggested export name is
  `archy-companion-backup-YYYYMMDD-HHmmss.json`.

## Restore semantics — never silently destructive

| What | On restore |
|---|---|
| Servers | Upsert (`ServerPreferences.upsertServer`): same npub merges (even when every address changed), new ones append |
| Active server | Set only when this phone has none (the fresh-wipe case) |
| FIPS identity | Restored only when this phone has none; node peers UNION by npub (`FipsPreferences.mergePeersJson`); party peers merge by npub |
| Signer key | Restored only when this phone has none |
| introSeen flag | Restored (no re-onboarding after a restore) |

The identity rules exist because a phone that already paired has a live mesh
identity nodes peer with; swapping it in from a backup would strand the
current pairing.

## Test checklist (on-device)

- [ ] Export → file saved, `version: 1`, `kind: companion`, base64 blob ≥ 44 chars.
- [ ] Wrong passphrase on import → "wrong passphrase" error, no state change.
- [ ] Correct passphrase → preview shows the right server count; restore on a
      second install (or after clearing app data) reconnects to the node
      without re-pairing, mesh included.
- [ ] Re-scan the node's QR after restore → no duplicate entry.
- [ ] The old phone's password for a node restores (login works on the new phone).

## Roadmap notes (node-side, tracked separately)

Node-side storage/quota/scheduling for companion backups ("passport prime
combo") is roadmap territory — this issue's scope was the phone side. The
envelope is ready to be a drop-in for the node's existing backup RPCs when
that lands.
