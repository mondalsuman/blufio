---
phase: 01-project-foundation-workspace
plan: 01
subsystem: infra
tags: [cargo, workspace, traits, jemalloc, ci, cargo-deny, licensing]

requires: []
provides:
  - Cargo workspace with 3 crates (blufio-core, blufio-config, blufio)
  - 7 adapter trait stubs with async_trait for dyn dispatch
  - BlufioError and common type definitions
  - Jemalloc global allocator in binary crate
  - CI/audit/release GitHub Actions pipelines
  - cargo-deny license and ban enforcement
  - Dual MIT/Apache-2.0 licensing and community docs
affects: [config-system, channel-adapters, provider-adapters, storage-adapters]

tech-stack:
  added: [async-trait, thiserror, strum, semver, clap, tikv-jemallocator, cargo-deny]
  patterns: [workspace-dependency-inheritance, async-trait-for-dyn-dispatch, spdx-headers]

key-files:
  created:
    - Cargo.toml
    - crates/blufio-core/src/lib.rs
    - crates/blufio-core/src/error.rs
    - crates/blufio-core/src/types.rs
    - crates/blufio-core/src/traits/adapter.rs
    - crates/blufio/src/main.rs
    - deny.toml
    - .github/workflows/ci.yml
  modified: []

key-decisions:
  - "Used async-trait for all adapter traits (not native async fn in trait) for dyn dispatch compatibility"
  - "Concrete BlufioError return type on all traits instead of associated error types"
  - "No tokio dependency in blufio-core — async-trait only needs std types"
  - "Ignored RUSTSEC-2024-0436 (paste unmaintained) — transitive via tikv-jemalloc-ctl, no alternative"
  - "Added MPL-2.0 to allowed licenses for dirs dependency chain"
  - "Used allow-wildcard-paths in deny.toml for internal workspace path dependencies"

patterns-established:
  - "SPDX dual-license header on all .rs files"
  - "Workspace dependency inheritance via workspace = true"
  - "PluginAdapter: Send + Sync + 'static supertraits on adapter traits"
  - "Conventional commits with scope: feat(01-01), docs(01)"

requirements-completed: [CORE-05, CORE-06, INFRA-01, INFRA-02, INFRA-03, INFRA-04]

duration: 15min
completed: 2026-02-28
---

# Plan 01-01: Workspace & Core Traits Summary

**Cargo workspace with 3 crates, 7 async adapter trait stubs, jemalloc allocator, cargo-deny enforcement, and CI/audit/release GitHub Actions**

## Performance

- **Duration:** ~15 min
- **Completed:** 2026-02-28
- **Tasks:** 2
- **Files modified:** 30+

## Accomplishments
- Cargo workspace with blufio-core, blufio-config (stub), and blufio binary crate
- 7 adapter traits (Channel, Provider, Storage, Embedding, Observability, Auth, SkillRuntime) with async_trait
- BlufioError enum with 8 variants, AdapterType enum, placeholder types for trait signatures
- Jemalloc global allocator on non-MSVC targets with clap CLI skeleton
- cargo-deny config banning openssl, enforcing license compliance
- CI workflow with 4 jobs (fmt, clippy, test, deny) across Linux + macOS matrix
- Security audit workflow with daily schedule
- Release workflow with musl cross-compilation via cross-rs

## Task Commits

1. **Task 1: Cargo workspace, core crate with trait stubs, and binary crate** - `1c2e524`
2. **Task 2: Licensing, community docs, cargo-deny config, and CI workflows** - `ccb63b8`

## Files Created/Modified
- `Cargo.toml` - Virtual workspace manifest with shared dependencies
- `crates/blufio-core/src/traits/*.rs` - 7 adapter trait definitions + base trait
- `crates/blufio-core/src/error.rs` - BlufioError enum
- `crates/blufio-core/src/types.rs` - Common types and placeholder structs
- `crates/blufio/src/main.rs` - Binary with jemalloc and clap CLI skeleton
- `deny.toml` - License/ban/advisory enforcement config
- `.github/workflows/ci.yml` - 4-job CI pipeline
- `.github/workflows/audit.yml` - Daily security audit
- `.github/workflows/release.yml` - musl cross-compile release

## Decisions Made
- Used async-trait (not native async fn in trait) for dyn dispatch compatibility
- Ignored RUSTSEC-2024-0436 for paste crate — no alternative available, transitive via jemalloc-ctl
- Added MPL-2.0 to allowed licenses (required by dirs dependency chain)
- Used allow-wildcard-paths in deny.toml for workspace path dependencies

## Deviations from Plan

### Auto-fixed Issues

**1. deny.toml config format updated for cargo-deny 0.19**
- **Found during:** Task 2 verification
- **Issue:** `unmaintained = "warn"` no longer valid in [advisories] section
- **Fix:** Removed deprecated field
- **Verification:** `cargo deny check` passes

**2. Added MPL-2.0 license allowance**
- **Found during:** Task 2 verification
- **Issue:** `option-ext` crate (via dirs) uses MPL-2.0, not in allow list
- **Fix:** Added "MPL-2.0" to licenses.allow
- **Verification:** `cargo deny check` licenses ok

**3. Added allow-wildcard-paths for internal deps**
- **Found during:** Task 2 verification
- **Issue:** Internal path deps flagged as wildcard dependencies
- **Fix:** Added `allow-wildcard-paths = true` to [bans]
- **Verification:** `cargo deny check` bans ok

---

**Total deviations:** 3 auto-fixed (config compatibility)
**Impact on plan:** All fixes necessary for cargo-deny compliance. No scope creep.

## Issues Encountered
- Content filter blocked subagent completion — task 2 completed directly by orchestrator

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Workspace compiles and tests pass
- Core traits ready for implementation in later phases
- Config crate stub ready for Plan 01-02 (TOML configuration system)

---
*Plan: 01-01-project-foundation-workspace*
*Completed: 2026-02-28*
