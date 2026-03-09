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

## Milestone: v1.2 -- Production Hardening

**Shipped:** 2026-03-04
**Phases:** 6 | **Plans:** 13

### What Was Built
- Backup integrity verification: PRAGMA integrity_check post-backup/restore, auto-delete corrupt backups
- sd_notify integration: Type=notify readiness, watchdog pings, STATUS= progress, silent no-op on non-systemd
- SQLCipher database encryption at rest: centralized key management, three-file safe migration, doctor reporting
- Minisign binary signature verification: embedded public key, blufio verify CLI
- Self-update with rollback: version check via GitHub Releases, download + Minisign verify + atomic swap + health check + rollback
- 1 new crate (blufio-verify), ~2,706 lines added

### What Worked
- **Milestone audit pattern**: v1.2-MILESTONE-AUDIT.md caught the CIPH-01 feature flag issue, missing VERIFICATION.md files, and SUMMARY frontmatter gaps before declaring done. Three milestones of evidence that pre-ship audit pays for itself.
- **Phase velocity improvement**: 13 plans in ~1 day (~13 plans/day), consistent with v1.1 velocity (~16/day). Structured approach is mature.
- **Single gap-closure phase**: Only 1 gap-closure phase (28) out of 6 total (17%), down from 28% (v1.0) and 37% (v1.1). Integration verification during execution is working better.
- **Feature flag discipline**: Using Cargo feature flags for SQLCipher conditional compilation kept the build clean and testable on both paths.
- **Three-file safety strategy**: The encrypt migration (original -> .encrypting temp -> verify -> swap) prevented data loss risk during a destructive operation.

### What Was Inefficient
- **SUMMARY frontmatter still required retroactive fix**: Phases 26 and 27 shipped without requirements-completed in SUMMARY frontmatter, requiring Phase 28 to backfill. This is the third milestone with the same issue.
- **Phase Detail plan lists left as TBD**: ROADMAP Phase Details for 24, 25, 26, 27 still said "Plans: TBD" after execution. Plan lists should be updated during plan-phase, not left stale.
- **Progress table column drift**: Phases 23, 24, 26, 27, 28 had malformed progress table rows (missing v1.2 milestone column). Manual table editing is error-prone.

### Patterns Established
- **SQLCipher centralized opener**: All database consumers use open_connection() factory from blufio-storage, ensuring PRAGMA key is always first statement
- **Minisign embedded key**: Public key as compile-time constant, no external key file distribution needed
- **Self-update atomic swap**: Backup current -> download new -> verify signature -> swap -> health check -> rollback if needed
- **sd_notify as no-op on non-systemd**: Platform-conditional code that compiles cleanly on macOS/Docker

### Key Lessons
1. **SUMMARY frontmatter must be filled during execution** -- three milestones of the same lesson. This should be enforced by tooling (gsd-tools reject empty requirements_completed).
2. **ROADMAP plan lists must be updated during plan-phase** -- "TBD" should never persist after plans are created. Add to plan-phase workflow checklist.
3. **Gap-closure overhead is trending down** -- v1.0: 28%, v1.1: 37%, v1.2: 17%. Integration checks during execution are reducing the audit delta.
4. **Milestone audit is a non-negotiable gate** -- every milestone has found real issues. Budget for it, do not skip it.

### Cost Observations
- Commits: 58 total (10 feat, 6 fix, 31 docs, 3 chore)
- Timeline: ~1 calendar day (2026-03-03 evening to 2026-03-04 morning)
- Notable: Gap-closure phase was 1 of 6 (17%), lowest ratio yet. Only 1 code fix needed (feature flag).

---

## Milestone: v1.3 -- Ecosystem Expansion

**Shipped:** 2026-03-08
**Phases:** 17 | **Plans:** 47

