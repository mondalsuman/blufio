# Phase 15: MCP Foundation - Research

**Researched:** 2026-03-02
**Domain:** MCP SDK integration, Rust workspace scaffolding, tool namespace enforcement
**Confidence:** HIGH

## Summary

Phase 15 establishes the foundation for MCP integration by scaffolding two new workspace crates (`blufio-mcp-server` and `blufio-mcp-client`), adding rmcp 0.17.0 as the MCP SDK, extending the config system with an `[mcp]` section, and adding namespace-aware collision detection to the existing `ToolRegistry`. The rmcp crate is the official Rust SDK for the Model Context Protocol, maintained under the `modelcontextprotocol` GitHub org.

The primary technical risk is the **reqwest version bump**: rmcp 0.17.0 depends on reqwest 0.13.2, while Blufio currently pins reqwest 0.12. These are semver-incompatible, so the workspace must upgrade to reqwest 0.13. This is a cascading change that touches every crate using reqwest (blufio-skill, blufio-anthropic, main binary). The rmcp crate also uses schemars 1.0 for JSON Schema generation, which is a new dependency for the workspace.

**Primary recommendation:** Upgrade reqwest to 0.13 workspace-wide first, then add rmcp and schemars as workspace dependencies, then scaffold crates and config, then evolve ToolRegistry.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Flat `[[mcp.servers]]` array in TOML -- each entry has name, transport, url/command, and optional auth
- Both HTTP and stdio transport types parsed from the start (even though stdio server is Phase 16)
- `mcp.enabled` toggle (default false) -- consistent with `gateway.enabled`, `skill.enabled`, `heartbeat.enabled` pattern
- Tool export allowlist: code-defined safe defaults (bash never exposed) + optional `mcp.export_tools` config override
- Double underscore separator: `server__tool` (e.g., `github__create_issue`)
- Built-in tools stay flat -- no prefix for `bash`, `http`, `file`, etc.
- Built-in tools always win on collision -- log a warning and skip the external tool
- Tool name regex validation: `[a-zA-Z][a-zA-Z0-9_]*` enforced at registration time, plus collision detection
- Thin newtypes at crate boundaries -- use rmcp freely inside MCP crates, expose Blufio-owned types in pub APIs
- Separate `McpSessionId(String)` newtype in blufio-mcp-server -- compiler prevents SessionId/McpSessionId conflation
- Reuse `serde_json::Value` for tool schemas -- consistent with existing `Tool::parameters_schema()` return type
- rmcp as direct dependency in both MCP crates -- no shared wrapper crate
- Feature-gated: `mcp-server` and `mcp-client` features in main blufio binary (default on)
- ToolRegistry evolves in blufio-skill -- add namespace support to existing registry rather than extracting
- rmcp 0.17.0 and schemars 1.0 added as workspace-level dependencies in root Cargo.toml
- Extend `BlufioError` with `Mcp` variant in blufio-core -- consistent with Channel, Provider, Vault pattern

