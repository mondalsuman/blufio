---
phase: 63-code-quality-hardening
plan: 01
subsystem: code-quality
tags: [refactoring, module-decomposition, rust-modules, code-organization]

# Dependency graph
requires: []
provides:
  - "serve/ directory with 5 focused modules (storage, channels, gateway, subsystems, mod)"
  - "cli/ directory with 7 handler modules (audit, config, injection, memory, nodes, plugin, skill)"
  - "QUAL-03: actual uptime in /api/status health endpoint"
  - "QUAL-04: no unimplemented!() in mock providers"
affects: [all-future-phases]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Monolithic file decomposition into directory modules with mod.rs orchestrator"
    - "CLI handler modules referenced via crate::cli:: path from main.rs dispatch"

key-files:
  created:
    - "crates/blufio/src/serve/mod.rs"
    - "crates/blufio/src/serve/storage.rs"
    - "crates/blufio/src/serve/channels.rs"
    - "crates/blufio/src/serve/gateway.rs"
    - "crates/blufio/src/serve/subsystems.rs"
    - "crates/blufio/src/cli/mod.rs"
    - "crates/blufio/src/cli/audit_cmd.rs"
    - "crates/blufio/src/cli/config_cmd.rs"
    - "crates/blufio/src/cli/injection_cmd.rs"
    - "crates/blufio/src/cli/memory_cmd.rs"
    - "crates/blufio/src/cli/nodes_cmd.rs"
    - "crates/blufio/src/cli/plugin_cmd.rs"
    - "crates/blufio/src/cli/skill_cmd.rs"
  modified:
    - "crates/blufio-gateway/src/handlers.rs"
    - "crates/blufio/src/providers.rs"
    - "crates/blufio/src/main.rs"
    - "crates/blufio/src/mcp_server.rs"

key-decisions:
  - "MCP client initialization kept inline in serve/mod.rs due to complex generic types that resist abstraction"
  - "init_gateway() takes individual webhook state parameters instead of whole ChannelInitResult to avoid borrow conflicts"
  - "Tests remain in main.rs referencing cli:: paths rather than moving to individual modules"

patterns-established:
  - "serve/ directory: orchestrator mod.rs calls init functions from sub-modules"
  - "cli/ directory: handler modules grouped by subcommand, referenced via cli::module::function()"
  - "Feature-gated webhook state parameters with #[cfg(feature)] on function parameters"

requirements-completed: [QUAL-03, QUAL-04, QUAL-05]

# Metrics
duration: 45min
completed: 2026-03-13
---

# Phase 63 Plan 01: Module Decomposition Summary

**Decomposed serve.rs (2331 lines) and main.rs (3220 lines) into focused directory modules, fixed hardcoded /api/status uptime, and removed unimplemented!() from mock providers**

## Performance

- **Duration:** ~45 min
- **Started:** 2026-03-13T14:10:00Z
- **Completed:** 2026-03-13T14:55:00Z
- **Tasks:** 2
- **Files modified:** 18

## Accomplishments
- serve.rs (2331 lines) decomposed into serve/ with 5 modules all under 870 lines
- main.rs (3220 lines) decomposed into main.rs (1667 lines) + cli/ with 7 handler modules all under 430 lines
- QUAL-03 fixed: /api/status returns actual uptime via HealthState.start_time.elapsed()
- QUAL-04 fixed: Mock provider unimplemented!() replaced with proper BlufioError::Internal returns
- Workspace compiles clean with cargo check, clippy shows only 5 pre-existing style warnings

## Task Commits

Each task was committed atomically:

1. **Task 1: Decompose serve.rs into serve/ directory** - `9d8d937` (refactor)
2. **Task 2: Decompose main.rs into cli/ directory** - `aad9ee6` (refactor)

## Files Created/Modified

### serve/ directory (Task 1)
- `crates/blufio/src/serve/mod.rs` (864 lines) - Orchestrator: run_serve, tracing init, agent loop
- `crates/blufio/src/serve/storage.rs` (226 lines) - DB init, cost tracking, tokenizer, memory system
- `crates/blufio/src/serve/channels.rs` (343 lines) - Channel adapter initialization (10 channels)
- `crates/blufio/src/serve/gateway.rs` (394 lines) - Gateway setup, provider registry, MCP transport
- `crates/blufio/src/serve/subsystems.rs` (765 lines) - Plugin, audit, resilience, tools, cron, hooks
- `crates/blufio/src/serve.rs` - DELETED (replaced by serve/ directory)

