// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Domain model types for storage entities.
//!
//! The canonical types are defined in `blufio-core::types` for use across
//! adapter trait boundaries. This module re-exports them for convenience
//! within the storage crate.

pub use blufio_core::types::{Message, QueueEntry, Session};
