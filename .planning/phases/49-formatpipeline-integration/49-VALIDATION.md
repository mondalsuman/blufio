---
phase: 49
slug: formatpipeline-integration
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-09
---

# Phase 49 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in Rust test framework) |
| **Config file** | Cargo.toml `[dev-dependencies]` in each crate |
| **Quick run command** | `cargo test -p blufio-core --lib format` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio-core --lib format`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 49-01-01 | 01 | 1 | FMT-04 | unit | `cargo test -p blufio-core --lib format::tests::detect` | Wave 0 | ⬜ pending |
| 49-01-02 | 01 | 1 | FMT-05 | unit | `cargo test -p blufio-core --lib format::tests::split` | Wave 0 | ⬜ pending |
| 49-01-03 | 01 | 1 | FMT-05 | unit | `cargo test -p blufio-core --lib format::tests::split_atomic` | Wave 0 | ⬜ pending |
| 49-01-04 | 01 | 1 | FMT-05 | unit | `cargo test -p blufio-core --lib format::tests::split_oversized` | Wave 0 | ⬜ pending |
| 49-01-05 | 01 | 1 | FMT-06 | unit | `cargo test -p blufio-core --lib format::tests::html_table` | Wave 0 | ⬜ pending |
| 49-02-01 | 02 | 2 | FMT-04 | integration | `cargo test -p blufio-telegram --lib tests` | Existing (modify) | ⬜ pending |
| 49-02-02 | 02 | 2 | FMT-06 | integration | `cargo test -p blufio-discord --lib tests` | Existing (modify) | ⬜ pending |
| 49-02-03 | 02 | 2 | FMT-06 | integration | `cargo test -p blufio-slack --lib tests` | Existing (modify) | ⬜ pending |
| 49-02-04 | 02 | 2 | CAP-04 | unit | `cargo test --workspace -- capabilities` | Existing (verify) | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `blufio-core/src/format.rs` — test functions for `detect_and_format()` (regex table/list/code detection)
- [ ] `blufio-core/src/format.rs` — test functions for `split_at_paragraphs()` (comprehensive matrix)
- [ ] `blufio-core/src/format.rs` — test functions for HTML table generation (Tier 0)
- [ ] Comprehensive test matrix: {short, at-limit, over-limit} x {plain, code block, list, table, mixed} x {channel limits}

*Existing infrastructure covers adapter-level capability tests.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Formatting fallback increments Prometheus counter | FMT-06 | Requires live API rejection | Send MarkdownV2 with invalid escaping, verify `blufio_format_fallback_total` increments |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
