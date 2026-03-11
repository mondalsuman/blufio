# Phase 55: Memory Enhancements - Research

**Researched:** 2026-03-11
**Domain:** Memory retrieval scoring, diversity reranking, index bounding, background validation, file watching
**Confidence:** HIGH

## Summary

Phase 55 enhances the existing `blufio-memory` crate with six capabilities: temporal decay scoring, importance boost, MMR diversity reranking, bounded index with LRU eviction, background validation (duplicates/stale/conflicts), and a file watcher for auto-indexing. All decisions are locked in CONTEXT.md with precise formulas, thresholds, and architecture choices.

The existing codebase provides a strong foundation. `HybridRetriever::retrieve()` already has a scoring step (Step 6) where decay and importance boost plug in naturally. `cosine_similarity()` in types.rs is reused by MMR. `MemoryStore` already emits events via `Optional<Arc<EventBus>>`. The `DEDUP_THRESHOLD` (0.9) and `CONTRADICTION_THRESHOLD` (0.7) constants in extractor.rs are reused by validation. `MemoryConfig` in blufio-config uses `#[serde(deny_unknown_fields)]` and needs new fields. The `notify` crate v8.2.0 (stable) with `notify-debouncer-mini` v0.7.0 provides cross-platform file watching with configurable debounce.

**Primary recommendation:** Implement in dependency order -- (1) config extensions, (2) scoring pipeline changes (decay + importance + MMR), (3) eviction, (4) validation, (5) file watcher, (6) CLI + metrics. Each layer builds on the prior, and the config must come first since everything reads from it.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Scoring Formula
- Multiplicative composition: `final_score = rrf_score * importance_boost * decay_factor`
- Importance boost replaces confidence in the formula: Explicit=1.0 (up from 0.9), Extracted=0.6 (unchanged), FileWatcher=0.8 (new)
- Temporal decay computed in HybridRetriever during scoring step (Step 6), not in SQL
- Decay based on `created_at` timestamp, not updated_at or last_accessed
- Decay formula: `max(decay_factor^days, decay_floor)` with configurable floor (default 0.1)
- File-sourced memories skip temporal decay entirely (decay_factor = 1.0) -- file content is current by definition
- MMR diversity reranking runs as post-scoring pass on top-N results only (not full candidate set)
- MMR uses existing cosine_similarity function with standard formula: `score = lambda*relevance - (1-lambda)*max_sim_to_selected`
- MMR operates on Memory.embedding (already loaded from get_memories_by_ids)
- Full pipeline order: (1) RRF fusion, (2) fetch memories, (3) apply importance_boost * decay, (4) sort by combined score, (5) MMR rerank top-N
- All parameters TOML configurable in [memory] section: decay_factor (default 0.95), decay_floor (default 0.1), mmr_lambda (default 0.7), importance_boost_explicit (default 1.0), importance_boost_extracted (default 0.6), importance_boost_file (default 0.8)

#### LRU Eviction Policy
- Eviction by composite score: lowest importance_boost * decay_factor entries evicted first
- Background sweep every 5 minutes via independent tokio::spawn timer in blufio-memory
- Hard DELETE from SQLite -- eviction bounds storage, audit trail records the event
- Batch eviction: when count > max_entries, evict down to 90% of max_entries (provides headroom)
- Only evict active memories -- superseded/forgotten handled by retention policies (Phase 58)
- max_entries config (default 10,000) counts active memories only
- Single bulk MemoryEvent::Evicted { count, lowest_score, highest_score } per sweep (not per-memory)
- Eviction sweep interval configurable via TOML (default 300 seconds)

#### Background Validation
- Duplicate detection reuses DEDUP_THRESHOLD (0.9 cosine similarity) from extractor.rs
- Conflict detection reuses CONTRADICTION_THRESHOLD (0.7 similarity), newer supersedes older
- Staleness: age-based -- memories older than configurable threshold (default 180 days) with decay score at floor
- Auto-resolve: duplicates superseded (keep higher-confidence), conflicts resolved (newer wins), all logged to audit trail
- Brute-force O(n^2) pairwise comparison -- acceptable at 10K cap, runs daily
- Daily schedule only, no startup scan (operators use CLI for immediate validation)
- Single background tokio task handles both eviction (5 min) and validation (daily)
- CLI: `blufio memory validate` with --dry-run for preview, --json output
- Prometheus metrics: blufio_memory_validation_duplicates_total, blufio_memory_validation_stale_total, blufio_memory_validation_conflicts_total, blufio_memory_active_count (gauge)

