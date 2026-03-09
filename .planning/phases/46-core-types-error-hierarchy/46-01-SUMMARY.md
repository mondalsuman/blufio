---
phase: 46-core-types-error-hierarchy
plan: 01
subsystem: error-handling
tags: [thiserror, serde, strum, proptest, tracing-test, error-hierarchy, classification]

# Dependency graph
requires: []
provides:
  - BlufioError typed hierarchy with 6 sub-enums and ErrorContext
  - 7 classification methods (is_retryable, severity, category, failure_mode, trips_circuit_breaker, suggested_backoff, user_message)
  - 18 typed constructor helpers + 6 deprecated fallback constructors
  - http_status_to_provider_error() centralized HTTP status mapping
  - error_log! macro for structured logging
  - ChannelCapabilities extended with 4 new typed fields (streaming_type, formatting_support, rate_limit, supports_code_blocks)
  - StreamingType, FormattingSupport, RateLimit types
affects: [46-02, 46-03, 46-04, blufio-agent, blufio-gateway, blufio-prometheus]

# Tech tracking
tech-stack:
  added: [proptest 1, tracing-test 0.2]
  patterns: [sub-enum kind fields, classification-derived retryability, ErrorContext metadata, deprecated fallback constructors]

key-files:
  created: []
  modified:
    - crates/blufio-core/src/error.rs
    - crates/blufio-core/src/types.rs
    - crates/blufio-core/src/format.rs
    - crates/blufio-core/src/lib.rs
    - crates/blufio-core/Cargo.toml
    - Cargo.toml

key-decisions:
  - "user_message() returns Cow<'static, str> for zero-allocation static messages with dynamic fallback"
  - "ChannelCapabilities derives Default for ergonomic construction with ..Default::default()"
  - "Deprecated fallback constructors map to sensible defaults (ServerError, DeliveryFailed, Busy, etc.)"

patterns-established:
  - "Sub-enum kind field pattern: BlufioError::Provider { kind: ProviderErrorKind, context: ErrorContext }"
  - "Classification derived from FailureMode: is_retryable() and trips_circuit_breaker() are pure functions of failure_mode()"
  - "Constructor helpers are pure -- no side effects, no logging, no metrics"
  - "error_log! macro dispatches to tracing levels based on severity with structured fields"

requirements-completed: [ERR-01, ERR-02, ERR-03]

# Metrics
duration: 5min
completed: 2026-03-09
---

# Phase 46 Plan 01: Core Types & Error Hierarchy Summary

**Typed error hierarchy with 6 sub-enums, 7 classification methods, error_log! macro, and ChannelCapabilities extended with streaming/formatting/rate-limit fields**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-09T08:13:46Z
- **Completed:** 2026-03-09T08:19:36Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- Defined complete typed error hierarchy: 6 sub-enums (ProviderErrorKind, ChannelErrorKind, StorageErrorKind, SkillErrorKind, McpErrorKind, MigrationErrorKind) with Severity, ErrorCategory, FailureMode classification enums
- Implemented 7 classification methods on BlufioError plus http_status_to_provider_error() with Anthropic 529 override
- Added 18 typed constructor helpers and 6 deprecated fallback constructors for downstream compatibility
- Extended ChannelCapabilities from 9 to 13 fields with StreamingType, FormattingSupport, RateLimit types
- 105 tests passing including proptest invariant tests and tracing-test structured field verification

## Task Commits

Each task was committed atomically:

1. **Task 1: Define error hierarchy types, sub-enums, ErrorContext, and classification methods** - `1c96b6e` (feat)
2. **Task 2: Extend ChannelCapabilities with 4 new typed fields** - `6ebe969` (feat)

## Files Created/Modified
- `crates/blufio-core/src/error.rs` - Complete typed error hierarchy: 6 sub-enums, ErrorContext, classification methods, constructors, error_log! macro, proptest and tracing tests
- `crates/blufio-core/src/types.rs` - StreamingType, FormattingSupport, RateLimit types; ChannelCapabilities extended to 13 fields with Default derive
- `crates/blufio-core/src/format.rs` - Updated test helpers to use ..Default::default() for new ChannelCapabilities fields
- `crates/blufio-core/src/lib.rs` - Updated re-exports for all new public types; updated variant construction test
- `crates/blufio-core/Cargo.toml` - Added tracing dependency and proptest/tracing-test dev-dependencies
- `Cargo.toml` - Added proptest and tracing-test to workspace dependencies

## Decisions Made
- `user_message()` returns `Cow<'static, str>` -- static for fixed messages, owned String for dynamic ones (avoids per-call allocation for the common case)
- `ChannelCapabilities` derives `Default` so all construction sites can use `..Default::default()` to handle the 4 new fields
- Deprecated fallback constructors map to sensible default kinds: `provider_generic` -> ServerError, `channel_generic` -> DeliveryFailed, `storage_generic` -> Busy, etc.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- All error types, classification methods, and constructors ready for downstream migration
- Plan 02 (provider migration) can proceed immediately -- deprecated fallback constructors ensure workspace compiles
- Plan 03 (channel/storage/MCP/skill migration) depends on Plan 02 completing first
- ChannelCapabilities new fields default to conservative values so adapter crates compile unchanged

---
*Phase: 46-core-types-error-hierarchy*
*Completed: 2026-03-09*

## Self-Check: PASSED

- All 6 files verified present on disk
- Both task commits verified in git log (1c96b6e, 6ebe969)
- 105 blufio-core tests passing
