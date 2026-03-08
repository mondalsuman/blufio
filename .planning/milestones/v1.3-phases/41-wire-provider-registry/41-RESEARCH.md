# Phase 41: Wire ProviderRegistry into Gateway - Research

**Researched:** 2026-03-07
**Domain:** Rust binary-crate wiring -- concrete ProviderRegistry + ToolRegistry into GatewayState
**Confidence:** HIGH

## Summary

Phase 41 is a wiring phase: it connects four existing provider crates (blufio-openai, blufio-ollama, blufio-openrouter, blufio-gemini) plus the existing blufio-anthropic into a concrete `ProviderRegistry` struct, then injects it into `GatewayChannel` via the already-existing `set_providers()` setter. Additionally, it wires the existing `ToolRegistry` into the gateway via `set_tools()` and passes the `api_tools_allowlist` from `GatewayConfig`.

All the trait definitions, state fields, setter methods, and provider crate implementations already exist. The work is purely: (1) add provider crate dependencies to the binary's Cargo.toml with feature flags, (2) implement a concrete `ProviderRegistry` struct in the binary crate, (3) initialize providers eagerly during `serve.rs` boot using config-required activation, and (4) call the gateway setters before `connect()`.

**Primary recommendation:** Follow the established Phase 40 pattern -- wiring logic lives in the binary crate (`crates/blufio/src/`), not in library crates. Use feature-gated `#[cfg(feature = "...")]` blocks matching the existing adapter initialization pattern in `serve.rs`.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
- Eager initialization at startup during serve.rs boot sequence
- Config-required activation: provider only inits if its config section has meaningful values (e.g., api_key set for OpenAI, default_model set for Ollama)
- Providers behind feature flags, matching existing adapter pattern (`#[cfg(feature = "...")]`)
- Non-default providers that fail to init: log warning, skip that provider, continue serving
- Default provider failure: crash the server (cannot serve without it)
- Ollama health check failure: soft warning, skip provider (don't require Ollama running at startup)
- OpenAI base_url: allow any URL without validation (supports Azure, Together, Fireworks per PROV-03)
- Concrete ProviderRegistry struct lives in the binary crate (blufio/src/providers.rs or similar)
- Matches Phase 40 pattern where wiring logic lives in the binary, not library crates
- Avoids adding provider crate deps to blufio-gateway or blufio-core
- Use existing `config.providers.default` field (defaults to "anthropic")
- Registry's `default_provider()` returns this config value
- Anthropic included in the registry alongside OpenAI/Ollama/OpenRouter/Gemini
- Provider prefix required: callers use "openai/gpt-4o", "ollama/llama3.2", "gemini/gemini-2.0-flash"
- Registry splits on "/" to resolve provider
- Unprefixed model names route to the default provider
- GET /v1/models aggregates models from ALL initialized providers
- Each model prefixed with provider name in ModelInfo.id
- Ollama models auto-discovered via /api/tags at startup (not live per-request)
- Cloud providers return static model lists (config-based or hardcoded known models)
- Wire ToolRegistry into gateway in this phase (not deferred)
- ToolRegistry already created in serve.rs for agent loop; pass via gateway.set_tools()
- Also wire api_tools_allowlist from GatewayConfig
- Dual constructors: from_config() for production, from_providers() for tests
- from_providers() accepts pre-built provider map + default name

### Claude's Discretion
- Exact module organization within binary crate (providers.rs vs inline in serve.rs)
- Static model lists for cloud providers (OpenAI, OpenRouter, Gemini)
- Feature flag naming conventions for new provider features
- Error message formatting for provider init failures

### Deferred Ideas (OUT OF SCOPE)
None.

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| API-01 | OpenAI-compatible chat completions via POST /v1/chat/completions | ProviderRegistry enables handler to resolve provider by model prefix and dispatch |
| API-02 | SSE streaming responses | Providers already implement streaming; registry wiring makes them accessible |
| API-03 | Tool calling (tools + tool_choice) | Provider adapters already support tool calling; gateway needs providers wired in |
| API-04 | response_format (JSON mode) | Handled by individual provider adapters once registry routes to them |
| API-05 | Usage (token counts + cost) | Provider responses already include usage; registry enables routing |
| API-06 | OpenAI wire types separate from internal ProviderResponse | Already implemented in gateway openai_compat module; registry completes the pipeline |
| API-07 | OpenResponses POST /v1/responses | Responses handler needs providers from GatewayState.providers |
| API-08 | Responses streaming semantic events | Same as API-07; providers must be wired for streaming to work |
| API-09 | POST /v1/tools/invoke | Wiring ToolRegistry into GatewayState.tools enables this endpoint |
| API-10 | GET /v1/tools with JSON schemas | Wiring ToolRegistry + api_tools_allowlist enables tool listing |
| PROV-01 | OpenAI provider with streaming and tool calling | OpenAIProvider crate exists; wiring adds it to registry |
| PROV-02 | OpenAI vision and structured outputs | Already in OpenAIProvider; registry makes it accessible via gateway |
| PROV-03 | OpenAI configurable base_url | OpenAIConfig.base_url exists; no URL validation per decision |
| PROV-04 | Ollama native /api/chat | OllamaProvider crate exists; wiring adds it to registry |
| PROV-05 | Ollama auto-discover via /api/tags | OllamaProvider::list_local_models() exists; called at startup for model list |
| PROV-06 | OpenRouter with streaming and headers | OpenRouterProvider crate exists; wiring adds it to registry |
| PROV-07 | OpenRouter provider fallback ordering | Already in OpenRouterProvider; registry makes it accessible |
| PROV-08 | Gemini native API format | GeminiProvider crate exists; wiring adds it to registry |
| PROV-09 | Gemini function calling mapped to ToolDefinition | Already in GeminiProvider; registry makes it accessible |

</phase_requirements>

## Standard Stack

### Core (Already Exists -- No New Dependencies)

| Library | Version | Purpose | Status |
|---------|---------|---------|--------|
| blufio-openai | workspace | OpenAI ProviderAdapter | Exists, needs dep in binary Cargo.toml |
| blufio-ollama | workspace | Ollama ProviderAdapter | Exists, needs dep in binary Cargo.toml |
| blufio-openrouter | workspace | OpenRouter ProviderAdapter | Exists, needs dep in binary Cargo.toml |
| blufio-gemini | workspace | Gemini ProviderAdapter | Exists, needs dep in binary Cargo.toml |
| blufio-anthropic | workspace | Anthropic ProviderAdapter | Already in binary Cargo.toml |
| blufio-core | workspace | ProviderRegistry trait, ProviderAdapter trait, ModelInfo | Already in binary Cargo.toml |
| blufio-gateway | workspace | GatewayChannel, GatewayState | Already in binary Cargo.toml |
| blufio-skill | workspace | ToolRegistry | Already in binary Cargo.toml |
| blufio-config | workspace | BlufioConfig, ProvidersConfig | Already in binary Cargo.toml |

### No New External Dependencies

This phase adds zero new external crates. All dependencies are internal workspace crates.

## Architecture Patterns

### Recommended Module Structure

```
crates/blufio/src/
  providers.rs     # NEW -- concrete ProviderRegistry struct + from_config() + from_providers()
  serve.rs         # MODIFIED -- initialize ProviderRegistry, call gateway setters
  Cargo.toml       # MODIFIED -- add 4 provider crate optional deps + feature flags
```

### Pattern 1: Concrete ProviderRegistry (NEW file)

**What:** A struct holding a `HashMap<String, Arc<dyn ProviderAdapter + Send + Sync>>` and a `default: String`. Implements the `ProviderRegistry` trait from blufio-core.

**When to use:** This is the sole implementation needed for this phase.

```rust
// crates/blufio/src/providers.rs
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use blufio_core::error::BlufioError;
use blufio_core::traits::{ModelInfo, ProviderAdapter, ProviderRegistry};

pub struct ConcreteProviderRegistry {
    providers: HashMap<String, Arc<dyn ProviderAdapter + Send + Sync>>,
    default: String,
}

impl ConcreteProviderRegistry {
    /// Production constructor: reads config, initializes providers conditionally.
    pub async fn from_config(config: &BlufioConfig) -> Result<Self, BlufioError> { ... }

    /// Test constructor: accepts pre-built provider map.
    pub fn from_providers(
        providers: HashMap<String, Arc<dyn ProviderAdapter + Send + Sync>>,
        default: String,
    ) -> Self { ... }
}

#[async_trait]
impl ProviderRegistry for ConcreteProviderRegistry {
    fn get_provider(&self, name: &str) -> Option<Arc<dyn ProviderAdapter + Send + Sync>> {
        self.providers.get(name).cloned()
    }

    fn default_provider(&self) -> &str {
        &self.default
    }

    async fn list_models(&self, provider_filter: Option<&str>) -> Result<Vec<ModelInfo>, BlufioError> {
        // Iterate providers, aggregate ModelInfo with "provider/model" ids
    }
}
```

### Pattern 2: Config-Required Activation

**What:** Each provider only initializes if its config section has meaningful values. Not just "section exists" (since all sections have defaults), but specific activation signals.

**Activation signals per provider:**

| Provider | Activation Check | Config Field |
|----------|-----------------|--------------|
| anthropic | api_key non-empty (from config or ANTHROPIC_API_KEY env) | `config.anthropic.api_key` |
| openai | api_key non-empty (from config or OPENAI_API_KEY env) | `config.providers.openai.api_key` |
| ollama | base_url non-empty (defaults to http://localhost:11434) | `config.providers.ollama.base_url` |
| openrouter | api_key non-empty (from config or OPENROUTER_API_KEY env) | `config.providers.openrouter.api_key` |
| gemini | api_key non-empty (from config or GEMINI_API_KEY env) | `config.providers.gemini.api_key` |

**Note:** Ollama defaults to localhost, so it's "always configured" but the soft health check handles Ollama-not-running gracefully.

### Pattern 3: Feature-Gated Provider Init

**What:** Each provider init block is behind `#[cfg(feature = "provider_name")]` matching the existing adapter pattern.

```rust
// In from_config() or serve.rs:
#[cfg(feature = "openai")]
{
    if !config.providers.openai.api_key.is_empty() {
        match OpenAIProvider::new(config).await {
            Ok(p) => { providers.insert("openai".into(), Arc::new(p)); }
            Err(e) => {
                if config.providers.default == "openai" {
                    return Err(e);  // Default provider: crash
                }
                warn!(error = %e, "openai provider init failed, skipping");
            }
        }
    }
}
```

### Pattern 4: Model Prefix Routing

**What:** The `/v1/chat/completions` handler extracts provider from model string.

```rust
/// Split "openai/gpt-4o" into ("openai", "gpt-4o").
/// Unprefixed "gpt-4o" returns (default_provider, "gpt-4o").
fn resolve_model(model: &str, default: &str) -> (&str, &str) {
    match model.split_once('/') {
        Some((provider, model_name)) => (provider, model_name),
        None => (default, model),
    }
}
```

This logic may live in the ProviderRegistry impl or in the handler itself.

### Pattern 5: Gateway Wiring in serve.rs

**What:** After building the ProviderRegistry and before adding gateway to the multiplexer, call the setters.

```rust
// In serve.rs, within the #[cfg(feature = "gateway")] block:
let provider_registry = Arc::new(
    ConcreteProviderRegistry::from_config(&config).await?
);
gateway.set_providers(provider_registry).await;

// ToolRegistry already exists as `tool_registry` -- wrap in Arc<RwLock<>>
let tool_registry_arc = Arc::new(RwLock::new(tool_registry));
gateway.set_tools(tool_registry_arc.clone()).await;
gateway.set_api_tools_allowlist(config.gateway.api_tools_allowlist.clone());
```

### Anti-Patterns to Avoid

- **Adding provider deps to blufio-gateway:** Violated by design -- gateway uses trait objects, binary crate provides the concrete implementation.
- **Lazy initialization:** Decision explicitly requires eager init at startup.
- **Live Ollama model discovery per request:** Decision says startup-only discovery via /api/tags.
- **Validating OpenAI base_url:** Decision says allow any URL (supports Azure, Together, Fireworks).
- **Custom error types for wiring:** Use existing `BlufioError::Config(...)` and `BlufioError::Provider(...)`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Provider trait abstraction | Custom trait | `ProviderAdapter` from blufio-core | Already exists, all 5 providers implement it |
| Registry trait | Custom interface | `ProviderRegistry` from blufio-core | Already exists with get_provider/default_provider/list_models |
| Gateway state injection | Custom DI | `GatewayChannel::set_providers()` / `set_tools()` | Setters already exist |
| Model info serialization | Custom types | `ModelInfo` from blufio-core | Already has Serialize derive |
| Config parsing | Manual TOML | `ProvidersConfig` from blufio-config | Already exists with all provider sub-configs |

## Common Pitfalls

### Pitfall 1: ToolRegistry Ownership Conflict
**What goes wrong:** ToolRegistry is already created in serve.rs for the agent loop. Wrapping it in `Arc<RwLock<>>` for the gateway must happen before it's moved into the agent loop.
**Why it happens:** serve.rs creates ToolRegistry early, passes it around. If you wrap it in Arc after the agent takes ownership, you get a move error.
**How to avoid:** Wrap ToolRegistry in `Arc<RwLock<ToolRegistry>>` immediately after creation, then clone the Arc for both gateway and agent loop.
**Warning signs:** Compile error "value moved here" on tool_registry.

### Pitfall 2: Default Provider Not in Registry
**What goes wrong:** If the default provider (e.g., "anthropic") fails to init or its feature is disabled, but config.providers.default still points to it, the registry claims a default that doesn't exist.
**Why it happens:** Config defaults to "anthropic" but feature flags control what's compiled.
**How to avoid:** After building the registry, verify that `providers.contains_key(&config.providers.default)`. If not, crash with a clear error message.
**Warning signs:** Runtime panic when handler calls `default_provider()` and then `get_provider()` returns None.

### Pitfall 3: Anthropic Provider Already Initialized Separately
**What goes wrong:** serve.rs already initializes `AnthropicProvider` for the agent loop. If you initialize it again inside from_config(), you have duplicate instances.
**Why it happens:** The agent loop needs its own provider reference; the registry needs one too.
**How to avoid:** Either share the same Arc'd provider between agent loop and registry, or accept that two instances is fine (they're stateless HTTP clients). The cleaner approach is to build the registry first, then extract the anthropic provider Arc for the agent loop.
**Warning signs:** None functionally, but wasteful resource use.

### Pitfall 4: Feature Flag Names Must Match Cargo.toml
**What goes wrong:** Using `#[cfg(feature = "openai")]` but Cargo.toml feature is named differently.
**How to avoid:** Add new feature flags to Cargo.toml's `[features]` section matching exactly: `openai = ["dep:blufio-openai"]`, `ollama = ["dep:blufio-ollama"]`, etc. Add them to the `default` list.
**Warning signs:** Code silently excluded by cfg, providers never initialize.

### Pitfall 5: Ollama Init Blocking Startup
**What goes wrong:** OllamaProvider::new() might try a health check that times out if Ollama isn't running, blocking server startup for seconds.
**Why it happens:** Decision says soft warning on health check failure, but the provider's `new()` might eagerly connect.
**How to avoid:** Check OllamaProvider::new() behavior. If it does a health check, handle the error as a soft warning and still include the provider (models list will be empty). Or skip the health check at init time.
**Warning signs:** Slow startup when Ollama isn't running.

## Code Examples

### Provider Initialization Order in serve.rs

Based on the existing code at lines 311-321, the Anthropic provider is initialized like:

```rust
let provider = {
    let p = AnthropicProvider::new(&config).await.map_err(|e| {
        error!(error = %e, "failed to initialize Anthropic provider");
        e
    })?;
    info!("anthropic provider initialized");
    Arc::new(p)
};
```

All four provider crates follow the same pattern:
- `OpenAIProvider::new(&BlufioConfig) -> Result<Self, BlufioError>`
- `OllamaProvider::new(&BlufioConfig) -> Result<Self, BlufioError>`
- `OpenRouterProvider::new(&BlufioConfig) -> Result<Self, BlufioError>`
- `GeminiProvider::new(&BlufioConfig) -> Result<Self, BlufioError>`

### Gateway Setter Signatures (from blufio-gateway/src/lib.rs)

```rust
pub async fn set_providers(&self, providers: Arc<dyn ProviderRegistry + Send + Sync>) { ... }
pub async fn set_tools(&self, tools: Arc<RwLock<ToolRegistry>>) { ... }
pub fn set_api_tools_allowlist(&mut self, allowlist: Vec<String>) { ... }
```

Note: `set_api_tools_allowlist` takes `&mut self` (not async), while the other two are async.

### GatewayState Fields (from blufio-gateway/src/server.rs)

```rust
pub providers: Option<Arc<dyn ProviderRegistry + Send + Sync>>,
pub tools: Option<Arc<RwLock<ToolRegistry>>>,
pub api_tools_allowlist: Vec<String>,
```

All three fields exist and are populated from GatewayChannel's internal state during `connect()`.

### Cargo.toml Feature Additions Needed

```toml
# New features (add to [features] section)
openai = ["dep:blufio-openai"]
ollama = ["dep:blufio-ollama"]
openrouter = ["dep:blufio-openrouter"]
gemini = ["dep:blufio-gemini"]

# Update default to include new providers
default = ["telegram", "discord", "slack", "whatsapp", "signal", "irc", "matrix",
           "bridge", "anthropic", "sqlite", "onnx", "prometheus", "keypair",
           "gateway", "mcp-server", "mcp-client", "node",
           "openai", "ollama", "openrouter", "gemini"]

# New optional deps
blufio-openai = { path = "../blufio-openai", optional = true }
blufio-ollama = { path = "../blufio-ollama", optional = true }
blufio-openrouter = { path = "../blufio-openrouter", optional = true }
blufio-gemini = { path = "../blufio-gemini", optional = true }
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single Anthropic provider hardcoded | Multi-provider registry via trait | Phase 31 (gateway) | Enables API-01 through API-08 |
| Tools only for agent loop | Tools exposed via gateway API | Phase 31 (gateway) | Enables API-09, API-10 |
| Provider wiring in library crates | Binary crate owns wiring | Phase 40 pattern | Clean dependency direction |

## Open Questions

1. **Ollama provider activation signal**
   - What we know: OllamaConfig defaults base_url to "http://localhost:11434", meaning it's always "configured"
   - What's unclear: Should Ollama init be attempted always (relying on soft health check failure), or should there be an explicit `enabled` field?
   - Recommendation: Attempt init always since base_url defaults. The soft health check handles Ollama-not-running. This matches the decision that Ollama health check failure = soft warning, skip.

2. **Static model lists for cloud providers**
   - What we know: Decision says cloud providers use static/hardcoded model lists
   - What's unclear: Exact model names to include for OpenAI, OpenRouter, Gemini
   - Recommendation: Claude's discretion per CONTEXT.md. Include commonly-used models (gpt-4o, gpt-4o-mini, gpt-3.5-turbo for OpenAI; gemini-2.0-flash, gemini-1.5-pro for Gemini; pass-through for OpenRouter since it's a proxy).

3. **Anthropic provider sharing between agent loop and registry**
   - What we know: Agent loop already creates an AnthropicProvider. Registry also needs one.
   - What's unclear: Share same Arc instance or create separate?
   - Recommendation: Build the registry first with all providers including Anthropic, then clone the Arc for the agent loop. Avoids duplicate init and is cleaner.

## Validation Architecture

> nyquist_validation not explicitly set to false in config.json -- section included.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in #[cfg(test)] + tokio::test |
| Config file | Workspace Cargo.toml |
| Quick run command | `cargo test -p blufio --lib providers` |
| Full suite command | `cargo test -p blufio` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| WIRING-01 | ConcreteProviderRegistry::from_providers() constructs with mock providers | unit | `cargo test -p blufio --lib providers -x` | Wave 0 |
| WIRING-02 | get_provider() returns correct provider by name | unit | `cargo test -p blufio --lib providers -x` | Wave 0 |
| WIRING-03 | default_provider() returns config value | unit | `cargo test -p blufio --lib providers -x` | Wave 0 |
| WIRING-04 | Unprefixed model resolves to default provider | unit | `cargo test -p blufio --lib providers -x` | Wave 0 |
| WIRING-05 | Prefixed "openai/gpt-4o" resolves to openai provider | unit | `cargo test -p blufio --lib providers -x` | Wave 0 |
| WIRING-06 | list_models aggregates from all providers | unit | `cargo test -p blufio --lib providers -x` | Wave 0 |
| WIRING-07 | Feature compilation succeeds with all provider features | build | `cargo check -p blufio --all-features` | N/A |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio --lib providers -x`
- **Per wave merge:** `cargo test -p blufio`
- **Phase gate:** Full suite green before /gsd:verify-work

### Wave 0 Gaps
- [ ] `crates/blufio/src/providers.rs` -- new file with unit tests for registry
- [ ] Feature flags `openai`, `ollama`, `openrouter`, `gemini` in Cargo.toml

## Sources

### Primary (HIGH confidence)
- `crates/blufio-core/src/traits/provider_registry.rs` -- ProviderRegistry trait, ModelInfo struct (read directly)
- `crates/blufio-gateway/src/lib.rs` -- GatewayChannel setters: set_providers(), set_tools(), set_api_tools_allowlist() (read directly)
- `crates/blufio-gateway/src/server.rs` -- GatewayState struct with providers/tools/allowlist fields (read directly)
- `crates/blufio/Cargo.toml` -- Current binary crate dependencies and feature flags (read directly)
- `crates/blufio/src/serve.rs` -- Current initialization patterns, feature-gated blocks (read directly)
- `crates/blufio-config/src/model.rs` -- ProvidersConfig, GatewayConfig, per-provider config structs (read directly)
- All four provider crates (`blufio-openai`, `blufio-ollama`, `blufio-openrouter`, `blufio-gemini`) -- constructor signatures confirmed as `pub async fn new(&BlufioConfig) -> Result<Self, BlufioError>` (grepped directly)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all crates exist and were read directly from source
- Architecture: HIGH -- follows established Phase 40 wiring pattern, all setters/fields confirmed
- Pitfalls: HIGH -- derived from reading actual code structure and ownership patterns

**Research date:** 2026-03-07
**Valid until:** 2026-04-07 (stable internal architecture, no external dependency changes)
