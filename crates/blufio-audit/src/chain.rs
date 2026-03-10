// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hash chain computation, verification, and GDPR erasure for the audit trail.
//!
//! # Hash Chain Design
//!
//! Each audit entry's hash is computed over a pipe-delimited canonical string:
//! ```text
//! SHA-256(prev_hash|timestamp|event_type|action|resource_type|resource_id)
//! ```
//!
//! PII fields (`actor`, `session_id`, `details_json`) are deliberately excluded
//! so that GDPR erasure can replace them with `[ERASED]` without breaking the chain.
//!
//! The first entry uses [`GENESIS_HASH`] (64 zero hex chars) as its `prev_hash`.

use sha2::{Digest, Sha256};

use crate::models::{AuditEntry, AuditErasureReport, AuditError};

/// The genesis hash: 64 zero hex characters. Used as `prev_hash` for the first entry.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Compute the SHA-256 hash for an audit entry.
///
/// The canonical format is pipe-delimited:
/// `prev_hash|timestamp|event_type|action|resource_type|resource_id`
///
/// Returns a 64-character lowercase hex string.
pub fn compute_entry_hash(
    prev_hash: &str,
    timestamp: &str,
    event_type: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
) -> String {
    let canonical =
        format!("{prev_hash}|{timestamp}|{event_type}|{action}|{resource_type}|{resource_id}");
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hex::encode(hasher.finalize())
}

/// A break in the hash chain.
#[derive(Debug, Clone)]
pub struct ChainBreak {
    /// The entry ID where the break was detected.
    pub entry_id: i64,
    /// The hash we expected (recomputed from previous entry).
    pub expected_hash: String,
    /// The hash actually stored in the entry.
    pub actual_hash: String,
}

/// A gap in the ID sequence.
#[derive(Debug, Clone)]
pub struct GapInfo {
    /// The entry ID after which the gap exists.
    pub after_id: i64,
    /// The missing entry ID.
    pub missing_id: i64,
}

/// Report from chain verification.
#[derive(Debug, Clone)]
pub struct VerifyReport {
    /// Whether the chain is intact (no breaks and no gaps).
    pub ok: bool,
    /// Number of entries verified.
    pub verified: usize,
    /// Chain breaks found.
    pub breaks: Vec<ChainBreak>,
    /// ID sequence gaps found.
    pub gaps: Vec<GapInfo>,
    /// Number of GDPR-erased entries encountered.
    pub erased_count: usize,
}

/// Verify the entire hash chain in the audit database.
///
/// Walks all entries in ID order, recomputes each hash from immutable fields,
/// checks for ID sequence gaps, and counts GDPR-erased entries.
pub fn verify_chain(conn: &rusqlite::Connection) -> Result<VerifyReport, AuditError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, entry_hash, prev_hash, timestamp, event_type, action, \
             resource_type, resource_id, pii_marker \
             FROM audit_entries ORDER BY id ASC",
        )
        .map_err(|e| AuditError::VerifyFailed(e.to_string()))?;

    let entries: Vec<AuditEntry> = stmt
        .query_map([], |row| {
            Ok(AuditEntry {
                id: row.get(0)?,
                entry_hash: row.get(1)?,
                prev_hash: row.get(2)?,
                timestamp: row.get(3)?,
                event_type: row.get(4)?,
                action: row.get(5)?,
                resource_type: row.get(6)?,
                resource_id: row.get(7)?,
                pii_marker: row.get(8)?,
                // PII fields not needed for verification
                actor: String::new(),
                session_id: String::new(),
                details_json: String::new(),
            })
        })
        .map_err(|e| AuditError::VerifyFailed(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AuditError::VerifyFailed(e.to_string()))?;

    let mut breaks = Vec::new();
    let mut gaps = Vec::new();
    let mut erased_count = 0;
    let mut prev_hash = GENESIS_HASH.to_string();
    let mut prev_id: Option<i64> = None;

    for entry in &entries {
        // Check for ID gaps
        if let Some(pid) = prev_id {
            let expected_id = pid + 1;
            if entry.id != expected_id {
                // There might be multiple missing IDs
                for missing in expected_id..entry.id {
                    gaps.push(GapInfo {
                        after_id: pid,
                        missing_id: missing,
                    });
                }
            }
        }

        // Verify hash
        let expected_hash = compute_entry_hash(
            &prev_hash,
            &entry.timestamp,
            &entry.event_type,
            &entry.action,
            &entry.resource_type,
            &entry.resource_id,
        );

        if entry.entry_hash != expected_hash {
            breaks.push(ChainBreak {
                entry_id: entry.id,
                expected_hash: expected_hash.clone(),
                actual_hash: entry.entry_hash.clone(),
            });
        }

        // Count erased entries
        if entry.pii_marker == 1 {
            erased_count += 1;
        }

        prev_hash = entry.entry_hash.clone();
        prev_id = Some(entry.id);
    }

    Ok(VerifyReport {
        ok: breaks.is_empty() && gaps.is_empty(),
        verified: entries.len(),
        breaks,
        gaps,
        erased_count,
    })
}

