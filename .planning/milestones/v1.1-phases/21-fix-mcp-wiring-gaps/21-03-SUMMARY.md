---
phase: 21-fix-mcp-wiring-gaps
plan: 03
subsystem: mcp-client
tags: [trust-zone, conditional-provider, security, prompt-injection]

# Dependency graph
requires:
  - phase: 18-mcp-client
    provides: McpClientManager, ExternalTool, ToolRegistry, ConditionalProvider trait
provides:
  - TrustZoneProvider implementing ConditionalProvider for external tool trust warnings
  - trusted field on McpServerEntry for operator trust marking
  - Context engine wiring in serve.rs for trust zone guidance
affects: [mcp-client, context-engine, agent-prompt]

# Tech tracking
tech-stack:
  added: []
  patterns: [conditional-provider-for-trust-zones, server-level-trust-marking]

key-files:
  created:
    - crates/blufio-mcp-client/src/trust_zone.rs
  modified:
    - crates/blufio-config/src/model.rs
    - crates/blufio-config/src/validation.rs
    - crates/blufio-mcp-client/src/lib.rs
    - crates/blufio-mcp-client/Cargo.toml
    - crates/blufio-mcp-client/src/manager.rs
    - crates/blufio/src/serve.rs
    - crates/blufio/tests/e2e_mcp_client.rs
    - crates/blufio/src/doctor.rs

key-decisions:
  - "TrustZoneProvider uses __ namespace separator to identify external tools"
  - "Trust zone guidance uses factual/neutral tone: warns about unverified data, no alarmist language"

patterns-established:
  - "Trust zone pattern: ConditionalProvider that filters tools by namespace prefix and server trust status"

requirements-completed: [CLNT-10]

# Metrics
duration: 20min
completed: 2026-03-03
---

# Phase 21 Plan 03: Trust Zone Provider Summary

**TrustZoneProvider injecting factual trust guidance into agent prompts when untrusted external MCP tools are registered, with operator-level trusted server suppression**

## Performance

- **Duration:** 20 min
- **Started:** 2026-03-03T13:53:47Z
- **Completed:** 2026-03-03T14:14:29Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments
- Created TrustZoneProvider implementing ConditionalProvider trait with 5 passing tests
- Added `trusted: bool` field to McpServerEntry with serde default (false) and 2 config tests
- Wired TrustZoneProvider into serve.rs context engine, registered only when external tools discovered
- Trusted servers (marked `trusted = true` in TOML config) correctly suppress their tools from warnings

## Task Commits

Each task was committed atomically:

1. **Task 1: Add trusted field and create TrustZoneProvider** - `97cae69` (feat)
2. **Task 2: Wire TrustZoneProvider into serve.rs** - `023d390` (feat)

## Files Created/Modified
- `crates/blufio-mcp-client/src/trust_zone.rs` - TrustZoneProvider implementing ConditionalProvider, with 5 unit tests
- `crates/blufio-config/src/model.rs` - Added `trusted: bool` field to McpServerEntry with serde(default)
- `crates/blufio-config/src/validation.rs` - Updated all McpServerEntry test instances with trusted field
- `crates/blufio-mcp-client/src/lib.rs` - Added trust_zone module declaration and TrustZoneProvider re-export
- `crates/blufio-mcp-client/Cargo.toml` - Added blufio-context dependency
- `crates/blufio-mcp-client/src/manager.rs` - Updated test McpServerEntry instances with trusted field
- `crates/blufio/src/serve.rs` - Registered TrustZoneProvider with context engine after MCP client init
- `crates/blufio/tests/e2e_mcp_client.rs` - Updated test McpServerEntry instances with trusted field
- `crates/blufio/src/doctor.rs` - Updated test McpServerEntry instance with trusted field

## Decisions Made
- TrustZoneProvider identifies external tools by `__` namespace separator (same convention as ToolRegistry namespacing)
- Trust zone guidance text is factual/neutral: "may return unverified data" and "do not pass sensitive information without user confirmation"
- Provider returns empty vec (no prompt injection) when no untrusted external tools exist
- Registration only happens when `result.tools_registered > 0` to avoid unnecessary provider overhead

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added blufio-context dependency to blufio-mcp-client**
- **Found during:** Task 1 (TrustZoneProvider creation)
- **Issue:** Plan specifies `use blufio_context::conditional::ConditionalProvider` but blufio-context was not in Cargo.toml dependencies
- **Fix:** Added `blufio-context = { path = "../blufio-context" }` to Cargo.toml
- **Files modified:** crates/blufio-mcp-client/Cargo.toml
- **Verification:** Compilation succeeds, all tests pass
- **Committed in:** 97cae69 (Task 1 commit)

**2. [Rule 1 - Bug] Fixed ToolRegistry::list() return type mismatch**
- **Found during:** Task 1 (TrustZoneProvider implementation)
- **Issue:** Plan code used `Vec<(String, String)>` but actual `list()` returns `Vec<(&str, &str)>`
- **Fix:** Adjusted filter_map to use `*name` (deref) instead of `name.as_str()`
- **Files modified:** crates/blufio-mcp-client/src/trust_zone.rs
- **Verification:** All 5 trust zone tests pass
- **Committed in:** 97cae69 (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both auto-fixes necessary for compilation. No scope creep.

## Issues Encountered
- Task 1 commit was absorbed by a parallel plan executor (21-01) that committed the working tree state including this plan's changes. Task 1 work is verified present in commit 97cae69.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Trust zone provider complete and wired
- Context engine now injects trust guidance for untrusted external MCP tools
- Operators can mark servers as trusted in TOML config to suppress warnings

## Self-Check: PASSED

- FOUND: crates/blufio-mcp-client/src/trust_zone.rs
- FOUND: crates/blufio-config/src/model.rs
- FOUND: crates/blufio/src/serve.rs
- FOUND: .planning/phases/21-fix-mcp-wiring-gaps/21-03-SUMMARY.md
- FOUND: 97cae69 (Task 1)
- FOUND: 023d390 (Task 2)

---
*Phase: 21-fix-mcp-wiring-gaps*
*Completed: 2026-03-03*
