// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Retention enforcement engine with two-phase deletion.
//!
//! Phase 1 (soft-delete): marks expired records with `deleted_at` timestamp.
//! Phase 2 (permanent delete): removes records past the grace period.
//!
//! CRITICAL: Retention operates ONLY on the main database connection.
//! Audit records (audit.db) are architecturally exempt per RETN-04.

pub mod permanent;
pub mod soft_delete;

use std::sync::Arc;

use blufio_config::model::RetentionConfig;
use tokio_rusqlite::Connection;

/// Orchestrates two-phase retention enforcement.
pub struct RetentionEnforcer {
    db: Arc<Connection>,
    config: RetentionConfig,
}

/// Report from a retention enforcement run.
#[derive(Debug, Clone, Default)]
pub struct RetentionReport {
    /// Number of records soft-deleted in phase 1.
    pub soft_deleted_count: u64,
    /// Number of records permanently deleted in phase 2.
    pub permanently_deleted_count: u64,
    /// Per-table breakdown of soft-deleted records.
    pub soft_delete_breakdown: TableBreakdown,
    /// Per-table breakdown of permanently deleted records.
    pub permanent_delete_breakdown: TableBreakdown,
}

/// Per-table record counts for retention operations.
#[derive(Debug, Clone, Default)]
pub struct TableBreakdown {
    pub messages: u64,
    pub sessions: u64,
    pub cost_records: u64,
    pub memories: u64,
}

impl RetentionReport {
    /// Generate a human-readable summary of the retention run.
    pub fn summary(&self) -> String {
        format!(
            "Retention: soft-deleted {} (messages={}, sessions={}, cost={}, memories={}), \
             permanently deleted {} (messages={}, sessions={}, cost={}, memories={})",
            self.soft_deleted_count,
            self.soft_delete_breakdown.messages,
            self.soft_delete_breakdown.sessions,
            self.soft_delete_breakdown.cost_records,
            self.soft_delete_breakdown.memories,
            self.permanently_deleted_count,
            self.permanent_delete_breakdown.messages,
            self.permanent_delete_breakdown.sessions,
            self.permanent_delete_breakdown.cost_records,
            self.permanent_delete_breakdown.memories,
        )
    }
}

impl RetentionEnforcer {
    /// Create a new retention enforcer.
    pub fn new(db: Arc<Connection>, config: RetentionConfig) -> Self {
        Self { db, config }
    }

    /// Run both phases of retention enforcement.
    ///
    /// Phase 1: Soft-delete expired records (set `deleted_at`).
    /// Phase 2: Permanently delete records past the grace period.
    pub async fn enforce(&self) -> Result<RetentionReport, String> {
        let mut report = RetentionReport::default();

        // Phase 1: Soft-delete expired records
        let soft_breakdown = soft_delete::run_soft_delete(
            &self.db,
            &self.config.periods,
            &self.config.restricted,
        )
        .await?;

        report.soft_delete_breakdown = soft_breakdown;
        report.soft_deleted_count = report.soft_delete_breakdown.messages
            + report.soft_delete_breakdown.sessions
            + report.soft_delete_breakdown.cost_records
            + report.soft_delete_breakdown.memories;

        // Phase 2: Permanently delete past-grace records
        let perm_breakdown = permanent::run_permanent_delete(
            &self.db,
            self.config.grace_period_days,
        )
        .await?;

        report.permanent_delete_breakdown = perm_breakdown;
        report.permanently_deleted_count = report.permanent_delete_breakdown.messages
            + report.permanent_delete_breakdown.sessions
            + report.permanent_delete_breakdown.cost_records
            + report.permanent_delete_breakdown.memories;

        Ok(report)
    }
}
