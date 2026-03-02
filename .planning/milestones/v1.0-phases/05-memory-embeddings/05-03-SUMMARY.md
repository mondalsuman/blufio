---
phase: 05-memory-embeddings
plan: 03
type: summary
status: complete
commit: 089198e
duration: ~15min
tests_added: 3
tests_total: 328
---

# Plan 05-03 Summary: Agent loop integration

## What was built

Wired the complete memory system into the agent loop, making memories live in the running agent.

### Changes

1. **FeatureType::Extraction** (`crates/blufio-cost/src/ledger.rs`)
   - Added `Extraction` variant to the `FeatureType` enum for tracking memory extraction costs
   - Existing Display/EnumString derive macros automatically handle serialization

2. **Dependencies** (`crates/blufio-agent/Cargo.toml`, `crates/blufio/Cargo.toml`)
   - Added `blufio-memory` dependency to both crates
   - Added `tokio-rusqlite` to binary crate for memory store initialization

3. **Startup initialization** (`crates/blufio/src/serve.rs`, `crates/blufio/src/shell.rs`)
   - `initialize_memory()` function in both serve.rs and shell.rs:
     - Downloads ONNX model via ModelManager on first run
     - Creates OnnxEmbedder, MemoryStore (separate SQLite connection), HybridRetriever
     - Creates MemoryProvider and registers with ContextEngine via `add_conditional_provider()`
     - Creates MemoryExtractor for background fact extraction
   - Graceful degradation: memory init failure logs warning, continues without memory
   - Memory skipped entirely when `config.memory.enabled` is false

4. **AgentLoop memory plumbing** (`crates/blufio-agent/src/lib.rs`)
   - Added `memory_provider: Option<MemoryProvider>` and `memory_extractor: Option<Arc<MemoryExtractor>>` fields
   - Passes memory components to each SessionActor on creation
   - Passes `idle_timeout_secs` from config to SessionActor

5. **SessionActor memory integration** (`crates/blufio-agent/src/session.rs`)
   - New fields: `memory_provider`, `memory_extractor`, `last_message_at`, `idle_timeout`
   - Sets current query on MemoryProvider before `context_engine.assemble()` call
   - Clears current query after assembly (regardless of success/failure)
   - `maybe_trigger_idle_extraction()`: checks if idle timeout exceeded, extracts facts from conversation via Haiku LLM call, records cost with FeatureType::Extraction
   - All memory operations wrapped in error handlers -- never crash the agent

6. **MemoryProvider enhancements** (`crates/blufio-memory/src/provider.rs`)
   - Made `MemoryProvider` `Clone` (cheap: all fields are Arc-wrapped)
   - Enables sharing between ContextEngine registration and SessionActor

7. **MemoryExtractor accessor** (`crates/blufio-memory/src/extractor.rs`)
   - Added `extraction_model()` method for cost tracking by SessionActor

### Key design decisions

- **Clone instead of Arc wrapping**: MemoryProvider derives Clone with cheap Arc-based internals rather than wrapping in Arc, avoiding orphan rule issues with ConditionalProvider trait
- **Separate SQLite connection**: MemoryStore opens its own connection to the same database, avoiding contention with the main storage connection
- **Check-on-next-message idle extraction**: Instead of a background timer, extraction triggers when the next message arrives after idle timeout -- simpler, no extra task management
- **Non-fatal memory operations**: Every memory operation is wrapped in error handling that logs warnings but never propagates errors to the main message flow

## Verification

- `cargo build` compiles cleanly
- `cargo test --workspace` passes all 328 tests (0 failures)
- FeatureType::Extraction round-trips through Display/FromStr
- Extraction cost records can be saved to cost ledger
- SessionActor accepts optional memory fields (None when disabled)
