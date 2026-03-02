---
phase: 10-multi-agent-final-integration
plan: 01
subsystem: auth
tags: [ed25519, signing, keypair, agent-message, config]

requires:
  - phase: 08-plugin-system-gateway
    provides: Ed25519 DeviceKeypair foundation (generate, public key export)
provides:
  - Ed25519 sign/verify methods on DeviceKeypair
  - AgentMessage and SignedAgentMessage types for inter-agent communication
  - AgentSpecConfig and DelegationConfig in BlufioConfig
affects: [10-03-delegation-router, multi-agent]

tech-stack:
  added: []
  patterns: [canonical-bytes serialization for deterministic signing, signed-message-envelope pattern]

key-files:
  created:
    - crates/blufio-auth-keypair/src/message.rs
  modified:
    - crates/blufio-auth-keypair/src/keypair.rs
    - crates/blufio-auth-keypair/src/lib.rs
    - crates/blufio-auth-keypair/Cargo.toml
    - crates/blufio-config/src/model.rs
    - crates/blufio-config/src/validation.rs

key-decisions:
  - "canonical_bytes() uses pipe-delimited format for deterministic message serialization"
  - "verify() method wraps ed25519_dalek verify_strict for maximum security"
  - "AgentSpecConfig defaults specialist model to claude-sonnet-4-20250514"
  - "DelegationConfig timeout defaults to 60 seconds"

patterns-established:
  - "Signed envelope: AgentMessage -> canonical_bytes -> sign -> SignedAgentMessage"
  - "Agent specialization via [[agents]] TOML array in BlufioConfig"

requirements-completed: [SEC-07]

duration: 25min
completed: 2026-03-01
---

# Plan 10-01: Ed25519 Signing & Agent Config Summary

**Ed25519 sign/verify on DeviceKeypair, AgentMessage/SignedAgentMessage types, and [[agents]] TOML config for specialist agents**

## Performance

- **Duration:** 25 min
- **Started:** 2026-03-01
- **Completed:** 2026-03-01
- **Tasks:** 3
- **Files modified:** 6

## Accomplishments
- DeviceKeypair extended with sign(), verify_strict(), and verify() methods for Ed25519 signatures
- AgentMessage and SignedAgentMessage types with canonical byte serialization, request/response constructors, and round-trip verification
- AgentSpecConfig and DelegationConfig added to BlufioConfig with validation for duplicate/empty agent names
- Fixed pre-existing clippy warning (PluginConfig derivable_impls)

## Task Commits

Each task was committed atomically:

1. **Task 1-3: Ed25519 signing, message types, agent config** - `10e7a7c` (feat)

## Files Created/Modified
- `crates/blufio-auth-keypair/src/keypair.rs` - sign(), verify_strict(), verify() methods + 7 tests
- `crates/blufio-auth-keypair/src/message.rs` - AgentMessage, AgentMessageType, SignedAgentMessage + 9 tests
- `crates/blufio-auth-keypair/src/lib.rs` - Re-exports for Signature, AgentMessage types
- `crates/blufio-auth-keypair/Cargo.toml` - Added uuid, chrono workspace dependencies
- `crates/blufio-config/src/model.rs` - AgentSpecConfig, DelegationConfig structs
- `crates/blufio-config/src/validation.rs` - Duplicate agent name validation + 6 tests

## Decisions Made
- canonical_bytes() uses pipe-delimited format (id|type|sender|recipient|timestamp|content) for deterministic serialization
- verify() wraps ed25519_dalek verify_strict() (not lenient verify) for maximum security
- Default specialist model is claude-sonnet-4-20250514
- Delegation timeout defaults to 60s

## Deviations from Plan
- Fixed pre-existing clippy derivable_impls warning on PluginConfig (auto-fix, necessary for clean CI)

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Ed25519 signing and AgentMessage types ready for DelegationRouter (Plan 10-03)
- Agent specialization config ready for multi-agent wiring

---
*Phase: 10-multi-agent-final-integration*
*Completed: 2026-03-01*