### What Was Built
- Internal event bus (tokio broadcast + mpsc) with provider-agnostic ToolDefinition and media provider traits
- 4 LLM provider plugins: OpenAI (streaming + vision), Ollama (native /api/chat), OpenRouter (fallback ordering), Gemini (native API)
- OpenAI-compatible gateway API: /v1/chat/completions, /v1/responses, /v1/tools with complete wire type separation
- Scoped API keys (rate limiting, revocation), webhooks (HMAC-SHA256, exponential backoff), batch processing
- 6 new channel adapters: Discord (serenity), Slack (slack-morphism), WhatsApp (Cloud API + Web), Signal (signal-cli), IRC (TLS + SASL), Matrix (matrix-sdk 0.11)
- Cross-channel bridging with configurable TOML rules and loop prevention
- Skill registry with Ed25519 code signing, pre-execution verification gate, capability enforcement
- Docker multi-stage distroless image, docker-compose deployment, multi-instance systemd template
- Node system: Ed25519 pairing, WebSocket heartbeat, fleet CLI, approval routing broadcast
- OpenClaw migration tool, bench, privacy report, config recipe, uninstall, bundle
- 14 new crates, ~40,150 lines added (total: 71,808 LOC across 35 crates)

### What Worked
- **Audit-driven gap closure phases (40-45)**: The v1.3 audit identified 5 runtime wiring gaps (EventBus, provider registry, gateway stores, event publishers, node approval) plus documentation staleness. Creating targeted gap-closure phases (40-45) resolved all of them systematically.
- **Provider crate decoupling**: Each LLM provider (OpenAI, Ollama, OpenRouter, Gemini) owns its own wire types with no cross-crate dependencies. Providers evolved independently without conflicts.
- **Phase velocity on gap closure**: Phases 40-45 averaged ~5 minutes per plan — tiny, focused plans that wired existing infrastructure. Fastest execution in the project's history.
- **Comprehensive integration verification**: Phase 39 created formal VERIFICATION.md for every phase, with 7 verification plans covering all 71 requirements and 4 E2E integration flows.
- **Three-audit pattern**: Initial audit (gaps_found) → gap closure (phases 40-44) → re-audit (tech_debt) → docs sync (phase 45) → final audit (passed). Each round found real issues.

### What Was Inefficient
- **Phase 32 SUMMARY files missing**: Phase 32 (Scoped API Keys, Webhooks & Batch) shipped without SUMMARY.md files. Code verified via 32-VERIFICATION.md but the analyzer showed 0/3 summaries. Fourth milestone with this pattern.
- **SUMMARY one_liner frontmatter empty**: All v1.3 SUMMARY files lack `one_liner` frontmatter, making automated accomplishment extraction return null. The complete-milestone CLI couldn't extract accomplishments automatically.
- **Gap closure overhead still significant**: 6 gap-closure phases (40-45) out of 17 total (35%). While individually small (1-2 plans each), the pattern of "build feature crates, wire them later" persists. Integration during initial feature phases would eliminate this entirely.
- **ROADMAP progress table formatting drift**: Phase 45 row had inconsistent column formatting (missing v1.3 milestone column). Manual table editing continues to be error-prone.
- **Three audit rounds**: Despite v1.2's improvement to 17% gap-closure ratio, v1.3 reverted to 35% because the milestone was significantly larger (17 phases vs 6) and included complex cross-crate wiring.

### Patterns Established
- **Provider-agnostic ToolDefinition**: Each LLM provider serializes to its own wire format from a common ToolDefinition type in blufio-core
- **Wire type separation**: OpenAI-compatible API uses its own request/response types completely separate from internal ProviderResponse — no leaky abstractions
- **ChannelAdapter trait with capabilities manifest**: All 8 channels declare capabilities (markdown, images, reactions, threads, etc.) and the format degradation pipeline adapts output
- **EventBus publish/subscribe with dual channels**: tokio broadcast for fire-and-forget (chat events, skill events), mpsc for reliable delivery (webhook, audit)
- **TOFU key management for skills**: First publisher key trusted on install, key changes hard-blocked
- **OnceLock for late-wiring**: OnceLock<Arc<T>> pattern for components that need to be set after construction (ApprovalRouter, EventBus)

