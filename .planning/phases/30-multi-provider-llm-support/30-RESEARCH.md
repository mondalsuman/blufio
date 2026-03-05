# Phase 30: Multi-Provider LLM Support - Research

**Completed:** 2026-03-05
**Status:** Ready for planning

## Phase Goal

Users can select OpenAI, Ollama, OpenRouter, or Gemini as their LLM backend with streaming and tool calling. Each provider implements the existing `ProviderAdapter` trait from blufio-core.

## Existing Architecture Analysis

### ProviderAdapter Trait (`blufio-core/src/traits/provider.rs`)
Two methods: `complete(ProviderRequest) -> ProviderResponse` and `stream(ProviderRequest) -> Stream<ProviderStreamChunk>`. All new providers must implement both.

### PluginAdapter Base Trait
Five methods: `name()`, `version()`, `adapter_type()`, `health_check()`, `shutdown()`. All providers must return `AdapterType::Provider`.

### Provider-Agnostic Types (`blufio-core/src/types.rs`)
- `ProviderRequest`: model, system_prompt, system_blocks, messages (Vec<ProviderMessage>), max_tokens, stream, tools (Option<Vec<ToolDefinition>>)
- `ProviderResponse`: id, content, model, stop_reason, usage (TokenUsage)
- `ProviderStreamChunk`: event_type (StreamEventType), text, usage, error, tool_use (ToolUseData), stop_reason
- `ToolDefinition`: name, description, input_schema (serde_json::Value)
- `ContentBlock`: Text, Image, ToolUse, ToolResult variants
- `StreamEventType`: MessageStart, ContentBlockStart/Delta/Stop, MessageDelta, MessageStop, Ping, Error

### Reference Implementation Pattern (`blufio-anthropic`)
- Crate structure: `lib.rs` (provider + trait impls), `client.rs` (HTTP client), `sse.rs` (stream parser), `types.rs` (wire format types)
- Dependencies: reqwest, serde/serde_json, async-trait, semver, tokio, tracing, futures, eventsource-stream
- API key resolution: config value first, then environment variable
- System prompt: file > inline > default pattern
- Streaming: stateful accumulator for tool_use JSON across deltas
- Retry: transient errors (429, 500, 503) retried once after 1s delay
- Security: SSRF-safe resolver + TLS 1.2+ minimum via SecurityConfig
- Test support: wiremock for HTTP mocking, `with_base_url()` for test client

### Config Structure (`blufio-config/src/model.rs`)
- `BlufioConfig.anthropic: AnthropicConfig` -- current Anthropic config
- `BlufioConfig.providers: ProvidersConfig` -- currently only holds `custom: HashMap<String, CustomProviderConfig>`
- Need to add: `providers.default` field, per-provider config sections (openai, ollama, openrouter, gemini)
- `deny_unknown_fields` on all config structs -- new fields require struct updates

### Integration Points
- Plugin registry (`blufio-plugin`) -- providers register as `AdapterType::Provider`
- Cost ledger (`blufio-cost/src/ledger.rs`) -- receives `TokenUsage` from provider responses
- Model routing (`blufio-config/src/model.rs` RoutingConfig) -- needs bypass when non-Anthropic provider is active
- Agent session (`blufio-agent/src/session.rs`) -- calls provider via `ProviderAdapter` trait object

---

## Provider Wire Format Research

### 1. OpenAI Chat Completions API

**Endpoint:** `POST https://api.openai.com/v1/chat/completions`

**Request:**
```json
{
  "model": "gpt-4o",
  "messages": [
    {"role": "system", "content": "system prompt"},
    {"role": "user", "content": "user message"},
    {"role": "assistant", "content": "...", "tool_calls": [...]},
    {"role": "tool", "tool_call_id": "call_abc", "content": "result"}
  ],
  "max_completion_tokens": 4096,
  "stream": true,
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "tool_name",
        "description": "description",
        "parameters": { "type": "object", "properties": {...} }
      }
    }
  ],
  "response_format": {"type": "json_object"}
}
```

**Key Differences from Anthropic:**
- System prompt goes in messages array (role: "system"), not separate field
- Tool definitions wrapped in `{"type": "function", "function": {...}}`
- Uses `max_completion_tokens` (newer) or `max_tokens`
- Tool results use role "tool" with `tool_call_id`, not ContentBlock::ToolResult
- Vision: images in content array as `{"type": "image_url", "image_url": {"url": "data:..."}}`
- Stop reason: `finish_reason` not `stop_reason` ("stop", "length", "tool_calls")
- `base_url` configurable for Azure OpenAI, Together, Fireworks compatibility

**Streaming SSE Format:**
```
data: {"id":"chatcmpl-xxx","choices":[{"delta":{"role":"assistant","content":"Hi"},"finish_reason":null}]}
data: {"id":"chatcmpl-xxx","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc","function":{"name":"fn","arguments":"..."}}]},"finish_reason":null}]}
data: {"id":"chatcmpl-xxx","choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":20}}
data: [DONE]
```

