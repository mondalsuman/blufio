---
phase: 55
slug: memory-enhancements
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-11
---

# Phase 55 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in Rust test framework) |
| **Config file** | Cargo.toml per crate (workspace-level) |
| **Quick run command** | `cargo test -p blufio-memory` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio-memory`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 55-01-01 | 01 | 1 | MEME-01 | unit | `cargo test -p blufio-memory -- temporal_decay` | ❌ W0 | ⬜ pending |
| 55-01-02 | 01 | 1 | MEME-01 | unit | `cargo test -p blufio-memory -- file_watcher_no_decay` | ❌ W0 | ⬜ pending |
| 55-01-03 | 01 | 1 | MEME-01 | unit | `cargo test -p blufio-memory -- decay_floor` | ❌ W0 | ⬜ pending |
| 55-01-04 | 01 | 1 | MEME-02 | unit | `cargo test -p blufio-memory -- importance_boost` | ❌ W0 | ⬜ pending |
| 55-01-05 | 01 | 1 | MEME-02 | unit | `cargo test -p blufio-memory -- scoring_multiplicative` | ❌ W0 | ⬜ pending |
| 55-02-01 | 02 | 1 | MEME-03 | unit | `cargo test -p blufio-memory -- mmr_dedup` | ❌ W0 | ⬜ pending |
| 55-02-02 | 02 | 1 | MEME-03 | unit | `cargo test -p blufio-memory -- mmr_lambda_one` | ❌ W0 | ⬜ pending |
| 55-02-03 | 02 | 1 | MEME-03 | unit | `cargo test -p blufio-memory -- mmr_empty` | ❌ W0 | ⬜ pending |
| 55-03-01 | 03 | 1 | MEME-04 | unit | `cargo test -p blufio-memory -- eviction_trigger` | ❌ W0 | ⬜ pending |
| 55-03-02 | 03 | 1 | MEME-04 | unit | `cargo test -p blufio-memory -- eviction_target` | ❌ W0 | ⬜ pending |
| 55-03-03 | 03 | 1 | MEME-04 | unit | `cargo test -p blufio-memory -- eviction_active_only` | ❌ W0 | ⬜ pending |
| 55-03-04 | 03 | 1 | MEME-04 | unit | `cargo test -p blufio-memory -- count_active` | ❌ W0 | ⬜ pending |
| 55-04-01 | 04 | 2 | MEME-05 | unit | `cargo test -p blufio-memory -- validation_duplicate` | ❌ W0 | ⬜ pending |
| 55-04-02 | 04 | 2 | MEME-05 | unit | `cargo test -p blufio-memory -- validation_conflict` | ❌ W0 | ⬜ pending |
| 55-04-03 | 04 | 2 | MEME-05 | unit | `cargo test -p blufio-memory -- validation_stale` | ❌ W0 | ⬜ pending |
| 55-05-01 | 05 | 2 | MEME-06 | unit | `cargo test -p blufio-memory -- watcher_new_file` | ❌ W0 | ⬜ pending |
| 55-05-02 | 05 | 2 | MEME-06 | unit | `cargo test -p blufio-memory -- watcher_delete` | ❌ W0 | ⬜ pending |
| 55-05-03 | 05 | 2 | MEME-06 | unit | `cargo test -p blufio-memory -- file_memory_id` | ❌ W0 | ⬜ pending |
| 55-05-04 | 05 | 2 | MEME-06 | unit | `cargo test -p blufio-memory -- watcher_max_size` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Test fixtures for memory structs with various timestamps (for decay testing)
- [ ] Test fixtures for similar embeddings (for MMR and validation testing)
- [ ] No additional framework install needed (cargo test is built-in)

*Existing infrastructure covers framework requirements.*

---

## Manual-Only Verifications

*All phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
