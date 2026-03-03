---
phase: 19-integration-testing-tech-debt
plan: 01
status: completed
requirements_completed: [INTG-04, INTG-05]
commit: c1e47c5
---

## Summary

Added MCP-specific Prometheus metrics and connection limits to the gateway.

### What Changed

**Task 1: MCP Prometheus Metrics (INTG-04)**
- Added 4 metrics to `recording.rs`: `blufio_mcp_connections_total` (counter), `blufio_mcp_active_connections` (gauge), `blufio_mcp_tool_response_size_bytes` (histogram), `blufio_mcp_context_utilization_ratio` (gauge)
- Added 4 helper functions: `record_mcp_connection()`, `set_mcp_active_connections()`, `record_mcp_tool_response_size()`, `set_mcp_context_utilization()`
- Re-exported all helpers from `blufio-prometheus/src/lib.rs`

**Task 2: Connection Limits (INTG-05)**
- Added `max_connections: usize` field to `McpConfig` in `model.rs` with default 10
- Added `tower` workspace dependency with `limit` feature
- Added `mcp_max_connections` field to `GatewayChannelConfig`
- Wrapped MCP router with `tower::limit::ConcurrencyLimitLayer` in `server.rs`
- Over-limit connections receive HTTP 503 Service Unavailable

### Files Modified
- `crates/blufio-prometheus/src/recording.rs` - 4 MCP metric registrations + 4 helpers
- `crates/blufio-prometheus/src/lib.rs` - Re-exports for new helpers
- `crates/blufio-config/src/model.rs` - `max_connections` field + default
- `crates/blufio-gateway/src/server.rs` - ConcurrencyLimitLayer on /mcp route
- `crates/blufio-gateway/src/lib.rs` - `mcp_max_connections` in config + connect()
- `crates/blufio-gateway/Cargo.toml` - tower dependency
- `crates/blufio/src/serve.rs` - Wire max_connections into GatewayChannelConfig
- `Cargo.toml` - tower workspace dependency

### Verification
- `cargo build -p blufio` passes
- `cargo test -p blufio-gateway -p blufio-prometheus -p blufio-config` passes (75 tests)