#### File Watcher
- Configurable watch paths via [memory.file_watcher] TOML section: `paths = ["./docs"]`, `extensions = ["md", "txt"]`
- Disabled until configured (no default paths)
- notify crate for cross-platform file watching with 500ms debounce
- One memory per file -- content becomes a single memory entry
- New MemorySource::FileWatcher variant to distinguish from explicit/extracted
- Memory ID: `file:` + SHA-256(canonical_path) -- stable, deterministic
- File path stored in session_id field (reuses existing nullable field, no schema change)
- Max file size: configurable (default 100KB), skip larger files with warning
- File deletion: soft-delete corresponding memory (status='forgotten')
- Initial full scan on startup + watch for changes after
- File-sourced memories skip temporal decay (always decay_factor = 1.0)

### Claude's Discretion
- Exact notify crate version and event debouncing implementation
- Internal module organization for new validation/eviction/watcher code
- Exact Prometheus metric label values beyond what's specified
- Test fixture organization and edge case selection
- Migration version numbering
- Background task shutdown coordination details
- Exact SQL queries for eviction scoring and batch deletion

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| MEME-01 | Temporal decay applies configurable decay factor (default 0.95^days) to retrieval scores | Scoring pipeline modification in retriever.rs Step 6; chrono 0.4 for date parsing; MemoryConfig extension |
| MEME-02 | Importance boost multiplier distinguishes explicit memories (1.0) from extracted memories (0.6) | Replaces existing confidence-based boost in retriever.rs; new MemorySource::FileWatcher variant at 0.8 |
| MEME-03 | MMR diversity reranking reduces redundant results using lambda-weighted relevance vs. similarity penalty | Post-scoring pass using existing cosine_similarity(); standard greedy MMR algorithm on top-N |
| MEME-04 | Bounded memory index with configurable max entries (default 10,000) and LRU eviction of lowest-scored entries | New store methods for count_active + batch_delete_lowest; background tokio task; MemoryEvent::Evicted changes to bulk format |
| MEME-05 | Background memory validation detects duplicates, stale entries, and conflicts on configurable interval | Reuses DEDUP_THRESHOLD/CONTRADICTION_THRESHOLD from extractor.rs; brute-force O(n^2) pairwise; combined background task with eviction |
| MEME-06 | File watcher auto re-indexes workspace files on change with 500ms debounce | notify 8.2.0 + notify-debouncer-mini 0.7.0; new MemorySource::FileWatcher; SHA-256 deterministic IDs |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| notify | 8.2.0 | Cross-platform filesystem notification | De facto standard for Rust file watching; used by rust-analyzer, cargo-watch, deno |
| notify-debouncer-mini | 0.7.0 | Event debouncing for notify | Official companion crate; provides 500ms debounce with simple API |
| chrono | 0.4 (workspace) | Date/time parsing and arithmetic for decay | Already in workspace; needed for days-since-created calculation |
| sha2 | 0.10 (workspace) | SHA-256 for deterministic file memory IDs | Already in workspace; `file:` + SHA-256(canonical_path) |
| tokio | 1 (workspace) | Background task scheduling (eviction + validation) | Already in workspace; tokio::spawn + interval timers |
| tokio-rusqlite | 0.7 (workspace) | Async SQLite operations for new store methods | Already in workspace; existing pattern |
| metrics | (workspace via blufio-prometheus) | Prometheus metric emission | Already in workspace; existing describe/gauge/counter pattern |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| rusqlite | 0.37 (workspace) | Raw SQL for eviction/validation queries | Batch DELETE, COUNT queries in store.rs |
| tracing | (workspace) | Structured logging for background tasks | Log eviction/validation activity |
| serde | (workspace) | Config deserialization for new fields | MemoryConfig extension |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| notify-debouncer-mini | notify-debouncer-full | Full debouncer tracks rename chains -- overkill for our use case (we just need path + change event) |
| notify 8.2.0 | notify 9.0.0-rc.2 | 9.0 is release candidate, not stable; 8.2.0 is battle-tested and sufficient |
| Brute-force O(n^2) validation | HNSW index | 10K cap makes O(n^2) tractable (~50M comparisons); HNSW adds dependency and complexity |

