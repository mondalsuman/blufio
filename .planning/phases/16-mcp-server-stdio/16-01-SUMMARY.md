---
phase: 16-mcp-server-stdio
plan: 01
status: complete
commit: bef9f63
files_modified:
  - crates/blufio-mcp-server/src/bridge.rs
  - crates/blufio-mcp-server/src/lib.rs
  - crates/blufio-config/src/model.rs
requirements_covered: [SRVR-12]
---

## Summary

Implemented the bridge layer between Blufio's `ToolRegistry` and MCP tool types.

### What was built

1. **`bridge.rs`** — Two public functions:
   - `filtered_tool_names()`: Applies export allowlist from `McpConfig::export_tools`. Empty allowlist = all non-bash tools. Non-empty = only listed tools minus bash. Bash is permanently excluded with a warning if listed.
   - `to_mcp_tool()`: Converts a Blufio `Tool` trait object to `rmcp::model::Tool` by mapping name, description, and parameters_schema to the MCP input_schema format.

2. **`McpConfig.tool_timeout_secs`** — Added 60s default timeout for MCP tool invocations. Manual `Default` impl replaces `#[derive(Default)]` to support the serde default function.

### Test coverage

- 7 filtering tests (empty allowlist, explicit allowlist, bash exclusion, nonexistent tools, empty registry)
- 4 conversion tests (name/description mapping, schema preservation, empty schema, namespaced name override)
- All 13 blufio-mcp-server tests pass
- All 21 blufio-config tests pass
- Clippy clean on both crates

### Key decisions

- `to_mcp_tool()` takes a `name` parameter separate from `tool.name()` to support namespace-prefixed tool names
- Non-object schemas fall back to `{"type": "object"}` with a warning log
- Bash exclusion is enforced at the bridge level, not just config validation
