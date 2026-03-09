---
phase: 47-accurate-token-counting
plan: 01
subsystem: core
tags: [token-counting, tiktoken-rs, tokenizers, heuristic, cjk, performance-config]

# Dependency graph
requires:
  - phase: 46-typed-error-hierarchy
    provides: BlufioError with Internal variant for token counter errors
provides:
  - TokenCounter async trait with count_tokens() and counter_name()
  - HeuristicCounter fallback (chars/3.5 default, chars/2.0 CJK)
  - TokenizerCache caching Arc<dyn TokenCounter> by model ID
  - TokenizerMode enum (Accurate vs Fast)
  - TiktokenEncoding enum (O200kBase vs Cl100kBase)
  - Model identification helpers (is_openai_model, is_anthropic_model, is_gemini_model)
  - PerformanceConfig with tokenizer_mode in BlufioConfig
  - Claude tokenizer vocabulary bundled in assets
  - tiktoken-rs 0.9 in workspace dependencies
affects: [47-02-provider-tokenizer-impls, 47-03-context-engine-integration]

# Tech tracking
tech-stack:
  added: [tiktoken-rs 0.9, tokenizers 0.21 (already workspace), tokio sync/rt features]
  patterns: [async TokenCounter trait with dyn dispatch, RwLock cache with double-check locking, CJK-aware heuristic, OpenRouter prefix stripping]

key-files:
  created:
    - crates/blufio-core/src/token_counter.rs
    - crates/blufio-core/assets/claude-tokenizer.json
  modified:
    - Cargo.toml
    - crates/blufio-core/Cargo.toml
    - crates/blufio-core/src/lib.rs
    - crates/blufio-config/src/model.rs

key-decisions:
  - "HeuristicCounter uses ceil() rounding to avoid underestimation -- better to slightly overcount than miss budget"
  - "TokenizerCache uses std::sync::RwLock (not tokio) since lock hold time is microseconds -- no async contention"
  - "OpenRouter model prefix stripping (e.g., openai/gpt-4o -> gpt-4o) done in resolve_counter for transparent routing"
  - "Plan 02 placeholder: OpenAI/Claude paths in Accurate mode return HeuristicCounter temporarily to keep code compiling"

patterns-established:
  - "TokenCounter trait: async_trait, Send + Sync, count_tokens(&str) -> Result<usize>"
  - "Model ID detection: pub(crate) is_*_model() pattern for provider classification"
  - "Cache pattern: RwLock<HashMap<String, Arc<dyn Trait>>> with fast-path read + slow-path write"

requirements-completed: [TOK-04, TOK-05, TOK-07, TOK-09]

# Metrics
duration: 17min
completed: 2026-03-09
---

# Phase 47 Plan 01: Token Counter Foundations Summary

**TokenCounter trait, HeuristicCounter with CJK-aware heuristics, TokenizerCache with model-keyed caching, PerformanceConfig wired into BlufioConfig, tiktoken-rs and Claude tokenizer vocabulary bundled**

## Performance

- **Duration:** 17 min
- **Started:** 2026-03-09T09:58:36Z
- **Completed:** 2026-03-09T10:16:17Z
- **Tasks:** 2 (TDD: 4 commits total -- 2 RED + 2 GREEN)
- **Files modified:** 6

## Accomplishments
- TokenCounter async trait with count_tokens() and counter_name() exported from blufio-core
- HeuristicCounter produces ceil(chars/3.5) for Latin text, ceil(chars/2.0) for >30% CJK text
- TokenizerCache caches counters by model ID, respects Fast/Accurate mode, strips OpenRouter prefixes
- PerformanceConfig.tokenizer_mode defaults to "accurate" in BlufioConfig, deserializes from TOML
- Claude tokenizer vocabulary (~1.77MB) bundled in crates/blufio-core/assets/
- tiktoken-rs 0.9 added to workspace dependencies
- 24 token_counter tests + 4 performance_config tests all pass

## Task Commits

Each task was committed atomically (TDD: RED then GREEN):

1. **Task 1: PerformanceConfig + workspace deps + Claude vocab**
   - `0c9e458` (test) -- failing tests for PerformanceConfig
   - `d8b5602` (feat) -- implementation passes all tests
2. **Task 2: TokenCounter trait + HeuristicCounter + TokenizerCache**
   - `1cb316c` (test) -- failing tests for all token counting types
   - `4b4855c` (feat) -- implementation passes all 24 tests

## Files Created/Modified
- `crates/blufio-core/src/token_counter.rs` -- TokenCounter trait, HeuristicCounter, TokenizerCache, model helpers
- `crates/blufio-core/assets/claude-tokenizer.json` -- Xenova/claude-tokenizer vocabulary (~1.77MB)
- `crates/blufio-core/src/lib.rs` -- Added token_counter module + re-exports
- `crates/blufio-core/Cargo.toml` -- Added tiktoken-rs, tokenizers, tokio deps
- `Cargo.toml` -- Added tiktoken-rs 0.9 to workspace dependencies
- `crates/blufio-config/src/model.rs` -- Added PerformanceConfig struct + BlufioConfig field

## Decisions Made
- HeuristicCounter uses ceil() rounding to avoid underestimation of token usage
- TokenizerCache uses std::sync::RwLock (not tokio::sync) since lock hold times are microseconds
- OpenRouter model prefix stripping (e.g., "openai/gpt-4o" -> "gpt-4o") done transparently in resolve_counter
- Plan 02 placeholder: OpenAI/Claude paths in Accurate mode temporarily return HeuristicCounter

## Deviations from Plan
None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- TokenCounter trait and HeuristicCounter ready for Plan 02 to add TiktokenCounter and HuggingFaceCounter
- TokenizerCache resolve_counter() has TODO comments marking where Plan 02 replaces placeholders
- PerformanceConfig ready for Plan 03 context engine integration
- All 147 blufio-core tests pass, all 85 blufio-config tests pass, workspace compiles clean

## Self-Check: PASSED

All 5 files verified present. All 4 commits verified in git log.

---
*Phase: 47-accurate-token-counting*
*Completed: 2026-03-09*
