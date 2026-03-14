---
phase: 69
slug: cross-phase-integration-validation
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-14
---

# Phase 69 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust), Criterion benchmarks |
| **Config file** | Cargo.toml workspace |
| **Quick run command** | `cargo test -p blufio --test e2e_integration` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~120 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio --test e2e_integration`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 69-01-01 | 01 | 1 | VEC-05 | integration | `cargo test -p blufio --test e2e_integration` | ❌ W0 | ⬜ pending |
| 69-01-02 | 01 | 1 | PERF-05 | integration | `cargo test -p blufio --test e2e_integration` | ❌ W0 | ⬜ pending |
| 69-02-01 | 02 | 1 | VEC-05,PERF-05 | benchmark | `cargo bench -p blufio --bench bench_hybrid` | ✅ | ⬜ pending |
| 69-03-01 | 03 | 2 | PERF-06 | verification | `cargo test --workspace` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/blufio/tests/e2e_integration.rs` — new cross-subsystem integration test file
- Existing infrastructure covers benchmark and regression requirements

*Existing test infrastructure (e2e_vec0.rs, bench_hybrid.rs, corpus_validation.rs) covers most phase requirements.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| OpenClaw comparison fairness | PERF-06 | Subjective accuracy assessment | Review docs/benchmarks.md for factual claims |
| Swagger UI renders correctly | N/A | Visual check | Open /docs endpoint, verify rendering |
| CLI help text consistency | N/A | Human readability | Run --help on all v1.6 commands |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
