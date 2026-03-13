// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Criterion benchmarks for compaction quality scoring.
//!
//! Benchmarks the CPU-bound quality score computation and gate evaluation:
//! - Weighted score calculation across 4 dimensions
//! - Weakest dimension identification
//! - Quality gate application (proceed/retry/abort)
//!
//! The LLM-based evaluation is excluded; these benchmarks target the
//! scoring and decision logic that runs after every compaction cycle.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

use blufio_context::compaction::quality::{QualityScores, QualityWeights, apply_gate};

// ---------------------------------------------------------------------------
// Deterministic test data generators
// ---------------------------------------------------------------------------

/// Generate a set of quality scores deterministically from a seed.
fn make_scores(seed: u32) -> QualityScores {
    // Use seed to create varied but reproducible scores.
    let base = (seed % 100) as f64 / 100.0;
    QualityScores {
        entity: (base * 1.1).min(1.0),
        decision: (base * 0.9).min(1.0),
        action: (base * 0.85).min(1.0),
        numerical: (base * 0.95).min(1.0),
    }
}

/// Generate a batch of quality scores for batch benchmarks.
fn make_score_batch(count: usize) -> Vec<QualityScores> {
    (0..count).map(|i| make_scores(i as u32)).collect()
}

/// Standard quality weights matching production defaults.
fn default_weights() -> QualityWeights {
    QualityWeights {
        entity: 0.35,
        decision: 0.25,
        action: 0.25,
        numerical: 0.15,
    }
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_weighted_score(c: &mut Criterion) {
    let mut group = c.benchmark_group("compaction_weighted_score");

    let weights = default_weights();

    // Small: 5 quality score computations (typical single compaction).
    for count in [5, 50, 200] {
        let scores = make_score_batch(count);
        let label = format!("{count}_scores");

        group.bench_with_input(
            BenchmarkId::new("weighted", &label),
            &scores,
            |b, scores| {
                b.iter(|| {
                    let _results: Vec<f64> = scores
                        .iter()
                        .map(|s| s.weighted_score(black_box(&weights)))
                        .collect();
                });
            },
        );
    }

    group.finish();
}

fn bench_weakest_dimension(c: &mut Criterion) {
    let mut group = c.benchmark_group("compaction_weakest_dim");

    for count in [5, 50, 200] {
        let scores = make_score_batch(count);
        let label = format!("{count}_scores");

        group.bench_with_input(
            BenchmarkId::new("weakest", &label),
            &scores,
            |b, scores| {
                b.iter(|| {
                    let _results: Vec<&str> = scores
                        .iter()
                        .map(|s| s.weakest_dimension())
                        .collect();
                });
            },
        );
    }

    group.finish();
}

fn bench_apply_gate(c: &mut Criterion) {
    let mut group = c.benchmark_group("compaction_gate");

    let weights = default_weights();

    for count in [5, 50, 200] {
        let scores = make_score_batch(count);
        let label = format!("{count}_scores");

        group.bench_with_input(
            BenchmarkId::new("gate", &label),
            &scores,
            |b, scores| {
                b.iter(|| {
                    for s in scores {
                        let weighted = s.weighted_score(&weights);
                        let weakest = s.weakest_dimension();
                        let _gate = apply_gate(
                            black_box(weighted),
                            black_box(0.6),
                            black_box(0.4),
                            black_box(weakest),
                        );
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_full_scoring_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("compaction_pipeline");

    let weights = default_weights();

    // Simulate the full quality scoring pipeline (score + weakest + gate)
    // at different batch sizes representing compaction volume.
    for count in [5, 50, 200] {
        let scores = make_score_batch(count);
        let label = format!("{count}_scores");

        group.bench_with_input(
            BenchmarkId::new("full_pipeline", &label),
            &scores,
            |b, scores| {
                b.iter(|| {
                    let mut proceed_count = 0u32;
                    let mut retry_count = 0u32;
                    let mut abort_count = 0u32;
                    for s in scores {
                        let weighted = s.weighted_score(black_box(&weights));
                        let weakest = s.weakest_dimension();
                        match apply_gate(weighted, 0.6, 0.4, weakest) {
                            blufio_context::compaction::quality::GateResult::Proceed(_) => {
                                proceed_count += 1
                            }
                            blufio_context::compaction::quality::GateResult::Retry(_, _) => {
                                retry_count += 1
                            }
                            blufio_context::compaction::quality::GateResult::Abort(_) => {
                                abort_count += 1
                            }
                        }
                    }
                    (proceed_count, retry_count, abort_count)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_weighted_score,
    bench_weakest_dimension,
    bench_apply_gate,
    bench_full_scoring_pipeline,
);
criterion_main!(benches);
