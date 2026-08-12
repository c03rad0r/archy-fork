//! Cluster module — high-availability multi-node clustering via Raft consensus.
//!
//! When 3+ nodes form a cluster, apps can have replicas across nodes.
//! If one node goes down, apps failover to remaining nodes automatically.
//!
//! Architecture:
//! - Uses Raft consensus for leader election and log replication
//! - Leader node coordinates app placement decisions
//! - Follower nodes replicate state and serve read requests
//! - Federation provides peer discovery; cluster adds consensus layer

use serde::{Deserialize, Serialize};

/// Cluster node role in the Raft consensus group.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ClusterRole {
    Leader,
    Follower,
    Candidate,
    Standalone, // Not part of a cluster
}

impl Default for ClusterRole {
    fn default() -> Self {
        ClusterRole::Standalone
    }
}

/// Cluster membership state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClusterState {
    pub enabled: bool,
    pub role: ClusterRole,
    pub leader_did: Option<String>,
    pub members: Vec<ClusterMember>,
    pub term: u64,
    pub commit_index: u64,
}

/// A member of the cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterMember {
    pub did: String,
    pub onion: String,
    pub role: ClusterRole,
    pub last_heartbeat: Option<String>,
    pub apps: Vec<String>,
}

/// App placement decision — which node should run which app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppPlacement {
    pub app_id: String,
    pub primary_node: String,    // DID of primary node
    pub replica_nodes: Vec<String>, // DIDs of replica nodes
    pub min_replicas: u32,
}

/// Cluster configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    pub min_nodes: u32,          // Minimum nodes for quorum (default: 3)
    pub heartbeat_interval_ms: u64, // Raft heartbeat (default: 150ms)
    pub election_timeout_ms: u64,   // Raft election timeout (default: 300ms)
    pub snapshot_interval: u64,     // Log entries before snapshot
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            min_nodes: 3,
            heartbeat_interval_ms: 150,
            election_timeout_ms: 300,
            snapshot_interval: 1000,
        }
    }
}