/// Erase PII from audit entries for a given user (GDPR right to erasure).
///
/// Matches entries where:
/// - `actor` starts with `"user:{user_id}"`, OR
/// - `details_json` contains the `user_id` string
///
/// Replaces `actor`, `session_id`, and `details_json` with `"[ERASED]"` and
/// sets `pii_marker = 1`. The hash chain remains valid because PII fields are
/// excluded from the hash computation.
///
/// **Important:** The caller should flush pending entries before calling this
/// to ensure complete coverage.
pub async fn erase_audit_entries(
    conn: &tokio_rusqlite::Connection,
    user_id: &str,
) -> Result<AuditErasureReport, AuditError> {
    let user_id = user_id.to_string();
    conn.call(move |conn| -> Result<AuditErasureReport, rusqlite::Error> {
        let actor_pattern = format!("user:{}%", user_id);
        let details_pattern = format!("%{}%", user_id);

        // Find matching entries (scope the statement borrow so we can start a transaction)
        let ids: Vec<i64> = {
            let mut find_stmt = conn.prepare(
                "SELECT id FROM audit_entries \
                 WHERE actor LIKE ?1 OR details_json LIKE ?2",
            )?;
            find_stmt
                .query_map(rusqlite::params![actor_pattern, details_pattern], |row| {
                    row.get(0)
                })?
                .collect::<Result<Vec<_>, _>>()?
        };

        let entries_found = ids.len();

        if ids.is_empty() {
            return Ok(AuditErasureReport {
                entries_found: 0,
                entries_erased: 0,
                erased_ids: vec![],
            });
        }

        // Erase PII fields
        let tx = conn.transaction()?;
        for &id in &ids {
            tx.execute(
                "UPDATE audit_entries SET actor = '[ERASED]', session_id = '[ERASED]', \
                 details_json = '[ERASED]', pii_marker = 1 WHERE id = ?1",
                rusqlite::params![id],
            )?;
        }
        tx.commit()?;

        Ok(AuditErasureReport {
            entries_found,
            entries_erased: ids.len(),
            erased_ids: ids,
        })
    })
    .await
    .map_err(|e| AuditError::DbUnavailable(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_hash_is_64_zero_hex_chars() {
        assert_eq!(GENESIS_HASH.len(), 64);
        assert!(GENESIS_HASH.chars().all(|c| c == '0'));
    }

    #[test]
    fn compute_entry_hash_returns_64_char_hex() {
        let hash = compute_entry_hash(
            GENESIS_HASH,
            "2026-01-01T00:00:00Z",
            "session.created",
            "create",
            "session",
            "s1",
        );
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn compute_entry_hash_is_deterministic() {
        let hash1 = compute_entry_hash(
            GENESIS_HASH,
            "2026-01-01T00:00:00Z",
            "session.created",
            "create",
            "session",
            "s1",
        );
        let hash2 = compute_entry_hash(
            GENESIS_HASH,
            "2026-01-01T00:00:00Z",
            "session.created",
            "create",
            "session",
            "s1",
        );
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn compute_entry_hash_different_inputs_different_outputs() {
        let hash1 = compute_entry_hash(
            GENESIS_HASH,
            "2026-01-01T00:00:00Z",
            "session.created",
            "create",
            "session",
            "s1",
        );
        let hash2 = compute_entry_hash(
            GENESIS_HASH,
            "2026-01-01T00:00:00Z",
            "session.closed",
            "close",
            "session",
            "s1",
        );
        assert_ne!(hash1, hash2);
    }

    /// Helper: build a chain of entries in an in-memory database and return the connection.
    fn build_test_chain(entries: &[(&str, &str, &str, &str, &str)]) -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE audit_entries (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                entry_hash   TEXT NOT NULL,
                prev_hash    TEXT NOT NULL,
                timestamp    TEXT NOT NULL,
                event_type   TEXT NOT NULL,
                action       TEXT NOT NULL,
                resource_type TEXT NOT NULL DEFAULT '',
                resource_id  TEXT NOT NULL DEFAULT '',
                actor        TEXT NOT NULL DEFAULT '',
                session_id   TEXT NOT NULL DEFAULT '',
                details_json TEXT NOT NULL DEFAULT '{}',
                pii_marker   INTEGER NOT NULL DEFAULT 0
            );",
        )
        .unwrap();

        let mut prev_hash = GENESIS_HASH.to_string();
        for (ts, event_type, action, resource_type, resource_id) in entries {
            let entry_hash = compute_entry_hash(
                &prev_hash,
                ts,
                event_type,
                action,
                resource_type,
                resource_id,
            );
            conn.execute(
                "INSERT INTO audit_entries (entry_hash, prev_hash, timestamp, event_type, action, resource_type, resource_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![entry_hash, prev_hash, ts, event_type, action, resource_type, resource_id],
            )
            .unwrap();
            prev_hash = entry_hash;
        }
        conn
    }

    #[test]
    fn verify_chain_valid_3_entries() {
        let conn = build_test_chain(&[
            (
                "2026-01-01T00:00:00Z",
                "session.created",
                "create",
                "session",
                "s1",
            ),
            (
                "2026-01-01T00:00:01Z",
                "memory.created",
                "create",
                "memory",
                "m1",
            ),
            (
                "2026-01-01T00:00:02Z",
                "session.closed",
                "close",
                "session",
                "s1",
            ),
        ]);

        let report = verify_chain(&conn).unwrap();
        assert!(report.ok);
        assert_eq!(report.verified, 3);
        assert_eq!(report.breaks.len(), 0);
        assert_eq!(report.gaps.len(), 0);
    }

    #[test]
    fn verify_chain_detects_tampered_entry() {
        let conn = build_test_chain(&[
            (
                "2026-01-01T00:00:00Z",
                "session.created",
                "create",
                "session",
                "s1",
            ),
            (
                "2026-01-01T00:00:01Z",
                "memory.created",
                "create",
                "memory",
                "m1",
            ),
            (
                "2026-01-01T00:00:02Z",
                "session.closed",
                "close",
                "session",
                "s1",
            ),
        ]);

        // Tamper with the second entry's event_type
        conn.execute(
            "UPDATE audit_entries SET event_type = 'memory.deleted' WHERE id = 2",
            [],
        )
        .unwrap();

        let report = verify_chain(&conn).unwrap();
        assert!(!report.ok);
        // Entry 2 hash mismatch (we changed event_type but hash is still old)
        // Entry 3 also breaks because its prev_hash was based on original entry 2
        assert!(!report.breaks.is_empty());
        assert_eq!(report.breaks[0].entry_id, 2);
    }

    #[test]
    fn verify_chain_detects_id_gaps() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE audit_entries (
                id           INTEGER PRIMARY KEY,
                entry_hash   TEXT NOT NULL,
                prev_hash    TEXT NOT NULL,
                timestamp    TEXT NOT NULL,
                event_type   TEXT NOT NULL,
                action       TEXT NOT NULL,
                resource_type TEXT NOT NULL DEFAULT '',
                resource_id  TEXT NOT NULL DEFAULT '',
                actor        TEXT NOT NULL DEFAULT '',
                session_id   TEXT NOT NULL DEFAULT '',
                details_json TEXT NOT NULL DEFAULT '{}',
                pii_marker   INTEGER NOT NULL DEFAULT 0
            );",
        )
        .unwrap();

        // Insert entries with IDs 1, 2, 4 (missing 3)
        let mut prev_hash = GENESIS_HASH.to_string();
        let entries_data = [
            (
                1,
                "2026-01-01T00:00:00Z",
                "session.created",
                "create",
                "session",
                "s1",
            ),
            (
                2,
                "2026-01-01T00:00:01Z",
                "memory.created",
                "create",
                "memory",
                "m1",
            ),
            (
                4,
                "2026-01-01T00:00:02Z",
                "session.closed",
                "close",
                "session",
                "s1",
            ),
        ];
        for (id, ts, et, action, rt, rid) in &entries_data {
            let entry_hash = compute_entry_hash(&prev_hash, ts, et, action, rt, rid);
            conn.execute(
                "INSERT INTO audit_entries (id, entry_hash, prev_hash, timestamp, event_type, action, resource_type, resource_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![id, entry_hash, prev_hash, ts, et, action, rt, rid],
            )
            .unwrap();
            prev_hash = entry_hash;
        }

        let report = verify_chain(&conn).unwrap();
        assert!(!report.ok);
        assert_eq!(report.gaps.len(), 1);
        assert_eq!(report.gaps[0].after_id, 2);
        assert_eq!(report.gaps[0].missing_id, 3);
    }

    #[test]
    fn verify_chain_handles_erased_entries() {
        let conn = build_test_chain(&[
            (
                "2026-01-01T00:00:00Z",
                "session.created",
                "create",
                "session",
                "s1",
            ),
            (
                "2026-01-01T00:00:01Z",
                "memory.created",
                "create",
                "memory",
                "m1",
            ),
        ]);

        // Simulate GDPR erasure on entry 1
        conn.execute(
            "UPDATE audit_entries SET actor = '[ERASED]', session_id = '[ERASED]', \
             details_json = '[ERASED]', pii_marker = 1 WHERE id = 1",
            [],
        )
        .unwrap();

        let report = verify_chain(&conn).unwrap();
        // Chain should still be valid because PII fields are excluded from hash
        assert!(report.ok);
        assert_eq!(report.verified, 2);
        assert_eq!(report.erased_count, 1);
    }

    #[tokio::test]
    async fn erase_audit_entries_replaces_pii() {
        let conn = tokio_rusqlite::Connection::open_in_memory().await.unwrap();

        // Create table and insert test data
        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute_batch(
                "CREATE TABLE audit_entries (
                    id           INTEGER PRIMARY KEY AUTOINCREMENT,
                    entry_hash   TEXT NOT NULL,
                    prev_hash    TEXT NOT NULL,
                    timestamp    TEXT NOT NULL,
                    event_type   TEXT NOT NULL,
                    action       TEXT NOT NULL,
                    resource_type TEXT NOT NULL DEFAULT '',
                    resource_id  TEXT NOT NULL DEFAULT '',
                    actor        TEXT NOT NULL DEFAULT '',
                    session_id   TEXT NOT NULL DEFAULT '',
                    details_json TEXT NOT NULL DEFAULT '{}',
                    pii_marker   INTEGER NOT NULL DEFAULT 0
                );"
            )?;

            let prev_hash = GENESIS_HASH;
            let hash1 = compute_entry_hash(prev_hash, "2026-01-01T00:00:00Z", "session.created", "create", "session", "s1");
            conn.execute(
                "INSERT INTO audit_entries (entry_hash, prev_hash, timestamp, event_type, action, resource_type, resource_id, actor, session_id, details_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![hash1, prev_hash, "2026-01-01T00:00:00Z", "session.created", "create", "session", "s1", "user:123", "sess-abc", r#"{"user_id":"123"}"#],
            )?;

            let hash2 = compute_entry_hash(&hash1, "2026-01-01T00:00:01Z", "memory.created", "create", "memory", "m1");
            conn.execute(
                "INSERT INTO audit_entries (entry_hash, prev_hash, timestamp, event_type, action, resource_type, resource_id, actor, session_id, details_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![hash2, hash1, "2026-01-01T00:00:01Z", "memory.created", "create", "memory", "m1", "system", "sess-xyz", r#"{"data":"safe"}"#],
            )?;

            Ok(())
        })
        .await
        .unwrap();

        let report = erase_audit_entries(&conn, "123").await.unwrap();
        assert_eq!(report.entries_found, 1);
        assert_eq!(report.entries_erased, 1);
        assert_eq!(report.erased_ids, vec![1]);

        // Verify PII is erased
        let (actor, session_id, details, pii_marker): (String, String, String, i32) = conn
            .call(|conn| {
                conn.query_row(
                    "SELECT actor, session_id, details_json, pii_marker FROM audit_entries WHERE id = 1",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
            })
            .await
            .unwrap();
        assert_eq!(actor, "[ERASED]");
        assert_eq!(session_id, "[ERASED]");
        assert_eq!(details, "[ERASED]");
        assert_eq!(pii_marker, 1);

        // Verify chain is still valid after erasure
        let chain_ok = conn
            .call(|conn| -> Result<bool, rusqlite::Error> { Ok(verify_chain(conn).unwrap().ok) })
            .await
            .unwrap();
        assert!(chain_ok);
    }

    #[tokio::test]
    async fn erase_audit_entries_matches_details_json() {
        let conn = tokio_rusqlite::Connection::open_in_memory().await.unwrap();

        conn.call(|conn| -> Result<(), rusqlite::Error> {
            conn.execute_batch(
                "CREATE TABLE audit_entries (
                    id           INTEGER PRIMARY KEY AUTOINCREMENT,
                    entry_hash   TEXT NOT NULL,
                    prev_hash    TEXT NOT NULL,
                    timestamp    TEXT NOT NULL,
                    event_type   TEXT NOT NULL,
                    action       TEXT NOT NULL,
                    resource_type TEXT NOT NULL DEFAULT '',
                    resource_id  TEXT NOT NULL DEFAULT '',
                    actor        TEXT NOT NULL DEFAULT '',
                    session_id   TEXT NOT NULL DEFAULT '',
                    details_json TEXT NOT NULL DEFAULT '{}',
                    pii_marker   INTEGER NOT NULL DEFAULT 0
                );"
            )?;

            let hash1 = compute_entry_hash(GENESIS_HASH, "2026-01-01T00:00:00Z", "channel.message", "send", "channel", "c1");
            conn.execute(
                "INSERT INTO audit_entries (entry_hash, prev_hash, timestamp, event_type, action, resource_type, resource_id, actor, session_id, details_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![hash1, GENESIS_HASH, "2026-01-01T00:00:00Z", "channel.message", "send", "channel", "c1", "system", "sess-1", r#"{"target_user":"456","msg":"hello"}"#],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        // Erase for user 456 -- should match via details_json
        let report = erase_audit_entries(&conn, "456").await.unwrap();
        assert_eq!(report.entries_found, 1);
        assert_eq!(report.entries_erased, 1);
    }

    // proptest for chain integrity
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn arbitrary_entries_produce_valid_chain(
            entries in prop::collection::vec(
                (
                    "[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z",
                    "[a-z]{3,10}\\.[a-z]{3,10}",
                    "[a-z]{3,8}",
                    "[a-z]{3,8}",
                    "[a-z0-9]{1,10}",
                ),
                1..20,
            )
        ) {
            let conn = rusqlite::Connection::open_in_memory().unwrap();
            conn.execute_batch(
                "CREATE TABLE audit_entries (
                    id           INTEGER PRIMARY KEY AUTOINCREMENT,
                    entry_hash   TEXT NOT NULL,
                    prev_hash    TEXT NOT NULL,
                    timestamp    TEXT NOT NULL,
                    event_type   TEXT NOT NULL,
                    action       TEXT NOT NULL,
                    resource_type TEXT NOT NULL DEFAULT '',
                    resource_id  TEXT NOT NULL DEFAULT '',
                    actor        TEXT NOT NULL DEFAULT '',
                    session_id   TEXT NOT NULL DEFAULT '',
                    details_json TEXT NOT NULL DEFAULT '{}',
                    pii_marker   INTEGER NOT NULL DEFAULT 0
                );",
            )
            .unwrap();

            let mut prev_hash = GENESIS_HASH.to_string();
            for (ts, et, action, rt, rid) in &entries {
                let entry_hash = compute_entry_hash(&prev_hash, ts, et, action, rt, rid);
                conn.execute(
                    "INSERT INTO audit_entries (entry_hash, prev_hash, timestamp, event_type, action, resource_type, resource_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![entry_hash, prev_hash, ts, et, action, rt, rid],
                )
                .unwrap();
                prev_hash = entry_hash;
            }

            let report = verify_chain(&conn).unwrap();
            prop_assert!(report.ok, "Chain should be valid for any arbitrary entries");
            prop_assert_eq!(report.verified, entries.len());
            prop_assert_eq!(report.breaks.len(), 0);
            prop_assert_eq!(report.gaps.len(), 0);
        }
    }
}
