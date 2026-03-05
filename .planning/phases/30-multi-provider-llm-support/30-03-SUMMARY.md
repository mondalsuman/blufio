---
phase: 30-multi-provider-llm-support
plan: 03
subsystem: provider
tags: [openrouter, llm, streaming, sse, tool-calling, provider-routing, multi-provider]

# Dependency graph
requires:
  - phase: 30-multi-provider-llm-support
    plan: 01
    provides: ProvidersConfig with OpenRouterConfig, OpenAI provider pattern (types + client + sse + lib)
provides:
  - blufio-openrouter crate implementing ProviderAdapter for OpenRouter API
  - OpenRouter wire format types (RouterRequest/RouterResponse with ProviderPreferences)
  - HTTP client with X-Title, HTTP-Referer, and Bearer auth headers
  - SSE stream parser for OpenAI-compatible streaming
  - Streaming tool call accumulation across deltas
  - Provider preference routing (order, allow_fallbacks)
affects: [30-04-gemini, provider-routing, agent-session]

# Tech tracking
tech-stack:
  added: []
  patterns: [openrouter-provider-preferences, x-title-http-referer-headers]

key-files:
  created:
    - crates/blufio-openrouter/Cargo.toml
    - crates/blufio-openrouter/src/lib.rs
    - crates/blufio-openrouter/src/client.rs
    - crates/blufio-openrouter/src/types.rs
    - crates/blufio-openrouter/src/sse.rs
  modified: []

key-decisions:
  - "OpenRouter uses own wire types (not shared with blufio-openai) for crate decoupling"
  - "Provider preferences only included when provider_order is non-empty (omitted from request otherwise)"
  - "Health check deferred to first request -- OpenRouter has no zero-cost auth endpoint"
  - "Full model IDs passed through directly (e.g., anthropic/claude-sonnet-4) -- no alias translation"

patterns-established:
  - "OpenRouter provider pattern follows same structure as OpenAI: types.rs + client.rs + sse.rs + lib.rs"
  - "Provider-specific headers (X-Title, HTTP-Referer) set as default headers on reqwest client"

requirements-completed: [PROV-06, PROV-07]

# Metrics
duration: 6min
completed: 2026-03-05
---

# Phase 30 Plan 03: OpenRouter Provider Summary

**OpenRouter provider crate with provider preference routing, X-Title/HTTP-Referer headers, streaming tool call accumulation, and full model ID passthrough**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-05T15:33:52Z
- **Completed:** 2026-03-05T15:40:41Z
- **Tasks:** 2 (both TDD: RED->GREEN)
- **Files modified:** 5

## Accomplishments
- Complete blufio-openrouter crate with ProviderAdapter + PluginAdapter implementations
- X-Title and HTTP-Referer headers sent on every request via default headers
- Provider fallback ordering configurable via provider_order config -> provider.order in request body
- OpenAI-compatible format with OpenRouter-specific ProviderPreferences extension
- Streaming tool calls work via SSE with stateful argument accumulation
- 49 total tests passing, clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Create blufio-openrouter crate with types and HTTP client** - `1686897` (feat: types + SSE + client)
2. **Task 2: Implement OpenRouterProvider with ProviderAdapter trait** - `d4c9ef9` (feat: ProviderAdapter impl)

## Files Created/Modified
- `crates/blufio-openrouter/Cargo.toml` - Crate manifest with workspace dependencies
- `crates/blufio-openrouter/src/types.rs` - OpenRouter wire format: RouterRequest, RouterResponse, ProviderPreferences, SseChunk
- `crates/blufio-openrouter/src/client.rs` - HTTP client with Bearer auth, X-Title, HTTP-Referer, retry logic
- `crates/blufio-openrouter/src/sse.rs` - SSE stream parser for OpenAI-compatible streaming protocol
- `crates/blufio-openrouter/src/lib.rs` - OpenRouterProvider with ProviderAdapter + PluginAdapter impls, streaming tool call accumulation

## Decisions Made
- OpenRouter wire types are independently owned (not shared with blufio-openai) for clean crate decoupling
- Provider preferences are only serialized into the request when provider_order is non-empty; when empty, the provider field is omitted entirely
- Health check returns Healthy immediately, deferring API key validation to the first real request since OpenRouter lacks a zero-cost auth endpoint
- Full OpenRouter model IDs (e.g., "anthropic/claude-sonnet-4") are passed through without any alias translation

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required. OpenRouter API key is needed at runtime
(set `OPENROUTER_API_KEY` env var or `providers.openrouter.api_key` in config) but not for tests.

## Next Phase Readiness
- OpenRouter provider ready for integration into agent session loop
- All four provider config structs in place (OpenAI, Ollama, OpenRouter, Gemini)
- Three of four providers complete (OpenAI 30-01, Ollama 30-02, OpenRouter 30-03)
- Gemini (30-04) is the final provider remaining

## Self-Check: PASSED

All 5 artifact files verified on disk. All 2 task commits verified in git log.

---
*Phase: 30-multi-provider-llm-support, Plan: 03*
*Completed: 2026-03-05*
