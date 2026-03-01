# Phase 5: Memory & Embeddings - Research

**Researched:** 2026-03-01
**Domain:** Local ONNX embedding inference, hybrid vector+BM25 search, long-term memory system
**Confidence:** HIGH

## Summary

Phase 5 adds a long-term memory system to Blufio that extracts facts from conversations, stores them with vector embeddings in SQLite, and retrieves relevant memories via hybrid search (cosine similarity + BM25) at context-assembly time. The system runs fully offline using a local ONNX model for embedding inference.

The core stack is: `ort` (ONNX Runtime wrapper for Rust) for embedding inference, `all-MiniLM-L6-v2` INT8 quantized model (~23MB) for embeddings, HuggingFace `tokenizers` crate for text tokenization, SQLite FTS5 for BM25 keyword search, and Reciprocal Rank Fusion (RRF) to merge vector and keyword results. Memory extraction uses a Haiku LLM call at end-of-conversation, and memories integrate into the context engine via the existing `ConditionalProvider` trait.

**Primary recommendation:** Create a new `blufio-memory` workspace crate containing the ONNX embedding adapter, memory store (SQLite tables + FTS5 virtual table), hybrid retriever (vector + BM25 with RRF fusion), extraction pipeline, and a `MemoryProvider` implementing `ConditionalProvider`. Use the existing `blufio-storage` migration pattern (refinery) for the V3 memory schema.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Hybrid extraction: automatic LLM-based extraction + explicit user flags ("remember this")
- Automatic extraction runs at end of conversation -- batch all turns when session goes idle (5+ minutes silence)
- Explicit user flags get higher confidence score than auto-extracted facts
- Extract everything meaningful: personal facts, preferences, project context, decisions made, task outcomes, instructions given
- Single-user scope -- no user-ID partitioning, all memories belong to one user
- Dynamic threshold-based retrieval -- load all memories above a semantic similarity threshold, no fixed cap
- Memories injected as structured block in the conditional zone via ConditionalProvider: "## Relevant Memories\n- [fact 1]\n- [fact 2]"
- Seamless usage -- agent uses memories naturally without calling them out
- Hybrid search combines vector similarity and BM25 keyword matching
- First-run download -- binary ships without model, auto-downloads from HuggingFace on first run, cached in data directory
- CPU-only ONNX runtime -- no GPU support needed (target is $4/month VPS)
- Explicit forget command -- user can say "forget that X", agent searches and soft-deletes matching memories (audit trail preserved)
- Newer wins for contradictions -- "my dog is Max" then "my dog is Luna" supersedes the older memory (old marked superseded)
- Source-based confidence scoring -- explicit user flags get high confidence, LLM-extracted facts get medium, affects retrieval ranking
- No time-based decay -- memories persist indefinitely unless explicitly forgotten or superseded

