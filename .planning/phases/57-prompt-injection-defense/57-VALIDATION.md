---
phase: 57
slug: prompt-injection-defense
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-03-12
---

# Phase 57 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[cfg(test)]` + `proptest` for property tests |
| **Config file** | `crates/blufio-injection/Cargo.toml` `[dev-dependencies]` |
| **Quick run command** | `cargo test -p blufio-injection` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~15 seconds (new crate unit tests) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio-injection`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 57-01-01 | 01 | 1 | INJC-01 | unit | `cargo test -p blufio-injection -- classifier` | ❌ W0 | ⬜ pending |
| 57-01-02 | 01 | 1 | INJC-01 | unit | `cargo test -p blufio-injection -- classifier::clean` | ❌ W0 | ⬜ pending |
| 57-01-03 | 01 | 1 | INJC-02 | unit | `cargo test -p blufio-injection -- classifier::log_mode` | ❌ W0 | ⬜ pending |
| 57-01-04 | 01 | 1 | INJC-02 | unit | `cargo test -p blufio-injection -- classifier::blocking` | ❌ W0 | ⬜ pending |
| 57-02-01 | 02 | 1 | INJC-03 | unit | `cargo test -p blufio-injection -- boundary::roundtrip` | ❌ W0 | ⬜ pending |
| 57-02-02 | 02 | 1 | INJC-03 | unit | `cargo test -p blufio-injection -- boundary::strip` | ❌ W0 | ⬜ pending |
| 57-02-03 | 02 | 1 | INJC-03 | unit | `cargo test -p blufio-injection -- boundary::tamper` | ❌ W0 | ⬜ pending |
| 57-03-01 | 03 | 2 | INJC-04 | unit | `cargo test -p blufio-injection -- output_screen::credentials` | ❌ W0 | ⬜ pending |
| 57-03-02 | 03 | 2 | INJC-04 | unit | `cargo test -p blufio-injection -- output_screen::relay` | ❌ W0 | ⬜ pending |
| 57-04-01 | 04 | 2 | INJC-05 | unit | `cargo test -p blufio-injection -- hitl::timeout` | ❌ W0 | ⬜ pending |
| 57-04-02 | 04 | 2 | INJC-05 | unit | `cargo test -p blufio-injection -- hitl::safe_tools` | ❌ W0 | ⬜ pending |
| 57-05-01 | 05 | 3 | INJC-06 | integration | `cargo test -p blufio-injection -- integration::mcp` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/blufio-injection/` — new crate directory and manifest
- [ ] `crates/blufio-injection/src/lib.rs` — crate root with re-exports
- [ ] `crates/blufio-injection/src/classifier.rs` — L1 test stubs for INJC-01, INJC-02
- [ ] `crates/blufio-injection/src/boundary.rs` — L3 test stubs for INJC-03
- [ ] `crates/blufio-injection/src/output_screen.rs` — L4 test stubs for INJC-04
- [ ] `crates/blufio-injection/src/hitl.rs` — L5 test stubs for INJC-05
- [ ] Attack corpus test data (known injection patterns for validation)

*Existing infrastructure covers: regex (workspace), ring (workspace), hex (workspace), blufio-bus, blufio-config, blufio-security*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| HITL interactive confirmation flow | INJC-05 | Requires human reply via channel | Start session, trigger MCP tool call, verify inline confirmation prompt appears, respond YES/NO, verify timeout auto-deny |
| Generic blocked message wording | INJC-02 | UI/UX verification | Send known injection text, verify only "I can't process this message." appears with no detection details |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
