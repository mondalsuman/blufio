---
phase: 53-data-classification-pii-foundation
plan: 05
subsystem: classification
tags: [sqlite, classification, context-filtering, pii-redaction, export, defense-in-depth]

# Dependency graph
requires:
  - phase: 53-04
    provides: Classification CRUD query module, StorageAdapter trait extension, Message/Session classification fields
  - phase: 53-03
    provides: CLI/API interface stubs referencing classification operations
  - phase: 53-01
    provides: DataClassification enum, ClassificationGuard singleton
provides:
  - CLI classify set/get/list/bulk handlers wired to real DB operations via Database::open
  - API PUT/GET/POST classify handlers using StorageAdapter methods from GatewayState
  - Defense-in-depth context filtering of Restricted messages in dynamic.rs
  - Export redaction utility (redact_for_export + filter_for_export) combining can_export with PII redaction
affects: [context-assembly, gdpr-export, api-classification, phase-60]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "CLI DB access via blufio_config::load_and_validate + Database::open (same pattern as doctor, migrate)"
    - "API State extraction with require_storage helper returning 503 when storage unavailable"
    - "Defense-in-depth context filtering: SQL primary filter + ClassificationGuard secondary filter in dynamic.rs"
    - "Export redaction: single entry point combining can_export check with PII redaction"

key-files:
  created: []
  modified:
    - crates/blufio/src/classify.rs
    - crates/blufio-gateway/src/classify.rs
    - crates/blufio-context/src/dynamic.rs
    - crates/blufio-context/src/lib.rs
    - crates/blufio-security/src/classification_guard.rs
    - crates/blufio-security/src/lib.rs

key-decisions:
  - "CLI uses Database::open (not raw open_connection) to access classification query functions that take &Database"
  - "API handlers use require_storage helper pattern returning Arc<dyn StorageAdapter> or 503"
  - "Context defense-in-depth filtering placed in dynamic.rs (where Message structs still have classification) rather than lib.rs (where ProviderMessage lacks classification)"
  - "Export utility redact_for_export as method on ClassificationGuard + filter_for_export as standalone batch function"

patterns-established:
  - "CLI classify handlers: validate inputs -> open_db -> query/update -> emit event -> print result"
  - "API classify handlers: auth -> validate -> require_storage -> query/update -> emit event -> publish to EventBus"
  - "Context defense-in-depth: SQL WHERE clause as primary filter, ClassificationGuard.can_include_in_context as secondary safety net"
  - "Export pipeline: redact_for_export combines eligibility check with PII redaction in single call"

requirements-completed: [DCLS-04, DCLS-03, PII-03]

# Metrics
duration: 18min
completed: 2026-03-10
---

# Phase 53 Plan 05: Gap Closure Summary

**CLI/API handlers wired to real DB via Database::open and StorageAdapter, context defense-in-depth filtering active in dynamic.rs, and export redaction utility combining can_export with PII redaction**

## Performance

- **Duration:** 18 min
- **Started:** 2026-03-10T12:27:05Z
- **Completed:** 2026-03-10T12:45:55Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments
- CLI classify set/get/list/bulk handlers now use real DB queries instead of placeholder values
- API PUT/GET/POST handlers use State<GatewayState> with StorageAdapter classification methods
- Context engine actively filters Restricted messages via ClassificationGuard in dynamic.rs (defense-in-depth)
- Export redaction utility redact_for_export combines can_export check with PII redaction (single entry point)
- filter_for_export provides batch export filtering with exclusion counting for Phase 60 GDPR export
- All placeholder comments removed from both CLI and gateway classify handlers
- Full workspace compiles, all tests pass, clippy clean with -D warnings

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire CLI and API handlers to real DB operations** - `f705772` (feat)
2. **Task 2: Fix context engine defense-in-depth and add export redaction utility** - `75b0ad2` (feat)

## Files Created/Modified
- `crates/blufio/src/classify.rs` - CLI handlers wired to Database::open + classification queries
- `crates/blufio-gateway/src/classify.rs` - API handlers using State<GatewayState> + StorageAdapter methods
- `crates/blufio-context/src/dynamic.rs` - Defense-in-depth classification filtering after message load
- `crates/blufio-context/src/lib.rs` - Replaced unused _guard with comment explaining filter location
- `crates/blufio-security/src/classification_guard.rs` - Added redact_for_export method and filter_for_export function
- `crates/blufio-security/src/lib.rs` - Exported filter_for_export from crate

## Decisions Made
- CLI uses `Database::open` rather than raw `open_connection` because classification query functions take `&Database`, not `&tokio_rusqlite::Connection`
- API handlers use a `require_storage` helper that returns `Arc<dyn StorageAdapter>` or a 503 error, following the pattern of requiring state components
- Context defense-in-depth filtering is placed in `dynamic.rs` (where `Message` structs still have the `classification` field) rather than `lib.rs` (where messages are already converted to `ProviderMessage` which lacks classification)
- Export utility is split into `redact_for_export` (method on ClassificationGuard for single items) and `filter_for_export` (standalone function for batch operations)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed clippy collapsible_if in CLI bulk classify handler**
- **Found during:** Verification
- **Issue:** Nested `if !dry_run { if let Some(cl) = current_level { ... } }` triggered clippy::collapsible_if
- **Fix:** Collapsed to `if !dry_run && let Some(cl) = current_level { ... }` using let-chains
- **Files modified:** crates/blufio/src/classify.rs
- **Committed in:** b3f5f2e

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Minor clippy compliance fix. No scope creep.

## Issues Encountered
None - plan executed smoothly.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 53 (Data Classification & PII Foundation) is now fully complete
- All 5 plans executed: PII patterns, event pipeline, CLI/API interface, storage layer, gap closure
- Classification system is end-to-end functional: CLI/API -> StorageAdapter -> DB with defense-in-depth context filtering
- Export redaction utility is ready for Phase 60 GDPR export integration

## Self-Check: PASSED

All 7 files verified present. All 3 commits (f705772, 75b0ad2, b3f5f2e) verified in git log.

---
*Phase: 53-data-classification-pii-foundation*
*Completed: 2026-03-10*
