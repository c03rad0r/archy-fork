//! Centralized constants for the Archipelago backend.
//! Avoids hardcoded values scattered across the codebase.

/// Bitcoin Core RPC endpoint (localhost only).
pub const BITCOIN_RPC_URL: &str = "http://127.0.0.1:8332/";

/// DWN (Decentralized Web Node) health check endpoint.
pub const DWN_HEALTH_URL: &str = "http://127.0.0.1:3100/health";

/// Tor SOCKS5 proxy for outbound onion connections.
pub const TOR_SOCKS_PROXY: &str = "socks5h://127.0.0.1:9050";
