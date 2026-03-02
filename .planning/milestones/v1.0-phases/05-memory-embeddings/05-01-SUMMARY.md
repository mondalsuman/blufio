---
phase: 05-memory-embeddings
plan: 01
type: summary
status: complete
commit: retroactive
duration: ~20min
tests_added: 8
tests_total: 280
---

# Plan 05-01 Summary: ONNX Embedder, Memory Store, Model Manager

**Retroactive: created during Phase 12 verification**

## What was built

Created the `blufio-memory` crate with the foundational components for the memory system: local ONNX embedding inference, SQLite-backed memory storage with FTS5 search, and a model manager for downloading the embedding model on first run.

### Changes

1. **OnnxEmbedder** (`crates/blufio-memory/src/embedder.rs`)
   - `OnnxEmbedder::new(model_path)` loads an ONNX model via `ort::InferenceSession`
   - `embed()` tokenizes input text and runs inference locally, producing 384-dimensional f32 vectors
   - Implements `EmbeddingAdapter` trait from `blufio-core`
   - Zero external API calls -- fully offline after model download

2. **MemoryStore** (`crates/blufio-memory/src/store.rs`)
   - SQLite-backed storage for memory entries with embedding BLOBs
   - `insert_memory()` stores content + embedding vector
   - `search_vector()` computes cosine similarity against stored embeddings
   - `search_bm25()` uses FTS5 virtual table with BM25 ranking
   - FTS5 table kept in sync via SQLite triggers

3. **ModelManager** (`crates/blufio-memory/src/model_manager.rs`)
   - Downloads the all-MiniLM-L6-v2 ONNX model on first run
   - `ensure_model()` checks for existing model file, downloads if missing
   - Stores model in data directory alongside the database

4. **V3 Migration** (`crates/blufio-storage/migrations/V3__memory_tables.sql`)
   - Creates `memories` table with content, embedding (BLOB), source, confidence, timestamps
   - Creates FTS5 virtual table for keyword search
   - Sync triggers for FTS5

5. **Config** (`crates/blufio-config/src/model.rs`)
   - Added `MemoryConfig` with `enabled`, `top_k`, `min_score`, `extraction_model` fields

### Key design decisions

- **ONNX Runtime via `ort` crate**: Provides fast local inference without Python/TensorFlow dependency
- **Embedding as BLOB**: Stored directly in SQLite for simplicity; cosine similarity computed in application layer
- **FTS5 for keyword search**: SQLite's built-in full-text search enables hybrid retrieval without external search infrastructure

## Verification

- `cargo build` compiles cleanly with `ort` dependency
- `cargo test --workspace` passes (OnnxEmbedder produces non-zero 384-dim vectors, MemoryStore roundtrip insert/search)