### Claude's Discretion
- ONNX model selection (balance quality vs 50-80MB idle memory target from CORE-07)
- Quantized INT8 vs full FP32 precision (optimize for VPS deployment)
- Hybrid search fusion strategy (RRF vs weighted linear combination vs other)
- Similarity threshold tuning for retrieval
- Extraction prompt design (what Haiku prompt extracts facts from conversation)
- Memory deduplication strategy during extraction
- BM25 tokenization approach within SQLite

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| MEM-01 | Memory system stores and retrieves long-term facts using hybrid search (vector + BM25) | ort + SQLite FTS5 + RRF fusion; memory store with vector column + FTS5 virtual table |
| MEM-02 | Local ONNX embedding model runs inference without external API calls | ort 2.0.0-rc.11 with CPU execution provider, all-MiniLM-L6-v2 INT8 quantized model |
| MEM-03 | Context engine loads only relevant memories per-turn based on semantic similarity | MemoryProvider implements ConditionalProvider, dynamic threshold filtering |
| MEM-05 | Memory embeddings stored in SQLite with efficient cosine similarity search | 384-dim f32 vectors stored as BLOB in SQLite, cosine similarity computed in Rust |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ort | 2.0.0-rc.11 | ONNX Runtime inference for embedding model | De facto Rust ONNX wrapper, HIGH reputation, 198 code snippets on Context7, actively maintained by pyke.io |
| tokenizers | 0.21 | HuggingFace tokenizer for text preprocessing | Official HuggingFace Rust crate, same tokenizer used by sentence-transformers, blazing fast |
| ndarray | 0.16 | N-dimensional array operations for vectors | Standard Rust scientific computing, ort uses it for tensor I/O |
| rusqlite | 0.37 (existing) | SQLite FTS5 virtual table for BM25 search | Already in workspace, FTS5 bundled with SQLite |
| tokio-rusqlite | 0.7 (existing) | Async SQLite operations | Already in workspace for single-writer pattern |
| refinery | 0.9 (existing) | Embedded DB migrations | Already in workspace, V3 migration for memory tables |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| reqwest | 0.12 (existing) | Download ONNX model from HuggingFace on first run | Only on first-run model download |
| serde_json | 1 (existing) | Serialize/deserialize memory metadata | Memory metadata, extraction results |
| uuid | 1 (existing) | Generate memory IDs | Each memory fact gets a UUID |
| chrono | 0.4 (existing) | Timestamps for memory creation/supersession | Already in workspace |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| ort (ONNX Runtime) | candle (pure Rust ML) | candle avoids C++ dependency but less mature for production embedding inference; ort wraps battle-tested ONNX Runtime |
| all-MiniLM-L6-v2 | bge-small-en-v1.5 | bge-small scores slightly higher on MTEB but larger; MiniLM is proven, 384-dim, 23MB INT8, ideal for memory-constrained VPS |
| SQLite FTS5 BM25 | tantivy (Rust search engine) | tantivy is more powerful but adds heavy dependency; FTS5 is built into SQLite (already bundled), zero additional deps |
| RRF fusion | Weighted linear combination | RRF is simpler, rank-based (no score normalization needed), well-proven in Azure AI Search and Weaviate |

### Embedding Model Choice: all-MiniLM-L6-v2 INT8

**Recommendation:** Use `all-MiniLM-L6-v2` with INT8 quantization.

| Property | Value |
|----------|-------|
| Model | all-MiniLM-L6-v2 |
| Quantization | INT8 |
| File size | ~23 MB |
| Dimensions | 384 |
| Max sequence length | 256 tokens |
| Runtime memory | ~30-40 MB (within CORE-07 budget) |
| Source | HuggingFace: `onnx-community/all-MiniLM-L6-v2-ONNX` |
| Inference speed | ~15ms per embedding on CPU |

**Why INT8 over FP32:**
- FP32 model is ~90 MB, INT8 is ~23 MB (75% smaller)
- INT8 fits within the 50-80MB idle memory target (CORE-07) when combined with other process memory
- Quality degradation is minimal for semantic similarity (not generative)
- CPU inference is actually faster with INT8 quantization

**Installation:** Dependencies added to workspace Cargo.toml:
```toml
ort = { version = "=2.0.0-rc.11", default-features = false }
tokenizers = { version = "0.21", default-features = false, features = ["onig"] }
ndarray = "0.16"
```

## Architecture Patterns

### Recommended Crate Structure
```
crates/blufio-memory/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API, re-exports
│   ├── embedder.rs          # OnnxEmbedder: EmbeddingAdapter impl
│   ├── store.rs             # MemoryStore: CRUD + vector/FTS5 ops
│   ├── retriever.rs         # HybridRetriever: vector + BM25 + RRF
│   ├── extractor.rs         # MemoryExtractor: LLM-based fact extraction
│   ├── provider.rs          # MemoryProvider: ConditionalProvider impl
│   ├── model_manager.rs     # First-run download, model path management
│   └── types.rs             # Memory, MemorySource, MemoryStatus types
```

### Pattern 1: ONNX Embedding Inference
**What:** Load ONNX model, tokenize text, run inference, get 384-dim vector
**When to use:** Every time a memory is stored or a query needs embedding

```rust
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::{inputs, value::TensorRef};
use ndarray::Array2;

pub struct OnnxEmbedder {
    session: Session,
    tokenizer: tokenizers::Tokenizer,
}

impl OnnxEmbedder {
    pub fn new(model_path: &Path) -> Result<Self, BlufioError> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(1)?  // Single thread for VPS
            .commit_from_file(model_path)?;
        let tokenizer_path = model_path.parent().unwrap().join("tokenizer.json");
        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| BlufioError::Internal(e.to_string()))?;
        Ok(Self { session, tokenizer })
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>, BlufioError> {
        let encoding = self.tokenizer.encode(text, true)
            .map_err(|e| BlufioError::Internal(e.to_string()))?;
        let input_ids = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();
        // Create tensors and run inference...
        // Mean pooling over token embeddings, then L2 normalize
    }
}
```

