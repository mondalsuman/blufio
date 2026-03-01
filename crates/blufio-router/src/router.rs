// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Model routing with budget-aware downgrades and per-message overrides.
//!
//! Orchestrates model selection: per-message override > global force > classify > budget downgrade.

use blufio_config::model::RoutingConfig;
use tracing::info;

use crate::classifier::{ComplexityTier, QueryClassifier};

/// Routing decision with both intended and actual model for cost tracking.
#[derive(Debug, Clone)]
pub struct RoutingDecision {
    /// Model the classifier intended (before budget downgrades).
    pub intended_model: String,
    /// Model actually used (after budget downgrades).
    pub actual_model: String,
    /// Max tokens for the selected model tier.
    pub max_tokens: u32,
    /// Whether budget forced a downgrade from intended model.
    pub downgraded: bool,
    /// Classified complexity tier.
    pub tier: ComplexityTier,
    /// Human-readable reason for routing decision.
    pub reason: String,
}

/// Orchestrates model selection with classification, budget awareness, and overrides.
pub struct ModelRouter {
    classifier: QueryClassifier,
    config: RoutingConfig,
}

impl ModelRouter {
    /// Create a new model router with the given configuration.
    pub fn new(config: RoutingConfig) -> Self {
        Self {
            classifier: QueryClassifier::new(),
            config,
        }
    }

    /// Route a message to the appropriate model.
    ///
    /// Priority order:
    /// 1. Per-message override (/opus, /haiku, /sonnet)
    /// 2. Global force_model config
    /// 3. Heuristic classification + budget-aware downgrade
    ///
    /// `budget_utilization` is a fraction (0.0-1.0+) of the higher of daily/monthly budget.
    pub fn route(
        &self,
        message: &str,
        recent_context: &[&str],
        budget_utilization: f64,
    ) -> RoutingDecision {
        // 1. Check per-message override
        let (override_model, _clean_text) = parse_model_override(message);
        if let Some(model) = override_model {
            let tier = self.tier_for_model(&model);
            let max_tokens = self.max_tokens_for_tier(tier);
            return RoutingDecision {
                intended_model: model.clone(),
                actual_model: model,
                max_tokens,
                downgraded: false,
                tier,
                reason: "per-message override".to_string(),
            };
        }

        // 2. Check global force_model config
        if let Some(ref forced) = self.config.force_model {
            let tier = self.tier_for_model(forced);
            let max_tokens = self.max_tokens_for_tier(tier);
            return RoutingDecision {
                intended_model: forced.clone(),
                actual_model: forced.clone(),
                max_tokens,
                downgraded: false,
                tier,
                reason: "global force_model config".to_string(),
            };
        }

        // 3. Classify complexity
        let classification = self.classifier.classify(message, recent_context);

        // Map tier to model
        let intended = self.model_for_tier(classification.tier);

        // 4. Apply budget downgrade
        let (actual, downgraded) = self.apply_budget_downgrade(
            classification.tier,
            &intended,
            budget_utilization,
        );

        let max_tokens = self.max_tokens_for_model(&actual);

        let reason = if downgraded {
            format!(
                "{} (downgraded from {} due to budget at {:.0}%)",
                classification.reason,
                Self::short_model_name(&intended),
                budget_utilization * 100.0
            )
        } else {
            classification.reason.to_string()
        };

        if downgraded {
            info!(
                intended = intended.as_str(),
                actual = actual.as_str(),
                budget_pct = budget_utilization * 100.0,
                "budget-aware model downgrade"
            );
        }

        RoutingDecision {
            intended_model: intended,
            actual_model: actual,
            max_tokens,
            downgraded,
            tier: classification.tier,
            reason,
        }
    }

    fn model_for_tier(&self, tier: ComplexityTier) -> String {
        match tier {
            ComplexityTier::Simple => self.config.simple_model.clone(),
            ComplexityTier::Standard => self.config.standard_model.clone(),
            ComplexityTier::Complex => self.config.complex_model.clone(),
        }
    }

    fn tier_for_model(&self, model: &str) -> ComplexityTier {
        let lower = model.to_lowercase();
        if lower.contains("haiku") {
            ComplexityTier::Simple
        } else if lower.contains("opus") {
            ComplexityTier::Complex
        } else {
            ComplexityTier::Standard
        }
    }

    fn max_tokens_for_tier(&self, tier: ComplexityTier) -> u32 {
        match tier {
            ComplexityTier::Simple => self.config.simple_max_tokens,
            ComplexityTier::Standard => self.config.standard_max_tokens,
            ComplexityTier::Complex => self.config.complex_max_tokens,
        }
    }

    fn max_tokens_for_model(&self, model: &str) -> u32 {
        self.max_tokens_for_tier(self.tier_for_model(model))
    }

