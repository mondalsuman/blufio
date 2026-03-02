---
phase: 16-mcp-server-stdio
plan: 02
status: complete
commit: 212ae00
files_modified:
  - crates/blufio-mcp-server/src/handler.rs
  - crates/blufio-mcp-server/src/lib.rs
  - crates/blufio-mcp-server/Cargo.toml
  - Cargo.toml
requirements_covered: [SRVR-01, SRVR-02, SRVR-04, SRVR-05]
---

## Summary

Implemented `BlufioMcpHandler` struct with rmcp `ServerHandler` trait.

### What was built

1. **`handler.rs`** -- Full `ServerHandler` implementation:
   - `get_info()`: Returns `ServerInfo` with tools-only capabilities (`resources=None`, `prompts=None`), server name "blufio", and `CARGO_PKG_VERSION`.
   - `list_tools()`: Acquires read lock on `ToolRegistry`, uses `bridge::filtered_tool_names` for allowlist, converts each to MCP tool via `bridge::to_mcp_tool`.
   - `call_tool()`: Five-step pipeline:
     1. Check export allowlist (bash always excluded)
     2. Look up tool in registry
     3. Build input JSON from arguments map
     4. Validate input against tool's JSON Schema via `jsonschema` crate
     5. Invoke with `tokio::time::timeout` wrapper
   - `validate_input()`: Helper function using `jsonschema::validator_for` with human-readable error messages.

2. **Dependencies added**:
   - `jsonschema = "0.28"` to workspace and blufio-mcp-server
   - `tokio` features extended with `time` for timeout support

### Test coverage

- 20 new handler tests covering:
  - `get_info` returns tools capability, correct server info
  - `is_tool_exported` with empty/explicit allowlists, bash exclusion
  - `list_tools` with filtering
  - `call_tool` with: non-exported tool, bash, nonexistent tool, valid input, missing required fields, wrong types, error tool, fail tool, timeout
  - `validate_input` with valid/invalid/empty schemas
- Tests use `call_tool_direct` helper that tests business logic without needing full `RequestContext`
- All 33 crate tests pass, clippy clean

### Key decisions

- Used `async fn` for `list_tools` and `call_tool` (clippy prefers over `-> impl Future` when the trait supports it)
- Input validation uses `jsonschema` 0.28 (not latest 0.44 to match plan spec)
- Error messages include tool name for debuggability
- Tool execution errors (`Err(BlufioError)`) become `isError:true` results, not protocol errors
