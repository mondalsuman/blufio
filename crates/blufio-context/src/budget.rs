// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Per-zone token budget enforcement for the three-zone context engine.
//!
//! Provides:
//! - [`ZoneBudget`]: Adaptive budget computation across static, conditional, and dynamic zones
//! - [`enforce_conditional_budget`]: Hard enforcement with provider-priority truncation
//!
//! The static zone is advisory-only (warns but never truncates). The conditional zone
//! enforces a hard budget with a 10% safety margin, dropping lowest-priority providers
//! first. The dynamic zone receives an adaptive budget: `total - actual_static - actual_conditional`.

use blufio_config::model::ContextConfig;
use blufio_core::token_counter::{TokenCounter, TokenizerCache, count_with_fallback};
use blufio_core::types::{ContentBlock, ProviderMessage};
use tracing::{debug, info};

/// Hardcoded safety margin for conditional zone budget (10%).
///
/// The effective conditional budget is `conditional_budget * (1 - SAFETY_MARGIN)`,
/// ensuring a buffer against tokenizer estimation variance.
const SAFETY_MARGIN: f64 = 0.10;

/// Per-zone token budget configuration with adaptive dynamic budget computation.
///
/// Created from [`ContextConfig`] at engine startup. The static and conditional
/// budgets are fixed; the dynamic budget is computed at assembly time based on
/// actual token counts from the other two zones.
#[derive(Debug, Clone)]
pub struct ZoneBudget {
    /// Token budget for the static zone (system prompt). Default: 3000.
    pub static_budget: u32,
    /// Token budget for the conditional zone (memories, skills). Default: 8000.
    pub conditional_budget: u32,
    /// Total context window budget in tokens. Default: 180_000.
    pub total_budget: u32,
}

impl ZoneBudget {
    /// Creates a `ZoneBudget` from the given context configuration.
    pub fn from_config(config: &ContextConfig) -> Self {
        Self {
            static_budget: config.static_zone_budget,
            conditional_budget: config.conditional_zone_budget,
            total_budget: config.context_budget,
        }
    }

    /// Returns the effective conditional zone budget after applying the safety margin.
    ///
    /// The safety margin (10%) provides a buffer against tokenizer estimation variance.
    /// For a default budget of 8000 tokens, this returns 7200.
    pub fn conditional_effective(&self) -> u32 {
        (self.conditional_budget as f64 * (1.0 - SAFETY_MARGIN)) as u32
    }

    /// Computes the adaptive dynamic zone budget.
    ///
    /// `dynamic_budget = total_budget - actual_static_tokens - actual_conditional_tokens`
    ///
    /// Uses saturating subtraction to prevent underflow when other zones exceed their budgets.
    /// The soft/hard compaction thresholds apply to this adaptive budget, not the total.
    pub fn dynamic_budget(&self, actual_static_tokens: u32, actual_conditional_tokens: u32) -> u32 {
        self.total_budget
            .saturating_sub(actual_static_tokens)
            .saturating_sub(actual_conditional_tokens)
    }
}

