---
phase: 60
slug: gdpr-tooling-data-export
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-12
---

# Phase 60 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in + proptest + tokio::test |
| **Config file** | Cargo.toml [dev-dependencies] per crate |
| **Quick run command** | `cargo test -p blufio-gdpr` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~45 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio-gdpr`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 45 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 60-01-01 | 01 | 1 | GDPR-01 | integration | `cargo test -p blufio-gdpr -- erasure_completeness` | ❌ W0 | ⬜ pending |
| 60-01-02 | 01 | 1 | GDPR-01 | integration | `cargo test -p blufio-gdpr -- proptest_erasure` | ❌ W0 | ⬜ pending |
| 60-01-03 | 01 | 1 | GDPR-02 | unit | `cargo test -p blufio-gdpr -- cost_anonymization` | ❌ W0 | ⬜ pending |
| 60-01-04 | 01 | 1 | GDPR-03 | integration | `cargo test -p blufio-gdpr -- audit_erasure` | ❌ W0 | ⬜ pending |
| 60-01-05 | 01 | 1 | GDPR-04 | unit | `cargo test -p blufio-gdpr -- report_counts` | ❌ W0 | ⬜ pending |
| 60-01-06 | 01 | 1 | GDPR-05 | integration | `cargo test -p blufio-gdpr -- export_before_erase_safety` | ❌ W0 | ⬜ pending |
| 60-01-07 | 01 | 1 | GDPR-06 | unit | `cargo test -p blufio-gdpr -- export_json_csv` | ❌ W0 | ⬜ pending |
| 60-01-08 | 01 | 1 | GDPR-06 | unit | `cargo test -p blufio-gdpr -- export_redaction` | ❌ W0 | ⬜ pending |
| 60-01-09 | 01 | 1 | N/A | unit | `cargo test -p blufio-gdpr -- active_session_refusal` | ❌ W0 | ⬜ pending |
| 60-01-10 | 01 | 1 | N/A | integration | `cargo test -p blufio-gdpr -- dry_run` | ❌ W0 | ⬜ pending |
| 60-01-11 | 01 | 1 | N/A | unit | `cargo test -p blufio-gdpr -- timeout` | ❌ W0 | ⬜ pending |
| 60-01-12 | 01 | 1 | N/A | unit | `cargo test -p blufio-gdpr -- csv_escaping` | ❌ W0 | ⬜ pending |
| 60-01-13 | 01 | 1 | N/A | snapshot | `cargo test -p blufio-gdpr -- golden` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/blufio-gdpr/Cargo.toml` — new crate with dev-dependencies (proptest, tempfile)
- [ ] `crates/blufio-gdpr/src/lib.rs` — crate root with module declarations
- [ ] `crates/blufio-gdpr/tests/integration_tests.rs` — GDPR completeness, export-then-erase, audit redaction
- [ ] `crates/blufio-gdpr/tests/golden/` — snapshot files for JSON/CSV export format

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Interactive confirmation prompt | N/A | Requires TTY input | Run `blufio gdpr erase --user test` and verify prompt appears |
| Colored CLI output | N/A | Visual verification | Run commands and verify green/red/yellow/cyan output |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 45s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
