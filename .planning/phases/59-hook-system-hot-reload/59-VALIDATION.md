---
phase: 59
slug: hook-system-hot-reload
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-12
---

# Phase 59 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml (workspace) |
| **Quick run command** | `cargo test -p blufio-hooks --lib` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio-hooks --lib && cargo test -p blufio-config --lib`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 59-01-01 | 01 | 1 | HOOK-01, HOOK-02 | unit | `cargo test -p blufio-hooks --lib` | ❌ W0 | ⬜ pending |
| 59-01-02 | 01 | 1 | HTRL-01, HTRL-04 | unit | `cargo test -p blufio-config --lib` | ❌ W0 | ⬜ pending |
| 59-02-01 | 02 | 2 | HOOK-03, HOOK-04, HOOK-06 | unit+integration | `cargo test -p blufio-hooks` | ❌ W0 | ⬜ pending |
| 59-02-02 | 02 | 2 | HOOK-05 | unit | `cargo test -p blufio-hooks --lib` | ❌ W0 | ⬜ pending |
| 59-03-01 | 03 | 2 | HTRL-01, HTRL-04, HTRL-05, HTRL-06 | unit+integration | `cargo test -p blufio-hooks` | ❌ W0 | ⬜ pending |
| 59-03-02 | 03 | 2 | HTRL-02, HTRL-03 | unit | `cargo test -p blufio-hooks --lib` | ❌ W0 | ⬜ pending |
| 59-04-01 | 04 | 3 | HOOK-05, HTRL-04, HTRL-06 | integration | `cargo test --workspace` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/blufio-hooks/src/lib.rs` — crate scaffolding with pub modules
- [ ] `crates/blufio-hooks/Cargo.toml` — workspace member with test dependencies
- [ ] Existing `cargo test` infrastructure covers framework needs

*Existing infrastructure covers framework requirements. Wave 0 creates new crate structure.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| TLS cert hot reload in live server | HTRL-02 | Requires running axum server with TLS | Start serve with TLS, replace cert file, verify new connections use new cert |
| Hook shell execution with restricted PATH | HOOK-04 | Platform-specific PATH restriction | Define hook with restricted PATH, verify commands outside PATH fail |
| Config file edit triggers reload | HTRL-01 | Requires running server with file watcher | Start serve, edit blufio.toml, verify config_reloaded event fires |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
