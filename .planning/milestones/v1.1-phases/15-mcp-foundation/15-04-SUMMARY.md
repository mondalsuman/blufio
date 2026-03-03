---
phase: 15-mcp-foundation
plan: 04
subsystem: api
tags: [tool-registry, namespace, collision-detection, regex, mcp]

requires:
  - phase: 15-mcp-foundation
    provides: BlufioError::Skill variant for tool registration errors
provides:
  - Namespace-aware ToolRegistry with register_builtin(), register_namespaced()
  - Tool name validation (flat and namespaced patterns)
  - Collision detection with built-in priority
  - validate_tool_name() and validate_namespaced_tool_name() public functions
affects: [16-mcp-server-stdio, 18-mcp-client]

tech-stack:
  added: [regex]
  patterns: [LazyLock for compiled regex, namespace__tool naming convention, Result-returning registration]

key-files:
  modified:
    - crates/blufio-skill/src/tool.rs
    - crates/blufio-skill/src/builtin/mod.rs
    - crates/blufio-skill/src/provider.rs
    - crates/blufio/src/serve.rs
    - crates/blufio-skill/Cargo.toml

key-decisions:
  - "register() now returns Result<(), BlufioError> -- breaking change handled by updating all call sites"
  - "Built-in tools use register_builtin() which marks them in builtin_names HashSet"
  - "register_namespaced() skips (returns Ok) on collision rather than erroring -- design decision for graceful degradation"
  - "tool_definitions() and list() use registry key (namespace__tool) not tool.name() -- ensures LLM sees namespaced names"
  - "Triple underscore (server___tool) accepted as valid: namespace=server_ separator=__ tool=tool"

patterns-established:
  - "Namespace convention: server__tool (double underscore separator)"
  - "Tool name regex: ^[a-zA-Z][a-zA-Z0-9_]*$"
  - "Namespaced regex: ^[a-zA-Z][a-zA-Z0-9_]*__[a-zA-Z][a-zA-Z0-9_]*$"
  - "Built-in priority: built-in tools always win on collision, external skipped with warning"
  - "Registration pattern: register_builtin() for built-ins, register_namespaced() for MCP tools"

requirements-completed: [FOUND-04]

duration: 15min
completed: 2026-03-02
---

# Plan 04: ToolRegistry namespace support Summary

**Namespace-aware ToolRegistry with register_builtin/register_namespaced, name validation via regex, and collision detection with built-in priority**

## Performance

- **Duration:** 15 min
- **Tasks:** 1
- **Files modified:** 5

## Accomplishments
- Added validate_tool_name() and validate_namespaced_tool_name() with LazyLock regex patterns
- Extended ToolRegistry with builtin_names HashSet for built-in tool tracking
- Added register_builtin() method that marks tools as built-in
- Added register_namespaced() method with namespace__tool prefixing and collision detection
- Changed register() to return Result with name validation and duplicate rejection
- Updated all call sites: builtin/mod.rs (3 tools), serve.rs (delegation tool), provider.rs (test helper)
- Updated list() and tool_definitions() to use registry key for namespaced tool names
- 22 tool tests pass (7 existing updated + 15 new)

## Task Commits

1. **Task 1: Namespace-aware ToolRegistry** - `ae7b0fa` (feat)

## Files Modified
- `crates/blufio-skill/src/tool.rs` - Core namespace support implementation + 15 new tests
- `crates/blufio-skill/src/builtin/mod.rs` - Changed register() to register_builtin() with .expect()
- `crates/blufio-skill/src/provider.rs` - Added .unwrap() to register() in test helper
- `crates/blufio/src/serve.rs` - Added .expect() to delegation tool registration
- `crates/blufio-skill/Cargo.toml` - Added regex dependency

## Decisions Made
- register_namespaced() returns Ok(()) on collision (skips silently with tracing::warn) rather than Err -- graceful degradation pattern
- list() returns registry key name for namespaced tools, not tool's own name() -- ensures consistency with tool_definitions()
- Triple underscore (server___tool) is accepted as valid because "server_" is a valid namespace name

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] list() returned tool.name() instead of registry key for namespaced tools**
- **Found during:** Task 1 (list_includes_builtin_and_namespaced test)
- **Issue:** list() used tool.name() which returns "add", but namespaced tools are registered as "github__add"
- **Fix:** Changed list() to iterate over (registry_name, tool) pairs, matching tool_definitions() pattern
- **Verification:** All 22 tool tests pass
- **Committed in:** ae7b0fa (part of task commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Essential fix for list() consistency with namespaced tools. No scope creep.

## Issues Encountered
None beyond the list() fix.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- ToolRegistry ready for external MCP tool registration (Phase 18)
- register_builtin() ready for MCP server tool export (Phase 16)
- Namespace convention established for MCP client tool discovery

---
*Phase: 15-mcp-foundation*
*Completed: 2026-03-02*