### Pattern 2: SQLite Vector Storage + FTS5
**What:** Store 384-dim f32 vectors as BLOB, with parallel FTS5 table for BM25
**When to use:** Memory persistence and retrieval

```sql
-- V3 migration: Memory tables
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY NOT NULL,
    content TEXT NOT NULL,
    embedding BLOB NOT NULL,  -- 384 x f32 = 1536 bytes
    source TEXT NOT NULL,      -- 'explicit' | 'extracted'
    confidence REAL NOT NULL DEFAULT 0.5,
    status TEXT NOT NULL DEFAULT 'active',  -- 'active' | 'superseded' | 'forgotten'
    superseded_by TEXT,
    session_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- FTS5 virtual table for BM25 keyword search
CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    content,
    content='memories',
    content_rowid='rowid'
);

-- Triggers to keep FTS5 in sync
CREATE TRIGGER memories_ai AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE TRIGGER memories_ad AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content)
        VALUES('delete', old.rowid, old.content);
END;

CREATE TRIGGER memories_au AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts, rowid, content)
        VALUES('delete', old.rowid, old.content);
    INSERT INTO memories_fts(rowid, content) VALUES (new.rowid, new.content);
END;

CREATE INDEX IF NOT EXISTS idx_memories_status ON memories(status);
CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);
```

### Pattern 3: Reciprocal Rank Fusion (RRF)
**What:** Merge vector similarity and BM25 results into a single ranked list
**When to use:** Every memory retrieval query

```rust
/// RRF score: sum of 1/(k + rank) across all lists
/// k=60 is the standard constant (from research literature)
fn reciprocal_rank_fusion(
    vector_results: &[(String, f32)],  // (memory_id, cosine_sim)
    bm25_results: &[(String, f64)],     // (memory_id, bm25_score)
    k: f32,
) -> Vec<(String, f32)> {
    let mut scores: HashMap<String, f32> = HashMap::new();

    for (rank, (id, _)) in vector_results.iter().enumerate() {
        *scores.entry(id.clone()).or_default() += 1.0 / (k + rank as f32 + 1.0);
    }
    for (rank, (id, _)) in bm25_results.iter().enumerate() {
        *scores.entry(id.clone()).or_default() += 1.0 / (k + rank as f32 + 1.0);
    }

    let mut fused: Vec<_> = scores.into_iter().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    fused
}
```

### Pattern 4: Memory Extraction via LLM
**What:** End-of-conversation batch extraction using Haiku
**When to use:** When session goes idle (5+ minutes silence)

```rust
const EXTRACTION_PROMPT: &str = r#"Extract factual information from this conversation that would be useful to remember for future conversations. Output as JSON array.

For each fact:
- "content": The fact as a standalone statement (e.g., "The user's dog is named Max")
- "category": One of: personal, preference, project, decision, instruction, outcome

Only include facts that are:
1. Stated by the user (not the assistant)
2. Specific and factual (not opinions unless explicitly stated as preferences)
3. Likely to be relevant in future conversations

If no memorable facts, return an empty array: []

Conversation:
{conversation}

Output JSON array only, no explanation:"#;
```

### Anti-Patterns to Avoid
- **Storing raw conversation text as memory:** Extract discrete facts, not conversation chunks. Raw text leads to poor retrieval and token waste.
- **Per-turn extraction:** Batching at end-of-conversation gives better context for extraction and reduces Haiku calls from N to 1.
- **Storing embeddings in a separate file/DB:** Keep everything in SQLite. Separate vector stores add complexity with no benefit at this scale.
- **Using cosine similarity in SQL:** SQLite has no native vector operations. Compute cosine similarity in Rust after loading candidate vectors, not in SQL queries.
- **FTS5 without content sync triggers:** The FTS5 content table must be kept in sync with the main table via triggers, or BM25 results will be stale.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Text tokenization for ONNX model | Custom tokenizer | HuggingFace `tokenizers` crate | WordPiece/BPE tokenization has dozens of edge cases; tokenizer.json from the model repo is the single source of truth |
| ONNX inference | Direct ONNX C API bindings | `ort` crate | ort handles session management, memory allocation, thread pool, execution providers -- all with safe Rust API |
| BM25 scoring | Custom BM25 implementation | SQLite FTS5 `bm25()` function | FTS5's BM25 is battle-tested, handles tokenization, stemming, and incremental updates automatically |
| Vector similarity ranking + BM25 fusion | Custom scoring formula | RRF (k=60) | RRF is rank-based (no score normalization needed), proven in production at Azure/Weaviate/OpenSearch |
| Model download | Custom HTTP download logic | reqwest with progress tracking | reqwest already in workspace; just need a simple GET + write-to-file with progress |

