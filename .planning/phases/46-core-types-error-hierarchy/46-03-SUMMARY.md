---
phase: 46-core-types-error-hierarchy
plan: 03
subsystem: error-handling
tags: [rust, error-types, channel-adapters, capabilities, typed-errors]

# Dependency graph
requires:
  - phase: 46-01
    provides: "Typed error sub-enums (ChannelErrorKind, StorageErrorKind, etc.), extended ChannelCapabilities struct, constructor helpers"
provides:
  - "All 8 channel adapters using typed ChannelErrorKind constructors"
  - "All 8 channel adapters reporting accurate streaming_type, formatting_support, rate_limit, supports_code_blocks"
  - "Storage, Skill, MCP, Migration crates using typed error sub-enums"
  - "All 6 deprecated fallback constructors removed workspace-wide"
affects: [46-04, format-pipeline, retry-logic, error-matching]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Typed error constructors: BlufioError::channel_delivery_failed(), storage_connection_failed(), skill_execution_failed(), mcp_tool_failed(), migration_schema_failed()"
    - "Extended ChannelCapabilities with StreamingType, FormattingSupport, RateLimit, supports_code_blocks"

key-files:
  created: []
  modified:
    - crates/blufio-core/src/error.rs
    - crates/blufio-discord/src/lib.rs
    - crates/blufio-discord/src/streaming.rs
    - crates/blufio-slack/src/lib.rs
    - crates/blufio-slack/src/streaming.rs
    - crates/blufio-telegram/src/lib.rs
    - crates/blufio-telegram/src/media.rs
    - crates/blufio-telegram/src/streaming.rs
    - crates/blufio-matrix/src/lib.rs
    - crates/blufio-whatsapp/src/api.rs
    - crates/blufio-whatsapp/src/cloud.rs
    - crates/blufio-whatsapp/src/web.rs
    - crates/blufio-signal/src/lib.rs
    - crates/blufio-signal/src/jsonrpc.rs
    - crates/blufio-irc/src/lib.rs
    - crates/blufio-irc/src/sasl.rs
    - crates/blufio-irc/src/flood.rs
    - crates/blufio-gateway/src/lib.rs
    - crates/blufio-gateway/src/server.rs
    - crates/blufio-storage/src/database.rs
    - crates/blufio-storage/src/adapter.rs
    - crates/blufio-storage/src/migrations.rs
    - crates/blufio-skill/src/builtin/bash.rs
    - crates/blufio-skill/src/builtin/file.rs
    - crates/blufio-skill/src/builtin/http.rs
    - crates/blufio-skill/src/manifest.rs
    - crates/blufio-skill/src/sandbox.rs
    - crates/blufio-skill/src/scaffold.rs
    - crates/blufio-skill/src/signing.rs
    - crates/blufio-skill/src/store.rs
    - crates/blufio-skill/src/tool.rs
    - crates/blufio-mcp-client/src/manager.rs
    - crates/blufio-mcp-client/src/external_tool.rs
    - crates/blufio/src/migrate.rs
    - crates/blufio/src/encrypt.rs
    - crates/blufio/src/backup.rs
    - crates/blufio/src/bench.rs
    - crates/blufio/src/main.rs
    - crates/blufio-memory/src/store.rs
    - crates/blufio-cost/src/ledger.rs
    - crates/blufio-gateway/src/webhooks/store.rs
    - crates/blufio-gateway/src/batch/store.rs
    - crates/blufio-gateway/src/api_keys/store.rs
    - crates/blufio-test-utils/src/harness.rs
    - crates/blufio-test-utils/src/mock_channel.rs
    - crates/blufio-agent/src/channel_mux.rs
    - crates/blufio/tests/e2e_mcp_server.rs

key-decisions:
  - "Map MCP client errors from BlufioError::Skill to BlufioError::Mcp (correcting pre-existing misclassification)"
  - "Use storage_connection_failed as default storage constructor for generic DB errors (most storage errors are connection-related)"
  - "Add skill_execution_msg/skill_compilation_msg constructors for message-only errors without source"
  - "Map HTTP 400 to non-retryable validation error via ModelNotFound kind (fixing pre-existing is_retryable bug)"
  - "Include request_id in Skill/Mcp/Migration Display format for test message compatibility"

patterns-established:
  - "Typed error constructor pattern: BlufioError::{domain}_{specific_kind}(source_or_msg)"
  - "Extended ChannelCapabilities: every adapter must set streaming_type, formatting_support, rate_limit, supports_code_blocks"