**Installation:**
```bash
# Add to crates/blufio-memory/Cargo.toml [dependencies]
notify = "8.2"
notify-debouncer-mini = "0.7"
# sha2, chrono, tokio already in workspace
```

## Architecture Patterns

### Recommended Module Organization
```
crates/blufio-memory/src/
  embedder.rs           # Existing -- ONNX embedder
  extractor.rs          # Existing -- LLM extraction (exports DEDUP_THRESHOLD, CONTRADICTION_THRESHOLD)
  lib.rs                # Existing -- add pub mod for new modules
  model_manager.rs      # Existing -- HuggingFace model download
  provider.rs           # Existing -- ConditionalProvider for context injection
  retriever.rs          # MODIFIED -- scoring pipeline: decay + importance + MMR
  store.rs              # MODIFIED -- add count_active, batch_evict, get_all_active_with_embeddings
  types.rs              # MODIFIED -- add MemorySource::FileWatcher
  eviction.rs           # NEW -- background eviction sweep logic
  validation.rs         # NEW -- duplicate/stale/conflict detection + resolution
  watcher.rs            # NEW -- file watcher with notify + debounce
  background.rs         # NEW -- combined tokio task for eviction (5min) + validation (daily)
```

### Pattern 1: Scoring Pipeline (retriever.rs Step 6 Modification)

**What:** Replace current `rrf_score * memory.confidence` with `rrf_score * importance_boost * decay_factor`

**When to use:** Every memory retrieval call

**Current code (retriever.rs:95-104):**
```rust
// Step 6: Apply confidence boost and build ScoredMemory
let mut scored: Vec<ScoredMemory> = memories
    .into_iter()
    .map(|memory| {
        let rrf_score = score_map.get(memory.id.as_str()).copied().unwrap_or(0.0);
        let boosted_score = rrf_score * memory.confidence as f32;
        ScoredMemory { memory, score: boosted_score }
    })
    .collect();
```

**New code pattern:**
```rust
// Step 6: Apply importance boost + temporal decay
let now = chrono::Utc::now();
let mut scored: Vec<ScoredMemory> = memories
    .into_iter()
    .map(|memory| {
        let rrf_score = score_map.get(memory.id.as_str()).copied().unwrap_or(0.0);
        let importance = importance_boost_for_source(&memory.source, &self.config);
        let decay = temporal_decay(&memory, now, &self.config);
        let final_score = rrf_score * importance * decay;
        ScoredMemory { memory, score: final_score }
    })
    .collect();

// Step 7: Sort by combined score descending
scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

// Step 8: MMR diversity reranking on top-N
let result = mmr_rerank(&scored, self.config.mmr_lambda, self.config.max_retrieval_results);
```

### Pattern 2: Temporal Decay Function

**What:** Exponential decay with floor, skip for file-sourced memories

```rust
fn temporal_decay(memory: &Memory, now: chrono::DateTime<chrono::Utc>, config: &MemoryConfig) -> f32 {
    // File-sourced memories skip decay entirely
    if memory.source == MemorySource::FileWatcher {
        return 1.0;
    }
    let created = chrono::DateTime::parse_from_rfc3339(&memory.created_at)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or(now); // fallback: no decay if unparseable
    let days = (now - created).num_days().max(0) as f32;
    (config.decay_factor.powf(days) as f32).max(config.decay_floor as f32)
}

fn importance_boost_for_source(source: &MemorySource, config: &MemoryConfig) -> f32 {
    match source {
        MemorySource::Explicit => config.importance_boost_explicit as f32,
        MemorySource::Extracted => config.importance_boost_extracted as f32,
        MemorySource::FileWatcher => config.importance_boost_file as f32,
    }
}
```

### Pattern 3: MMR Greedy Reranking

