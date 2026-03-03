---
phase: 19-integration-testing-tech-debt
plan: 04
status: completed
requirements: [INTG-02]
commit: 53d93a6
---

## Summary

Added MCP client E2E tests verifying external tool discovery, registration, and graceful failure handling.

### What Changed

**Task 1: MCP Client E2E Tests (INTG-02)**
- Created `e2e_mcp_client.rs` with 8 tests covering:
  - Unreachable server handled gracefully (CLNT-14): connect to port 1, verify 0 connected/1 failed/0 tools
  - Multiple unreachable servers fail independently: 2 servers, both tracked as failed
  - Empty server list succeeds with empty results
  - Invalid transport type ("grpc") fails gracefully
  - Server state tracking: disconnected servers tracked by name
  - Namespace convention: double-underscore separator (server__tool)
  - Description sanitization: long descriptions capped at 1024 chars
  - Trust guidance: EXTERNAL_TOOL_TRUST_GUIDANCE contains expected keywords
- Also fixed `McpConfig` test constructor in `blufio-mcp-server/handler.rs` (missing `max_connections` field from Plan 01)

### Files Modified
- `crates/blufio/tests/e2e_mcp_client.rs` - 8 MCP client E2E tests (new)
- `crates/blufio/Cargo.toml` - Added blufio-mcp-client dev-dependency
- `crates/blufio-mcp-server/src/handler.rs` - Fixed test McpConfig with max_connections field

### Verification
- All 8 new tests pass
- Full test suite passes (all 46 test suites, 0 failures)
