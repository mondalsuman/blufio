// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Property-based tests for compaction quality scoring.
//!
//! Validates invariants of `QualityScores::weighted_score()` and
//! `apply_gate()` using proptest with randomized inputs.

use blufio_context::compaction::quality::{GateResult, QualityScores, QualityWeights, apply_gate};
use proptest::prelude::*;

/// Standard weights that sum to 1.0 (matching production defaults).
fn default_weights() -> QualityWeights {
    QualityWeights {
        entity: 0.35,
        decision: 0.25,
        action: 0.25,
        numerical: 0.15,
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..Default::default() })]

    // ── Property 1: score always in [0.0, 1.0] for any input ──────────

    #[test]
    fn weighted_score_always_in_unit_range(
        entity in 0.0f64..=1.0,
        decision in 0.0f64..=1.0,
        action in 0.0f64..=1.0,
        numerical in 0.0f64..=1.0,
    ) {
        let scores = QualityScores { entity, decision, action, numerical };
        let weights = default_weights();
        let result = scores.weighted_score(&weights);
        prop_assert!(
            (0.0..=1.0).contains(&result),
            "weighted_score must be in [0.0, 1.0], got: {result}"
        );
    }

    // ── Property 2: monotonicity (higher scores -> higher or equal weighted score) ──

    #[test]
    fn higher_all_scores_produce_higher_or_equal_weighted_score(
        base_entity in 0.0f64..=0.9,
        base_decision in 0.0f64..=0.9,
        base_action in 0.0f64..=0.9,
        base_numerical in 0.0f64..=0.9,
        delta_entity in 0.0f64..=0.1,
        delta_decision in 0.0f64..=0.1,
        delta_action in 0.0f64..=0.1,
        delta_numerical in 0.0f64..=0.1,
    ) {
        let weights = default_weights();

        let base_scores = QualityScores {
            entity: base_entity,
            decision: base_decision,
            action: base_action,
            numerical: base_numerical,
        };

        let higher_scores = QualityScores {
            entity: base_entity + delta_entity,
            decision: base_decision + delta_decision,
            action: base_action + delta_action,
            numerical: base_numerical + delta_numerical,
        };

        let base_result = base_scores.weighted_score(&weights);
        let higher_result = higher_scores.weighted_score(&weights);

        prop_assert!(
            higher_result >= base_result - f64::EPSILON,
            "higher scores should produce >= weighted score: {higher_result} < {base_result}"
        );
    }

    // ── Property 3: zero input -> score is 0.0 ────────────────────────

    #[test]
    fn zero_scores_produce_zero_weighted_score(
        w_entity in 0.0f64..=1.0,
        w_decision in 0.0f64..=1.0,
        w_action in 0.0f64..=1.0,
        w_numerical in 0.0f64..=1.0,
    ) {
        let scores = QualityScores {
            entity: 0.0,
            decision: 0.0,
            action: 0.0,
            numerical: 0.0,
        };
        let weights = QualityWeights {
            entity: w_entity,
            decision: w_decision,
            action: w_action,
            numerical: w_numerical,
        };
        let result = scores.weighted_score(&weights);
        prop_assert!(
            result.abs() < f64::EPSILON,
            "all-zero scores should produce 0.0, got: {result}"
        );
    }

    // ── Property 4: perfect scores -> score is 1.0 (with normalized weights) ──

    #[test]
    fn perfect_scores_produce_one_with_normalized_weights(
        // Generate weights that will be normalized to sum to 1.0
        raw_entity in 0.01f64..=1.0,
        raw_decision in 0.01f64..=1.0,
        raw_action in 0.01f64..=1.0,
        raw_numerical in 0.01f64..=1.0,
    ) {
        // Normalize weights to sum to 1.0
        let total = raw_entity + raw_decision + raw_action + raw_numerical;
        let weights = QualityWeights {
            entity: raw_entity / total,
            decision: raw_decision / total,
            action: raw_action / total,
            numerical: raw_numerical / total,
        };

        let scores = QualityScores {
            entity: 1.0,
            decision: 1.0,
            action: 1.0,
            numerical: 1.0,
        };

        let result = scores.weighted_score(&weights);
        prop_assert!(
            (result - 1.0).abs() < 1e-10,
            "all-1.0 scores with normalized weights should produce 1.0, got: {result}"
        );
    }

    // ── Property 5: gate result consistency ───────────────────────────

    #[test]
    fn gate_result_matches_score_ranges(
        score in 0.0f64..=1.0,
        proceed_threshold in 0.5f64..=1.0,
        retry_ratio in 0.3f64..=0.9,
    ) {
        let retry_threshold = proceed_threshold * retry_ratio;

        match apply_gate(score, proceed_threshold, retry_threshold, "test") {
            GateResult::Proceed(s) => {
                prop_assert!(s >= proceed_threshold,
                    "Proceed score {s} should be >= proceed_threshold {proceed_threshold}");
            }
            GateResult::Retry(s, _) => {
                prop_assert!(s >= retry_threshold && s < proceed_threshold,
                    "Retry score {s} should be in [{retry_threshold}, {proceed_threshold})");
            }
            GateResult::Abort(s) => {
                prop_assert!(s < retry_threshold,
                    "Abort score {s} should be < retry_threshold {retry_threshold}");
            }
        }
    }

    // ── Property 6: weakest dimension is correct ──────────────────────

    #[test]
    fn weakest_dimension_is_minimum(
        entity in 0.0f64..=1.0,
        decision in 0.0f64..=1.0,
        action in 0.0f64..=1.0,
        numerical in 0.0f64..=1.0,
    ) {
        let scores = QualityScores { entity, decision, action, numerical };
        let weakest = scores.weakest_dimension();
        let min_score = entity.min(decision).min(action).min(numerical);

        let weakest_score = match weakest {
            "entity" => entity,
            "decision" => decision,
            "action" => action,
            "numerical" => numerical,
            other => panic!("unexpected dimension: {other}"),
        };

        prop_assert!(
            (weakest_score - min_score).abs() < f64::EPSILON,
            "weakest dimension '{weakest}' score {weakest_score} should be the minimum {min_score}"
        );
    }
}
