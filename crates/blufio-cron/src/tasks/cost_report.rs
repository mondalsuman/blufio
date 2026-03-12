// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cost report cron task.
//!
//! Generates a cost summary for the most recent 24-hour period, aggregated
//! by provider.

use std::sync::Arc;

use async_trait::async_trait;
use tokio_rusqlite::Connection;

use super::{CronTask, CronTaskError};

/// Cost report task that aggregates spending from the cost_ledger.
pub struct CostReportTask {
    db: Arc<Connection>,
}

impl CostReportTask {
    /// Create a new cost report task.
    pub fn new(db: Arc<Connection>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl CronTask for CostReportTask {
    fn name(&self) -> &str {
        "cost_report"
    }

    fn description(&self) -> &str {
        "Generate cost summary for recent period"
    }

    async fn execute(&self) -> Result<String, CronTaskError> {
        let report = self
            .db
            .call(|conn| -> Result<CostSummary, rusqlite::Error> {
                // Aggregate cost by model (using model field as provider proxy) for last 24h
                let mut stmt = conn.prepare(
                    "SELECT \
                       COALESCE(model, 'unknown') as provider, \
                       COALESCE(SUM(cost_usd), 0.0) as total_cost, \
                       COUNT(*) as call_count \
                     FROM cost_ledger \
                     WHERE created_at >= datetime('now', '-1 day') \
                       AND deleted_at IS NULL \
                     GROUP BY model \
                     ORDER BY total_cost DESC",
                )?;

                let entries: Vec<ProviderCost> = stmt
                    .query_map([], |row| {
                        Ok(ProviderCost {
                            provider: row.get(0)?,
                            total_cost: row.get(1)?,
                            call_count: row.get(2)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?;

                let total: f64 = entries.iter().map(|e| e.total_cost).sum();
                let total_calls: i64 = entries.iter().map(|e| e.call_count).sum();

                Ok(CostSummary {
                    total,
                    total_calls,
                    by_provider: entries,
                })
            })
            .await
            .map_err(|e| CronTaskError::DatabaseError(e.to_string()))?;

        if report.by_provider.is_empty() {
            return Ok("Total: $0.00 across 0 calls (no activity in last 24h)".to_string());
        }

        let provider_breakdown: Vec<String> = report
            .by_provider
            .iter()
            .map(|p| format!("{}: ${:.4}", p.provider, p.total_cost))
            .collect();

        Ok(format!(
            "Total: ${:.4} across {} calls ({})",
            report.total,
            report.total_calls,
            provider_breakdown.join(", ")
        ))
    }
}

struct CostSummary {
    total: f64,
    total_calls: i64,
    by_provider: Vec<ProviderCost>,
}

struct ProviderCost {
    provider: String,
    total_cost: f64,
    call_count: i64,
}
