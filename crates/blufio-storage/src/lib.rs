#![cfg_attr(not(test), deny(clippy::unwrap_used))]
// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite persistence layer for the Blufio agent framework.
//!
//! Provides WAL-mode SQLite storage with embedded migrations, a single-writer
//! concurrency model via `tokio-rusqlite`, and typed CRUD operations for
//! sessions, messages, and a crash-safe message queue.
//!
//! The primary entry point is [`SqliteStorage`], which implements the
//! [`StorageAdapter`](blufio_core::StorageAdapter) trait from `blufio-core`.

pub mod adapter;
pub mod database;
pub mod migrations;
pub mod models;
pub mod queries;
pub mod writer;

pub use adapter::SqliteStorage;
pub use database::{Database, is_plaintext_sqlite, open_connection, open_connection_sync};
pub use models::*;
pub use queries::classification::BulkClassificationResult;

/// Register the sqlite-vec extension globally via `sqlite3_auto_extension`.
///
/// Idempotent and process-global. Must be called before any database
/// connections are opened so that the `vec0` virtual table module is
/// available when migrations run.
#[allow(clippy::missing_transmute_annotations)]
pub fn register_sqlite_vec() {
    use rusqlite::ffi::sqlite3_auto_extension;
    use sqlite_vec::sqlite3_vec_init;
    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }
}
