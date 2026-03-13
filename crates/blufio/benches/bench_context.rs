// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Criterion benchmarks for context assembly hot paths.
//!
//! Benchmarks the CPU-bound parts of the three-zone context engine:
//! - Zone budget computation (static, conditional, dynamic allocation)
//! - System prompt block construction (JSON serialization)
//! - Token counting via heuristic counter (the fast path)
//!
//! Full assembly requires async I/O + LLM provider and is excluded.
//! These benchmarks target the per-request overhead in the assembly pipeline.

use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

use blufio_config::model::ContextConfig;
use blufio_context::ZoneBudget;
use blufio_core::token_counter::{HeuristicCounter, TokenCounter, TokenizerCache, TokenizerMode};

// ---------------------------------------------------------------------------
// Deterministic test data generators
// ---------------------------------------------------------------------------

/// Generate a deterministic system prompt of approximately `target_bytes`.
fn generate_system_prompt(target_bytes: usize) -> String {
    let block = concat!(
        "You are Blufio, a personal AI assistant. You help users with tasks, ",
        "answer questions, and maintain context across conversations. ",
        "You are secure, efficient, and simple to deploy. ",
        "Always be helpful, concise, and accurate in your responses. ",
        "When you are unsure, acknowledge uncertainty rather than guessing. ",
        "Protect user privacy and handle sensitive information carefully. ",
        "Follow the user's instructions precisely and ask for clarification when needed. ",
        "You have access to long-term memory for persistent facts about the user. ",
    );

    let block_len = block.len();
    let repetitions = (target_bytes / block_len).max(1);
    let mut result = String::with_capacity(target_bytes + block_len);
    for _ in 0..repetitions {
        result.push_str(block);
    }
    result.truncate(target_bytes);
    result
}

/// Generate deterministic conversation messages of approximately `target_bytes` total.
fn generate_conversation(target_bytes: usize) -> Vec<String> {
    let user_msgs = [
        "Can you help me understand how the context engine works in detail?",
        "What are the three zones and how do they interact with each other?",
        "How does compaction work when the context window fills up?",
        "Can you explain the memory retrieval pipeline step by step?",
        "What is reciprocal rank fusion and why is it used here?",
    ];
    let assistant_msgs = [
        "The context engine uses a three-zone architecture: static (system prompt), \
         conditional (session-specific context like memories and skills), and dynamic \
         (conversation history). Each zone has a configurable token budget.",
        "The static zone holds the system prompt and is never truncated. The conditional \
         zone contains memories and skill context, enforced with provider-priority \
         truncation. The dynamic zone manages conversation history with compaction.",
        "When the dynamic zone exceeds the soft trigger threshold, L1 compaction \
         summarizes older messages. If it exceeds the hard trigger, L2 cascade \
         compaction merges L1 summaries into a higher-level summary.",
        "The memory retrieval pipeline: 1) embed query, 2) vector similarity search, \
         3) BM25 keyword search, 4) RRF fusion, 5) importance boost + temporal decay, \
         6) sort by score, 7) MMR diversity reranking, 8) return top-k results.",
        "Reciprocal Rank Fusion merges two ranked lists by computing 1/(k+rank) for \
         each document in each list. Documents appearing in both lists get higher \
         combined scores. The constant k=60 balances rank differences.",
    ];

    let mut messages = Vec::new();
    let mut total_len = 0;
    let mut i = 0;
    while total_len < target_bytes {
        let user = user_msgs[i % user_msgs.len()];
        let assistant = assistant_msgs[i % assistant_msgs.len()];
        messages.push(user.to_string());
        messages.push(assistant.to_string());
        total_len += user.len() + assistant.len();
        i += 1;
    }
    messages
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_zone_budget_computation(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_zone_budget");

    let config = ContextConfig::default();
    let budget = ZoneBudget::from_config(&config);

    // Benchmark budget creation from config.
    group.bench_function("from_config", |b| {
        b.iter(|| ZoneBudget::from_config(black_box(&config)));
    });

    // Benchmark conditional effective budget (with safety margin).
    group.bench_function("conditional_effective", |b| {
        b.iter(|| budget.conditional_effective());
    });

    // Benchmark dynamic budget computation with varying static/conditional token counts.
    for (static_tokens, cond_tokens) in [(500, 1000), (2000, 5000), (3000, 8000)] {
        let label = format!("dynamic_s{static_tokens}_c{cond_tokens}");
        group.bench_function(&label, |b| {
            b.iter(|| {
                budget.dynamic_budget(
                    black_box(static_tokens as u32),
                    black_box(cond_tokens as u32),
                )
            });
        });
    }

    group.finish();
}

fn bench_heuristic_token_counting(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_token_count");

    let counter = HeuristicCounter::default();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    for size in [1024, 5120, 10240] {
        let text = generate_system_prompt(size);
        let label = format!("{}KB", size / 1024);

        group.bench_with_input(BenchmarkId::new("heuristic", &label), &text, |b, text| {
            b.iter(|| rt.block_on(async { counter.count_tokens(black_box(text)).await.unwrap() }));
        });
    }

    group.finish();
}

fn bench_tokenizer_cache_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_tokenizer_cache");

    let cache = Arc::new(TokenizerCache::new(TokenizerMode::Fast));

    // Warm the cache with a model lookup.
    let _ = cache.get_counter("claude-sonnet-4-20250514");

    group.bench_function("cache_hit", |b| {
        b.iter(|| cache.get_counter(black_box("claude-sonnet-4-20250514")));
    });

    group.finish();
}

fn bench_system_blocks_construction(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_system_blocks");

    for size in [1024, 5120, 10240] {
        let prompt = generate_system_prompt(size);
        let label = format!("{}KB", size / 1024);

        group.bench_with_input(
            BenchmarkId::new("json_blocks", &label),
            &prompt,
            |b, prompt| {
                b.iter(|| {
                    // Simulate StaticZone::system_blocks() JSON construction.
                    let blocks = serde_json::json!([{
                        "type": "text",
                        "text": black_box(prompt),
                        "cache_control": {"type": "ephemeral"}
                    }]);
                    black_box(blocks)
                });
            },
        );
    }

    group.finish();
}

fn bench_conversation_token_counting(c: &mut Criterion) {
    let mut group = c.benchmark_group("context_conversation_tokens");

    let counter = HeuristicCounter::default();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    for size in [1024, 5120, 10240] {
        let messages = generate_conversation(size);
        let label = format!("{}KB", size / 1024);

        group.bench_with_input(
            BenchmarkId::new("count_messages", &label),
            &messages,
            |b, messages| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut total = 0usize;
                        for msg in messages {
                            total += counter.count_tokens(black_box(msg)).await.unwrap();
                        }
                        total
                    })
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_zone_budget_computation,
    bench_heuristic_token_counting,
    bench_tokenizer_cache_lookup,
    bench_system_blocks_construction,
    bench_conversation_token_counting,
);
criterion_main!(benches);
