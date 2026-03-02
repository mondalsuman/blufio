# Phase 12: Verify Unverified Phases - Research

**Researched:** 2026-03-01
**Domain:** Verification and documentation of existing code
**Confidence:** HIGH

## Summary

Phase 12 is a verification and documentation phase -- no new features or code are written. The goal is to create VERIFICATION.md files for the 5 phases that lack formal verification (2, 5, 6, 8, 9), fix missing/empty SUMMARY files, and confirm all 33 unverified requirements are satisfied by the wired code.

The existing codebase is complete for phases 1-11. Phase 11's VERIFICATION.md serves as the format template. Each VERIFICATION.md follows the SC-N numbered criteria pattern with evidence blocks and a requirements coverage table.

**Primary recommendation:** Verify each phase in order (2, 5, 6, 8, 9), using deep code tracing to specific files and functions. Create retroactive SUMMARY files for missing plans (Phase 5 plans 01/02 and Phase 6 plans 01/02/03). Use PASS/PARTIAL/FAIL three-tier status for each success criterion.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Deep code tracing for each requirement -- trace to specific files, functions, and line-level evidence
- Include build+test evidence (cargo check/test results) in each VERIFICATION.md
- Phase-scoped verification only -- each VERIFICATION.md covers only its own phase's requirements
- Use success criteria from ROADMAP.md as the primary checklist structure (SC-N pattern)
- Three-tier status: PASS / PARTIAL / FAIL per success criterion
- Gap handling: trivial gaps fixed inline, non-trivial gaps flagged
- Full execution records for retroactive SUMMARYs matching existing format
- Mark retroactive SUMMARYs with "Retroactive: created during Phase 12 verification"
- Phase 11 style for VERIFICATION.md format (plain markdown, SC-N, evidence blocks, requirements table)
- One overall build verification section at the end (not per-criterion)
- Top-level verdict at the top for quick scanning
- Leave Phase 1's existing VERIFICATION.md as-is

### Claude's Discretion
- Judging gap severity (trivial fix vs flag for later)
- Routing flagged gaps to Phase 13 vs new phase
- Runtime vs static verification per requirement
- Best format reference for each retroactive SUMMARY
- Verification order across the 5 phases

### Deferred Ideas (OUT OF SCOPE)
- None -- discussion stayed within phase scope
</user_constraints>

## Verification Scope

### Phases Requiring VERIFICATION.md (5 files)

| Phase | Name | Requirements | Success Criteria |
|-------|------|-------------|-----------------|
| 2 | Persistence & Security Vault | PERS-01-05, SEC-01, SEC-04, SEC-08-10 | 5 SC |
| 5 | Memory & Embeddings | MEM-01-03, MEM-05 | 4 SC |
| 6 | Model Routing & Smart Heartbeats | LLM-06 (LLM-05 moved to Phase 11) | 2 SC |
| 8 | Plugin System & Gateway | PLUG-01-04, INFRA-05 | 4 SC |
| 9 | Production Hardening | CORE-04, CORE-07-08, COST-04, CLI-02-04, CLI-07-08, CORE-06 | 5 SC |

### Missing SUMMARY Files (5 retroactive summaries)

| Phase | Plan | Existing? | Action |
|-------|------|-----------|--------|
| 5 | 05-01-PLAN.md | No | Create retroactive SUMMARY |
| 5 | 05-02-PLAN.md | No | Create retroactive SUMMARY |
| 6 | 06-01-PLAN.md | No | Create retroactive SUMMARY |
| 6 | 06-02-PLAN.md | No | Create retroactive SUMMARY |
| 6 | 06-03-PLAN.md | No | Create retroactive SUMMARY |

### Existing SUMMARY Files (reference)

Phases with all summaries present: 2 (01, 02), 8 (01, 02, 03), 9 (01, 02, 03)
Phase 5 partial: only 05-03-SUMMARY.md exists

## Requirements Mapping

### Phase 2 Requirements (10 total)

| Req ID | Description | Crate(s) |
|--------|-------------|----------|
| PERS-01 | Single SQLite DB, WAL mode, ACID | blufio-storage |
| PERS-02 | Sessions persist across restarts | blufio-storage |
| PERS-03 | Crash-safe SQLite-backed queue | blufio-storage |
| PERS-04 | Backup is cp blufio.db | blufio-storage |
| PERS-05 | Single-writer prevents SQLITE_BUSY | blufio-storage |
| SEC-01 | Bind to 127.0.0.1 by default | blufio-config, blufio-security |
| SEC-04 | Vault key via Argon2id, never on disk | blufio-vault |
| SEC-08 | Secrets redacted from logs | blufio-security |
| SEC-09 | SSRF prevention (private IP blocking) | blufio-security |
| SEC-10 | TLS required for remote connections | blufio-security |

