# ADR-008: Dual Key Strategy (Ed25519 + Secp256k1)

## Status

Accepted

## Context

Archipelago operates at the intersection of two cryptographic ecosystems:

- **Web5 / DIDs**: The W3C DID specification and Verifiable Credentials ecosystem predominantly uses **Ed25519** (EdDSA) for digital signatures
- **Nostr / Bitcoin**: The Nostr protocol and Bitcoin ecosystem use **secp256k1** (ECDSA/Schnorr) for signatures

A single key type cannot serve both ecosystems without conversion layers or compatibility issues.

## Decision

Maintain **two key pairs per node identity**:

1. **Ed25519** — Primary identity key for DID documents, verifiable credentials, federation authentication, and backup encryption
2. **Secp256k1** — Nostr-compatible key for relay publishing, node discovery, and Lightning Network interactions

### Key Derivation

- Both keys are derived from the same master seed during node initialization
- The Ed25519 key is the canonical identity (stored in the DID document)
- The secp256k1 key is linked to the DID via the Nostr profile (NIP-05 verification)

### Usage Matrix

| Operation | Key Used |
|-----------|----------|
| DID document signing | Ed25519 |
| Verifiable credentials | Ed25519 |
| Federation auth | Ed25519 |
| Backup encryption | Ed25519 (via X25519 DH) |
| Nostr event publishing | secp256k1 |
| Node discovery | secp256k1 (Nostr) |
| Lightning channel auth | secp256k1 |

## Consequences

### Positive

- Full compatibility with both Web5 and Nostr ecosystems
- No conversion layers or compatibility hacks needed
- Each key type is used in its native context (maximum security)
- Both keys from same seed — single backup protects both
- Future-proof: can add new key types without breaking existing ones

### Negative

- Two keys to manage instead of one
- Users need to understand which pubkey is which (mitigated by UI)
- Key rotation must update both key types
- Slightly larger DID documents (two verification methods)

### Mitigations

- UI presents a unified identity view — users see "My Identity" not "My Ed25519 Key"
- Backup system captures the master seed, from which both keys derive
- DID document includes both verification methods with clear purpose labels
