# Roadmap: Blufio

## Milestones

- ✅ **v1.0 MVP** — Phases 1-14 (shipped 2026-03-02)
- **v1.1 MCP Integration** — Phases 15-19 (in progress)

## Phases

<details>
<summary>v1.0 MVP (Phases 1-14) — SHIPPED 2026-03-02</summary>

- [x] Phase 1: Project Foundation & Workspace (2/2 plans) — completed 2026-02-28
- [x] Phase 2: Persistence & Security Vault (2/2 plans) — completed 2026-02-28
- [x] Phase 3: Agent Loop & Telegram (4/4 plans) — completed 2026-03-01
- [x] Phase 4: Context Engine & Cost Tracking (3/3 plans) — completed 2026-03-01
- [x] Phase 5: Memory & Embeddings (3/3 plans) — completed 2026-03-01
- [x] Phase 6: Model Routing & Smart Heartbeats (3/3 plans) — completed 2026-03-01
- [x] Phase 7: WASM Skill Sandbox (4/4 plans) — completed 2026-03-01
- [x] Phase 8: Plugin System & Gateway (3/3 plans) — completed 2026-03-01
- [x] Phase 9: Production Hardening (3/3 plans) — completed 2026-03-01
- [x] Phase 10: Multi-Agent & Final Integration (3/3 plans) — completed 2026-03-01
- [x] Phase 11: Fix Critical Integration Bugs (4/4 plans) — completed 2026-03-01
- [x] Phase 12: Verify Unverified Phases (5/5 plans) — completed 2026-03-01
- [x] Phase 13: Sync Traceability & Documentation (1/1 plan) — completed 2026-03-02
- [x] Phase 14: Wire Cross-Phase Integration (3/3 plans) — completed 2026-03-02

</details>

### v1.1 MCP Integration

**Milestone Goal:** Make Blufio a full MCP citizen -- expose its capabilities as an MCP server (tools, resources, prompts) and consume external MCP tools as a client. Done = operator can (1) point Claude Desktop at Blufio via stdio and use skills/memory, (2) configure external MCP servers in TOML, (3) agent uses external MCP tools in conversation.

**Phase Numbering:** Integer phases (15, 16, 17, 18, 19). Decimal phases (e.g. 16.1) reserved for urgent insertions.

- [x] **Phase 15: MCP Foundation** - Config structs, workspace crates, dependency integration, namespace enforcement, abstraction boundary
- [ ] **Phase 16: MCP Server stdio** - ServerHandler, tools/list, tools/call, stdio transport, Claude Desktop connectivity
- [ ] **Phase 17: MCP Server HTTP + Resources** - Streamable HTTP transport, auth, resources, prompts, notifications, CORS
- [ ] **Phase 18: MCP Client** - External MCP server connections, tool discovery, security hardening, agent integration
- [ ] **Phase 19: Integration Testing + Tech Debt** - E2E tests, cross-contamination, Prometheus metrics, connection limits, v1.0 debt

## Phase Details

### Phase 15: MCP Foundation
**Goal**: Both MCP crates can compile and the ToolRegistry enforces namespaced tool names with collision detection
**Depends on**: Phase 14 (v1.0 complete)
**Requirements**: FOUND-01, FOUND-02, FOUND-03, FOUND-04, FOUND-05, FOUND-06
**Success Criteria** (what must be TRUE):
  1. TOML config with `[mcp]` section and `[[mcp.servers]]` array parses correctly and rejects unknown fields
  2. `cargo build -p blufio-mcp-server` and `cargo build -p blufio-mcp-client` succeed with feature flags
  3. ToolRegistry rejects duplicate tool names across namespaces and built-in tools always win priority
  4. MCP session IDs and Blufio session IDs are distinct types that cannot be accidentally conflated
  5. No rmcp types appear in any public API outside blufio-mcp-server and blufio-mcp-client
**Plans**: 4/4 completed (2026-03-02)

### Phase 16: MCP Server stdio
**Goal**: Operator can point Claude Desktop at Blufio via stdio and invoke skills as MCP tools
**Depends on**: Phase 15
**Requirements**: SRVR-01, SRVR-02, SRVR-03, SRVR-04, SRVR-05, SRVR-12, SRVR-15
**Success Criteria** (what must be TRUE):
  1. Claude Desktop connects to `blufio mcp-server` via stdio, completes capability negotiation, and lists available tools
  2. Claude Desktop can invoke a Blufio skill through MCP tools/call and receive the result
  3. Invalid tool inputs return JSON-RPC -32602 error with a human-readable message
  4. Only tools on the explicit export allowlist are visible to MCP clients (bash is never exposed)
  5. All process output goes to stderr in stdio mode -- no stdout corruption of the JSON-RPC stream
**Plans**: TBD

