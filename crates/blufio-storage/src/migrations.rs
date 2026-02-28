// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Embedded database migrations using refinery.
//!
//! SQL migration files are compiled into the binary at build time via
//! `embed_migrations!`. Migrations run automatically on database open.

use blufio_core::BlufioError;

mod embedded {
    use refinery::embed_migrations;
    embed_migrations!("migrations");
}

/// Run all pending migrations against the given connection.
///
/// Refinery tracks applied migrations in its own `refinery_schema_history` table.
pub fn run_migrations(conn: &mut rusqlite::Connection) -> Result<(), BlufioError> {
    embedded::migrations::runner().run(conn).map_err(|e| {
        BlufioError::Storage {
            source: Box::new(e),
        }
    })?;
    Ok(())
}
