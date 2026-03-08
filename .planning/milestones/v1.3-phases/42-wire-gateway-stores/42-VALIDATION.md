---
phase: 42
slug: wire-gateway-stores
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-07
---

# Phase 42 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in) |
| **Config file** | Cargo.toml workspace |
| **Quick run command** | `cargo check -p blufio --features "gateway,anthropic,openai,ollama,openrouter,gemini,sqlite,keypair"` |
| **Full suite command** | `cargo test -p blufio --features "gateway,anthropic,openai,ollama,openrouter,gemini,sqlite,keypair" && cargo clippy -p blufio --features "gateway,anthropic,openai,ollama,openrouter,gemini,sqlite,keypair" -- -D warnings` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo check -p blufio --features "gateway,anthropic,openai,ollama,openrouter,gemini,sqlite,keypair"`
- **After every plan wave:** Run full suite command
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 42-01-01 | 01 | 1 | API-11..14 | compile+unit | `cargo check -p blufio --features "gateway,sqlite"` | ✅ | ⬜ pending |
| 42-01-02 | 01 | 1 | API-15..18 | compile+unit | `cargo check -p blufio --features "gateway,sqlite"` | ✅ | ⬜ pending |
| 42-02-01 | 02 | 2 | API-15,16 | compile+unit | `cargo test -p blufio --features "gateway,sqlite"` | ✅ | ⬜ pending |
| 42-02-02 | 02 | 2 | API-11..18 | integration | `cargo clippy --workspace -- -D warnings` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. Store tests exist in blufio-gateway (53+ tests). This phase is pure wiring — compilation and clippy are the primary gates.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| API key auth flow | API-11..14 | Requires running server + HTTP client | `curl -X POST http://localhost:3000/v1/api-keys -H "Authorization: Bearer <token>"` |
| Webhook delivery | API-15,16 | Requires running server + webhook endpoint | Register webhook, send message, verify delivery |
| Batch submission | API-17,18 | Requires running server + batch payload | `curl -X POST http://localhost:3000/v1/batch` |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
