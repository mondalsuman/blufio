# Requirements: Blufio

**Defined:** 2026-03-02
**Core Value:** An always-on personal AI agent that is secure enough to trust, efficient enough to afford, and simple enough to deploy by copying one file.

## v1.1 Requirements

Requirements for MCP Integration milestone. Each maps to roadmap phases.

### Foundation

- [x] **FOUND-01**: MCP config structs added to blufio-config with TOML parsing and deny_unknown_fields
- [x] **FOUND-02**: Workspace crates blufio-mcp-server and blufio-mcp-client scaffolded with feature flags
- [x] **FOUND-03**: rmcp 0.17.0 and schemars 1.0 added to workspace dependencies (verify reqwest version unification)
- [x] **FOUND-04**: Tool namespace convention enforced in ToolRegistry with collision detection and built-in priority
- [x] **FOUND-05**: MCP session ID type distinct from Blufio conversation session ID
- [x] **FOUND-06**: rmcp abstraction boundary established (Blufio-owned types, no public rmcp re-exports)

### MCP Server

- [x] **SRVR-01**: User can connect Claude Desktop to Blufio via stdio and list available tools
- [x] **SRVR-02**: User can invoke Blufio skills from Claude Desktop via MCP tools/call
- [x] **SRVR-03**: `blufio mcp-server` CLI subcommand for stdio-only mode (no agent loop)
- [x] **SRVR-04**: Capability negotiation (initialize/initialized handshake) with MCP spec 2025-11-25
- [x] **SRVR-05**: Tool input validation against inputSchema with JSON-RPC -32602 errors
- [x] **SRVR-06**: Streamable HTTP transport mounted on existing gateway at /mcp
- [x] **SRVR-07**: MCP-specific auth middleware for HTTP transport (bearer token)
- [x] **SRVR-08**: Memory exposed as MCP resources (blufio://memory/{id}, search template)
- [x] **SRVR-09**: Session history exposed as read-only MCP resources
- [x] **SRVR-10**: Prompt templates via prompts/list and prompts/get
- [x] **SRVR-11**: Tool annotations (readOnlyHint, destructiveHint, idempotentHint, openWorldHint)
- [x] **SRVR-12**: Explicit MCP tool export allowlist (bash permanently excluded, default empty)
- [x] **SRVR-13**: notifications/tools/list_changed emitted on skill install or discovery changes
- [x] **SRVR-14**: Progress notifications for long-running WASM tools
- [x] **SRVR-15**: All logging redirected to stderr in stdio mode with clippy::print_stdout lint
- [x] **SRVR-16**: CORS restricted to configured origins on MCP HTTP endpoints

### MCP Client

- [ ] **CLNT-01**: User can configure external MCP servers via TOML ([[mcp.servers]])
- [ ] **CLNT-02**: Blufio connects to external MCP servers via Streamable HTTP transport
- [ ] **CLNT-03**: External tools discovered (tools/list) and registered in ToolRegistry with namespace prefix
- [ ] **CLNT-04**: Agent can invoke external MCP tools in conversation turns
- [ ] **CLNT-05**: Legacy SSE client transport for backward compatibility with older MCP servers
- [ ] **CLNT-06**: Connection lifecycle management (ping health checks, exponential backoff, graceful degradation)
- [x] **CLNT-07**: SHA-256 hash pinning of tool definitions at discovery (stored in SQLite)
- [ ] **CLNT-08**: Description sanitization (instruction-pattern stripping, 200-char cap on external descriptions)
- [ ] **CLNT-09**: Response size caps (4096 char default, configurable per-server in TOML)
- [ ] **CLNT-10**: External tools labeled as separate trust zone in prompt context
- [ ] **CLNT-11**: HTTP-only transport enforced (reject command: config entries with clear error message)
- [x] **CLNT-12**: Per-server budget tracking in unified cost ledger
- [ ] **CLNT-13**: MCP server health checks added to `blufio doctor`
- [ ] **CLNT-14**: Client startup failure is non-fatal (agent starts without external MCP tools)

### Integration & Hardening

- [ ] **INTG-01**: E2E test: Claude Desktop connects via stdio, lists tools, invokes tool, reads resource
- [ ] **INTG-02**: E2E test: Agent uses external MCP tool in a conversation turn
- [ ] **INTG-03**: Cross-contamination tests (JSON-RPC to non-MCP endpoints returns 4xx, vice versa)
- [ ] **INTG-04**: Prometheus metrics for MCP (connection count, tool response sizes, context utilization)
- [ ] **INTG-05**: Connection count limits enforced (configurable defaults)

### Tech Debt

- [ ] **DEBT-01**: GET /v1/sessions returns actual session data (wire StorageAdapter into GatewayState)
- [ ] **DEBT-02**: Commit systemd unit file for production deployment
- [ ] **DEBT-03**: Refactor SessionActor constructor to reduce argument count
- [ ] **DEBT-04**: Live Telegram E2E verification (human test)
- [ ] **DEBT-05**: Session persistence verification across restarts (human test)
- [ ] **DEBT-06**: SIGTERM drain timing verification (human test)
- [ ] **DEBT-07**: Memory bounds measured over 72+ hour runtime

## Future Requirements

Deferred to v1.2+. Tracked but not in current roadmap.

### MCP Advanced

- **MCPA-01**: Tasks capability (experimental in MCP spec, no Claude Desktop support)
- **MCPA-02**: Elicitation capability (requires UI proxy through Telegram)
- **MCPA-03**: Sampling capability (MCP servers request LLM completions through Blufio)
- **MCPA-04**: OAuth 2.1 authorization for remote MCP server access
- **MCPA-05**: MCP Apps Extension support
- **MCPA-06**: Resource subscriptions for memory changes

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| stdio transport for MCP client | Violates single-binary constraint; requires spawning subprocesses with full process permissions |
| MCP Bundles distribution | Single binary IS the distribution; bundles add complexity without value |
| OAuth 2.1 in v1.1 | Bearer token sufficient for initial deployment; OAuth needed only for public-facing remote servers |
| Native plugin system via libloading | WASM-only for sandboxing guarantees; deferred from v1.0 |
| Additional channel adapters (Discord, Slack) | Separate milestone focus |
| DAG workflow engine | Post-v1.1 |
| Client SDKs | Post-v1.1 |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| FOUND-01 | Phase 15 (verify: Phase 20) | Complete |
| FOUND-02 | Phase 15 (verify: Phase 20) | Complete |
| FOUND-03 | Phase 15 (verify: Phase 20) | Complete |
| FOUND-04 | Phase 15 (verify: Phase 20) | Complete |
| FOUND-05 | Phase 15 (verify: Phase 20) | Complete |
| FOUND-06 | Phase 15 (verify: Phase 20) | Complete |
| SRVR-01 | Phase 16 (verify: Phase 20) | Complete |
| SRVR-02 | Phase 16 (verify: Phase 20) | Complete |
| SRVR-03 | Phase 16 (verify: Phase 20) | Complete |
| SRVR-04 | Phase 16 (verify: Phase 20) | Complete |
| SRVR-05 | Phase 16 (verify: Phase 20) | Complete |
| SRVR-06 | Phase 17 | Complete |
| SRVR-07 | Phase 17 | Complete |
| SRVR-08 | Phase 17 | Complete |
| SRVR-09 | Phase 17 | Complete |
| SRVR-10 | Phase 17 | Complete |
| SRVR-11 | Phase 17 | Complete |
| SRVR-12 | Phase 16 (verify: Phase 20) | Complete |
| SRVR-13 | Phase 17 | Complete |
| SRVR-14 | Phase 17 | Complete |
| SRVR-15 | Phase 16 (verify: Phase 20) | Complete |
| SRVR-16 | Phase 17 | Complete |
| CLNT-01 | Phase 18 (verify: Phase 22) | Pending |
| CLNT-02 | Phase 18 (verify: Phase 22) | Pending |
| CLNT-03 | Phase 18 (verify: Phase 22) | Pending |
| CLNT-04 | Phase 18 (verify: Phase 22) | Pending |
| CLNT-05 | Phase 18 (verify: Phase 22) | Pending |
| CLNT-06 | Phase 18 (fix: Phase 21, verify: Phase 22) | Pending |
| CLNT-07 | Phase 18 (fix: Phase 21, verify: Phase 22) | Complete |
| CLNT-08 | Phase 18 (verify: Phase 22) | Pending |
| CLNT-09 | Phase 18 (verify: Phase 22) | Pending |
| CLNT-10 | Phase 18 (fix: Phase 21, verify: Phase 22) | Pending |
| CLNT-11 | Phase 18 (verify: Phase 22) | Pending |
| CLNT-12 | Phase 18 (fix: Phase 21, verify: Phase 22) | Complete |
| CLNT-13 | Phase 18 (verify: Phase 22) | Pending |
| CLNT-14 | Phase 18 (verify: Phase 22) | Pending |
| INTG-01 | Phase 19 (verify: Phase 22) | Pending |
| INTG-02 | Phase 19 (verify: Phase 22) | Pending |
| INTG-03 | Phase 19 (verify: Phase 22) | Pending |
| INTG-04 | Phase 19 (fix: Phase 21, verify: Phase 22) | Pending |
| INTG-05 | Phase 19 (verify: Phase 22) | Pending |
| DEBT-01 | Phase 19 (verify: Phase 22) | Pending |
| DEBT-02 | Phase 19 (verify: Phase 22) | Pending |
| DEBT-03 | Phase 19 (verify: Phase 22) | Pending |
| DEBT-04 | Phase 19 (verify: Phase 22) | Pending |
| DEBT-05 | Phase 19 (verify: Phase 22) | Pending |
| DEBT-06 | Phase 19 (verify: Phase 22) | Pending |
| DEBT-07 | Phase 19 (verify: Phase 22) | Pending |

**Coverage:**
- v1.1 requirements: 48 total
- Mapped to phases: 48
- Unmapped: 0

---
*Requirements defined: 2026-03-02*
*Last updated: 2026-03-03 after gap closure phases 21-22 added*
