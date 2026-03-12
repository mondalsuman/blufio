// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Quality scoring engine for compaction summaries.
//!
//! Evaluates compaction output via a separate LLM call that scores entity,
//! decision, action, and numerical retention on a 0.0-1.0 scale. Quality
//! gates enforce thresholds: >=0.6 proceed, 0.4-0.6 retry with weakest
//! dimension emphasis, <0.4 abort.

use blufio_config::model::ContextConfig;
use blufio_core::error::BlufioError;
use blufio_core::traits::ProviderAdapter;
use blufio_core::types::{ContentBlock, Message, ProviderMessage, ProviderRequest};
use serde::Deserialize;

/// System prompt for quality scoring evaluation.
const QUALITY_SCORING_PROMPT: &str = r#"You are a compaction quality evaluator. Compare the original conversation messages with the generated summary and score how well the summary preserves critical information.

Score each dimension from 0.0 to 1.0:
- entity: How well are named entities (people, places, tools, projects) preserved?
- decision: How well are key decisions and their rationale preserved?
- action: How well are action items, commitments, and their status preserved?
- numerical: How well are numerical data (dates, amounts, counts, measurements) preserved?

Return ONLY a JSON object with these four fields. No other text.
Example: {"entity": 0.9, "decision": 0.8, "action": 0.75, "numerical": 0.85}"#;

/// Quality scores for the four dimensions evaluated by the scoring LLM.
#[derive(Debug, Clone, Deserialize)]
pub struct QualityScores {
    /// How well named entities are preserved (0.0-1.0).
    pub entity: f64,
    /// How well key decisions are preserved (0.0-1.0).
    pub decision: f64,
    /// How well action items are preserved (0.0-1.0).
    pub action: f64,
    /// How well numerical data is preserved (0.0-1.0).
    pub numerical: f64,
}

/// Configurable weights for the four quality dimensions.
#[derive(Debug, Clone)]
pub struct QualityWeights {
    /// Weight for entity preservation (default 0.35).
    pub entity: f64,
    /// Weight for decision preservation (default 0.25).
    pub decision: f64,
    /// Weight for action preservation (default 0.25).
    pub action: f64,
    /// Weight for numerical preservation (default 0.15).
    pub numerical: f64,
}

impl QualityWeights {
    /// Creates quality weights from context configuration.
    pub fn from_config(config: &ContextConfig) -> Self {
        Self {
            entity: config.quality_weight_entity,
            decision: config.quality_weight_decision,
            action: config.quality_weight_action,
            numerical: config.quality_weight_numerical,
        }
    }
}

impl QualityScores {
    /// Computes the weighted score across all dimensions.
    pub fn weighted_score(&self, weights: &QualityWeights) -> f64 {
        self.entity * weights.entity
            + self.decision * weights.decision
            + self.action * weights.action
            + self.numerical * weights.numerical
    }

    /// Returns the name of the dimension with the lowest score.
    pub fn weakest_dimension(&self) -> &str {
        let mut weakest = "entity";
        let mut min = self.entity;
        if self.decision < min {
            weakest = "decision";
            min = self.decision;
        }
        if self.action < min {
            weakest = "action";
            min = self.action;
        }
        if self.numerical < min {
            weakest = "numerical";
            // min not needed after this point
            let _ = min;
        }
        weakest
    }
}

/// Result of applying a quality gate to a weighted score.
#[derive(Debug)]
pub enum GateResult {
    /// Score meets or exceeds the proceed threshold.
    Proceed(f64),
    /// Score is between retry and proceed thresholds; includes weakest dimension name.
    Retry(f64, String),
    /// Score is below the retry threshold.
    Abort(f64),
}

/// Evaluates compaction quality by comparing original messages against the summary.
///
/// Sends both the original messages and the summary to the LLM for evaluation.
/// On JSON parse failure, returns fallback scores of 0.5 (retry range) per CONTEXT.md.
pub async fn evaluate_quality(
    provider: &dyn ProviderAdapter,
    original_messages: &[Message],
    summary: &str,
    model: &str,
) -> Result<QualityScores, BlufioError> {
    let conversation_text: String = original_messages
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");

    let request = ProviderRequest {
        model: model.to_string(),
        system_prompt: Some(QUALITY_SCORING_PROMPT.to_string()),
        system_blocks: None,
        messages: vec![ProviderMessage {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: format!(
                    "Original conversation:\n{}\n\nGenerated summary:\n{}",
                    conversation_text, summary
                ),
            }],
        }],
        max_tokens: 256,
        stream: false,
        tools: None,
    };

    let response = provider.complete(request).await?;

    // Parse quality scores from JSON response.
    match parse_quality_scores(&response.content) {
        Some(scores) => Ok(scores),
        None => {
            tracing::warn!("Quality score JSON parse failed, treating as 0.5");
            Ok(QualityScores {
                entity: 0.5,
                decision: 0.5,
                action: 0.5,
                numerical: 0.5,
            })
        }
    }
}

