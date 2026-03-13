---
phase: 66-injection-defense-hardening
plan: 03
subsystem: security
tags: [injection-defense, normalization, severity-weights, canary, cli, unicode, base64]

# Dependency graph
requires:
  - phase: 66-01
    provides: "normalize.rs normalization pipeline, expanded pattern set with 8 categories and multi-language support"
  - phase: 66-02
    provides: "canary.rs CanaryTokenManager, output screening, scan duration and detection metrics"
provides:
  - "Full L1 classifier with normalization pre-pass, dual scan, severity weight multiplication"
  - "Pipeline with scan duration recording, CanaryTokenManager integration, screen_llm_response delegation"
  - "CLI test-canary and validate-corpus subcommands"
  - "Enhanced CLI test/status/config output with normalization details and weights"
  - "Doctor canary self-test integration"
affects: [66-04, injection-defense, mcp-client, audit]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Dual-scan pattern matching (original + normalized text with HashSet deduplication)"
    - "Severity weight multiplication with validation (NaN/negative -> warn + 1.0, cap at 3.0)"
    - "Evasion bonus scoring (+0.1 zero-width, +0.1 confusable, additive)"
    - "Base64 decoded content re-scanning with EncodingEvasion category injection"

key-files:
  created: []
  modified:
    - "crates/blufio-injection/src/classifier.rs"
    - "crates/blufio-injection/src/pipeline.rs"
    - "crates/blufio/src/cli/injection_cmd.rs"
    - "crates/blufio/src/main.rs"
    - "crates/blufio/src/doctor.rs"
    - "contrib/blufio.example.toml"
    - "crates/blufio-audit/src/subscriber.rs"
    - "crates/blufio-mcp-client/src/manager.rs"
    - "crates/blufio-mcp-client/src/external_tool.rs"

key-decisions:
  - "Deduplication by (pattern_index, matched_text) tuple rather than (pattern_index, span) to handle different span offsets in original vs normalized text"
  - "Evasion bonus added independently of category weights -- always additive on top of weighted score"
  - "Pipeline scan_input uses synchronous Instant timing (not async timeout) since classify() is CPU-bound regex matching"
  - "Canary self-test integrated into doctor as pass/fail alongside existing HMAC self-test"

patterns-established:
  - "Dual-scan with dedup: normalize input, scan both original and normalized, merge via HashSet<(usize, String)>"
  - "Weight validation: NaN/negative warn + fallback to 1.0, 0.0 disables, 3.0 cap"
  - "Per-category metrics: record_input_detection(source, action, category) with loop over all matched categories"

requirements-completed: [INJ-01, INJ-02, INJ-03, INJ-04, INJ-05, INJ-06, INJ-07]

# Metrics
duration: 14min
completed: 2026-03-13
---

# Phase 66 Plan 03: Pipeline Integration Summary

**Normalization pre-pass with dual scan, severity weight multiplication, evasion bonuses, and CLI/doctor canary integration wired into the full L1 injection defense pipeline**

## Performance

- **Duration:** 14 min
- **Started:** 2026-03-13T22:01:01Z
- **Completed:** 2026-03-13T22:15:21Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments
- Classifier normalizes input before scanning, scans both original and normalized text with match deduplication, re-scans base64 decoded segments
- Severity weights from config multiply base severity per category (0.0 disables, 3.0 cap, invalid values warn + fallback to 1.0)
- Evasion bonuses (+0.1 zero-width chars, +0.1 confusable chars) applied independently on top of weighted score
- Pipeline records scan duration metric and integrates CanaryTokenManager with OutputScreener
- CLI test-canary and validate-corpus subcommands added; test/status/config enhanced with normalization details, weighted severity, per-language/category counts
- Doctor includes canary self-test alongside HMAC self-test

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire normalization + severity weights into classifier and pipeline** - `16c5dc5` (feat, TDD)
2. **Task 2: CLI subcommands, doctor canary check, and example TOML** - `1fa7638` (feat)

