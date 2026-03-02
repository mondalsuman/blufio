---
phase: 16-mcp-server-stdio
plan: 03
status: complete
commit: 5ba0b8b
files_modified:
  - crates/blufio/src/main.rs
  - crates/blufio/src/mcp_server.rs
  - crates/blufio-mcp-server/src/lib.rs
  - crates/blufio-mcp-server/Cargo.toml
requirements_covered: [SRVR-01, SRVR-02, SRVR-03, SRVR-04, SRVR-15]
---

## Summary

Implemented the `blufio mcp-server` CLI subcommand with stdio transport.

### What was built

1. **`mcp_server.rs`** -- Subcommand handler:
   - Initializes tracing to stderr only (SRVR-15) via `RedactingMakeWriter`
   - Logs bash exclusion warning if present in export_tools config
   - Opens database, runs vault startup check
   - Initializes tool registry with built-in tools
   - Prints startup banner to stderr: `blufio 0.1.0 MCP server ready`
   - Creates `BlufioMcpHandler` and delegates to `serve_stdio()`
   - Clean database close on shutdown

2. **`McpServer` CLI variant** -- Added to `Commands` enum in main.rs:
   - Feature-gated behind `mcp-server` feature flag
   - Graceful error message when compiled without the feature

3. **`serve_stdio()` function** -- Added to blufio-mcp-server lib.rs:
   - Wraps rmcp `ServiceExt::serve_with_ct` with stdio transport
   - Keeps rmcp types out of the public API (abstraction boundary)
   - Passes CancellationToken for signal-triggered shutdown
   - Blocks until client disconnects (stdin EOF) or signal received

### Test coverage

- CLI parsing test: `cli_parses_mcp_server` verifies `blufio mcp-server` parses correctly
- Existing 33 blufio-mcp-server tests still pass
- Clippy clean with `-D warnings` on both crates

### Key decisions

- Duplicated `RedactingMakeWriter` in mcp_server.rs rather than making serve.rs's version pub(crate), to keep the modules independent
- `serve_stdio()` lives in blufio-mcp-server crate to maintain the "no rmcp in public API" boundary
- Added `tokio-util` dependency to blufio-mcp-server for `CancellationToken` type in `serve_stdio` signature
