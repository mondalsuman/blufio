# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v1.0 — MVP

**Shipped:** 2026-03-02
**Phases:** 14 | **Plans:** 43

### What Was Built
- Complete Rust AI agent platform: 14 crates, 28,790 LOC, 111 source files
- FSM agent loop with Anthropic streaming, Telegram adapter, SQLite persistence
- Three-zone context engine with prompt cache alignment and cost ledger
- Local ONNX memory with hybrid search (vector + BM25)
- WASM skill sandbox with capability manifests and progressive discovery
- Plugin system (7 adapter traits), HTTP/WebSocket gateway, Prometheus metrics
- Multi-agent delegation with Ed25519 signing, model routing (Haiku/Sonnet/Opus)
- Security hardening: TLS enforcement, SSRF protection, secret redaction, encrypted vault

### What Worked
- **Vertical-slice phase design**: Each phase delivered a complete, testable capability rather than horizontal layers. This made verification straightforward.
- **GSD workflow**: Structured phases with PLAN.md -> SUMMARY.md -> VERIFICATION.md created clear audit trail. Made re-auditing possible.
- **Gap closure phases (11-14)**: The milestone audit found real gaps. Creating targeted gap-closure phases was more effective than trying to fix everything at once.
- **Dependency ordering**: Phase 1-10 followed a clean dependency graph. Each phase could build on verified prior work.
- **3-day sprint**: 28,790 LOC Rust in 3 days with Claude Code. The structured approach prevented scope creep and kept momentum.

### What Was Inefficient
- **Retroactive SUMMARY files**: Phases 5-9, 11-12 had empty/missing SUMMARYs that needed retroactive creation in Phase 12. Should have enforced SUMMARY completion as part of phase execution.
- **Audit round-trip**: First audit found 33 unsatisfied requirements and 4 integration bugs. Three gap-closure phases (11, 12, 13) plus a re-audit were needed. Better phase verification during execution would have caught these earlier.
- **Cross-phase integration gaps**: build_secure_client(), RedactingWriter, and Prometheus business metrics were all implemented but not wired into their consumers until Phase 14. Integration testing between phases should be explicit.
- **Phase 13 plan checkbox**: ROADMAP.md showed `[ ]` for 13-01-PLAN.md despite Phase 13 being complete. Minor documentation inconsistency from manual sync.

### Patterns Established
- **Cargo workspace with per-feature crates**: Each major capability gets its own crate (blufio-storage, blufio-vault, blufio-context, etc.). Clean dependency graph, fast incremental builds.
- **Adapter trait pattern**: All pluggable components (Channel, Provider, Storage, Embedding, Observability, Auth, SkillRuntime) implement async traits with dyn dispatch via async-trait.
- **Single-writer SQLite**: tokio-rusqlite provides a dedicated writer thread. No SQLITE_BUSY contention.
- **Security-by-default**: localhost binding, TLS enforcement, SSRF prevention, secret redaction, encrypted vault — all enabled without configuration.
- **Phase verification**: Every phase gets a VERIFICATION.md that checks success criteria against the actual codebase. Three-source cross-reference (VERIFICATION, SUMMARY, REQUIREMENTS) for complete coverage.

### Key Lessons
1. **Wire integration at phase boundaries, not in a final "wiring" phase.** Three integration gaps (INT-01, INT-02, INT-03) were all cases where a crate was implemented but not consumed by its intended user. Each phase should include an "integration check" step.
2. **Enforce SUMMARY.md completion as a phase gate.** Empty frontmatter in retroactive summaries is less useful than summaries written during execution when context is fresh.
3. **Milestone audit before declaring done pays for itself.** The v1.0 audit found 33 stale requirement statuses, 4 real bugs, and 5 unverified phases. Without it, v1.0 would have shipped with material gaps.
4. **Structured requirement IDs (CORE-01, SEC-03, etc.) make traceability tractable.** 70 requirements across 14 phases remained auditable because every requirement had a unique ID and phase mapping.
5. **ort 2.0-rc requires careful version pinning.** ndarray 0.17 required (not 0.16), and ort features list is specific. Document exact versions in decisions.

### Cost Observations
- Commits: 158 total (44 feat, 8 fix, 79 docs)
- Timeline: 3 calendar days
- Notable: Gap-closure phases (11-14) were ~25% of total phases but addressed critical quality gaps that would have been much more expensive to find post-ship

---

## Milestone: v1.1 — MCP Integration

**Shipped:** 2026-03-03
**Phases:** 8 | **Plans:** 32

### What Was Built
- MCP server with stdio + Streamable HTTP transports for Claude Desktop connectivity
- MCP client consuming external servers via TOML config with agent tool invocation
- Security chain: namespace enforcement, export allowlist, SHA-256 hash pinning, description sanitization, trust zone labeling
- MCP resources: memory search/lookup, session history, prompt templates
- Prometheus MCP metrics, connection limits, health monitoring with exponential backoff
- 2 new crates (blufio-mcp-server, blufio-mcp-client), ~8,452 lines added

