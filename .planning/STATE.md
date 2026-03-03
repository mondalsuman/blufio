---
gsd_state_version: 1.0
milestone: v1.1
milestone_name: MCP Integration
status: in-progress
last_updated: "2026-03-03T14:11:32Z"
progress:
  total_phases: 6
  completed_phases: 5
  total_plans: 25
  completed_plans: 23
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-02)

**Core value:** An always-on personal AI agent that is secure enough to trust, efficient enough to afford, and simple enough to deploy by copying one file.
**Current focus:** v1.1 MCP Integration -- Phase 21: Fix MCP Wiring Gaps

## Current Position

Phase: 21 of 22 (Fix MCP Wiring Gaps)
Plan: 1 of 4 in current phase
Status: Phase 21 In Progress
Last activity: 2026-03-03 -- Plan 21-01 complete (PinStore wiring + server cost attribution)

Progress: [##########################....] 18/20 phases (v1.0 complete, v1.1 Phases 15-18 done, Phase 21 plan 1/4)

## Performance Metrics

**Velocity (v1.0):**
- Total plans completed: 43
- Total execution time: ~3 days
- Average: ~10 plans/day

**v1.1:**
- Phase 15: 4 plans completed
- Phase 16: 3 plans completed
- Phase 17: 5 plans completed (17-01, 33min, 2 tasks, 11 files; 17-02, 15min, 2 tasks, 2 files; 17-03, 17min, 2 tasks, 6 files; 17-04, 15min, 2 tasks, 4 files; 17-05, 5min, 1 task, 2 files)
- Phase 18: 4 plans completed (18-01: config+security; 18-02: manager+ExternalTool+wiring; 18-03: PinStore+health+unregister; 18-04: doctor checks)
- Phase 21: 1/4 plans completed (21-01: 18min, 2 tasks, 6 files)
- Total plans completed: 17

## Accumulated Context

### Decisions

All v1.0 decisions logged in PROJECT.md Key Decisions table.

v1.1 decisions so far:
- rmcp 0.17.0 selected as MCP SDK (official Anthropic-maintained Rust SDK)
- HTTP-only transport for MCP client (no stdio subprocess spawning -- preserves single-binary constraint)
- Server before client phase ordering (primary done condition is Claude Desktop connectivity)
- Security embedded per phase, not deferred (namespace in 15, export allowlist in 16, CORS/auth in 17, hash pinning in 18)
- reqwest 0.13 feature rustls-tls renamed to rustls -- updated workspace config
- teloxide-core still pulls reqwest 0.12 as transitive dep -- acceptable dual version
- McpSessionId placed in blufio-mcp-server (not blufio-core) per CONTEXT.md
- register_namespaced() skips on collision (returns Ok) rather than erroring -- graceful degradation
- list() and tool_definitions() use registry key for namespaced tools, not tool.name()
- Triple underscore (server___tool) accepted as valid namespace format
- to_mcp_tool() takes separate name parameter to support namespace-prefixed tool names
- jsonschema 0.28 for input validation (not latest 0.44, matches plan spec)
- serve_stdio() wraps rmcp in blufio-mcp-server, keeping rmcp out of public API
- RedactingMakeWriter duplicated in mcp_server.rs (independent from serve.rs)
- Default tool annotations: read_only=false, destructive=false, idempotent=false, open_world=true
- All annotation hints always populated with explicit Some(bool) for MCP clients
- StreamableHttpService factory closure pattern with Arc<handler> cloning per session
- MCP router nested at /mcp before permissive CorsLayer (restricted CORS on MCP routes)
- GatewayChannel.set_mcp_router() for pre-connect MCP injection (avoids Router in Clone config)
- Signal handler moved earlier in serve.rs for MCP CancellationToken availability
- blufio:// URI scheme for MCP resource addressing (memory/{id}, memory/search, sessions, sessions/{id})
- with_resources() builder pattern: stdio mode skips resources, HTTP mode injects MemoryStore + StorageAdapter
- Memory resources exclude embedding vectors (explicit field selection, not serde derive)
- initialize_memory returns 3-tuple to expose Arc<MemoryStore> for MCP resource sharing
- Blufio-owned prompt types (PromptDef, PromptArgDef, PromptMessageDef) mapped to rmcp types only in handler.rs
- System messages use PromptMessageRole::Assistant (MCP spec has no system role)
- tokio::sync::watch with u64 generation counter for tools-changed notification coalescing
- ProgressReporter logs via tracing until WASM tools support progress callbacks
- ToolsChangedSender held via Option<> with underscore prefix in serve.rs (no callers yet)
- ProgressReporter created with underscore prefix in call_tool (BlufioTool::invoke lacks progress callback)
- progressToken extraction handles both String and Number value types per MCP spec
- PinStore opened from database_path in serve.rs with graceful fallback on failure
- connected_session_map() added to McpClientManager for health monitoring (CLNT-06)

### Pending Todos

None.

### Blockers/Concerns

- rmcp reconnection API: unclear if McpClientSession supports re-initialization without dropping ToolRegistry tools -- investigate in Phase 18 planning
- ContextEngine progressive disclosure with runtime MCP tools: design needed for Phase 18
- OAuth 2.1 deferred to v1.2 -- validate bearer token is sufficient for target users before Phase 17

## Session Continuity

Last session: 2026-03-03
Stopped at: Completed 21-01-PLAN.md (PinStore wiring + server cost attribution)
Next action: Execute 21-02-PLAN.md
