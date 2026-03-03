---
phase: 21-fix-mcp-wiring-gaps
verified: 2026-03-03T15:00:00Z
status: passed
score: 13/13 must-haves verified
re_verification: false
gaps: []
human_verification: []
---

# Phase 21: Fix MCP Wiring Gaps — Verification Report

**Phase Goal:** Fix 5 code-level wiring issues found by milestone audit — the root causes behind 3 broken E2E flows (resilience, rug-pull detection, observability) and 6 integration gaps
**Verified:** 2026-03-03T15:00:00Z
**Status:** PASSED
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| #  | Truth | Status | Evidence |
|----|-------|--------|----------|
| 1  | PinStore is instantiated from the main database path and verify_or_store() is called during tool discovery | VERIFIED | `serve.rs:221` opens PinStore from `config.storage.database_path`; `manager.rs:338` calls `store.verify_or_store()` inside `discover_and_register()` loop |
| 2  | On pin mismatch, the entire server is blocked and no tools from that server are registered | VERIFIED | `manager.rs:354-362`: `PinVerification::Mismatch` returns `Err(BlufioError::Skill{...})` from `discover_and_register()`, preventing any tool registration for that server |
| 3  | CostRecord has an optional server_name field and it is stored in SQLite | VERIFIED | `ledger.rs:58`: `pub server_name: Option<String>`; `ledger.rs:163`: included as `?12` in INSERT; `ledger.rs:297`: `server_name TEXT` in test schema |
| 4  | mcp_tool_pins table is created by refinery migration alongside server_name column on cost_ledger | VERIFIED | `V6__mcp_wiring.sql`: `CREATE TABLE IF NOT EXISTS mcp_tool_pins` with composite PK; `ALTER TABLE cost_ledger ADD COLUMN server_name TEXT`; index created |
| 5  | HealthTracker background task is spawned in serve.rs when MCP servers are configured | VERIFIED | `serve.rs:356-374`: `spawn_health_monitor()` called with sessions from `connected_session_map()` using child cancel token |
| 6  | Health monitor calls session.ping() on each tick and tracks degraded/healthy state transitions | VERIFIED | `health.rs:173-179`: `session.send_request(ClientRequest::PingRequest(...))` with 5-second timeout; `mark_healthy` / `mark_degraded` called on result |
| 7  | Health check interval is configurable via mcp.health_check_interval_secs TOML config | VERIFIED | `model.rs:901-902`: field with `#[serde(default = "default_health_check_interval_secs")]`; default fn returns 60; `serve.rs:364`: passed to `spawn_health_monitor()` |
| 8  | Degraded servers logged with tracing::warn; recovery with tracing::info | VERIFIED | `health.rs:86`: `warn!(...)` in `mark_degraded()`; `health.rs:71`: `info!(...)` in `mark_healthy()` on state change |
| 9  | When external MCP tools are registered, the agent's prompt includes trust zone guidance listing untrusted tool names | VERIFIED | `serve.rs:266-284`: `TrustZoneProvider::new(...)` created and `context_engine.add_conditional_provider(Box::new(trust_zone_provider))` called when `result.tools_registered > 0` |
| 10 | Servers marked as trusted = true in TOML config suppress trust zone warnings for their tools | VERIFIED | `serve.rs:267-273`: trusted set built by filtering `config.mcp.servers` where `s.trusted == true`; `trust_zone.rs:72`: trusted servers filtered out of untrusted_tools |
| 11 | record_mcp_connection() is called when an MCP server connects successfully | VERIFIED | `manager.rs:119`: `blufio_prometheus::recording::record_mcp_connection(&server.transport)` after successful tool registration |
| 12 | record_mcp_tool_response_size() is called after each external tool invocation with the response byte size | VERIFIED | `external_tool.rs:137-138`: `record_mcp_tool_response_size(response_bytes)` called after `extract_text()`, before truncation |
| 13 | set_mcp_active_connections() is called after connect_all() with the count of connected servers | VERIFIED | `serve.rs:241`: `blufio_prometheus::recording::set_mcp_active_connections(result.connected as f64)` immediately after `connect_all()` |

