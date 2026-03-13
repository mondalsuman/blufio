// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Audit trail CLI handlers for `blufio audit` subcommands.

/// Run `blufio audit verify` -- walk the hash chain and report integrity.
pub(crate) fn run_audit_verify(db_path: &str, json: bool) {
    let path = std::path::Path::new(db_path);
    if !path.exists() {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "ok": true,
                    "verified": 0,
                    "breaks": [],
                    "gaps": [],
                    "erased_count": 0,
                    "message": "audit database not found"
                })
            );
        } else {
            println!("Audit database not found: {db_path}");
            println!("No entries to verify.");
        }
        return;
    }

    let conn = match blufio_storage::open_connection_sync(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to open audit database: {e}");
            std::process::exit(1);
        }
    };

    let report = match blufio_audit::verify_chain(&conn) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: audit verification failed: {e}");
            std::process::exit(1);
        }
    };

    if json {
        let breaks_json: Vec<serde_json::Value> = report
            .breaks
            .iter()
            .map(|b| {
                serde_json::json!({
                    "entry_id": b.entry_id,
                    "expected_hash": b.expected_hash,
                    "actual_hash": b.actual_hash,
                })
            })
            .collect();
        let gaps_json: Vec<serde_json::Value> = report
            .gaps
            .iter()
            .map(|g| {
                serde_json::json!({
                    "after_id": g.after_id,
                    "missing_id": g.missing_id,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "ok": report.ok,
                "verified": report.verified,
                "breaks": breaks_json,
                "gaps": gaps_json,
                "erased_count": report.erased_count,
            })
        );
    } else {
        let status = if report.ok { "OK" } else { "BROKEN" };
        println!("Hash chain: {status}");
        println!("Entries verified: {}", report.verified);
        println!("Erased (GDPR): {}", report.erased_count);
        println!("Gaps: {}", report.gaps.len());

        for b in &report.breaks {
            println!(
                "  BREAK at entry {}: expected {} got {}",
                b.entry_id, b.expected_hash, b.actual_hash
            );
        }
        for g in &report.gaps {
            println!(
                "  GAP: missing entry {} after entry {}",
                g.missing_id, g.after_id
            );
        }
    }

    if !report.ok {
        std::process::exit(1);
    }
}

/// Run `blufio audit tail` -- show recent audit entries with filters.
pub(crate) fn run_audit_tail(
    db_path: &str,
    n: usize,
    event_type: Option<String>,
    since: Option<String>,
    until: Option<String>,
    actor: Option<String>,
    json: bool,
) {
    let path = std::path::Path::new(db_path);
    if !path.exists() {
        if json {
            println!("[]");
        } else {
            println!("Audit database not found: {db_path}");
            println!("No entries to display.");
        }
        return;
    }

    let conn = match blufio_storage::open_connection_sync(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to open audit database: {e}");
            std::process::exit(1);
        }
    };

    // Build dynamic query with filters.
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(ref et) = event_type {
        if et.ends_with(".*") {
            let prefix = et.strip_suffix(".*").unwrap();
            conditions.push(format!("event_type LIKE ?{}", params.len() + 1));
            params.push(Box::new(format!("{prefix}.%")));
        } else {
            conditions.push(format!("event_type = ?{}", params.len() + 1));
            params.push(Box::new(et.clone()));
        }
    }
    if let Some(ref s) = since {
        conditions.push(format!("timestamp >= ?{}", params.len() + 1));
        params.push(Box::new(s.clone()));
    }
    if let Some(ref u) = until {
        conditions.push(format!("timestamp <= ?{}", params.len() + 1));
        params.push(Box::new(u.clone()));
    }
    if let Some(ref a) = actor {
        conditions.push(format!("actor LIKE ?{}", params.len() + 1));
        params.push(Box::new(format!("{a}%")));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!(
        "SELECT id, entry_hash, prev_hash, timestamp, event_type, action, \
         resource_type, resource_id, actor, session_id, details_json, pii_marker \
         FROM audit_entries {where_clause} ORDER BY id DESC LIMIT {n}"
    );

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to query audit entries: {e}");
            std::process::exit(1);
        }
    };

    let entries: Vec<blufio_audit::AuditEntry> = match stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(blufio_audit::AuditEntry {
                id: row.get(0)?,
                entry_hash: row.get(1)?,
                prev_hash: row.get(2)?,
                timestamp: row.get(3)?,
                event_type: row.get(4)?,
                action: row.get(5)?,
                resource_type: row.get(6)?,
                resource_id: row.get(7)?,
                actor: row.get(8)?,
                session_id: row.get(9)?,
                details_json: row.get(10)?,
                pii_marker: row.get(11)?,
            })
        })
        .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
    {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("error: failed to read audit entries: {e}");
            std::process::exit(1);
        }
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_string())
        );
    } else {
        if entries.is_empty() {
            println!("No audit entries found.");
            return;
        }
        // Print in reverse order so newest entries appear at the bottom (natural reading).
        for entry in entries.iter().rev() {
            let marker = if entry.pii_marker == 1 {
                " [ERASED]"
            } else {
                ""
            };
            println!(
                "[{}] {} {} {}/{} {}{}",
                entry.timestamp,
                entry.event_type,
                entry.action,
                entry.resource_type,
                entry.resource_id,
                entry.actor,
                marker,
            );
        }
    }
}

/// Run `blufio audit stats` -- show audit trail statistics.
pub(crate) fn run_audit_stats(db_path: &str, json: bool) {
    let path = std::path::Path::new(db_path);
    if !path.exists() {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "total_entries": 0,
                    "first_entry": null,
                    "last_entry": null,
                    "erased_count": 0,
                    "by_type": {},
                    "message": "audit database not found"
                })
            );
        } else {
            println!("Audit database not found: {db_path}");
            println!("No statistics available.");
        }
        return;
    }

    let conn = match blufio_storage::open_connection_sync(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to open audit database: {e}");
            std::process::exit(1);
        }
    };

    // Summary stats.
    let (total, first_ts, last_ts, erased): (i64, Option<String>, Option<String>, i64) = match conn
        .query_row(
            "SELECT COUNT(*), MIN(timestamp), MAX(timestamp), \
             COALESCE(SUM(pii_marker), 0) FROM audit_entries",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: failed to query audit stats: {e}");
            std::process::exit(1);
        }
    };

    // Per-type breakdown.
    let by_type: Vec<(String, i64)> = {
        let mut stmt = match conn.prepare(
            "SELECT event_type, COUNT(*) as cnt FROM audit_entries \
             GROUP BY event_type ORDER BY cnt DESC",
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: failed to query audit type breakdown: {e}");
                std::process::exit(1);
            }
        };

        match stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .and_then(|r| r.collect::<Result<Vec<(String, i64)>, _>>())
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: failed to read audit type breakdown: {e}");
                std::process::exit(1);
            }
        }
    };

    if json {
        let type_map: serde_json::Map<String, serde_json::Value> = by_type
            .iter()
            .map(|(t, c)| (t.clone(), serde_json::json!(c)))
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "total_entries": total,
                "first_entry": first_ts,
                "last_entry": last_ts,
                "erased_count": erased,
                "by_type": type_map,
            })
        );
    } else {
        println!("Total entries: {total}");
        println!("First entry: {}", first_ts.as_deref().unwrap_or("(none)"));
        println!("Last entry: {}", last_ts.as_deref().unwrap_or("(none)"));
        println!("Erased (GDPR): {erased}");
        if !by_type.is_empty() {
            println!("\nBy event type:");
            for (event_type, count) in &by_type {
                println!("  {event_type}: {count}");
            }
        }
    }
}
