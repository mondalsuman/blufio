# Phase 15: MCP Foundation - Context

**Gathered:** 2026-03-02
**Status:** Ready for planning

<domain>
## Phase Boundary

Config structs, workspace crates, dependency integration, namespace enforcement, and abstraction boundary for MCP. Both `blufio-mcp-server` and `blufio-mcp-client` crates compile, and the ToolRegistry enforces namespaced tool names with collision detection. No transport implementation, no protocol handling — that's Phase 16+.

</domain>

<decisions>
## Implementation Decisions

### Config design
- Flat `[[mcp.servers]]` array in TOML — each entry has name, transport, url/command, and optional auth
- Both HTTP and stdio transport types parsed from the start (even though stdio server is Phase 16)
- `mcp.enabled` toggle (default false) — consistent with `gateway.enabled`, `skill.enabled`, `heartbeat.enabled` pattern
- Tool export allowlist: code-defined safe defaults (bash never exposed) + optional `mcp.export_tools` config override

### Namespace convention
- Double underscore separator: `server__tool` (e.g., `github__create_issue`)
- Built-in tools stay flat — no prefix for `bash`, `http`, `file`, etc.
- Built-in tools always win on collision — log a warning and skip the external tool
- Tool name regex validation: `[a-zA-Z][a-zA-Z0-9_]*` enforced at registration time, plus collision detection

### Abstraction boundary
- Thin newtypes at crate boundaries — use rmcp freely inside MCP crates, expose Blufio-owned types in pub APIs
- Separate `McpSessionId(String)` newtype in blufio-mcp-server — compiler prevents SessionId/McpSessionId conflation
- Reuse `serde_json::Value` for tool schemas — consistent with existing `Tool::parameters_schema()` return type
- rmcp as direct dependency in both MCP crates — no shared wrapper crate

### Crate organization
- Feature-gated: `mcp-server` and `mcp-client` features in main blufio binary (default on)
- ToolRegistry evolves in blufio-skill — add namespace support to existing registry rather than extracting
- rmcp 0.17.0 and schemars 1.0 added as workspace-level dependencies in root Cargo.toml
- Extend `BlufioError` with `Mcp` variant in blufio-core — consistent with Channel, Provider, Vault pattern

### Claude's Discretion
- Internal module structure within blufio-mcp-server and blufio-mcp-client
- Exact fields on McpServerConfig struct beyond name/transport/url/command/auth
- Specific regex pattern details (whether to allow hyphens, max length, etc.)
- reqwest version unification strategy if rmcp brings a different version

</decisions>

<specifics>
## Specific Ideas

- Config pattern mirrors Claude Desktop's MCP server configuration — familiar to operators coming from that ecosystem
- Double underscore convention matches Claude Desktop's namespace format
- The `deny_unknown_fields` pattern must be preserved on all new config structs — this is a core safety invariant of the config system

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ToolRegistry` (blufio-skill/src/tool.rs): HashMap-based, register/get/list/tool_definitions — needs namespace-aware lookup and collision detection added
- `Tool` trait (blufio-skill/src/tool.rs): name(), description(), parameters_schema() -> serde_json::Value, invoke() — MCP tool definitions can derive from this
- `BlufioConfig` (blufio-config/src/model.rs): 15 sections, all with deny_unknown_fields, Figment loading — new `[mcp]` section follows same pattern
- `SessionId(String)` (blufio-core/src/types.rs): existing newtype — McpSessionId follows same pattern
- `BlufioError` (blufio-core/src/error.rs): thiserror enum with 11 variants — add Mcp variant

### Established Patterns
- Config: `#[serde(deny_unknown_fields)]` on all structs, `#[serde(default)]` with default functions, Figment for TOML+env loading
- Errors: thiserror with miette diagnostics, all variants in single BlufioError enum
- Workspace: edition 2024, shared deps in workspace Cargo.toml, each crate in crates/ directory
- Testing: blufio-test-utils with mock harness, serial_test for integration tests

### Integration Points
- `BlufioConfig` needs new `mcp: McpConfig` field with `#[serde(default)]`
- `ToolRegistry::register()` needs namespace validation and collision detection before insertion
- Root `Cargo.toml` workspace members list needs `blufio-mcp-server` and `blufio-mcp-client`
- Main binary `blufio/Cargo.toml` needs feature flags for MCP crate dependencies

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 15-mcp-foundation*
*Context gathered: 2026-03-02*
