// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Cost tracking, budget enforcement, and pricing for the Blufio agent framework.
//!
//! This crate provides:
//! - **Cost ledger**: Persistent recording of every LLM API call with full token breakdown
//! - **Budget tracker**: In-memory daily/monthly cap enforcement with 80% warnings
//! - **Pricing**: Model-specific cost calculation using official Anthropic pricing

pub mod budget;
pub mod ledger;
pub mod pricing;

pub use budget::BudgetTracker;
pub use ledger::{CostLedger, CostRecord, FeatureType};