### Key Lessons
1. **Gap-closure phases scale with milestone size.** v1.2 had 17% overhead (6 phases). v1.3 had 35% (17 phases). For large milestones, integration wiring must be part of the original feature phase, not deferred.
2. **SUMMARY one_liner frontmatter needs to be mandatory.** Four milestones of empty frontmatter. The GSD workflow should enforce this during plan completion.
3. **Phase 32's missing SUMMARYs prove the frontmatter lesson.** Despite 53 tests passing and full VERIFICATION.md, the phase appeared incomplete to tooling because SUMMARY files were absent.
4. **Small, focused gap-closure plans execute fastest.** Phases 40-45 averaged 2-7 minutes per plan because each had a single, well-defined wiring task. This is the ideal plan granularity.
5. **Three-audit pattern is the right quality bar for large milestones.** The first audit found 5 wiring gaps, the second found documentation staleness, the third confirmed clean. Each round caught real issues.

### Cost Observations
- Commits: 156 total
- Timeline: 4 calendar days (2026-03-05 to 2026-03-08)
- Notable: Gap-closure phases (40-45) were 6 of 17 phases (35%) but only ~11 plans of 47 total (23%). Small plans, big impact.
- Lines: +40,150 net across 207 files

---

## Milestone: v1.4 -- Quality & Resilience

**Shipped:** 2026-03-09
**Phases:** 7 | **Plans:** 16

### What Was Built
- Typed error hierarchy with `is_retryable()`, `severity()`, `category()` classification across all 35 crates
- Accurate token counting via tiktoken-rs (OpenAI o200k/cl100k) and HuggingFace tokenizers (Claude), replacing `len()/4` heuristic
- Per-dependency circuit breaker FSM (Closed/Open/HalfOpen) with configurable thresholds and Prometheus metrics
- 6-level graceful degradation ladder (L0-L5) with auto-escalation, hysteresis, fallback provider routing, user notifications
- FormatPipeline wired into all 8 channel adapters with paragraph-boundary splitting and adapter-specific formatting
- Extended ChannelCapabilities (streaming_type, formatting_support, rate_limit) and Table/List content types
- Architectural decision records (ADR-001 ORT, ADR-002 Plugin Architecture) in MADR 4.0.0 format
- 1 new crate (blufio-resilience), ~8,293 lines added (total: 80,101 LOC across 35 crates)

### What Worked
- **Audit-driven gap closure (again)**: Initial audit found 5 unsatisfied requirements, 1 integration gap, 1 broken flow. Phases 51-52 closed all gaps. Re-audit passed clean.
- **Highest velocity yet**: 16 plans in 1 day (~16 plans/day), matching v1.1's record. Deep familiarity with the codebase accelerated execution.
- **Minimal gap-closure overhead**: Only 2 gap-closure phases (51, 52) out of 7 total (29%). Phase 52 was pure bookkeeping (2 minutes). Effective integration during feature phases.
- **Custom circuit breaker over crate deps**: Building a ~200 LOC circuit breaker instead of using failsafe/tower crates avoided dyn dispatch incompatibility. Right abstraction level for the problem.
- **Clock trait injection for testing**: Deterministic testing of circuit breaker timeouts via MockClock. No flaky time-dependent tests.

### What Was Inefficient
- **SUMMARY one_liner still empty**: All 16 SUMMARY files lack `one_liner` frontmatter, making automated accomplishment extraction return null during milestone completion. Fifth milestone with this pattern.
- **Phase 52 exists**: A pure metadata fix phase (checkbox and frontmatter corrections) shouldn't be needed if SUMMARY frontmatter were filled during execution.
- **Audit found wiring gap**: SessionActor didn't publish CB events to EventBus — the same "infrastructure built, consumption deferred" pattern from v1.1/v1.3. Phase 51 fixed it.
- **Net negative lines**: 25,845 deletions vs 17,645 insertions — significant refactoring of error handling across all crates, but the diff size suggests the typed error migration touched more code than expected.

### Patterns Established
- **Circuit breaker with Clock injection**: Arc<dyn Clock> for deterministic testing, RealClock for production
- **DegradationManager with hysteresis**: Level changes require sustained recovery (configurable timer) before de-escalation
- **4-step adapter pipeline**: detect → format → split → escape enforced consistently in all 8 channel adapters
- **Tier-based fallback provider mapping**: Model names mapped to capability tiers for cross-provider failover
- **ADR documentation convention**: MADR 4.0.0 format with docs/adr/ directory and README index

