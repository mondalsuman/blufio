// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Single-writer documentation and enforcement.
//!
//! All writes in blufio-storage are serialized through `tokio-rusqlite`'s
//! single background thread. The `Database` struct IS the single writer.
//! Query modules accept `&Database` and call through `conn.call()`.
//!
//! **Do NOT create additional Connection instances for writes.**

// The single-writer pattern is enforced by design:
// - `Database` wraps a single `tokio_rusqlite::Connection`
// - All query functions accept `&Database` and use `database.conn().call()`
// - tokio-rusqlite serializes all closure calls on one background thread
// - This eliminates SQLITE_BUSY errors under concurrent access
