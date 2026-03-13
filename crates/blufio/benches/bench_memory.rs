// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Criterion benchmarks for memory retrieval hot paths.
//!
//! Benchmarks the CPU-bound parts of the memory retrieval pipeline:
//! - Reciprocal Rank Fusion (RRF) merging of vector + BM25 results
//! - Cosine similarity computation
//! - MMR diversity reranking (the greedy selection algorithm)
//!
//! Embedding generation is excluded since it requires an ONNX model.
//! These benchmarks focus on the algorithmic hot paths that scale with
//! the number of memory entries.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

use blufio_memory::retriever::reciprocal_rank_fusion;
use blufio_memory::types::cosine_similarity;

// ---------------------------------------------------------------------------
// Deterministic test data generators
// ---------------------------------------------------------------------------

/// Generate a deterministic f32 embedding vector of dimension `dim` seeded by `seed`.
fn make_embedding(dim: usize, seed: u32) -> Vec<f32> {
    let mut vec = Vec::with_capacity(dim);
    // Simple deterministic pseudo-random using a linear congruential generator.
    let mut state = seed.wrapping_mul(2654435761);
    for _ in 0..dim {
        state = state.wrapping_mul(1103515245).wrapping_add(12345);
        let val = ((state >> 16) & 0x7FFF) as f32 / 32767.0;
        vec.push(val);
    }
    // L2-normalize for realistic cosine similarity behavior.
    let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for v in &mut vec {
            *v /= norm;
        }
    }
    vec
}

/// Generate vector search results: (id, similarity) pairs.
fn make_vector_results(n: usize) -> Vec<(String, f32)> {
    (0..n)
        .map(|i| {
            let score = 0.95 - (i as f32 * 0.01); // descending scores
            (format!("mem-{i}"), score)
        })
        .collect()
}

/// Generate BM25 search results: (id, bm25_score) pairs.
/// BM25 scores are negative (more negative = more relevant).
fn make_bm25_results(n: usize, id_offset: usize) -> Vec<(String, f64)> {
    (0..n)
        .map(|i| {
            let id = format!("mem-{}", i + id_offset);
            let score = -(10.0 - i as f64 * 0.2); // ascending (less negative)
            (id, score)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_rrf_fusion(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_rrf_fusion");

    for n in [50, 200, 500] {
        // Create overlapping result sets: 50% overlap between vector and BM25.
        let vector_results = make_vector_results(n);
        let bm25_results = make_bm25_results(n, n / 2);

        group.bench_with_input(
            BenchmarkId::new("rrf", format!("{n}_entries")),
            &(vector_results, bm25_results),
            |b, (vec_r, bm25_r)| {
                b.iter(|| reciprocal_rank_fusion(black_box(vec_r), black_box(bm25_r)));
            },
        );
    }

    group.finish();
}

fn bench_cosine_similarity(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_cosine_sim");

    let dim = 384; // all-MiniLM-L6-v2 dimension
    let a = make_embedding(dim, 42);
    let b = make_embedding(dim, 99);

    group.bench_function("384dim", |bench| {
        bench.iter(|| cosine_similarity(black_box(&a), black_box(&b)));
    });

    group.finish();
}

fn bench_cosine_similarity_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_cosine_batch");

    let dim = 384;
    let query = make_embedding(dim, 1);

    for n in [50, 200, 500] {
        let embeddings: Vec<Vec<f32>> = (0..n)
            .map(|i| make_embedding(dim, i as u32 + 100))
            .collect();

        group.bench_with_input(
            BenchmarkId::new("batch_sim", format!("{n}_vectors")),
            &embeddings,
            |b, embs| {
                b.iter(|| {
                    let _scores: Vec<f32> = embs
                        .iter()
                        .map(|e| cosine_similarity(black_box(&query), black_box(e)))
                        .collect();
                });
            },
        );
    }

    group.finish();
}

fn bench_rrf_with_sorting(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_rrf_sort");

    // Benchmark RRF fusion + sort pipeline (the full non-IO path).
    for n in [50, 200, 500] {
        let vector_results = make_vector_results(n);
        let bm25_results = make_bm25_results(n, n / 2);

        group.bench_with_input(
            BenchmarkId::new("fuse_and_sort", format!("{n}_entries")),
            &(vector_results, bm25_results),
            |b, (vec_r, bm25_r)| {
                b.iter(|| {
                    let mut fused = reciprocal_rank_fusion(black_box(vec_r), black_box(bm25_r));
                    fused
                        .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    fused
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_rrf_fusion,
    bench_cosine_similarity,
    bench_cosine_similarity_batch,
    bench_rrf_with_sorting,
);
criterion_main!(benches);
