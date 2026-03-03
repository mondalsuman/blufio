# Phase 19: Integration Testing + Tech Debt - Context

**Gathered:** 2026-03-03
**Status:** Ready for planning

<domain>
## Phase Boundary

End-to-end MCP workflows verified across server and client, Prometheus observability covers MCP, and critical v1.0 tech debt resolved. Requirements: INTG-01 through INTG-05, DEBT-01 through DEBT-07.

</domain>

<decisions>
## Implementation Decisions

### Claude's Discretion

All areas discussed were delegated to Claude's judgment. The following decisions should be guided by existing codebase patterns and best engineering practices:

**E2E Test Strategy (INTG-01, INTG-02, INTG-03)**
- Test approach for MCP stdio E2E (INTG-01): in-process mock vs real subprocess — prefer consistency with existing TestHarness patterns (mock provider/channel)
- Test approach for agent-uses-external-MCP-tool E2E (INTG-02): mock MCP server vs fixture binary — prefer in-process for CI speed
- Cross-contamination tests (INTG-03): unit tests on router/handler vs integration with running server — use whichever level provides thorough coverage with minimal complexity
- MCP Prometheus metrics (INTG-04): which metrics to add (connection count, tool response sizes, context utilization required per spec) — pick metrics that satisfy requirements without noise

**Human Verification Tasks (DEBT-04, DEBT-05, DEBT-06, DEBT-07)**
- Format for human test procedures — runbook markdown, semi-automated scripts, or hybrid
- Memory bounds test setup (DEBT-07, 72h) — local/VPS manual run vs containerized with monitoring
- Session persistence verification depth (DEBT-05) — session data only vs full conversation continuity
- SIGTERM drain window (DEBT-06) — 30s vs 60s, pick based on typical LLM response times and systemd conventions

**Connection Limits (INTG-05)**
- Default max concurrent MCP connections — pick a conservative default for personal agent use case
- Over-limit behavior — hard reject (503) vs queue with timeout
- Limit scope — global across transports vs per-transport
- Configuration method — config file only vs config file + env var override, consistent with existing blufio-config patterns

**Systemd & Deployment (DEBT-02)**
- Target OS — any Linux with systemd (portable unit file)
- Installation paths — pick paths that work with single-binary design
- Service user — dedicated user vs root, follow systemd security best practices
- Restart policy — on-failure vs always, pick what makes sense for a long-running agent

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. All implementation decisions delegated to Claude's engineering judgment based on existing codebase patterns.

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- **TestHarness** (`blufio-test-utils/src/harness.rs`): Full agent pipeline test environment with builder pattern, mock provider/channel, temp SQLite. Already tests message pipeline, cost tracking, ed25519, delegation. Extend for MCP E2E.
- **MockProvider** (`blufio-test-utils/src/mock_provider.rs`): Canned LLM responses for testing
- **MockChannel** (`blufio-test-utils/src/mock_channel.rs`): Mock channel adapter
- **E2E test suite** (`blufio/tests/e2e.rs`): 7 existing E2E tests covering message pipeline, persistence, cost tracking, signing, delegation, isolation
- **PrometheusAdapter** (`blufio-prometheus/src/lib.rs`): metrics-rs facade with Prometheus exporter, `recording.rs` helpers for counter/gauge/histogram
- **recording.rs** (`blufio-prometheus/src/recording.rs`): Existing metrics — messages_total, tokens_total, errors_total, active_sessions, budget_remaining, memory_heap/rss/resident/pressure, response_latency. No MCP-specific metrics yet.
- **Doctor command** (`blufio/src/doctor.rs`): Diagnostic checks framework — add MCP health checks here
- **MCP health monitor** (`blufio-mcp-client/src/health.rs`): Client-side health monitoring for external MCP servers

### Established Patterns
- **Test pattern**: TestHarness builder with `.with_mock_responses()`, `.with_budget()`, `.build().await` — all E2E tests use this
- **Metrics pattern**: `describe_*!()` in `register_metrics()`, recording helpers as free functions (`record_message()`, `set_active_sessions()`)
- **Config pattern**: `blufio-config` crate with `model.rs` structs, TOML deserialization
- **Handler pattern**: axum handlers in `blufio-gateway/src/handlers.rs` with State extraction

### Integration Points
- **GET /v1/sessions** (`handlers.rs:253`): Currently returns hardcoded empty list — DEBT-01 wires StorageAdapter into GatewayState
- **SessionActor::new** (`blufio-agent/src/session.rs:115`): 15 constructor arguments — DEBT-03 targets this for refactoring
- **MCP server crate** (`blufio-mcp-server`): handler, transport, auth, bridge, resources modules — integration test targets
- **MCP client crate** (`blufio-mcp-client`): manager, health, external_tool, pin_store — integration test targets
- **Gateway server** (`blufio-gateway/src/server.rs`): Routes and state — add connection limit middleware here

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 19-integration-testing-tech-debt*
*Context gathered: 2026-03-03*