/// Applies a quality gate to a weighted score.
///
/// - `score >= proceed_threshold` -> `GateResult::Proceed`
/// - `score >= retry_threshold` -> `GateResult::Retry`
/// - otherwise -> `GateResult::Abort`
pub fn apply_gate(
    score: f64,
    proceed_threshold: f64,
    retry_threshold: f64,
    weakest: &str,
) -> GateResult {
    if score >= proceed_threshold {
        GateResult::Proceed(score)
    } else if score >= retry_threshold {
        GateResult::Retry(score, weakest.to_string())
    } else {
        GateResult::Abort(score)
    }
}

/// Evaluates quality and returns the weighted score with gate result.
///
/// This is a convenience function that evaluates quality, computes the weighted
/// score, and applies the gate in one call. The caller (DynamicZone) is
/// responsible for handling retry logic (re-compacting with emphasis on the
/// weakest dimension).
pub async fn evaluate_and_gate(
    provider: &dyn ProviderAdapter,
    original_messages: &[Message],
    summary: &str,
    model: &str,
    weights: &QualityWeights,
    proceed_threshold: f64,
    retry_threshold: f64,
) -> Result<(f64, GateResult), BlufioError> {
    let scores = evaluate_quality(provider, original_messages, summary, model).await?;
    let weighted = scores.weighted_score(weights);
    let weakest = scores.weakest_dimension().to_string();
    let gate = apply_gate(weighted, proceed_threshold, retry_threshold, &weakest);

    // Record Prometheus metrics via tracing (blufio-prometheus subscribes to EventBus).
    tracing::info!(
        quality_score = weighted,
        entity = scores.entity,
        decision = scores.decision,
        action = scores.action,
        numerical = scores.numerical,
        gate_result = match &gate {
            GateResult::Proceed(_) => "proceed",
            GateResult::Retry(_, _) => "retry",
            GateResult::Abort(_) => "abort",
        },
        "compaction quality evaluated"
    );

    Ok((weighted, gate))
}