### Claude's Discretion
- Internal module structure within blufio-mcp-server and blufio-mcp-client
- Exact fields on McpServerConfig struct beyond name/transport/url/command/auth
- Specific regex pattern details (whether to allow hyphens, max length, etc.)
- reqwest version unification strategy if rmcp brings a different version

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| FOUND-01 | MCP config structs added to blufio-config with TOML parsing and deny_unknown_fields | Config pattern analysis of existing BlufioConfig (15 sections, all with deny_unknown_fields). New `McpConfig` follows same pattern with `#[serde(default)]`. |
| FOUND-02 | Workspace crates blufio-mcp-server and blufio-mcp-client scaffolded with feature flags | Workspace uses `members = ["crates/*"]` glob. New crates go in `crates/blufio-mcp-server` and `crates/blufio-mcp-client`. Feature flags follow existing `telegram`, `gateway` pattern in main binary. |
| FOUND-03 | rmcp 0.17.0 and schemars 1.0 added to workspace dependencies (verify reqwest version unification) | rmcp 0.17.0 requires reqwest 0.13.2. Current workspace pins reqwest 0.12. Must upgrade reqwest workspace-wide to 0.13. schemars 1.0 is new dependency. |
| FOUND-04 | Tool namespace convention enforced in ToolRegistry with collision detection and built-in priority | Existing ToolRegistry is a simple HashMap<String, Arc<dyn Tool>>. Needs: namespace validation regex, `register_namespaced()` method, collision detection in `register()`, built-in priority flag. |
| FOUND-05 | MCP session ID type distinct from Blufio conversation session ID | Existing `SessionId(String)` in blufio-core/src/types.rs. New `McpSessionId(String)` in blufio-mcp-server with same newtype pattern but distinct type. |
| FOUND-06 | rmcp abstraction boundary established (Blufio-owned types, no public rmcp re-exports) | rmcp types used freely inside MCP crates. Public API exposes Blufio-owned types only. Verified: rmcp `Content`, `CallToolResult`, `ServerInfo` etc. are internal only. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rmcp | 0.17.0 | Official Rust MCP SDK | Official SDK under modelcontextprotocol org; provides ServerHandler trait, tool macros, transport implementations |
| schemars | 1.0 | JSON Schema generation from Rust types | Required by rmcp for `#[tool]` macro structured output; derives JsonSchema for tool parameter types |
| reqwest | 0.13.2 | HTTP client | Required by rmcp; upgrade from 0.12 needed workspace-wide |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| serde | 1.0 | Serialization (already in workspace) | Config structs, MCP types |
| serde_json | 1.0 | JSON (already in workspace) | Tool schemas, MCP payloads |
| tokio | 1.x | Async runtime (already in workspace) | MCP transport, async tool invocation |
| tracing | 0.1 | Logging (already in workspace) | MCP connection events, collision warnings |
| regex | 1.x | Regex (already in workspace) | Tool name validation pattern |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| rmcp | rust-mcp-sdk | rmcp is official SDK; rust-mcp-sdk is third-party. No reason to use alternative. |
| schemars 1.0 | Manual JSON Schema | rmcp requires schemars for `#[tool]` macro; manual schemas wouldn't integrate |

**Installation:**
```toml
# workspace Cargo.toml [workspace.dependencies]
rmcp = { version = "0.17", default-features = false }
schemars = "1"
reqwest = { version = "0.13", features = ["json", "rustls-tls", "stream"], default-features = false }
```

## Architecture Patterns

### Recommended Crate Structure
```
crates/
  blufio-mcp-server/
    Cargo.toml
    src/
      lib.rs          # pub API: McpSessionId, re-exports
      config.rs       # McpConfig, McpServerConfig (deny_unknown_fields)
      types.rs        # McpSessionId newtype, Blufio-owned MCP types
  blufio-mcp-client/
    Cargo.toml
    src/
      lib.rs          # pub API: client types
      config.rs       # McpClientConfig if needed (or reuse from server)
```

### Pattern 1: Config Section with deny_unknown_fields
**What:** Every config struct uses `#[serde(deny_unknown_fields)]` and `#[serde(default)]` on Optional sections
**When to use:** All new TOML config sections
**Example:**
```rust
// Source: existing BlufioConfig pattern in blufio-config/src/model.rs
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct McpConfig {
    /// Enable MCP functionality.
    #[serde(default)]
    pub enabled: bool,

    /// MCP server configuration entries.
    #[serde(default)]
    pub servers: Vec<McpServerEntry>,

    /// Tools to export via MCP server (empty = all safe defaults).
    #[serde(default)]
    pub export_tools: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct McpServerEntry {
    /// Unique name for this MCP server.
    pub name: String,

    /// Transport type: "http" or "stdio".
    pub transport: String,

    /// URL for HTTP transport.
    #[serde(default)]
    pub url: Option<String>,

    /// Command for stdio transport.
    #[serde(default)]
    pub command: Option<String>,

    /// Command arguments for stdio transport.
    #[serde(default)]
    pub args: Vec<String>,

    /// Optional bearer token for authentication.
    #[serde(default)]
    pub auth_token: Option<String>,
}
```