**Key insight:** The memory system's value is in the extraction quality and integration UX, not in building custom search infrastructure. Use proven components and focus effort on the extraction prompt and seamless context injection.

## Common Pitfalls

### Pitfall 1: Mean Pooling Without Attention Mask
**What goes wrong:** Using all token embeddings (including padding) in mean pooling produces degraded vectors.
**Why it happens:** Sentence-transformer models output per-token embeddings. You must mask padding tokens before averaging.
**How to avoid:** Apply attention mask before mean pooling:
```rust
// Correct: attention-masked mean pooling
let masked = token_embeddings * attention_mask_expanded;
let sum = masked.sum_axis(Axis(1));
let count = attention_mask_expanded.sum_axis(Axis(1)).mapv(|v| v.max(1e-9));
let mean_pooled = sum / count;
```
**Warning signs:** All embeddings look similar; retrieval returns random results.

### Pitfall 2: ONNX Model Thread Contention
**What goes wrong:** Multiple concurrent embedding calls block each other or crash.
**Why it happens:** ONNX Session is not `Send + Sync` by default in some configurations.
**How to avoid:** Use `with_intra_threads(1)` for VPS (single-core optimization). Wrap Session access behind a mutex or use a dedicated embedding thread.
**Warning signs:** Deadlocks during concurrent retrieval, high CPU with low throughput.

### Pitfall 3: FTS5 Content Table Desync
**What goes wrong:** BM25 search returns results that don't exist in the main table, or misses new memories.
**Why it happens:** FTS5 external content tables require explicit sync via triggers; INSERT/UPDATE/DELETE on the main table don't automatically propagate.
**How to avoid:** Use `content=` and `content_rowid=` with AFTER INSERT/UPDATE/DELETE triggers (shown in Architecture Patterns).
**Warning signs:** BM25 returns stale or missing results compared to vector search.

### Pitfall 4: Cosine Similarity Scale Misunderstanding
**What goes wrong:** Threshold of 0.9 returns nothing; threshold of 0.1 returns everything.
**Why it happens:** all-MiniLM-L6-v2 cosine similarities typically cluster in the 0.2-0.8 range for related content.
**How to avoid:** Start with threshold 0.35 for relevance filtering. Test with real conversation data and tune.
**Warning signs:** Either zero memories returned or every memory returned every turn.

### Pitfall 5: Embedding Model Download Race Condition
**What goes wrong:** Two concurrent sessions both try to download the model simultaneously.
**Why it happens:** First-run check is not atomic.
**How to avoid:** Use a file-based lock or `tokio::sync::OnceCell` for model initialization.
**Warning signs:** Corrupted model file, duplicate downloads, startup crashes.

## Code Examples

### Cosine Similarity in Rust
```rust
/// Compute cosine similarity between two vectors.
/// Assumes vectors are L2-normalized (all-MiniLM-L6-v2 outputs normalized vectors).
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Convert f32 vector to bytes for SQLite BLOB storage.
fn vec_to_blob(vec: &[f32]) -> Vec<u8> {
    vec.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Convert SQLite BLOB back to f32 vector.
fn blob_to_vec(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}
```

### FTS5 BM25 Query
```sql
-- Search memories by keyword with BM25 ranking
SELECT m.id, m.content, m.confidence, m.source,
       bm25(memories_fts) as bm25_score
FROM memories_fts
JOIN memories m ON m.rowid = memories_fts.rowid
WHERE memories_fts MATCH ?
  AND m.status = 'active'
ORDER BY bm25(memories_fts)
LIMIT 50;
```

