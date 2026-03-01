# Phase 5: Memory & Embeddings - Context

**Gathered:** 2026-03-01
**Status:** Ready for planning

<domain>
## Phase Boundary

The agent remembers long-term facts across conversations using local ONNX embedding inference and hybrid search (vector similarity + BM25 keyword matching), loading only relevant memories into the context window per-turn. Memory extraction, storage, retrieval, and lifecycle management are in scope. Memory does NOT include skill-specific context (Phase 7) or multi-user partitioning (future phase).

</domain>

<decisions>
## Implementation Decisions

### Memory extraction
- Hybrid extraction: automatic LLM-based extraction + explicit user flags ("remember this")
- Automatic extraction runs at end of conversation — batch all turns when session goes idle (5+ minutes silence)
- Explicit user flags get higher confidence score than auto-extracted facts
- Extract everything meaningful: personal facts, preferences, project context, decisions made, task outcomes, instructions given
- Single-user scope — no user-ID partitioning, all memories belong to one user

### Retrieval & surfacing
- Dynamic threshold-based retrieval — load all memories above a semantic similarity threshold, no fixed cap
- Memories injected as structured block in the conditional zone via ConditionalProvider: "## Relevant Memories\n- [fact 1]\n- [fact 2]"
- Seamless usage — agent uses memories naturally without calling them out ("How's Max?" not "Based on my memory that your dog is named Max...")
- Hybrid search combines vector similarity and BM25 keyword matching

### Embedding model
- First-run download — binary ships without model, auto-downloads from HuggingFace on first run, cached in data directory
- CPU-only ONNX runtime — no GPU support needed (target is $4/month VPS)

### Memory lifecycle
- Explicit forget command — user can say "forget that X", agent searches and soft-deletes matching memories (audit trail preserved)
- Newer wins for contradictions — "my dog is Max" then "my dog is Luna" supersedes the older memory (old marked superseded)
- Source-based confidence scoring — explicit user flags get high confidence, LLM-extracted facts get medium, affects retrieval ranking
- No time-based decay — memories persist indefinitely unless explicitly forgotten or superseded

### Claude's Discretion
- ONNX model selection (balance quality vs 50-80MB idle memory target from CORE-07)
- Quantized INT8 vs full FP32 precision (optimize for VPS deployment)
- Hybrid search fusion strategy (RRF vs weighted linear combination vs other)
- Similarity threshold tuning for retrieval
- Extraction prompt design (what Haiku prompt extracts facts from conversation)
- Memory deduplication strategy during extraction
- BM25 tokenization approach within SQLite

</decisions>

<specifics>
## Specific Ideas

- End-of-conversation extraction batches all turns for better context — one Haiku call per conversation, not per turn
- Confidence scoring creates a natural ranking hierarchy: user-flagged > auto-extracted, which also serves as a tiebreaker in retrieval
- Soft-delete for forget + superseded memories means the audit trail is preserved but these never surface in retrieval
- The ConditionalProvider trait in blufio-context is the exact integration point — memory becomes a registered provider

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `EmbeddingAdapter` trait (blufio-core/src/traits/embedding.rs): Ready to implement for ONNX model, currently has placeholder types
- `EmbeddingInput`/`EmbeddingOutput` types (blufio-core/src/types.rs): Placeholder structs to be filled with real fields (text input, vector output)
- `ConditionalProvider` trait (blufio-context/src/conditional.rs): Integration point for injecting memories into prompt assembly
- `ContextEngine::add_conditional_provider()` (blufio-context/src/lib.rs): Registration method ready to accept memory provider
- `Database` / `SqliteStorage` (blufio-storage): WAL-mode SQLite with tokio-rusqlite single-writer, embedded migrations pattern
- `BlufioError` enum (blufio-core/src/error.rs): Has Storage, Internal, and other variants for error handling

### Established Patterns
- Adapter trait pattern: All 7 adapter types follow PluginAdapter base + specific methods
- Workspace crate structure: new crate would be `blufio-memory`
- Embedded migrations: SQL files compiled into binary, run on DB open
- Single-writer SQLite: All writes serialized through tokio-rusqlite background thread
- Cost tracking: blufio-cost tracks token usage — extraction Haiku calls must be recorded

### Integration Points
- `ContextEngine.assemble()` already calls conditional providers in registration order (line 99-102 of blufio-context/src/lib.rs)
- `SessionActor` in blufio-agent manages session lifecycle — idle detection for extraction trigger
- `CostLedger` must record extraction Haiku calls as a separate feature type
- New SQLite migration needed for memories table (vectors, metadata, BM25 index)
- `blufio-config` will need MemoryConfig section (threshold, model path, extraction settings)

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 05-memory-embeddings*
*Context gathered: 2026-03-01*
