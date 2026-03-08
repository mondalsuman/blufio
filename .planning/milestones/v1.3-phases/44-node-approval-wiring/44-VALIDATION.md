---
phase: 44
slug: node-approval-wiring
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-08
---

# Phase 44 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml workspace |
| **Quick run command** | `cargo test -p blufio-node --lib` |
| **Full suite command** | `cargo test -p blufio-node -p blufio-bus` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio-node --lib`
- **After every plan wave:** Run `cargo test -p blufio-node -p blufio-bus`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 44-01-01 | 01 | 1 | NODE-05 | unit | `cargo test -p blufio-bus bus_event_to_type_string` | ❌ W0 | ⬜ pending |
| 44-01-02 | 01 | 1 | NODE-05 | unit | `cargo test -p blufio-node approval_subscription` | ❌ W0 | ⬜ pending |
| 44-02-01 | 02 | 1 | NODE-05 | unit | `cargo test -p blufio-node connection_forwarding` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Tests for `bus_event_to_type_string()` helper mapping
- [ ] Tests for approval subscription event filtering
- [ ] Tests for ConnectionManager forwarding to ApprovalRouter

*Existing test infrastructure (cargo test) covers all framework requirements.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| serve.rs wiring order | NODE-05 | Startup integration requires running binary | Start with `cargo run -- serve`, verify no panics |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
