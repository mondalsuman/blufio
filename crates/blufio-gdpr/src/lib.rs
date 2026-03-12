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
//! - [`erasure`] -- Erasure orchestrator (atomic multi-table cascade)
//! - [`manifest`] -- Erasure manifest generation and persistence
//! - [`export`] -- Data export (JSON/CSV) with filtering and PII redaction
//! - [`report`] -- Transparency report (count queries)

pub mod config;
pub mod erasure;
pub mod events;
pub mod export;
pub mod manifest;
pub mod models;
pub mod report;

// Re-export key types for ergonomic access.
pub use config::GdprConfig;
pub use erasure::{
    UserSession, check_active_sessions, cleanup_memory_index, erase_audit_trail, execute_erasure,
    find_user_sessions,
};
pub use events::{
    erasure_completed, erasure_started, export_completed, hash_user_id, report_generated,
};
pub use export::{
    CollectedData, apply_redaction, collect_user_data, resolve_export_path, write_csv_export,
    write_json_export,
};
pub use manifest::{create_manifest, write_manifest};
pub use models::{
    ErasureManifest, ErasureResult, ExportData, ExportEnvelope, ExportMetadata, FilterCriteria,
    GdprError, ReportData,
};
pub use report::count_user_data;