requirements-completed: [ERR-05, CAP-01, CAP-02, CAP-03]

# Metrics
duration: 42min
completed: 2026-03-09
---

# Phase 46 Plan 03: Channel/Storage/Skill/MCP/Migration Error Migration Summary

**Migrated all 8 channel adapters to typed ChannelErrorKind with extended capabilities, plus storage/skill/MCP/migration crates to typed sub-enums, and removed all 6 deprecated fallback constructors workspace-wide**

## Performance

- **Duration:** 42 min
- **Started:** 2026-03-09T08:24:12Z
- **Completed:** 2026-03-09T09:06:32Z
- **Tasks:** 2
- **Files modified:** 47

## Accomplishments
- All 8 channel adapters (Discord, Slack, Telegram, Matrix, WhatsApp, Signal, IRC, Gateway) use typed ChannelErrorKind constructors and report accurate extended ChannelCapabilities (streaming_type, formatting_support, rate_limit, supports_code_blocks)
- Storage crate (database, adapter, migrations) migrated to StorageErrorKind constructors
- Skill crate (9 files, ~50 construction sites) migrated to SkillErrorKind constructors
- MCP client crate migrated to McpErrorKind constructors (corrected misclassified Skill errors)
- Migration module (19 sites) migrated to MigrationErrorKind constructors
- All 6 deprecated fallback constructors (provider_generic, channel_generic, storage_generic, skill_generic, mcp_generic, migration_generic) removed from error.rs
- Full workspace compiles cleanly with all tests passing

## Task Commits

Each task was committed atomically:

1. **Task 1: Migrate 8 channel adapters to typed ChannelErrorKind and update extended capabilities** - `e6ac135` (feat)
2. **Task 2: Migrate Storage, MCP, Skill, and Migration crates to typed error sub-enums** - `f18c5bc` (feat)

## Files Created/Modified
- `crates/blufio-core/src/error.rs` - Added new constructors (storage_connection_failed, skill_execution_msg, mcp_timeout, etc.), removed 6 deprecated fallback constructors, fixed HTTP 400 mapping, updated Display formats
- `crates/blufio-discord/src/lib.rs` - Typed ChannelErrorKind + EditBased/FullMarkdown/5msg-s capabilities
- `crates/blufio-slack/src/lib.rs` - Typed ChannelErrorKind + EditBased/FullMarkdown/1msg-s capabilities
- `crates/blufio-telegram/src/lib.rs` - Typed ChannelErrorKind + EditBased/BasicMarkdown/30msg-s capabilities
- `crates/blufio-matrix/src/lib.rs` - Typed ChannelErrorKind + EditBased/HTML/no rate limit capabilities
- `crates/blufio-whatsapp/src/{api,cloud,web}.rs` - Typed ChannelErrorKind + None/BasicMarkdown/80msg-s capabilities
- `crates/blufio-signal/src/{lib,jsonrpc}.rs` - Typed ChannelErrorKind + None/PlainText/no rate limit capabilities
- `crates/blufio-irc/src/{lib,sasl,flood}.rs` - Typed ChannelErrorKind + AppendOnly/PlainText/2msg-s capabilities
- `crates/blufio-gateway/src/{lib,server}.rs` - Typed ChannelErrorKind + AppendOnly/HTML/no rate limit capabilities
- `crates/blufio-storage/src/{database,adapter,migrations}.rs` - StorageErrorKind constructors
- `crates/blufio-skill/src/{builtin/*,manifest,sandbox,scaffold,signing,store,tool}.rs` - SkillErrorKind constructors
- `crates/blufio-mcp-client/src/{manager,external_tool}.rs` - McpErrorKind constructors
- `crates/blufio/src/{migrate,encrypt,backup,bench,main}.rs` - Typed constructors across main crate
- `crates/blufio-{memory,cost,gateway,test-utils,agent}/*` - Additional typed constructor migrations

