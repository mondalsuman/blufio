---
phase: 66-injection-defense-hardening
plan: 01
subsystem: security
tags: [unicode-normalization, injection-detection, regex, base64, confusable-mapping, multi-language]

# Dependency graph
requires:
  - phase: 57-injection-defense
    provides: "L1 pattern classifier with 3 categories, 11 patterns, LazyLock RegexSet"
provides:
  - "normalize.rs: NFKC normalization, zero-width stripping, confusable mapping, base64 decoding, content extraction"
  - "patterns.rs: 8 InjectionCategory variants, 38 patterns across 6 languages, language field on InjectionPattern"
  - "model.rs: severity_weights HashMap on InputDetectionConfig"
affects: [66-02-canary-tokens, 66-03-classifier-integration, 66-04-corpus-validation]

# Tech tracking
tech-stack:
  added: [unicode-normalization 0.1, base64 0.22]
  patterns: [normalization-pipeline, confusable-mapping-table, multi-language-phrase-patterns]

key-files:
  created:
    - crates/blufio-injection/src/normalize.rs
  modified:
    - crates/blufio-injection/Cargo.toml
    - crates/blufio-injection/src/lib.rs
    - crates/blufio-injection/src/patterns.rs
    - crates/blufio-config/src/model.rs
    - crates/blufio-injection/src/canary.rs
    - crates/blufio-injection/src/events.rs
    - crates/blufio-injection/src/metrics.rs
    - crates/blufio-injection/src/output_screen.rs
    - crates/blufio-injection/src/pipeline.rs

key-decisions:
  - "Confusable mapping table has ~60 entries covering Cyrillic/Greek uppercase+lowercase plus fullwidth Latin ranges"
  - "PATTERNS expanded to 38 entries (from 11) with 23 English + 15 multi-language patterns across 8 categories"
  - "EncodingEvasion has no static patterns -- triggered dynamically when decoded content matches another category"

patterns-established:
  - "Normalization pipeline: strip_zero_width -> NFKC -> map_confusables -> decode_base64"
  - "Multi-language patterns use phrase-level regex (not single words) to minimize false positives"
  - "extract_content produces per-segment results for independent scanning"

requirements-completed: [INJ-01, INJ-02, INJ-03, INJ-04, INJ-05, INJ-06]

# Metrics
duration: 9min
completed: 2026-03-13
---

# Phase 66 Plan 01: Normalization Pipeline and Extended Pattern Set Summary

**Unicode normalization pipeline with NFKC/confusable/base64 defense, 38 injection patterns across 8 categories and 6 languages, and configurable severity weights**

## Performance

- **Duration:** 9 min
- **Started:** 2026-03-13T21:47:20Z
- **Completed:** 2026-03-13T21:56:46Z
- **Tasks:** 2
- **Files modified:** 11

## Accomplishments
- Created normalize.rs (616 lines) with full normalization pipeline: zero-width stripping (7 chars + Unicode tags), NFKC normalization, confusable mapping (~60 Cyrillic/Greek/fullwidth entries), base64 segment detection and decoding, and content extraction from HTML comments, markdown fences, and JSON values
- Expanded InjectionCategory enum from 3 to 8 variants (added PromptLeaking, Jailbreak, DelimiterManipulation, IndirectInjection, EncodingEvasion) with snake_case Display impl
- Expanded PATTERNS array from 11 to 38 entries across 6 languages (EN/FR/DE/ES/ZH/JA)
- Added `language: &'static str` field to InjectionPattern struct
- Added `severity_weights: HashMap<String, f64>` with `#[serde(default)]` to InputDetectionConfig
- All 183 blufio-injection tests pass, all 91 blufio-config tests pass, clippy clean, fmt clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Create normalize.rs normalization pipeline module** - `d865edd` (feat)
2. **Task 2: Expand patterns.rs with 5 new categories, language field, and ~38 patterns** - `6d40075` (feat)

## Files Created/Modified
- `crates/blufio-injection/src/normalize.rs` - Full normalization pipeline: NFKC, zero-width strip, confusable mapping, base64 decode, content extraction
- `crates/blufio-injection/src/patterns.rs` - 8 InjectionCategory variants, 38 patterns with language field, multi-language support
- `crates/blufio-injection/Cargo.toml` - Added unicode-normalization and base64 dependencies
- `crates/blufio-injection/src/lib.rs` - Declared normalize and canary modules
- `crates/blufio-config/src/model.rs` - severity_weights HashMap on InputDetectionConfig
- `crates/blufio-injection/src/canary.rs` - Clippy fixes (collapsible_if)
- `crates/blufio-injection/src/events.rs` - CanaryDetection event constructor (Phase 66 scaffolding)
- `crates/blufio-injection/src/metrics.rs` - Canary counter, scan duration histogram, category label on input detection
- `crates/blufio-injection/src/output_screen.rs` - Canary token integration in output screener (Phase 66 scaffolding)
- `crates/blufio-injection/src/pipeline.rs` - Category label in metrics recording
- `Cargo.lock` - Updated for new dependencies

## Decisions Made
- Confusable mapping table sized at ~60 entries (Cyrillic uppercase+lowercase, Greek uppercase+lowercase, plus fullwidth Latin ranges via char arithmetic) -- sufficient for common evasion without full TR39 complexity
- PATTERNS expanded to 38 total (exceeding ~25 minimum): 23 English patterns + 15 multi-language patterns (3 per language x 5 languages)
- EncodingEvasion has no static patterns in PATTERNS array -- it is triggered dynamically in the classifier when decoded/extracted content matches another category
- Multi-language patterns use phrase-level regex to prevent false positives (e.g., `ignorez les instructions precedentes` not just `instructions`)
- Pre-existing Phase 66 scaffolding (canary events, metrics, output screener) included in Task 1 commit since it was already present but uncommitted

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Pre-existing uncommitted Phase 66 scaffolding**
- **Found during:** Task 1
- **Issue:** Files modified by prior context/research sessions (canary events, metrics expansion, output screener canary integration, pipeline category label) were present but uncommitted, and needed to be included for compilation
- **Fix:** Included all pre-existing changes in Task 1 commit as they represent Phase 66 infrastructure scaffolding
- **Files modified:** events.rs, metrics.rs, pipeline.rs, output_screen.rs (blufio-injection), events.rs (blufio-bus)
- **Verification:** All 183 tests pass
- **Committed in:** d865edd (Task 1 commit)

**2. [Rule 1 - Bug] Clippy collapsible_if warnings**
- **Found during:** Task 2 (post-implementation verification)
- **Issue:** Nested if-let patterns in normalize.rs and canary.rs flagged by clippy as collapsible
- **Fix:** Collapsed nested if-let expressions using let-chain syntax
- **Files modified:** normalize.rs, canary.rs
- **Verification:** cargo clippy -p blufio-injection -- -D warnings passes clean
- **Committed in:** 6d40075 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both auto-fixes necessary for build correctness and CI cleanliness. No scope creep.

## Issues Encountered
None - all tasks executed smoothly.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- normalize.rs module ready for classifier integration (Plan 03 will wire normalize() into classify() pre-pass)
- Expanded pattern set ready for dual-scan (original + normalized) in Plan 03
- severity_weights config field ready for weight multiplication in score calculation (Plan 03)
- Canary module stub and scaffolding in place for Plan 02 to build upon

## Self-Check: PASSED

All created files exist, all commit hashes verified.

---
*Phase: 66-injection-defense-hardening*
*Completed: 2026-03-13*
