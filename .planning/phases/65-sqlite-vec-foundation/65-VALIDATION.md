---
phase: 65
slug: sqlite-vec-foundation
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-13
---

# Phase 65 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test + criterion 0.5 |
| **Config file** | crates/blufio/Cargo.toml (existing bench targets) |
| **Quick run command** | `cargo test -p blufio-memory --lib` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio-memory --lib`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 65-01-01 | 01 | 1 | VEC-02 | integration | `cargo test -p blufio --test e2e_vec0 -- sqlcipher` | ❌ W0 | ⬜ pending |
| 65-01-02 | 01 | 1 | VEC-01 | unit | `cargo test -p blufio-memory -- vec0::tests` | ❌ W0 | ⬜ pending |
| 65-01-03 | 01 | 1 | VEC-01 | unit | `cargo test -p blufio-memory -- vec0::tests::search` | ❌ W0 | ⬜ pending |
| 65-01-04 | 01 | 1 | VEC-03 | unit | `cargo test -p blufio-memory -- vec0::tests::filter` | ❌ W0 | ⬜ pending |
| 65-02-01 | 02 | 1 | VEC-01 | integration | `cargo test -p blufio --test e2e_vec0 -- toggle` | ❌ W0 | ⬜ pending |
| 65-02-02 | 02 | 1 | VEC-01 | unit | `cargo test -p blufio-memory -- vec0::tests::populate` | ❌ W0 | ⬜ pending |
| 65-02-03 | 02 | 1 | VEC-01 | unit | `cargo test -p blufio-memory -- vec0::tests::dual_write` | ❌ W0 | ⬜ pending |
| 65-03-01 | 03 | 2 | VEC-01 | integration | `cargo test -p blufio-memory -- vec0::tests::parity` | ❌ W0 | ⬜ pending |
| 65-03-02 | 03 | 2 | VEC-01 | bench | `cargo bench -p blufio -- vec0` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/blufio-memory/src/vec0.rs` — vec0 module stubs (registration, search, sync)
- [ ] `crates/blufio/tests/e2e_vec0.rs` — SQLCipher + vec0 integration test stubs
- [ ] `crates/blufio/benches/bench_vec0.rs` — vec0 vs in-memory benchmark stubs
- [ ] sqlite-vec dependency: `sqlite-vec = "0.1.6"` at workspace level

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `blufio doctor` vec0 health check | VEC-01 | CLI output inspection | Run `blufio doctor` and verify vec0 section shows: extension loaded, row count, sync status |
| `blufio memory rebuild-vec0` | VEC-01 | Destructive recovery | Run rebuild-vec0, verify vec0 table matches memories table after rebuild |
| Fallback log rate-limiting | VEC-01 | Timing-dependent | Trigger vec0 failures, check logs show first 5 then every 60s |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
