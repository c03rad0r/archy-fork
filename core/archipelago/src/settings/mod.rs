//! User-editable settings that are not part of the initial onboarding
//! flow. Each submodule persists a focused slice of preferences to
//! `<data_dir>/settings/*.json` and exposes a process-wide handle so
//! call sites (deep in the transport / RPC / ingest stacks) don't need
//! to thread a data_dir or Arc through the entire call graph.

pub mod ai_permissions;
pub mod session_policy;
pub mod transport;