### Phase 17: MCP Server HTTP + Resources
**Goal**: Remote clients can access Blufio via Streamable HTTP at /mcp, and MCP clients can browse memory and session history as resources
**Depends on**: Phase 16
**Requirements**: SRVR-06, SRVR-07, SRVR-08, SRVR-09, SRVR-10, SRVR-11, SRVR-13, SRVR-14, SRVR-16
**Success Criteria** (what must be TRUE):
  1. MCP client connects via Streamable HTTP at /mcp with bearer token and lists tools and resources
  2. MCP client reads a memory item via `blufio://memory/{id}` and searches memory via the search template
  3. MCP client reads session history as a read-only resource
  4. Prompt templates are available via prompts/list and prompts/get
  5. CORS rejects requests from origins not in the configured allowlist
**Plans**: TBD

### Phase 18: MCP Client
**Goal**: Agent discovers and invokes external MCP tools configured by the operator, with security hardening that prevents tool poisoning, rug pulls, and context window blowups
**Depends on**: Phase 17
**Requirements**: CLNT-01, CLNT-02, CLNT-03, CLNT-04, CLNT-05, CLNT-06, CLNT-07, CLNT-08, CLNT-09, CLNT-10, CLNT-11, CLNT-12, CLNT-13, CLNT-14
**Success Criteria** (what must be TRUE):
  1. Operator configures an external MCP server in TOML and the agent discovers its tools with namespace-prefixed names
  2. Agent invokes an external MCP tool during a conversation turn and the result appears in the response
  3. Config entries with `command:` (stdio transport) are rejected with a clear error message
  4. Tool definitions are SHA-256 hash-pinned at discovery; schema mutations disable the tool and alert the operator
  5. External tool descriptions are sanitized (instruction patterns stripped, 200-char cap) and labeled as a separate trust zone in prompt context
**Plans**: TBD

### Phase 19: Integration Testing + Tech Debt
**Goal**: End-to-end MCP workflows are verified across server and client, Prometheus observability covers MCP, and critical v1.0 tech debt is resolved
**Depends on**: Phase 18
**Requirements**: INTG-01, INTG-02, INTG-03, INTG-04, INTG-05, DEBT-01, DEBT-02, DEBT-03, DEBT-04, DEBT-05, DEBT-06, DEBT-07
**Success Criteria** (what must be TRUE):
  1. E2E test passes: Claude Desktop connects via stdio, lists tools, invokes a tool, and reads a memory resource
  2. E2E test passes: agent uses an external MCP tool in a conversation turn end-to-end
  3. JSON-RPC requests to non-MCP endpoints return 4xx; gateway-format requests to /mcp return MCP protocol errors
  4. GET /v1/sessions returns actual session data from storage (not a hard-coded empty list)
  5. `blufio doctor` reports MCP server health for all configured external servers
**Plans**: TBD

## Progress

**Execution Order:** Phases execute in numeric order: 15 -> 15.x -> 16 -> 16.x -> 17 -> 17.x -> 18 -> 18.x -> 19

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Project Foundation & Workspace | v1.0 | 2/2 | Complete | 2026-02-28 |
| 2. Persistence & Security Vault | v1.0 | 2/2 | Complete | 2026-02-28 |
| 3. Agent Loop & Telegram | v1.0 | 4/4 | Complete | 2026-03-01 |
| 4. Context Engine & Cost Tracking | v1.0 | 3/3 | Complete | 2026-03-01 |
| 5. Memory & Embeddings | v1.0 | 3/3 | Complete | 2026-03-01 |
| 6. Model Routing & Smart Heartbeats | v1.0 | 3/3 | Complete | 2026-03-01 |
| 7. WASM Skill Sandbox | v1.0 | 4/4 | Complete | 2026-03-01 |
| 8. Plugin System & Gateway | v1.0 | 3/3 | Complete | 2026-03-01 |
| 9. Production Hardening | v1.0 | 3/3 | Complete | 2026-03-01 |
| 10. Multi-Agent & Final Integration | v1.0 | 3/3 | Complete | 2026-03-01 |
| 11. Fix Critical Integration Bugs | v1.0 | 4/4 | Complete | 2026-03-01 |
| 12. Verify Unverified Phases | v1.0 | 5/5 | Complete | 2026-03-01 |
| 13. Sync Traceability & Documentation | v1.0 | 1/1 | Complete | 2026-03-02 |
| 14. Wire Cross-Phase Integration | v1.0 | 3/3 | Complete | 2026-03-02 |
| 15. MCP Foundation | v1.1 | 4/4 | Complete | 2026-03-02 |
| 16. MCP Server stdio | v1.1 | 0/TBD | Not started | - |
| 17. MCP Server HTTP + Resources | v1.1 | 0/TBD | Not started | - |
| 18. MCP Client | v1.1 | 0/TBD | Not started | - |
| 19. Integration Testing + Tech Debt | v1.1 | 0/TBD | Not started | - |

---
*Roadmap created: 2026-03-02*
*Last updated: 2026-03-02 (Phase 15 completed)*
