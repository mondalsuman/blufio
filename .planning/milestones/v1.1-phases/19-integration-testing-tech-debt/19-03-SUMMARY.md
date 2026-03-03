---
phase: 19-integration-testing-tech-debt
plan: 03
status: completed
requirements_completed: [INTG-01, INTG-03]
commit: 7fcdf2c
---

## Summary

Added MCP server E2E tests and cross-contamination tests verifying protocol isolation between REST and MCP endpoints.

### What Changed

**Task 1: MCP Server E2E Tests (INTG-01)**
- Created `e2e_mcp_server.rs` with 10 tests covering:
  - Server capabilities (tools, prompts, resources)
  - Tool listing via registry and bridge filtering
  - Tool invocation error handling (file tool with nonexistent path)
  - Bridge conversion to MCP format (name, description, input_schema)
  - Export allowlist filtering (bash always excluded)
  - Resource capability with/without storage
- Uses `BlufioMcpHandler` in-process with `MockStorage` for resource tests
- Implements `StorageAdapter` and `PluginAdapter` traits on MockStorage

**Task 2: Cross-Contamination Tests (INTG-03)**
- Created `e2e_cross_contamination.rs` with 6 tests covering:
  - JSON-RPC body to REST /v1/messages returns 422
  - Empty JSON body to REST endpoint returns 422
  - Sessions endpoint returns valid JSON list
  - Health endpoint returns { status: "healthy" }
  - Invalid content-type returns 415
  - GET on POST-only endpoint returns 405
- Uses axum test utilities (tower::ServiceExt::oneshot) without port binding

### Files Modified
- `crates/blufio/tests/e2e_mcp_server.rs` - 10 MCP server E2E tests (new)
- `crates/blufio/tests/e2e_cross_contamination.rs` - 6 cross-contamination tests (new)
- `crates/blufio/Cargo.toml` - Added dev-dependencies for tower, http, axum, dashmap, rmcp, async-trait, semver, blufio-gateway, blufio-mcp-server, blufio-skill

### Verification
- All 16 new tests pass
- Full test suite passes (81+ unit tests, 12 existing E2E tests)
