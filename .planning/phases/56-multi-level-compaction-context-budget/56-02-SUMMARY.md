---
phase: 56-multi-level-compaction-context-budget
plan: 02
subsystem: context
tags: [compaction, levels, entity-extraction, cascade, event-bus, dynamic-zone]

# Dependency graph
requires:
  - phase: 56-multi-level-compaction-context-budget
    plan: 01
    provides: "Extended ContextConfig (soft/hard triggers, L1-L3 tokens), CompactionEvent, delete_messages_by_ids"
provides:
  - "CompactionLevel enum (L0-L3) with as_str() and serde"
  - "CompactionResult struct for multi-level compaction output"
  - "compact_to_l1 and compact_to_l2 functions with level-specific prompts"
  - "Entity extraction (extract_entities) with JSON parse fallback"
  - "Dual soft/hard trigger DynamicZone with cascade compaction"
  - "CompactionStarted/Completed event emission via EventBus"
  - "Compaction failure fallback to truncation (never blocks agent loop)"
  - "DynamicResult.compaction_usages Vec for cascade cost tracking"
  - "DynamicResult.extracted_entities for caller to persist as Memory entries"
  - "persist_compaction_summary_with_level with level metadata"
  - "build_compaction_metadata helper for structured JSON metadata"
affects: [56-03, 56-04, 56-05, quality-scoring, archive-system, cli, serve.rs]

# Tech tracking
tech-stack:
  added: [blufio-bus (blufio-context dependency)]
  patterns: [dual-trigger compaction cascade, entity extraction before L1, error fallback to truncation, trait-based memory store avoidance for cycle prevention]

key-files:
  created:
    - "crates/blufio-context/src/compaction/levels.rs"
    - "crates/blufio-context/src/compaction/extract.rs"
  modified:
    - "crates/blufio-context/src/compaction/mod.rs"
    - "crates/blufio-context/src/dynamic.rs"
    - "crates/blufio-context/src/lib.rs"
    - "crates/blufio-context/Cargo.toml"
    - "crates/blufio-agent/src/session.rs"
    - "crates/blufio/src/shell.rs"

key-decisions:
  - "Entity extraction returns strings to caller instead of directly saving to MemoryStore (avoids circular dependency blufio-context <-> blufio-memory)"
  - "compaction_usage changed from Option<TokenUsage> to compaction_usages: Vec<TokenUsage> for cascade support"
  - "DynamicResult gains extracted_entities field so caller (agent/shell) persists Memory entries"
  - "DynamicZone no longer derives Clone (EventBus Arc prevents it); struct is not Copy-needed"
  - "Entity extraction uses lenient JSON parsing with bracket-finding fallback for LLM response"
  - "L1 max_tokens scales with turn-pair count (capped at 2048) for proportional output"

patterns-established:
  - "Cascade compaction: L1 fires first, re-estimate tokens, cascade to L2 if hard threshold exceeded"
  - "Error fallback: any compaction error falls back to truncation of oldest messages"
  - "Entity extraction decoupled from storage: extract.rs returns data, caller persists"
  - "EventBus optional pattern: DynamicZone.event_bus: Option<Arc<EventBus>> (None for tests/CLI)"

requirements-completed: [COMP-01, COMP-04, COMP-06]

# Metrics
duration: 10min
completed: 2026-03-12
---

# Phase 56 Plan 02: Multi-Level Compaction Engine Summary

**L0-L3 compaction level engine with dual soft/hard triggers, cascade compaction (L1 then L2), entity extraction before L1, and truncation fallback**

## Performance

- **Duration:** 10 min
- **Started:** 2026-03-11T22:50:08Z
- **Completed:** 2026-03-12T00:00:08Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments
- Refactored compaction.rs into compaction/ module directory with mod.rs, levels.rs, and extract.rs
- Built CompactionLevel enum (L0-L3) and CompactionResult struct with level-specific metadata
- Implemented compact_to_l1 (turn-pair bullet summaries) and compact_to_l2 (session narrative) with tailored LLM prompts
- Rewrote DynamicZone with dual soft/hard triggers and cascade compaction
- Added entity extraction before L1 with robust JSON parse fallback
- Integrated CompactionStarted/Completed event emission via EventBus
- Added compaction error fallback: truncation of oldest messages so agent loop never blocks
- Changed compaction_usage to compaction_usages Vec across the stack (DynamicResult, AssembledContext, session.rs, shell.rs)
- All 30 tests pass, entire workspace compiles cleanly

## Task Commits

Each task was committed atomically:

1. **Task 1: Refactor compaction.rs to compaction/ module and implement L0-L3 level engine** - `428df5a` (feat)
2. **Task 2: Implement dual triggers, cascade compaction in DynamicZone, and entity extraction** - `5297fcc` (feat)

