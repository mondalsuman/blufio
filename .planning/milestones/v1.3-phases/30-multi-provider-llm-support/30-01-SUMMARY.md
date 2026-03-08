---
phase: 30-multi-provider-llm-support
plan: 01
subsystem: provider
tags: [openai, llm, streaming, sse, tool-calling, vision, multi-provider]

# Dependency graph
requires:
  - phase: 29-event-bus-and-trait-extensions
    provides: ProviderAdapter trait with ToolDefinition, ToolUseData, ContentBlock types
provides:
  - blufio-openai crate implementing ProviderAdapter for OpenAI Chat Completions API
  - ProvidersConfig with default field and all four provider config structs
  - OpenAI wire format types (ChatRequest, ChatResponse, SseChunk)
  - SSE stream parser for OpenAI data: [DONE] protocol
  - Streaming tool call accumulation across deltas
affects: [30-02-ollama, 30-03-openrouter, 30-04-gemini, provider-routing, agent-session]

# Tech tracking
tech-stack:
  added: [eventsource-stream, wiremock]
  patterns: [openai-sse-stream-parsing, tool-call-accumulation, finish-reason-mapping]

key-files:
  created:
    - crates/blufio-openai/Cargo.toml
    - crates/blufio-openai/src/lib.rs
    - crates/blufio-openai/src/client.rs
    - crates/blufio-openai/src/sse.rs
    - crates/blufio-openai/src/types.rs
  modified:
    - crates/blufio-config/src/model.rs
    - crates/blufio-config/src/validation.rs

key-decisions:
  - "OpenAI system prompt mapped to system role message in messages array (not separate field like Anthropic)"
  - "Used max_completion_tokens instead of deprecated max_tokens for OpenAI requests"
  - "Tool call arguments accumulated across SSE deltas using HashMap<index, (id, name, args)>"
  - "Finish reason mapping: stop->end_turn, tool_calls->tool_use, length->max_tokens"
  - "stream_options.include_usage=true to get token usage in streaming responses"

patterns-established:
  - "OpenAI provider pattern: types.rs (wire format) + client.rs (HTTP) + sse.rs (stream) + lib.rs (ProviderAdapter)"
  - "ProvidersConfig pattern: per-provider config struct with serde defaults and deny_unknown_fields"

requirements-completed: [PROV-01, PROV-02, PROV-03]

# Metrics
duration: 11min
completed: 2026-03-05
---

# Phase 30 Plan 01: OpenAI Provider Summary

**OpenAI provider crate with streaming SSE, tool call accumulation, vision support, and configurable base_url for Azure/Together/Fireworks compatibility**

## Performance

- **Duration:** 11 min
- **Started:** 2026-03-05T15:19:08Z
- **Completed:** 2026-03-05T15:30:35Z
- **Tasks:** 3 (all TDD: RED->GREEN->REFACTOR)
- **Files modified:** 7

## Accomplishments
- ProvidersConfig extended with `default` field and OpenAI/Ollama/OpenRouter/Gemini config structs
- Full OpenAI Chat Completions API client with Bearer auth, retry on 429/500/503, configurable base_url
- SSE stream parser handling `data: [DONE]` terminator and typed chunk parsing
- ProviderAdapter trait fully implemented with streaming tool call accumulation
- Vision content maps to image_url format with data URI encoding
- 63 total tests (20 config + 43 provider) all passing, clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend ProvidersConfig** - `ca502e6` (test: failing tests) + `c3fbfb2` (feat: implementation)
2. **Task 2: Create blufio-openai crate** - `a122769` (feat: types + SSE + client)
3. **Task 3: Implement OpenAIProvider** - `a8c00a3` (feat: ProviderAdapter impl)

_Note: Task 1 followed TDD with separate RED/GREEN commits._

## Files Created/Modified
- `crates/blufio-openai/Cargo.toml` - Crate manifest with workspace dependencies
- `crates/blufio-openai/src/types.rs` - OpenAI wire format: ChatRequest, ChatResponse, SseChunk, DeltaToolCall
- `crates/blufio-openai/src/client.rs` - HTTP client with Bearer auth, retry, configurable base_url
- `crates/blufio-openai/src/sse.rs` - SSE stream parser for OpenAI streaming protocol
- `crates/blufio-openai/src/lib.rs` - OpenAIProvider with ProviderAdapter + PluginAdapter impls
- `crates/blufio-config/src/model.rs` - ProvidersConfig + OpenAI/Ollama/OpenRouter/Gemini config structs
- `crates/blufio-config/src/validation.rs` - Updated test struct literals for new ProvidersConfig shape

## Decisions Made
- OpenAI system prompt goes in messages array as role "system" (differs from Anthropic's separate field)
- Used `max_completion_tokens` (newer API) instead of deprecated `max_tokens`
- Tool call arguments accumulated across SSE deltas using HashMap<index, (id, name, args_string)>
- Finish reason mapping: "stop" -> "end_turn", "tool_calls" -> "tool_use", "length" -> "max_tokens"
- Enabled `stream_options.include_usage=true` to get token usage in streaming responses
- Content blocks split into separate ChatMessages when mixing text/images with tool_use/tool_result

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed ProvidersConfig struct literals in validation.rs**
- **Found during:** Task 1 (ProvidersConfig extension)
- **Issue:** Existing test code in validation.rs constructed ProvidersConfig directly, missing new fields
- **Fix:** Added `..Default::default()` to all 4 ProvidersConfig struct literals in validation tests
- **Files modified:** crates/blufio-config/src/validation.rs
- **Verification:** All blufio-config tests pass (21/21)
- **Committed in:** c3fbfb2 (part of Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary fix to maintain existing test compatibility. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required. OpenAI API key is needed at runtime
(set `OPENAI_API_KEY` env var or `providers.openai.api_key` in config) but not for tests.

## Next Phase Readiness
- ProvidersConfig infrastructure ready for Ollama (30-02), OpenRouter (30-03), and Gemini (30-04) plans
- All four provider config structs in place with sensible defaults
- OpenAI provider pattern established for other providers to follow

## Self-Check: PASSED

All 7 artifact files verified on disk. All 4 task commits verified in git log.

---
*Phase: 30-multi-provider-llm-support, Plan: 01*
*Completed: 2026-03-05*