### Pattern 2: Newtype for Session ID Separation
**What:** Distinct newtype wrappers prevent accidental conflation of session ID types
**When to use:** When two domains use string IDs that must not be confused
**Example:**
```rust
// In blufio-core/src/types.rs (existing)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

// In blufio-mcp-server/src/types.rs (new)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct McpSessionId(pub String);

impl McpSessionId {
    pub fn new(id: String) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for McpSessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
```

### Pattern 3: Feature-Gated Crate Dependencies
**What:** Optional crate dependencies controlled by Cargo features in the main binary
**When to use:** MCP crates as optional deps in `blufio` binary crate
**Example:**
```toml
# crates/blufio/Cargo.toml
[features]
default = ["telegram", "anthropic", "sqlite", "onnx", "prometheus", "keypair", "gateway", "mcp-server", "mcp-client"]
mcp-server = ["dep:blufio-mcp-server"]
mcp-client = ["dep:blufio-mcp-client"]

[dependencies]
blufio-mcp-server = { path = "../blufio-mcp-server", optional = true }
blufio-mcp-client = { path = "../blufio-mcp-client", optional = true }
```

### Pattern 4: rmcp ServerHandler (for reference -- implemented in Phase 16)
**What:** The trait that rmcp requires for MCP server implementation
**When to use:** Phase 16 will implement this; Phase 15 only scaffolds the crate
**Example:**
```rust
// Source: rmcp docs.rs
use rmcp::{ServerHandler, model::ServerInfo, tool, tool_router};

#[derive(Clone)]
pub struct BlufioMcpServer {
    // tool_router: ToolRouter<Self>,
}

impl ServerHandler for BlufioMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("Blufio MCP Server".into()),
            ..Default::default()
        }
    }
}
```

### Anti-Patterns to Avoid
- **Exposing rmcp types in public APIs:** Never `pub use rmcp::model::Content` from MCP crate boundaries. Wrap in Blufio-owned types.
- **Storing tools with raw names:** Always validate tool names against the regex pattern before insertion into ToolRegistry.
- **Silently overwriting on collision:** Always log a warning when a namespace collision occurs. Built-in wins, but the operator must know.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| MCP protocol messages | Custom JSON-RPC structs | rmcp model types | MCP spec is complex; rmcp implements initialize/initialized handshake, error codes, content types |
| JSON Schema from Rust types | Manual schema construction | schemars derive | Error-prone, rmcp `#[tool]` macro expects schemars-derived schemas |
| Tool name regex | Hand-written char matching | `regex` crate with compiled pattern | Edge cases in Unicode, anchoring; `regex` handles correctly |
| TOML config parsing | Custom parser | figment + serde (existing pattern) | Figment already handles TOML+env layering with deny_unknown_fields |

**Key insight:** rmcp handles the MCP protocol complexity. Phase 15 only scaffolds crates and types -- the actual protocol handling is Phase 16+.

## Common Pitfalls

### Pitfall 1: reqwest Version Mismatch
**What goes wrong:** rmcp 0.17.0 depends on reqwest 0.13.2. Blufio workspace pins reqwest 0.12. Cargo cannot unify these.
**Why it happens:** reqwest 0.12 and 0.13 are semver-incompatible.
**How to avoid:** Upgrade workspace reqwest to 0.13 first, before adding rmcp. Check all crates that depend on reqwest: `blufio-skill`, `blufio-anthropic`, main binary.
**Warning signs:** `cargo build` fails with "two different versions of crate reqwest" or features not matching.

