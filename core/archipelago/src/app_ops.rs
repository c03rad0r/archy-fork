//! Cross-layer registry of per-app lifecycle-operation locks and stack
//! membership.
//!
//! The RPC layer's package.start/stop/restart workers serialize through
//! these locks (FIFO, see api::rpc::package::runtime). Background actors
//! (the reconciler; eventually the health monitor) must NOT act on an app
//! while a lifecycle op is mid-sequence: the reconciler once saw a stack
//! member "missing" between a restart worker's stop and start halves and
//! repair-recreated it behind systemd's back, killing the worker's fresh
//! container and leaving the unit down for minutes (.228 mempool frontend,
//! gate 2026-07-09). This module lives outside both layers so each can
//! consult the same state without an api ↔ container dependency cycle.

use std::sync::Arc;

static APP_OP_LOCKS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
> = std::sync::LazyLock::new(Default::default);

/// The per-app lifecycle-operation lock for a (normalized) app key. Workers
/// take this as their first await; tokio's Mutex is fair (FIFO), so queued
/// operations run in RPC arrival order and the final state matches the last
/// request.
pub fn op_lock(app_key: &str) -> Arc<tokio::sync::Mutex<()>> {
    APP_OP_LOCKS
        .lock()
        .expect("APP_OP_LOCKS poisoned")
        .entry(app_key.to_string())
        .or_default()
        .clone()
}

/// Member APP ids (start order) for orchestrator-managed stacks. Every entry
/// is a real manifest app id the orchestrator can `start()`/`stop()` so the
/// quadlet .service is driven instead of raw podman racing systemd's --rm
/// cleanup. Single source of truth — the RPC layer re-exports this.
pub fn stack_member_app_ids(package_id: &str) -> &'static [&'static str] {
    match package_id {
        "immich" => &["immich-postgres", "immich-redis", "immich"],
        "indeedhub" => &[
            "indeedhub-postgres",
            "indeedhub-redis",
            "indeedhub-minio",
            "indeedhub-relay",
            "indeedhub-api",
            "indeedhub-ffmpeg",
            "indeedhub",
        ],
        "btcpay-server" | "btcpayserver" | "btcpay" => {
            &["archy-btcpay-db", "archy-nbxplorer", "btcpay-server"]
        }
        "netbird" => &["netbird-server", "netbird-dashboard", "netbird"],
        "pine" => &["pine-whisper", "pine-piper", "pine-openwakeword", "pine"],
        // The legacy umbrella id maps to the split stack (the orchestrator's
        // umbrella alias handles this too; listing it here keeps the RPC
        // layer's fan-out explicit).
        "mempool" | "mempool-web" => &["archy-mempool-db", "mempool-api", "archy-mempool-web"],
        _ => &[],
    }
}

/// Dependents that resolve a backend's container address once at startup and
/// hold it: moving the backend's IP (restart OR recreate) strands them until
/// they restart too. lnd dials the bitcoin RPC address it resolved at boot
/// and never re-resolves (gate lnd getinfo test, .228 2026-07-09; hardening
/// plan §C). The RPC start/restart workers and the reconciler both consult
/// this — single source of truth, like the stack table above.
pub fn address_caching_dependents(package_id: &str) -> &'static [&'static str] {
    match package_id {
        "bitcoin" | "bitcoin-core" | "bitcoin-knots" => &["lnd"],
        _ => &[],
    }
}

/// The package whose lifecycle lock covers `app_id`: the stack package when
/// `app_id` is a member (RPC ops on "mempool" hold the "mempool" lock while
/// they drive archy-mempool-web), otherwise the app itself.
/// Also consulted by the reconciler's absent-stack-member recovery.
pub fn owning_package(app_id: &str) -> &str {
    const STACKS: &[&str] = &[
        "immich",
        "indeedhub",
        "btcpay-server",
        "netbird",
        "mempool",
        "pine",
    ];
    for stack in STACKS {
        if stack_member_app_ids(stack).contains(&app_id) {
            return stack;
        }
    }
    app_id
}

/// True when a package.start/stop/restart worker currently holds the
/// lifecycle lock covering `app_id` (under its own key or its owning stack
/// package's key). Background actors use this to skip the app for a cycle
/// instead of interleaving with the worker's multi-step sequence. try_lock
/// on a fair tokio Mutex is non-blocking and does not queue.
pub fn lifecycle_op_in_flight(app_id: &str) -> bool {
    let keys = [app_id, owning_package(app_id)];
    for key in keys {
        let lock = op_lock(key);
        let held = lock.try_lock().is_err();
        if held {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owning_package_maps_members_to_stack() {
        assert_eq!(owning_package("archy-mempool-web"), "mempool");
        assert_eq!(owning_package("immich-postgres"), "immich");
        assert_eq!(owning_package("indeedhub-relay"), "indeedhub");
        assert_eq!(owning_package("archy-nbxplorer"), "btcpay-server");
        assert_eq!(owning_package("lnd"), "lnd");
    }

    #[tokio::test]
    async fn in_flight_reflects_held_package_lock() {
        assert!(!lifecycle_op_in_flight("archy-mempool-web"));
        let lock = op_lock("mempool");
        let _guard = lock.lock().await;
        assert!(lifecycle_op_in_flight("archy-mempool-web"));
        assert!(lifecycle_op_in_flight("mempool"));
        assert!(!lifecycle_op_in_flight("jellyfin"));
    }
}
