# Phase 63: Code Quality Hardening - Context

**Gathered:** 2026-03-13
**Status:** Ready for planning

<domain>
## Phase Boundary

Sweep all library crates for unwrap panics, decompose oversized binary crate functions, fix known tech debt (hardcoded uptime, mock stubs), and add test/benchmark coverage. This is the final v1.5 phase that hardens everything written in Phases 53-62.

Requirements: QUAL-01 through QUAL-08.

</domain>

<decisions>
## Implementation Decisions

### Unwrap Sweep Scope
- Production library crates only (35 crates). Binary crate (crates/blufio/) keeps unwrap() -- panics there are just process exit
- Test code (#[cfg(test)] modules) keeps unwrap() -- panics in tests are expected behavior
- Only #![deny(clippy::unwrap_used)] -- no clippy::expect_used, no clippy::pedantic, no additional lints
- Incremental rollout: add deny directive to one crate at a time, fix all its unwraps, verify CI, move to next
- Prioritize heaviest offenders first: blufio-skill (147), blufio-storage (168), blufio-memory (68), blufio-audit (44), blufio-config (36), blufio-vault (35)

### Unwrap Replacement Patterns
- Default: propagate with `?` operator, adding proper error handling. Most unwrap() calls become Result propagation
- Mutex/RwLock: `.expect("lock poisoned")` -- lock poisoning means a prior panic; no recovery path
- Regex::new with literals: `.expect("valid regex: <pattern_name>")` -- compile-time-known patterns can't fail
- SystemTime::now().duration_since(UNIX_EPOCH): `.expect("system clock before epoch")` -- system invariant
- Channel send/recv (tokio mpsc/broadcast): `if let Err(e) = send() { tracing::warn!("channel closed: {e}") }` -- shutdown-time failures log and continue
- serde_json parsing: propagate as error via `?` -- malformed input is always possible
- String/number parsing from DB/config: propagate as error via `?` -- data can be corrupted
- File system operations: propagate with context via `.map_err(|e| BlufioError::Relevant(format!("context: {e}")))?`
- Option unwraps (HashMap::get, Vec::first): `.ok_or_else(|| BlufioError::Internal("expected X"))?`
- Channel adapter external SDK errors: map to existing `BlufioError::Channel(format!("adapter: {err}"))` -- no new error variants
- No #[allow(clippy::unwrap_used)] annotations -- use .expect() instead for self-documenting code

### Error Handling Patterns
- Keep existing BlufioError enum and conversion patterns -- no thiserror migration, no From impl standardization
- Error context messages follow "operation: details" format consistently (e.g., "parsing config: invalid TOML at line 5")
- External crate errors use consistent `.map_err()` with context -- explicit mapping over From impls
- Log errors at caller site only -- don't log inside error creation to avoid duplicates
- Trust the type system for new ? paths -- no need to test every new error propagation path
- Don't sweep unwrap_or/unwrap_or_default -- these are safe patterns, out of scope

### Function Decomposition
- Decompose serve.rs (2,331 lines) into serve/ directory: serve/mod.rs orchestrator + serve/storage.rs, serve/channels.rs, serve/gateway.rs, serve/subsystems.rs
- Decompose main.rs (3,220 lines) into cli/ directory: cli/mod.rs + cli/config_cmd.rs, cli/cron_cmd.rs, cli/gdpr_cmd.rs, cli/audit_cmd.rs, etc.
- Keep clap struct definitions in main.rs as single source of truth for CLI interface. Handlers move to cli/ modules
- Other large files (config/model.rs, events.rs, session.rs) NOT decomposed -- they're large but not complex
- Decompose FIRST, then unwrap sweep -- smaller files are easier to work on
- Pure refactor + tagged fixes: primarily move code unchanged, fix obvious issues (dead code, redundant clones) if found
- Follow existing naming conventions: snake_case module names matching subsystem names
- Brief //! module docs at top of each new module (1-2 lines explaining purpose)

### Quick Fixes (bundled with decomposition)
- Fix hardcoded uptime_secs: 0 in gateway -- pass Instant::now() from serve startup into AppState/gateway shared state
- Verify QUAL-04 (mock provider unimplemented!()) -- scan found 0 instances; verify thoroughly and close if already resolved
- Fix trivial TODOs discovered during sweep; track significant ones
- Remove clearly dead/unreachable code blocks (git preserves history)
- Add // SAFETY: comments to any unsafe blocks found (unlikely but handle if present)

### Integration Tests (QUAL-06)
- Scope: Email, iMessage, SMS adapters only (new v1.5 Phase 61 additions)
- Style: Mock HTTP server (wiremock-rs) + trait compliance tests
- wiremock-rs as workspace dev-dependency, caret version range
- Email tests: MIME parsing + SMTP only. Skip full IMAP mocking (too complex for value)
- iMessage tests: BlueBubbles REST API mock with wiremock
- SMS tests: Twilio API mock with wiremock, HMAC validation
- Edge cases: malformed input, API timeout, auth failure, rate limiting, empty messages, oversized content
- Always run in CI (no feature gate -- they use mock servers, no real external deps)
- Behavior tests only -- no snapshot tests (insta) for adapters

### Property-Based Tests (QUAL-07)
- Framework: proptest (already used in 3 files in the codebase)
- Test case count: 64 per proptest run (PROPTEST_CASES=64 for CI speed)
- **Compaction quality scoring**: score always in [0.0, 1.0], higher entity/decision retention -> higher score (monotonic), empty input -> 0.0, perfect retention -> 1.0
- **PII detection**: all 4 patterns (email, phone, SSN, credit card) generate valid inputs and verify detection; generate non-PII strings and verify no false positives
- **Hash chain verification**: valid chain always verifies, modifying single entry breaks verification, reordering breaks verification, appending preserves prior chain validity

### Benchmark Regression Detection (QUAL-08)
- Dual approach: criterion for CI regression detection + keep existing `blufio bench` for operator CLI
- Benchmark targets (core hot paths): context assembly, memory retrieval (embedding + MMR), PII detection, compaction quality scoring
- Input sizes: realistic (1KB, 5KB, 10KB messages). No stress tests
- Top-level benches/ directory at workspace root (bench_context.rs, bench_memory.rs, bench_pii.rs, bench_compaction.rs)
- criterion as workspace dev-dependency, caret version range
- Regression threshold: 20% -- fail if any benchmark regresses more than 20% from baseline
- Trigger: main branch push only (not on PRs). Separate .github/workflows/bench.yml
- Baseline storage: GitHub Actions cache only (key on Cargo.lock hash + bench file hash). No committed baselines
- Runner: default ubuntu-latest
- HTML reports uploaded as GitHub Actions artifacts for manual investigation
- No memory benchmarking (skip dhat -- too complex for this phase)
- No cargo-audit/cargo-deny addition (out of scope)

### Dependency Management
- Quick unused dep scan at start (cargo machete). Fix obvious removals. One-time, not a recurring plan
- Scan for duplicate versions (cargo tree -d) but don't fix -- just log findings
- Skip feature flag audit -- existing features work correctly
- New dev deps (criterion, wiremock, proptest) use caret version ranges
- Check total direct dep count before/after -- verify <80 runtime dep constraint still holds

### Code Style (Minimal Touch)
- Trust cargo fmt -- CI already enforces it. No rustfmt.toml, no import ordering rules
- No visibility enforcement, no comment style rules, no Cargo.toml metadata checks
- No clippy.toml -- deny directive in lib.rs is sufficient

### Claude's Discretion
- Exact grouping of crates into unwrap sweep batches per plan
- Exact module boundaries within serve/ and cli/ directories
- Which trivial TODOs to fix vs track
- Specific benchmark function signatures and test data generation
- Whether to add proptest::ProptestConfig or use env var for case count

</decisions>

<specifics>
## Specific Ideas

- Plan ordering: 1) Decompose serve.rs + main.rs, 2) Unwrap sweep (incremental per-crate), 3) Integration + property tests, 4) Criterion benchmarks + CI
- Error context messages should be grep-friendly: "operation: details" format
- The 1,564 unwrap() count is across library crates only. Binary crate excluded from deny directive
- wiremock-rs chosen over custom axum test stubs for adapter integration tests
- proptest over quickcheck for consistency with existing codebase usage

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- blufio-test-utils crate: may have mock providers already (check for QUAL-04)
- proptest already in workspace for error.rs, pii.rs, chain.rs -- extend patterns
- Existing bench module (blufio/src/bench.rs) for operator-facing benchmarks
- BlufioError typed hierarchy with severity(), is_retryable(), category() -- all unwrap replacements map to existing variants
- BlufioError::Channel variant already exists for adapter error mapping

### Established Patterns
- serde(default) for config structs -- maintain during any config-adjacent changes
- String fields in event sub-enums to avoid cross-crate deps -- follow for any new events
- tokio::spawn fire-and-forget for non-critical event emission -- channel send failures follow this pattern
- .map_err(|e| BlufioError::Variant(format!("context: {e}")))? as the standard error conversion pattern

### Integration Points
- serve.rs orchestrates all subsystem initialization -- decomposition splits but keeps same flow
- main.rs dispatches all CLI subcommands -- cli/ modules receive dispatch calls
- .github/workflows/ci.yml runs fmt + clippy + test -- new bench.yml parallel workflow
- Gateway AppState carries shared state -- uptime Instant goes here
- CI clippy step: `cargo clippy --workspace --all-targets -- -D warnings` -- deny(clippy::unwrap_used) adds to this

</code_context>

<deferred>
## Deferred Ideas

None -- discussion stayed within phase scope

</deferred>

---

*Phase: 63-code-quality-hardening*
*Context gathered: 2026-03-13*