### Pitfall 2: schemars Version Alignment
**What goes wrong:** rmcp uses schemars 1.0. If any other dependency brings schemars 0.8 (the old major), types won't be compatible.
**Why it happens:** schemars 1.0 was a major rewrite. The `JsonSchema` derive macro from 0.8 and 1.0 produce different trait impls.
**How to avoid:** Pin schemars 1.0 at workspace level. Verify no existing deps bring schemars 0.8 with `cargo tree -i schemars`.
**Warning signs:** "conflicting implementations of trait `JsonSchema`" errors.

### Pitfall 3: deny_unknown_fields Breaking Existing Configs
**What goes wrong:** Adding `mcp: McpConfig` to `BlufioConfig` without `#[serde(default)]` makes the field required, breaking all existing config files.
**Why it happens:** Forgetting `#[serde(default)]` on the new field.
**How to avoid:** Always add `#[serde(default)]` to new Optional config sections. The McpConfig struct itself implements `Default`. Existing TOML files without `[mcp]` section will deserialize with defaults.
**Warning signs:** Existing config test failures.

### Pitfall 4: rand Version Duplication
**What goes wrong:** rmcp optionally depends on rand 0.10. Blufio workspace pins rand 0.8. Two versions compile in parallel.
**Why it happens:** rand 0.8 and 0.10 are semver-incompatible.
**How to avoid:** This is acceptable -- they compile as separate crates. Types don't need to cross the boundary. Do NOT upgrade workspace rand unless needed independently.
**Warning signs:** Increased compile time and binary size (minor).

### Pitfall 5: ToolRegistry Namespace Collision Silently Overwrites
**What goes wrong:** Current `register()` uses `HashMap::insert` which silently overwrites duplicate keys.
**Why it happens:** Original design assumed all tools have unique names (true for built-in only).
**How to avoid:** Add collision detection before insert. Return `Result` or log warning. Built-in tools get an `is_builtin` flag.
**Warning signs:** External MCP tool shadows a built-in tool without any log output.

## Code Examples

### Tool Name Validation with Regex
```rust
// Source: project convention from CONTEXT.md
use regex::Regex;
use std::sync::LazyLock;

/// Valid tool names: starts with letter, contains letters/digits/underscores.
static TOOL_NAME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z][a-zA-Z0-9_]*$").expect("valid regex")
});

/// Namespaced tool names: server__tool format.
static NAMESPACED_TOOL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z][a-zA-Z0-9_]*__[a-zA-Z][a-zA-Z0-9_]*$").expect("valid regex")
});

pub fn validate_tool_name(name: &str) -> bool {
    TOOL_NAME_REGEX.is_match(name)
}

pub fn validate_namespaced_tool_name(name: &str) -> bool {
    NAMESPACED_TOOL_REGEX.is_match(name)
}
```

### ToolRegistry with Namespace Support
```rust
use std::collections::HashMap;
use std::sync::Arc;

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    builtin_names: std::collections::HashSet<String>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            builtin_names: std::collections::HashSet::new(),
        }
    }

    /// Register a built-in tool (flat name, always wins collisions).
    pub fn register_builtin(&mut self, tool: Arc<dyn Tool>) -> Result<(), BlufioError> {
        let name = tool.name().to_string();
        if !validate_tool_name(&name) {
            return Err(BlufioError::Skill {
                message: format!("invalid tool name: {name}"),
                source: None,
            });
        }
        self.builtin_names.insert(name.clone());
        self.tools.insert(name, tool);
        Ok(())
    }

    /// Register a namespaced external tool (server__tool format).
    /// Returns Err if name collides with built-in.
    pub fn register_namespaced(
        &mut self,
        namespace: &str,
        tool: Arc<dyn Tool>,
    ) -> Result<(), BlufioError> {
        let namespaced_name = format!("{namespace}__{}", tool.name());
        if !validate_namespaced_tool_name(&namespaced_name) {
            return Err(BlufioError::Skill {
                message: format!("invalid namespaced tool name: {namespaced_name}"),
                source: None,
            });
        }
        if self.builtin_names.contains(&namespaced_name) {
            tracing::warn!(
                "namespace collision: {namespaced_name} collides with built-in, skipping"
            );
            return Ok(()); // Built-in wins, log warning
        }
        if self.tools.contains_key(&namespaced_name) {
            tracing::warn!(
                "duplicate tool name: {namespaced_name} already registered, skipping"
            );
            return Ok(());
        }
        self.tools.insert(namespaced_name, tool);
        Ok(())
    }
}
```

