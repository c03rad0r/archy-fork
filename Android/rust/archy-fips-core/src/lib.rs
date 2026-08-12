//! Embedded FIPS mesh node for the Archipelago companion app.
//!
//! The phone runs a real, leaf-only FIPS node in-process: Android's
//! `VpnService` owns the TUN fd (routing only `fd00::/8`, so normal traffic
//! never touches the tunnel) and hands it to [`fips::Node::start_with_tun_fd`].
//! Peering is outbound-only — the pairing QR carries the node's npub and
//! transport endpoints, and FIPS nodes accept inbound peers without prior
//! registration, so no server-side enrollment step exists.
//!
//! The JNI surface (`jni_glue`, Android-only) is deliberately tiny and
//! JSON-over-strings, mirroring the myco / nostr-vpn embedding pattern:
//! `generateIdentity`, `deriveIdentity`, `start`, `stop`, `isRunning`.

pub mod mesh;

#[cfg(target_os = "android")]
mod jni_glue;