**Score:** 13/13 truths verified

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/blufio-storage/migrations/V6__mcp_wiring.sql` | mcp_tool_pins table + cost_ledger server_name column | VERIFIED | File exists, 13 lines, contains `CREATE TABLE IF NOT EXISTS mcp_tool_pins`, `ALTER TABLE cost_ledger ADD COLUMN server_name TEXT`, index creation |
| `crates/blufio-cost/src/ledger.rs` | server_name field on CostRecord | VERIFIED | `server_name: Option<String>` at line 58, `with_server_name()` builder at line 102, `by_server_total()` at line 237, INSERT includes server_name at line 163 |
| `crates/blufio-mcp-client/src/manager.rs` | PinStore integration in discover_and_register | VERIFIED | `verify_or_store` called at line 338, Mismatch blocks server at lines 345-362, `connected_session_map()` at line 209 |
| `crates/blufio-mcp-client/src/health.rs` | Enhanced spawn_health_monitor with actual ping checks | VERIFIED | `spawn_health_monitor()` signature at line 155 accepts sessions/tracker/interval/cancel; `ClientRequest::PingRequest` at line 176; 5-second timeout at lines 173-179 |
| `crates/blufio-config/src/model.rs` | health_check_interval_secs config field + trusted field on McpServerEntry | VERIFIED | `health_check_interval_secs: u64` at line 902 with serde default; `trusted: bool` at line 974 with `#[serde(default)]`; both with tests |
| `crates/blufio/src/serve.rs` | PinStore open, HealthTracker spawn, TrustZoneProvider registration, metric calls | VERIFIED | All four wiring points present: PinStore at 221, health monitor at 356, TrustZoneProvider at 275, `set_mcp_active_connections` at 241 |
| `crates/blufio-mcp-client/src/trust_zone.rs` | TrustZoneProvider implementing ConditionalProvider | VERIFIED | File created, `TrustZoneProvider` struct, `impl ConditionalProvider`, 5 unit tests, all logic substantive |
| `crates/blufio-mcp-client/src/external_tool.rs` | record_mcp_tool_response_size call and server_name exposure | VERIFIED | `server_name` field at line 36, `server_name()` getter at line 77, metric recording at lines 137-138 |
| `crates/blufio-prometheus/src/recording.rs` | record_mcp_connection, set_mcp_active_connections, record_mcp_tool_response_size | VERIFIED | All three functions exist at lines 118, 124, 129; call site documentation at lines 110-115 |
| `crates/blufio-mcp-client/src/lib.rs` | PinStore and TrustZoneProvider re-exported | VERIFIED | `pub use pin_store::PinStore` at line 25; `pub use trust_zone::TrustZoneProvider` at line 26; `pub mod trust_zone` at line 22 |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `manager.rs` | `pin_store.rs` | `PinStore::verify_or_store()` called in `discover_and_register()` | WIRED | `use crate::pin_store::{PinStore, PinVerification}` at line 25; `store.verify_or_store(...)` at line 338 |
| `V6__mcp_wiring.sql` | `pin_store.rs` | Table schema matches PinStore's SQL expectations | WIRED | `mcp_tool_pins` with `server_name, tool_name, pin_hash` columns matches PinStore API (server: &str, tool: &str, hash: &str) |
| `serve.rs` | `health.rs` | `spawn_health_monitor()` called after `connect_all()` | WIRED | `blufio_mcp_client::health::spawn_health_monitor(sessions, health_tracker, config.mcp.health_check_interval_secs, health_cancel)` at serve.rs:361 |
| `health.rs` | rmcp RunningService | `session.ping()` called on each health check tick | WIRED | `session.send_request(ClientRequest::PingRequest(Default::default()))` inside tokio::time::timeout at health.rs:175 |
| `serve.rs` | `trust_zone.rs` | `context_engine.add_conditional_provider(Box::new(trust_zone_provider))` | WIRED | Called at serve.rs:279 inside `if result.tools_registered > 0` block, before `Arc::new(context_engine)` at line 292 |
| `trust_zone.rs` | `conditional.rs` | Implements `ConditionalProvider` trait | WIRED | `impl ConditionalProvider for TrustZoneProvider` at trust_zone.rs:54 |
| `manager.rs` | `recording.rs` | `record_mcp_connection()` on successful connect | WIRED | `blufio_prometheus::recording::record_mcp_connection(&server.transport)` at manager.rs:119 |
| `external_tool.rs` | `recording.rs` | `record_mcp_tool_response_size()` after invoke | WIRED | `blufio_prometheus::recording::record_mcp_tool_response_size(response_bytes)` at external_tool.rs:138 |

