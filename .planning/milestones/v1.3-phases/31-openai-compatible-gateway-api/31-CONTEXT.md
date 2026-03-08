# Phase 31: OpenAI-Compatible Gateway API - Context

**Gathered:** 2026-03-05
**Status:** Ready for planning

<domain>
## Phase Boundary

External callers can use Blufio as a drop-in OpenAI-compatible server via standard API endpoints: POST /v1/chat/completions, POST /v1/responses, GET /v1/tools, POST /v1/tools/invoke. OpenAI wire types are completely separate from internal ProviderResponse — no Anthropic-specific field names leak to external callers.

Scoped API keys, webhooks, and batch operations are Phase 32. This phase delivers the endpoints themselves.

</domain>

<decisions>
## Implementation Decisions

### Model string mapping
- Use `provider/model` format for the `model` field (e.g., `openai/gpt-4o`, `ollama/llama3`, `gemini/gemini-pro`)
- Support config-defined aliases with ordered fallback chains: `fast = ["ollama/llama3", "openrouter/meta-llama/llama-3-8b"]` — first healthy provider wins
- Bare model names (no provider prefix) route to the provider configured as `providers.default` in config
- Expose GET /v1/models returning a merged list across all configured providers in OpenAI ListModels format, with optional `?provider=ollama` filter
- Ollama models auto-discovered via /api/tags; other providers list models from config

### Wire format fidelity
- Superset of OpenAI format: full OpenAI compatibility PLUS extra fields with `x_` prefix (provider, cost, latency) that don't break OpenAI SDKs
- Support core request parameter set: model, messages, temperature, max_tokens, stream, tools, tool_choice, response_format, stop, n (=1 only). Unsupported params return 400 with clear message.
- Extended error format: OpenAI base shape `{"error": {"message", "type", "param", "code"}}` plus `provider` and `retry_after` fields
- Streaming usage follows OpenAI's opt-in behavior: only include usage in final chunk when caller sends `stream_options: {include_usage: true}`

### OpenResponses event design
- Claude has discretion on event set — determine what the OpenAI Agents SDK actually needs for basic functionality and implement that
- Multi-turn via previous_response_id mapped to Blufio session IDs; reuse existing session infrastructure
- Map OpenAI built-in tool names to Blufio equivalents where they exist (e.g., web_search -> MCP tool if configured); return unsupported_tool error for others
- Streaming only for now — no background/async mode (store: true). Async execution deferred to batch phase (32)

### Tool invoke semantics
- POST /v1/tools/invoke is direct execution — bypasses LLM entirely. Caller specifies tool name + input JSON, gets result back
- Operator configures which tools are API-accessible via explicit allowlist in config (not all tools should be externally callable)
- Synchronous request-response only — no streaming tool results
- GET /v1/tools returns extended format: OpenAI function schema base ({type, function: {name, description, parameters}}) plus source (wasm/mcp/builtin), version, required_permissions metadata

### Claude's Discretion
- Exact OpenResponses event set (research what Agents SDK needs)
- Alias fallback health-check implementation details
- Error type taxonomy (which errors map to which OpenAI error types)
- How to handle provider-specific parameters that don't map across providers

</decisions>

<specifics>
## Specific Ideas

- Drop-in compatibility is the goal — any OpenAI SDK client should work by changing the base URL to Blufio's address
- The provider/model format matches OpenRouter's convention, which users are already familiar with
- Alias fallback chains enable local-first development (try Ollama, fall back to cloud)

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `blufio-gateway` crate: axum-based HTTP server with auth middleware, SSE support, and GatewayState with inbound message channels
- `blufio-openai` crate: ChatRequest, ChatResponse, SseChunk wire types from Phase 30 — can be reused or adapted for the gateway response format
- `blufio-core::ProviderAdapter` trait with complete() and streaming — provider routing already works
- `blufio-config::ProvidersConfig` with `default` field and per-provider config structs (OpenAI, Ollama, OpenRouter, Gemini)
- Existing SSE infrastructure in `blufio-gateway::sse` (currently Blufio-native format)

### Established Patterns
- Gateway uses `GatewayState` with `inbound_tx: mpsc::Sender<InboundMessage>` for routing to agent loop
- Device keypair auth via `auth_middleware` in gateway
- All provider crates follow types.rs + client.rs + stream/sse.rs + lib.rs pattern
- `ProviderResponse` (internal) is separate from provider wire types (already enforced in Phase 30)

### Integration Points
- New routes added to `blufio-gateway/src/server.rs` router
- New handler modules in `blufio-gateway/src/handlers.rs` or new files
- Provider resolution needs access to all 4 provider crates + config
- Tool registry access via `blufio-skill` (WASM) and `blufio-mcp-client` (MCP tools)
- Session system for /v1/responses state management

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 31-openai-compatible-gateway-api*
*Context gathered: 2026-03-05*
