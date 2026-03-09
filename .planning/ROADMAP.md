# Roadmap: Blufio

## Milestones

- ✅ **v1.0 MVP** — Phases 1-14 (shipped 2026-03-02)
- ✅ **v1.1 MCP Integration** — Phases 15-22 (shipped 2026-03-03)
- ✅ **v1.2 Production Hardening** — Phases 23-28 (shipped 2026-03-04)
- ✅ **v1.3 Ecosystem Expansion** — Phases 29-45 (shipped 2026-03-08)
- 🚧 **v1.4 Quality & Resilience** — Phases 46-50 (in progress)

## Phases

<details>
<summary>✅ v1.0 MVP (Phases 1-14) — SHIPPED 2026-03-02</summary>

- [x] Phase 1: Project Foundation & Workspace (2/2 plans) — completed 2026-02-28
- [x] Phase 2: Persistence & Security Vault (2/2 plans) — completed 2026-02-28
- [x] Phase 3: Agent Loop & Telegram (4/4 plans) — completed 2026-03-01
- [x] Phase 4: Context Engine & Cost Tracking (3/3 plans) — completed 2026-03-01
- [x] Phase 5: Memory & Embeddings (3/3 plans) — completed 2026-03-01
- [x] Phase 6: Model Routing & Smart Heartbeats (3/3 plans) — completed 2026-03-01
- [x] Phase 7: WASM Skill Sandbox (4/4 plans) — completed 2026-03-01
- [x] Phase 8: Plugin System & Gateway (3/3 plans) — completed 2026-03-01
- [x] Phase 9: Production Hardening (3/3 plans) — completed 2026-03-01
- [x] Phase 10: Multi-Agent & Final Integration (3/3 plans) — completed 2026-03-01
- [x] Phase 11: Fix Critical Integration Bugs (4/4 plans) — completed 2026-03-01
- [x] Phase 12: Verify Unverified Phases (5/5 plans) — completed 2026-03-01
- [x] Phase 13: Sync Traceability & Documentation (1/1 plan) — completed 2026-03-02
- [x] Phase 14: Wire Cross-Phase Integration (3/3 plans) — completed 2026-03-02

</details>

<details>
<summary>✅ v1.1 MCP Integration (Phases 15-22) — SHIPPED 2026-03-03</summary>

- [x] Phase 15: MCP Foundation (4/4 plans) — completed 2026-03-02
- [x] Phase 16: MCP Server stdio (3/3 plans) — completed 2026-03-02
- [x] Phase 17: MCP Server HTTP + Resources (5/5 plans) — completed 2026-03-02
- [x] Phase 18: MCP Client (4/4 plans) — completed 2026-03-03
- [x] Phase 19: Integration Testing + Tech Debt (5/5 plans) — completed 2026-03-03
- [x] Phase 20: Verify Phase 15 & 16 Completeness (4/4 plans) — completed 2026-03-03
- [x] Phase 21: Fix MCP Wiring Gaps (4/4 plans) — completed 2026-03-03
- [x] Phase 22: Verify Phase 18 & 19 + Close Traceability (3/3 plans) — completed 2026-03-03

</details>

<details>
<summary>✅ v1.2 Production Hardening (Phases 23-28) — SHIPPED 2026-03-04</summary>

- [x] Phase 23: Backup Integrity Verification (1/1 plan) — completed 2026-03-03
- [x] Phase 24: sd_notify Integration (2/2 plans) — completed 2026-03-03
- [x] Phase 25: SQLCipher Database Encryption (4/4 plans) — completed 2026-03-03
- [x] Phase 26: Minisign Signature Verification (2/2 plans) — completed 2026-03-03
- [x] Phase 27: Self-Update with Rollback (2/2 plans) — completed 2026-03-03
- [x] Phase 28: Close Audit Gaps (2/2 plans) — completed 2026-03-04

</details>

<details>
<summary>✅ v1.3 Ecosystem Expansion (Phases 29-45) — SHIPPED 2026-03-08</summary>