/// Enforces the conditional zone token budget by dropping lowest-priority providers.
///
/// Providers are ordered by registration priority (first registered = highest priority).
/// When the total conditional token count exceeds `budget`, providers are dropped from
/// the END (lowest priority = last registered) first, working backward until the
/// budget is satisfied.
///
/// # Arguments
///
/// - `provider_results`: Ordered list of `(provider_name, messages)` pairs
/// - `budget`: Effective conditional zone budget (after safety margin)
/// - `token_cache`: Tokenizer cache for accurate counting
/// - `model`: Model ID for provider-specific tokenizer selection
///
/// # Returns
///
/// `(kept_messages, dropped_provider_names)` -- kept messages are flattened in
/// registration order; dropped provider names are returned for debugging.
pub async fn enforce_conditional_budget(
    provider_results: Vec<(String, Vec<ProviderMessage>)>,
    budget: u32,
    token_cache: &TokenizerCache,
    model: &str,
) -> (Vec<ProviderMessage>, Vec<String>) {
    if provider_results.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let counter = token_cache.get_counter(model);

    // Count tokens per provider.
    let mut provider_tokens: Vec<(String, Vec<ProviderMessage>, usize)> = Vec::new();
    let mut total_tokens: usize = 0;

    for (name, messages) in provider_results {
        let mut provider_count: usize = 0;
        for msg in &messages {
            for block in &msg.content {
                if let ContentBlock::Text { text } = block {
                    provider_count += count_with_fallback(counter.as_ref(), text).await;
                }
            }
        }
        total_tokens += provider_count;
        provider_tokens.push((name, messages, provider_count));
    }

    // If within budget, return all messages.
    if total_tokens <= budget as usize {
        debug!(
            total_tokens = total_tokens,
            budget = budget,
            "conditional zone within budget"
        );
        let all_messages: Vec<ProviderMessage> = provider_tokens
            .into_iter()
            .flat_map(|(_, msgs, _)| msgs)
            .collect();
        return (all_messages, Vec::new());
    }

    // Over budget: drop providers from the end (lowest priority) first.
    let mut dropped: Vec<String> = Vec::new();
    let mut current_tokens = total_tokens;

    // Work backward through providers (last = lowest priority).
    let mut keep_count = provider_tokens.len();
    for i in (0..provider_tokens.len()).rev() {
        if current_tokens <= budget as usize {
            break;
        }
        current_tokens -= provider_tokens[i].2;
        dropped.push(provider_tokens[i].0.clone());
        keep_count = i;
    }

    info!(
        dropped_providers = ?dropped,
        original_tokens = total_tokens,
        remaining_tokens = current_tokens,
        budget = budget,
        "conditional zone budget enforcement: dropped providers"
    );

    // Collect kept messages in registration order.
    let kept_messages: Vec<ProviderMessage> = provider_tokens
        .into_iter()
        .take(keep_count)
        .flat_map(|(_, msgs, _)| msgs)
        .collect();

    (kept_messages, dropped)
}

