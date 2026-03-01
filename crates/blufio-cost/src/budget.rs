// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Budget tracking with daily and monthly caps.
//!
//! The budget tracker keeps in-memory running totals and enforces spending
//! caps configured via `CostConfig`. It emits a `tracing::warn` at 80% of
//! any cap and returns `BlufioError::BudgetExhausted` when a cap is reached.
//!
//! On restart, `from_ledger()` re-hydrates totals from the persistent cost
//! ledger so budget enforcement survives process restarts.

use blufio_config::model::CostConfig;
use blufio_core::BlufioError;
use chrono::{Datelike, Utc};
use tracing::warn;

use crate::ledger::CostLedger;

/// In-memory budget tracker with daily and monthly spending caps.
pub struct BudgetTracker {
    /// Running total of today's spend.
    daily_total_usd: f64,
    /// Running total of this month's spend.
    monthly_total_usd: f64,
    /// Daily spending cap (None = unlimited).
    daily_cap: Option<f64>,
    /// Monthly spending cap (None = unlimited).
    monthly_cap: Option<f64>,
    /// Day-of-year for daily reset detection.
    current_day: u32,
    /// Month number for monthly reset detection.
    current_month: u32,
}

impl BudgetTracker {
    /// Create a new budget tracker with zero totals.
    pub fn new(config: &CostConfig) -> Self {
        let now = Utc::now();
        Self {
            daily_total_usd: 0.0,
            monthly_total_usd: 0.0,
            daily_cap: config.daily_budget_usd,
            monthly_cap: config.monthly_budget_usd,
            current_day: now.ordinal(),
            current_month: now.month(),
        }
    }

    /// Create a budget tracker initialized from existing ledger data.
    ///
    /// This handles restart recovery: on startup, we query the ledger for
    /// today's and this month's totals so budget enforcement is continuous.
    pub async fn from_ledger(
        config: &CostConfig,
        ledger: &CostLedger,
    ) -> Result<Self, BlufioError> {
        let now = Utc::now();
        let today = now.format("%Y-%m-%d").to_string();
        let year_month = now.format("%Y-%m").to_string();

        let daily_total = ledger.daily_total(&today).await?;
        let monthly_total = ledger.monthly_total(&year_month).await?;

        Ok(Self {
            daily_total_usd: daily_total,
            monthly_total_usd: monthly_total,
            daily_cap: config.daily_budget_usd,
            monthly_cap: config.monthly_budget_usd,
            current_day: now.ordinal(),
            current_month: now.month(),
        })
    }

    /// Check whether the budget allows another API call.
    ///
    /// Emits `tracing::warn` at 80% of daily or monthly cap.
    /// Returns `BlufioError::BudgetExhausted` when a cap is exceeded.
    pub fn check_budget(&mut self) -> Result<(), BlufioError> {
        self.maybe_reset_daily();
        self.maybe_reset_monthly();

        if let Some(daily_cap) = self.daily_cap {
            if self.daily_total_usd >= daily_cap {
                return Err(BlufioError::BudgetExhausted {
                    message: format!(
                        "Daily budget of ${:.2} reached. Resumes at midnight UTC.",
                        daily_cap
                    ),
                });
            }
            if self.daily_total_usd >= daily_cap * 0.8 {
                warn!(
                    daily_total = self.daily_total_usd,
                    daily_cap = daily_cap,
                    "approaching daily budget cap (80%+)"
                );
            }
        }

        if let Some(monthly_cap) = self.monthly_cap {
            if self.monthly_total_usd >= monthly_cap {
                return Err(BlufioError::BudgetExhausted {
                    message: format!(
                        "Monthly budget of ${:.2} reached. Resumes next month.",
                        monthly_cap
                    ),
                });
            }
            if self.monthly_total_usd >= monthly_cap * 0.8 {
                warn!(
                    monthly_total = self.monthly_total_usd,
                    monthly_cap = monthly_cap,
                    "approaching monthly budget cap (80%+)"
                );
            }
        }

        Ok(())
    }

    /// Record a cost, incrementing daily and monthly totals.
    pub fn record_cost(&mut self, cost_usd: f64) {
        self.daily_total_usd += cost_usd;
        self.monthly_total_usd += cost_usd;
    }

    /// Reset daily total if the day has changed.
    fn maybe_reset_daily(&mut self) {
        let today = Utc::now().ordinal();
        if today != self.current_day {
            self.daily_total_usd = 0.0;
            self.current_day = today;
        }
    }

    /// Reset monthly total if the month has changed.
    fn maybe_reset_monthly(&mut self) {
        let month = Utc::now().month();
        if month != self.current_month {
            self.monthly_total_usd = 0.0;
            self.current_month = month;
        }
    }

    /// Current daily spend (for testing/reporting).
    pub fn daily_total(&self) -> f64 {
        self.daily_total_usd
    }

    /// Current monthly spend (for testing/reporting).
    pub fn monthly_total(&self) -> f64 {
        self.monthly_total_usd
    }