- [x] Phase 29: Event Bus & Core Trait Extensions (2/2 plans) — completed 2026-03-05
- [x] Phase 30: Multi-Provider LLM Support (4/4 plans) — completed 2026-03-05
- [x] Phase 31: OpenAI-Compatible Gateway API (3/3 plans) — completed 2026-03-05
- [x] Phase 32: Scoped API Keys, Webhooks & Batch (3/3 plans) — completed 2026-03-06
- [x] Phase 33: Discord & Slack Channel Adapters (3/3 plans) — completed 2026-03-06
- [x] Phase 34: WhatsApp, Signal, IRC & Matrix Adapters (5/5 plans) — completed 2026-03-06
- [x] Phase 35: Skill Registry & Code Signing (2/2 plans) — completed 2026-03-06
- [x] Phase 36: Docker Image & Deployment (2/2 plans) — completed 2026-03-07
- [x] Phase 37: Node System (3/3 plans) — completed 2026-03-07
- [x] Phase 38: Migration & CLI Utilities (2/2 plans) — completed 2026-03-07
- [x] Phase 39: Integration Verification (7/7 plans) — completed 2026-03-07
- [x] Phase 40: Wire Global EventBus & Bridge (2/2 plans) — completed 2026-03-07
- [x] Phase 41: Wire ProviderRegistry into Gateway (2/2 plans) — completed 2026-03-07
- [x] Phase 42: Wire Gateway Stores (2/2 plans) — completed 2026-03-07
- [x] Phase 43: Wire EventBus Event Publishers (1/1 plan) — completed 2026-03-08
- [x] Phase 44: Node Approval Wiring (2/2 plans) — completed 2026-03-08
- [x] Phase 45: Documentation & Traceability Sync (2/2 plans) — completed 2026-03-08

</details>

### 🚧 v1.4 Quality & Resilience (In Progress)

**Milestone Goal:** Address QA audit deviations -- accurate token counting, circuit breakers, graceful degradation, typed errors, format pipeline integration, and architectural decision records.

- [ ] **Phase 46: Core Types & Error Hierarchy** - Typed error hierarchy with retryable/severity/category classification, extended ChannelCapabilities, and Table/List content types
- [ ] **Phase 47: Accurate Token Counting** - Replace len()/4 heuristic with real tokenizer-backed counting for all 5 LLM providers
- [ ] **Phase 48: Circuit Breaker & Degradation Ladder** - Per-dependency circuit breakers with 6-level graceful degradation and automatic escalation
- [ ] **Phase 49: FormatPipeline Integration** - Wire FormatPipeline into all 8 channel adapters with message splitting and adapter-specific formatting
- [ ] **Phase 50: ADRs & Documentation** - Architectural decision records for ORT pinning and plugin architecture

## Phase Details

### Phase 46: Core Types & Error Hierarchy
**Goal**: All errors in the system carry structured metadata enabling automated retry decisions, and core types support rich content formatting with per-channel capability awareness
**Depends on**: Nothing (foundation for v1.4)
**Requirements**: ERR-01, ERR-02, ERR-03, ERR-04, ERR-05, CAP-01, CAP-02, CAP-03, FMT-01, FMT-02, FMT-03
**Success Criteria** (what must be TRUE):
  1. Calling `error.is_retryable()` on any BlufioError returns a meaningful bool -- RateLimited and Timeout are retryable, AuthFailed and Config are not
  2. Calling `error.severity()` and `error.category()` on any BlufioError returns structured enums that can be matched exhaustively
  3. Provider errors (RateLimited, AuthFailed, ServerError, Timeout, ModelNotFound) and channel errors (DeliveryFailed, ConnectionLost, RateLimited) are distinct matchable variants
  4. ChannelCapabilities reports streaming_type, formatting_support, and rate_limit fields for capability-aware downstream decisions
  5. FormatPipeline accepts Table and BulletList/OrderedList content and degrades them to aligned text or plain text for channels without native support
