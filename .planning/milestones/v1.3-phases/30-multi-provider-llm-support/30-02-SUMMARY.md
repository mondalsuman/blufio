---
phase: 30-multi-provider-llm-support
plan: 02
subsystem: provider
tags: [ollama, llm, ndjson, streaming, tool-calling, local-inference, multi-provider]

# Dependency graph
requires:
  - phase: 30-multi-provider-llm-support
    plan: 01
    provides: ProvidersConfig with OllamaConfig, ProviderAdapter trait with ToolDefinition/ToolUseData
provides:
  - blufio-ollama crate implementing ProviderAdapter for Ollama native /api/chat endpoint
  - NDJSON stream parser for line-delimited JSON (not SSE)
  - Ollama wire format types (OllamaRequest, OllamaResponse, TagsResponse)
  - Local model discovery via /api/tags
  - Tool calling support with complete tool calls (not partial deltas)
affects: [30-04-gemini, provider-routing, agent-session]

# Tech tracking
tech-stack:
  added: [bytes]
  patterns: [ndjson-stream-parsing, ollama-native-api, local-provider-no-auth]

key-files:
  created:
    - crates/blufio-ollama/Cargo.toml
    - crates/blufio-ollama/src/lib.rs
    - crates/blufio-ollama/src/client.rs
    - crates/blufio-ollama/src/stream.rs
    - crates/blufio-ollama/src/types.rs
  modified: []

key-decisions:
  - "Ollama uses native /api/chat endpoint (not OpenAI compatibility shim) per project constraints"
  - "NDJSON streaming with BytesMut buffer for partial line accumulation across HTTP chunks"
  - "Tool calls arrive complete from Ollama (not partial deltas) -- each tool_call gets a generated UUID"
  - "done_reason mapping: stop->end_turn, length->max_tokens (same pattern as OpenAI)"
  - "No auth headers, no TLS enforcement, no SSRF protection -- Ollama is a local service"
  - "Response IDs generated as ollama-{uuid} since Ollama API does not provide response IDs"
  - "Image content blocks skipped with warning -- model-dependent, not universally supported"

patterns-established:
  - "NDJSON parser pattern: BytesMut buffer + newline splitting + per-line JSON parse"
  - "Local provider pattern: no api_key, no security hardening, health check via /api/tags"

requirements-completed: [PROV-04, PROV-05]

# Metrics
duration: 8min
completed: 2026-03-05
---

# Phase 30 Plan 02: Ollama Provider Summary

**Ollama native provider with NDJSON streaming, tool calling, /api/tags model discovery, and fail-fast validation for local LLM inference**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-05T15:33:40Z
- **Completed:** 2026-03-05T15:41:53Z
- **Tasks:** 2 (both TDD)
- **Files modified:** 5

## Accomplishments
- Full ProviderAdapter implementation for Ollama's native /api/chat endpoint with NDJSON streaming
- NDJSON stream parser correctly handles partial lines across HTTP chunks, empty lines, and invalid JSON
- HTTP client for /api/chat (POST) and /api/tags (GET) with no auth headers
- Tool calling works with streaming -- Ollama sends complete tool_calls (not partial deltas)
- Fail-fast validation: clear error if default_model missing or Ollama unreachable
- Local model discovery via list_local_models() -> /api/tags
- 44 total tests all passing, clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Create blufio-ollama crate with types, NDJSON stream parser, and HTTP client** - `1966546` (feat)
2. **Task 2: Implement OllamaProvider with ProviderAdapter trait** - `6cff238` (feat)

## Files Created/Modified
- `crates/blufio-ollama/Cargo.toml` - Crate manifest with workspace deps (reqwest, serde, bytes, uuid)
- `crates/blufio-ollama/src/types.rs` - Ollama wire format: OllamaRequest, OllamaResponse, TagsResponse, tool types
- `crates/blufio-ollama/src/stream.rs` - NDJSON stream parser with BytesMut partial-line buffering
- `crates/blufio-ollama/src/client.rs` - HTTP client: chat(), chat_stream(), list_tags(), health_check()
- `crates/blufio-ollama/src/lib.rs` - OllamaProvider with PluginAdapter + ProviderAdapter impls

## Decisions Made
- Used native /api/chat endpoint (not OpenAI compat shim) per project constraint
- NDJSON parsing via BytesMut buffer with newline splitting (simpler than SSE parsing)
- Tool calls arrive complete from Ollama, each gets generated UUID (ollama-tc-{uuid})
- Response IDs generated as ollama-{uuid} since Ollama API doesn't provide them
- Image content blocks skipped with warning (Ollama vision is model-dependent)
- No retry logic -- Ollama is a local service, transient network errors are unlikely
- Cache tokens always 0 -- Ollama has no prompt caching mechanism

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Created stub lib.rs for blufio-gemini and blufio-openrouter**
- **Found during:** Task 1 (initial cargo build)
- **Issue:** blufio-gemini and blufio-openrouter crates (created by Plan 30-01) had src/types.rs and src/stream.rs but no src/lib.rs, causing workspace compilation failure
- **Fix:** Created minimal lib.rs stubs with `pub mod` declarations for existing modules
- **Files modified:** crates/blufio-gemini/src/lib.rs, crates/blufio-openrouter/src/lib.rs
- **Verification:** Workspace compiles successfully
- **Note:** These files were auto-formatted by the project linter on creation

**2. [Rule 1 - Bug] Fixed partial_lines_across_chunks test data**
- **Found during:** Task 1 (test execution)
- **Issue:** Raw string literal `r#"..."#` delimiter consumed a quote character that was part of the JSON test data, producing invalid JSON when chunks were concatenated
- **Fix:** Changed the split point in test data to avoid the delimiter ambiguity
- **Verification:** Test passes with correct JSON round-trip

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both fixes necessary for compilation and test correctness. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required. Ollama must be installed and running locally at runtime, but not for tests (wiremock mocks all HTTP calls).

## Next Phase Readiness
- Ollama provider complete, ready for provider routing integration (30-04)
- Pattern established for local providers (no auth, no security hardening)
- NDJSON parsing pattern available for reuse if other providers use line-delimited JSON

## Self-Check: PASSED

All 5 artifact files verified on disk. Both task commits verified in git log.

---
*Phase: 30-multi-provider-llm-support, Plan: 02*
*Completed: 2026-03-05*