    fn apply_budget_downgrade(
        &self,
        tier: ComplexityTier,
        intended: &str,
        budget_utilization: f64,
    ) -> (String, bool) {
        if budget_utilization >= 0.95 {
            // Everything routes to Haiku at 95%+
            let actual = self.config.simple_model.clone();
            let downgraded = actual != intended;
            (actual, downgraded)
        } else if budget_utilization >= 0.80 {
            // Downgrade one tier
            let actual = match tier {
                ComplexityTier::Complex => self.config.standard_model.clone(),
                ComplexityTier::Standard => self.config.simple_model.clone(),
                ComplexityTier::Simple => self.config.simple_model.clone(),
            };
            let downgraded = actual != intended;
            (actual, downgraded)
        } else {
            (intended.to_string(), false)
        }
    }

    /// Get a short display name for a model string.
    pub fn short_model_name(model: &str) -> &str {
        let lower = model.to_lowercase();
        if lower.contains("opus") {
            "Opus"
        } else if lower.contains("haiku") {
            "Haiku"
        } else {
            "Sonnet"
        }
    }
}

/// Parse a per-message model override prefix from user input.
///
/// Supports `/opus `, `/haiku `, `/sonnet ` prefixes (with trailing space).
/// Returns `(Some(model_string), rest_of_message)` if an override is found,
/// or `(None, original_message)` if no override.
///
/// The override prefix is stripped from the returned message text.
pub fn parse_model_override(text: &str) -> (Option<String>, &str) {
    let trimmed = text.trim_start();
    if let Some(rest) = trimmed.strip_prefix("/opus ") {
        (Some("claude-opus-4-20250514".to_string()), rest)
    } else if let Some(rest) = trimmed.strip_prefix("/haiku ") {
        (Some("claude-haiku-4-5-20250901".to_string()), rest)
    } else if let Some(rest) = trimmed.strip_prefix("/sonnet ") {
        (Some("claude-sonnet-4-20250514".to_string()), rest)
    } else {
        (None, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> RoutingConfig {
        RoutingConfig::default()
    }

    #[test]
    fn parse_override_opus() {
        let (model, rest) = parse_model_override("/opus analyze this code");
        assert_eq!(model.unwrap(), "claude-opus-4-20250514");
        assert_eq!(rest, "analyze this code");
    }

    #[test]
    fn parse_override_haiku() {
        let (model, rest) = parse_model_override("/haiku what time");
        assert_eq!(model.unwrap(), "claude-haiku-4-5-20250901");
        assert_eq!(rest, "what time");
    }

    #[test]
    fn parse_override_sonnet() {
        let (model, rest) = parse_model_override("/sonnet help me");
        assert_eq!(model.unwrap(), "claude-sonnet-4-20250514");
        assert_eq!(rest, "help me");
    }

    #[test]
    fn parse_override_none() {
        let (model, rest) = parse_model_override("normal message");
        assert!(model.is_none());
        assert_eq!(rest, "normal message");
    }

    #[test]
    fn route_with_force_model() {
        let mut config = test_config();
        config.force_model = Some("claude-sonnet-4-20250514".to_string());
        let router = ModelRouter::new(config);

        let decision = router.route("analyze this complex problem step by step", &[], 0.0);
        assert_eq!(decision.actual_model, "claude-sonnet-4-20250514");
        assert!(!decision.downgraded);
        assert_eq!(decision.reason, "global force_model config");
    }

    #[test]
    fn route_budget_downgrade_80_percent() {
        let router = ModelRouter::new(test_config());

        // Complex query at 85% budget
        let decision = router.route(
            "analyze this code and refactor it for better performance",
            &[],
            0.85,
        );
        // Opus intended -> Sonnet actual (one tier down)
        assert!(decision.intended_model.contains("opus"));
        assert!(decision.actual_model.contains("sonnet"));
        assert!(decision.downgraded);
    }

    #[test]
    fn route_budget_downgrade_95_percent() {
        let router = ModelRouter::new(test_config());

        // Standard query at 96% budget
        let decision = router.route("what's the weather like?", &[], 0.96);
        // Everything routes to Haiku at 95%+
        assert!(decision.actual_model.contains("haiku"));
    }

    #[test]
    fn route_per_message_override_bypasses_budget() {
        let router = ModelRouter::new(test_config());

        // Per-message override should bypass budget downgrade
        let decision = router.route("/opus analyze this", &[], 0.96);
        assert!(decision.actual_model.contains("opus"));
        assert!(!decision.downgraded);
    }

    #[test]
    fn routing_decision_intended_vs_actual() {
        let router = ModelRouter::new(test_config());

        // No downgrade: intended == actual
        let decision = router.route("hi", &[], 0.0);
        assert_eq!(decision.intended_model, decision.actual_model);
        assert!(!decision.downgraded);

        // With downgrade: intended != actual
        let decision = router.route(
            "analyze this code and refactor it for better performance",
            &[],
            0.85,
        );
        assert_ne!(decision.intended_model, decision.actual_model);
        assert!(decision.downgraded);
    }

    #[test]
    fn short_model_name_extraction() {
        assert_eq!(ModelRouter::short_model_name("claude-opus-4-20250514"), "Opus");
        assert_eq!(ModelRouter::short_model_name("claude-haiku-4-5-20250901"), "Haiku");
        assert_eq!(ModelRouter::short_model_name("claude-sonnet-4-20250514"), "Sonnet");
    }
}