### Model Download from HuggingFace
```rust
async fn download_model(data_dir: &Path) -> Result<PathBuf, BlufioError> {
    let model_dir = data_dir.join("models").join("all-MiniLM-L6-v2");
    let model_path = model_dir.join("model.onnx");

    if model_path.exists() {
        return Ok(model_path);
    }

    tokio::fs::create_dir_all(&model_dir).await?;

    let files = [
        ("model.onnx", "https://huggingface.co/onnx-community/all-MiniLM-L6-v2-ONNX/resolve/main/onnx/model_quantized.onnx"),
        ("tokenizer.json", "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json"),
    ];

    for (filename, url) in &files {
        let resp = reqwest::get(*url).await?;
        let bytes = resp.bytes().await?;
        tokio::fs::write(model_dir.join(filename), &bytes).await?;
    }

    Ok(model_path)
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| ort 1.x (onnxruntime crate) | ort 2.0.0-rc.11 (pykeio/ort) | 2024 | Complete API redesign, Session::builder pattern, better error handling |
| FP32 embedding models | INT8/ONNX quantized models | 2024 | 75% smaller files, faster CPU inference, minimal quality loss for similarity |
| Pure vector search | Hybrid vector + BM25 with RRF | 2024 | Better recall for both semantic and keyword queries, industry standard |
| Per-message memory extraction | End-of-conversation batch extraction | 2024 | Fewer LLM calls, better context for extraction, lower cost |

**Deprecated/outdated:**
- `onnxruntime` crate (0.0.14): Abandoned, use `ort` instead
- FP32 models for similarity tasks on constrained hardware: INT8 is now standard practice

## Open Questions

1. **ONNX Runtime Static vs Dynamic Linking on musl**
   - What we know: ort supports both static and dynamic linking. Static is preferred for musl builds.
   - What's unclear: Whether the bundled ONNX Runtime builds cleanly with musl static linking.
   - Recommendation: Start with default (downloads prebuilt ONNX Runtime), validate musl build in CI. STATE.md already flags this: "Embedding model (ONNX) performance on musl static builds not validated -- test during Phase 5".

2. **Optimal Similarity Threshold**
   - What we know: all-MiniLM-L6-v2 cosine similarities cluster 0.2-0.8 for related content.
   - What's unclear: Exact threshold for "relevant enough to inject into context" for personal memory facts.
   - Recommendation: Default to 0.35, make configurable via `MemoryConfig.similarity_threshold`. Tune based on testing.

3. **Extraction Prompt Robustness**
   - What we know: Haiku is fast and cheap for structured extraction.
   - What's unclear: Edge cases in extraction (sarcasm, hypotheticals, corrections).
   - Recommendation: Start with conservative prompt that errs on extraction over omission. Confidence scoring handles noise.

## Sources

### Primary (HIGH confidence)
- Context7 `/pykeio/ort` - Session creation, inference API, linking strategies, execution providers
- [ort crates.io](https://crates.io/crates/ort/2.0.0-rc.9) - Version 2.0.0-rc.11, released Jan 7 2026
- [SQLite FTS5 Extension](https://sqlite.org/fts5.html) - BM25 scoring, content sync, virtual table API
- [HuggingFace all-MiniLM-L6-v2](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2) - Model specs, 384 dimensions
- [onnx-community/all-MiniLM-L6-v2-ONNX](https://huggingface.co/onnx-community/all-MiniLM-L6-v2-ONNX) - INT8 quantized ONNX model files

### Secondary (MEDIUM confidence)
- [Azure AI Search RRF](https://learn.microsoft.com/en-us/azure/search/hybrid-search-ranking) - RRF algorithm, k=60 constant
- [Weaviate Hybrid Search](https://weaviate.io/blog/hybrid-search-explained) - Hybrid search architecture patterns
- [Xenova/all-MiniLM-L6-v2](https://huggingface.co/Xenova/all-MiniLM-L6-v2/tree/main/onnx) - ONNX model variants and sizes
- [Building Sentence Transformers in Rust](https://dev.to/mayu2008/building-sentence-transformers-in-rust-a-practical-guide-with-burn-onnx-runtime-and-candle-281k) - ort + tokenizers integration pattern

### Tertiary (LOW confidence)
- None -- all findings verified with primary or secondary sources

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - ort and tokenizers are well-documented, Context7 has extensive coverage
- Architecture: HIGH - patterns follow established hybrid search implementations (Azure, Weaviate)
- Pitfalls: HIGH - documented in official sources and community experience
- Model choice: HIGH - all-MiniLM-L6-v2 INT8 specs verified on HuggingFace

**Research date:** 2026-03-01
**Valid until:** 2026-04-01 (30 days -- stack is stable)
