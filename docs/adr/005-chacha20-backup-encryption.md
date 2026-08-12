# ADR-005: ChaCha20-Poly1305 for Backup Encryption

**Status**: Accepted
**Date**: 2026-03

## Context

Backups contain sensitive data (keys, credentials, app state) and must be encrypted at rest. Options: AES-256-GCM, ChaCha20-Poly1305, XChaCha20-Poly1305.

## Decision

Use ChaCha20-Poly1305 (AEAD) with Argon2id key derivation for backup encryption.

## Consequences

### Positive
- **Software performance**: ChaCha20 is faster than AES on hardware without AES-NI (common on ARM/SBCs)
- **Constant-time**: No timing side channels, unlike some AES implementations
- **AEAD**: Authenticated encryption ensures both confidentiality and integrity
- **Widely audited**: Used in TLS 1.3, WireGuard, and Signal Protocol
- **Simple implementation**: No padding, no CBC/CTR mode complexity
- **Argon2id KDF**: Memory-hard key derivation resists GPU/ASIC brute force attacks

### Negative
- **96-bit nonce**: Must ensure nonce uniqueness per encryption (random generation with collision check)
- **Not FIPS-certified**: Some enterprise environments require AES (not relevant for personal nodes)
- **Less hardware acceleration**: AES-NI on x86 can make AES faster on desktop CPUs

### Mitigation
- Generate random nonce per backup; store nonce alongside ciphertext
- Argon2id with high memory cost (64MB) and iterations (3) for password-to-key derivation
- Target hardware is mixed x86/ARM; ChaCha20's consistent performance is an advantage
