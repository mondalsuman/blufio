---
phase: 16-mcp-server-stdio
verified: 2026-03-03T12:00:00Z
status: passed
score: 5/5 criteria verified
human_verification:
  - test: "Connect Claude Desktop to `blufio mcp-server` via stdio, send initialize request, then tools/list"
    expected: "Client receives ServerInfo with tools capability; tools/list returns available non-bash tools with correct names and schemas"
    why_human: "Full MCP stdio session lifecycle requires a live MCP client (Claude Desktop or mcp-cli)"
  - test: "Invoke a Blufio skill from Claude Desktop via MCP tools/call"
    expected: "Tool executes and returns result content; isError field is false for successful tools"
    why_human: "End-to-end tool invocation through stdio transport requires a live MCP client"
---

# Phase 16: MCP Server stdio Verification Report

**Phase Goal:** Operator can point Claude Desktop at Blufio via stdio and invoke skills as MCP tools
**Verified:** 2026-03-03T12:00:00Z
**Status:** passed

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Claude Desktop connects via stdio, completes capability negotiation, and lists available tools | VERIFIED | `handler.rs` line 119: `get_info()` returns `ServerInfo` with `tools: Some(...)` capability; line 368: `list_tools()` acquires ToolRegistry read lock, calls `bridge::filtered_tool_names()`; `lib.rs` line 42: `serve_stdio()` wraps rmcp stdio transport; `mcp_server.rs`: orchestrates config->db->registry->serve_stdio; tests `get_info_returns_tools_capability`, `get_info_returns_blufio_server_info`, `list_tools_returns_filtered_tools` all pass |
| 2 | Claude Desktop can invoke a Blufio skill through MCP tools/call and receive the result | VERIFIED | `handler.rs` line 392: `call_tool()` 5-step pipeline (export check, lookup, input build, validate, timeout invoke); errors become `isError:true` results; tests `call_tool_with_valid_input_returns_content`, `call_tool_error_tool_returns_is_error_true`, `call_tool_fail_tool_returns_is_error_true`, `call_tool_timeout_returns_is_error_true` all pass |
| 3 | Invalid tool inputs return JSON-RPC -32602 error with a human-readable message | VERIFIED | `handler.rs` line 473: `validate_input()` uses `jsonschema::validator_for()` for schema validation; line 442: returns `rmcp::ErrorData::invalid_params()` (maps to JSON-RPC -32602) with human-readable message including tool name; tests `validate_input_accepts_valid_input`, `validate_input_rejects_missing_required`, `validate_input_rejects_wrong_type`, `call_tool_with_invalid_input_returns_validation_error`, `call_tool_with_wrong_type_returns_validation_error` all pass |
| 4 | Only tools on the explicit export allowlist are visible to MCP clients (bash is never exposed) | VERIFIED | `bridge.rs` line 24: `filtered_tool_names()` applies allowlist; bash permanently excluded (lines 8-9 doc comment); empty allowlist = all non-bash tools; `handler.rs` `is_tool_exported()` enforces same check for `call_tool`; tests: 7 filtering tests + `call_tool_with_non_exported_tool_returns_error`, `call_tool_with_bash_returns_error` all pass |
| 5 | All process output goes to stderr in stdio mode -- no stdout corruption of the JSON-RPC stream | VERIFIED | `mcp_server.rs` line 31-32: `init_tracing_stderr()` initializes tracing subscriber targeting stderr only via `RedactingMakeWriter` (line 109); `std::io::stderr()` at line 117; startup banner printed to stderr (line 65); `serve_stdio()` in `lib.rs` uses rmcp's stdio transport (stdin/stdout for JSON-RPC only); no `println!` or `print!` macros in mcp_server.rs |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/blufio-mcp-server/src/bridge.rs` | Bridge between ToolRegistry and MCP types | VERIFIED | `filtered_tool_names()` (line 24) and `to_mcp_tool()` (line 60); 11 bridge tests pass |
| `crates/blufio-mcp-server/src/handler.rs` | BlufioMcpHandler with ServerHandler trait | VERIFIED | `get_info()` (line 119), `list_tools()` (line 368), `call_tool()` (line 392), `validate_input()` (line 473); 92 total crate tests pass |
| `crates/blufio-mcp-server/src/lib.rs` | serve_stdio() function | VERIFIED | Line 42: wraps rmcp `ServiceExt::serve_with_ct` with stdio transport; keeps rmcp out of public API |
| `crates/blufio/src/mcp_server.rs` | MCP server CLI subcommand handler | VERIFIED | Initializes tracing to stderr, opens database, initializes tool registry, calls serve_stdio() |
| `crates/blufio/src/main.rs` | McpServer CLI variant | VERIFIED | Feature-gated behind `mcp-server` feature flag; CLI parsing test passes |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| McpConfig.export_tools | filtered_tool_names | bridge.rs allowlist logic | WIRED | handler passes export_tools to bridge; bridge filters accordingly |
| ToolRegistry | MCP tool list | bridge::to_mcp_tool | WIRED | list_tools acquires read lock, iterates filtered names, converts each to rmcp Tool |
| call_tool input | jsonschema validation | validate_input() | WIRED | Input validated against tool's parameters_schema before invocation |
| mcp_server.rs | serve_stdio | lib.rs serve_stdio() | WIRED | mcp_server.rs creates BlufioMcpHandler and delegates to serve_stdio with CancellationToken |
| Tracing subscriber | stderr | RedactingMakeWriter | WIRED | init_tracing_stderr() configures subscriber with std::io::stderr() target |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| SRVR-01 | Plan 02, 03 | Connect Claude Desktop via stdio and list tools | SATISFIED | serve_stdio() wraps rmcp stdio transport; get_info returns tools capability; list_tools returns filtered tool list; Human verification needed for end-to-end Claude Desktop connectivity |
| SRVR-02 | Plan 02, 03 | Invoke skills via MCP tools/call | SATISFIED | call_tool() 5-step pipeline with export check, lookup, validation, timeout invoke; 10+ call_tool tests pass; Human verification needed for end-to-end Claude Desktop invocation |
| SRVR-03 | Plan 03 | blufio mcp-server CLI subcommand | SATISFIED | Commands::McpServer variant in main.rs; mcp_server.rs handler; CLI parsing test passes; feature-gated behind mcp-server |
| SRVR-04 | Plan 02, 03 | Capability negotiation with MCP spec 2025-11-25 | SATISFIED | get_info() returns ServerInfo with tools capability, server name "blufio", CARGO_PKG_VERSION; rmcp handles initialize/initialized handshake |
| SRVR-05 | Plan 02 | Tool input validation with JSON-RPC -32602 | SATISFIED | validate_input() uses jsonschema crate; ErrorData::invalid_params maps to -32602; human-readable messages include tool name; 5 validation tests pass |
| SRVR-12 | Plan 01 | Explicit MCP tool export allowlist, bash excluded | SATISFIED | filtered_tool_names() applies export_tools allowlist; bash permanently excluded at bridge level; 7 filtering tests + 2 bash exclusion handler tests pass |
| SRVR-15 | Plan 03 | All logging to stderr in stdio mode | SATISFIED | init_tracing_stderr() configures RedactingMakeWriter targeting std::io::stderr(); no println!/print! in mcp_server.rs; startup banner to stderr |

**No orphaned requirements.** All 7 SRVR requirement IDs from plan frontmatter map to REQUIREMENTS.md entries. All 7 are verified.

### Human Verification Required

#### 1. MCP stdio End-to-End Connectivity

**Test:** Configure Claude Desktop with `blufio mcp-server` as stdio server. Start session and issue `tools/list`.

**Expected:** Client receives ServerInfo with tools capability during initialization. `tools/list` returns non-bash tools with correct names, descriptions, and inputSchemas.

**Why human:** Full MCP stdio session lifecycle (spawn process -> initialize -> list -> call) requires a live MCP client.

#### 2. MCP Tool Invocation via Claude Desktop

**Test:** From Claude Desktop connected via stdio, invoke a Blufio skill using `tools/call` with valid input.

**Expected:** Tool executes and returns result content. `isError` is false for successful tools.

**Why human:** End-to-end tool invocation through stdio transport requires a live MCP client process.

### Gaps Summary

No gaps found. All 5 criteria pass. All 7 requirements verified. 2 items flagged for human verification (Claude Desktop connectivity and tool invocation).

---

_Verified: 2026-03-03T12:00:00Z_
_Verifier: Claude (gsd-verifier)_
