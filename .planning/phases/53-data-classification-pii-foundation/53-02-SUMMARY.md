---
phase: 53-data-classification-pii-foundation
plan: 02
subsystem: security
tags: [classification, pii, redaction, events, migration, config, sqlite]

# Dependency graph
requires:
  - phase: 53-01
    provides: DataClassification enum, PII detection engine, ClassificationGuard, Classifiable trait
provides:
  - V12 SQL migration adding classification column to memories, messages, sessions
  - ClassificationConfig TOML section with enabled, auto_classify_pii, default_level, warn_unencrypted
  - BusEvent::Classification with 4 sub-event variants and event_type_string() support
  - Combined PII + secret redaction pipeline in RedactingWriter
  - Memory store SQL-level filtering excluding Restricted data from retrieval
  - scan_and_classify() auto-classification pipeline
  - Event helper functions for classification event emission
affects: [53-03-PLAN, blufio-agent, blufio-context, blufio-gateway, blufio-prometheus]

# Tech tracking
tech-stack:
  added: []
  patterns: [combined PII+secret redaction pipeline, SQL-level classification filtering, string-typed event metadata to avoid cross-crate dependencies]

key-files:
  created:
    - crates/blufio-storage/migrations/V12__data_classification.sql
  modified:
    - crates/blufio-config/src/model.rs
    - crates/blufio-bus/src/events.rs
    - crates/blufio-security/src/redact.rs
    - crates/blufio-security/src/pii.rs
    - crates/blufio-security/src/lib.rs
    - crates/blufio-security/Cargo.toml
    - crates/blufio-memory/src/store.rs

key-decisions:
  - "ClassificationEvent uses String fields (not DataClassification enum) to avoid blufio-bus dependency on blufio-core"
  - "PII redaction runs before secret redaction in combined pipeline to match on original text"
  - "Memory store excludes Restricted data via SQL WHERE clause (zero Rust-side filtering overhead)"
  - "Event helpers use BTreeSet for deterministic PII type deduplication in events"

patterns-established:
  - "Combined redaction pipeline: PII type-specific placeholders first, then secret patterns"
  - "SQL-level classification filtering: WHERE classification != 'restricted' on all retrieval queries"
  - "String-typed event metadata: cross-crate events carry string representations to avoid crate dependencies"

requirements-completed: [DCLS-05, PII-02, PII-04]

# Metrics
duration: 10min
completed: 2026-03-10
---

# Phase 53 Plan 02: Data Classification Wiring Summary

**V12 migration for classification columns, ClassificationConfig TOML section, BusEvent::Classification with 4 event variants, combined PII+secret redaction pipeline, and SQL-level Restricted data exclusion from memory retrieval**

## Performance

- **Duration:** 10 min
- **Started:** 2026-03-10T10:42:11Z
- **Completed:** 2026-03-10T10:52:15Z
- **Tasks:** 3
- **Files modified:** 8

## Accomplishments
- V12 SQL migration adds classification TEXT column to memories, messages, sessions with 'internal' default and indexes on memories and messages
- ClassificationConfig struct in TOML with enabled, auto_classify_pii, default_level, warn_unencrypted (all with sensible defaults)
- ClassificationEvent sub-enum with Changed, PiiDetected, Enforced, BulkChanged variants added to BusEvent
- Combined PII + secret redaction pipeline: redact() now automatically detects and redacts PII with type-specific placeholders alongside existing secret patterns
- Memory store SQL queries exclude Restricted data from all retrieval paths (get_active, get_active_embeddings, search_bm25, get_memories_by_ids)
- scan_and_classify() auto-suggests Confidential classification when PII detected with info-level logging
- Event helper functions create classification events carrying metadata only (never actual PII values)

## Task Commits

Each task was committed atomically:

1. **Task 1: Database migration, config, and EventBus events** - `957d26e` (feat)
2. **Task 2: RedactingWriter PII integration and memory store classification filtering** - `87b3b2b` (feat)
3. **Task 3: PII auto-classification integration and EventBus emission** - `ad0c9d0` (feat)

## Files Created/Modified
- `crates/blufio-storage/migrations/V12__data_classification.sql` - Classification columns and indexes for memories, messages, sessions
- `crates/blufio-config/src/model.rs` - ClassificationConfig struct and BlufioConfig.classification field
- `crates/blufio-bus/src/events.rs` - ClassificationEvent enum with 4 variants and event_type_string() support
- `crates/blufio-security/src/redact.rs` - Combined PII + secret redaction pipeline via redact_with_pii()
- `crates/blufio-security/src/pii.rs` - scan_and_classify(), PiiScanResult, event helper functions
- `crates/blufio-security/src/lib.rs` - Re-exports for new public API
- `crates/blufio-security/Cargo.toml` - Added blufio-bus dependency and serde_json dev-dependency
- `crates/blufio-memory/src/store.rs` - Classification column in INSERT/SELECT, Restricted exclusion in WHERE

## Decisions Made
- ClassificationEvent uses String fields rather than DataClassification enum to avoid adding blufio-core as a dependency of blufio-bus (events carry metadata only)
- PII redaction runs before secret redaction in the combined pipeline so PII patterns match on original text before secret patterns alter the string
- Memory store excludes Restricted data via SQL WHERE clause rather than Rust-side post-filtering for zero overhead
- Event helpers use BTreeSet for deterministic ordering of PII type names in events

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Database migration V12 ready for deployment (classification columns on all three tables)
- ClassificationConfig available in TOML for operator configuration
- Combined redaction pipeline active in all log output paths via RedactingWriter
- Event helpers ready for Plan 03 integration (CLI/API endpoints, context engine filtering)
- All workspace crates compile clean with zero clippy warnings

## Self-Check: PASSED

- [x] crates/blufio-storage/migrations/V12__data_classification.sql exists
- [x] .planning/phases/53-data-classification-pii-foundation/53-02-SUMMARY.md exists
- [x] Commit 957d26e exists (Task 1)
- [x] Commit 87b3b2b exists (Task 2)
- [x] Commit ad0c9d0 exists (Task 3)

---
*Phase: 53-data-classification-pii-foundation*
*Completed: 2026-03-10*
