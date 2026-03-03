---
phase: 15-mcp-foundation
plan: 01
subsystem: infra
tags: [reqwest, rmcp, schemars, cargo, workspace-deps]

requires:
  - phase: 14-wire-integration
    provides: stable v1.0 workspace with reqwest 0.12
provides:
  - reqwest 0.13 workspace dependency (upgraded from 0.12)
  - rmcp 0.17 workspace dependency (MCP SDK)
  - schemars 1.0 workspace dependency (JSON Schema generation)
affects: [15-mcp-foundation, 16-mcp-server-stdio, 17-mcp-server-http, 18-mcp-client]

tech-stack:
  added: [rmcp 0.17, schemars 1.0]
  patterns: [workspace dependency pinning]

key-files:
  modified:
    - Cargo.toml

key-decisions:
  - "reqwest 0.13 feature rustls-tls renamed to rustls -- updated workspace feature list"
  - "teloxide-core still pulls reqwest 0.12 as transitive dep -- acceptable dual version"

patterns-established:
  - "Workspace deps: rmcp and schemars follow same pattern as other workspace dependencies"

requirements-completed: [FOUND-03]

duration: 10min
completed: 2026-03-02
---

# Plan 01: Upgrade reqwest + workspace deps Summary

**Upgraded reqwest 0.12 to 0.13, added rmcp 0.17 and schemars 1.0 as workspace dependencies**

## Performance

- **Duration:** 10 min
- **Tasks:** 1
- **Files modified:** 2 (Cargo.toml, Cargo.lock)

## Accomplishments
- Upgraded reqwest from 0.12 to 0.13 across entire workspace
- Added rmcp 0.17 (MCP SDK) as workspace dependency with default-features disabled
- Added schemars 1.0 as workspace dependency for JSON Schema generation
- All existing workspace tests pass with new dependency versions

## Task Commits

1. **Task 1: Upgrade reqwest, add rmcp and schemars** - feat(15-01)

## Files Modified
- `Cargo.toml` - Workspace dependency versions updated
- `Cargo.lock` - Auto-regenerated

## Decisions Made
- reqwest 0.13 renamed feature `rustls-tls` to `rustls` -- fixed in workspace config
- teloxide-core still depends on reqwest 0.12 transitively -- accepted as dual version since types don't cross crate boundaries

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] rustls-tls feature renamed in reqwest 0.13**
- **Found during:** Task 1 (reqwest upgrade)
- **Issue:** Feature `rustls-tls` doesn't exist in reqwest 0.13
- **Fix:** Changed feature name from `rustls-tls` to `rustls` in workspace Cargo.toml
- **Verification:** `cargo build --workspace` succeeds

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Essential fix for reqwest 0.13 compatibility. No scope creep.

## Issues Encountered
None beyond the feature rename.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- rmcp 0.17 available as workspace dep for MCP crate creation
- schemars 1.0 available for JSON Schema generation in MCP server

---
*Phase: 15-mcp-foundation*
*Completed: 2026-03-02*
