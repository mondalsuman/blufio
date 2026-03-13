---
phase: 63-code-quality-hardening
plan: 03
subsystem: code-quality
tags: [clippy, unwrap-sweep, error-handling, rust-linting, code-hardening]

# Dependency graph
requires:
  - phase: 63-01
    provides: "Decomposed serve/ and cli/ modules for easier sweep"
provides:
  - "cfg_attr(not(test), deny(clippy::unwrap_used)) enforced in all 43 library crates"
  - "Zero unwrap() calls in non-test production code across all library crates"
  - "Clean cargo clippy --workspace --all-targets -- -D warnings"
  - "QUAL-01 and QUAL-02 fully complete"
affects: [ci-pipeline]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "cfg_attr(not(test), deny(clippy::unwrap_used)) in every library crate lib.rs"
    - "Regex::new() on static literals uses .expect('valid regex: name') pattern"
    - "Iterator invariants documented with .expect('reason checked above')"
    - "HTTP header values from numeric .to_string() use .expect('valid header: description')"

key-files:
  created: []
  modified:
    - "crates/*/src/lib.rs (43 library crates -- deny directive added)"
    - "crates/blufio-gateway/src/rate_limit.rs (response builder/header unwraps)"
    - "crates/blufio-security/src/pii.rs (regex and Luhn algorithm unwraps)"
    - "crates/blufio-security/src/redact.rs (secret pattern regex unwraps)"
    - "crates/blufio-gemini/src/stream.rs (JSON parser obj_start unwrap)"
    - "crates/blufio-telegram/src/markdown.rs (peekable char iterator unwraps)"
    - "crates/blufio-slack/src/handler.rs (mention regex unwrap)"
    - "crates/blufio-slack/src/markdown.rs (markdown conversion regex unwraps)"
    - "crates/blufio/src/serve/gateway.rs (collapsible return statements)"
    - "crates/blufio/src/serve/subsystems.rs (collapsible if statements)"

key-decisions:
  - "cfg_attr(not(test), deny()) instead of plain deny() to allow test code to keep unwrap()"
  - "All 43 library crates covered (15 core + 22 remaining + 6 Plan-02 crates with uncommitted directive)"
  - "blufio-test-utils included in sweep even though plan didn't list it explicitly"
  - "Pre-existing binary crate clippy warnings fixed inline (collapsible_if, unneeded return)"

patterns-established:
  - "cfg_attr(not(test), deny(clippy::unwrap_used)) at crate level for CI compatibility"
  - "Regex literal unwraps replaced with expect('valid regex: pattern_name')"
  - "Option invariants use expect('reason') with inline comment documenting the guarantee"

requirements-completed: [QUAL-01, QUAL-02]

# Metrics
duration: 23min
completed: 2026-03-13
---

# Phase 63 Plan 03: Remaining Unwrap Sweep Summary

**Enforced deny(clippy::unwrap_used) across all 43 library crates with cfg_attr(not(test)) for CI-compatible test code, replacing all production unwrap() calls with expect() or error propagation**

## Performance

- **Duration:** ~23 min
- **Started:** 2026-03-13T14:58:53Z
- **Completed:** 2026-03-13T15:22:00Z
- **Tasks:** 2
- **Files modified:** 56

## Accomplishments
- All 43 library crates now have `#![cfg_attr(not(test), deny(clippy::unwrap_used))]`
- Zero clippy warnings across entire workspace with `--all-targets -- -D warnings`
- All workspace tests pass
- QUAL-01 (unwrap elimination) and QUAL-02 (clippy deny directive) fully complete
- Also committed 6 Plan-02 crates (audit, config, vault, skill, storage, memory) that had the directive from a prior incomplete execution

## Task Commits

Each task was committed atomically:

1. **Task 1: Unwrap sweep -- core infrastructure crates (15 crates)** - `7125e1b` (feat)
2. **Task 2: Unwrap sweep -- provider + channel + remaining crates** - `6b2058c` (feat)

## Files Created/Modified

### Task 1: 15 core infrastructure crates
- `crates/blufio-agent/src/lib.rs` - deny directive added
- `crates/blufio-bus/src/lib.rs` - deny directive added
- `crates/blufio-context/src/lib.rs` - deny directive added
- `crates/blufio-core/src/lib.rs` - deny directive added
- `crates/blufio-cost/src/lib.rs` - deny directive added
- `crates/blufio-cron/src/lib.rs` - deny directive added
- `crates/blufio-gateway/src/lib.rs` - deny directive added
- `crates/blufio-gateway/src/rate_limit.rs` - 7 unwrap() replaced with expect()
- `crates/blufio-gdpr/src/lib.rs` - deny directive added
- `crates/blufio-hooks/src/lib.rs` - deny directive added
- `crates/blufio-injection/src/lib.rs` - deny directive added
- `crates/blufio-mcp-client/src/lib.rs` - deny directive added
- `crates/blufio-mcp-server/src/lib.rs` - deny directive added
- `crates/blufio-node/src/lib.rs` - deny directive added
- `crates/blufio-plugin/src/lib.rs` - deny directive added
- `crates/blufio-resilience/src/lib.rs` - deny directive added