**Plans:** 3/4 plans executed
Plans:
- [ ] 46-01-PLAN.md -- Core error types, sub-enums, classification methods, ErrorContext, ChannelCapabilities extension
- [ ] 46-02-PLAN.md -- Provider crate migration (5 crates) to typed ProviderErrorKind
- [ ] 46-03-PLAN.md -- Channel/storage/MCP/skill migration to typed sub-enums + extended capabilities
- [ ] 46-04-PLAN.md -- FormatPipeline Table/List + error consumer updates + comprehensive tests

### Phase 47: Accurate Token Counting
**Goal**: Context engine counts tokens accurately for all supported LLM providers instead of estimating with len()/4
**Depends on**: Nothing (independent of Phase 46, can run in parallel)
**Requirements**: TOK-01, TOK-02, TOK-03, TOK-04, TOK-05, TOK-06, TOK-07, TOK-08, TOK-09
**Success Criteria** (what must be TRUE):
  1. Context engine token counts for OpenAI models use tiktoken-rs with the correct encoding (o200k_base for GPT-4o+, cl100k_base for GPT-4/3.5)
  2. Context engine token counts for Claude models use HuggingFace tokenizers crate with Xenova/claude-tokenizer vocabulary
  3. Ollama models use per-model tokenizer.json when available and a calibrated heuristic as fallback; Gemini uses calibrated heuristic; OpenRouter delegates to the underlying model's tokenizer
  4. Tokenizer instances are lazy-loaded, cached, and reused across calls -- not created per request
  5. Token counting runs via spawn_blocking so synchronous tokenizer.encode() never blocks tokio worker threads
**Plans:** 4 plans
Plans:
- [ ] 46-01-PLAN.md -- Core error types, sub-enums, classification methods, ErrorContext, ChannelCapabilities extension
- [ ] 46-02-PLAN.md -- Provider crate migration (5 crates) to typed ProviderErrorKind
- [ ] 46-03-PLAN.md -- Channel/storage/MCP/skill migration to typed sub-enums + extended capabilities
- [ ] 46-04-PLAN.md -- FormatPipeline Table/List + error consumer updates + comprehensive tests

### Phase 48: Circuit Breaker & Degradation Ladder
**Goal**: Every external dependency has an independent circuit breaker, and the system automatically degrades through 6 levels when dependencies fail
**Depends on**: Phase 46 (requires is_retryable() from typed errors)
**Requirements**: CB-01, CB-02, CB-03, CB-04, CB-05, CB-06, CB-07, DEG-01, DEG-02, DEG-03, DEG-04, DEG-05, DEG-06
**Success Criteria** (what must be TRUE):
  1. Each external dependency (5 providers, 8 channels) has its own circuit breaker with Closed/Open/HalfOpen states, and non-retryable errors (auth, config) do not trip the breaker
  2. Circuit breaker state transitions publish Resilience events to EventBus and emit Prometheus gauge `blufio_circuit_breaker_state` per dependency
  3. Circuit breaker thresholds (failure count, reset timeout, half-open probes) are configurable per dependency via TOML `[resilience.circuit_breakers.<name>]`
  4. DegradationManager tracks current level (L0-L5), auto-escalates based on circuit breaker state changes, and de-escalates only after sustained recovery (hysteresis)
  5. Degradation state is visible via `/v1/health` API, published to EventBus, and user-facing messages are delivered to the primary channel at each level transition
**Plans:** 4 plans
Plans:
- [ ] 46-01-PLAN.md -- Core error types, sub-enums, classification methods, ErrorContext, ChannelCapabilities extension
- [ ] 46-02-PLAN.md -- Provider crate migration (5 crates) to typed ProviderErrorKind
- [ ] 46-03-PLAN.md -- Channel/storage/MCP/skill migration to typed sub-enums + extended capabilities
- [ ] 46-04-PLAN.md -- FormatPipeline Table/List + error consumer updates + comprehensive tests

