---
phase: 41
slug: wire-provider-registry
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-07
---

# Phase 41 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in #[cfg(test)] + tokio::test |
| **Config file** | Workspace Cargo.toml |
| **Quick run command** | `cargo test -p blufio --lib providers` |
| **Full suite command** | `cargo test -p blufio` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio --lib providers`
- **After every plan wave:** Run `cargo test -p blufio`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 41-01-01 | 01 | 1 | WIRING-01 | unit | `cargo test -p blufio --lib providers` | Wave 0 | pending |
| 41-01-02 | 01 | 1 | WIRING-02 | unit | `cargo test -p blufio --lib providers` | Wave 0 | pending |
| 41-01-03 | 01 | 1 | WIRING-03 | unit | `cargo test -p blufio --lib providers` | Wave 0 | pending |
| 41-01-04 | 01 | 1 | WIRING-04 | unit | `cargo test -p blufio --lib providers` | Wave 0 | pending |
| 41-01-05 | 01 | 1 | WIRING-05 | unit | `cargo test -p blufio --lib providers` | Wave 0 | pending |
| 41-01-06 | 01 | 1 | WIRING-06 | unit | `cargo test -p blufio --lib providers` | Wave 0 | pending |
| 41-02-01 | 02 | 1 | WIRING-07 | build | `cargo check -p blufio --all-features` | N/A | pending |

*Status: pending / green / red / flaky*

---

## Wave 0 Requirements

- [ ] `crates/blufio/src/providers.rs` — new file with unit tests for ConcreteProviderRegistry
- [ ] Feature flags `openai`, `ollama`, `openrouter`, `gemini` in Cargo.toml

*Existing infrastructure covers most phase requirements.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Ollama soft health check | PROV-04/05 | Requires running Ollama | Start Ollama, verify provider registers |

---

## Validation Sign-Off

- [ ] All tasks have automated verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
