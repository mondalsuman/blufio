---
phase: 18
name: MCP Client
status: complete
plans_completed: 4
total_plans: 4
requirements_completed:
  - CLNT-01
  - CLNT-02
  - CLNT-03
  - CLNT-04
  - CLNT-05
  - CLNT-06
  - CLNT-07
  - CLNT-08
  - CLNT-09
  - CLNT-10
  - CLNT-11
  - CLNT-12
  - CLNT-13
  - CLNT-14
date_completed: "2026-03-03"
---

# Phase 18: MCP Client - Execution Summary

## Objective

Enable the Blufio agent to discover and invoke external MCP tools configured by the operator, with security hardening against tool poisoning, rug pulls, and context window blowups.

## Plans Executed

### Plan 18-01: Config Extensions + Security Primitives (Wave 1)
- Extended `McpServerEntry` with `connect_timeout_secs` (default 10) and `response_size_cap` (default 4096)
- Added config validation: stdio rejection, command rejection, unknown transport rejection, URL requirement
- Implemented SHA-256 hash pinning (`pin.rs`) with canonical JSON (sorted keys via BTreeMap)
- Implemented description sanitization (`sanitize.rs`) stripping instruction patterns, 200-char cap, server prefix

### Plan 18-02: Client Manager + ExternalTool + serve.rs (Wave 2)
- Created `McpClientManager` with concurrent server connections via JoinSet
- Implemented `ExternalTool` wrapping rmcp `RunningService` into Blufio `Tool` trait
- Tool discovery via `list_all_tools()` with pin computation and description sanitization
- Wired MCP client initialization into `serve.rs` startup sequence
- SSE transport mapped to Streamable HTTP (rmcp 0.17 handles SSE fallback automatically)

### Plan 18-03: PinStore + Health Monitor + ToolRegistry.unregister (Wave 3)
- Created SQLite-backed `PinStore` for persistent tool hash pin storage
- Implemented `verify_or_store` workflow: FirstSeen/Verified/Mismatch detection
- Created health monitor background task with exponential backoff (1s-60s cap)
- Added `HealthTracker` for per-server state management
- Added `ToolRegistry.unregister()` for removing degraded server tools

### Plan 18-04: Doctor Health Checks (Wave 3)
- Added `check_mcp_servers` diagnostic to `blufio doctor`
- Each configured server gets independent connect + tools/list check
- Added `diagnose_server` helper in mcp-client crate (avoids rmcp dep leaking)
- Feature-gated behind `mcp-client` flag

## Files Modified/Created

### New files
- `crates/blufio-mcp-client/src/external_tool.rs` - ExternalTool implementing BlufioTool trait
- `crates/blufio-mcp-client/src/manager.rs` - McpClientManager, connect logic, diagnose_server
- `crates/blufio-mcp-client/src/pin.rs` - SHA-256 hash pinning
- `crates/blufio-mcp-client/src/sanitize.rs` - Description sanitization and response truncation
- `crates/blufio-mcp-client/src/pin_store.rs` - SQLite-backed pin storage
- `crates/blufio-mcp-client/src/health.rs` - Health monitoring background task

### Modified files
- `crates/blufio-mcp-client/Cargo.toml` - Added rmcp features, tokio-rusqlite, rusqlite, tokio-util deps
- `crates/blufio-mcp-client/src/lib.rs` - Module exports and public re-exports
- `crates/blufio-config/src/model.rs` - McpServerEntry extensions
- `crates/blufio-config/src/validation.rs` - CLNT-11 validation rules
- `crates/blufio-skill/src/tool.rs` - ToolRegistry.unregister() method
- `crates/blufio/src/serve.rs` - MCP client initialization wiring
- `crates/blufio/src/doctor.rs` - MCP server health checks

## Test Coverage

- 58 tests in blufio-mcp-client (pin, sanitize, external_tool, manager, pin_store, health)
- 4 tests for ToolRegistry.unregister in blufio-skill
- 2 tests for MCP doctor checks in blufio
- All tests passing, clippy clean, cargo fmt clean

## Success Criteria Verification

1. **Operator configures external MCP server in TOML, agent discovers tools with namespace-prefixed names** -- McpClientManager.connect_all() discovers tools and registers with `server__tool` naming
2. **Agent invokes external MCP tool during conversation, result in response** -- ExternalTool.invoke() calls tool and returns text result
3. **Config entries with command (stdio) rejected with clear error** -- validation.rs rejects stdio transport and command fields
4. **Tool definitions SHA-256 hash-pinned; mutations disable tool and alert** -- pin.rs + pin_store.rs with verify_or_store workflow
5. **External tool descriptions sanitized and labeled as separate trust zone** -- sanitize.rs strips instructions, caps at 200 chars, EXTERNAL_TOOL_TRUST_GUIDANCE in prompt

## Key Design Decisions

- rmcp 0.17 has no separate SSE client transport; Streamable HTTP handles SSE fallback automatically
- `RunningService<RoleClient, ()>` shared via `Arc` between ExternalTool instances and manager
- `Arc::try_unwrap` pattern for clean session shutdown (cancel() consumes self)
- diagnose_server() in mcp-client crate avoids leaking rmcp dependency to blufio crate
- Canonical JSON with sorted keys (BTreeMap) ensures deterministic SHA-256 hashing