### Phase 49: FormatPipeline Integration
**Goal**: Every channel adapter uses FormatPipeline to format outbound messages, with content splitting at paragraph boundaries and adapter-specific rendering applied after degradation
**Depends on**: Phase 46 (requires extended ChannelCapabilities and Table/List content types)
**Requirements**: FMT-04, FMT-05, FMT-06, CAP-04
**Success Criteria** (what must be TRUE):
  1. FormatPipeline is called inside each of the 8 channel adapters' `send()` methods, converting RichContent to channel-appropriate format before delivery
  2. Messages exceeding a channel's max_message_length are split at paragraph boundaries, not mid-sentence
  3. Adapter-specific formatting (Telegram MarkdownV2, Slack mrkdwn, Discord Markdown, etc.) is applied after FormatPipeline degradation, not before
  4. All 8 channel adapters report accurate extended capability fields (streaming_type, formatting_support, rate_limit)
**Plans:** 4 plans
Plans:
- [ ] 46-01-PLAN.md -- Core error types, sub-enums, classification methods, ErrorContext, ChannelCapabilities extension
- [ ] 46-02-PLAN.md -- Provider crate migration (5 crates) to typed ProviderErrorKind
- [ ] 46-03-PLAN.md -- Channel/storage/MCP/skill migration to typed sub-enums + extended capabilities
- [ ] 46-04-PLAN.md -- FormatPipeline Table/List + error consumer updates + comprehensive tests

### Phase 50: ADRs & Documentation
**Goal**: Architectural decisions for ORT RC pinning and plugin architecture are formally documented with rationale, trade-offs, and upgrade plans
**Depends on**: Nothing (can run in parallel with any phase)
**Requirements**: DOC-01, DOC-02
**Success Criteria** (what must be TRUE):
  1. An ADR exists documenting why ORT is pinned at rc.11 over Candle, the trade-offs of each approach, and a concrete upgrade plan for when stable 2.0.0 lands
  2. An ADR exists documenting the Phase 1 compiled-in plugin architecture, why dynamic loading was deferred, and the migration path to libloading in the future
**Plans:** 4 plans
Plans:
- [ ] 46-01-PLAN.md -- Core error types, sub-enums, classification methods, ErrorContext, ChannelCapabilities extension
- [ ] 46-02-PLAN.md -- Provider crate migration (5 crates) to typed ProviderErrorKind
- [ ] 46-03-PLAN.md -- Channel/storage/MCP/skill migration to typed sub-enums + extended capabilities
- [ ] 46-04-PLAN.md -- FormatPipeline Table/List + error consumer updates + comprehensive tests

## Progress

