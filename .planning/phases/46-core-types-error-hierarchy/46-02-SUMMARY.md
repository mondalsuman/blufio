---
phase: 46-core-types-error-hierarchy
plan: 02
subsystem: error-handling
tags: [provider-errors, typed-constructors, error-migration, retry-logic, retry-after]

# Dependency graph
requires: [46-01]
provides:
  - All 5 provider crates using typed ProviderErrorKind constructors
  - Centralized is_retryable() replacing per-crate is_transient_error()
  - retry-after header extraction into ErrorContext
  - Anthropic 529 -> RateLimited mapping via provider_from_http()
affects: [46-03, 46-04, blufio-agent, blufio-gateway]

# Tech tracking
tech-stack:
  added: []
  patterns: [provider_from_http for HTTP errors, error.is_retryable() for retry decisions, extract_retry_after helper]

key-files:
  created: []
  modified:
    - crates/blufio-anthropic/src/client.rs
    - crates/blufio-anthropic/src/sse.rs
    - crates/blufio-openai/src/client.rs
    - crates/blufio-openai/src/sse.rs
    - crates/blufio-openai/src/lib.rs
    - crates/blufio-gemini/src/client.rs
    - crates/blufio-gemini/src/lib.rs
    - crates/blufio-gemini/src/stream.rs
    - crates/blufio-openrouter/src/client.rs
    - crates/blufio-openrouter/src/sse.rs
    - crates/blufio-openrouter/src/lib.rs
    - crates/blufio-ollama/src/client.rs
    - crates/blufio-ollama/src/stream.rs

key-decisions:
  - "Timeout errors from reqwest detected via e.is_timeout() and mapped to provider_timeout()"
  - "retry-after header extraction added as helper method on each client struct"
  - "Ollama connection errors map to ServerError (indicates local server is down, not network)"

patterns-established:
  - "provider_from_http(status, provider_name, source) for all HTTP error responses"
  - "error.is_retryable() replaces per-crate is_transient_error() for retry decisions"
  - "extract_retry_after() helper on each cloud provider client for Retry-After header parsing"
  - "SSE/stream parse errors use ProviderErrorKind::ServerError with provider_name context"

requirements-completed: [ERR-04]

# Metrics
duration: 10min
completed: 2026-03-09
---

# Phase 46 Plan 02: Provider Error Migration Summary

**All 5 provider crates migrated from string-based BlufioError::Provider to typed ProviderErrorKind with ErrorContext, retry-after extraction, and centralized is_retryable() retry logic**

## Performance

- **Duration:** 10 min
- **Started:** 2026-03-09T08:22:50Z
- **Completed:** 2026-03-09T08:32:54Z
- **Tasks:** 2
- **Files modified:** 13

## Accomplishments
- Migrated all BlufioError::Provider { message, source } constructions in 5 provider crates to typed { kind, context, source } format
- Replaced 5 per-crate is_transient_error() functions with centralized error.is_retryable() in retry loops
- Added retry-after header extraction in Anthropic, OpenAI, and OpenRouter clients (stored in ErrorContext.retry_after)
- HTTP errors now use provider_from_http() with Anthropic 529 -> RateLimited special mapping
- Timeout errors from reqwest detected via is_timeout() and mapped to provider_timeout()
- SSE parsers (Anthropic, OpenAI, OpenRouter) and stream parsers (Gemini, Ollama NDJSON) all use typed ProviderErrorKind::ServerError
- Provider adapter lib.rs files (OpenAI, Gemini, OpenRouter) updated for typed error construction
- All tests updated to verify typed error classification rather than string matching

## Task Commits

Each task was committed atomically:

1. **Task 1: Migrate Anthropic, OpenAI, Gemini to typed errors** - `8c650d8` (feat)
2. **Task 2: Migrate OpenRouter, Ollama to typed errors** - `9023123` (feat)

## Files Created/Modified
- `crates/blufio-anthropic/src/client.rs` - Typed constructors, retry-after extraction, is_retryable() retry loop, 529 test
- `crates/blufio-anthropic/src/sse.rs` - SSE parse errors use typed ProviderErrorKind::ServerError
- `crates/blufio-openai/src/client.rs` - Typed constructors, retry-after extraction, is_retryable() retry loop
- `crates/blufio-openai/src/sse.rs` - SSE parse errors use typed ProviderErrorKind::ServerError
- `crates/blufio-openai/src/lib.rs` - Provider response extraction uses typed error
- `crates/blufio-gemini/src/client.rs` - Typed constructors, is_retryable() retry loop
- `crates/blufio-gemini/src/lib.rs` - Provider response extraction uses typed error
- `crates/blufio-gemini/src/stream.rs` - Stream parse errors use typed ProviderErrorKind::ServerError
- `crates/blufio-openrouter/src/client.rs` - Typed constructors, retry-after extraction, is_retryable() retry loop
- `crates/blufio-openrouter/src/sse.rs` - SSE parse errors use typed ProviderErrorKind::ServerError
- `crates/blufio-openrouter/src/lib.rs` - Provider response extraction uses typed error
- `crates/blufio-ollama/src/client.rs` - Typed constructors, provider_from_http for HTTP errors, connection errors -> ServerError
- `crates/blufio-ollama/src/stream.rs` - NDJSON parse errors use typed ProviderErrorKind::ServerError

## Decisions Made
- `reqwest::Error::is_timeout()` used to detect timeout errors and map to `provider_timeout()` (more precise than always using ServerError)
- `retry-after` header extraction added as a helper method on cloud provider client structs; Ollama (local) does not need it
- Ollama connection-refused errors map to `ServerError` (not network error) since it specifically indicates the local server is down
- Test assertions updated from string-matching (`contains("error_text")`) to classification-based (`is_retryable()`, `category()`)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- All 5 provider crates now use typed ProviderErrorKind constructors
- Plan 03 (channel/storage/MCP/skill migration) can proceed -- these crates still use deprecated fallback constructors
- Plan 04 (remove deprecated fallbacks) depends on Plan 03 completing first
- Circuit breaker (Phase 48) can now match on ProviderErrorKind to make automated decisions

---
*Phase: 46-core-types-error-hierarchy*
*Completed: 2026-03-09*

## Self-Check: PASSED

- All 13 files verified present on disk
- Both task commits verified in git log (8c650d8, 9023123)
- All 5 provider crates compile cleanly
