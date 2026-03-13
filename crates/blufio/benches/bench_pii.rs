// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Criterion benchmarks for PII detection and redaction.
//!
//! Benchmarks the regex-based PII scanner on text of varying sizes (1KB, 5KB, 10KB)
//! with both mixed PII content and clean (no PII) text to measure baseline scan cost.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

use blufio_security::pii::{detect_pii, luhn_validate, redact_pii};

// ---------------------------------------------------------------------------
// Deterministic test data generators
// ---------------------------------------------------------------------------

/// Generates text of approximately `target_bytes` with embedded PII of all types.
fn generate_pii_text(target_bytes: usize) -> String {
    let pii_block = concat!(
        "Dear John, please reach out to alice@example.com or bob.jones@company.org ",
        "for project details. Call the office at 555-123-4567 or +1 (800) 555-0199. ",
        "For UK inquiries contact +44 20 7946 0958 or our Berlin office at +49 30 1234 5678. ",
        "Employee SSN on file: 123-45-6789. Payment card ending 4111111111111111 is on record. ",
        "Alternative card: 5500000000000004. Amex backup: 340000000000009. ",
        "Secondary SSN reference: 555-12-3456. Contact hr@internal.example.com for benefits. ",
        "The quarterly revenue report shows a 15% increase over the previous quarter. ",
        "Next meeting is scheduled for Monday at 2pm in conference room B. ",
    );

    let block_len = pii_block.len();
    let repetitions = (target_bytes / block_len).max(1);
    let mut result = String::with_capacity(target_bytes + block_len);
    for _ in 0..repetitions {
        result.push_str(pii_block);
    }
    // Trim to approximate target size.
    result.truncate(target_bytes);
    result
}

/// Generates clean text (no PII) of approximately `target_bytes`.
fn generate_clean_text(target_bytes: usize) -> String {
    let clean_block = concat!(
        "The context engine orchestrates three-zone prompt assembly for the language model. ",
        "Static zone holds the system prompt with cache-aligned blocks for efficient caching. ",
        "Conditional zone provides session-specific context from registered providers. ",
        "Dynamic zone manages conversation history with dual soft and hard compaction triggers. ",
        "Memory retrieval uses hybrid search combining vector similarity and keyword matching. ",
        "Reciprocal rank fusion merges results from both search methods with a constant of sixty. ",
        "Maximal marginal relevance reranking ensures diversity in the final result set. ",
        "Quality scoring evaluates compaction output across entity, decision, action, and numerical ",
        "dimensions using configurable weights for each scoring category. ",
    );

    let block_len = clean_block.len();
    let repetitions = (target_bytes / block_len).max(1);
    let mut result = String::with_capacity(target_bytes + block_len);
    for _ in 0..repetitions {
        result.push_str(clean_block);
    }
    result.truncate(target_bytes);
    result
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_detect_pii_with_mixed_content(c: &mut Criterion) {
    let mut group = c.benchmark_group("pii_detect_mixed");

    for size in [1024, 5120, 10240] {
        let text = generate_pii_text(size);
        let label = format!("{}KB", size / 1024);
        group.bench_with_input(BenchmarkId::new("detect", &label), &text, |b, text| {
            b.iter(|| detect_pii(black_box(text)));
        });
    }

    group.finish();
}

fn bench_detect_pii_clean_text(c: &mut Criterion) {
    let mut group = c.benchmark_group("pii_detect_clean");

    for size in [1024, 5120, 10240] {
        let text = generate_clean_text(size);
        let label = format!("{}KB", size / 1024);
        group.bench_with_input(BenchmarkId::new("detect", &label), &text, |b, text| {
            b.iter(|| detect_pii(black_box(text)));
        });
    }

    group.finish();
}

fn bench_redact_pii(c: &mut Criterion) {
    let mut group = c.benchmark_group("pii_redact");

    for size in [1024, 5120, 10240] {
        let text = generate_pii_text(size);
        let label = format!("{}KB", size / 1024);
        group.bench_with_input(BenchmarkId::new("redact", &label), &text, |b, text| {
            b.iter(|| redact_pii(black_box(text)));
        });
    }

    group.finish();
}

fn bench_luhn_validate(c: &mut Criterion) {
    let mut group = c.benchmark_group("pii_luhn");

    group.bench_function("valid_visa", |b| {
        b.iter(|| luhn_validate(black_box("4111111111111111")));
    });

    group.bench_function("invalid_number", |b| {
        b.iter(|| luhn_validate(black_box("4111111111111112")));
    });

    group.bench_function("with_spaces", |b| {
        b.iter(|| luhn_validate(black_box("4111 1111 1111 1111")));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_detect_pii_with_mixed_content,
    bench_detect_pii_clean_text,
    bench_redact_pii,
    bench_luhn_validate,
);
criterion_main!(benches);