### Phase 5 Requirements (4 total)

| Req ID | Description | Crate(s) |
|--------|-------------|----------|
| MEM-01 | Hybrid search (vector + BM25) | blufio-memory |
| MEM-02 | Local ONNX embedding, no API calls | blufio-memory |
| MEM-03 | Context loads only relevant memories | blufio-memory, blufio-context |
| MEM-05 | Embeddings stored in SQLite | blufio-memory |

### Phase 6 Requirements (1 total)

| Req ID | Description | Crate(s) |
|--------|-------------|----------|
| LLM-06 | Smart heartbeats on Haiku, <=$10/month | blufio-router |

### Phase 8 Requirements (5 total)

| Req ID | Description | Crate(s) |
|--------|-------------|----------|
| PLUG-01 | Plugin host loads adapter plugins | blufio-plugin |
| PLUG-02 | Plugin CLI (list/search/install/remove/update) | blufio-plugin, blufio |
| PLUG-03 | Plugin manifest (plugin.toml) | blufio-plugin |
| PLUG-04 | Default plugin bundle ships | blufio-plugin |
| INFRA-05 | HTTP/WebSocket gateway (axum) | blufio-gateway |

### Phase 9 Requirements (10 total)

| Req ID | Description | Crate(s) |
|--------|-------------|----------|
| CORE-04 | Background daemon, auto-restart via systemd | blufio-config |
| CORE-06 | jemalloc, bounded caches/channels, lock timeouts | blufio (binary) |
| CORE-07 | Idle memory 50-80MB | blufio |
| CORE-08 | Load memory 100-200MB | blufio |
| COST-04 | Prometheus metrics endpoint | blufio-prometheus, blufio-gateway |
| CLI-02 | blufio status command | blufio |
| CLI-03 | blufio config get/set/set-secret/validate | blufio |
| CLI-04 | blufio doctor command | blufio |
| CLI-07 | systemd unit file | deployment/ |
| CLI-08 | Shell scripts for backup/logrotate/lifecycle | deployment/ |

## Code Structure

### Key Source Files by Phase

**Phase 2:**
- `crates/blufio-storage/src/database.rs` -- WAL mode, PRAGMAs
- `crates/blufio-storage/src/writer.rs` -- Single-writer pattern
- `crates/blufio-storage/src/queries/` -- Session/message/queue CRUD
- `crates/blufio-vault/src/crypto.rs` -- AES-256-GCM
- `crates/blufio-vault/src/kdf.rs` -- Argon2id
- `crates/blufio-vault/src/vault.rs` -- Vault lifecycle
- `crates/blufio-security/src/tls.rs` -- TLS enforcement
- `crates/blufio-security/src/ssrf.rs` -- SSRF prevention
- `crates/blufio-security/src/redact.rs` -- Secret redaction

**Phase 5:**
- `crates/blufio-memory/src/` -- Memory system
- `crates/blufio-memory/src/embedder.rs` -- ONNX embedder
- `crates/blufio-memory/src/store.rs` -- SQLite memory store
- `crates/blufio-memory/src/retriever.rs` -- Hybrid retriever
- `crates/blufio-memory/src/provider.rs` -- MemoryProvider

**Phase 6:**
- `crates/blufio-router/src/` -- Model routing
- `crates/blufio-router/src/classifier.rs` -- Query classifier
- `crates/blufio-router/src/router.rs` -- Model router
- `crates/blufio-router/src/heartbeat.rs` -- HeartbeatRunner

**Phase 8:**
- `crates/blufio-plugin/src/` -- Plugin system
- `crates/blufio-gateway/src/` -- HTTP/WebSocket gateway

**Phase 9:**
- `crates/blufio-prometheus/src/` -- Prometheus metrics
- `crates/blufio/src/` -- CLI commands
- `deployment/` -- systemd, scripts

## VERIFICATION.md Format Reference

Based on Phase 11's VERIFICATION.md:

```markdown
# Phase N Verification: Phase Name

**Phase:** NN-phase-slug
**Verified:** YYYY-MM-DD
**Requirements:** REQ-01, REQ-02, ...

## Phase Status: PASS|PARTIAL|FAIL (N/M criteria verified)

## Success Criteria Verification

### SC-1: [Success criterion text from ROADMAP.md]
**Status:** PASS|PARTIAL|FAIL

**Evidence:**
- [File path + specific function/struct + what it does]
- [Another evidence point]
- ...

### SC-2: ...

## Build Verification

```
cargo check --workspace  -- PASS|FAIL
cargo test --workspace   -- PASS|FAIL (N tests, M failures)
```

## Requirements Coverage

| Requirement | Status | Evidence |
|-------------|--------|----------|
| REQ-01 | Satisfied|Partial|Unsatisfied | SC-N (brief explanation) |
```