**Token Usage:** `usage.prompt_tokens`, `usage.completion_tokens`, `usage.total_tokens`

### 2. Ollama Native /api/chat

**Endpoint:** `POST http://localhost:11434/api/chat`
**Tags Endpoint:** `GET http://localhost:11434/api/tags`

**Request:**
```json
{
  "model": "llama3.2",
  "messages": [
    {"role": "system", "content": "system prompt"},
    {"role": "user", "content": "user message"},
    {"role": "assistant", "content": "", "tool_calls": [...]},
    {"role": "tool", "content": "result"}
  ],
  "stream": true,
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "tool_name",
        "description": "description",
        "parameters": {"type": "object", "properties": {...}, "required": [...]}
      }
    }
  ]
}
```

**Key Differences:**
- NDJSON streaming (not SSE) -- each line is a JSON object
- No API key needed (local service)
- Token usage in timing fields: `prompt_eval_count`, `eval_count`
- `done: true/false` field indicates completion
- `done_reason: "stop"` on final chunk
- Tags endpoint returns `{"models": [{"name": "llama3.2", "modified_at": "...", ...}]}`
- No `id` field in responses -- generate one locally
- Tool format same as OpenAI (type: "function" wrapper)
- No image support in messages (model-dependent)

**Streaming NDJSON Format:**
```
{"model":"llama3.2","message":{"role":"assistant","content":"Hi"},"done":false}
{"model":"llama3.2","message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"fn","arguments":{"key":"val"}}}]},"done":false}
{"model":"llama3.2","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":26,"eval_count":15,"total_duration":123456789}
```

### 3. OpenRouter API

**Endpoint:** `POST https://openrouter.ai/api/v1/chat/completions`
**Auth Validation:** `GET https://openrouter.ai/api/v1/auth/key`

**Required Headers:**
- `Authorization: Bearer <OPENROUTER_API_KEY>`
- `Content-Type: application/json`

**Optional Headers:**
- `HTTP-Referer: <site_url>` -- identifies app for rankings
- `X-Title: <app_name>` -- app title for rankings (also accepts `X-OpenRouter-Title`)

**Request:** OpenAI-compatible format plus `provider` object:
```json
{
  "model": "anthropic/claude-sonnet-4",
  "messages": [...],
  "stream": true,
  "tools": [...],
  "provider": {
    "order": ["Anthropic", "Google"],
    "allow_fallbacks": true,
    "data_collection": "deny"
  }
}
```

**Provider Preferences:**
- `order`: Array of provider names for preference ordering
- `only`: Restrict to specific providers
- `ignore`: Exclude providers
- `allow_fallbacks`: Boolean to enable fallback to other providers
- `data_collection`: "deny" to prevent training data collection

**Key Differences from raw OpenAI:**
- Model IDs include provider prefix: `anthropic/claude-sonnet-4`, `openai/gpt-4o`
- Provider preference object for routing
- X-Title and HTTP-Referer headers for app identification
- SSE streaming format same as OpenAI
- No key validation endpoint confirmed -- use lightweight request to validate

### 4. Google Gemini API

**Endpoints:**
- `POST https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent`
- `POST https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent`

**Request:**
```json
{
  "contents": [
    {
      "role": "user",
      "parts": [{"text": "user message"}]
    }
  ],
  "systemInstruction": {
    "parts": [{"text": "system prompt"}]
  },
  "tools": [
    {
      "functionDeclarations": [
        {
          "name": "tool_name",
          "description": "description",
          "parameters": {"type": "object", "properties": {...}, "required": [...]}
        }
      ]
    }
  ],
  "generationConfig": {
    "temperature": 1.0,
    "maxOutputTokens": 4096,
    "topP": 0.8,
    "topK": 10
  }
}
```

