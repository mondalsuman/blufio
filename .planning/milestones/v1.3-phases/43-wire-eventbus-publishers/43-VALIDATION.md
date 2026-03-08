---
phase: 43
slug: wire-eventbus-publishers
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-08
---

# Phase 43 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml workspace |
| **Quick run command** | `cargo test -p blufio-agent --lib && cargo test -p blufio-skill --lib` |
| **Full suite command** | `cargo test -p blufio-agent && cargo test -p blufio-skill && cargo test -p blufio-bus && cargo test -p blufio-core` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio-agent --lib && cargo test -p blufio-skill --lib`
- **After every plan wave:** Run `cargo test -p blufio-agent && cargo test -p blufio-skill && cargo test -p blufio-bus && cargo test -p blufio-core`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 43-01-01 | 01 | 1 | API-16 | unit | `cargo test -p blufio-agent test_event_bus` | ❌ W0 | ⬜ pending |
| 43-01-02 | 01 | 1 | API-16 | unit | `cargo test -p blufio-agent test_message_sent` | ❌ W0 | ⬜ pending |
| 43-02-01 | 02 | 1 | API-16 | unit | `cargo test -p blufio-skill test_event_bus` | ❌ W0 | ⬜ pending |
| 43-02-02 | 02 | 1 | API-16 | unit | `cargo test -p blufio-skill test_skill_event` | ❌ W0 | ⬜ pending |
| 43-03-01 | 03 | 2 | API-16 | integration | `cargo test -p blufio-test-utils webhook_e2e` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- Existing infrastructure covers all phase requirements. Tests will be added alongside implementation in each plan.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| serve.rs wiring compiles | API-16 | Build verification | `cargo build` after wiring changes |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
