// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Criterion benchmarks for the hybrid retrieval pipeline.
//!
//! Measures the synchronous hot path of the hybrid retrieval pipeline:
//! - vec0 KNN search
//! - BM25 keyword search via FTS5
//! - Reciprocal Rank Fusion (RRF) merging both result sets
//!
//! These components are benchmarked individually and combined to measure
//! the end-to-end synchronous pipeline latency (excluding ONNX embedding
//! generation). Entry counts: [100, 500, 1000].
//!
//! The full async pipeline (including ONNX embedding) requires model files
//! on disk and is skipped when the model is not available.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

use blufio_memory::retriever::reciprocal_rank_fusion;
use blufio_memory::types::vec_to_blob;
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

/// Realistic memory content for BM25 diversity (different topics).
const MEMORY_TOPICS: &[&str] = &[
    "The user prefers dark mode in all applications and editors",
    "Project deadline is next Friday, need to finish the API integration",
    "Discussed the new database migration strategy for PostgreSQL to SQLite",
    "User mentioned they enjoy hiking in the mountains during weekends",
    "The deployment pipeline uses GitHub Actions with Docker containers",
    "Favorite programming languages are Rust and TypeScript",
    "Meeting notes from the architecture review of the microservices",
    "User requested help with debugging the authentication flow",
    "The team decided to use WebSocket for real-time notifications",
    "Budget allocation for Q2 includes cloud infrastructure costs",
    "The user lives in San Francisco and works remotely",
    "Code review feedback on the memory retrieval optimization",
    "Discussed machine learning model training with PyTorch",
    "The project uses a monorepo structure with Cargo workspaces",
    "User asked about best practices for error handling in async Rust",
    "Sprint retrospective highlighted need for better documentation",
    "Investigated performance bottleneck in the vector search pipeline",
    "The CI pipeline runs benchmarks on every push to main branch",
    "User preference for concise responses without unnecessary detail",
    "Explored options for cross-platform desktop application framework",
];

/// Generate diverse memory content for a given index.
fn generate_memory_content(index: usize) -> String {
    let topic = MEMORY_TOPICS[index % MEMORY_TOPICS.len()];
    // Add index-specific variation for uniqueness
    format!("{topic}. Context item {index} with additional details for search diversity.")
}

/// Set up an in-memory SQLite DB with memories, vec0, and FTS5 tables.
/// Returns the connection, a query embedding, and the query text for BM25.
fn setup_hybrid_bench_db(count: usize) -> (rusqlite::Connection, Vec<f32>, String) {
    vec0::ensure_sqlite_vec_registered();
    let conn = rusqlite::Connection::open_in_memory().unwrap();

    // Create memories table
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

    // Create vec0 virtual table
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

    // Create FTS5 table with content sync triggers
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
            content,
            content='memories',
            content_rowid='rowid'
        );

        CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
            INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
        END;

        CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content)
                VALUES('delete', old.rowid, old.content);
        END;

        CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
            INSERT INTO memories_fts(memories_fts, rowid, content)
                VALUES('delete', old.rowid, old.content);
            INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
        END;

        CREATE INDEX IF NOT EXISTS idx_memories_status ON memories(status);
        CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);",
    )
    .unwrap();

    // Insert entries
    for i in 0..count {
        let id = format!("mem-{i}");
        let emb = make_embedding(i as u32 + 10);
        let emb_blob = vec_to_blob(&emb);
        let content = generate_memory_content(i);

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

    // Query: a search about project discussions
    let query_emb = make_embedding(15);
    let query_text = "project architecture discussion".to_string();
    (conn, query_emb, query_text)
}

// ---------------------------------------------------------------------------
// BM25 search (synchronous, directly on rusqlite)
// ---------------------------------------------------------------------------

/// Run BM25 search on FTS5 table synchronously.
fn bm25_search(conn: &rusqlite::Connection, query: &str, limit: usize) -> Vec<(String, f64)> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, bm25(memories_fts) as score \
             FROM memories_fts \
             JOIN memories m ON m.rowid = memories_fts.rowid \
             WHERE memories_fts MATCH ?1 \
             AND m.status = 'active' \
             AND m.classification != 'restricted' \
             AND m.deleted_at IS NULL \
             ORDER BY bm25(memories_fts) LIMIT ?2",
        )
        .unwrap();

    stmt.query_map(rusqlite::params![query, limit as i64], |row| {
        let id: String = row.get(0)?;
        let score: f64 = row.get(1)?;
        Ok((id, score))
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_hybrid_pipeline(c: &mut Criterion) {
    let mut group = c.benchmark_group("hybrid_pipeline");
    // ONNX embedding is expensive; use reduced sample size for all sizes
    group.sample_size(10);

    for count in [100, 500, 1000] {
        let (conn, query_emb, query_text) = setup_hybrid_bench_db(count);

        // Benchmark 1: vec0 KNN search (vector similarity)
        group.bench_with_input(
            BenchmarkId::new("vec0_knn", format!("{count}_entries")),
            &(&conn, &query_emb),
            |b, &(conn, query_emb)| {
                b.iter(|| {
                    let results =
                        vec0::vec0_search(black_box(conn), black_box(query_emb), 10, 0.3, None)
                            .unwrap();
                    black_box(results)
                });
            },
        );

        // Benchmark 2: BM25 keyword search via FTS5
        group.bench_with_input(
            BenchmarkId::new("bm25_search", format!("{count}_entries")),
            &(&conn, &query_text),
            |b, &(conn, query)| {
                b.iter(|| {
                    let results = bm25_search(black_box(conn), black_box(query), 10);
                    black_box(results)
                });
            },
        );

        // Benchmark 3: RRF fusion of vec0 + BM25 results
        // Pre-compute result sets for the fusion benchmark
        let vec0_results: Vec<(String, f32)> = vec0::vec0_search(&conn, &query_emb, 10, 0.3, None)
            .unwrap()
            .into_iter()
            .map(|r| (r.memory_id, r.similarity))
            .collect();
        let bm25_results = bm25_search(&conn, &query_text, 10);

        group.bench_with_input(
            BenchmarkId::new("rrf_fusion", format!("{count}_entries")),
            &(&vec0_results, &bm25_results),
            |b, &(vec0_res, bm25_res)| {
                b.iter(|| {
                    let fused = reciprocal_rank_fusion(black_box(vec0_res), black_box(bm25_res));
                    black_box(fused)
                });
            },
        );

        // Benchmark 4: Combined synchronous pipeline (vec0 + BM25 + RRF)
        // This measures the full synchronous hot path without ONNX embedding
        group.bench_with_input(
            BenchmarkId::new("sync_pipeline", format!("{count}_entries")),
            &(&conn, &query_emb, &query_text),
            |b, &(conn, query_emb, query_text)| {
                b.iter(|| {
                    // Step 1: vec0 KNN
                    let vec0_res: Vec<(String, f32)> =
                        vec0::vec0_search(black_box(conn), black_box(query_emb), 10, 0.3, None)
                            .unwrap()
                            .into_iter()
                            .map(|r| (r.memory_id, r.similarity))
                            .collect();

                    // Step 2: BM25
                    let bm25_res = bm25_search(black_box(conn), black_box(query_text), 10);

                    // Step 3: RRF fusion
                    let fused = reciprocal_rank_fusion(&vec0_res, &bm25_res);
                    black_box(fused)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_hybrid_pipeline);
criterion_main!(benches);
