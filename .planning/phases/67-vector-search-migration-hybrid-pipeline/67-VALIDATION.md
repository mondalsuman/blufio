---
phase: 67
slug: vector-search-migration-hybrid-pipeline
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-14
---

# Phase 67 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml per crate |
| **Quick run command** | `cargo test -p blufio-memory -- --test-threads=1` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio-memory -- --test-threads=1`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 67-01-01 | 01 | 1 | VEC-04 | integration | `cargo test -p blufio-memory store::tests::vec0_populate_copies_active_memories -- -x` | ✅ | ⬜ pending |
| 67-01-02 | 01 | 1 | VEC-04 | integration | `cargo test -p blufio-memory store::tests::vec0_populate_is_idempotent -- -x` | ✅ | ⬜ pending |
| 67-01-03 | 01 | 1 | VEC-04 | integration | `cargo test -p blufio-memory -- vec0_startup_migration -x` | ❌ W0 | ⬜ pending |
| 67-02-01 | 02 | 1 | VEC-08 | integration | `cargo test -p blufio-memory -- get_embeddings_by_ids -x` | ❌ W0 | ⬜ pending |
| 67-02-02 | 02 | 1 | VEC-08 | integration | `cargo test -p blufio-memory -- vec0_auxiliary_scoring -x` | ❌ W0 | ⬜ pending |
| 67-03-01 | 03 | 2 | VEC-05 | integration | `cargo test -p blufio-memory -- parity -x` | ❌ W0 | ⬜ pending |
| 67-03-02 | 03 | 2 | VEC-05 | integration | `cargo test -p blufio-memory -- parity_scale -x` | ❌ W0 | ⬜ pending |
| 67-03-03 | 03 | 2 | VEC-06 | integration | `cargo test -p blufio-memory store::tests::vec0_batch_evict_deletes_from_vec0 -- -x` | ✅ | ⬜ pending |
| 67-03-04 | 03 | 2 | VEC-06 | integration | `cargo test -p blufio-memory store::tests::vec0_soft_delete_updates_status_in_vec0 -- -x` | ✅ | ⬜ pending |
| 67-03-05 | 03 | 2 | VEC-07 | unit | `cargo test -p blufio-memory vec0::tests::vec0_knn_search_with_session_id_filter -- -x` | ✅ | ⬜ pending |
| 67-04-01 | 04 | 3 | VEC-05 | unit | `cargo test -p blufio-config -- vec0_enabled_default -x` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/blufio-memory/tests/parity.rs` — parity test comparing vec0 vs in-memory full pipeline at 10/100/1K scale (VEC-05)
- [ ] `crates/blufio-memory/src/store.rs` — test for `get_embeddings_by_ids()` (VEC-08)
- [ ] `crates/blufio-memory/tests/vec0_scoring.rs` — test for vec0 auxiliary data used in scoring pipeline (VEC-08)
- [ ] `crates/blufio-memory/tests/startup_migration.rs` — test for startup migration wiring (VEC-04)
- [ ] `crates/blufio-config/src/model.rs` — test for `vec0_enabled` default value change (VEC-05)

*Existing infrastructure covers VEC-04 basic migration, VEC-06 atomic sync, VEC-07 session partitioning.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Migration blocks startup under real load | VEC-04 | Requires multi-GB database with real embeddings | Start blufio with large DB, verify no queries succeed before migration completes |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
