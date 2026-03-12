---
phase: 61
slug: channel-adapters
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-13
---

# Phase 61 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test + tokio::test |
| **Config file** | None (Cargo-standard test discovery) |
| **Quick run command** | `cargo test -p blufio-email -p blufio-imessage -p blufio-sms` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p blufio-email -p blufio-imessage -p blufio-sms`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 61-01-01 | 01 | 1 | CHAN-01 | unit | `cargo test -p blufio-email` | ❌ W0 | ⬜ pending |
| 61-01-02 | 01 | 1 | CHAN-02 | unit | `cargo test -p blufio-email -- imap::tests` | ❌ W0 | ⬜ pending |
| 61-02-01 | 02 | 1 | CHAN-03 | unit | `cargo test -p blufio-imessage` | ❌ W0 | ⬜ pending |
| 61-02-02 | 02 | 1 | CHAN-04 | manual-only | N/A (doc review) | N/A | ⬜ pending |
| 61-03-01 | 03 | 1 | CHAN-05 | unit | `cargo test -p blufio-sms` | ❌ W0 | ⬜ pending |
| 61-04-01 | 04 | 2 | CHAN-06 | unit | `cargo test -p blufio-email -p blufio-imessage -p blufio-sms -- adapter` | ❌ W0 | ⬜ pending |
| 61-04-02 | 04 | 2 | CHAN-07 | unit | `cargo test -p blufio-email -p blufio-imessage -p blufio-sms -- format` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/blufio-email/` — new crate, all tests needed
- [ ] `crates/blufio-imessage/` — new crate, all tests needed
- [ ] `crates/blufio-sms/` — new crate, all tests needed
- [ ] Email MIME parsing tests with real-world email fixtures (Gmail, Outlook, Apple Mail)
- [ ] Quoted-text stripping tests covering all three client patterns
- [ ] Twilio HMAC-SHA1 validation tests with known test vectors
- [ ] BlueBubbles webhook payload parsing with sample JSON fixtures
- [ ] Config validation tests for each adapter (missing fields, invalid formats)
- [ ] E.164 phone number format validation tests

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| iMessage experimental docs | CHAN-04 | Documentation review | Verify README/docs mark iMessage as experimental, mention macOS host requirement |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
