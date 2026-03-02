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

## Cross-Milestone Trends

### Process Evolution

| Milestone | Phases | Plans | Key Change |
|-----------|--------|-------|------------|
| v1.0 | 14 | 43 | Initial structured approach with GSD workflow |

### Cumulative Quality

| Milestone | LOC | Source Files | Requirements | Verified |
|-----------|-----|-------------|-------------|----------|
| v1.0 | 28,790 | 111 | 70 | 70/70 |

### Top Lessons (Verified Across Milestones)

1. Wire integration at phase boundaries — don't defer cross-phase wiring to later phases
2. Milestone audit before completion catches gaps that in-phase verification misses