**What:** Iteratively select most relevant + most diverse results

```rust
fn mmr_rerank(scored: &[ScoredMemory], lambda: f64, k: usize) -> Vec<ScoredMemory> {
    if scored.is_empty() || k == 0 {
        return vec![];
    }
    let lambda = lambda as f32;
    let mut selected: Vec<usize> = Vec::with_capacity(k);
    let mut remaining: Vec<usize> = (0..scored.len()).collect();

    // First: highest scoring document
    selected.push(remaining.remove(0)); // scored is already sorted desc

    while selected.len() < k && !remaining.is_empty() {
        let mut best_idx = 0;
        let mut best_mmr = f32::NEG_INFINITY;

        for (ri, &ci) in remaining.iter().enumerate() {
            let relevance = scored[ci].score;
            let max_sim = selected.iter()
                .map(|&si| cosine_similarity(&scored[ci].memory.embedding, &scored[si].memory.embedding))
                .fold(f32::NEG_INFINITY, f32::max);
            let mmr_score = lambda * relevance - (1.0 - lambda) * max_sim;
            if mmr_score > best_mmr {
                best_mmr = mmr_score;
                best_idx = ri;
            }
        }

        selected.push(remaining.remove(best_idx));
    }

    selected.into_iter().map(|i| scored[i].clone()).collect()
}
```

### Pattern 4: Combined Background Task

**What:** Single tokio::spawn manages eviction (5 min) and validation (daily)

```rust
pub async fn spawn_background_task(
    store: Arc<MemoryStore>,
    config: MemoryConfig,
    event_bus: Option<Arc<EventBus>>,
    cancel: CancellationToken,
) {
    let eviction_interval = Duration::from_secs(config.eviction_sweep_interval_secs);
    let mut eviction_timer = tokio::time::interval(eviction_interval);
    let mut validation_timer = tokio::time::interval(Duration::from_secs(86400)); // daily

    // Skip first immediate tick
    eviction_timer.tick().await;
    validation_timer.tick().await;

    loop {
        tokio::select! {
            _ = eviction_timer.tick() => {
                if let Err(e) = run_eviction_sweep(&store, &config, &event_bus).await {
                    tracing::warn!(error = %e, "eviction sweep failed");
                }
            }
            _ = validation_timer.tick() => {
                if let Err(e) = run_validation(&store, &config, &event_bus).await {
                    tracing::warn!(error = %e, "validation sweep failed");
                }
            }
            _ = cancel.cancelled() => {
                tracing::info!("memory background task shutting down");
                break;
            }
        }
    }
}
```

### Pattern 5: File Watcher with Debounce

**What:** notify + debouncer-mini watching configured paths, bridged to tokio via mpsc

```rust
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};
use std::time::Duration;

pub fn start_file_watcher(
    config: &FileWatcherConfig,
    store: Arc<MemoryStore>,
    embedder: Arc<OnnxEmbedder>,
    cancel: CancellationToken,
) -> Result<(), BlufioError> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<PathBuf>>(100);

    // notify runs on its own thread; bridge to tokio via mpsc
    let mut debouncer = new_debouncer(
        Duration::from_millis(500),
        move |res: DebounceEventResult| {
            if let Ok(events) = res {
                let paths: Vec<PathBuf> = events.into_iter().map(|e| e.path).collect();
                let _ = tx.blocking_send(paths);
            }
        },
    ).map_err(|e| BlufioError::Internal(format!("file watcher init failed: {e}")))?;

    for path in &config.paths {
        debouncer.watcher().watch(Path::new(path), RecursiveMode::Recursive)?;
    }

    // Process events in tokio task
    tokio::spawn(async move {
        let _debouncer = debouncer; // keep alive
        loop {
            tokio::select! {
                Some(paths) = rx.recv() => {
                    for path in paths {
                        process_file_change(&path, &config, &store, &embedder).await;
                    }
                }
                _ = cancel.cancelled() => break,
            }
        }
    });

    Ok(())
}
```

