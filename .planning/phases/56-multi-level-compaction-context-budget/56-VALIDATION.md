---
phase: 56
slug: multi-level-compaction-context-budget
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-11
---

# Phase 56 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `cargo test` / `cargo nextest run` |
| **Config file** | `.config/nextest.toml` |
| **Quick run command** | `cargo nextest run -p blufio-context` |
| **Full suite command** | `cargo nextest run -p blufio-context -p blufio-config -p blufio-storage -p blufio-bus -p blufio-prometheus -p blufio` |
| **Estimated runtime** | ~45 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo nextest run -p blufio-context`
- **After every plan wave:** Run `cargo nextest run -p blufio-context -p blufio-config -p blufio-storage -p blufio-bus -p blufio-prometheus -p blufio`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 45 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| *Populated during planning* | | | | | | | |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `blufio-context/tests/compaction_levels.rs` — stubs for COMP-01, COMP-02
- [ ] `blufio-context/tests/compaction_quality.rs` — stubs for COMP-03, COMP-04
- [ ] `blufio-context/tests/compaction_archive.rs` — stubs for COMP-05, COMP-06
- [ ] `blufio-context/tests/zone_budget.rs` — stubs for CTXE-01, CTXE-02, CTXE-03

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| L3 archive generated on session close | COMP-05 | Requires real session lifecycle | Start session, add messages past hard trigger, close session, verify archive row in DB |
| Cross-session archive quality | COMP-06 | Requires multiple session histories | Run 3+ sessions, verify deep archive merge produces coherent summary |
| Deprecation warning for old config key | COMP-01 | Requires log inspection | Set `compaction_threshold = 0.7` in config, start server, verify warning logged |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 45s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
