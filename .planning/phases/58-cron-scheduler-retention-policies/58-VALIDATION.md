---
phase: 58
slug: cron-scheduler-retention-policies
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-03-12
---

# Phase 58 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml workspace |
| **Quick run command** | `cargo test -p blufio-cron` |
| **Full suite command** | `cargo test -p blufio-cron && cargo test -p blufio && cargo check` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio-cron`
- **After every plan wave:** Run `cargo test -p blufio-cron && cargo test -p blufio && cargo check`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 58-01-01 | 01 | 1 | CRON-01 | unit | `cargo test -p blufio-cron -- cron_config` | ❌ W0 | ⬜ pending |
| 58-01-02 | 01 | 1 | CRON-06 | unit | `cargo test -p blufio-cron -- last_run` | ❌ W0 | ⬜ pending |
| 58-02-01 | 02 | 1 | RETN-01 | unit | `cargo test -p blufio-cron -- retention_config` | ❌ W0 | ⬜ pending |
| 58-02-02 | 02 | 1 | RETN-03,RETN-04,RETN-05 | unit | `cargo test -p blufio-cron -- soft_delete` | ❌ W0 | ⬜ pending |
| 58-03-01 | 03 | 2 | CRON-04,CRON-05 | integration | `cargo test -p blufio-cron -- scheduler` | ❌ W0 | ⬜ pending |
| 58-03-02 | 03 | 2 | RETN-02 | integration | `cargo test -p blufio-cron -- enforcement` | ❌ W0 | ⬜ pending |
| 58-04-01 | 04 | 3 | CRON-02 | unit | `cargo test -p blufio -- cron_cli` | ❌ W0 | ⬜ pending |
| 58-04-02 | 04 | 3 | CRON-03 | unit | `cargo test -p blufio-cron -- systemd_timer` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/blufio-cron/` — new crate with Cargo.toml
- [ ] `crates/blufio-cron/src/lib.rs` — module structure
- [ ] Unit test stubs for CRON-01 through CRON-06 and RETN-01 through RETN-05

*Existing infrastructure (cargo test, rusqlite, tokio) covers framework needs.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| systemd timer activation | CRON-03 | Requires systemd runtime | Generate timer files, inspect unit syntax, `systemd-analyze verify` |
| Process restart persistence | CRON-06 | Requires process lifecycle | Stop blufio, restart, verify last-run timestamps survived |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
