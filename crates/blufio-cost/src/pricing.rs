// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Model pricing tables and cost calculation.
//!
//! Pricing verified from <https://docs.anthropic.com/en/docs/about-claude/pricing>
//! on 2026-03-01.
//!
//! Claude Haiku 3.5:  input=$0.80/MTok, output=$4.00/MTok
//! Claude Sonnet 4:   input=$3.00/MTok, output=$15.00/MTok
//! Claude Opus 4:     input=$15.00/MTok, output=$75.00/MTok
//! Cache read = 10% of input price, cache write = 25% premium over input price.

use blufio_core::TokenUsage;

/// Per-model pricing in USD per million tokens.
#[derive(Debug, Clone)]
pub struct ModelPricing {
    /// Cost per million input tokens.
    pub input_per_mtok: f64,
    /// Cost per million output tokens.
    pub output_per_mtok: f64,
    /// Cost per million cache-read tokens.
    pub cache_read_per_mtok: f64,
    /// Cost per million cache-write (creation) tokens.
    pub cache_write_per_mtok: f64,
}

/// Look up pricing for a given model identifier.
///
/// Matches on substrings: "opus", "haiku", "sonnet". Falls back to Sonnet
/// pricing for unknown models so cost tracking never silently drops records.
pub fn get_pricing(model: &str) -> ModelPricing {
    let lower = model.to_lowercase();

    if lower.contains("opus") {
        ModelPricing {
            input_per_mtok: 15.0,
            output_per_mtok: 75.0,
            cache_read_per_mtok: 1.50,
            cache_write_per_mtok: 18.75,
        }
    } else if lower.contains("haiku") {
        ModelPricing {
            input_per_mtok: 0.80,
            output_per_mtok: 4.0,
            cache_read_per_mtok: 0.08,
            cache_write_per_mtok: 1.0,
        }
    } else {
        // Default to Sonnet pricing (including unknown models).
        ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: 0.30,
            cache_write_per_mtok: 3.75,
        }
    }
}

/// Calculate cost in USD for a given token usage and pricing.
///
/// Formula: sum of (tokens / 1_000_000) * price_per_million for each token type.
pub fn calculate_cost(usage: &TokenUsage, pricing: &ModelPricing) -> f64 {
    let input = (usage.input_tokens as f64 / 1_000_000.0) * pricing.input_per_mtok;
    let output = (usage.output_tokens as f64 / 1_000_000.0) * pricing.output_per_mtok;
    let cache_read = (usage.cache_read_tokens as f64 / 1_000_000.0) * pricing.cache_read_per_mtok;
    let cache_write =
        (usage.cache_creation_tokens as f64 / 1_000_000.0) * pricing.cache_write_per_mtok;
    input + output + cache_read + cache_write
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sonnet_pricing() {
        let p = get_pricing("claude-sonnet-4-20250514");
        assert!((p.input_per_mtok - 3.0).abs() < f64::EPSILON);
        assert!((p.output_per_mtok - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn haiku_pricing() {
        let p = get_pricing("claude-haiku-4-5-20250901");
        assert!((p.input_per_mtok - 0.80).abs() < f64::EPSILON);
        assert!((p.output_per_mtok - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn opus_pricing() {
        let p = get_pricing("claude-opus-4-20250514");
        assert!((p.input_per_mtok - 15.0).abs() < f64::EPSILON);
        assert!((p.output_per_mtok - 75.0).abs() < f64::EPSILON);
    }

    #[test]
    fn unknown_model_falls_back_to_sonnet() {
        let p = get_pricing("unknown-model-xyz");
        assert!((p.input_per_mtok - 3.0).abs() < f64::EPSILON);
        assert!((p.output_per_mtok - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn calculate_cost_with_all_token_types() {
        let pricing = get_pricing("claude-sonnet-4-20250514");
        let usage = TokenUsage {
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 200,
            cache_creation_tokens: 0,
        };
        let cost = calculate_cost(&usage, &pricing);
        // input: 1000/1M * 3.0 = 0.003
        // output: 500/1M * 15.0 = 0.0075
        // cache_read: 200/1M * 0.30 = 0.00006
        // cache_write: 0
        let expected = 0.003 + 0.0075 + 0.00006;
        assert!(
            (cost - expected).abs() < 1e-10,
            "expected {expected}, got {cost}"
        );
    }

    #[test]
    fn zero_tokens_zero_cost() {
        let pricing = get_pricing("claude-sonnet-4-20250514");
        let usage = TokenUsage::default();
        let cost = calculate_cost(&usage, &pricing);
        assert!((cost - 0.0).abs() < f64::EPSILON);
    }
}