### cli/ directory (Task 2)
- `crates/blufio/src/cli/mod.rs` (15 lines) - Module declarations
- `crates/blufio/src/cli/audit_cmd.rs` (345 lines) - audit verify/tail/stats handlers
- `crates/blufio/src/cli/config_cmd.rs` (341 lines) - set-secret, list-secrets, config get, recipes
- `crates/blufio/src/cli/skill_cmd.rs` (426 lines) - skill init/list/install/remove/update/sign/keygen/verify/info
- `crates/blufio/src/cli/injection_cmd.rs` (197 lines) - injection test/status/config
- `crates/blufio/src/cli/plugin_cmd.rs` (130 lines) - plugin list/search/install/remove/update
- `crates/blufio/src/cli/nodes_cmd.rs` (111 lines) - nodes list/pair/remove/group/exec
- `crates/blufio/src/cli/memory_cmd.rs` (64 lines) - memory validate

### Bug fixes
- `crates/blufio-gateway/src/handlers.rs` - QUAL-03: uptime_secs uses actual elapsed time
- `crates/blufio/src/providers.rs` - QUAL-04: mock provider returns error instead of unimplemented!()
- `crates/blufio/src/mcp_server.rs` - Updated open_db reference to cli::config_cmd::open_db

## Decisions Made
- MCP client initialization kept inline in serve/mod.rs: the return type involves complex generics (`rmcp::service::RunningService<rmcp::RoleClient, rmcp::transport::TokioChildProcess>`) that cannot be cleanly abstracted into a function signature without importing rmcp at the serve/ level
- init_gateway() restructured to accept individual webhook state parameters instead of `&ChannelInitResult` to resolve Rust borrow checker conflict (cannot borrow `channel_result.mux` as mutable and `channel_result` as immutable simultaneously)
- Tests remain in main.rs rather than distributing to cli/ modules, referencing functions via `cli::module_name::function()` paths

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed QUAL-03: hardcoded uptime in /api/status**
- **Found during:** Task 1 (serve.rs decomposition)
- **Issue:** handlers.rs line 286 had `uptime_secs: 0, // TODO: track actual uptime`
- **Fix:** Changed to `uptime_secs: state.health.start_time.elapsed().as_secs()` (HealthState already had start_time field)
- **Files modified:** crates/blufio-gateway/src/handlers.rs
- **Committed in:** 9d8d937

**2. [Rule 1 - Bug] Fixed QUAL-04: unimplemented!() in mock provider**
- **Found during:** Task 1 (serve.rs decomposition)
- **Issue:** MockProvider in #[cfg(test)] used `unimplemented!("mock provider")` for complete() and stream()
- **Fix:** Replaced with `Err(BlufioError::Internal("mock provider: ... not implemented".to_string()))`
- **Files modified:** crates/blufio/src/providers.rs
- **Committed in:** 9d8d937

**3. [Rule 3 - Blocking] Fixed mcp_server.rs open_db reference**
- **Found during:** Task 2 (main.rs decomposition)
- **Issue:** mcp_server.rs referenced `crate::open_db` which was moved to cli::config_cmd
- **Fix:** Updated to `crate::cli::config_cmd::open_db`
- **Files modified:** crates/blufio/src/mcp_server.rs
- **Committed in:** aad9ee6

---

**Total deviations:** 3 auto-fixed (2 bugs, 1 blocking)
**Impact on plan:** All auto-fixes necessary for correctness. QUAL-03 and QUAL-04 were explicit plan requirements. The mcp_server.rs fix was a direct consequence of the decomposition.

## Issues Encountered
- Rust edition 2024 disallows `ref` in implicitly-borrowing patterns: removed explicit `ref` from `if let Some(ref x)` patterns in subsystems.rs and gateway.rs
- Borrow checker conflict with `&mut channel_result.mux` + `&channel_result`: resolved by restructuring init_gateway() parameters
- Disk space exhaustion during editing (target/ was 36GB): cleaned incremental build cache to recover space

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All 5 serve/ modules under 870 lines (orchestrator) and focused
- All 7 cli/ modules under 430 lines and focused
- Workspace compiles clean, all test binaries compile
- Ready for remaining phase 63 plans

## Self-Check: PASSED

- All 13 created files verified present on disk
- Both task commits (9d8d937, aad9ee6) verified in git log
- Workspace compiles clean (cargo check --workspace)

---
*Phase: 63-code-quality-hardening*
*Completed: 2026-03-13*
