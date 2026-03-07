---
gsd_state_version: 1.0
milestone: v1.3
milestone_name: Ecosystem Expansion
status: completed
stopped_at: Plan 39-03 complete (channel adapters verification)
last_updated: "2026-03-07T16:55:00Z"
last_activity: "2026-03-07 — Plan 39-03 complete (Phases 33+34 verification: 13/13 channel requirements verified)"
progress:
  total_phases: 11
  completed_phases: 9
  total_plans: 29
  completed_plans: 27
  percent: 93
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-05)

**Core value:** An always-on personal AI agent that is secure enough to trust, efficient enough to afford, and simple enough to deploy by copying one file.
**Current focus:** v1.3 Ecosystem Expansion — Phase 39 (Integration Verification)

## Current Position

Phase: 39 of 39 (Integration Verification)
Plan: 3 of 7 in current phase
Status: Plan 39-03 complete
Last activity: 2026-03-07 — Plan 39-03 complete (Phases 33+34 verification: 13/13 channel requirements verified)

Progress: [█████████░] 93%

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
| 33 | 01 | ~15min | 2 | 8 |
| 33 | 02 | ~10min | 2 | 5 |
| 33 | 03 | ~8min | 2 | 4 |
| 34 | 01 | ~20min | 2 | 8 |
| 34 | 02 | ~15min | 2 | 5 |
| 34 | 03 | ~15min | 2 | 6 |
| 34 | 04 | ~15min | 2 | 4 |
| 34 | 05 | ~10min | 2 | 5 |
| 35 | 01 | ~25min | 2 | 7 |
| 35 | 02 | ~20min | 2 | 1 |
| 36 | 01 | ~10min | 2 | 6 |
| 36 | 02 | ~5min | 2 | 2 |
| 37 | 01 | ~12min | 2 | 11 |
| 37 | 02 | ~11min | 2 | 7 |
| 37 | 03 | ~2min | 2 | 3 |
| 38 | 01 | ~17min | 2 | 7 |
| 38 | 02 | ~11min | 2 | 7 |
| 39 | 03 | ~13min | 2 | 2 |

## Accumulated Context

### Decisions

All decisions logged in PROJECT.md Key Decisions table.
Key v1.3 constraints:
- PublisherKeypair is separate from DeviceKeypair (skill author identity vs device identity)
- TOFU key management: first publisher key seen is trusted, key changes hard-blocked
- Pre-execution verification gate runs before every invoke() call
- WASM bytes stored in memory for TOCTOU prevention
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
- Docker uses gcr.io/distroless/cc-debian12:nonroot (not static-debian12) because ONNX Runtime ships glibc-linked .so files
- Single full-featured Docker image with all adapters compiled in; users enable/disable via config.toml
- Docker health check uses `blufio healthcheck` subcommand (no shell/curl needed in distroless)
- Multi-instance systemd template uses /etc/blufio/instances/%i/ for config, /var/lib/blufio/instances/%i/ for data
- Node config structs defined in blufio-config to avoid circular dependency; re-exported from blufio-node/config.rs
- Pairing fingerprint: SHA-256 of sorted concatenated public keys, formatted as XXXX-XXXX-XXXX-XXXX
- tokio-rusqlite errors need explicit type annotation: |e: tokio_rusqlite::Error<rusqlite::Error>|
- [Phase 37]: register_connection/remove_connection async because EventBus::publish is async
- [Phase 37]: First-wins approval via DashMap::remove (atomic remove guarantees only one responder wins)
- [Phase 37]: ConnectionManager gets optional approval_router via setter to avoid circular construction
- [Phase 38]: OpenClaw detection order: --data-dir > $OPENCLAW_HOME > ~/.openclaw
- [Phase 38]: Idempotent migration via migration_log UNIQUE(source, item_type, source_id)
- [Phase 38]: Config translate preserves unmapped fields as TOML comments
- [Phase 38]: Bench SQLite storage ops gated behind cfg(feature = "sqlite") for graceful degradation
- [Phase 38]: Peak RSS via libc getrusage on macOS, /proc/self/status VmHWM on Linux
- [Phase 38]: Bundle verifies binary signature before packaging, continues with warning if .minisig missing
- [Phase 38]: Privacy report is static config analysis only -- no server connection needed

### Pending Todos

None.

### Blockers/Concerns

None.

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 1 | Update all documentation according to current states | 2026-03-04 | f559572 | [1-update-all-documentation-according-to-cu](./quick/1-update-all-documentation-according-to-cu/) |

## Session Continuity

Last session: 2026-03-07T16:55:00Z
Stopped at: Completed 39-03-PLAN.md
Resume file: .planning/phases/39-integration-verification/39-03-SUMMARY.md
