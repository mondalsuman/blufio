---
phase: 38-migration-cli-utilities
plan: 01
subsystem: cli
tags: [migration, openclaw, toml, json, indicatif, progress-bar, idempotent]

# Dependency graph
requires:
  - phase: 37-node-system
    provides: base CLI infrastructure and storage migrations
provides:
  - "blufio migrate --from-openclaw command for full data import"
  - "blufio migrate preview command for dry-run categorized report"
  - "blufio config translate command for JSON-to-TOML conversion"
  - "BlufioError::Migration variant for migration-specific errors"
  - "V10 migration_log table for idempotent import tracking"
affects: [38-02-cli-utilities]

# Tech tracking
tech-stack:
  added: [indicatif]
  patterns: [idempotent migration via UNIQUE constraint, progress bar multi-bar pattern, secret auto-detection via key patterns]

key-files:
  created:
    - crates/blufio/src/migrate.rs
    - crates/blufio-storage/migrations/V10__migration_log.sql
  modified:
    - crates/blufio/src/main.rs
    - crates/blufio/Cargo.toml
    - crates/blufio-core/src/error.rs
    - Cargo.toml

key-decisions:
  - "OpenClaw directory auto-detection: --data-dir override > $OPENCLAW_HOME > ~/.openclaw"
  - "Idempotent imports via migration_log table with UNIQUE(source, item_type, source_id)"
  - "Best-effort SQLite parsing for OpenClaw databases with multiple table name patterns"
  - "Unknown personality files imported to ~/.local/share/blufio/import/openclaw/"
  - "Secret detection via key pattern matching (api_key, token, secret, etc.)"
  - "Config translate preserves unmapped fields as TOML comments"

patterns-established:
  - "Migration module pattern: detect -> parse -> preview/import with idempotent tracking"
  - "Progress bar pattern: MultiProgress with per-category ProgressBar instances"

requirements-completed: [MIGR-01, MIGR-02, MIGR-03, MIGR-04, MIGR-05]

# Metrics
duration: 17min
completed: 2026-03-07
---

# Phase 38 Plan 01: Migration CLI Summary

**OpenClaw migration pipeline with auto-detection, dry-run preview, idempotent import, and JSON-to-TOML config translation using indicatif progress bars**

## Performance

- **Duration:** 17 min
- **Started:** 2026-03-07T14:05:54Z
- **Completed:** 2026-03-07T14:23:41Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Full migration pipeline: detect OpenClaw directory, parse sessions/costs/files/secrets, preview and import
- Idempotent import tracking via migration_log table (V10 migration) with UNIQUE constraint
- Config translate converts OpenClaw JSON to Blufio TOML with unmapped fields as comments
- Progress bars during import for sessions, cost records, files, and secrets
- BlufioError::Migration variant for all migration-specific errors

## Task Commits

Each task was committed atomically:

1. **Task 1: Create migration tracking schema and error variant** - `b2fd932` (feat)
2. **Task 2: Implement migrate module with preview, import, and config translate** - `3da3884` (feat)

## Files Created/Modified
- `crates/blufio-storage/migrations/V10__migration_log.sql` - Migration log table for idempotent import tracking
- `crates/blufio-core/src/error.rs` - Added Migration(String) variant to BlufioError
- `crates/blufio/src/migrate.rs` - Full migration module (detection, parsing, preview, import, config translate)
- `crates/blufio/src/main.rs` - Added Migrate command, MigrateCommands enum, Translate in ConfigCommands
- `crates/blufio/Cargo.toml` - Added indicatif, toml, dirs dependencies
- `Cargo.toml` - Added indicatif to workspace dependencies

## Decisions Made
- OpenClaw directory auto-detection checks: explicit --data-dir, $OPENCLAW_HOME, ~/.openclaw (in that order)
- Best-effort SQLite parsing tries multiple table name patterns (sessions/conversations, cost_ledger/costs)
- Known personality files (SOUL.md, PERSONALITY.md, etc.) map to personality dir; unknown files to import dir
- Secret detection uses substring matching on key names (api_key, token, secret, password, credential)
- Config translate maps known fields directly and appends unmapped fields as TOML comments
- Used dirs crate for home directory detection (consistent with rest of codebase)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed config model field names to match actual types**
- **Found during:** Task 2 (migrate.rs implementation)
- **Issue:** Plan referenced `anthropic.model`, `agent.personality`, `telegram.allowed_user_ids` but actual config model uses `anthropic.default_model`, `agent.system_prompt`, `telegram.allowed_users`
- **Fix:** Updated all field references in config translate to match actual BlufioConfig struct
- **Files modified:** crates/blufio/src/migrate.rs
- **Verification:** cargo check passes, all field accesses compile
- **Committed in:** 3da3884 (Task 2 commit)

**2. [Rule 3 - Blocking] Added missing dirs dependency**
- **Found during:** Task 2 (migrate.rs implementation)
- **Issue:** dirs crate needed for home_dir() in OpenClaw detection but not in blufio Cargo.toml
- **Fix:** Added dirs.workspace = true to crates/blufio/Cargo.toml
- **Files modified:** crates/blufio/Cargo.toml
- **Verification:** cargo check passes
- **Committed in:** 3da3884 (Task 2 commit)

**3. [Rule 1 - Bug] Removed temperature mapping (field doesn't exist on AnthropicConfig)**
- **Found during:** Task 2 (migrate.rs implementation)
- **Issue:** Plan referenced `anthropic.temperature` but AnthropicConfig has no temperature field
- **Fix:** Removed temperature mapping from config translate (unmappable fields fall through to comments)
- **Files modified:** crates/blufio/src/migrate.rs
- **Verification:** cargo check passes
- **Committed in:** 3da3884 (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (2 bugs, 1 blocking)
**Impact on plan:** All fixes necessary for compilation. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Migration infrastructure complete, ready for plan 02 (CLI utilities)
- migration_log table available for any future migration sources
- BlufioError::Migration variant usable by all crates

---
*Phase: 38-migration-cli-utilities*
*Completed: 2026-03-07*
