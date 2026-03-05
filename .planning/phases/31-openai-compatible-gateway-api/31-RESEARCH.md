# Phase 31: OpenAI-Compatible Gateway API - Research

**Researched:** 2026-03-05
**Status:** Complete

## Executive Summary

Phase 31 adds OpenAI-compatible API endpoints to the existing `blufio-gateway` axum server, enabling external callers to use Blufio as a drop-in OpenAI-compatible server. The phase covers four endpoint groups: `/v1/chat/completions`, `/v1/models`, `/v1/responses`, `/v1/tools` + `/v1/tools/invoke`.

## Existing Infrastructure

### Gateway Architecture
- **`blufio-gateway`** is an axum-based HTTP/WebSocket server (`crates/blufio-gateway/src/`)
- Routes defined in `server.rs` using `Router::new().route()` pattern
- Handlers live in `handlers.rs` with request/response types defined inline
- SSE streaming exists in `sse.rs` using `axum::response::sse::{Event, Sse}` with `futures::stream`
- Auth middleware in `auth.rs` supports bearer token and Ed25519 keypair verification
- `GatewayState` holds `inbound_tx`, `response_map`, `ws_senders`, `auth`, `health`, `storage`
- Currently serves: `POST /v1/messages`, `GET /v1/sessions`, `GET /v1/health`, `GET /ws`

### Provider System
- **`ProviderAdapter`** trait in `blufio-core::traits::provider` with `complete()` and `stream()` methods
- **`ProviderRequest`** contains model, system_prompt, messages, max_tokens, stream, tools
- **`ProviderResponse`** (internal) uses `stop_reason` (not `finish_reason`)
- **`ProviderStreamChunk`** with `StreamEventType` enum for streaming events
- **`ToolDefinition`** is provider-agnostic: `{name, description, input_schema}`
- Provider crates: `blufio-openai`, `blufio-ollama`, `blufio-openrouter`, `blufio-gemini`
- Each provider maps internal `ProviderRequest` to its wire format and back

### OpenAI Wire Types (existing in blufio-openai)
- `ChatRequest`, `ChatResponse`, `SseChunk` types already exist for the OpenAI *client*
- These are *outbound* types (Blufio calling OpenAI), not *inbound* (external callers calling Blufio)
- Key difference: Gateway needs to *receive* `ChatRequest`-like requests and *produce* `ChatResponse`-like responses
- The existing `blufio-openai::types` can serve as a reference but need **separate gateway wire types** to avoid coupling

### Config System
- `ProvidersConfig` in `blufio-config::model` has `default` field (default: "anthropic")
- Per-provider configs: `OpenAIConfig`, `OllamaConfig`, `OpenRouterConfig`, `GeminiConfig`
- `CustomProviderConfig` for third-party providers via TOML
- No model alias system exists yet — this needs to be added

### Tool Registry
- `ToolRegistry` in `blufio-skill::tool` with `get()`, `list()`, `tool_definitions()` methods
- Built-in tools: bash, http, file
- Supports namespaced tools via `register_namespaced()`
- `tool_definitions()` returns `Vec<ToolDefinition>` — exactly what `/v1/tools` needs
- Tool execution via `Tool::execute()` trait method returns `ToolOutput { content, is_error }`

## Design Decisions

### 1. Wire Type Separation (API-06)

**Decision:** Create new `OpenAiGatewayRequest` / `OpenAiGatewayResponse` types in blufio-gateway, completely separate from `blufio-openai::types`.

**Rationale:**
- `blufio-openai::types::ChatResponse` uses `Deserialize` (client receives from OpenAI API)
- Gateway response types need `Serialize` (gateway sends to external callers)
- Fields differ: gateway adds `x_provider`, `x_cost`, `x_latency_ms`
- Finish reason mapping is reversed: internally `end_turn` -> externally `stop`
- No Anthropic-specific field names (`stop_reason`) in external types

**Implementation:** A new `openai_compat` module in `blufio-gateway` with:
- `GatewayCompletionRequest` (Deserialize) — incoming request from OpenAI SDK clients
- `GatewayCompletionResponse` (Serialize) — outgoing response matching OpenAI format
- `GatewaySseChunk` (Serialize) — streaming delta chunks
- `GatewayErrorResponse` — OpenAI error shape with extended fields

### 2. Model String Resolution

**Decision:** Support `provider/model` format with fallback to `providers.default`.

**Flow:**
1. Parse `model` field from request
2. If contains `/`: split into `(provider, model_name)` — e.g., `openai/gpt-4o`
3. If bare name (no `/`): use `providers.default` config value as provider
4. Look up provider adapter, set model on `ProviderRequest`
5. For aliases: defer to Phase 32+ (not in scope for this phase per CONTEXT.md discussion)

### 3. Provider Adapter Access

**Problem:** Current `GatewayState` doesn't hold provider adapters. The gateway routes inbound messages via `inbound_tx` channel to the agent loop. But `/v1/chat/completions` needs direct provider access.

**Decision:** Add a `ProviderRegistry` (new type or map) to `GatewayState`:
```rust
pub providers: Arc<dyn ProviderRegistry + Send + Sync>,
```

Where `ProviderRegistry` provides:
- `get_provider(name: &str) -> Option<Arc<dyn ProviderAdapter>>`
- `list_models() -> Vec<ModelInfo>`
- `default_provider() -> &str`

This is a new trait/struct that wraps the initialized provider adapters.

### 4. SSE Streaming for Chat Completions

**Decision:** Use axum's native SSE support with `data: [DONE]` termination.