### Anti-Patterns to Avoid
- **Computing decay in SQL:** The CONTEXT.md explicitly states decay is computed in HybridRetriever Step 6, not in SQL. This keeps the scoring logic testable and avoids SQLite expression complexity.
- **Per-memory eviction events:** CONTEXT.md specifies a single bulk `MemoryEvent::Evicted { count, lowest_score, highest_score }` per sweep, not per-memory. The current Evicted variant has `memory_id: String` and `reason: String` which must be changed to bulk fields.
- **Evicting superseded/forgotten memories:** Only active memories are subject to eviction. Other statuses are handled by Phase 58 retention policies.
- **Running validation on startup:** CONTEXT.md explicitly says "no startup scan" -- operators use CLI for immediate validation.
- **Using updated_at or last_accessed for decay:** Decay is based strictly on `created_at`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| File system notifications | Custom polling loop | `notify` 8.2.0 crate | Cross-platform (inotify/FSEvents/kqueue/ReadDirectoryChanges); handles edge cases like rename chains, symlinks |
| Event debouncing | Custom timer + dedup logic | `notify-debouncer-mini` 0.7.0 | Handles rapid succession events, batching, timer management |
| SHA-256 hashing | Custom implementation | `sha2` 0.10 (workspace) | Already in workspace for audit trail hash chains |
| Date arithmetic | Manual epoch math | `chrono` 0.4 (workspace) | Already in workspace; handles timezone, RFC3339 parsing, days-between |
| Cosine similarity | New similarity function | Existing `cosine_similarity()` in types.rs | Already tested, L2-normalized dot product |

**Key insight:** The existing codebase already provides most building blocks. The new code is primarily orchestration (combining existing primitives) rather than new algorithms.

## Common Pitfalls

### Pitfall 1: MemoryEvent::Evicted Breaking Change
**What goes wrong:** The current `MemoryEvent::Evicted` has fields `{ event_id, timestamp, memory_id, reason }` (per-memory). CONTEXT.md requires `{ count, lowest_score, highest_score }` (bulk). Changing this breaks the audit subscriber in `blufio-audit/src/subscriber.rs` which pattern-matches on the old fields.
**Why it happens:** The event was pre-created for future use with a per-memory shape, but the actual design chose bulk reporting.
**How to avoid:** Update the Evicted variant fields AND update ALL consumers: `events.rs` (definition + event_type_string), `subscriber.rs` (audit mapping), and tests in both crates.
**Warning signs:** Compiler errors in subscriber.rs pattern matching; audit trail entries with wrong fields.

### Pitfall 2: Timestamp Parsing Failures in Decay
**What goes wrong:** `created_at` is stored as ISO 8601 string ("2026-03-01T00:00:00.000Z"). If parsing fails, decay calculation breaks.
**Why it happens:** Inconsistent timestamp formats (some with milliseconds, some without), or empty strings from legacy data.
**How to avoid:** Use `chrono::DateTime::parse_from_rfc3339` with a fallback: if parsing fails, treat as "no decay" (return 1.0) and log a warning. Never panic on timestamp parsing.
**Warning signs:** `unwrap()` on timestamp parsing; test fixtures with non-RFC3339 timestamps.

### Pitfall 3: File Watcher Thread vs Tokio Runtime
**What goes wrong:** The `notify` crate uses std threads internally. Calling `tokio::sync::mpsc::Sender::send()` from a non-async context panics.
**Why it happens:** `notify` callbacks run on the watcher's background thread, not the tokio runtime.
**How to avoid:** Use `tx.blocking_send()` (not `.send().await`) in the notify callback, which is designed for synchronous contexts sending to async receivers.
**Warning signs:** "cannot start a runtime from within a runtime" panics; test hangs.

### Pitfall 4: FTS5 Trigger on Hard DELETE
**What goes wrong:** Hard DELETE from memories table fires the `memories_ad` trigger which removes the FTS5 entry. If the DELETE is inside a transaction that fails, FTS5 and main table can get out of sync.
**Why it happens:** SQLite triggers are part of the transaction, but error handling may not properly roll back.
**How to avoid:** Wrap batch eviction DELETE in a single transaction. The existing FTS5 triggers (`memories_ad`) will correctly clean up. Test that FTS5 search works correctly after eviction.
**Warning signs:** FTS5 search returning results for evicted memories; "FTS content table mismatch" errors.