/// Counts total tokens across a slice of [`ProviderMessage`]s.
pub async fn count_messages_tokens(
    messages: &[ProviderMessage],
    counter: &dyn TokenCounter,
) -> usize {
    let mut total: usize = 0;
    for msg in messages {
        for block in &msg.content {
            if let ContentBlock::Text { text } = block {
                total += count_with_fallback(counter, text).await;
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use blufio_core::token_counter::{TokenizerCache, TokenizerMode};

    fn make_message(role: &str, text: &str) -> ProviderMessage {
        ProviderMessage {
            role: role.to_string(),
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
        }
    }

    #[test]
    fn zone_budget_from_config_default() {
        let config = ContextConfig::default();
        let budget = ZoneBudget::from_config(&config);
        assert_eq!(budget.static_budget, 3000);
        assert_eq!(budget.conditional_budget, 8000);
        assert_eq!(budget.total_budget, 180_000);
    }

    #[test]
    fn conditional_effective_applies_safety_margin() {
        let budget = ZoneBudget {
            static_budget: 3000,
            conditional_budget: 8000,
            total_budget: 180_000,
        };
        // 8000 * 0.90 = 7200
        assert_eq!(budget.conditional_effective(), 7200);
    }

    #[test]
    fn conditional_effective_with_custom_budget() {
        let budget = ZoneBudget {
            static_budget: 3000,
            conditional_budget: 10_000,
            total_budget: 180_000,
        };
        // 10000 * 0.90 = 9000
        assert_eq!(budget.conditional_effective(), 9000);
    }

    #[test]
    fn dynamic_budget_computation() {
        let budget = ZoneBudget {
            static_budget: 3000,
            conditional_budget: 8000,
            total_budget: 180_000,
        };
        // 180_000 - 2500 - 6000 = 171_500
        assert_eq!(budget.dynamic_budget(2500, 6000), 171_500);
    }

    #[test]
    fn dynamic_budget_saturating_subtraction() {
        let budget = ZoneBudget {
            static_budget: 3000,
            conditional_budget: 8000,
            total_budget: 10_000,
        };
        // 10_000 - 8000 - 5000 would be negative => saturates to 0
        assert_eq!(budget.dynamic_budget(8000, 5000), 0);
    }

    #[test]
    fn dynamic_budget_with_zero_usage() {
        let budget = ZoneBudget {
            static_budget: 3000,
            conditional_budget: 8000,
            total_budget: 180_000,
        };
        assert_eq!(budget.dynamic_budget(0, 0), 180_000);
    }

    #[tokio::test]
    async fn enforce_budget_within_budget_keeps_all() {
        let cache = Arc::new(TokenizerCache::new(TokenizerMode::Fast));
        let providers = vec![
            (
                "memory".to_string(),
                vec![make_message("system", "short")],
            ),
            (
                "skills".to_string(),
                vec![make_message("system", "brief")],
            ),
        ];

        let (kept, dropped) =
            enforce_conditional_budget(providers, 10_000, &cache, "test-model").await;
        assert_eq!(dropped.len(), 0);
        assert_eq!(kept.len(), 2);
    }

    #[tokio::test]
    async fn enforce_budget_drops_lowest_priority_first() {
        let cache = Arc::new(TokenizerCache::new(TokenizerMode::Fast));

        // Create providers with enough text to exceed a tight budget.
        // With heuristic counter (chars/3.5), "x".repeat(100) ~ 29 tokens.
        let providers = vec![
            (
                "memory".to_string(),
                vec![make_message("system", &"x".repeat(100))],
            ),
            (
                "skills".to_string(),
                vec![make_message("system", &"y".repeat(100))],
            ),
            (
                "archive".to_string(),
                vec![make_message("system", &"z".repeat(100))],
            ),
        ];

        // Budget of 60 tokens: ~29 * 3 = ~87 tokens total, need to drop at least one.
        let (kept, dropped) =
            enforce_conditional_budget(providers, 60, &cache, "test-model").await;

        // Archive (last registered = lowest priority) should be dropped first.
        assert!(dropped.contains(&"archive".to_string()));
        // Memory (first registered = highest priority) should be kept.
        assert!(!dropped.contains(&"memory".to_string()));
        assert!(kept.len() <= 2);
    }

    #[tokio::test]
    async fn enforce_budget_empty_providers() {
        let cache = Arc::new(TokenizerCache::new(TokenizerMode::Fast));
        let providers: Vec<(String, Vec<ProviderMessage>)> = vec![];

        let (kept, dropped) =
            enforce_conditional_budget(providers, 1000, &cache, "test-model").await;
        assert!(kept.is_empty());
        assert!(dropped.is_empty());
    }

    #[tokio::test]
    async fn enforce_budget_drops_multiple_providers() {
        let cache = Arc::new(TokenizerCache::new(TokenizerMode::Fast));

        let providers = vec![
            (
                "memory".to_string(),
                vec![make_message("system", &"a".repeat(100))],
            ),
            (
                "skills".to_string(),
                vec![make_message("system", &"b".repeat(100))],
            ),
            (
                "archive".to_string(),
                vec![make_message("system", &"c".repeat(100))],
            ),
        ];

        // Budget of 30 tokens: only room for ~1 provider.
        let (kept, dropped) =
            enforce_conditional_budget(providers, 30, &cache, "test-model").await;

        // Both archive and skills should be dropped.
        assert_eq!(dropped.len(), 2);
        assert!(dropped.contains(&"archive".to_string()));
        assert!(dropped.contains(&"skills".to_string()));
        assert_eq!(kept.len(), 1); // Only memory survives.
    }
}
