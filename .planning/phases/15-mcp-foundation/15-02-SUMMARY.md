---
phase: 15-mcp-foundation
plan: 02
subsystem: api
tags: [config, toml, serde, error-handling, mcp]

requires:
  - phase: 14-wire-integration
    provides: BlufioConfig and BlufioError types
provides:
  - McpConfig struct with enabled, servers, export_tools fields
  - McpServerEntry struct with name, transport, url, command, args, auth_token fields
  - BlufioError::Mcp variant for MCP-specific errors
affects: [16-mcp-server-stdio, 17-mcp-server-http, 18-mcp-client]

tech-stack:
  added: []
  patterns: [deny_unknown_fields on config structs, serde(default) for optional sections]

key-files:
  modified:
    - crates/blufio-config/src/model.rs
    - crates/blufio-core/src/error.rs

key-decisions:
  - "McpConfig uses #[serde(deny_unknown_fields)] to reject unknown TOML keys -- consistent with existing config pattern"
  - "BlufioError::Mcp uses message + optional source pattern -- consistent with existing error variants"

patterns-established:
  - "MCP config pattern: [[mcp.servers]] array with name/transport/url/command/args/auth_token"
  - "Error variant: BlufioError::Mcp { message, source } for all MCP-related errors"

requirements-completed: [FOUND-01]

duration: 8min
completed: 2026-03-02
---

# Plan 02: MCP config + error variant Summary

**McpConfig/McpServerEntry structs with deny_unknown_fields and BlufioError::Mcp variant for MCP errors**

## Performance

- **Duration:** 8 min
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Added McpConfig struct (enabled, servers vec, export_tools vec) with serde(default) and deny_unknown_fields
- Added McpServerEntry struct (name, transport, url, command, args, auth_token) with deny_unknown_fields
- Added BlufioError::Mcp variant with message and optional source
- 6 tests: 4 for config parsing/validation, 2 for error formatting

## Task Commits

1. **Task 1: McpConfig and McpServerEntry** - feat(15-02)
2. **Task 2: BlufioError::Mcp variant** - feat(15-02)

## Files Modified
- `crates/blufio-config/src/model.rs` - McpConfig, McpServerEntry structs + mcp field on BlufioConfig
- `crates/blufio-core/src/error.rs` - BlufioError::Mcp variant

## Decisions Made
None - followed plan as specified.

## Deviations from Plan
None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- McpConfig available for MCP crate initialization
- BlufioError::Mcp available for error propagation in MCP crates

---
*Phase: 15-mcp-foundation*
*Completed: 2026-03-02*
