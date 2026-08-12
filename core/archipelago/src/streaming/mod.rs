// WIP streaming/ecash module — suppress dead_code until callers land.
#![allow(dead_code)]
//! Streaming ecash payments for metered data access.
//!
//! Implements a TollGate-inspired protocol for paying for streaming data
//! using Cashu ecash micropayments. Supports three metering models:
//!
//! - **Bytes**: Pay per MB downloaded (content, federation sync)
//! - **Time**: Pay per minute of access (relay, API endpoints)
//! - **Requests**: Pay per API call
//!
//! # Architecture
//!
//! ```text
//! Paying Node                          Selling Node
//! ┌──────────────┐                     ┌──────────────────────┐
//! │ Cashu Wallet  │──cashuA token──────▶│ Gate                 │
//! │ (real BDHKE)  │                     │  verify + receive    │
//! └──────────────┘                     └──────┬───────────────┘
//!                                             │ create/topup session
//!                                      ┌──────▼───────────────┐
//!                                      │ Meter                 │
//!                                      │  track usage          │
//!                                      └──────┬───────────────┘
//!                                             │ enforce allotment
//!                                      ┌──────▼───────────────┐
//!                                      │ Service               │
//!                                      │  content / sync / api │
//!                                      └──────────────────────┘
//! ```
//!
//! # Discovery
//!
//! Services are advertised via Nostr kind 10021 events (TollGate TIP-01
//! compatible) containing pricing tags per TIP-02.

pub mod advertisement;
pub mod gate;
pub mod meter;
pub mod pricing;
pub mod session;
