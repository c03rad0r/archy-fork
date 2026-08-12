//! Backup and restore for Archipelago.
//!
//! - `identity`: Encrypted DID identity key backup (existing).
//! - `full`: Full system backup — identity + app data + configs + settings.

pub mod full;
mod identity;

pub use identity::{create_encrypted_backup, restore_encrypted_backup};
