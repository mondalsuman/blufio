---
phase: 38-migration-cli-utilities
plan: 02
subsystem: cli
tags: [bench, privacy, bundle, uninstall, config-recipe, tar, flate2, sysinfo, wasmtime]

# Dependency graph
requires:
  - phase: 38-01
    provides: Migration pipeline, config translate, V10 migration
provides:
  - bench command with startup/context/wasm/sqlite benchmarks and CI regression detection
  - privacy evidence-report with endpoint/store/skill enumeration and data classification
  - config recipe subcommand with personal/team/production/iot presets
  - uninstall command with process detection, auto-backup, and service removal
  - bundle command for air-gapped deployment with tar.gz archives
  - V11 bench_results migration for benchmark result storage
affects: [deployment, operations, monitoring]

# Tech tracking
tech-stack:
  added: [sysinfo, wasmtime (for bench), flate2, tar, libc]
  patterns: [cfg-gated storage access, platform-specific RSS measurement, config recipe generation]

key-files:
  created:
    - crates/blufio/src/bench.rs
    - crates/blufio/src/privacy.rs
    - crates/blufio/src/bundle.rs
    - crates/blufio/src/uninstall.rs
    - crates/blufio-storage/migrations/V11__bench_results.sql
  modified:
    - crates/blufio/src/main.rs
    - crates/blufio/Cargo.toml

key-decisions:
  - "SQLite storage ops in bench.rs gated behind #[cfg(feature = sqlite)] for graceful degradation"
  - "Peak RSS measurement via libc getrusage on macOS, /proc/self/status VmHWM on Linux"
  - "Bundle verifies binary signature before packaging but proceeds with warning if .minisig missing"
  - "Config recipe uses r##\"...\"## raw strings for TOML containing hash characters"
  - "Privacy report is static config analysis only, no server connection needed"

patterns-established:
  - "cfg-gated storage: bench save/load functions have sqlite and non-sqlite variants"
  - "recipe generation: inline commented TOML templates per preset, printed to stdout"

requirements-completed: [CLI-01, CLI-02, CLI-03, CLI-04, CLI-05]

# Metrics
duration: 11min
completed: 2026-03-07
---

# Phase 38 Plan 02: CLI Utilities Summary

**Five CLI utility commands: bench with SQLite storage and CI regression mode, privacy evidence-report with data classification, config recipe for four presets, uninstall with auto-backup and process detection, and bundle for air-gapped deployment archives**

## Performance

- **Duration:** 11 min
- **Started:** 2026-03-07T14:26:21Z
- **Completed:** 2026-03-07T14:38:11Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Built `blufio bench` with four built-in benchmarks (startup, context assembly, WASM, SQLite), system info header, table/JSON output, compare/baseline/CI modes, and SQLite result storage
- Built `blufio privacy evidence-report` with outbound endpoint enumeration (all 9 channel types + providers + MCP), local store listing, WASM skill permission breakdown, and data classification summary
- Built `blufio config recipe` generating commented TOML templates for personal, team, production, and IoT presets
- Built `blufio uninstall` with systemd/launchd process detection, shell completion cleanup, auto-backup before purge, and interactive data removal
- Built `blufio bundle` creating tar.gz archives with binary, sanitized config, WASM skills, manifest, and install.sh; verifies binary signature before bundling

## Task Commits

Each task was committed atomically:

1. **Task 1: Create bench and privacy modules with storage migration** - `48a0691` (feat)
2. **Task 2: Create bundle, uninstall, config recipe modules and wire into main.rs** - `de1105d` (feat)

## Files Created/Modified
- `crates/blufio/src/bench.rs` - Benchmarking with startup/context/wasm/sqlite, system info, result storage, compare/baseline/CI
- `crates/blufio/src/privacy.rs` - Privacy evidence report with endpoint/store/skill enumeration and data classification
- `crates/blufio/src/bundle.rs` - Air-gapped deployment bundle creation with tar.gz, manifest, install.sh
- `crates/blufio/src/uninstall.rs` - Clean uninstallation with process detection, service removal, auto-backup
- `crates/blufio-storage/migrations/V11__bench_results.sql` - Bench results table with benchmark/median/min/max/peak_rss/baseline columns
- `crates/blufio/src/main.rs` - Added Bench, Privacy, Bundle, Uninstall commands and PrivacyCommands/Recipe enums
- `crates/blufio/Cargo.toml` - Added sysinfo, wasmtime, flate2, tar, libc dependencies

## Decisions Made
- SQLite storage operations in bench.rs are feature-gated behind `#[cfg(feature = "sqlite")]` to compile gracefully without storage
- Peak RSS measurement uses `libc::getrusage` on macOS and `/proc/self/status` VmHWM on Linux, with None fallback
- Bundle command verifies binary Minisign signature before packaging but continues with warning if `.minisig` not found
- Config recipes use `r##"..."##` raw strings to accommodate TOML comments containing hash characters (e.g., `["#channel"]`)
- Privacy report performs static config analysis only -- no server connection required

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed raw string delimiter collision in config recipe**
- **Found during:** Task 2 (config recipe implementation)
- **Issue:** TOML templates containing `"#your-channel"` terminated `r#"..."#` raw strings prematurely
- **Fix:** Used `r##"..."##` delimiter for the production recipe containing hash characters
- **Files modified:** crates/blufio/src/main.rs
- **Verification:** `cargo check -p blufio` passes clean
- **Committed in:** de1105d (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Syntax fix required for correct Rust compilation. No scope creep.

## Issues Encountered
None beyond the raw string delimiter fix noted above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All CLI utility requirements (CLI-01 through CLI-05) are complete
- Phase 38 is fully complete (both plans executed)
- Ready for next milestone phase

---
*Phase: 38-migration-cli-utilities*
*Completed: 2026-03-07*
