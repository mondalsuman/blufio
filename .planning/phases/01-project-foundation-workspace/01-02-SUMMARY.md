---
phase: 01-project-foundation-workspace
plan: 02
subsystem: config
tags: [toml, figment, miette, serde, deny-unknown-fields, fuzzy-matching, strsim, xdg]

requires:
  - phase: 01-01
    provides: Cargo workspace with blufio-config crate stub
provides:
  - TOML config system with deny_unknown_fields on all structs
  - Figment-based layered config loading (XDG hierarchy + env var overrides)
  - Figment-to-miette error bridge with fuzzy match typo suggestions
  - Post-deserialization validation for config values
  - Binary startup config loading with diagnostic error rendering
affects: [channel-adapters, provider-adapters, storage-adapters, agent-startup]

tech-stack:
  added: [figment, miette, strsim, dirs]
  patterns: [deny-unknown-fields-on-all-config-structs, env-map-not-split, figment-to-miette-bridge, jaro-winkler-fuzzy-matching]

key-files:
  created:
    - crates/blufio-config/src/model.rs
    - crates/blufio-config/src/loader.rs
    - crates/blufio-config/src/diagnostic.rs
    - crates/blufio-config/src/validation.rs
    - crates/blufio-config/tests/config_tests.rs
  modified:
    - crates/blufio-config/src/lib.rs
    - crates/blufio/src/main.rs

key-decisions:
  - "Used Env::map() NOT Env::split() for environment variable mapping to avoid underscore ambiguity (Pitfall 5)"
  - "Jaro-Winkler threshold 0.75 for fuzzy matching (catches typos like naem->name, bot_tken->bot_token)"
  - "Made CLI command optional (Option<Commands>) for cleaner startup with config-only validation"
  - "miette derive warnings from proc macro expansion are harmless -- miette's internal destructuring pattern"

patterns-established:
  - "deny_unknown_fields on ALL config structs, no exceptions"
  - "No serde(flatten) anywhere in config system (incompatible with deny_unknown_fields)"
  - "Env::map() with explicit section-to-dot replacen() for BLUFIO_ prefix mapping"
  - "load_and_validate() as high-level config entry point combining loader + validation + diagnostics"

requirements-completed: [CLI-06]

duration: 8min
completed: 2026-02-28
---

# Plan 01-02: TOML Config System Summary

**Strict TOML config with deny_unknown_fields, Jaro-Winkler typo suggestions, miette diagnostic rendering, XDG file hierarchy, and BLUFIO_ env var overrides using Env::map()**

## Performance

- **Duration:** 8 min
- **Started:** 2026-02-28T21:21:46Z
- **Completed:** 2026-02-28T21:30:22Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Config model with 7 structs (BlufioConfig + 6 sections), all with `deny_unknown_fields`
- Figment layered loader: defaults < /etc/blufio/ < ~/.config/blufio/ < ./blufio.toml < BLUFIO_* env vars
- Figment-to-miette error bridge converting UnknownField/MissingField/InvalidType to rich diagnostics
- Jaro-Winkler fuzzy matching for typo suggestions (threshold 0.75) with valid key listings
- Post-deserialization validation (IP addresses, non-empty paths, non-negative budgets)
- Binary startup wired to load config, render miette diagnostics on error, exit(1) on failure
- 30 tests total across unit and integration test suites

## Task Commits

Each task was committed atomically:

1. **Task 1: Config model structs and figment loader with XDG hierarchy** - `53b1fe0` (feat)
2. **Task 2: Figment-to-miette error bridge with fuzzy suggestions and binary wiring** - `704042b` (feat)

_Note: TDD tasks -- tests written alongside implementation, verified via cargo test_

## Files Created/Modified
- `crates/blufio-config/src/model.rs` - Config structs with deny_unknown_fields and serde defaults
- `crates/blufio-config/src/loader.rs` - Figment-based config assembly with XDG lookup and Env::map()
- `crates/blufio-config/src/diagnostic.rs` - Figment-to-miette error bridge with Jaro-Winkler suggestions
- `crates/blufio-config/src/validation.rs` - Post-deserialization validation for config values
- `crates/blufio-config/src/lib.rs` - Module exports and load_and_validate() entry point
- `crates/blufio-config/tests/config_tests.rs` - 21 integration tests for config system
- `crates/blufio/src/main.rs` - Binary startup with config loading and diagnostic error rendering

## Decisions Made
- Used `Env::map()` with explicit `replacen()` for section-to-dot mapping instead of `Env::split("_")` to avoid the underscore ambiguity pitfall where `BLUFIO_TELEGRAM_BOT_TOKEN` would incorrectly map to `telegram.bot.token`
- Set Jaro-Winkler threshold at 0.75 (lower than typical 0.8) to catch more useful suggestions like `max_sesions` -> `max_sessions`
- Made CLI `command` field `Option<Commands>` so the binary can start cleanly without a subcommand, enabling config-only validation testing
- ConfigError uses miette's `#[diagnostic]` derive for all variants, enabling rich ANSI rendering with source spans and help text

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed Rust 2024 edition lifetime syntax in generic impl**
- **Found during:** Task 2 compilation
- **Issue:** `impl<I: Iterator<Item = &str>>` requires explicit lifetime annotation in Rust 2024 edition (E0637)
- **Fix:** Replaced ZipOffsets trait approach with direct byte offset tracking in a for loop
- **Files modified:** `crates/blufio-config/src/diagnostic.rs`
- **Verification:** `cargo test -p blufio-config` passes

**2. [Rule 3 - Blocking] Fixed figment::Kind::UnknownField expected field type**
- **Found during:** Task 2 compilation
- **Issue:** `expected` is `&'static [&'static str]`, not `Vec<Uncased>` -- used `.as_str()` on wrong type (E0658)
- **Fix:** Used `.to_vec()` directly on the static slice since elements are already `&str`
- **Files modified:** `crates/blufio-config/src/diagnostic.rs`
- **Verification:** `cargo test -p blufio-config` passes

**3. [Rule 3 - Blocking] Fixed miette render_report trait bound**
- **Found during:** Task 2 compilation
- **Issue:** `ConfigError.as_ref()` doesn't satisfy `render_report`'s `&dyn Diagnostic` parameter (E0599)
- **Fix:** Explicit cast: `let diagnostic: &dyn Diagnostic = error;`
- **Files modified:** `crates/blufio-config/src/diagnostic.rs`
- **Verification:** `cargo test -p blufio-config` passes

---

**Total deviations:** 3 auto-fixed (all compilation/type errors in diagnostic module)
**Impact on plan:** Standard Rust type system corrections. No scope creep.

## Issues Encountered
- miette derive macro produces spurious "value assigned but never read" warnings for struct fields used in format strings within `#[diagnostic(help(...))]` attributes. These are false positives from the macro expansion and do not affect correctness.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Config system is complete and ready for all future phases to use
- All adapter implementations (Telegram, Anthropic, Storage) can add their config sections
- Binary loads config at startup -- future phases wire config into their initialization
- Error rendering provides Elm-style diagnostic output for any config typos

## Self-Check: PASSED

All 7 created/modified files verified present. Both task commits (53b1fe0, 704042b) verified in git log. 38 tests pass across workspace.

---
*Plan: 01-02-project-foundation-workspace*
*Completed: 2026-02-28*