/// Attempts to parse QualityScores from LLM response text.
///
/// Handles common cases where the model wraps JSON in markdown or text.
fn parse_quality_scores(text: &str) -> Option<QualityScores> {
    // Try direct parse first.
    if let Ok(parsed) = serde_json::from_str::<QualityScores>(text) {
        return Some(parsed);
    }

    // Try to extract JSON object from surrounding text.
    if let Some(start) = text.find('{')
        && let Some(end) = text.rfind('}')
        && let Ok(parsed) = serde_json::from_str::<QualityScores>(&text[start..=end])
    {
        return Some(parsed);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_score_calculation() {
        let scores = QualityScores {
            entity: 0.9,
            decision: 0.8,
            action: 0.75,
            numerical: 0.85,
        };
        let weights = QualityWeights {
            entity: 0.35,
            decision: 0.25,
            action: 0.25,
            numerical: 0.15,
        };
        // 0.9*0.35 + 0.8*0.25 + 0.75*0.25 + 0.85*0.15
        // = 0.315 + 0.2 + 0.1875 + 0.1275 = 0.83
        let score = scores.weighted_score(&weights);
        assert!((score - 0.83).abs() < 0.001);
    }

    #[test]
    fn weighted_score_all_ones() {
        let scores = QualityScores {
            entity: 1.0,
            decision: 1.0,
            action: 1.0,
            numerical: 1.0,
        };
        let weights = QualityWeights {
            entity: 0.35,
            decision: 0.25,
            action: 0.25,
            numerical: 0.15,
        };
        let score = scores.weighted_score(&weights);
        assert!((score - 1.0).abs() < 0.001);
    }

    #[test]
    fn weakest_dimension_entity() {
        let scores = QualityScores {
            entity: 0.2,
            decision: 0.8,
            action: 0.75,
            numerical: 0.85,
        };
        assert_eq!(scores.weakest_dimension(), "entity");
    }

    #[test]
    fn weakest_dimension_decision() {
        let scores = QualityScores {
            entity: 0.9,
            decision: 0.3,
            action: 0.75,
            numerical: 0.85,
        };
        assert_eq!(scores.weakest_dimension(), "decision");
    }

    #[test]
    fn weakest_dimension_action() {
        let scores = QualityScores {
            entity: 0.9,
            decision: 0.8,
            action: 0.1,
            numerical: 0.85,
        };
        assert_eq!(scores.weakest_dimension(), "action");
    }

    #[test]
    fn weakest_dimension_numerical() {
        let scores = QualityScores {
            entity: 0.9,
            decision: 0.8,
            action: 0.75,
            numerical: 0.05,
        };
        assert_eq!(scores.weakest_dimension(), "numerical");
    }

    #[test]
    fn gate_proceed() {
        match apply_gate(0.75, 0.6, 0.4, "entity") {
            GateResult::Proceed(s) => assert!((s - 0.75).abs() < 0.001),
            _ => panic!("expected Proceed"),
        }
    }

    #[test]
    fn gate_proceed_at_threshold() {
        match apply_gate(0.6, 0.6, 0.4, "entity") {
            GateResult::Proceed(s) => assert!((s - 0.6).abs() < 0.001),
            _ => panic!("expected Proceed at threshold"),
        }
    }

    #[test]
    fn gate_retry() {
        match apply_gate(0.5, 0.6, 0.4, "numerical") {
            GateResult::Retry(s, dim) => {
                assert!((s - 0.5).abs() < 0.001);
                assert_eq!(dim, "numerical");
            }
            _ => panic!("expected Retry"),
        }
    }

    #[test]
    fn gate_retry_at_threshold() {
        match apply_gate(0.4, 0.6, 0.4, "action") {
            GateResult::Retry(s, dim) => {
                assert!((s - 0.4).abs() < 0.001);
                assert_eq!(dim, "action");
            }
            _ => panic!("expected Retry at retry threshold"),
        }
    }

    #[test]
    fn gate_abort() {
        match apply_gate(0.3, 0.6, 0.4, "entity") {
            GateResult::Abort(s) => assert!((s - 0.3).abs() < 0.001),
            _ => panic!("expected Abort"),
        }
    }

    #[test]
    fn parse_valid_json() {
        let json = r#"{"entity": 0.9, "decision": 0.8, "action": 0.75, "numerical": 0.85}"#;
        let scores = parse_quality_scores(json).unwrap();
        assert!((scores.entity - 0.9).abs() < 0.001);
        assert!((scores.decision - 0.8).abs() < 0.001);
        assert!((scores.action - 0.75).abs() < 0.001);
        assert!((scores.numerical - 0.85).abs() < 0.001);
    }

    #[test]
    fn parse_json_with_surrounding_text() {
        let response = r#"Here are the quality scores:
{"entity": 0.7, "decision": 0.6, "action": 0.5, "numerical": 0.4}
That's my evaluation."#;
        let scores = parse_quality_scores(response).unwrap();
        assert!((scores.entity - 0.7).abs() < 0.001);
        assert!((scores.numerical - 0.4).abs() < 0.001);
    }

    #[test]
    fn parse_invalid_json_returns_none() {
        assert!(parse_quality_scores("not a json").is_none());
    }

    #[test]
    fn quality_weights_from_config() {
        let config = ContextConfig::default();
        let weights = QualityWeights::from_config(&config);
        assert!((weights.entity - 0.35).abs() < 0.001);
        assert!((weights.decision - 0.25).abs() < 0.001);
        assert!((weights.action - 0.25).abs() < 0.001);
        assert!((weights.numerical - 0.15).abs() < 0.001);
    }

    #[test]
    fn quality_scoring_prompt_contains_dimensions() {
        assert!(QUALITY_SCORING_PROMPT.contains("entity"));
        assert!(QUALITY_SCORING_PROMPT.contains("decision"));
        assert!(QUALITY_SCORING_PROMPT.contains("action"));
        assert!(QUALITY_SCORING_PROMPT.contains("numerical"));
        assert!(QUALITY_SCORING_PROMPT.contains("JSON"));
    }
}
