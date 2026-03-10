---
phase: 54-audit-trail
plan: 02
subsystem: audit
tags: [event-bus, subscriber, mpsc, middleware, axum, audit-trail, provider-events, memory-events]

requires:
  - phase: 54-audit-trail
    provides: AuditWriter, PendingEntry, EventFilter, AuditError from Plan 01

provides:
  - AuditSubscriber converting all BusEvent variants to PendingEntry
  - 5 new BusEvent variants (Config, Memory, Audit, Api, Provider)
  - MemoryEvent emission on CRUD in blufio-memory
  - ProviderEvent emission after LLM calls in blufio-agent
  - ApiEvent emission via audit middleware in blufio-gateway

affects: [54-03, 60-gdpr-tooling]

tech-stack:
  added: []
  patterns: [audit-subscriber-pattern, fire-and-forget-event-emission, optional-eventbus-pattern, audit-middleware-layer]

key-files:
  created:
    - crates/blufio-audit/src/subscriber.rs
    - crates/blufio-gateway/src/audit.rs
  modified:
    - crates/blufio-bus/src/events.rs
    - crates/blufio-audit/src/lib.rs
    - crates/blufio-audit/Cargo.toml
    - crates/blufio-memory/Cargo.toml
    - crates/blufio-memory/src/store.rs
    - crates/blufio-agent/src/session.rs
    - crates/blufio-gateway/src/server.rs
    - crates/blufio-gateway/src/lib.rs

key-decisions:
  - "All sub-enums use String fields (not enum types) to avoid cross-crate dependencies"
  - "MemoryStore uses Optional<Arc<EventBus>> pattern (None for tests/CLI)"
  - "ProviderEvent emitted in persist_response after cost recording (not during stream)"
  - "Audit middleware uses tokio::spawn for fire-and-forget event emission"
  - "ApiEvent actor derived from AuthContext (user:master, api-key:{id}, anonymous)"

patterns-established:
  - "AuditSubscriber: single subscriber filters via EventFilter, converts all BusEvent variants"
  - "convert_to_pending_entry: exhaustive match mapping each variant to audit fields"
  - "Optional<Arc<EventBus>> field pattern for optional event emission in library crates"
  - "Gateway audit middleware: axum route_layer with State(Option<Arc<EventBus>>)"

requirements-completed: [AUDT-02]

duration: 15min
completed: 2026-03-10
---

# Phase 54 Plan 02: Event Pipeline and AuditSubscriber Summary

**5 new BusEvent variants with AuditSubscriber conversion and event emission in memory, agent, and gateway crates**

## Performance

- **Duration:** 15 min
- **Started:** 2026-03-10T20:32:47Z
- **Completed:** 2026-03-10T20:47:43Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments
- Added 5 new BusEvent variants (Config, Memory, Audit, Api, Provider) with 14 leaf sub-variants
- Created AuditSubscriber that exhaustively converts all 33 BusEvent leaf variants to PendingEntry
- Wired event emission in 3 crates: blufio-memory (CRUD), blufio-agent (LLM calls), blufio-gateway (HTTP mutations)
- All existing match sites unaffected (already had wildcard arms or used if-let patterns)
- 10 new subscriber tests + all 33 blufio-audit, 17 blufio-bus, 61 blufio-memory tests pass

## Task Commits

Each task was committed atomically:

1. **Task 1: Add 5 new BusEvent variants and update all existing match sites** - `62a004a` (feat)
2. **Task 2: Create AuditSubscriber and add event emission in memory, agent, and gateway** - `90742fe` (feat)

## Files Created/Modified
- `crates/blufio-bus/src/events.rs` - 5 new BusEvent variants with sub-enums and event_type_string() arms
- `crates/blufio-audit/src/subscriber.rs` - AuditSubscriber with convert_to_pending_entry for all variants
- `crates/blufio-audit/src/lib.rs` - Added subscriber module and AuditSubscriber re-export
- `crates/blufio-audit/Cargo.toml` - Added blufio-bus dependency
- `crates/blufio-memory/Cargo.toml` - Added blufio-bus dependency
- `crates/blufio-memory/src/store.rs` - Optional EventBus + MemoryEvent emission on CRUD
- `crates/blufio-agent/src/session.rs` - ProviderEvent emission after cost recording
- `crates/blufio-gateway/src/audit.rs` - Audit middleware for ApiEvent on mutating requests
- `crates/blufio-gateway/src/server.rs` - Wired audit middleware into router
- `crates/blufio-gateway/src/lib.rs` - Added audit module

## Decisions Made
- All sub-enums use String fields to avoid cross-crate dependencies (established in Phase 53)
- MemoryStore uses Optional<Arc<EventBus>> with None for tests/CLI (no test breakage)
- ProviderEvent emitted in persist_response (not during stream) since tokens/cost available there
- Audit middleware uses tokio::spawn fire-and-forget to avoid blocking response
- Actor string derived from AuthContext: "user:master" for bearer, "api-key:{id}" for scoped, "anonymous" if missing
- ProviderEvent.latency_ms set to 0 since latency is not tracked at the persist_response level

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Complete event pipeline: emission sites -> EventBus -> AuditSubscriber -> AuditWriter
- Plan 03 can wire AuditSubscriber into serve.rs startup and add CLI subcommands
- All 5 new BusEvent variants and their event_type_string() mappings available for TOML filter config

## Self-Check: PASSED