**Format:**
```
data: {"id":"chatcmpl-xxx","choices":[{"delta":{"content":"Hello"},"index":0}],"model":"gpt-4o"}

data: {"id":"chatcmpl-xxx","choices":[{"delta":{},"finish_reason":"stop","index":0}],"usage":{...},"model":"gpt-4o"}

data: [DONE]
```

**Implementation:**
1. Create `ProviderRequest` from incoming `GatewayCompletionRequest`
2. Call `provider.stream()` to get `Stream<Item = ProviderStreamChunk>`
3. Map each `ProviderStreamChunk` to `GatewaySseChunk` (reversing the stop_reason mapping)
4. Emit as SSE events with `data:` prefix
5. End with `data: [DONE]\n\n`

### 5. OpenResponses API (/v1/responses)

**Research: What does the OpenAI Agents SDK need?**

The OpenAI Agents SDK (Python) uses the Responses API with these minimal event types:
- `response.created` — response object with metadata
- `response.output_item.added` — new output item started
- `response.content_part.added` — content part started
- `response.output_text.delta` — text content delta
- `response.output_text.done` — text content complete
- `response.content_part.done` — content part complete
- `response.output_item.done` — output item complete
- `response.completed` — response fully complete
- `response.failed` — error occurred

For tool calling:
- `response.function_call_arguments.delta` — partial function args
- `response.function_call_arguments.done` — complete function args

**Decision:** Implement the core event set above. Multi-turn via `previous_response_id` mapped to Blufio session IDs. Streaming only (no store/async mode).

### 6. Tools API

**GET /v1/tools:**
- Returns tools from `ToolRegistry` in OpenAI function schema format
- Extended with `source` (builtin/wasm/mcp), `version`, `required_permissions`
- Optional `?source=builtin` or `?source=mcp` filter

**POST /v1/tools/invoke:**
- Direct tool execution bypassing LLM
- Request: `{ "name": "bash", "input": {"command": "echo hello"} }`
- Response: `{ "name": "bash", "output": "hello\n", "is_error": false }`
- Tool allowlist from config (not all tools externally callable)

### 7. Error Mapping

OpenAI error format:
```json
{
  "error": {
    "message": "...",
    "type": "invalid_request_error",
    "param": null,
    "code": "invalid_api_key"
  }
}
```

Extended with `provider` and `retry_after` fields.

**Error type taxonomy:**
- `invalid_request_error` — bad request body, unsupported params, invalid model
- `authentication_error` — invalid/missing API key
- `permission_denied` — tool not in allowlist
- `not_found` — model or tool not found
- `rate_limit_error` — rate limited (Phase 32, but error type defined now)
- `server_error` — internal errors, provider failures
- `timeout_error` — provider response timeout

### 8. GET /v1/models

Returns merged list of available models across all configured providers.

Response format:
```json
{
  "object": "list",
  "data": [
    {
      "id": "openai/gpt-4o",
      "object": "model",
      "created": 0,
      "owned_by": "openai"
    }
  ]
}
```

- Ollama models discovered via `/api/tags` (already in OllamaClient)
- Other providers: list from config (known model names)
- Optional `?provider=ollama` filter

## File Plan

### New files in blufio-gateway:
- `src/openai_compat/mod.rs` — module declaration
- `src/openai_compat/types.rs` — Gateway wire types (separate from blufio-openai)
- `src/openai_compat/handlers.rs` — Route handlers for /v1/chat/completions, /v1/models
- `src/openai_compat/stream.rs` — SSE streaming logic for chat completions
- `src/openai_compat/responses.rs` — /v1/responses handler and event types
- `src/openai_compat/tools.rs` — /v1/tools and /v1/tools/invoke handlers

### Modified files:
- `blufio-gateway/src/lib.rs` — Add `openai_compat` module, extend GatewayState
- `blufio-gateway/src/server.rs` — Add new routes, extend GatewayState with providers/tools
- `blufio-gateway/Cargo.toml` — Add blufio-config dependency (for ProvidersConfig types)
- `blufio-config/src/model.rs` — Add `api_tools_allowlist` to GatewayConfig

### Provider Registry:
- New `ProviderRegistry` trait/struct — either in blufio-core or blufio-gateway
- Maps provider names to `Arc<dyn ProviderAdapter>` instances
- Implements model listing

## Requirement Coverage

| Req | Coverage | Plan |
|-----|----------|------|
| API-01 | POST /v1/chat/completions handler | Plan 01 |
| API-02 | SSE streaming with data: [DONE] | Plan 01 |
| API-03 | tools + tool_choice in request | Plan 01 |
| API-04 | response_format (JSON mode) | Plan 01 |
| API-05 | usage in response + stream_options | Plan 01 |
| API-06 | Separate OpenAI wire types | Plan 01 |
| API-07 | POST /v1/responses handler | Plan 02 |
| API-08 | Semantic streaming events | Plan 02 |
| API-09 | POST /v1/tools/invoke | Plan 03 |
| API-10 | GET /v1/tools | Plan 03 |

## Risk Assessment

1. **Provider registry initialization** — Need to wire provider adapters into GatewayState at startup. This touches the main binary crate's initialization code.
2. **Model string parsing** — Edge cases with custom providers whose names contain `/`. Mitigation: well-known provider prefixes.
3. **OpenResponses complexity** — The Responses API has many event types. Scoping to the core set keeps it manageable.
4. **Tool allowlist** — Config-based allowlist needs a new field. Default: empty (no tools externally accessible) for security.

---

*Phase: 31-openai-compatible-gateway-api*
*Research completed: 2026-03-05*