### What Worked
- **Audit-driven gap closure**: First audit (gaps_found) triggered phases 20-22 which closed all 26 orphaned requirements and 6 integration gaps. Second audit confirmed tech_debt only.
- **Security-per-phase pattern**: Embedding security in each phase (namespace in 15, allowlist in 16, CORS/auth in 17, hash pinning in 18) instead of deferring to a security phase. Zero security gaps at audit.
- **rmcp SDK choice**: Official Anthropic Rust MCP SDK worked cleanly for both stdio and HTTP transports with a single handler implementation.
- **Phase velocity**: 32 plans in 2 days (~16 plans/day), up from ~10/day in v1.0. Structured approach matured.
- **Verification phases**: Dedicated verification phases (20, 22) created formal VERIFICATION.md reports, catching SUMMARY frontmatter inconsistencies.

### What Was Inefficient
- **SUMMARY frontmatter gaps persisted**: Phases 16, 18, 19 still had empty `requirements_completed` arrays despite lesson from v1.0. The fix (Phase 20, 22 verification) validated requirements but didn't retroactively fix frontmatter.
- **Three audit rounds**: Initial build → first audit (gaps) → gap closure phases → second audit (tech_debt). Could have caught wiring gaps earlier with integration checks during phase execution.
- **Deferred infrastructure consumption**: 5 items (SRVR-13/14, CLNT-06/12, INTG-04) built infrastructure but never wired consumption. The pattern "build infrastructure now, wire callers later" creates tech debt that accumulates.
- **Phase 18 consolidated SUMMARY**: Single 18-SUMMARY.md instead of per-plan summaries made the roadmap analyzer report Phase 18 as "partial" despite full completion. Convention mismatch.

### Patterns Established
- **MCP handler pattern**: Single BlufioMcpHandler with `#[tool]` macros, shared between stdio and HTTP transports via Arc
- **Trust zone separation**: External tools identified by `__` namespace separator, labeled in prompt context with factual/neutral tone
- **Hash pinning for rug-pull detection**: PinStore in SQLite with SHA-256 hashes, checked at discovery time
- **Builder pattern for optional fields**: `with_resources()`, `with_server_name()` patterns for graceful degradation

### Key Lessons
1. **Integration checks during phase execution, not just at audit.** The same lesson from v1.0 repeated — wiring gaps found at audit instead of during execution. Must be enforced as a phase gate.
2. **SUMMARY frontmatter must be filled during execution.** Two milestones of the same lesson. Consider making gsd-tools reject empty `requirements_completed` arrays.
3. **Consolidated SUMMARYs break tooling conventions.** Phase 18's single SUMMARY confused the roadmap analyzer. One SUMMARY per plan is the convention.
4. **"Infrastructure built, consumption deferred" is a pattern that needs tracking.** The 5 deferred items are all this pattern. Consider a DEFERRED.md or explicit tracking.
5. **Dual-audit pattern is valuable but expensive.** The gap-closure phases (20-22) were 3 of 8 phases (37.5%). Earlier integration verification would reduce this overhead.

### Cost Observations
- Commits: 42 total
- Timeline: 2 calendar days
- Notable: Gap-closure phases (20-22) were 37.5% of phases, consistent with v1.0 (25%). Integration verification remains the main quality gate.

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Phases | Plans | Velocity | Key Change |
|-----------|--------|-------|----------|------------|
| v1.0 | 14 | 43 | ~10/day | Initial structured approach with GSD workflow |
| v1.1 | 8 | 32 | ~16/day | Security-per-phase, dual audit, gap closure phases |

### Cumulative Quality

| Milestone | LOC | Crates | Requirements | Verified | Tech Debt Items |
|-----------|-----|--------|-------------|----------|-----------------|
| v1.0 | 28,790 | 14 | 70 | 70/70 | 10 |
| v1.1 | 36,462 | 16 | 48 (118 total) | 48/48 | 12 |

### Top Lessons (Verified Across Milestones)

1. **Wire integration at phase boundaries** — both milestones had wiring gaps caught only at audit. Must be enforced as a phase gate, not just advice.
2. **Milestone audit before completion catches gaps** — v1.0 found 33 stale requirements; v1.1 found 26 orphaned requirements. Always audit.
3. **SUMMARY frontmatter must be filled during execution** — both milestones had empty/missing frontmatter. Consider tooling enforcement.
4. **Gap-closure phases are ~25-37% overhead** — v1.0: 4/14 phases (28%), v1.1: 3/8 phases (37%). Worth the investment but reducible with earlier verification.
