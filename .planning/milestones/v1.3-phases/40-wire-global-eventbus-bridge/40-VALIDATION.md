---
phase: 40
slug: wire-global-eventbus-bridge
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-07
---

# Phase 40 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Workspace Cargo.toml |
| **Quick run command** | `cargo test -p blufio-bus && cargo test -p blufio-bridge` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio-bus && cargo test -p blufio-bridge && cargo check -p blufio`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 40-01-01 | 01 | 1 | INFRA-01 | unit | `cargo test -p blufio-bus` | Yes (8 tests) | pending |
| 40-01-02 | 01 | 1 | INFRA-02 | unit | `cargo test -p blufio-bus -- events` | Yes (5 tests) | pending |
| 40-01-03 | 01 | 1 | INFRA-03 | unit | `cargo test -p blufio-bus -- reliable` | Yes (2 tests) | pending |
| 40-02-01 | 02 | 1 | INFRA-06 | unit | `cargo test -p blufio-bridge` | Yes (6 tests) | pending |
| 40-02-02 | 02 | 1 | INFRA-06 | compile | `cargo check -p blufio` | Yes | pending |

*Status: pending · green · red · flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. The blufio-bus and blufio-bridge crates already have comprehensive unit tests. This phase's validation is primarily compile-check (serve.rs wiring compiles) plus existing test suites passing.

- [ ] ChannelMultiplexer `set_event_bus()` method needs unit test
- [ ] ChannelMultiplexer `connected_channels_ref()` method needs unit test

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Bridge dispatch in serve.rs | INFRA-06 | Full integration requires multi-adapter runtime | Verify via `cargo check -p blufio` compile check and code inspection of wiring |

---

## Validation Sign-Off

- [ ] All tasks have automated verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
