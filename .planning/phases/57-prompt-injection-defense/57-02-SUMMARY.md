---
phase: 57-prompt-injection-defense
plan: 02
subsystem: security
tags: [injection-defense, hmac, hkdf, boundary-tokens, ring, content-zone-integrity]

# Dependency graph
requires:
  - phase: 57-prompt-injection-defense
    provides: "blufio-injection crate, SecurityEvent::BoundaryFailure on EventBus, HmacBoundaryConfig"
provides:
  - "BoundaryManager with HKDF-SHA256 per-session key derivation and HMAC signing"
  - "ZoneType enum (Static, Conditional, Dynamic) for content zone classification"
  - "BoundaryToken parse/display for <<BLUF-ZONE-v1:{zone}:{source}:{hex}>> format"
  - "validate_and_strip: HMAC verification with tampered zone stripping and SecurityEvent emission"
  - "re_sign for post-compaction/truncation content re-signing"
  - "Disabled mode passthrough when config.enabled=false"
affects: [57-03-PLAN, 57-04-PLAN]

# Tech tracking
tech-stack:
  added: []
  patterns: [hkdf-per-session-key-derivation, hmac-boundary-token-format, validate-strip-pipeline, timing-safe-hmac-verify]

key-files:
  created:
    - crates/blufio-injection/src/boundary.rs
  modified:
    - crates/blufio-injection/src/lib.rs

key-decisions:
  - "Regex uses non-greedy source capture (.+?) to handle colon-containing sources like mcp:server_name"
  - "HKDF expand uses hmac::HMAC_SHA256 (not &hmac::HMAC_SHA256) per ring 0.17 KeyType trait requirement"
  - "Token format uses hex encoding (64 chars) via hex crate already in workspace (not base64)"

patterns-established:
  - "HKDF per-session key derivation: Salt=session_id, IKM=master_key, Info=b'hmac-boundary'"
  - "Boundary token format: <<BLUF-ZONE-v1:{zone}:{source}:{hex64}>> with regex parsing"
  - "validate_and_strip pipeline: parse token pairs, verify HMAC, strip tokens, emit BoundaryFailure events"
  - "Disabled mode pattern: all methods check self.enabled flag and return passthrough when false"

requirements-completed: [INJC-03]

# Metrics
duration: 5min
completed: 2026-03-12
---

# Phase 57 Plan 02: HMAC Boundary Tokens Summary

**HMAC-SHA256 boundary token system with HKDF per-session keys, sign/verify/strip pipeline, tamper detection emitting SecurityEvent::BoundaryFailure, and disabled-mode passthrough**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-12T13:20:59Z
- **Completed:** 2026-03-12T13:26:15Z
- **Tasks:** 1
- **Files modified:** 2

## Accomplishments
- BoundaryManager with HKDF-SHA256 per-session key derivation from vault master key, producing deterministic session-isolated HMAC keys
- Full sign/verify/wrap/strip/validate_and_strip/re_sign API with timing-safe HMAC verification via ring::hmac::verify
- validate_and_strip parses token pairs from assembled context, verifies each zone's HMAC, strips tokens for clean LLM input, emits SecurityEvent::BoundaryFailure for tampered zones
- 27 tests covering key derivation determinism, session isolation, tamper detection (1-byte change), cross-session key rejection, multi-zone validation, source provenance preservation, and disabled mode passthrough

## Task Commits

Each task was committed atomically:

1. **Task 1: HMAC key derivation and boundary token sign/verify/strip** - `aa0cffe` (feat)

## Files Created/Modified
- `crates/blufio-injection/src/boundary.rs` - L3 HMAC boundary token system: BoundaryManager, BoundaryToken, BoundedContent, ZoneType, derive_session_key, validate_and_strip pipeline
- `crates/blufio-injection/src/lib.rs` - Added `pub mod boundary;` export

## Decisions Made
- Regex for boundary token parsing uses non-greedy source capture (`(.+?)`) to handle sources containing colons like "mcp:weather_server" and "skill:code_runner"
- HKDF expand call passes `hmac::HMAC_SHA256` without reference (ring 0.17 KeyType trait requires owned value, not reference)
- Hex encoding (64 chars for 32-byte HMAC tag) chosen over base64 since hex crate already in workspace; 64 chars is acceptable for tokens that get stripped before LLM sees context

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed ring 0.17 HKDF expand API call**
- **Found during:** Task 1 (initial compilation)
- **Issue:** Plan/research showed `&hmac::HMAC_SHA256` for expand's KeyType parameter, but ring 0.17 requires owned value (KeyType trait not implemented for references)
- **Fix:** Changed `&hmac::HMAC_SHA256` to `hmac::HMAC_SHA256` in derive_session_key
- **Files modified:** crates/blufio-injection/src/boundary.rs
- **Verification:** cargo check passes, all HKDF tests pass
- **Committed in:** aa0cffe (Task 1 commit)

**2. [Rule 1 - Bug] Fixed boundary token regex for colon-containing sources**
- **Found during:** Task 1 (test failure for wrap_content with "mcp:weather" source)
- **Issue:** Original regex `([^:]+)` for source field broke when source contained colons (e.g., "mcp:weather_server"). Token `<<BLUF-ZONE-v1:conditional:mcp:weather:{hex}>>` failed to parse because `[^:]+` stopped at the first colon in source
- **Fix:** Changed source capture group from `([^:]+)` to `(.+?)` (non-greedy), which correctly matches up to the colon before the 64-char hex tag
- **Files modified:** crates/blufio-injection/src/boundary.rs
- **Verification:** All 27 tests pass including sources with colons (mcp:weather_server, skill:code_runner)
- **Committed in:** aa0cffe (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (2 bugs)
**Impact on plan:** Both fixes necessary for correctness. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- L3 HMAC boundary token system complete, ready for L4 output screening (Plan 03)
- BoundaryManager ready for integration into ContextEngine::assemble() pipeline
- re_sign method ready for post-compaction content re-signing
- SecurityEvent::BoundaryFailure events ready for audit trail integration

## Self-Check: PASSED

All 1 created file verified on disk. Task commit (aa0cffe) verified in git log.

---
*Phase: 57-prompt-injection-defense*
*Completed: 2026-03-12*
