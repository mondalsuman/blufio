// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite persistence layer for the Blufio agent framework.
//!
//! Provides WAL-mode SQLite storage with embedded migrations, a single-writer
//! concurrency model via `tokio-rusqlite`, and typed CRUD operations for
//! sessions, messages, and a crash-safe message queue.

pub mod database;
pub mod migrations;
pub mod models;
pub mod queries;
pub mod writer;

pub use database::Database;
pub use models::*;
