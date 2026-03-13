---
phase: 62-observability-api-surface
plan: 03
subsystem: observability
tags: [opentelemetry, tracing, spans, distributed-tracing, w3c-traceparent, otel]

# Dependency graph
requires:
  - phase: 62-01
    provides: "Workspace OTel dependencies, feature flags, TracerProvider init"
provides:
  - "5 instrumented code paths with named OTel spans and rich attributes"
  - "blufio.mcp.call span for MCP tool invocation visibility"
  - "X-Trace-Id response header middleware on HTTP gateway"
  - "otel feature propagation from blufio binary to blufio-mcp-client and blufio-gateway"
affects: [62-observability-api-surface, future-otel-dashboards]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Span handle pattern (not .entered()) for async code -- avoids !Send issues"
    - "tracing::Instrument trait to attach spans to specific async futures"
    - "tracing::field::Empty + span.record() for deferred attribute population"
    - "Feature-gated middleware with #[allow(unused_mut)] for cfg-conditional mutation"
    - "Conditional feature propagation via crate?/feature syntax in Cargo.toml"

key-files:
  created: []
  modified:
    - "crates/blufio-agent/src/session.rs"
    - "crates/blufio-memory/src/retriever.rs"
    - "crates/blufio-context/src/lib.rs"
    - "crates/blufio-mcp-client/src/external_tool.rs"
    - "crates/blufio-gateway/src/server.rs"
    - "crates/blufio/src/otel.rs"
    - "crates/blufio/Cargo.toml"
    - "crates/blufio-mcp-client/Cargo.toml"
    - "crates/blufio-gateway/Cargo.toml"

key-decisions:
  - "Span handles (not .entered()) for async functions to avoid !Send across .await points"
  - "tracing::Instrument for wrapping specific async calls (provider.stream, tool.invoke)"
  - "rmcp traceparent injection deferred -- rmcp manages its own HTTP transport without hook points"
  - "blufio.mcp.call span provides Blufio-level MCP tracing while rmcp lacks header injection"
  - "Conditional feature propagation via blufio-mcp-client?/otel syntax (only if crate enabled)"
  - "X-Trace-Id middleware placed before CORS layer for all routes"

patterns-established:
  - "Async span pattern: create info_span! handle, use .instrument() on futures, record() on handle"
  - "Feature-gated middleware: #[cfg(feature)] block inside function body, #[allow(unused_mut)] on response"
  - "OTel attribute naming: dotted strings (blufio.session.id) work in info_span! but NOT in #[instrument] fields()"

requirements-completed: [OTEL-03, OTEL-04]

# Metrics
duration: 25min
completed: 2026-03-13
---

# Phase 62 Plan 03: OTel Span Instrumentation Summary

**5 named OTel spans across agent loop, LLM call, tool execution, memory retrieval, and context assembly, plus MCP call span and X-Trace-Id response header**

## Performance

- **Duration:** ~25 min
- **Started:** 2026-03-13T11:05:00Z
- **Completed:** 2026-03-13T11:30:02Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments
- Instrumented 5 key code paths with named tracing spans that automatically become OTel spans when feature is enabled
- Added blufio.mcp.call span wrapping all MCP tool invocations with server_name, tool_name, trust_zone attributes
- Added X-Trace-Id HTTP response header middleware to gateway for trace correlation
- Propagated otel feature flag from blufio binary to blufio-mcp-client and blufio-gateway subcrates
- All instrumentation compiles to standard tracing spans (structured logs) when otel feature is disabled

## Task Commits

Each task was committed atomically:

1. **Task 1: Instrument 5 key code paths with OTel-enriched spans** - `2f76cc9` (feat)
2. **Task 2: MCP trace propagation and X-Trace-Id response header** - `01e1def` (feat)

## Files Created/Modified
- `crates/blufio-agent/src/session.rs` - Agent loop, LLM call, and tool execution spans
- `crates/blufio-memory/src/retriever.rs` - Memory retrieval span with results_count and top_score
- `crates/blufio-context/src/lib.rs` - Context assembly span with zone token counts
- `crates/blufio-mcp-client/src/external_tool.rs` - blufio.mcp.call span wrapping MCP tool invocations
- `crates/blufio-gateway/src/server.rs` - X-Trace-Id response header middleware
- `crates/blufio/src/otel.rs` - Removed unused OpenTelemetryLayer import
- `crates/blufio/Cargo.toml` - Propagate otel feature to mcp-client and gateway
- `crates/blufio-mcp-client/Cargo.toml` - Added otel feature with opentelemetry dep
- `crates/blufio-gateway/Cargo.toml` - Added otel feature with tracing-opentelemetry and opentelemetry deps

## Decisions Made
- **Span handles over .entered()**: EnteredSpan is !Send and cannot be held across .await points in async functions. Used span handles with tracing::Instrument for specific futures instead.
- **info_span! over #[instrument]**: The #[instrument] attribute's fields() macro does not support dotted string literal field names like "blufio.session.id". Manual info_span! creation supports arbitrary string field names.
- **rmcp traceparent deferred**: The rmcp crate manages its own HTTP transport internally. W3C traceparent header injection requires rmcp to expose a header hook or middleware point. Documented the limitation; blufio.mcp.call span provides Blufio-level trace correlation in the meantime.
- **Conditional feature propagation**: Used `blufio-mcp-client?/otel` syntax (with `?`) to only propagate otel feature when the mcp-client crate is also enabled, avoiding forced activation of optional dependencies.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed unused OpenTelemetryLayer import warning in otel.rs**
- **Found during:** Task 2
- **Issue:** otel.rs imported `tracing_opentelemetry::OpenTelemetryLayer` via `use` but the function used fully qualified path `tracing_opentelemetry::OpenTelemetryLayer<...>` instead
- **Fix:** Removed the redundant `use` import
- **Files modified:** crates/blufio/src/otel.rs
- **Verification:** `cargo check --features otel` compiles without warnings
- **Committed in:** 01e1def (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug fix)
**Impact on plan:** Minor cleanup, no scope creep.

## Issues Encountered
- **Async span !Send issue (Task 1)**: Using `.entered()` on spans in async functions causes `!Send` errors because `EnteredSpan` holds a `RefCell` borrow. Resolved by using span handles without entering and the `tracing::Instrument` trait for wrapping specific async calls.
- **#[instrument] field name limitation (Task 1)**: The `fields()` macro in `#[tracing::instrument]` requires Rust identifiers, not dotted string literals. Resolved by creating spans manually with `info_span!`.
- **rmcp transport opacity (Task 2)**: Cannot inject W3C traceparent headers into outbound MCP HTTP requests because rmcp manages its own HTTP transport without middleware hooks. Documented the limitation and provided blufio.mcp.call span as the Blufio-level correlation point.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- All 5 named spans produce structured tracing output by default and OTel spans when feature enabled
- MCP tool invocations are now visible in traces via blufio.mcp.call span
- X-Trace-Id header allows API consumers to correlate HTTP responses with traces
- Full end-to-end MCP distributed tracing (traceparent injection) requires rmcp framework support

## Self-Check: PASSED

All 9 modified files verified present. Both task commits (2f76cc9, 01e1def) confirmed in git log. SUMMARY.md created successfully.

---
*Phase: 62-observability-api-surface*
*Completed: 2026-03-13*
