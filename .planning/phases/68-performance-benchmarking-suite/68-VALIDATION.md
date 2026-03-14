---
phase: 68
slug: performance-benchmarking-suite
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-14
---

# Phase 68 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test + criterion benchmarks |
| **Config file** | crates/blufio/Cargo.toml (bench targets) |
| **Quick run command** | `cargo test -p blufio --lib bench` |
| **Full suite command** | `cargo bench -p blufio` |
| **Estimated runtime** | ~120 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio --lib bench`
- **After every plan wave:** Run `cargo bench -p blufio`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 68-01-01 | 01 | 1 | PERF-01 | integration | `cargo test -p blufio --lib bench` | ❌ W0 | ⬜ pending |
| 68-01-02 | 01 | 1 | PERF-02 | integration | `cargo test -p blufio --lib bench` | ❌ W0 | ⬜ pending |
| 68-02-01 | 02 | 1 | PERF-03 | bench | `cargo bench -p blufio -- vector_search` | ✅ | ⬜ pending |
| 68-02-02 | 02 | 1 | PERF-04 | bench | `cargo bench -p blufio -- injection` | ❌ W0 | ⬜ pending |
| 68-02-03 | 02 | 1 | PERF-05 | bench | `cargo bench -p blufio -- hybrid` | ❌ W0 | ⬜ pending |
| 68-03-01 | 03 | 2 | PERF-06 | manual | review docs/benchmarks.md | ❌ W0 | ⬜ pending |
| 68-04-01 | 04 | 2 | PERF-07 | CI | verify bench.yml changes | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/blufio/benches/bench_injection.rs` — stubs for PERF-04
- [ ] Extend `bench_vec0.rs` with 5K/10K — stubs for PERF-03 extended scales

*Existing infrastructure covers criterion framework and bench.yml.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| OpenClaw comparison accuracy | PERF-06 | External project metrics need manual verification | Review cited sources against docs/benchmarks.md claims |
| CI PR comment rendering | PERF-07 | Requires actual GitHub PR to verify | Create test PR and verify github-action-benchmark comment |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