## Decisions Made
- MCP client crate was incorrectly using `BlufioError::Skill` for MCP errors -- corrected to `BlufioError::Mcp` with proper McpErrorKind variants
- Added `skill_execution_msg` and `skill_compilation_msg` constructors for message-only errors (no boxed source)
- Added `mcp_timeout` and `mcp_protocol_error` constructors for MCP-specific error patterns
- Updated Display format for Skill/Mcp/Migration variants to include `context.request_id` for test assertion compatibility
- Fixed HTTP 400 mapping in `http_status_to_provider_error` from ServerError (retryable) to ModelNotFound/Validation (non-retryable)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] blufio-memory store.rs uses old Storage pattern**
- **Found during:** Task 2 (Storage migration)
- **Issue:** `crates/blufio-memory/src/store.rs` used `BlufioError::Storage { source: Box::new(e) }` which doesn't compile after Plan 01's variant shape change
- **Fix:** Updated `storage_err` helper to use `BlufioError::storage_connection_failed(e)`
- **Files modified:** `crates/blufio-memory/src/store.rs`
- **Verification:** `cargo check -p blufio-memory` passes

**2. [Rule 3 - Blocking] 12+ additional crates with old Storage { source } pattern**
- **Found during:** Task 2 (workspace-wide cargo check)
- **Issue:** blufio-cost, blufio-gateway stores, blufio main crate (encrypt, backup, bench, main), blufio-test-utils, blufio-agent all construct Storage variant with old shape
- **Fix:** Updated all `BlufioError::Storage { source: Box::new(e) }` to `BlufioError::storage_connection_failed(e)` across 12 files
- **Files modified:** ledger.rs, webhooks/store.rs, batch/store.rs, api_keys/store.rs, harness.rs, encrypt.rs, backup.rs, bench.rs, main.rs, channel_mux.rs, mock_channel.rs
- **Verification:** `cargo check --workspace` succeeds

**3. [Rule 3 - Blocking] channel_mux.rs missing new ChannelCapabilities fields**
- **Found during:** Task 2 (workspace cargo check after storage fixes)
- **Issue:** ChannelMultiplexer's `capabilities()` missing streaming_type, formatting_support, rate_limit, supports_code_blocks fields added in Plan 01
- **Fix:** Added 4 missing fields with default values (None/PlainText) and added supports_code_blocks merging logic
- **Files modified:** `crates/blufio-agent/src/channel_mux.rs`

**4. [Rule 3 - Blocking] mock_channel.rs missing new ChannelCapabilities fields**
- **Found during:** Task 2 (workspace cargo check)
- **Issue:** MockChannel test adapter missing 4 new ChannelCapabilities fields
- **Fix:** Added fields with test-appropriate defaults
- **Files modified:** `crates/blufio-test-utils/src/mock_channel.rs`

**5. [Rule 3 - Blocking] main.rs uses old Skill { message, source } pattern**
- **Found during:** Task 2 (workspace cargo check)
- **Issue:** 11 occurrences in CLI command handlers using old Skill variant shape
- **Fix:** Replaced with skill_execution_failed(e) for IO errors, skill_execution_msg for validation
- **Files modified:** `crates/blufio/src/main.rs`

**6. [Rule 1 - Bug] HTTP 400 incorrectly mapped to retryable ServerError**
- **Found during:** Task 2 (test suite run)
- **Issue:** `http_status_to_provider_error` catch-all maps 400 to ServerError -> Unavailable -> retryable, but test correctly expects 400 to be non-retryable
- **Fix:** Added explicit 400/422 mapping to ModelNotFound (Validation -> non-retryable)
- **Files modified:** `crates/blufio-core/src/error.rs`

**7. [Rule 1 - Bug] e2e_mcp_server test assertion too strict for new error format**
- **Found during:** Task 2 (test suite run)
- **Issue:** Test asserted error contains "not found"/"No such file"/"error" but new format shows "ExecutionFailed: missing required"
- **Fix:** Added "ExecutionFailed" and "missing required" to accepted error patterns
- **Files modified:** `crates/blufio/tests/e2e_mcp_server.rs`

---

**Total deviations:** 7 auto-fixed (2 bugs [Rule 1], 5 blocking [Rule 3])
**Impact on plan:** All auto-fixes necessary for workspace compilation and test correctness. No scope creep -- these are downstream effects of the Plan 01 variant shape changes.

## Issues Encountered
- Plan scope underestimated the blast radius of removing deprecated constructors. Plan listed 4 crates but 15+ crates across the workspace construct error variants directly. All were handled via Rule 3 auto-fixes.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All error types are now fully typed with sub-enums -- ready for Plan 04 (verification and property tests)
- The format pipeline can now match on `streaming_type` and `formatting_support` from `ChannelCapabilities`
- Retry logic can match on `FailureMode` derived from typed error kinds

---
*Phase: 46-core-types-error-hierarchy*
*Completed: 2026-03-09*
