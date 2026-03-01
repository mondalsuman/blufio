// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cost ledger for persisting LLM API call records to SQLite.
//!
//! Each provider request is recorded with a full token breakdown and calculated
//! cost in USD. The ledger supports daily, monthly, and per-session totals for
//! budget enforcement and reporting.

use blufio_core::{BlufioError, TokenUsage};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tracing::info;

/// The type of feature that triggered an LLM call.
#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize)]
pub enum FeatureType {
    /// A regular user/assistant message exchange.
    Message,
    /// Context compaction (summarization of older messages).
    Compaction,
    /// Tool/function call.
    Tool,
    /// Periodic heartbeat or keep-alive prompt.
    Heartbeat,
    /// Memory extraction via Haiku (background fact extraction).
    Extraction,
}

/// A single cost record representing one LLM API call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostRecord {
    /// Unique record identifier (UUID v4).
    pub id: String,
    /// Session that triggered this call.
    pub session_id: String,
    /// Model identifier used (e.g., "claude-sonnet-4-20250514").
    pub model: String,
    /// What feature triggered this call.
    pub feature_type: FeatureType,
    /// Number of input tokens.
    pub input_tokens: u32,
    /// Number of output tokens.
    pub output_tokens: u32,
    /// Number of cache-read tokens.
    pub cache_read_tokens: u32,
    /// Number of cache-creation tokens.
    pub cache_creation_tokens: u32,
    /// Calculated cost in USD.
    pub cost_usd: f64,
    /// ISO 8601 timestamp.
    pub created_at: String,
}

impl CostRecord {
    /// Create a new cost record from a token usage and calculated cost.
    pub fn new(
        session_id: String,
        model: String,
        feature_type: FeatureType,
        usage: &TokenUsage,
        cost_usd: f64,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            session_id,
            model,
            feature_type,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cache_read_tokens: usage.cache_read_tokens,
            cache_creation_tokens: usage.cache_creation_tokens,
            cost_usd,
            created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        }
    }
}

/// Convert a tokio-rusqlite error into BlufioError::Storage.
fn map_tr_err(e: tokio_rusqlite::Error<rusqlite::Error>) -> BlufioError {
    BlufioError::Storage {
        source: Box::new(e),
    }
}

/// Persistent cost ledger backed by SQLite.
///
/// Records are written to the `cost_ledger` table (created by V2 migration).
/// All operations go through the single tokio-rusqlite background thread.
pub struct CostLedger {
    conn: tokio_rusqlite::Connection,
}

impl CostLedger {
    /// Create a new cost ledger using the given tokio-rusqlite connection.
    pub fn new(conn: tokio_rusqlite::Connection) -> Self {
        Self { conn }
    }

    /// Open a cost ledger from a database file path.
    ///
    /// Creates its own tokio-rusqlite connection to the given path.
    /// The cost_ledger table must already exist (created by storage migrations).
    pub async fn open(path: &str) -> Result<Self, BlufioError> {
        let conn = tokio_rusqlite::Connection::open(path)
            .await
            .map_err(|e| BlufioError::Storage {
                source: Box::new(e),
            })?;
        Ok(Self::new(conn))
    }

    /// Record a cost entry in the ledger.
    pub async fn record(&self, record: &CostRecord) -> Result<(), BlufioError> {
        let id = record.id.clone();
        let session_id = record.session_id.clone();
        let model = record.model.clone();
        let feature_type = record.feature_type.to_string();
        let input_tokens = record.input_tokens;
        let output_tokens = record.output_tokens;
        let cache_read_tokens = record.cache_read_tokens;
        let cache_creation_tokens = record.cache_creation_tokens;
        let cost_usd = record.cost_usd;
        let created_at = record.created_at.clone();

        self.conn
            .call(move |conn| {
                conn.execute(
                    "INSERT INTO cost_ledger (id, session_id, model, feature_type, \
                     input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens, \
                     cost_usd, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        id,
                        session_id,
                        model,
                        feature_type,
                        input_tokens,
                        output_tokens,
                        cache_read_tokens,
                        cache_creation_tokens,
                        cost_usd,
                        created_at,
                    ],
                )?;
                Ok(())
            })
            .await
            .map_err(map_tr_err)?;

        info!(
            session_id = %record.session_id,
            model = %record.model,
            input_tokens = record.input_tokens,
            output_tokens = record.output_tokens,
            cache_read_tokens = record.cache_read_tokens,
            cost_usd = record.cost_usd,
            "cost recorded"
        );

