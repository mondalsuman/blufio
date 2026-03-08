---
gsd_state_version: 1.0
milestone: v1.3
milestone_name: Ecosystem Expansion
status: completed
stopped_at: Completed 44-02-PLAN.md (Phase 44 complete)
last_updated: "2026-03-08T20:31:30.097Z"
last_activity: 2026-03-08 -- Phase 44 Plan 02 complete, ApprovalRouter wired into serve.rs with EventBus subscription
progress:
  total_phases: 17
  completed_phases: 15
  total_plans: 45
  completed_plans: 43
  percent: 96
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-07)

**Core value:** An always-on personal AI agent that is secure enough to trust, efficient enough to afford, and simple enough to deploy by copying one file.
**Current focus:** Phase 44 -- Node Approval Wiring (gap closure)

## Current Position

Phase: 44 of 45 (Node Approval Wiring)
Plan: 2 of 2 complete
Status: Phase Complete
Last activity: 2026-03-08 -- Phase 44 Plan 02 complete, ApprovalRouter wired into serve.rs with EventBus subscription

Progress: [███████████████████░] 44/45 plans (96%)

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
| 39 | 04 | ~18min | 2 | 2 |
| Phase 39 P02 | 20min | 2 tasks | 2 files |
| Phase 39 P01 | 21min | 2 tasks | 2 files |
| 39 | 05 | ~21min | 2 | 2 |
| 39 | 06 | ~9min | 2 | 3 |
| 39 | 07 | ~8min | 2 | 4 |

**Velocity (v1.3) Summary:**
- Total plans completed: 36
- Total execution time: ~3 days
- Average: ~12 plans/day
| Phase 40 P01 | 7min | 2 tasks | 3 files |
| Phase 40 P02 | 7min | 2 tasks | 1 files |
| Phase 41 P01 | 7min | 2 tasks | 3 files |
| Phase 41 P02 | 5min | 2 tasks | 2 files |
| Phase 42 P01 | 3min | 2 tasks | 2 files |
| Phase 42 P02 | 2min | 2 tasks | 1 files |
| Phase 43 P01 | 7min | 2 tasks | 5 files |
| Phase 44 P01 | 4min | 2 tasks | 2 files |
| Phase 44 P02 | 5min | 2 tasks | 2 files |

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
- [Phase 39]: Docker build UNVERIFIED due to missing daemon -- static analysis confirms correctness
- [Phase 39]: Phase 32 code verified from source despite ROADMAP showing Not started; all API-11..18 modules exist and 53 tests pass
- [Phase 39]: Phase 29 verification scored 8/8 -- all requirements have code + test evidence
- [Phase 39]: Phase 30 re-verification confirmed 9/9 -- no regressions, test counts unchanged at 209
- [Phase 39]: Phase 37 re-verification confirmed 17/19 -- 2 gaps are implementation gaps (approval wiring), NODE-05 core satisfied
- [Phase 39]: Phase 38 re-verification confirmed 13/13 -- no regressions, 142 tests pass
- [Phase 39]: Integration flow tests in blufio-test-utils/tests/ (not unit tests) -- uses wiremock + TestHarness for cross-crate E2E
- [Phase 39]: Gateway tested via TestHarness pipeline, not actual server binding -- avoids port allocation in CI
- [Phase 39]: v1.3 milestone declared READY TO SHIP -- 71/71 requirements verified, 4/4 integration flows passing, 2 Phase 37 internal wiring gaps are non-blocking
- [Phase 40]: Global EventBus capacity 1024 (up from node-scoped 128) since it handles all event types
- [Phase 40]: blufio-bus added as dependency to blufio-agent (was only in blufio main crate)
- [Phase 40]: Bridge dispatch calls adapter.send() directly (outbound-only) to prevent infinite loops
- [Phase 41]: Ollama provider stored as separate Arc<OllamaProvider> field to avoid Any downcast for list_local_models()
- [Phase 41]: All four new provider features (openai, ollama, openrouter, gemini) added to default feature set
- [Phase 41]: Provider registry init gated on config.gateway.enabled (no unnecessary API key validation)
- [Phase 41]: set_api_tools_allowlist uses &mut self, gateway binding changed to `let mut gateway`
- [Phase 42]: Dedicated tokio_rusqlite connection for gateway stores (separate from main storage connection)
- [Phase 42]: webhook_store cloned before setter call to preserve Arc for Plan 02 webhook delivery
- [Phase 42]: webhook_store moved (not cloned) into delivery task since setter already consumed its own clone
- [Phase 43]: EventBus.publish() is fire-and-forget (returns ()), publish calls use simple await without error handling
- [Phase 43]: WasmSkillRuntime not created in production serve.rs yet; set_event_bus ready for wiring when skill loading implemented
- [Phase 43]: MessageSent published after both send and edit-in-place paths complete, before persist_response
- [Phase 44]: OnceLock<Arc<ApprovalRouter>> replaces Option<Arc<>> for Arc-compatible set_approval_router(&self)
- [Phase 44]: BusEvent::event_type_string() returns &'static str (zero allocation) for all 15 leaf variants
- [Phase 44]: Approval subscription spawned before reconnect_all to capture events during reconnection
- [Phase 44]: Fire-and-forget request_approval (drop Receiver) -- events are post-action notifications, not gates

### Pending Todos

None.

### Blockers/Concerns

None.

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 1 | Update all documentation according to current states | 2026-03-04 | f559572 | [1-update-all-documentation-according-to-cu](./quick/1-update-all-documentation-according-to-cu/) |

## Session Continuity

Last session: 2026-03-08T20:27:54.954Z
Stopped at: Completed 44-02-PLAN.md (Phase 44 complete)
Resume file: None