## Files Created/Modified
- `crates/blufio-injection/src/classifier.rs` - Added normalization pre-pass, dual scan with dedup, severity weight multiplication, evasion bonus, 7 new tests
- `crates/blufio-injection/src/pipeline.rs` - Added CanaryTokenManager field, scan duration recording, per-category metrics, new_session/canary_line/screen_llm_response methods
- `crates/blufio/src/cli/injection_cmd.rs` - Enhanced test output (normalization, weighted severity, language), added test-canary and validate-corpus handlers, enhanced status (per-language/category counts) and config (effective weights)
- `crates/blufio/src/main.rs` - Added TestCanary and ValidateCorpus variants to InjectionCommands enum
- `crates/blufio/src/doctor.rs` - Added canary self-test to check_injection_defense
- `contrib/blufio.example.toml` - Added severity_weights documentation section
- `crates/blufio-audit/src/subscriber.rs` - Added CanaryDetection match arm (Rule 3 auto-fix)
- `crates/blufio-mcp-client/src/manager.rs` - Updated record_input_detection to 3-arg signature (Rule 3 auto-fix)
- `crates/blufio-mcp-client/src/external_tool.rs` - Updated record_input_detection to 3-arg signature (Rule 3 auto-fix)

## Decisions Made
- Used `(pattern_index, matched_text)` as deduplication key instead of `(pattern_index, span)` since spans differ between original and normalized text
- Evasion bonus is additive and independent of severity weights -- it applies even if all category weights are reduced
- Pipeline uses synchronous `Instant::now()` timing rather than async timeout since `classify()` is CPU-bound regex matching
- Canary self-test wired as pass/fail check alongside existing HMAC self-test in doctor

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Non-exhaustive match for CanaryDetection in audit subscriber**
- **Found during:** Task 2 (building blufio binary)
- **Issue:** `SecurityEvent::CanaryDetection` added in Plan 02 was not matched in `convert_to_pending_entry()` in audit subscriber, causing non-exhaustive match error
- **Fix:** Added match arm handling `CanaryDetection { correlation_id, token_type, action }` with appropriate audit entry creation
- **Files modified:** `crates/blufio-audit/src/subscriber.rs`
- **Verification:** Build compiles cleanly
- **Committed in:** `1fa7638` (Task 2 commit)

**2. [Rule 3 - Blocking] record_input_detection signature mismatch in MCP manager**
- **Found during:** Task 2 (building blufio binary)
- **Issue:** Plan 02 added `category` parameter to `record_input_detection()` but MCP manager still used 2-arg signature
- **Fix:** Updated call to extract first category from scan result and pass as third argument
- **Files modified:** `crates/blufio-mcp-client/src/manager.rs`
- **Verification:** Build compiles cleanly
- **Committed in:** `1fa7638` (Task 2 commit)

**3. [Rule 3 - Blocking] record_input_detection signature mismatch in MCP external_tool**
- **Found during:** Task 2 (building blufio binary)
- **Issue:** Same 2-arg vs 3-arg mismatch as above in external_tool.rs
- **Fix:** Updated call to extract first category from scan result and pass as third argument
- **Files modified:** `crates/blufio-mcp-client/src/external_tool.rs`
- **Verification:** Build compiles cleanly
- **Committed in:** `1fa7638` (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (3 blocking - Rule 3)
**Impact on plan:** All auto-fixes were necessary for compilation. Cross-crate API changes from Plans 01/02 required downstream callers to be updated. No scope creep.

## Issues Encountered
- Rust 2024 edition implicit borrow rules: `ref custom_set` pattern required removal since pattern matching on reference types implicitly borrows in the new edition. Resolved by removing `ref` keyword.
- `CanaryTokenManager` private fields prevented struct literal construction with field spread -- resolved by using `CanaryTokenManager::new()` directly.
- `suspicious_double_ref_op` clippy warning on `cat.clone()` with double reference -- resolved with `(*cat).clone()`.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Full L1 pipeline integration complete with normalization, severity weights, evasion bonuses, and canary detection
- Plan 04 (integration tests and stress testing) can proceed -- all interfaces are wired and functional
- CLI provides complete diagnostic tooling for testing and validating injection defense behavior

## Self-Check: PASSED

- All 9 modified files verified on disk
- All 2 task commits verified in git history (16c5dc5, 1fa7638)
- 190 tests pass, clippy clean, build clean

---
*Phase: 66-injection-defense-hardening*
*Completed: 2026-03-13*