### McpConfig TOML Parsing
```toml
# Example blufio.toml
[mcp]
enabled = true
export_tools = ["http", "file"]

[[mcp.servers]]
name = "github"
transport = "http"
url = "https://mcp.github.com"
auth_token = "ghp_xxx"

[[mcp.servers]]
name = "local-tools"
transport = "stdio"
command = "npx"
args = ["-y", "@my-org/mcp-server"]
```

### BlufioError Mcp Variant
```rust
// Extend existing BlufioError in blufio-core/src/error.rs
/// MCP protocol and connection errors.
#[error("mcp error: {message}")]
Mcp {
    message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
},
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| reqwest 0.12 | reqwest 0.13.2 | 2025 | API changes in redirect handling, TLS config; mostly compatible |
| schemars 0.8 | schemars 1.0 | 2025 | Complete rewrite of derive macro; not backward compatible |
| MCP spec 2024-11-05 | MCP spec 2025-11-25 | Nov 2025 | Streamable HTTP transport, elicitation, tasks capability |
| rmcp 0.16 | rmcp 0.17.0 | 2026 | OAuth support, task lifecycle, streamable HTTP server |

**Deprecated/outdated:**
- schemars 0.8: rmcp 0.17 requires schemars 1.0. Cannot mix.
- reqwest 0.12: Will not satisfy rmcp's dependency resolver.

## Open Questions

1. **reqwest 0.13 API Breaking Changes**
   - What we know: reqwest 0.13 is a semver bump from 0.12. Core API (get/post/json) is similar.
   - What's unclear: Exact breaking changes that affect Blufio crates using reqwest (blufio-skill, blufio-anthropic).
   - Recommendation: Run `cargo build` after upgrade to find any API breaks. Likely minimal -- mostly TLS backend and redirect config changes.

2. **rmcp default-features interaction**
   - What we know: rmcp default features include `base64`, `macros`, `server`. We need `server` for mcp-server crate and `client` for mcp-client crate.
   - What's unclear: Whether disabling defaults and cherry-picking features causes any compilation issues.
   - Recommendation: Use `default-features = false` at workspace level, then enable specific features per MCP crate.

## Sources

### Primary (HIGH confidence)
- Context7 /websites/rs_rmcp - rmcp SDK API, tool_router macro, ServerHandler trait, client patterns
- Context7 /websites/rs_rmcp_rmcp - rmcp structured output, Calculator example with schemars
- GitHub modelcontextprotocol/rust-sdk Cargo.toml - rmcp 0.17.0 dependencies: reqwest 0.13.2, schemars 1.0, rand 0.10
- Existing codebase: BlufioConfig (model.rs), ToolRegistry (tool.rs), BlufioError (error.rs), SessionId (types.rs)

### Secondary (MEDIUM confidence)
- crates.io reqwest 0.13 - version availability confirmed
- GitHub rust-sdk README - transport options, feature flags

### Tertiary (LOW confidence)
- None

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - rmcp 0.17 confirmed via GitHub, dependencies verified
- Architecture: HIGH - patterns derived from existing codebase analysis
- Pitfalls: HIGH - reqwest version conflict identified and verified, schemars compatibility confirmed

**Research date:** 2026-03-02
**Valid until:** 2026-04-02 (30 days -- stable dependencies)
