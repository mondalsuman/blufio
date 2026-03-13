---
phase: 66
slug: injection-defense-hardening
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-13
---

# Phase 66 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test framework + cargo test |
| **Config file** | Workspace Cargo.toml + crates/blufio-injection/Cargo.toml |
| **Quick run command** | `cargo test -p blufio-injection --lib` |
| **Full suite command** | `cargo test -p blufio-injection` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio-injection --lib`
- **After every plan wave:** Run `cargo test -p blufio-injection && cargo test -p blufio-config --lib`
- **Before `/gsd:verify-work`:** Full suite must be green + `cargo clippy -p blufio-injection -- -D warnings` + `cargo fmt -- --check`
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 66-01-01 | 01 | 1 | INJ-01 | unit | `cargo test -p blufio-injection normalize` | ❌ W0 | ⬜ pending |
| 66-01-02 | 01 | 1 | INJ-02 | unit | `cargo test -p blufio-injection base64` | ❌ W0 | ⬜ pending |
| 66-01-03 | 01 | 1 | INJ-04 | unit | `cargo test -p blufio-injection extract` | ❌ W0 | ⬜ pending |
| 66-02-01 | 02 | 1 | INJ-03 | unit | `cargo test -p blufio-injection patterns` | ✅ partial | ⬜ pending |
| 66-02-02 | 02 | 1 | INJ-05 | unit | `cargo test -p blufio-injection language` | ❌ W0 | ⬜ pending |
| 66-03-01 | 03 | 2 | INJ-06 | unit | `cargo test -p blufio-injection weight` | ❌ W0 | ⬜ pending |
| 66-03-02 | 03 | 2 | INJ-07 | unit | `cargo test -p blufio-injection canary` | ❌ W0 | ⬜ pending |
| 66-04-01 | 04 | 3 | INJ-08 | integration | `cargo test -p blufio-injection --test corpus_validation` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/blufio-injection/src/normalize.rs` — new module for NFKC normalization, zero-width stripping, confusable mapping (INJ-01, INJ-02, INJ-04)
- [ ] `crates/blufio-injection/src/canary.rs` — new module for canary token generation and detection (INJ-07)
- [ ] `crates/blufio-injection/tests/` — create tests directory
- [ ] `crates/blufio-injection/tests/fixtures/benign_corpus.json` — 100+ benign messages (INJ-08)
- [ ] `crates/blufio-injection/tests/fixtures/attack_corpus.json` — 50+ attack messages (INJ-08)
- [ ] `crates/blufio-injection/tests/corpus_validation.rs` — integration test for corpus validation (INJ-08)
- [ ] Add `unicode-normalization = "0.1"` to `crates/blufio-injection/Cargo.toml`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `blufio injection test <text>` shows normalization output | INJ-01 | CLI output formatting | Run `blufio injection test "tеst"` (Cyrillic e), verify output shows original vs normalized |
| `blufio injection test-canary` works end-to-end | INJ-07 | Requires running server | Start server, run `blufio injection test-canary`, verify detection |
| `blufio injection config` shows weights | INJ-06 | CLI output formatting | Run command, verify all 8 categories shown with defaults |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
