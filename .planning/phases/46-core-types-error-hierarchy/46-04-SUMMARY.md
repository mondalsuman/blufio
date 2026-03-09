---
phase: 46-core-types-error-hierarchy
plan: 04
subsystem: error-handling
tags: [format-pipeline, table-degradation, error-classification, prometheus, gateway-api, rich-content]

# Dependency graph
requires:
  - phase: 46-02
    provides: "Typed ProviderErrorKind constructors in all 5 provider crates"
  - phase: 46-03
    provides: "Typed sub-enum constructors in all channel/storage/skill/MCP/migration crates, extended ChannelCapabilities"
provides:
  - "Table and List RichContent variants with 3-tier table degradation"
  - "Structured error counter blufio_errors_total with 3 labels (category, failure_mode, severity)"
  - "Gateway error responses with category, retryable, failure_mode fields"
  - "Agent loop using error.category() instead of classify_error_type()"
  - "ColumnAlign, ListStyle, Table, List types in blufio-core"
affects: [format-pipeline, phase-48-circuit-breaker, blufio-gateway, blufio-prometheus]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "3-tier table degradation: unicode box (code blocks) -> GFM markdown (FullMarkdown) -> key:value (PlainText)"
    - "record_classified_error(&BlufioError) for structured Prometheus metrics"
    - "GatewayErrorDetail extended with classification fields for client retry automation"

key-files:
  created: []
  modified:
    - crates/blufio-core/src/format.rs
    - crates/blufio-core/src/lib.rs
    - crates/blufio-agent/src/lib.rs
    - crates/blufio-prometheus/src/recording.rs
    - crates/blufio-prometheus/src/lib.rs
    - crates/blufio-gateway/src/openai_compat/types.rs
    - crates/blufio-gateway/src/openai_compat/handlers.rs
    - crates/blufio-gateway/src/openai_compat/responses.rs
    - crates/blufio-gateway/src/openai_compat/tools.rs

key-decisions:
  - "Table degradation uses FormattingSupport and supports_code_blocks from ChannelCapabilities for tier selection"
  - "HTML formatting support uses Tier 2 (GFM) when code blocks unavailable"
  - "Gateway provider error responses populate classification from BlufioError methods; static error responses use None"
  - "Legacy record_error(type) kept alongside new record_error_classified(category, failure_mode, severity)"

patterns-established:
  - "RichContent::Table and RichContent::List produce FormattedOutput::Text (no new FormattedOutput variants)"
  - "Empty tables show headers + '(no data)' at all 3 degradation tiers"
  - "Lists degrade cleanly to all channels using universal dash/numbered format"
  - "Error consumer pattern: blufio_prometheus::record_classified_error(&e) for one-call structured recording"

requirements-completed: [FMT-01, FMT-02, FMT-03]

# Metrics
duration: 15min
completed: 2026-03-09
---

# Phase 46 Plan 04: FormatPipeline Table/List Extensions & Error Consumer Updates Summary

**Table and List RichContent with 3-tier degradation, Prometheus 3-label error counter, gateway classification fields, and classify_error_type() removal**

## Performance

- **Duration:** 15 min
- **Started:** 2026-03-09T09:10:30Z
- **Completed:** 2026-03-09T09:25:38Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments
- Added Table and List content types to FormatPipeline with ColumnAlign, ListStyle enums, Table and List structs
- Implemented 3-tier table degradation: Tier 1 unicode box-drawing in code fence, Tier 2 GFM markdown table, Tier 3 key:value per row
- Removed classify_error_type() from agent loop; all error consumers now use BlufioError classification methods
- Added record_classified_error(&BlufioError) to blufio-prometheus for one-call structured error recording with 3 labels
- Extended GatewayErrorDetail with category, retryable, failure_mode fields populated from BlufioError on provider errors
- 28 format tests (14 table, 6 list, 8 original), 120 gateway tests, full workspace green

## Task Commits

Each task was committed atomically:

1. **Task 1: Add Table and List content types with 3-tier degradation** - `e6336ca` (feat)
2. **Task 2: Update error consumers with structured classification** - `b672cf6` (feat)

## Files Created/Modified
- `crates/blufio-core/src/format.rs` - Added ColumnAlign, ListStyle, Table, List types; RichContent::Table and RichContent::List variants; 3-tier table degradation (unicode box, GFM markdown, key:value); list rendering (bullet/ordered); 28 tests
- `crates/blufio-core/src/lib.rs` - Re-exports for ColumnAlign, ListStyle, Table, List
- `crates/blufio-agent/src/lib.rs` - Removed classify_error_type(); uses record_classified_error(&e) for prometheus metrics
- `crates/blufio-prometheus/src/recording.rs` - Added record_error_classified(category, failure_mode, severity) and record_classified_error(&BlufioError) convenience function
- `crates/blufio-prometheus/src/lib.rs` - Re-exports for new recording functions
- `crates/blufio-gateway/src/openai_compat/types.rs` - Added category, retryable, failure_mode fields to GatewayErrorDetail (skip_serializing_if None); 2 new tests
- `crates/blufio-gateway/src/openai_compat/handlers.rs` - Provider error responses populate classification fields from BlufioError; 14 GatewayErrorDetail sites updated
- `crates/blufio-gateway/src/openai_compat/responses.rs` - All 4 GatewayErrorDetail sites updated with new fields
- `crates/blufio-gateway/src/openai_compat/tools.rs` - All 3 GatewayErrorDetail sites updated with new fields

## Decisions Made
- Table degradation tier selection uses `supports_code_blocks` (Tier 1) then `formatting_support` (Tier 2 for FullMarkdown/HTML, Tier 3 for PlainText/BasicMarkdown) -- consistent with the ChannelCapabilities fields added in Plan 01
- HTML formatting support falls through to Tier 2 (GFM markdown) when code blocks are unavailable, since HTML channels can render markdown tables
- Legacy `record_error(type)` function kept alongside new `record_error_classified(category, failure_mode, severity)` to avoid breaking any downstream code that might call the single-label API directly
- Only the provider error response path in handlers.rs populates classification fields from BlufioError; static error responses (not found, bad request, etc.) use None since they don't originate from BlufioError

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Phase 46 is complete: all error types migrated, format pipeline extended, error consumers updated
- Phase 48 (circuit breaker) can use error.trips_circuit_breaker() and error.suggested_backoff() directly
- FormatPipeline Table/List types ready for use by channel adapters
- Gateway API clients can use category/retryable/failure_mode fields for automated retry decisions

## Self-Check: PASSED

All 9 modified files verified present on disk. Both task commits (`e6336ca`, `b672cf6`) verified in git log.

---
*Phase: 46-core-types-error-hierarchy*
*Completed: 2026-03-09*
