---
phase: 07-wasm-skill-sandbox
plan: 04
subsystem: skill
tags: [wasm, wasmtime, tool-registry, tool-use, sandbox, capability-gating, ssrf]

# Dependency graph
requires:
  - phase: 07-wasm-skill-sandbox (plans 01-03)
    provides: ToolRegistry, SkillProvider, WasmSkillRuntime, sandbox host functions
provides:
  - shell.rs ToolRegistry and SkillProvider wiring (matching serve.rs)
  - shell.rs tool_use/tool_result loop for interactive tool execution
  - Real WASM host function implementations (http_request, read_file, write_file)
  - Capability-denied traps (not error codes) for sandbox enforcement
affects: [08-testing, 09-production-hardening]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Capability-denied WASM host functions trap via Err(anyhow!()) instead of returning -1"
    - "tokio Handle::block_on for sync HTTP requests inside spawn_blocking context"
    - "Domain allowlist validation in WASM http_request host function"
    - "Path prefix validation for WASM filesystem host functions"

key-files:
  created: []
  modified:
    - crates/blufio/src/shell.rs
    - crates/blufio-skill/src/sandbox.rs

key-decisions:
  - "Used Handle::current().block_on() for HTTP in WASM host functions instead of reqwest::blocking to avoid adding blocking feature to workspace reqwest"
  - "HTTP response body stored in result_json for skill access (pragmatic WASM memory management)"
  - "Domain validation uses exact match or subdomain match (ends_with .domain) pattern"
  - "Path validation uses starts_with prefix check against manifest-declared paths"

patterns-established:
  - "WASM capability denial pattern: Err(anyhow!('capability not permitted: ...').into()) for wasmtime trap"
  - "Shell tool loop mirrors agent loop: consume stream, check tool_use, execute tools, re-call LLM"

requirements-completed: [SEC-05, SEC-06, SKILL-01, SKILL-02, SKILL-03, SKILL-04, SKILL-05, SKILL-06]

# Metrics
duration: 11min
completed: 2026-03-01
---

# Phase 7 Plan 4: Gap Closure Summary

**ToolRegistry/tool_use loop wired into shell.rs; WASM host functions http_request/read_file/write_file upgraded from stubs to real implementations with capability traps**

## Performance

- **Duration:** 11 min
- **Started:** 2026-03-01T19:52:15Z
- **Completed:** 2026-03-01T20:03:15Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- shell.rs now has full tool support matching serve.rs: ToolRegistry, SkillProvider, tool_use/tool_result loop
- WASM host functions trap on denied capabilities (security improvement over returning -1)
- http_request makes real HTTP requests with domain validation and SSRF prevention
- read_file and write_file perform real filesystem operations with path prefix validation
- 8 new tests verifying capability denial traps, real file I/O, and domain validation

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire ToolRegistry and tool_use loop into shell.rs** - `00a5763` (feat)
2. **Task 2: Implement real WASM host functions and use traps for denied capabilities** - `97d6915` (feat)

## Files Created/Modified
- `crates/blufio/src/shell.rs` - Added ToolRegistry initialization, SkillProvider wiring, tool_use/tool_result loop with cost tracking per iteration
- `crates/blufio-skill/src/sandbox.rs` - Replaced stub host functions with real implementations; capability denial now traps instead of returning -1; added 8 tests

## Decisions Made
- Used `Handle::current().block_on()` for HTTP in WASM host functions instead of `reqwest::blocking` -- avoids adding the `blocking` feature to the workspace `reqwest` dependency, and works correctly because host functions execute inside `spawn_blocking` where the tokio handle is available
- HTTP response body stored in `result_json` (same mechanism as `set_output`) -- pragmatic approach to WASM memory management without complex guest-side allocation
- Domain validation uses exact match or subdomain match (`ends_with(".{domain}")`) -- matches common API domain patterns
- Path validation uses `starts_with` prefix check against manifest-declared paths -- simple and secure

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed missing `mut` on write_file closure parameter**
- **Found during:** Task 2
- **Issue:** `caller.get_export("memory")` requires `&mut Caller` but the write_file closure had an immutable `caller` parameter
- **Fix:** Changed `caller: Caller<...>` to `mut caller: Caller<...>` in the write_file closure
- **Files modified:** `crates/blufio-skill/src/sandbox.rs`
- **Verification:** `cargo check --workspace` compiles clean
- **Committed in:** `97d6915` (part of Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Minor syntax fix, no scope change.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 7 gap closure complete: both shell.rs wiring and WASM host function implementations are done
- All workspace tests pass (64 blufio-skill tests including 8 new sandbox capability tests)
- Ready for Phase 8 testing or Phase 9 production hardening

---
*Phase: 07-wasm-skill-sandbox*
*Completed: 2026-03-01*
