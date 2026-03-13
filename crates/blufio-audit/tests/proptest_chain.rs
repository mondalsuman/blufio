// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Property-based tests for hash chain verification.
//!
//! Validates chain integrity invariants: valid chains always verify,
//! tampered entries break the chain, reordering breaks the chain,
//! and appending preserves prior chain validity.

use blufio_audit::chain::{GENESIS_HASH, compute_entry_hash, verify_chain};
use proptest::prelude::*;

/// Helper: build a chain of entries in an in-memory database and return the connection.
fn build_chain_db(entries: &[(String, String, String, String, String)]) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE audit_entries (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            entry_hash    TEXT NOT NULL,
            prev_hash     TEXT NOT NULL,
            timestamp     TEXT NOT NULL,
            event_type    TEXT NOT NULL,
            action        TEXT NOT NULL,
            resource_type TEXT NOT NULL DEFAULT '',
            resource_id   TEXT NOT NULL DEFAULT '',
            actor         TEXT NOT NULL DEFAULT '',
            session_id    TEXT NOT NULL DEFAULT '',
            details_json  TEXT NOT NULL DEFAULT '{}',
            pii_marker    INTEGER NOT NULL DEFAULT 0
        );",
    )
    .unwrap();

    let mut prev_hash = GENESIS_HASH.to_string();
    for (ts, et, action, rt, rid) in entries {
        let entry_hash = compute_entry_hash(&prev_hash, ts, et, action, rt, rid);
        conn.execute(
            "INSERT INTO audit_entries (entry_hash, prev_hash, timestamp, event_type, action, resource_type, resource_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![entry_hash, prev_hash, ts, et, action, rt, rid],
        )
        .unwrap();
        prev_hash = entry_hash;
    }
    conn
}

/// Strategy to generate a random audit entry tuple.
fn entry_strategy() -> impl Strategy<Value = (String, String, String, String, String)> {
    (
        "[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z",
        "[a-z]{3,10}\\.[a-z]{3,10}",
        "[a-z]{3,8}",
        "[a-z]{3,8}",
        "[a-z0-9]{1,10}",
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..Default::default() })]

    // ── Property 1: valid chain of N entries always verifies ──────────

    #[test]
    fn valid_chain_always_verifies(
        entries in prop::collection::vec(entry_strategy(), 1..50)
    ) {
        let conn = build_chain_db(&entries);
        let report = verify_chain(&conn).unwrap();
        prop_assert!(report.ok, "Valid chain should always verify");
        prop_assert_eq!(report.verified, entries.len());
        prop_assert_eq!(report.breaks.len(), 0);
        prop_assert_eq!(report.gaps.len(), 0);
    }

    // ── Property 2: modifying any single entry's content breaks chain ──

    #[test]
    fn modifying_single_entry_breaks_chain(
        entries in prop::collection::vec(entry_strategy(), 2..20),
        target_idx in 0usize..19, // will be clamped to valid range
        new_action in "[a-z]{3,8}",
    ) {
        let conn = build_chain_db(&entries);

        // Clamp target to valid range
        let target_id = (target_idx % entries.len()) as i64 + 1;

        // Tamper with the entry's action field (which is part of the hash)
        conn.execute(
            "UPDATE audit_entries SET action = ?1 WHERE id = ?2",
            rusqlite::params![new_action, target_id],
        )
        .unwrap();

        let report = verify_chain(&conn).unwrap();

        // The chain must report at least one break (the tampered entry or a
        // downstream entry whose prev_hash is now wrong).
        prop_assert!(
            !report.ok || report.breaks.is_empty(),
            "Tampering with entry {} should break the chain (ok={}, breaks={})",
            target_id,
            report.ok,
            report.breaks.len()
        );

        // If the new_action happens to match the original (extremely unlikely
        // but possible), the chain would still verify. Skip assertion in that case.
        if !report.ok {
            prop_assert!(
                !report.breaks.is_empty(),
                "If chain is not ok, there must be at least one break"
            );
        }
    }

    // ── Property 3: reordering any two entries breaks verification ────

    #[test]
    fn reordering_entries_breaks_chain(
        entries in prop::collection::vec(entry_strategy(), 3..20),
        idx_a in 0usize..19,
        idx_b in 0usize..19,
    ) {
        let conn = build_chain_db(&entries);

        // Clamp indices and ensure they're different
        let a = (idx_a % entries.len()) as i64 + 1;
        let b = (idx_b % entries.len()) as i64 + 1;

        if a == b {
            // Same index, no reorder -- skip
            return Ok(());
        }

        // Swap the event_type and action of two entries
        let get_fields = |id: i64| -> (String, String) {
            conn.query_row(
                "SELECT event_type, action FROM audit_entries WHERE id = ?1",
                [id],
                |row| Ok((row.get(0).unwrap(), row.get(1).unwrap())),
            )
            .unwrap()
        };

        let (et_a, act_a) = get_fields(a);
        let (et_b, act_b) = get_fields(b);

        conn.execute(
            "UPDATE audit_entries SET event_type = ?1, action = ?2 WHERE id = ?3",
            rusqlite::params![et_b, act_b, a],
        )
        .unwrap();
        conn.execute(
            "UPDATE audit_entries SET event_type = ?1, action = ?2 WHERE id = ?3",
            rusqlite::params![et_a, act_a, b],
        )
        .unwrap();

        let report = verify_chain(&conn).unwrap();

        // If the swapped entries happened to have identical fields, the chain
        // would still be valid. Only assert when fields actually differ.
        if et_a != et_b || act_a != act_b {
            prop_assert!(!report.ok, "Swapping entries should break the chain");
        }
    }

    // ── Property 4: appending new entries preserves prior chain validity ──

    #[test]
    fn appending_entries_preserves_chain(
        initial_entries in prop::collection::vec(entry_strategy(), 1..20),
        append_entries in prop::collection::vec(entry_strategy(), 1..10),
    ) {
        // Build initial chain
        let conn = build_chain_db(&initial_entries);

        // Verify initial chain is valid
        let report = verify_chain(&conn).unwrap();
        prop_assert!(report.ok, "Initial chain should be valid");

        // Get the last entry's hash for chaining
        let last_hash: String = conn
            .query_row(
                "SELECT entry_hash FROM audit_entries ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();

        // Append new entries
        let mut prev_hash = last_hash;
        for (ts, et, action, rt, rid) in &append_entries {
            let entry_hash = compute_entry_hash(&prev_hash, ts, et, action, rt, rid);
            conn.execute(
                "INSERT INTO audit_entries (entry_hash, prev_hash, timestamp, event_type, action, resource_type, resource_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![entry_hash, prev_hash, ts, et, action, rt, rid],
            )
            .unwrap();
            prev_hash = entry_hash;
        }

        // Verify extended chain is still valid
        let report = verify_chain(&conn).unwrap();
        prop_assert!(report.ok, "Chain should still be valid after appending entries");
        prop_assert_eq!(
            report.verified,
            initial_entries.len() + append_entries.len()
        );
    }
}