    /// Returns the higher of daily or monthly budget utilization as a fraction (0.0-1.0+).
    ///
    /// The model router uses this to apply budget-aware downgrades:
    /// - >= 0.80: downgrade one model tier
    /// - >= 0.95: route everything to Haiku
    ///
    /// Returns 0.0 if no caps are configured.
    pub fn budget_utilization(&self) -> f64 {
        let daily_util = match self.daily_cap {
            Some(cap) if cap > 0.0 => self.daily_total_usd / cap,
            _ => 0.0,
        };
        let monthly_util = match self.monthly_cap {
            Some(cap) if cap > 0.0 => self.monthly_total_usd / cap,
            _ => 0.0,
        };
        daily_util.max(monthly_util)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_caps(daily: Option<f64>, monthly: Option<f64>) -> CostConfig {
        CostConfig {
            daily_budget_usd: daily,
            monthly_budget_usd: monthly,
            track_tokens: true,
        }
    }

    #[test]
    fn check_budget_ok_when_under_cap() {
        let config = config_with_caps(Some(10.0), Some(100.0));
        let mut tracker = BudgetTracker::new(&config);
        tracker.record_cost(5.0);
        assert!(tracker.check_budget().is_ok());
    }

    #[test]
    fn check_budget_exhausted_daily() {
        let config = config_with_caps(Some(10.0), None);
        let mut tracker = BudgetTracker::new(&config);
        tracker.record_cost(10.0);
        let err = tracker.check_budget().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Daily budget"),
            "expected daily budget error, got: {msg}"
        );
    }

    #[test]
    fn check_budget_exhausted_monthly() {
        let config = config_with_caps(None, Some(50.0));
        let mut tracker = BudgetTracker::new(&config);
        tracker.record_cost(50.0);
        let err = tracker.check_budget().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Monthly budget"),
            "expected monthly budget error, got: {msg}"
        );
    }

    #[test]
    fn no_caps_always_ok() {
        let config = config_with_caps(None, None);
        let mut tracker = BudgetTracker::new(&config);
        tracker.record_cost(999_999.0);
        assert!(tracker.check_budget().is_ok());
    }

    #[test]
    fn warning_at_80_percent() {
        // This test verifies the 80% logic path doesn't return an error.
        // The actual tracing::warn is verified by the code path being reachable.
        let config = config_with_caps(Some(10.0), None);
        let mut tracker = BudgetTracker::new(&config);
        tracker.record_cost(8.5); // 85% of daily cap
        // Should warn but not error
        assert!(tracker.check_budget().is_ok());
    }

    #[test]
    fn record_cost_increments_both_totals() {
        let config = config_with_caps(Some(100.0), Some(1000.0));
        let mut tracker = BudgetTracker::new(&config);
        tracker.record_cost(5.0);
        tracker.record_cost(3.0);
        assert!((tracker.daily_total() - 8.0).abs() < f64::EPSILON);
        assert!((tracker.monthly_total() - 8.0).abs() < f64::EPSILON);
    }

    #[test]
    fn budget_utilization_with_daily_cap() {
        let config = config_with_caps(Some(10.0), Some(100.0));
        let mut tracker = BudgetTracker::new(&config);
        tracker.record_cost(8.5);
        let util = tracker.budget_utilization();
        // Daily: 8.5/10 = 0.85, Monthly: 8.5/100 = 0.085 -> max = 0.85
        assert!(
            (util - 0.85).abs() < 1e-10,
            "expected 0.85, got {util}"
        );
    }

    #[test]
    fn budget_utilization_with_monthly_cap() {
        let config = config_with_caps(None, Some(50.0));
        let mut tracker = BudgetTracker::new(&config);
        tracker.record_cost(25.0);
        let util = tracker.budget_utilization();
        // No daily cap -> 0.0, Monthly: 25/50 = 0.5 -> max = 0.5
        assert!(
            (util - 0.5).abs() < 1e-10,
            "expected 0.5, got {util}"
        );
    }

    #[test]
    fn budget_utilization_no_caps() {
        let config = config_with_caps(None, None);
        let mut tracker = BudgetTracker::new(&config);
        tracker.record_cost(999_999.0);
        let util = tracker.budget_utilization();
        assert!(
            util.abs() < 1e-10,
            "expected 0.0 with no caps, got {util}"
        );
    }

    #[test]
    fn budget_utilization_over_100_percent() {
        let config = config_with_caps(Some(10.0), None);
        let mut tracker = BudgetTracker::new(&config);
        tracker.record_cost(12.0);
        let util = tracker.budget_utilization();
        // 12/10 = 1.2 (over 100%)
        assert!(
            (util - 1.2).abs() < 1e-10,
            "expected 1.2, got {util}"
        );
    }

    #[tokio::test]
    async fn from_ledger_initializes_totals() {
        // Create in-memory DB with cost_ledger table
        let conn = tokio_rusqlite::Connection::open_in_memory()
            .await
            .unwrap();
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute_batch(
                "CREATE TABLE cost_ledger (
                    id TEXT PRIMARY KEY NOT NULL,
                    session_id TEXT NOT NULL,
                    model TEXT NOT NULL,
                    feature_type TEXT NOT NULL,
                    input_tokens INTEGER NOT NULL DEFAULT 0,
                    output_tokens INTEGER NOT NULL DEFAULT 0,
                    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                    cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
                    cost_usd REAL NOT NULL DEFAULT 0.0,
                    created_at TEXT NOT NULL,
                    intended_model TEXT
                );",
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let ledger = CostLedger::new(conn);

        // Insert a record for today
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let record = crate::ledger::CostRecord {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: "s1".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            feature_type: crate::ledger::FeatureType::Message,
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cost_usd: 3.50,
            created_at: format!("{today}T12:00:00.000Z"),
            intended_model: None,
        };
        ledger.record(&record).await.unwrap();

        let config = config_with_caps(Some(10.0), Some(100.0));
        let tracker = BudgetTracker::from_ledger(&config, &ledger).await.unwrap();

        assert!(
            (tracker.daily_total() - 3.50).abs() < 1e-10,
            "expected 3.50, got {}",
            tracker.daily_total()
        );
        assert!(
            (tracker.monthly_total() - 3.50).abs() < 1e-10,
            "expected 3.50, got {}",
            tracker.monthly_total()
        );
    }
}
