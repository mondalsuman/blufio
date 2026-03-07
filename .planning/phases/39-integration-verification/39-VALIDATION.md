---
phase: 39
slug: integration-verification
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-07
---

# Phase 39 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) + tokio::test for async |
| **Config file** | Cargo.toml workspace test settings |
| **Quick run command** | `cargo test -p blufio-test-utils` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~120 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p {relevant-crate} -x`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 39-01-01 | 01 | 1 | All v1.3 reqs | audit + unit | `cargo test --workspace` | Yes | pending |
| 39-01-02 | 01 | 1 | Traceability | audit | manual REQUIREMENTS.md update | Yes | pending |
| 39-02-01 | 02 | 2 | Flow 1-4 | integration | `cargo test -p blufio-test-utils --test integration_flows` | No -- Wave 0 | pending |
| 39-02-02 | 02 | 2 | Docker | build + audit | `docker build .` | Dockerfile exists | pending |

*Status: pending / green / red / flaky*

---

## Wave 0 Requirements

- [ ] `crates/blufio-test-utils/tests/integration_flows.rs` -- 4 E2E flow test stubs
- [ ] blufio-test-utils Cargo.toml -- add dev-deps: wiremock, blufio-gateway, blufio-bus, blufio-openrouter, blufio-discord
- [ ] Any missing per-crate tests identified during verification audit

*Existing infrastructure covers most phase requirements. Only integration flow tests need new files.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Docker image build | INFRA-04 | Requires Docker daemon | `docker build -t blufio:test .` and verify image size <200MB |
| docker-compose.yml validity | INFRA-05 | Requires Docker daemon | `docker-compose config` validates syntax |
| Healthcheck endpoint | INFRA-05 | Requires running container | `docker run blufio:test blufio healthcheck` |

---

## Validation Sign-Off

- [ ] All tasks have automated verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