### Task 2: remaining crates + fixes
- 22 additional lib.rs files with deny directive (providers, channels, misc)
- 6 Plan-02 crate lib.rs files updated from deny() to cfg_attr(not(test), deny())
- All 15 Task-1 lib.rs files updated from deny() to cfg_attr(not(test), deny())
- `crates/blufio-security/src/pii.rs` - 4 unwrap() replaced (3 regex, 1 Luhn digit)
- `crates/blufio-security/src/redact.rs` - 4 unwrap() replaced (regex patterns)
- `crates/blufio-gemini/src/stream.rs` - 1 unwrap() replaced (obj_start invariant)
- `crates/blufio-telegram/src/markdown.rs` - 3 unwrap() replaced (char iterator)
- `crates/blufio-slack/src/handler.rs` - 1 unwrap() replaced (mention regex)
- `crates/blufio-slack/src/markdown.rs` - 7 unwrap() replaced (markdown regex patterns)
- `crates/blufio/src/serve/gateway.rs` - 3 unneeded returns removed
- `crates/blufio/src/serve/subsystems.rs` - 2 collapsible if blocks collapsed

## Decisions Made
- **cfg_attr(not(test)) wrapping:** Using `#![cfg_attr(not(test), deny(clippy::unwrap_used))]` instead of plain `#![deny(clippy::unwrap_used)]` because CI runs `cargo clippy --all-targets -- -D warnings` which compiles test targets with the crate-level attribute. The conditional form ensures production code is enforced while test code can still use unwrap().
- **Plan-02 crate inclusion:** 6 Plan-02 crates (audit, config, vault, skill, storage, memory) had the deny directive added by a prior incomplete execution but were never committed. Included them in this commit for completeness.
- **blufio-test-utils inclusion:** Added the deny directive even though it wasn't listed in the plan, since the plan goal is "every library crate."

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed cfg_attr wrapping for CI compatibility**
- **Found during:** Task 2 (workspace-wide verification)
- **Issue:** Plain `#![deny(clippy::unwrap_used)]` fails with `--all-targets` because test modules use unwrap() and the deny is crate-level
- **Fix:** Changed to `#![cfg_attr(not(test), deny(clippy::unwrap_used))]` across all 43 crates
- **Files modified:** All 43 lib.rs files
- **Committed in:** 6b2058c

**2. [Rule 3 - Blocking] Fixed pre-existing clippy warnings in binary crate**
- **Found during:** Task 2 (workspace-wide verification)
- **Issue:** `cargo clippy --workspace --all-targets -- -D warnings` failed on binary crate with 3 `unneeded return` and 2 `collapsible_if` warnings, blocking verification
- **Fix:** Removed unnecessary returns in gateway.rs, collapsed nested ifs in subsystems.rs
- **Files modified:** crates/blufio/src/serve/gateway.rs, crates/blufio/src/serve/subsystems.rs
- **Committed in:** 6b2058c

**3. [Rule 2 - Missing Critical] Added Plan-02 uncommitted work**
- **Found during:** Task 2 (pre-flight check)
- **Issue:** 6 Plan-02 crates had deny directives from a prior execution but were never committed
- **Fix:** Included and updated them alongside Task 2 changes
- **Files modified:** 6 lib.rs files (audit, config, vault, skill, storage, memory)
- **Committed in:** 6b2058c

---

**Total deviations:** 3 auto-fixed (1 bug, 1 blocking, 1 missing critical)
**Impact on plan:** All fixes necessary for correct CI behavior. The cfg_attr wrapping is the standard Rust approach. No scope creep.

## Issues Encountered
- Test code in inline `#[cfg(test)] mod tests` blocks uses unwrap() extensively; plain `#![deny]` would require modifying every test module. Resolved by using `cfg_attr(not(test))` conditional.
- Pre-existing binary crate clippy warnings blocked workspace-wide verification. Fixed inline.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All 43 library crates enforced with deny(clippy::unwrap_used) via cfg_attr
- Workspace clippy passes clean with `--all-targets -- -D warnings`
- All existing tests pass
- Ready for remaining phase 63 plans (benchmarks, integration tests)

## Self-Check: PASSED

- 43 library crates verified with deny directive
- Both task commits verified in git log (7125e1b, 6b2058c)
- cargo clippy --workspace --all-targets -- -D warnings passes clean
- Binary crate verified: no deny directive present

---
*Phase: 63-code-quality-hardening*
*Completed: 2026-03-13*
