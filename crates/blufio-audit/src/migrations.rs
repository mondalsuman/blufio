// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Embedded database migrations for the audit trail database (audit.db).
//!
//! SQL migration files are compiled into the binary at build time via
//! `embed_migrations!`. Migrations run automatically when `AuditWriter` starts.

use crate::models::AuditError;

mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

/// Run all pending audit trail migrations against the given connection.
///
/// Refinery tracks applied migrations in its own `refinery_schema_history` table.
pub fn run(conn: &mut rusqlite::Connection) -> Result<(), AuditError> {
    embedded::migrations::runner()
        .run(conn)
        .map_err(|e| AuditError::DbUnavailable(format!("audit migration failed: {e}")))?;
    Ok(())
}