### Pitfall 5: Eviction Score Inconsistency
**What goes wrong:** Eviction sorts by `importance_boost * decay_factor` but retrieval sorts by `rrf_score * importance_boost * decay_factor`. The eviction scoring doesn't include RRF, so a rarely-queried but relevant memory might be evicted.
**Why it happens:** RRF is query-dependent and can't be computed for eviction without a query.
**How to avoid:** This is by design (CONTEXT.md specifies eviction by `importance_boost * decay_factor`). Document that eviction prioritizes freshness and source type, not query relevance. The 90% eviction target provides headroom.
**Warning signs:** None -- this is intentional behavior.

### Pitfall 6: O(n^2) Validation Memory Pressure
**What goes wrong:** Loading 10K memory embeddings (384 dims * 4 bytes * 10K = ~15MB) into memory for pairwise comparison.
**Why it happens:** Brute-force comparison needs all embeddings in memory simultaneously.
**How to avoid:** Load embeddings once via `get_active_embeddings()` (already exists). Process comparisons in-place without cloning. 15MB is acceptable for a daily background task. If memory pressure is a concern, process in batches of 1K.
**Warning signs:** RSS spike during validation; OOM on constrained systems.

## Code Examples

### MemoryConfig Extension (blufio-config/src/model.rs)

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryConfig {
    // ... existing fields ...

    // Scoring parameters
    #[serde(default = "default_decay_factor")]
    pub decay_factor: f64,       // default 0.95
    #[serde(default = "default_decay_floor")]
    pub decay_floor: f64,        // default 0.1
    #[serde(default = "default_mmr_lambda")]
    pub mmr_lambda: f64,         // default 0.7
    #[serde(default = "default_importance_boost_explicit")]
    pub importance_boost_explicit: f64,   // default 1.0
    #[serde(default = "default_importance_boost_extracted")]
    pub importance_boost_extracted: f64,  // default 0.6
    #[serde(default = "default_importance_boost_file")]
    pub importance_boost_file: f64,       // default 0.8

    // Eviction parameters
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,               // default 10_000
    #[serde(default = "default_eviction_sweep_interval_secs")]
    pub eviction_sweep_interval_secs: u64, // default 300

    // Validation parameters
    #[serde(default = "default_stale_threshold_days")]
    pub stale_threshold_days: u64,        // default 180

    // File watcher (nested struct, optional)
    #[serde(default)]
    pub file_watcher: FileWatcherConfig,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FileWatcherConfig {
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default = "default_max_file_size")]
    pub max_file_size: usize,  // default 102_400 (100KB)
}
```

### MemorySource::FileWatcher Variant (types.rs)

```rust
pub enum MemorySource {
    Explicit,
    Extracted,
    FileWatcher,
}

impl MemorySource {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemorySource::Explicit => "explicit",
            MemorySource::Extracted => "extracted",
            MemorySource::FileWatcher => "file_watcher",
        }
    }

    pub fn from_str_value(s: &str) -> Self {
        match s {
            "explicit" => MemorySource::Explicit,
            "file_watcher" => MemorySource::FileWatcher,
            _ => MemorySource::Extracted,
        }
    }
}
```

### MemoryEvent::Evicted Bulk Format (events.rs)

```rust
// CHANGE from current per-memory format to bulk format
Evicted {
    event_id: String,
    timestamp: String,
    count: u32,
    lowest_score: f64,
    highest_score: f64,
},
```

### New Store Methods (store.rs)

```rust
/// Count active (non-restricted) memories.
pub async fn count_active(&self) -> Result<usize, BlufioError> {
    self.conn
        .call(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE status = 'active' AND classification != 'restricted'",
                [],
                |row| row.get(0),
            )?;
            Ok(count as usize)
        })
        .await
        .map_err(storage_err)
}

