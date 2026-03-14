// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Criterion benchmarks comparing vec0 KNN vs in-memory cosine similarity.
//!
//! Measures search latency at 100, 1000, 5000, and 10000 entries to
//! establish baseline performance characteristics for the sqlite-vec
//! integration. Setup time (DB creation, population) is excluded from
//! measurements. Counts >= 5000 use reduced sample sizes to avoid CI
//! timeouts.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

use blufio_memory::types::{cosine_similarity, vec_to_blob};
use blufio_memory::vec0;

// ---------------------------------------------------------------------------
// Deterministic test data generators
// ---------------------------------------------------------------------------

/// Generate a normalized deterministic 384-dim embedding from a seed.
fn make_embedding(seed: u32) -> Vec<f32> {
    let mut emb = vec![0.0f32; 384];
    for (i, val) in emb.iter_mut().enumerate() {
        *val = ((seed as f32 * 0.1 + i as f32 * 0.01).sin()) * 0.1;
    }
    let norm: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for x in &mut emb {
            *x /= norm;
        }
    }
    emb
}

/// Set up a rusqlite in-memory DB with `count` entries in both memories and vec0 tables.
/// Returns the connection and a query embedding.
fn setup_bench_db(count: usize) -> (rusqlite::Connection, Vec<f32>) {
    vec0::ensure_sqlite_vec_registered();
    let conn = rusqlite::Connection::open_in_memory().unwrap();

    // Create tables
    conn.execute_batch(
        "CREATE TABLE memories (
            id TEXT PRIMARY KEY NOT NULL,
            content TEXT NOT NULL,
            embedding BLOB NOT NULL,
            source TEXT NOT NULL DEFAULT 'extracted',
            confidence REAL NOT NULL DEFAULT 0.5,
            status TEXT NOT NULL DEFAULT 'active',
            superseded_by TEXT,
            session_id TEXT,
            classification TEXT NOT NULL DEFAULT 'internal',
            created_at TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL DEFAULT '',
            deleted_at TEXT
        );",
    )
    .unwrap();

    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec0 USING vec0(\
            status text, \
            classification text, \
            session_id text partition key, \
            embedding float[384] distance_metric=cosine, \
            +memory_id text, \
            +content text, \
            +source text, \
            +confidence float, \
            +created_at text\
        );",
    )
    .unwrap();

    // Insert entries
    for i in 0..count {
        let id = format!("mem-{i}");
        let emb = make_embedding(i as u32 + 10);
        let emb_blob = vec_to_blob(&emb);
        let content = format!("Benchmark memory number {i}");

        conn.execute(
            "INSERT INTO memories (id, content, embedding, source, confidence, status, \
             classification, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 'explicit', 0.9, 'active', 'internal', \
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            rusqlite::params![id, content, emb_blob],
        )
        .unwrap();

        let rowid: i64 = conn
            .query_row(
                "SELECT rowid FROM memories WHERE id = ?1",
                rusqlite::params![id],
                |row| row.get(0),
            )
            .unwrap();

        let tx = conn.unchecked_transaction().unwrap();
        vec0::vec0_insert(
            &tx,
            rowid,
            "active",
            "internal",
            None,
            &emb,
            &id,
            &content,
            "explicit",
            0.9,
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        tx.commit().unwrap();
    }

    // Query embedding: close to entry #5
    let query_emb = make_embedding(15);
    (conn, query_emb)
}

/// Set up in-memory embeddings for brute-force cosine comparison.
fn setup_bench_embeddings(count: usize) -> Vec<(String, Vec<f32>)> {
    (0..count)
        .map(|i| (format!("mem-{i}"), make_embedding(i as u32 + 10)))
        .collect()
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_vector_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_search");

    for count in [100, 1000, 5000, 10000] {
        // Reduce sample size for large counts to avoid CI timeouts
        if count >= 5000 {
            group.sample_size(10);
            group.measurement_time(std::time::Duration::from_secs(30));
        }

        // vec0 KNN
        let (conn, query_emb) = setup_bench_db(count);
        group.bench_with_input(
            BenchmarkId::new("vec0_knn", format!("{count}_entries")),
            &(&conn, &query_emb),
            |b, &(conn, query_emb)| {
                b.iter(|| {
                    let results =
                        vec0::vec0_search(black_box(conn), black_box(query_emb), 10, 0.3, None)
                            .unwrap();
                    black_box(results);
                });
            },
        );

        // In-memory cosine
        let embeddings = setup_bench_embeddings(count);
        let query_for_mem = make_embedding(15);
        group.bench_with_input(
            BenchmarkId::new("in_memory_cosine", format!("{count}_entries")),
            &(&embeddings, &query_for_mem),
            |b, &(embeddings, query_emb)| {
                b.iter(|| {
                    let mut results: Vec<(String, f32)> = embeddings
                        .iter()
                        .filter_map(|(id, emb)| {
                            let sim = cosine_similarity(black_box(query_emb), black_box(emb));
                            if sim >= 0.3 {
                                Some((id.clone(), sim))
                            } else {
                                None
                            }
                        })
                        .collect();
                    results
                        .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    results.truncate(10);
                    black_box(results);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_vector_search);
criterion_main!(benches);