## Files Created/Modified
- `crates/blufio-context/src/compaction/levels.rs` - New: CompactionLevel enum, CompactionResult struct, compact_to_l1, compact_to_l2, L1_COMPACTION_PROMPT, build_compaction_metadata
- `crates/blufio-context/src/compaction/extract.rs` - New: extract_entities with JSON parse fallback, ExtractionOutput struct, EXTRACTION_PROMPT
- `crates/blufio-context/src/compaction/mod.rs` - Refactored from compaction.rs: added submodule declarations, re-exports, persist_compaction_summary_with_level
- `crates/blufio-context/src/dynamic.rs` - Rewritten: dual soft/hard triggers, cascade L1->L2, entity extraction, EventBus integration, truncation fallback
- `crates/blufio-context/src/lib.rs` - Updated AssembledContext.compaction_usages to Vec, updated ContextEngine.assemble
- `crates/blufio-context/Cargo.toml` - Added blufio-bus dependency for compaction events
- `crates/blufio-agent/src/session.rs` - Updated compaction cost recording loop for Vec<TokenUsage>
- `crates/blufio/src/shell.rs` - Updated compaction cost recording loop for Vec<TokenUsage>

## Decisions Made
- **Entity extraction avoids circular dependency**: blufio-memory depends on blufio-context, so extract.rs returns entity strings to the caller instead of directly saving via MemoryStore. The caller (session.rs or serve.rs) handles persistence. This is a design deviation from the plan which specified `memory_store: Option<&MemoryStore>` parameter.
- **compaction_usages as Vec**: Changed from `Option<TokenUsage>` to `Vec<TokenUsage>` throughout the stack to support cascade compaction producing multiple LLM calls (entity extraction + L1 + optional L2).
- **DynamicZone not Clone**: The struct now holds `Option<Arc<EventBus>>` and is not used in Clone contexts, so `#[derive(Clone)]` was removed.
- **L1 max_tokens scaling**: Per-turn-pair budget (default 256) is multiplied by pair count and capped at 2048, ensuring proportional output for varying conversation sizes.
- **Lenient JSON parsing**: Entity extraction tries direct parse first, then finds `[...]` brackets in the response for models that add explanatory text around the JSON.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Avoided circular dependency blufio-context <-> blufio-memory**
- **Found during:** Task 1 (adding blufio-memory dependency)
- **Issue:** blufio-memory already depends on blufio-context, creating a cyclic package dependency
- **Fix:** Removed blufio-memory from blufio-context Cargo.toml. Restructured extract.rs to return entity strings to the caller instead of directly saving to MemoryStore. Added extracted_entities field to DynamicResult.
- **Files modified:** crates/blufio-context/Cargo.toml, crates/blufio-context/src/compaction/extract.rs, crates/blufio-context/src/dynamic.rs
- **Verification:** cargo check --workspace passes with no cycles
- **Committed in:** 428df5a (Task 1 commit)

**2. [Rule 3 - Blocking] Updated compaction_usage consumers for Vec<TokenUsage>**
- **Found during:** Task 1 (changing DynamicResult)
- **Issue:** shell.rs and session.rs used `if let Some(ref compaction_usage)` pattern which doesn't work with Vec
- **Fix:** Changed to `for compaction_usage in &assembled.compaction_usages` loop in both consumers
- **Files modified:** crates/blufio/src/shell.rs, crates/blufio-agent/src/session.rs
- **Verification:** cargo check --workspace passes
- **Committed in:** 428df5a (Task 1 commit)

**3. [Rule 3 - Blocking] Skipped metrics.workspace dependency (not available)**
- **Found during:** Task 1 (adding Cargo.toml dependencies)
- **Issue:** Plan specified `metrics.workspace = true` but no `metrics` dependency exists in workspace Cargo.toml
- **Fix:** Skipped adding metrics; Prometheus metrics are handled through blufio-prometheus via EventBus pattern already
- **Files modified:** None (skip)
- **Verification:** Workspace compiles, metrics will be handled via EventBus subscription in prometheus crate
- **Committed in:** 428df5a (Task 1 commit)

---

**Total deviations:** 3 auto-fixed (3 blocking)
**Impact on plan:** All auto-fixes necessary for compilation correctness and architectural integrity. The circular dependency avoidance is a cleaner design than direct coupling. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- CompactionLevel, CompactionResult, compact_to_l1/l2 ready for quality scoring (Plan 03)
- Entity extraction returns entities for memory persistence in serve.rs integration (Plan 05)
- DynamicZone.with_event_bus ready for serve.rs wiring (Plan 05)
- persist_compaction_summary_with_level ready for quality score enrichment (Plan 03)
- All foundation compaction types in place for archive system (Plan 03)

## Self-Check: PASSED

All created files verified present. All commit hashes verified in git log.

---
*Phase: 56-multi-level-compaction-context-budget*
*Completed: 2026-03-12*
