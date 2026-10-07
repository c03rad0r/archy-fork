//! Node federation: trusted multi-node clusters with state sync.
//!
//! Nodes federate by exchanging invite codes containing DID + onion address.
//! Trust is bilateral — both sides must agree. Federated nodes periodically
//! sync container status, health metrics, and availability.

mod invites;
pub mod pending;
pub(crate) mod handshake_delivery;
mod storage;
mod sync;
mod types;

// Re-export all public items so `crate::federation::*` continues to work.
pub use invites::{accept_invite, create_invite, parse_invite};
pub(crate) use invites::restrict_discovery_invite;
// Crate-internal: used by the periodic federation auto-sync to re-assert
// membership to peers that don't list us back (asymmetry self-heal).
pub(crate) use invites::notify_join;
// Crate-internal: peer-joined resolves the granted trust level by matching
// the acceptor's invite_token against our stored outgoing invites.
pub(crate) use storage::load_invites;
pub(crate) use storage::load_unique_payment_peer;
#[allow(unused_imports)]
pub use storage::{
    add_node, fips_npub_for_onion, load_nodes, load_removed_dids, record_peer_transport,
    record_sync_result, remove_node, save_nodes, set_trust_level, update_node,
};
pub use sync::{build_local_state, deploy_to_peer, sync_with_peer, sync_with_peer_by_did};
pub use types::{AppStatus, FederatedNode, NodeStateSnapshot, TrustLevel, TrustSource};
