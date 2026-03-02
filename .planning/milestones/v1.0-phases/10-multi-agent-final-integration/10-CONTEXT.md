# Phase 10: Multi-Agent & Final Integration - Context

**Gathered:** 2026-03-01
**Status:** Ready for planning

<domain>
## Phase Boundary

Multiple specialized agents can delegate work to each other via Ed25519-signed inter-session messages, and the complete system passes end-to-end integration validation across all v1 requirements. This phase delivers two capabilities: (1) multi-agent routing with cryptographic signing (SEC-07, INFRA-06), and (2) E2E smoke tests validating the full Blufio pipeline.

</domain>

<decisions>
## Implementation Decisions

### Delegation Model
- In-process delegation: primary agent spawns new SessionActors in the same process for specialists
- Synchronous execution: primary blocks (with timeout) until specialist responds
- Tool-based triggering: delegation exposed as tool calls (e.g., `delegate_to_researcher`), LLM decides when to delegate
- Single-level depth only: primary can delegate to specialists, specialists cannot delegate further (no chains)
- Hidden from user: specialist responses are tool_result content incorporated by the primary — user sees one coherent response
- Ephemeral sessions: specialist SessionActors created per delegation and discarded after — no persistent state

### Message Signing Scheme
- Full payload signing: entire serialized message (sender, recipient, task, content, timestamp) is signed with Ed25519
- Per-agent keypairs: each agent (primary + specialists) generates its own Ed25519 keypair at startup; primary registers specialist public keys
- No replay protection for v1: in-process agents share a trust boundary, replay protection deferred
- Extend DeviceKeypair: add `sign()` and `verify()` methods to existing `DeviceKeypair` in `blufio-auth-keypair` crate

### Agent Specialization
- TOML-configured: agents defined in `blufio.toml` with name, system_prompt, allowed skills, and model preference
- Fresh context per delegation: specialists get their own system prompt + delegated task description, no access to primary's conversation history
- Primary provides relevant context in the task payload

### E2E Validation
- Mocked services: use mock implementations of ProviderAdapter and ChannelAdapter — fast, free, deterministic, CI-runnable
- Integration test binary: dedicated `tests/e2e.rs` using standard `#[tokio::test]` with TestHarness builder pattern
- Core pipeline scenarios (must-have): (1) message-to-response pipeline, (2) conversation persistence across restart, (3) tool execution loop, (4) multi-agent delegation, (5) cost tracking, (6) budget enforcement, (7) Ed25519 signing/verification
- Nice-to-have scenarios: memory recall, model routing, plugin loading, Prometheus export, gateway API
- Reusable test utility crate: `blufio-test-utils` with MockProvider, MockChannel, TestHarness — available to all workspace crates

### Claude's Discretion
- Exact delegation timeout value
- AgentMessage struct field design
- TestHarness builder API design
- Mock response generation strategy
- Integration test organization within e2e.rs

</decisions>

<specifics>
## Specific Ideas

- Delegation fits naturally into the existing tool-use/tool_result loop in SessionActor — the specialist response becomes a tool_result
- DeviceKeypair already imports `ed25519_dalek` which has full sign/verify — just needs methods exposed
- The `[[agents]]` TOML array pattern matches the existing config style (deny_unknown_fields, serde)
- TestHarness with builder pattern mirrors the existing serve.rs initialization pattern but with mocks

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `DeviceKeypair` (`blufio-auth-keypair`): Ed25519 keypair with `ed25519_dalek`, needs `sign()`/`verify()` added
- `SessionActor` (`blufio-agent/session.rs`): Per-session FSM with tool execution loop — delegation fits as a new tool type
- `ToolRegistry` (`blufio-skill`): Dynamic tool registration — delegation tools can be registered here
- `AgentLoop` (`blufio-agent`): Manages sessions, can be extended with specialist session lifecycle
- `ChannelMultiplexer` (`blufio-agent`): Multi-channel aggregation pattern
- `CostLedger` + `BudgetTracker` (`blufio-cost`): Already tracks per-session costs — specialist sessions get their own tracking

### Established Patterns
- Adapter trait pattern: 7 adapter types via `PluginAdapter` — no new adapter type needed, delegation is internal
- TOML config with `deny_unknown_fields`: all config goes through `BlufioConfig` → `serde` → strict validation
- Arc-based sharing: storage, provider, cost_ledger shared via `Arc<dyn Trait + Send + Sync>`
- Feature-gated compilation: `#[cfg(feature = "...")]` pattern for optional components

### Integration Points
- `SessionActor::execute_tools()`: Where delegation tool calls would be intercepted and routed to specialist sessions
- `ToolRegistry`: Delegation tools registered alongside built-in tools at startup
- `BlufioConfig`: New `[[agents]]` section for specialist definitions
- `serve.rs::run_serve()`: Where specialist agent configs are loaded and keypairs generated

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 10-multi-agent-final-integration*
*Context gathered: 2026-03-01*
