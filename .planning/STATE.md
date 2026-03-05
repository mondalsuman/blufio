---
gsd_state_version: 1.0
milestone: v1.3
milestone_name: Ecosystem Expansion
status: unknown
last_updated: "2026-03-05T17:00:00.000Z"
progress:
  total_phases: 3
  completed_phases: 3
  total_plans: 9
  completed_plans: 9
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-05)

**Core value:** An always-on personal AI agent that is secure enough to trust, efficient enough to afford, and simple enough to deploy by copying one file.
**Current focus:** v1.3 Ecosystem Expansion — Phase 32 (Scoped API Keys, Webhooks & Batch)

## Current Position

Phase: 32 of 39 (Scoped API Keys, Webhooks & Batch)
Plan: 0 of 0 in current phase
Status: Ready to plan
Last activity: 2026-03-05 — Phase 31 complete (3/3 plans, OpenAI-compatible gateway API)

Progress: [███░░░░░░░] 27%

## Performance Metrics

**Velocity (v1.0):**
- Total plans completed: 43
- Total execution time: ~3 days
- Average: ~10 plans/day

**Velocity (v1.1):**
- Total plans completed: 32
- Total execution time: ~2 days
- Average: ~16 plans/day

**Velocity (v1.2):**
- Total plans completed: 13
- Total execution time: ~1 day
- Average: ~13 plans/day

**Velocity (v1.3):**

| Phase | Plan | Duration | Tasks | Files |
|-------|------|----------|-------|-------|
| 30 | 01 | 11min | 3 | 7 |
| 30 | 02 | 8min | 2 | 5 |
| 30 | 03 | 6min | 2 | 5 |
| 30 | 04 | 7min | 2 | 5 |
| 31 | 01 | ~15min | 2 | 10 |
| 31 | 02 | ~10min | 2 | 3 |
| 31 | 03 | ~10min | 2 | 4 |

## Accumulated Context

### Decisions

All decisions logged in PROJECT.md Key Decisions table.
Key v1.3 constraints:
- Event bus must come first (unblocks webhooks, bridging, nodes, batch)
- Provider-agnostic ToolDefinition must exist before non-Anthropic providers
- OpenAI wire types MUST be separate from internal ProviderResponse
- Ollama must use native /api/chat (not OpenAI compat shim)
- matrix-sdk must pin to 0.11.0 (0.12+ requires Rust 1.88)
- Signal uses signal-cli sidecar (no native Rust library)
- WhatsApp Web is experimental/feature-flagged
- OpenAI system prompt mapped to system role message (not separate field)
- Used max_completion_tokens for OpenAI (newer API, not deprecated max_tokens)
- Tool call args accumulated via HashMap<index, (id, name, args)> across SSE deltas
- Ollama NDJSON streaming with BytesMut buffer for partial line accumulation
- Ollama tool calls arrive complete (not partial deltas); each gets generated UUID
- Ollama response IDs generated as ollama-{uuid} (API doesn't provide them)
- OpenRouter wire types independent from OpenAI crate (crate decoupling)
- OpenRouter provider preferences only included when provider_order non-empty
- OpenRouter health check deferred to first request (no zero-cost auth endpoint)
- Gemini uses native API format (not OpenAI-compatible shim) with systemInstruction, functionDeclarations
- Gemini API key sent as query parameter ?key= (not header)
- Gemini streams chunked JSON (not SSE); parser uses brace depth counter
- Gemini function calls arrive complete; UUIDs generated for response IDs
- Gateway OpenAI compat uses Pin<Box<dyn Stream>> for SSE to unify match arms
- ProviderRegistry trait in blufio-core; GatewayState extended with providers/tools/allowlist
- Tool source detection uses name pattern: `__` separator = namespaced (mcp/wasm), else builtin
- GatewayConfig api_tools_allowlist: empty = no tools accessible (secure default)
- OpenResponses streaming-only (no store/async); stream=false returns 400

### Pending Todos

None.

### Blockers/Concerns

None.

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 1 | Update all documentation according to current states | 2026-03-04 | f559572 | [1-update-all-documentation-according-to-cu](./quick/1-update-all-documentation-according-to-cu/) |

## Session Continuity

Last session: 2026-03-05
Stopped at: Phase 32 context gathered, ready to plan
Resume file: .planning/phases/32-scoped-api-keys-webhooks-batch/32-CONTEXT.md
