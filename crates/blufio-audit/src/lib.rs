// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tamper-evident hash-chain audit trail for the Blufio agent framework.
//!
//! # Architecture
//!
//! Every security-relevant action is recorded as an [`AuditEntry`] in a
//! dedicated `audit.db` SQLite database. Entries form a SHA-256 hash chain:
//!
//! ```text
//! entry_hash = SHA-256(prev_hash | timestamp | event_type | action | resource_type | resource_id)
//! ```
//!
//! PII fields (`actor`, `session_id`, `details_json`) are **excluded** from the
//! hash, enabling GDPR right-to-erasure without breaking the chain.
//!
//! # Modules
//!
//! - [`models`] -- Core data types: `AuditEntry`, `PendingEntry`, `AuditErasureReport`, `AuditError`
//! - [`chain`] -- Hash computation, chain verification, GDPR erasure
//! - [`writer`] -- Async background writer with mpsc channel and batch flush
//! - [`filter`] -- TOML event allowlist with dot-prefix matching
//! - [`migrations`] -- Refinery embedded migrations for `audit.db`

pub mod chain;
pub mod filter;
pub mod migrations;
pub mod models;
pub mod subscriber;
pub mod writer;

// Re-exports for convenience
pub use chain::{
    ChainBreak, GENESIS_HASH, GapInfo, VerifyReport, compute_entry_hash, erase_audit_entries,
    verify_chain,
};
pub use filter::EventFilter;
pub use models::{AuditEntry, AuditErasureReport, AuditError, PendingEntry};
pub use subscriber::AuditSubscriber;
pub use writer::{AuditCommand, AuditWriter};