/// Delete the N lowest-scored active memories by composite eviction score.
/// Returns the count actually deleted and (lowest_score, highest_score) of deleted.
pub async fn batch_evict(
    &self,
    count: usize,
    decay_factor: f64,
    decay_floor: f64,
    importance_boosts: (f64, f64, f64), // (explicit, extracted, file_watcher)
) -> Result<(usize, f64, f64), BlufioError> {
    // Implementation computes importance_boost * decay_factor per memory in SQL
    // using CASE expression on source column and date arithmetic,
    // then DELETE the lowest-scored entries
    // ...
}
```

### Prometheus Metrics (recording.rs)

```rust
// In register_metrics():
describe_counter!("blufio_memory_validation_duplicates_total", "Total duplicate memories detected by validation");
describe_counter!("blufio_memory_validation_stale_total", "Total stale memories detected by validation");
describe_counter!("blufio_memory_validation_conflicts_total", "Total conflicting memories detected by validation");
describe_gauge!("blufio_memory_active_count", "Current count of active memories");

// Helper functions:
pub fn record_validation_duplicates(count: u64) {
    metrics::counter!("blufio_memory_validation_duplicates_total").increment(count);
}
pub fn record_validation_stale(count: u64) {
    metrics::counter!("blufio_memory_validation_stale_total").increment(count);
}
pub fn record_validation_conflicts(count: u64) {
    metrics::counter!("blufio_memory_validation_conflicts_total").increment(count);
}
pub fn set_memory_active_count(count: f64) {
    metrics::gauge!("blufio_memory_active_count").set(count);
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| confidence-based boost (0.9/0.6) | importance_boost (1.0/0.6/0.8) | This phase | Explicit memories score higher; file-sourced get intermediate boost |
| No temporal decay | Exponential decay with floor | This phase | Old memories naturally deprioritized; configurable via TOML |
| No diversity guarantee | MMR reranking | This phase | Redundant results eliminated; lambda controls relevance/diversity tradeoff |
| Unbounded memory index | LRU eviction at 10K cap | This phase | Storage bounded; prevents unbounded growth |
| Per-memory MemoryEvent::Evicted | Bulk MemoryEvent::Evicted | This phase | Single event per sweep; reduces audit noise |
| notify 6.x | notify 8.2.0 | 2025 | Simpler API, better cross-platform support |

**Deprecated/outdated:**
- The `memory.confidence` field is no longer used in scoring (replaced by source-based importance_boost). The field remains in the Memory struct for backward compatibility but is not part of the scoring formula.

## Open Questions

1. **Eviction SQL: compute in SQL or Rust?**
   - What we know: CONTEXT.md says eviction score = `importance_boost * decay_factor`. SQL can compute decay via julianday arithmetic and CASE for source-based boost. Alternatively, load all active memories into Rust and compute there.
   - What's unclear: Whether tokio-rusqlite's `call()` closure can efficiently do the date arithmetic + CASE + ORDER BY + DELETE in one SQL statement.
   - Recommendation: Compute in SQL for efficiency (avoids loading 10K memories into Rust memory). Use `julianday('now') - julianday(created_at)` for days and CASE on source column for importance_boost. Falls under "Claude's Discretion" per CONTEXT.md.

2. **Migration version number**
   - What we know: Latest migration is V9. No schema change is actually needed (session_id field reused for file paths, MemorySource handled by as_str/from_str_value).
   - What's unclear: Whether any index is needed for eviction performance.
   - Recommendation: Add a V10 migration that creates `CREATE INDEX IF NOT EXISTS idx_memories_source ON memories(source)` for efficient file-sourced memory queries. Falls under "Claude's Discretion."

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in Rust test framework) |
| Config file | Cargo.toml per crate (workspace-level) |
| Quick run command | `cargo test -p blufio-memory` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| MEME-01 | Temporal decay reduces old memory scores | unit | `cargo test -p blufio-memory -- temporal_decay` | Wave 0 |
| MEME-01 | File-sourced memories skip decay | unit | `cargo test -p blufio-memory -- file_watcher_no_decay` | Wave 0 |
| MEME-01 | Decay floor prevents zero scores | unit | `cargo test -p blufio-memory -- decay_floor` | Wave 0 |
| MEME-02 | Importance boost ordering: Explicit > FileWatcher > Extracted | unit | `cargo test -p blufio-memory -- importance_boost` | Wave 0 |
| MEME-02 | Scoring formula is multiplicative | unit | `cargo test -p blufio-memory -- scoring_multiplicative` | Wave 0 |
| MEME-03 | MMR eliminates near-duplicate results | unit | `cargo test -p blufio-memory -- mmr_dedup` | Wave 0 |
| MEME-03 | MMR lambda=1.0 preserves relevance order | unit | `cargo test -p blufio-memory -- mmr_lambda_one` | Wave 0 |
| MEME-03 | MMR with empty input returns empty | unit | `cargo test -p blufio-memory -- mmr_empty` | Wave 0 |
| MEME-04 | Eviction triggers when count > max_entries | unit | `cargo test -p blufio-memory -- eviction_trigger` | Wave 0 |
| MEME-04 | Eviction evicts down to 90% of max | unit | `cargo test -p blufio-memory -- eviction_target` | Wave 0 |
| MEME-04 | Eviction skips superseded/forgotten | unit | `cargo test -p blufio-memory -- eviction_active_only` | Wave 0 |
| MEME-04 | count_active counts only active non-restricted | unit | `cargo test -p blufio-memory -- count_active` | Wave 0 |
| MEME-05 | Duplicate detection at 0.9 threshold | unit | `cargo test -p blufio-memory -- validation_duplicate` | Wave 0 |
| MEME-05 | Conflict detection at 0.7 threshold | unit | `cargo test -p blufio-memory -- validation_conflict` | Wave 0 |
| MEME-05 | Stale detection by age + decay floor | unit | `cargo test -p blufio-memory -- validation_stale` | Wave 0 |
| MEME-06 | File watcher indexes new file | unit | `cargo test -p blufio-memory -- watcher_new_file` | Wave 0 |
| MEME-06 | File deletion soft-deletes memory | unit | `cargo test -p blufio-memory -- watcher_delete` | Wave 0 |
| MEME-06 | Memory ID is deterministic SHA-256 | unit | `cargo test -p blufio-memory -- file_memory_id` | Wave 0 |
| MEME-06 | Files > max_size are skipped | unit | `cargo test -p blufio-memory -- watcher_max_size` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-memory`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- All test files listed above are new and need to be created as part of implementation
- Test fixtures for memory structs with various timestamps (for decay testing) needed
- Test fixtures for similar embeddings (for MMR and validation testing) needed
- No additional framework install needed (cargo test is built-in)

## Sources

### Primary (HIGH confidence)
- Codebase analysis: `crates/blufio-memory/src/` -- retriever.rs, store.rs, types.rs, extractor.rs, provider.rs, lib.rs
- Codebase analysis: `crates/blufio-config/src/model.rs` -- MemoryConfig struct
- Codebase analysis: `crates/blufio-bus/src/events.rs` -- MemoryEvent enum
- Codebase analysis: `crates/blufio-audit/src/subscriber.rs` -- Evicted event consumption
- Codebase analysis: `crates/blufio-prometheus/src/recording.rs` -- metric registration pattern
- Codebase analysis: `crates/blufio/src/serve.rs` -- memory initialization and background task patterns
- Codebase analysis: `crates/blufio/src/main.rs` -- CLI subcommand patterns (Audit as reference)

### Secondary (MEDIUM confidence)
- [notify-debouncer-mini docs](https://docs.rs/notify-debouncer-mini/latest/notify_debouncer_mini/) -- v0.7.0 API, new_debouncer function
- [notify crate](https://docs.rs/crate/notify/latest) -- v8.2.0 stable, RecommendedWatcher API
- [MMR Algorithm (Carbonell & Goldstein 1998)](https://www.cs.cmu.edu/~jgc/publication/The_Use_MMR_Diversity_Based_LTMIR_1998.pdf) -- Original MMR paper; formula verified

### Tertiary (LOW confidence)
- None -- all findings verified against codebase or official docs

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in workspace except notify/debouncer-mini which are well-established
- Architecture: HIGH -- patterns directly derived from existing codebase analysis
- Pitfalls: HIGH -- identified from concrete code inspection (timestamp parsing, thread/tokio bridge, FTS5 triggers, breaking event changes)

**Research date:** 2026-03-11
**Valid until:** 2026-04-11 (30 days -- stable domain, no fast-moving dependencies)
