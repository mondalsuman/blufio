// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Database connection management with PRAGMA setup, WAL mode, and lifecycle.
//!
//! All writes are serialized through tokio-rusqlite's single background thread.
//! Do NOT create additional Connection instances for writes.

// Implementation in Task 2.
