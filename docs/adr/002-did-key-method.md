# ADR-002: DID Key Method for Node Identity

**Status**: Accepted
**Date**: 2026-03

## Context

Each Archipelago node needs a cryptographic identity for peer authentication, federation, and verifiable credentials. Multiple DID methods exist (did:web, did:ion, did:key, did:peer).

## Decision

Use `did:key` as the primary DID method.

## Consequences

### Positive
- **Self-contained**: The DID document is derived entirely from the public key — no external resolution needed
- **Offline-capable**: Works without internet, aligning with sovereignty principles
- **Simple**: No registration, no blockchain, no web server required
- **Fast**: DID resolution is a local computation, not a network request
- **Ed25519**: Uses Ed25519 keys which are fast, compact, and well-supported

### Negative
- **No key rotation**: The DID is bound to a single key; rotating requires a new DID
- **No service endpoints in DID**: Cannot embed service URLs in the DID document itself
- **No revocation**: Cannot revoke a did:key without out-of-band mechanisms

### Mitigation
- Use federation trust lists for key management and revocation
- Store service endpoints (onion address, pubkey) separately in federation state
- Support migration to did:peer or did:web in future versions if key rotation is needed
