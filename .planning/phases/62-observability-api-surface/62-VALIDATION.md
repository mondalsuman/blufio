---
phase: 62
slug: observability-api-surface
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-13
---

# Phase 62 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in) + insta for snapshots |
| **Config file** | workspace Cargo.toml (test profiles) |
| **Quick run command** | `cargo test -p blufio-gateway --lib` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~45 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace --lib`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green + `cargo test --workspace --features otel`
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 62-01-01 | 01 | 1 | OTEL-01 | unit | `cargo test -p blufio --lib otel --features otel` | ❌ W0 | ⬜ pending |
| 62-01-02 | 01 | 1 | OTEL-02 | unit | `cargo test -p blufio --lib otel --features otel` | ❌ W0 | ⬜ pending |
| 62-01-03 | 01 | 1 | OTEL-03 | unit | `cargo test -p blufio-agent --lib -- otel --features otel` | ❌ W0 | ⬜ pending |
| 62-01-04 | 01 | 1 | OTEL-04 | unit | `cargo test -p blufio-mcp-client --lib -- trace --features otel` | ❌ W0 | ⬜ pending |
| 62-01-05 | 01 | 1 | OTEL-05 | smoke | `cargo build 2>&1 && cargo tree -p blufio \| grep -c opentelemetry` | ❌ W0 | ⬜ pending |
| 62-01-06 | 01 | 1 | OTEL-06 | integration | `cargo test -p blufio --lib -- prometheus_otel --features otel,prometheus` | ❌ W0 | ⬜ pending |
| 62-02-01 | 02 | 1 | OAPI-01 | unit | `cargo test -p blufio-gateway --lib -- openapi` | ❌ W0 | ⬜ pending |
| 62-02-02 | 02 | 1 | OAPI-02 | unit | `cargo test -p blufio-gateway --lib -- openapi_json` | ❌ W0 | ⬜ pending |
| 62-02-03 | 02 | 1 | OAPI-03 | unit | `cargo test -p blufio-gateway --lib -- swagger` | ❌ W0 | ⬜ pending |
| 62-02-04 | 02 | 1 | OAPI-04 | snapshot | `cargo test -p blufio-gateway --lib -- openapi_spec_snapshot` | ❌ W0 | ⬜ pending |
| 62-03-01 | 03 | 1 | LITE-01 | unit | `cargo test -p blufio --lib -- litestream_init` | ❌ W0 | ⬜ pending |
| 62-03-02 | 03 | 1 | LITE-02 | unit | `cargo test -p blufio --lib -- litestream_status` | ❌ W0 | ⬜ pending |
| 62-03-03 | 03 | 1 | LITE-03 | unit | `cargo test -p blufio --lib -- litestream_sqlcipher` | ❌ W0 | ⬜ pending |
| 62-03-04 | 03 | 1 | LITE-04 | unit | `cargo test -p blufio-config --lib -- litestream` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `insta` crate added to workspace dev-dependencies
- [ ] Snapshot directory `crates/blufio-gateway/src/snapshots/` created
- [ ] CI workflow updated with `--features otel` matrix entry
- [ ] CI workflow updated with `--features otel,prometheus` matrix entry
- [ ] Test stubs for all OTEL-* requirements in otel feature-gated test modules
- [ ] Test stubs for all OAPI-* requirements in blufio-gateway
- [ ] Test stubs for all LITE-* requirements in blufio/blufio-config

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Swagger UI renders correctly at /docs | OAPI-03 | Visual rendering verification | Start server, navigate to /docs, verify interactive UI loads |
| Litestream config template works with real Litestream binary | LITE-01 | Requires external binary | Install Litestream, run `blufio litestream init`, validate generated YAML |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
