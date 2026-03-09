---
phase: 47-accurate-token-counting
plan: 02
subsystem: core
tags: [token-counting, tiktoken-rs, tokenizers, huggingface, openai, claude, openrouter, spawn-blocking]

# Dependency graph
requires:
  - phase: 47-accurate-token-counting
    plan: 01
    provides: TokenCounter trait, HeuristicCounter, TokenizerCache, TiktokenEncoding enum, model helpers
provides:
  - TiktokenCounter using tiktoken-rs singletons (o200k_base + cl100k_base)
  - HuggingFaceCounter using bundled Claude vocabulary via OnceLock + include_bytes!
  - count_with_fallback graceful degradation function
  - TokenizerCache resolve_counter wired to real tokenizer implementations
  - OpenRouter prefix routing (openai/*, anthropic/*, google/*, unknown/*)
affects: [47-03-context-engine-integration]

# Tech tracking
tech-stack:
  added: []
  patterns: [spawn_blocking for all synchronous encode() calls, OnceLock singleton for HuggingFace tokenizer, tiktoken-rs singleton pattern for zero-allocation BPE access, OpenRouter prefix-based provider routing]

key-files:
  created: []
  modified:
    - crates/blufio-core/src/token_counter.rs
    - crates/blufio-core/src/lib.rs

key-decisions:
  - "OnceLock init uses get+set pattern instead of unstable get_or_try_init -- race-safe, discards duplicate"
  - "tokenizers::Tokenizer::encode takes &str not &String -- use text.as_str() for correct trait bound"
  - "resolve_counter lowercases model IDs for case-insensitive matching"

patterns-established:
  - "spawn_blocking pattern: clone text into String, move into closure, call synchronous encode, return len()"
  - "OnceLock singleton pattern: get() first, then construct + set() with race tolerance"
  - "OpenRouter prefix routing: strip prefix to determine provider, then delegate to provider-specific counter"

requirements-completed: [TOK-02, TOK-03, TOK-06, TOK-08]

# Metrics
duration: 5min
completed: 2026-03-09
---

# Phase 47 Plan 02: Provider Tokenizer Implementations Summary

**TiktokenCounter for OpenAI models (o200k/cl100k singletons), HuggingFaceCounter for Claude (bundled vocab via OnceLock), count_with_fallback for graceful degradation, TokenizerCache wired to all 5 provider strategies**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-09T10:19:13Z
- **Completed:** 2026-03-09T10:23:43Z
- **Tasks:** 2 (TDD: 3 commits -- 1 RED + 1 GREEN + 1 test-only)
- **Files modified:** 2

## Accomplishments
- TiktokenCounter uses o200k_base_singleton for GPT-4o/4.1/5/o1/o3/o4 and cl100k_base_singleton for GPT-4/3.5
- HuggingFaceCounter loads Claude vocabulary exactly once via OnceLock + include_bytes!, ~1.77MB compiled in
- All synchronous encode() calls wrapped in tokio::task::spawn_blocking -- never block tokio worker threads
- TokenizerCache resolve_counter maps all 5 provider model patterns plus OpenRouter prefixes to correct counters
- count_with_fallback gracefully degrades to HeuristicCounter when primary tokenizer fails
- 43 token_counter tests pass (up from 24 in Plan 01), workspace compiles clean

## Task Commits

Each task was committed atomically (TDD flow):

1. **Task 1: TiktokenCounter and HuggingFaceCounter implementations**
   - `8fbe2ee` (test) -- failing tests for TiktokenCounter, HuggingFaceCounter, count_with_fallback
   - `1aa0a5e` (feat) -- implementation passes all tests
2. **Task 2: DelegatingCounter and TokenizerCache wiring**
   - `b54e72f` (test) -- routing and end-to-end tests for all provider paths

## Files Created/Modified
- `crates/blufio-core/src/token_counter.rs` -- Added TiktokenCounter, HuggingFaceCounter, count_with_fallback, updated resolve_counter with real implementations
- `crates/blufio-core/src/lib.rs` -- Re-exported TiktokenCounter, HuggingFaceCounter, count_with_fallback

## Decisions Made
- Used OnceLock get()+set() pattern instead of unstable get_or_try_init -- race-safe with harmless duplicate discard
- tokenizers::Tokenizer::encode requires &str (not &String) -- used text.as_str() for correct trait bound resolution
- resolve_counter lowercases model IDs for case-insensitive matching before provider detection

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] OnceLock::get_or_try_init is unstable**
- **Found during:** Task 1 (HuggingFaceCounter implementation)
- **Issue:** `get_or_try_init` requires nightly Rust (unstable feature `once_cell_try`)
- **Fix:** Used `get()` + manual `set()` pattern with race tolerance
- **Files modified:** crates/blufio-core/src/token_counter.rs
- **Verification:** Compiles on stable Rust, all tests pass
- **Committed in:** `1aa0a5e` (Task 1 GREEN commit)

**2. [Rule 3 - Blocking] tokenizers::Tokenizer::encode trait bound mismatch**
- **Found during:** Task 1 (HuggingFaceCounter implementation)
- **Issue:** `encode(&text, false)` failed because `&String` does not implement `Into<EncodeInput>`, only `&str` does
- **Fix:** Changed to `encode(text.as_str(), false)` for correct trait bound
- **Files modified:** crates/blufio-core/src/token_counter.rs
- **Verification:** Compiles cleanly, HuggingFace tokenization tests pass
- **Committed in:** `1aa0a5e` (Task 1 GREEN commit)

---

**Total deviations:** 2 auto-fixed (2 blocking issues)
**Impact on plan:** Both were necessary API compatibility fixes. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All 5 provider token counting strategies implemented and registered in TokenizerCache
- TokenizerCache ready for Plan 03 to inject into context engine (DynamicZone::resolve)
- count_with_fallback ready for use at all token estimation sites
- 43 token_counter tests provide regression coverage

## Self-Check: PASSED

All files verified present. All 3 commits verified in git log.

---
*Phase: 47-accurate-token-counting*
*Completed: 2026-03-09*
