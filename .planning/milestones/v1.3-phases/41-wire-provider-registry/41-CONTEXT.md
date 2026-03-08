# Phase 41: Wire ProviderRegistry into Gateway - Context

**Gathered:** 2026-03-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Add Phase 30 provider crates (blufio-openai, blufio-ollama, blufio-openrouter, blufio-gemini) as binary dependencies, implement a concrete ProviderRegistry struct, wire it into GatewayState via serve.rs, and wire ToolRegistry into the gateway. Covers requirements API-01 through API-10 and PROV-01 through PROV-09 wiring.

</domain>

<decisions>
## Implementation Decisions

### Provider initialization strategy
- Eager initialization at startup during serve.rs boot sequence
- Config-required activation: provider only inits if its config section has meaningful values (e.g., api_key set for OpenAI, default_model set for Ollama)
- Providers behind feature flags, matching existing adapter pattern (`#[cfg(feature = "...")]`)

### Failure handling
- Non-default providers that fail to init: log warning, skip that provider, continue serving
- Default provider failure: crash the server (cannot serve without it)
- Ollama health check failure: soft warning, skip provider (don't require Ollama running at startup)
- OpenAI base_url: allow any URL without validation (supports Azure, Together, Fireworks per PROV-03)

### Registry placement
- Concrete ProviderRegistry struct lives in the binary crate (blufio/src/providers.rs or similar)
- Matches Phase 40 pattern where wiring logic lives in the binary, not library crates
- Avoids adding provider crate deps to blufio-gateway or blufio-core

### Default provider selection
- Use existing `config.providers.default` field (defaults to "anthropic")
- Registry's `default_provider()` returns this config value
- Anthropic included in the registry alongside OpenAI/Ollama/OpenRouter/Gemini (all providers available through gateway API)

### Model routing through gateway
- Provider prefix required: callers use "openai/gpt-4o", "ollama/llama3.2", "gemini/gemini-2.0-flash"
- Registry splits on "/" to resolve provider
- Unprefixed model names (e.g., just "gpt-4o") route to the default provider
- Consistent with existing ModelInfo.id convention ("openai/gpt-4o" format)

### list_models behavior
- GET /v1/models aggregates models from ALL initialized providers
- Each model prefixed with provider name in ModelInfo.id
- Ollama models auto-discovered via /api/tags at startup (not live per-request)
- Cloud providers return static model lists (config-based or hardcoded known models)

### ToolRegistry wiring
- Wire ToolRegistry into gateway in this phase (not deferred to Phase 42)
- ToolRegistry already created in serve.rs for agent loop; pass via gateway.set_tools()
- Also wire api_tools_allowlist from GatewayConfig
- Closes API-09/API-10 wiring

### Testing
- Dual constructors for concrete ProviderRegistry: from_config() for production, from_providers() for tests
- from_providers() accepts pre-built provider map + default name, enables unit tests without API keys

### Claude's Discretion
- Exact module organization within binary crate (providers.rs vs inline in serve.rs)
- Static model lists for cloud providers (OpenAI, OpenRouter, Gemini)
- Feature flag naming conventions for new provider features
- Error message formatting for provider init failures

</decisions>

<specifics>
## Specific Ideas

No specific requirements -- open to standard approaches. Decisions follow the established wiring patterns from Phase 40 and the existing feature-gated adapter initialization in serve.rs.

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ProviderRegistry` trait: blufio-core/src/traits/provider_registry.rs -- get_provider(), default_provider(), list_models()
- `GatewayChannel::set_providers()`: setter already exists, accepts Arc<dyn ProviderRegistry>
- `GatewayChannel::set_tools()`: setter already exists, accepts Arc<RwLock<ToolRegistry>>
- `GatewayState.providers`: Option<Arc<dyn ProviderRegistry>> field ready to populate
- `GatewayState.tools`: Option<Arc<RwLock<ToolRegistry>>> field ready to populate
- `config.providers.default`: String field in BlufioConfig, defaults to "anthropic"
- All four provider crates implement ProviderAdapter trait (confirmed for OpenAI, Ollama, OpenRouter, Gemini, and Anthropic)
- `OllamaProvider::list_local_models()`: calls /api/tags for model discovery

### Established Patterns
- Feature-gated initialization: `#[cfg(feature = "...")]` blocks in serve.rs for each adapter
- Arc sharing: storage, vault, context engine, cost ledger all shared via Arc
- Provider init: each provider takes &BlufioConfig, resolves API key from config then env var
- Error pattern: `BlufioError::Config(...)` for configuration errors, `BlufioError::Provider(...)` for runtime errors

### Integration Points
- serve.rs startup: create concrete ProviderRegistry after config load, before gateway connect()
- blufio/Cargo.toml: add blufio-openai, blufio-ollama, blufio-openrouter, blufio-gemini deps
- GatewayChannel: call set_providers() and set_tools() before connect()
- ToolRegistry: already created in serve.rs, just needs Arc wrapping and passing to gateway

</code_context>

<deferred>
## Deferred Ideas

None -- discussion stayed within phase scope.

</deferred>

---

*Phase: 41-wire-provider-registry*
*Context gathered: 2026-03-07*
