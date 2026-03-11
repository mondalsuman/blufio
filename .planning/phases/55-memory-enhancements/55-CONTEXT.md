# Phase 55: Memory Enhancements - Context

**Gathered:** 2026-03-11
**Status:** Ready for planning

<domain>
## Phase Boundary

Memory retrieval returns the most relevant, diverse, and temporally appropriate results with bounded index size. Temporal decay penalizes old memories, importance boost differentiates explicit from extracted, MMR eliminates redundant results, LRU eviction caps the index, background validation cleans up duplicates/stale/conflicts, and file watcher auto-indexes configured paths.

</domain>

<decisions>
## Implementation Decisions

### Scoring Formula
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

### LRU Eviction Policy
- Eviction by composite score: lowest importance_boost * decay_factor entries evicted first
- Background sweep every 5 minutes via independent tokio::spawn timer in blufio-memory
- Hard DELETE from SQLite -- eviction bounds storage, audit trail records the event
- Batch eviction: when count > max_entries, evict down to 90% of max_entries (provides headroom)
- Only evict active memories -- superseded/forgotten handled by retention policies (Phase 58)
- max_entries config (default 10,000) counts active memories only
- Single bulk MemoryEvent::Evicted { count, lowest_score, highest_score } per sweep (not per-memory)
- Eviction sweep interval configurable via TOML (default 300 seconds)

### Background Validation
- Duplicate detection reuses DEDUP_THRESHOLD (0.9 cosine similarity) from extractor.rs
- Conflict detection reuses CONTRADICTION_THRESHOLD (0.7 similarity), newer supersedes older
- Staleness: age-based -- memories older than configurable threshold (default 180 days) with decay score at floor
- Auto-resolve: duplicates superseded (keep higher-confidence), conflicts resolved (newer wins), all logged to audit trail
- Brute-force O(n^2) pairwise comparison -- acceptable at 10K cap, runs daily
- Daily schedule only, no startup scan (operators use CLI for immediate validation)
- Single background tokio task handles both eviction (5 min) and validation (daily)
- CLI: `blufio memory validate` with --dry-run for preview, --json output
- Prometheus metrics: blufio_memory_validation_duplicates_total, blufio_memory_validation_stale_total, blufio_memory_validation_conflicts_total, blufio_memory_active_count (gauge)

### File Watcher
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

</decisions>

<specifics>
## Specific Ideas

- Scoring pipeline layers cleanly on existing HybridRetriever: steps 1-5 exist, steps 3-5 modified to add decay/importance/MMR
- Background task combines eviction (5 min) and validation (daily) in one tokio::spawn with two interval timers
- File watcher reuses session_id field for path storage -- pragmatic, no migration needed for this
- File memory IDs prefixed with "file:" for easy SQL filtering of file-sourced memories
- Validation CLI matches existing patterns: --dry-run from classify bulk, --json from other commands

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `HybridRetriever::retrieve()`: Scoring step (Step 6) where decay and importance boost integrate
- `cosine_similarity()` in types.rs: Reused by MMR for pairwise diversity computation
- `MemoryStore`: All CRUD methods already emit MemoryEvent to EventBus
- `DEDUP_THRESHOLD` (0.9) and `CONTRADICTION_THRESHOLD` (0.7) in extractor.rs: Reused by validation
- `MemorySource` enum with as_str()/from_str_value(): Pattern for adding FileWatcher variant
- `MemoryConfig` in blufio-config: Extended with new scoring/eviction/watcher fields

### Established Patterns
- Optional<Arc<EventBus>> for components that emit events (None in tests/CLI)
- tokio::spawn for background tasks with interval timers
- #[serde(deny_unknown_fields)] on config structs
- as_str()/from_str_value() for SQLite text column serialization
- CLI subcommands in main binary crate, library logic in crate libraries
- Prometheus metrics via EventBus subscription

### Integration Points
- blufio-memory/src/retriever.rs: Modify scoring to add decay, importance boost, MMR
- blufio-memory/src/types.rs: Add MemorySource::FileWatcher variant
- blufio-memory/src/store.rs: Add eviction queries (count active, delete lowest-scored batch)
- blufio-config/src/model.rs: Extend MemoryConfig with new fields
- blufio (binary): Add `blufio memory validate` CLI subcommand
- serve.rs: Spawn background eviction/validation task, init file watcher

</code_context>

<deferred>
## Deferred Ideas

None -- discussion stayed within phase scope

</deferred>

---

*Phase: 55-memory-enhancements*
*Context gathered: 2026-03-11*
