//! Authenticity layer for the DHT distribution plan (Phase 0).
//!
//! Content addressing (SHA-256 today, BLAKE3 later) proves downloaded bytes are
//! *intact*. It does not prove they were *authorized*. This module adds the
//! missing half: detached Ed25519 signatures over canonical JSON, verified
//! against a pinned **release-root** trust anchor.
//!
//! Layout:
//!   * [`anchor`]    — the pinned release-root public key (+ env override).
//!   * [`canonical`] — deterministic JSON serialization for signing.
//!   * [`did`]       — `did:key` <-> Ed25519 public key.
//!   * [`signed_doc`]— detached sign/verify over a signed document.
//!
//! The release-root *private* key is publisher-only and derived in the signing
//! ceremony via [`crate::seed::derive_release_root_ed25519`]; fleet nodes only
//! ever hold the public key.

pub mod anchor;
pub mod canonical;
pub mod did;
pub mod signed_doc;

pub use signed_doc::{verify_detached, SignatureStatus};
