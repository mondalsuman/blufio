# Phase 30: Multi-Provider LLM Support - Context

**Gathered:** 2026-03-05
**Status:** Ready for planning

<domain>
## Phase Boundary

Users can select OpenAI, Ollama, OpenRouter, or Gemini as their LLM backend with streaming and tool calling. Each provider implements the existing `ProviderAdapter` trait. Configuration, provider switching, and cost tracking are included. Cross-provider model routing and per-agent provider overrides are out of scope.

</domain>

<decisions>
## Implementation Decisions

### Provider switching & defaults
- Anthropic remains the default provider out of the box — users opt-in to others explicitly
- No automatic fallback when a provider is down — fail with a clear error message. Respect the user's provider choice; avoid surprise API charges
- Unified config namespace: move to `[providers.openai]`, `[providers.ollama]`, etc. Migrate the existing `[anthropic]` section to `[providers.anthropic]`
- Add `providers.default = "anthropic"` field to `ProvidersConfig`
- Global provider selection only — no per-agent provider overrides for now

### Model routing across providers
- Model routing (simple/standard/complex tiers) stays Anthropic-only for now
- When `providers.default != "anthropic"`, routing is disabled — the provider's `default_model` is always used
- Each provider has a sensible hardcoded default model (e.g., `gpt-4o` for OpenAI, `gemini-2.0-flash` for Gemini); user can override via `default_model` in provider config
- Cost tracking extends to all providers — all providers report `TokenUsage` through existing types, and the cost ledger tracks usage from all providers

### Ollama local discovery
- Auto-detect Ollama at `http://localhost:11434` by default; user can override `base_url` in config
- Error at startup if Ollama is selected but not reachable — fail fast with a clear message ("Ollama not reachable at http://localhost:11434. Is it running?")
- Use Ollama's native `/api/chat` endpoint, not the OpenAI compatibility shim — matches success criteria requirement
- Require explicit `default_model` for Ollama — no auto-picking from `/api/tags`. Ollama instances can have many models; auto-picking could choose something inappropriate

### OpenRouter provider preferences
- `X-Title` defaults to the agent name from config (e.g., "Blufio"); `HTTP-Referer` defaults to a reasonable value. Both overridable in config
- Provider fallback ordering is configurable via `providers.openrouter.provider_order = ["anthropic", "google"]` — maps to OpenRouter's provider preferences API parameter
- Use full OpenRouter model IDs directly (e.g., `"anthropic/claude-sonnet-4"`) — no simplified aliases
- Validate OpenRouter API key at startup with a lightweight API call (e.g., `/api/v1/auth/key`)

### Claude's Discretion
- Crate architecture decisions (one crate per provider vs. multi-provider crate)
- Wire format mapping details for each provider's unique API format
- SSE/streaming event format mapping to `ProviderStreamChunk`
- Gemini-specific function calling mapping to `ToolDefinition`
- Vision/image content handling per provider
- Error message formatting and retry logic within each provider
- Default model choices for each provider (reasonable current defaults)

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. Follow the existing `blufio-anthropic` implementation as a reference pattern for new provider crates.

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ProviderAdapter` trait (`blufio-core/src/traits/provider.rs`): `complete()` + `stream()` — all new providers implement this
- `ToolDefinition` struct (`blufio-core/src/types.rs`): Provider-agnostic tool definition with `name`, `description`, `input_schema`
- `ProviderRequest/ProviderResponse/ProviderStreamChunk` types: Provider-agnostic request/response types all providers must map to/from
- `MockProvider` (`blufio-test-utils/src/mock_provider.rs`): Test utility showing minimal ProviderAdapter implementation pattern
- `PluginAdapter` base trait: `name()`, `version()`, `adapter_type()`, `health_check()`, `shutdown()`
- `AnthropicClient` + SSE parser (`blufio-anthropic/src/client.rs`, `sse.rs`): Reference implementation for HTTP client + streaming

### Established Patterns
- Provider implements both `PluginAdapter` + `ProviderAdapter` traits
- API key resolution: config value first, then environment variable fallback
- System prompt: file > inline > default pattern
- Streaming: SSE event stream mapped to `ProviderStreamChunk` via stateful accumulator (handles tool_use JSON across deltas)
- `ContentBlock` → provider-specific wire format conversion in provider crate
- `#[serde(deny_unknown_fields)]` on all config structs for validation

### Integration Points
- `BlufioConfig.providers: ProvidersConfig` — needs `default` field added
- `BlufioConfig.anthropic: AnthropicConfig` — to be migrated to `providers.anthropic` namespace
- Plugin registry (`blufio-plugin`) — new providers register as `AdapterType::Provider`
- Cost ledger (`blufio-cost/src/ledger.rs`) — receives `TokenUsage` from provider responses
- Agent session loop (`blufio-agent/src/session.rs`) — calls provider via `ProviderAdapter` trait object
- Model routing (`blufio-config/src/model.rs` + `RoutingConfig`) — needs bypass when non-Anthropic provider is active

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 30-multi-provider-llm-support*
*Context gathered: 2026-03-05*
