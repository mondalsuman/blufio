---
phase: 03-agent-loop-telegram
plan: 01
subsystem: provider
tags: [anthropic, sse, streaming, reqwest, eventsource, llm, provider-adapter]

# Dependency graph
requires:
  - phase: 01-skeleton
    provides: "PluginAdapter trait, BlufioError, placeholder core types, workspace structure"
  - phase: 02-config-storage
    provides: "BlufioConfig with AnthropicConfig, StorageConfig, config loader"
provides:
  - "AnthropicProvider implementing ProviderAdapter with complete() and stream()"
  - "SSE stream parser for Anthropic Messages API events"
  - "Real core types (InboundMessage, OutboundMessage, ProviderRequest, ProviderResponse, ProviderStreamChunk)"
  - "ChannelAdapter edit_message and send_typing default methods"
  - "AgentConfig system_prompt/system_prompt_file fields"
  - "AnthropicConfig max_tokens and api_version fields"
affects: [03-02, 03-03, 04-01, 04-02, 04-03, 05-01, 05-02, 05-03, 07-01]

# Tech tracking
tech-stack:
  added: [eventsource-stream 0.2, pin-project-lite 0.2, wiremock 0.6, futures 0.3, futures-core 0.3]
  patterns: [SSE streaming via eventsource-stream, transient error retry (429/500/503), API key resolution (config > env > error), system prompt loading priority (file > inline > default)]

key-files:
  created:
    - crates/blufio-anthropic/Cargo.toml
    - crates/blufio-anthropic/src/lib.rs
    - crates/blufio-anthropic/src/client.rs
    - crates/blufio-anthropic/src/types.rs
    - crates/blufio-anthropic/src/sse.rs
  modified:
    - crates/blufio-core/src/types.rs
    - crates/blufio-core/src/traits/provider.rs
    - crates/blufio-core/src/traits/channel.rs
    - crates/blufio-core/Cargo.toml
    - crates/blufio-config/src/model.rs
    - Cargo.toml

key-decisions:
  - "eventsource-stream 0.2 for SSE parsing with reqwest byte streams"
  - "ProviderAdapter::stream returns Pin<Box<dyn Stream>> not Iterator for async compatibility"
  - "Anthropic API key resolution: config.api_key > ANTHROPIC_API_KEY env var > error"
  - "System prompt loading: file > inline string > default template"
  - "Transient error retry: 1 retry after 1s delay on 429/500/503"
  - "CacheControlMarker::ephemeral() auto-applied on all Anthropic requests"

patterns-established:
  - "SSE streaming pattern: reqwest bytes_stream -> eventsource -> filter_map to typed events"
  - "Provider adapter pattern: convert ProviderRequest -> API-specific request, map response back"
  - "Retry pattern: for loop with max_retries, tokio::time::sleep between attempts"
  - "Config resolution: config field > env var > error, with clear error messages"

requirements-completed: [LLM-01, LLM-02, LLM-08, CORE-02]

# Metrics
duration: 11min
completed: 2026-03-01
---

# Phase 3 Plan 1: Anthropic Provider and Core Types Summary

**Anthropic LLM provider crate with SSE streaming, real core types replacing placeholders, and extended adapter traits for channel editing and typing indicators**

## Performance

- **Duration:** 11 min
- **Started:** 2026-03-01T00:25:58+01:00
- **Completed:** 2026-03-01T00:36:51+01:00
- **Tasks:** 2
- **Files modified:** 13

## Accomplishments
- Replaced all placeholder types (InboundMessage, OutboundMessage, ProviderRequest, ProviderResponse, ProviderStreamChunk, ChannelCapabilities) with full structs containing real fields
- Created blufio-anthropic crate with complete SSE streaming implementation for Anthropic Messages API
- Implemented transient error retry logic (429/500/503) with 1-second backoff
- Added edit_message and send_typing default methods to ChannelAdapter trait
- Extended config model with system_prompt, system_prompt_file, max_tokens, and api_version fields
- 55 unit tests for the anthropic crate covering serialization, SSE parsing, retry logic, and system prompt loading

## Task Commits

Each task was committed atomically:

1. **Task 1: Flesh out core types and extend traits** - `a682a15` (feat)
2. **Task 2: Create blufio-anthropic crate with SSE streaming** - `cdd319e` (feat)

## Files Created/Modified
- `crates/blufio-anthropic/Cargo.toml` - Crate manifest with workspace deps and eventsource-stream
- `crates/blufio-anthropic/src/lib.rs` - AnthropicProvider implementing ProviderAdapter with streaming support
- `crates/blufio-anthropic/src/client.rs` - HTTP client for api.anthropic.com/v1/messages with retry logic
- `crates/blufio-anthropic/src/types.rs` - Anthropic API request/response types and SSE event types (18 SSE types)
- `crates/blufio-anthropic/src/sse.rs` - SSE stream parser converting byte stream to typed StreamEvent variants
- `crates/blufio-core/src/types.rs` - Real content fields for all message and provider types
- `crates/blufio-core/src/traits/provider.rs` - Stream return type using Pin<Box<dyn Stream>>
- `crates/blufio-core/src/traits/channel.rs` - edit_message and send_typing with default no-op implementations
- `crates/blufio-core/Cargo.toml` - Added futures-core dependency for Stream trait
- `crates/blufio-config/src/model.rs` - system_prompt, system_prompt_file, max_tokens, api_version fields
- `Cargo.toml` - Added blufio-anthropic to workspace members

## Decisions Made
- Used eventsource-stream 0.2 (not 0.1) for SSE parsing -- compatible with reqwest 0.12 byte streams
- ProviderAdapter::stream returns Pin<Box<dyn Stream<Item = Result<ProviderStreamChunk, BlufioError>> + Send>> for async compatibility
- API key resolution order: config field -> ANTHROPIC_API_KEY env var -> error (no vault fallback yet)
- System prompt loading: file > inline string > default "You are {name}, a concise personal assistant."
- CacheControlMarker::ephemeral() auto-applied on all requests for Anthropic prompt caching
- SSE unknown event types silently ignored per Anthropic API versioning policy
- wiremock 0.6 for mock HTTP testing (no real API calls in tests)

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required. ANTHROPIC_API_KEY environment variable must be set at runtime.

## Next Phase Readiness
- Anthropic provider ready for consumption by agent loop (Plan 03-03)
- Core types fleshed out for Telegram adapter (Plan 03-02)
- ChannelAdapter extended with edit_message/send_typing for Telegram streaming UX
- All workspace tests pass (55 anthropic tests + full workspace green)

## Self-Check: PASSED

All referenced files exist. All commit hashes verified in git history.

---
*Phase: 03-agent-loop-telegram*
*Completed: 2026-03-01*