### Key Lessons
1. **SUMMARY one_liner MUST be enforced by tooling** — five milestones with the same gap. This is no longer a process issue; it requires a code fix in gsd-tools.
2. **The "infrastructure built, consumption deferred" pattern persists** — Phase 51 (wiring CB events to EventBus) is the same pattern from v1.1 and v1.3. Feature phases should include a "verify all producers connected to all consumers" checklist item.
3. **Refactoring milestones touch more code than feature milestones** — v1.4 changed 245 files but had net negative LOC. Error hierarchy migration across 35 crates is inherently high-touch.
4. **Custom implementations beat external crates when trait compatibility is needed** — failsafe and tower-limit couldn't work with dyn dispatch. ~200 LOC of custom CB code was cleaner and more testable.
5. **Gap-closure overhead is trending down** — v1.0: 28%, v1.1: 37%, v1.2: 17%, v1.3: 35%, v1.4: 29%. Integration during feature phases is improving, but wiring gaps still appear for cross-cutting concerns.

### Cost Observations
- Commits: 54 total
- Timeline: 1 calendar day (2026-03-09)
- Notable: Phase 52 (bookkeeping) completed in 2 minutes. Phase 51 (CB→EventBus wiring) took 15 minutes. Small, focused plans remain the fastest to execute.

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Phases | Plans | Velocity | Key Change |
|-----------|--------|-------|----------|------------|
| v1.0 | 14 | 43 | ~10/day | Initial structured approach with GSD workflow |
| v1.1 | 8 | 32 | ~16/day | Security-per-phase, dual audit, gap closure phases |
| v1.2 | 6 | 13 | ~13/day | Lowest gap-closure ratio (17%), mature audit pattern |
| v1.3 | 17 | 47 | ~12/day | Largest milestone, three-audit pattern, provider crate decoupling |
| v1.4 | 7 | 16 | ~16/day | Refactoring milestone, highest velocity tied with v1.1 |

### Cumulative Quality

| Milestone | LOC | Crates | Requirements | Verified | Tech Debt Items |
|-----------|-----|--------|-------------|----------|-----------------|
| v1.0 | 28,790 | 14 | 70 | 70/70 | 10 |
| v1.1 | 36,462 | 16 | 48 (118 total) | 48/48 | 12 |
| v1.2 | 39,168 | 21 | 30 (148 total) | 30/30 | 12 (carry-forward) |
| v1.3 | 71,808 | 35 | 71 (219 total) | 71/71 | 16 |
| v1.4 | 80,101 | 35 | 39 (258 total) | 39/39 | 16 (carry-forward) |

### Gap-Closure Ratio

| Milestone | Total Phases | Gap-Closure | Ratio | Trend |
|-----------|-------------|-------------|-------|-------|
| v1.0 | 14 | 4 | 28% | baseline |
| v1.1 | 8 | 3 | 37% | worse (but smaller milestone) |
| v1.2 | 6 | 1 | 17% | improvement |
| v1.3 | 17 | 6 | 35% | reverted (large milestone, complex cross-crate wiring) |
| v1.4 | 7 | 2 | 29% | stable (one wiring gap, one bookkeeping fix) |

### Top Lessons (Verified Across Milestones)

1. **Wire integration at phase boundaries** — all five milestones had wiring gaps caught only at audit. Must be enforced as a phase gate, not just advice.
2. **Milestone audit before completion catches gaps** — v1.0 found 33 stale requirements; v1.1 found 26 orphaned; v1.3 found 5 runtime wiring gaps; v1.4 found CB→EventBus wiring gap. Always audit.
3. **SUMMARY frontmatter must be filled during execution** — five milestones of empty/missing frontmatter. Tooling enforcement is overdue. This is the single most repeated lesson.
4. **Gap-closure overhead correlates with milestone scope** — small milestones (v1.2: 17%) have less overhead than large ones (v1.3: 35%). v1.4's 29% is mid-range for a refactoring milestone.
5. **Small, focused gap-closure plans execute fastest** — v1.3 phases 40-45 averaged 2-7 minutes per plan; v1.4 Phase 52 completed in 2 minutes. This is the ideal plan granularity for wiring tasks.
6. **Custom implementations beat external crates when trait compatibility is needed** — v1.4's ~200 LOC circuit breaker was cleaner than using failsafe/tower crates with dyn dispatch incompatibility.