        Ok(())
    }

    /// Sum of costs for a given date (ISO 8601 date prefix, e.g. "2026-03-01").
    pub async fn daily_total(&self, date: &str) -> Result<f64, BlufioError> {
        let date = date.to_string();
        self.conn
            .call(move |conn| {
                let total: f64 = conn
                    .query_row(
                        "SELECT COALESCE(SUM(cost_usd), 0.0) FROM cost_ledger \
                         WHERE created_at >= ?1 AND created_at < date(?1, '+1 day')",
                        rusqlite::params![date],
                        |row| row.get(0),
                    )?;
                Ok(total)
            })
            .await
            .map_err(map_tr_err)
    }

    /// Sum of costs for a given year-month prefix (e.g. "2026-03").
    pub async fn monthly_total(&self, year_month: &str) -> Result<f64, BlufioError> {
        let prefix = format!("{year_month}%");
        self.conn
            .call(move |conn| {
                let total: f64 = conn
                    .query_row(
                        "SELECT COALESCE(SUM(cost_usd), 0.0) FROM cost_ledger \
                         WHERE created_at LIKE ?1",
                        rusqlite::params![prefix],
                        |row| row.get(0),
                    )?;
                Ok(total)
            })
            .await
            .map_err(map_tr_err)
    }

    /// Sum of costs for a given session.
    pub async fn session_total(&self, session_id: &str) -> Result<f64, BlufioError> {
        let session_id = session_id.to_string();
        self.conn
            .call(move |conn| {
                let total: f64 = conn
                    .query_row(
                        "SELECT COALESCE(SUM(cost_usd), 0.0) FROM cost_ledger \
                         WHERE session_id = ?1",
                        rusqlite::params![session_id],
                        |row| row.get(0),
                    )?;
                Ok(total)
            })
            .await
            .map_err(map_tr_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create an in-memory database with the cost_ledger schema applied.
    async fn test_db() -> tokio_rusqlite::Connection {
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
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE INDEX idx_cost_ledger_session ON cost_ledger(session_id);
                CREATE INDEX idx_cost_ledger_created ON cost_ledger(created_at);
                CREATE INDEX idx_cost_ledger_model ON cost_ledger(model);",
            )?;
            Ok(())
        })
        .await
        .unwrap();
        conn
    }

    fn sample_record(session_id: &str, cost_usd: f64, created_at: &str) -> CostRecord {
        CostRecord {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            feature_type: FeatureType::Message,
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            cost_usd,
            created_at: created_at.to_string(),
        }
    }

    #[tokio::test]
    async fn record_inserts_row() {
        let conn = test_db().await;
        let ledger = CostLedger::new(conn);
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        };
        let record = CostRecord::new(
            "sess-1".to_string(),
            "claude-sonnet-4-20250514".to_string(),
            FeatureType::Message,
            &usage,
            0.001,
        );
        ledger.record(&record).await.unwrap();

        // Verify the row exists
        let total = ledger.session_total("sess-1").await.unwrap();
        assert!(total > 0.0);
    }

    #[tokio::test]
    async fn daily_total_sums_today() {
        let conn = test_db().await;
        let ledger = CostLedger::new(conn);

        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let ts = format!("{today}T10:00:00.000Z");

        ledger
            .record(&sample_record("s1", 1.50, &ts))
            .await
            .unwrap();
        ledger
            .record(&sample_record("s1", 0.75, &ts))
            .await
            .unwrap();

        let total = ledger.daily_total(&today).await.unwrap();
        assert!(
            (total - 2.25).abs() < 1e-10,
            "expected 2.25, got {total}"
        );
    }

    #[tokio::test]
    async fn monthly_total_sums_month() {
        let conn = test_db().await;
        let ledger = CostLedger::new(conn);

        let now = chrono::Utc::now();
        let year_month = now.format("%Y-%m").to_string();
        let ts1 = format!("{year_month}-01T10:00:00.000Z");
        let ts2 = format!("{year_month}-15T10:00:00.000Z");

        ledger
            .record(&sample_record("s1", 2.0, &ts1))
            .await
            .unwrap();
        ledger
            .record(&sample_record("s1", 3.0, &ts2))
            .await
            .unwrap();

        let total = ledger.monthly_total(&year_month).await.unwrap();
        assert!(
            (total - 5.0).abs() < 1e-10,
            "expected 5.0, got {total}"
        );
    }

    #[tokio::test]
    async fn session_total_filters_by_session() {
        let conn = test_db().await;
        let ledger = CostLedger::new(conn);

        let ts = "2026-03-01T10:00:00.000Z";
        ledger
            .record(&sample_record("sess-a", 1.0, ts))
            .await
            .unwrap();
        ledger
            .record(&sample_record("sess-b", 2.0, ts))
            .await
            .unwrap();

        let total_a = ledger.session_total("sess-a").await.unwrap();
        let total_b = ledger.session_total("sess-b").await.unwrap();

        assert!((total_a - 1.0).abs() < 1e-10);
        assert!((total_b - 2.0).abs() < 1e-10);
    }

    #[test]
    fn feature_type_display_and_parse() {
        use std::str::FromStr;
        let ft = FeatureType::Message;
        assert_eq!(ft.to_string(), "Message");
        let parsed = FeatureType::from_str("Compaction").unwrap();
        assert_eq!(parsed, FeatureType::Compaction);
        let extraction = FeatureType::from_str("Extraction").unwrap();
        assert_eq!(extraction, FeatureType::Extraction);
        assert_eq!(FeatureType::Extraction.to_string(), "Extraction");
    }

    #[tokio::test]
    async fn record_extraction_cost() {
        let conn = test_db().await;
        let ledger = CostLedger::new(conn);
        let usage = TokenUsage {
            input_tokens: 200,
            output_tokens: 100,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
        };
        let record = CostRecord::new(
            "sess-extract".to_string(),
            "claude-haiku-4-5-20250901".to_string(),
            FeatureType::Extraction,
            &usage,
            0.0005,
        );
        ledger.record(&record).await.unwrap();

        let total = ledger.session_total("sess-extract").await.unwrap();
        assert!(
            (total - 0.0005).abs() < 1e-10,
            "extraction cost should be recorded, got {total}"
        );
    }

    #[test]
    fn cost_record_new_sets_fields() {
        let usage = TokenUsage {
            input_tokens: 500,
            output_tokens: 200,
            cache_read_tokens: 100,
            cache_creation_tokens: 50,
        };
        let rec = CostRecord::new(
            "s1".to_string(),
            "claude-sonnet-4-20250514".to_string(),
            FeatureType::Tool,
            &usage,
            0.05,
        );
        assert_eq!(rec.input_tokens, 500);
        assert_eq!(rec.cache_read_tokens, 100);
        assert!(!rec.id.is_empty());
        assert!(!rec.created_at.is_empty());
    }
}
