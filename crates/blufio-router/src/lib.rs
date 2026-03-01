// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Query complexity classification and model routing for the Blufio agent.
//!
//! This crate provides:
//! - [`QueryClassifier`]: Heuristic complexity classification (zero-cost, zero-latency)
//! - [`ModelRouter`]: Budget-aware model selection with per-message overrides
//!
//! The router intercepts user messages before LLM calls, selecting the
//! appropriate Claude model tier (Haiku/Sonnet/Opus) based on query
//! complexity, budget utilization, and optional per-message overrides.

pub mod classifier;
pub mod router;

pub use classifier::{ClassificationResult, ComplexityTier, QueryClassifier};
pub use router::{parse_model_override, ModelRouter, RoutingDecision};