**Key Differences:**
- Completely different wire format from OpenAI/Anthropic
- Messages use `contents` with `parts` array (not `messages` with `content` string)
- System prompt via `systemInstruction` (separate from contents)
- Tools use `functionDeclarations` (not OpenAI's type/function wrapper)
- Roles: "user" and "model" (not "assistant")
- Tool results sent as `functionResponse` part in user role
- Function calls returned as `functionCall` part: `{"functionCall": {"name": "fn", "args": {...}}}`
- Auth via API key in query string (`?key=API_KEY`) or `x-goog-api-key` header
- Streaming: NDJSON array chunks with `candidates[].content.parts`
- Usage: `usageMetadata.promptTokenCount`, `candidatesTokenCount`, `totalTokenCount`
- No cache token fields
- Vision: inline image data in `parts` as `{"inlineData": {"mimeType": "...", "data": "base64..."}}`

**Streaming Response:**
Each chunk is a `GenerateContentResponse`:
```json
{
  "candidates": [{
    "content": {"role": "model", "parts": [{"text": "chunk"}]},
    "finishReason": "STOP",
    "safetyRatings": [...]
  }],
  "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 5, "totalTokenCount": 15}
}
```

---

## Configuration Design

### ProvidersConfig Extension

```toml
[providers]
default = "anthropic"    # "anthropic" | "openai" | "ollama" | "openrouter" | "gemini"

[providers.openai]
api_key = ""              # or OPENAI_API_KEY env var
default_model = "gpt-4o"
base_url = "https://api.openai.com/v1"   # Override for Azure, Together, etc.

[providers.ollama]
base_url = "http://localhost:11434"
default_model = "llama3.2"               # Required -- no auto-pick

[providers.openrouter]
api_key = ""              # or OPENROUTER_API_KEY env var
default_model = "anthropic/claude-sonnet-4"
x_title = "Blufio"
http_referer = ""
provider_order = ["Anthropic", "Google"]

[providers.gemini]
api_key = ""              # or GEMINI_API_KEY env var
default_model = "gemini-2.0-flash"
```

### Routing Bypass Logic
When `providers.default != "anthropic"`, model routing (simple/standard/complex tiers) is disabled. The provider's `default_model` is always used. This is checked in the agent session loop before calling `classify_complexity()`.

---

## Crate Architecture Decision

**One crate per provider** (matching existing `blufio-anthropic` pattern):
- `blufio-openai` -- OpenAI + Azure/compatible endpoints
- `blufio-ollama` -- Ollama native API
- `blufio-openrouter` -- OpenRouter with provider preferences
- `blufio-gemini` -- Google Gemini native API

**Rationale:**
- Matches existing project structure (separate `blufio-anthropic` crate)
- Feature-flag control: each provider crate is optional
- Independent dependency trees (e.g., Gemini doesn't need eventsource-stream)
- Clear ownership and test isolation

---

## Shared Patterns Across Providers

### HTTP Client Construction
All providers follow the same pattern from AnthropicClient:
- reqwest::Client with default headers
- SSRF-safe resolver + TLS 1.2+ from SecurityConfig
- Configurable base URL (test + production)
- Retry on transient errors (429, 500, 503)

### API Key Resolution
Config value first, then environment variable fallback. Pattern from `resolve_api_key()` in blufio-anthropic.

### Stream Mapping
Each provider maps its streaming format to `ProviderStreamChunk`:
- OpenAI SSE -> ProviderStreamChunk (similar to Anthropic SSE mapping)
- Ollama NDJSON -> ProviderStreamChunk (line-by-line JSON parsing)
- OpenRouter SSE -> ProviderStreamChunk (identical to OpenAI)
- Gemini JSON array -> ProviderStreamChunk (chunk-by-chunk parsing)

### Tool Call Accumulation
OpenAI and OpenRouter stream tool calls incrementally (function name + arguments in deltas). Need stateful accumulator similar to Anthropic's `tool_use_blocks` HashMap pattern. Ollama sends complete tool_calls in one chunk. Gemini sends complete functionCall parts.

### ContentBlock Conversion
Each provider maps `ContentBlock` variants to its wire format:
- Text -> provider text format
- Image -> provider image format (or skip if unsupported)
- ToolUse -> provider tool call format
- ToolResult -> provider tool result format

---

## Risk Areas

1. **Ollama NDJSON parsing**: Not SSE -- cannot use eventsource-stream. Need line-delimited JSON parser over reqwest byte stream.
2. **Gemini streaming format**: Array-based response, not SSE. Similar challenge to Ollama.
3. **Tool call streaming across providers**: Different accumulation patterns. OpenAI/OpenRouter stream partial function args. Ollama/Gemini send complete calls.
4. **Vision content mapping**: Each provider handles images differently. OpenAI uses image_url, Gemini uses inlineData, Ollama support is model-dependent.
5. **OpenRouter auth validation**: No confirmed lightweight auth endpoint. May need to handle at first request.

---

## Requirement Coverage Plan

| Requirement | Plan | Notes |
|-------------|------|-------|
| PROV-01 | 30-01 | OpenAI streaming + tool calling |
| PROV-02 | 30-01 | OpenAI vision + structured outputs (response_format) |
| PROV-03 | 30-01 | OpenAI configurable base_url |
| PROV-04 | 30-02 | Ollama native /api/chat |
| PROV-05 | 30-02 | Ollama /api/tags discovery |
| PROV-06 | 30-03 | OpenRouter streaming + headers |
| PROV-07 | 30-03 | OpenRouter provider fallback ordering |
| PROV-08 | 30-04 | Gemini native API format |
| PROV-09 | 30-04 | Gemini function calling -> ToolDefinition |

All 9 requirements mapped to 4 plans.

---

*Phase: 30-multi-provider-llm-support*
*Research completed: 2026-03-05*

## RESEARCH COMPLETE