## Retroactive SUMMARY Format Reference

Based on Phase 5's 05-03-SUMMARY.md:

```markdown
---
phase: NN-phase-slug
plan: PP
type: summary
status: complete
commit: [hash or "retroactive"]
duration: ~Xmin
tests_added: N
tests_total: N
---

# Plan NN-PP Summary: Plan title

**Retroactive: created during Phase 12 verification**

## What was built

[Description of what the plan delivered]

### Changes

1. **Component** (`path/to/file`)
   - Description of change
   ...
```

## Common Pitfalls

### Pitfall 1: Claiming PASS without code evidence
**What goes wrong:** Verifier claims a requirement is satisfied without tracing to specific code
**How to avoid:** Every SC must reference specific files, functions, and explain how they satisfy the criterion

### Pitfall 2: Missing cross-phase wiring
**What goes wrong:** Code exists in a crate but is never called from the binary
**How to avoid:** Trace from the binary entry point (serve.rs, shell.rs, main.rs) to the feature code

### Pitfall 3: Confusing "code exists" with "feature works"
**What goes wrong:** Struct/function exists but is never used in the actual flow
**How to avoid:** Verify wiring from serve startup to feature usage

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| PERS-01 | Single SQLite, WAL, ACID | Verify WAL pragma in database.rs |
| PERS-02 | Sessions persist across restarts | Verify session CRUD in storage queries |
| PERS-03 | Crash-safe queue | Verify queue module in storage |
| PERS-04 | cp backup (single file) | Verify WAL checkpoint on close |
| PERS-05 | Single-writer, no SQLITE_BUSY | Verify tokio-rusqlite single-writer pattern |
| SEC-01 | Bind 127.0.0.1 default | Verify config default + gateway binding |
| SEC-04 | Argon2id vault key, never on disk | Verify KDF + vault key wrapping |
| SEC-08 | Secrets redacted from logs | Verify RedactingWriter in security crate |
| SEC-09 | SSRF prevention | Verify SsrfSafeResolver |
| SEC-10 | TLS required remote | Verify TLS enforcement in security crate |
| MEM-01 | Hybrid search (vector + BM25) | Verify HybridRetriever with RRF |
| MEM-02 | Local ONNX embedding | Verify OnnxEmbedder |
| MEM-03 | Relevant memories only | Verify MemoryProvider threshold filtering |
| MEM-05 | Embeddings in SQLite | Verify MemoryStore SQLite backend |
| LLM-06 | Smart heartbeats on Haiku | Verify HeartbeatRunner with skip-when-unchanged |
| PLUG-01 | Plugin host loads adapters | Verify PluginRegistry |
| PLUG-02 | Plugin CLI commands | Verify plugin subcommands in main.rs |
| PLUG-03 | Plugin manifest (plugin.toml) | Verify PluginManifest struct |
| PLUG-04 | Default plugin bundle | Verify built-in catalog |
| INFRA-05 | HTTP/WebSocket gateway | Verify axum gateway in blufio-gateway |
| CORE-04 | Daemon with auto-restart | Verify DaemonConfig and systemd file |
| CORE-06 | jemalloc, bounded caches | Verify jemalloc + bounded channels |
| CORE-07 | Idle memory 50-80MB | Verify memory monitoring config |
| CORE-08 | Load memory 100-200MB | Verify memory bounds config |
| COST-04 | Prometheus metrics endpoint | Verify Prometheus exporter |
| CLI-02 | blufio status | Verify status subcommand |
| CLI-03 | blufio config commands | Verify config subcommands |
| CLI-04 | blufio doctor | Verify doctor subcommand |
| CLI-07 | systemd unit file | Verify deployment/blufio.service |
| CLI-08 | Shell scripts (backup, logrotate) | Verify deployment/ scripts |
| CORE-06 | jemalloc + bounded resources | Verify allocator + channel bounds |
</phase_requirements>

## Sources

### Primary (HIGH confidence)
- Existing PLAN.md files for phases 2, 5, 6, 8, 9 -- task-level implementation details
- Existing SUMMARY.md files -- execution records
- Phase 11 VERIFICATION.md -- format template
- ROADMAP.md -- success criteria for all phases
- REQUIREMENTS.md -- requirement definitions

## Metadata

**Confidence breakdown:**
- Verification format: HIGH -- existing Phase 11 template
- Requirements mapping: HIGH -- from ROADMAP.md and REQUIREMENTS.md
- Code locations: HIGH -- from existing PLANs and SUMMARYs

**Research date:** 2026-03-01
**Valid until:** N/A (verification phase, not technology research)