**Execution Order:**
Phases execute in numeric order: 46 -> 47 -> 48 -> 49 -> 50
Note: Phase 47 is independent and can execute in parallel with Phase 46. Phase 50 can execute in parallel with any phase.

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Project Foundation & Workspace | v1.0 | 2/2 | Complete | 2026-02-28 |
| 2. Persistence & Security Vault | v1.0 | 2/2 | Complete | 2026-02-28 |
| 3. Agent Loop & Telegram | v1.0 | 4/4 | Complete | 2026-03-01 |
| 4. Context Engine & Cost Tracking | v1.0 | 3/3 | Complete | 2026-03-01 |
| 5. Memory & Embeddings | v1.0 | 3/3 | Complete | 2026-03-01 |
| 6. Model Routing & Smart Heartbeats | v1.0 | 3/3 | Complete | 2026-03-01 |
| 7. WASM Skill Sandbox | v1.0 | 4/4 | Complete | 2026-03-01 |
| 8. Plugin System & Gateway | v1.0 | 3/3 | Complete | 2026-03-01 |
| 9. Production Hardening | v1.0 | 3/3 | Complete | 2026-03-01 |
| 10. Multi-Agent & Final Integration | v1.0 | 3/3 | Complete | 2026-03-01 |
| 11. Fix Critical Integration Bugs | v1.0 | 4/4 | Complete | 2026-03-01 |
| 12. Verify Unverified Phases | v1.0 | 5/5 | Complete | 2026-03-01 |
| 13. Sync Traceability & Documentation | v1.0 | 1/1 | Complete | 2026-03-02 |
| 14. Wire Cross-Phase Integration | v1.0 | 3/3 | Complete | 2026-03-02 |
| 15. MCP Foundation | v1.1 | 4/4 | Complete | 2026-03-02 |
| 16. MCP Server stdio | v1.1 | 3/3 | Complete | 2026-03-02 |
| 17. MCP Server HTTP + Resources | v1.1 | 5/5 | Complete | 2026-03-02 |
| 18. MCP Client | v1.1 | 4/4 | Complete | 2026-03-03 |
| 19. Integration Testing + Tech Debt | v1.1 | 5/5 | Complete | 2026-03-03 |
| 20. Verify Phase 15 & 16 Completeness | v1.1 | 4/4 | Complete | 2026-03-03 |
| 21. Fix MCP Wiring Gaps | v1.1 | 4/4 | Complete | 2026-03-03 |
| 22. Verify Phase 18 & 19 + Close Traceability | v1.1 | 3/3 | Complete | 2026-03-03 |
| 23. Backup Integrity Verification | v1.2 | 1/1 | Complete | 2026-03-03 |
| 24. sd_notify Integration | v1.2 | 2/2 | Complete | 2026-03-03 |
| 25. SQLCipher Database Encryption | v1.2 | 4/4 | Complete | 2026-03-03 |
| 26. Minisign Signature Verification | v1.2 | 2/2 | Complete | 2026-03-03 |
| 27. Self-Update with Rollback | v1.2 | 2/2 | Complete | 2026-03-03 |
| 28. Close Audit Gaps | v1.2 | 2/2 | Complete | 2026-03-04 |
| 29. Event Bus & Core Trait Extensions | v1.3 | 2/2 | Complete | 2026-03-05 |
| 30. Multi-Provider LLM Support | v1.3 | 4/4 | Complete | 2026-03-05 |
| 31. OpenAI-Compatible Gateway API | v1.3 | 3/3 | Complete | 2026-03-05 |
| 32. Scoped API Keys, Webhooks & Batch | v1.3 | 3/3 | Complete | 2026-03-06 |
| 33. Discord & Slack Channel Adapters | v1.3 | 3/3 | Complete | 2026-03-06 |
| 34. WhatsApp, Signal, IRC & Matrix Adapters | v1.3 | 5/5 | Complete | 2026-03-06 |
| 35. Skill Registry & Code Signing | v1.3 | 2/2 | Complete | 2026-03-06 |
| 36. Docker Image & Deployment | v1.3 | 2/2 | Complete | 2026-03-07 |
| 37. Node System | v1.3 | 3/3 | Complete | 2026-03-07 |
| 38. Migration & CLI Utilities | v1.3 | 2/2 | Complete | 2026-03-07 |
| 39. Integration Verification | v1.3 | 7/7 | Complete | 2026-03-07 |
| 40. Wire Global EventBus & Bridge | v1.3 | 2/2 | Complete | 2026-03-07 |
| 41. Wire ProviderRegistry into Gateway | v1.3 | 2/2 | Complete | 2026-03-07 |
| 42. Wire Gateway Stores | v1.3 | 2/2 | Complete | 2026-03-07 |
| 43. Wire EventBus Event Publishers | v1.3 | 1/1 | Complete | 2026-03-08 |
| 44. Node Approval Wiring | v1.3 | 2/2 | Complete | 2026-03-08 |
| 45. Documentation & Traceability Sync | v1.3 | 2/2 | Complete | 2026-03-08 |
| 46. Core Types & Error Hierarchy | 3/4 | In Progress|  | - |
| 47. Accurate Token Counting | v1.4 | 0/? | Not started | - |
| 48. Circuit Breaker & Degradation Ladder | v1.4 | 0/? | Not started | - |
| 49. FormatPipeline Integration | v1.4 | 0/? | Not started | - |
| 50. ADRs & Documentation | v1.4 | 0/? | Not started | - |

---
*Roadmap created: 2026-02-28*
*Last updated: 2026-03-08 after v1.4 Quality & Resilience roadmap creation*