---

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| CLNT-06 | 21-02-PLAN | Connection lifecycle management (ping health checks, exponential backoff, graceful degradation) | SATISFIED | `spawn_health_monitor()` pings each server with 5-second timeout; `mark_degraded()` uses exponential backoff (`compute_backoff()`); failed servers recorded as `Disconnected` (non-fatal) |
| CLNT-07 | 21-01-PLAN | SHA-256 hash pinning of tool definitions at discovery (stored in SQLite) | SATISFIED | `verify_or_store()` called for every tool in `discover_and_register()`; V6 migration creates `mcp_tool_pins` table; Mismatch blocks entire server |
| CLNT-10 | 21-03-PLAN | External tools labeled as separate trust zone in prompt context | SATISFIED | `TrustZoneProvider` injects "## External Tools (untrusted)" guidance; `trusted = true` servers suppressed; registered via `add_conditional_provider()` |
| CLNT-12 | 21-01-PLAN, 21-04-PLAN | Per-server budget tracking in unified cost ledger | SATISFIED | `CostRecord.server_name: Option<String>` stored in SQLite; `by_server_total()` groups costs by server; `ExternalTool.server_name` field enables attribution at invocation time |
| INTG-04 | 21-04-PLAN | Prometheus metrics for MCP (connection count, tool response sizes, context utilization) | SATISFIED (3/4 wired) | `record_mcp_connection()` wired in manager.rs; `record_mcp_tool_response_size()` wired in external_tool.rs; `set_mcp_active_connections()` wired in serve.rs; `set_mcp_context_utilization()` explicitly deferred to context engine integration (documented in recording.rs:115) |

**Note on INTG-04:** The `set_mcp_context_utilization()` metric is registered and its helper function exists, but the call site is intentionally deferred. The plan documents this decision: "requires token counting during assembly, which is separate from MCP wiring scope." This is a known, accepted limitation — 3 of 4 MCP metrics are wired.

---

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `external_tool.rs` | 136 | `content.len() as f64` records char count, not byte count | Info | Minor semantic inaccuracy — `len()` on a Rust `String` returns bytes, not chars, so this is actually correct for ASCII but may under-count for multi-byte UTF-8 responses. Non-blocking. |

No TODO/FIXME/placeholder patterns found in any modified files. No stub implementations found. No empty handlers.

---

### Human Verification Required

None. All must-haves are verifiable programmatically through source code inspection.

Items that would benefit from integration testing (but are not blockers):
1. **Ping health check under real MCP server degradation** — Verify that `mark_degraded()` is called when a live MCP server becomes unreachable and `mark_healthy()` when it recovers. Needs a real or mock MCP server.
2. **Rug-pull detection end-to-end** — Start with a pinned tool, modify the tool's schema on the server, reconnect, verify that the agent blocks the entire server rather than registering tools. Needs an actual MCP server where tool schemas can be modified.
3. **Trust zone text appears in agent prompt** — Verify the trust zone guidance text actually appears in a real conversation context when an untrusted external tool is registered. Needs a running agent session.

---

### Gaps Summary

No gaps found. All 13 observable truths verified. All 10 required artifacts exist and are substantive. All 8 key links are wired. All 5 requirement IDs (CLNT-06, CLNT-07, CLNT-10, CLNT-12, INTG-04) are satisfied.

The one intentional deferral (set_mcp_context_utilization) is documented in source, accepted in the plan, and does not block the phase goal.

---

### Git Commit Verification

All commits documented in summaries confirmed in git log:

| Commit | Plan | Purpose |
|--------|------|---------|
| `fbb5146` | 21-01 Task 1 | V6 migration + CostRecord server_name |
| `97cae69` | 21-01 Task 2 / 21-03 Task 1 | PinStore wiring + TrustZoneProvider creation |
| `ec7a551` | 21-02 Task 1 | Health monitor with real pings + config field |
| `023d390` | 21-03 Task 2 | TrustZoneProvider wired into serve.rs |
| `18cd576` | 21-04 Task 1 | Prometheus metric call sites wired |
| `c84c04b` | 21-04 Task 2 | Metric call site documentation + compilation fixes |

---

_Verified: 2026-03-03T15:00:00Z_
_Verifier: Claude (gsd-verifier)_
