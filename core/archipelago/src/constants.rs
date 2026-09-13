//! Centralized constants for the Archipelago backend.
//! Avoids hardcoded values scattered across the codebase.

/// Bitcoin Core RPC endpoint (localhost only).
pub const BITCOIN_RPC_URL: &str = "http://127.0.0.1:8332/";

/// DWN (Decentralized Web Node) health check endpoint.
pub const DWN_HEALTH_URL: &str = "http://127.0.0.1:3100/health";

/// Tor SOCKS5 proxy for outbound onion connections.
pub const TOR_SOCKS_PROXY: &str = "socks5h://127.0.0.1:9050";

/// Smallest disk (GB, total) a cuprate node may be installed, started,
/// restarted, updated, or boot-reconciled onto. Cuprate has no on-disk
/// pruning (verified against upstream `cuprated/src/config.rs` — the
/// `pruning` crate is Monero's p2p protocol pruning), so unlike the bitcoin
/// apps it cannot self-shrink on a scarce disk; below this line the ~250 GiB
/// Monero chain simply does not fit and running it would fill the filesystem
/// and take Archipelago down. 450 = chain + growth/headroom: allows
/// 500 GB-class disks, refuses the 250 GB VPS class.
///
/// SINGLE SOURCE OF TRUTH — the RPC gates
/// (`api::rpc::package::dependencies`) and the boot reconciler
/// (`container::prod_orchestrator`) both read this; a drift between them
/// would silently reopen the disk-fill failure the gate exists to close.
/// Keep `apps/cuprate/manifest.yml` (storage dependency + comments) aligned.
pub const CUPRATE_MIN_DISK_GB: u64 = 450;
