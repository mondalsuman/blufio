// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! GDPR data subject rights tooling for the Blufio agent framework.
//!
//! Implements the right to erasure (Art. 17), data portability (Art. 20),
//! and transparency (Art. 15) through programmatic APIs consumed by the
//! `blufio gdpr` CLI subcommands.
//!
//! # Data Flow
//!
//! ```text
//! find sessions --> collect data --> export (optional)
//!     --> atomic delete --> audit erase --> re-index --> emit events
//! ```
//!
//! # Modules
//!
//! - [`config`] -- GDPR configuration types (`GdprConfig`)
//! - [`models`] -- Domain types: errors, manifests, export envelopes, reports
//! - [`events`] -- Bus event constructors with SHA-256 hashed user IDs
//!
//! Erasure, export, and report logic modules are added in subsequent plans.

pub mod config;
pub mod erasure;
pub mod events;
pub mod manifest;
pub mod models;

// Re-export key types for ergonomic access.
pub use config::GdprConfig;
pub use erasure::{
    check_active_sessions, cleanup_memory_index, erase_audit_trail, execute_erasure,
    find_user_sessions, UserSession,
};
pub use events::{
    erasure_completed, erasure_started, export_completed, hash_user_id, report_generated,
};
pub use manifest::{create_manifest, write_manifest};
pub use models::{
    ErasureManifest, ErasureResult, ExportData, ExportEnvelope, ExportMetadata, FilterCriteria,
    GdprError, ReportData,
};
