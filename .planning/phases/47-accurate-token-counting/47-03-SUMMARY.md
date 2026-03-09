---
phase: 47-accurate-token-counting
plan: 03
subsystem: context-engine
tags: [tokenizer, token-counting, tiktoken, huggingface, context-engine]

# Dependency graph
requires:
  - phase: 47-01
    provides: "TokenCounter trait, HeuristicCounter, TokenizerCache, TiktokenCounter"
  - phase: 47-02
    provides: "HuggingFaceCounter, count_with_fallback, model routing in TokenizerCache"
provides:
  - "Context engine uses accurate token counting via TokenizerCache"
  - "len()/4 heuristic completely removed from DynamicZone"
  - "All 4 ContextEngine construction sites pass TokenizerCache"
  - "TOK-01 satisfied: accurate token counting for all 5 providers"
affects: [context-engine, compaction, session-actor]

# Tech tracking
tech-stack:
  added: []
  patterns: ["Arc<TokenizerCache> injection through ContextEngine to DynamicZone"]

key-files:
  modified:
    - crates/blufio-context/src/dynamic.rs
    - crates/blufio-context/src/lib.rs
    - crates/blufio-core/src/token_counter.rs
    - crates/blufio/src/serve.rs
    - crates/blufio/src/shell.rs
    - crates/blufio-agent/src/delegation.rs
    - crates/blufio-test-utils/src/harness.rs

key-decisions:
  - "delegation.rs uses TokenizerMode::Accurate (no BlufioConfig access, safe default)"
  - "test harness uses TokenizerMode::Fast for test performance"
  - "Added Debug impl for TokenizerCache (needed by DynamicZone derive)"

patterns-established:
  - "TokenizerCache injection: Arc<TokenizerCache> passed from config to ContextEngine to DynamicZone"
  - "count_with_fallback pattern: graceful degradation to heuristic on tokenizer failure"

requirements-completed: [TOK-01]

# Metrics
duration: 10min
completed: 2026-03-09
---

# Phase 47 Plan 03: Context Engine Token Counting Integration Summary

**Replaced len()/4 heuristic with real tokenizer-backed counting via TokenizerCache injection into DynamicZone and ContextEngine**

## Performance

- **Duration:** 10 min
- **Started:** 2026-03-09T10:26:35Z
- **Completed:** 2026-03-09T10:37:00Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Replaced the len()/4 heuristic in DynamicZone with count_with_fallback() using provider-specific tokenizers
- Injected Arc<TokenizerCache> through ContextEngine::new() into DynamicZone for accurate token estimation
- Updated all 4 ContextEngine construction sites (serve.rs, shell.rs, delegation.rs, harness.rs) to pass TokenizerCache
- serve.rs and shell.rs read config.performance.tokenizer_mode to determine Accurate vs Fast mode

## Task Commits

Each task was committed atomically:

1. **Task 1: Inject TokenizerCache into DynamicZone and ContextEngine** - `7fde6b7` (feat)
2. **Task 2: Update all ContextEngine construction sites** - `c0185f6` (feat)

## Files Created/Modified
- `crates/blufio-core/src/token_counter.rs` - Added Debug impl for TokenizerCache
- `crates/blufio-context/src/dynamic.rs` - Added token_cache field, replaced len()/4 with count_with_fallback, added model param
- `crates/blufio-context/src/lib.rs` - Added token_cache field to ContextEngine, updated constructor signature
- `crates/blufio/src/serve.rs` - Create TokenizerCache from config.performance.tokenizer_mode, pass to ContextEngine
- `crates/blufio/src/shell.rs` - Same pattern as serve.rs
- `crates/blufio-agent/src/delegation.rs` - Create TokenizerCache with Accurate mode for specialist agents
- `crates/blufio-test-utils/src/harness.rs` - Create TokenizerCache with Fast mode for test harness

## Decisions Made
- delegation.rs uses TokenizerMode::Accurate as default since specialist agents don't have access to BlufioConfig's performance section
- test harness uses TokenizerMode::Fast to avoid loading real tokenizers during tests
- Added Debug impl for TokenizerCache (Rule 3 auto-fix: needed because DynamicZone derives Debug, and Arc<T> requires T: Debug)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added Debug impl for TokenizerCache**
- **Found during:** Task 1 (DynamicZone struct modification)
- **Issue:** DynamicZone derives Debug, adding Arc<TokenizerCache> requires TokenizerCache: Debug
- **Fix:** Implemented fmt::Debug for TokenizerCache showing mode and cached counter count
- **Files modified:** crates/blufio-core/src/token_counter.rs
- **Verification:** Compilation succeeds with DynamicZone derive(Debug)
- **Committed in:** 7fde6b7 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Minimal -- Debug impl is a trivial addition required for compilation. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- TOK-01 is fully satisfied: context engine uses accurate token counting for all 5 providers
- Phase 47 (Accurate Token Counting) is complete
- Ready for Phase 48 (Circuit Breaker for Provider Calls)

---
*Phase: 47-accurate-token-counting*
*Completed: 2026-03-09*
