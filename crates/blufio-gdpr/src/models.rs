// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! GDPR domain models: error types, erasure manifest, export metadata, and report data.

use blufio_core::error::BlufioError;
use serde::Serialize;

// ---------------------------------------------------------------------------
// GdprError
// ---------------------------------------------------------------------------

/// Errors specific to GDPR operations (erasure, export, reporting).
#[derive(Debug, thiserror::Error)]
pub enum GdprError {
    /// Erasure operation failed.
    #[error("erasure failed: {0}")]
    ErasureFailed(String),

    /// Data export operation failed.
    #[error("export failed: {0}")]
    ExportFailed(String),

    /// Transparency report generation failed.
    #[error("report failed: {0}")]
    ReportFailed(String),

    /// No data found for the specified user.
    #[error("no data found for user: {0}")]
    UserNotFound(String),

    /// User has active (open) sessions that must be closed first.
    #[error("user has {0} active sessions -- close them first or pass --force")]
    ActiveSessionsExist(usize),

    /// Export directory is not writable.
    #[error("export directory not writable: {0}")]
    ExportDirNotWritable(String),
}

impl From<GdprError> for BlufioError {
    fn from(e: GdprError) -> Self {
        BlufioError::Internal(format!("gdpr: {e}"))
    }
}

// ---------------------------------------------------------------------------
// Erasure types
// ---------------------------------------------------------------------------

/// Manifest recording what was erased during a GDPR erasure operation.
///
/// Contains counts and identifiers (no content) for audit purposes. Always
/// written to the export directory, even when `--skip-export` is passed.
#[derive(Debug, Clone, Serialize)]
pub struct ErasureManifest {
    /// Unique manifest identifier.
    pub manifest_id: String,
    /// ISO 8601 timestamp of the erasure operation.
    pub timestamp: String,
    /// User ID that was erased.
    pub user_id: String,
    /// Number of messages deleted.
    pub messages_deleted: u64,
    /// Number of sessions deleted.
    pub sessions_deleted: u64,
    /// Number of memories deleted.
    pub memories_deleted: u64,
    /// Number of compaction archives deleted.
    pub archives_deleted: u64,
    /// Number of cost records anonymized (session_id set to NULL).
    pub cost_records_anonymized: u64,
    /// Number of audit entries redacted.
    pub audit_entries_redacted: u64,
    /// Session IDs that were affected.
    pub session_ids: Vec<String>,
}

/// Result of a GDPR erasure operation.
#[derive(Debug)]
pub struct ErasureResult {
    /// The erasure manifest with counts and IDs.
    pub manifest: ErasureManifest,
    /// Duration of the erasure operation in milliseconds.
    pub duration_ms: u64,
    /// Path to the pre-erasure export file, if one was created.
    pub export_path: Option<String>,
    /// Warning about audit erasure (e.g., if audit DB was unavailable).
    pub audit_warning: Option<String>,
}

// ---------------------------------------------------------------------------
// Export types
// ---------------------------------------------------------------------------

/// Metadata header for a GDPR data export file.
#[derive(Debug, Clone, Serialize)]
pub struct ExportMetadata {
    /// ISO 8601 timestamp of the export.
    pub timestamp: String,
    /// User ID whose data was exported.
    pub user_id: String,
    /// Blufio version that generated the export.
    pub blufio_version: String,
    /// Criteria used to filter the exported data.
    pub filter_criteria: FilterCriteria,
}

/// Filtering criteria applied to a GDPR data export.
#[derive(Debug, Clone, Serialize)]
pub struct FilterCriteria {
    /// Filter to a specific session.
    pub session_id: Option<String>,
    /// Include only data created after this ISO 8601 timestamp.
    pub since: Option<String>,
    /// Include only data created before this ISO 8601 timestamp.
    pub until: Option<String>,
    /// Include only specific data types (e.g., `["messages", "memories"]`).
    pub data_types: Option<Vec<String>>,
    /// Whether PII redaction was applied.
    pub redacted: bool,
}

/// Top-level JSON envelope for a GDPR data export.
#[derive(Debug, Clone, Serialize)]
pub struct ExportEnvelope {
    /// Export metadata (timestamp, user, version, filter criteria).
    pub export_metadata: ExportMetadata,
    /// Exported data sections.
    pub data: ExportData,
}

/// Data sections within a GDPR export envelope.
#[derive(Debug, Clone, Serialize)]
pub struct ExportData {
    /// Exported messages as JSON values.
    pub messages: Vec<serde_json::Value>,
    /// Exported sessions as JSON values.
    pub sessions: Vec<serde_json::Value>,
    /// Exported memories as JSON values.
    pub memories: Vec<serde_json::Value>,
    /// Exported cost records as JSON values.
    pub cost_records: Vec<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Report types
// ---------------------------------------------------------------------------

/// Transparency report data showing counts of held user data.
#[derive(Debug, Clone, Serialize)]
pub struct ReportData {
    /// User ID the report is for.
    pub user_id: String,
    /// Number of messages held.
    pub messages: u64,
    /// Number of sessions held.
    pub sessions: u64,
    /// Number of memories held.
    pub memories: u64,
    /// Number of compaction archives held.
    pub archives: u64,
    /// Number of cost records held.
    pub cost_records: u64,
    /// Number of audit entries referencing this user.
    pub audit_entries: u64,
    /// Note about audit entries (e.g., retention policy).
    pub audit_note: String,
}
