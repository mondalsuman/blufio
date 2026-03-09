# Requirements: Blufio

**Defined:** 2026-03-08
**Core Value:** An always-on personal AI agent that is secure enough to trust, efficient enough to afford, and simple enough to deploy by copying one file.

## v1.4 Requirements

Requirements for v1.4 Quality & Resilience milestone. Each maps to roadmap phases.

### Error Hierarchy

- [x] **ERR-01**: BlufioError exposes `is_retryable()` method returning bool based on error variant
- [x] **ERR-02**: BlufioError exposes `severity()` method returning Severity enum (Critical, Error, Warning, Info)
- [x] **ERR-03**: BlufioError exposes `category()` method returning structured category (Provider(name), Channel(name), Storage, Config, Security)
- [x] **ERR-04**: Provider errors distinguish between RateLimited, AuthFailed, ServerError, Timeout, and ModelNotFound variants
- [x] **ERR-05**: Channel errors distinguish between DeliveryFailed, ConnectionLost, and RateLimited variants

### Circuit Breaker

- [x] **CB-01**: CircuitBreaker implements 3-state FSM (Closed, Open, HalfOpen) with configurable thresholds
- [x] **CB-02**: Each external dependency (5 providers, 8 channels) has an independent circuit breaker instance
- [x] **CB-03**: Circuit breaker uses `is_retryable()` from typed errors — non-retryable errors (auth, config) do not count as failures
- [x] **CB-04**: Circuit breaker publishes state transitions to EventBus as Resilience events
- [x] **CB-05**: Prometheus gauge `blufio_circuit_breaker_state` emitted per dependency with state label
- [x] **CB-06**: Circuit breaker thresholds configurable via TOML config `[resilience.circuit_breakers.<name>]`
- [x] **CB-07**: HalfOpen state allows a configurable number of probe requests before transitioning to Closed

### Degradation Ladder

- [x] **DEG-01**: DegradationLevel enum with 6 levels (L0 FullyOperational through L5 SafeShutdown)
- [x] **DEG-02**: DegradationManager tracks current level and auto-escalates based on circuit breaker state changes
- [x] **DEG-03**: De-escalation uses hysteresis timer — level drops only after sustained recovery period
- [x] **DEG-04**: Degradation state changes published to EventBus and visible via `/v1/health` API
- [ ] **DEG-05**: User-facing degradation messages delivered to primary channel at each level transition
- [ ] **DEG-06**: Configurable fallback provider via `[resilience.fallback_provider]` activated at L2+

### Token Counting

- [x] **TOK-01**: Context engine uses accurate token counting instead of `len()/4` heuristic
- [x] **TOK-02**: OpenAI token counting uses tiktoken-rs with o200k_base/cl100k_base encodings per model
- [x] **TOK-03**: Claude token counting uses HuggingFace tokenizers crate with Xenova/claude-tokenizer vocabulary
- [x] **TOK-04**: Ollama token counting uses per-model tokenizer.json when available, calibrated heuristic as fallback
- [x] **TOK-05**: Gemini token counting uses calibrated heuristic (no local tokenizer available)
- [x] **TOK-06**: OpenRouter token counting delegates to underlying model's tokenizer based on model ID
- [x] **TOK-07**: Tokenizer instances lazy-loaded and reused across calls (not created per-request)
- [x] **TOK-08**: Token counting runs via `spawn_blocking` to avoid blocking tokio worker threads
- [x] **TOK-09**: Config toggle `[performance.tokenizer_mode]` allows "accurate" (default) vs "fast" (heuristic) mode

### Content Formatting

- [x] **FMT-01**: FormatPipeline extended with Table and BulletList/OrderedList RichContent variants
- [x] **FMT-02**: Table content degrades to aligned text for channels without table support
- [x] **FMT-03**: List content renders as channel-native list format or plain text fallback
- [ ] **FMT-04**: FormatPipeline called inside each channel adapter's `send()` method
- [ ] **FMT-05**: Message length splitting integrated — content split at paragraph boundaries respecting `max_message_length`
- [ ] **FMT-06**: Adapter-specific formatting (MarkdownV2, mrkdwn, etc.) applied after FormatPipeline degradation

### Channel Capabilities Extension

- [x] **CAP-01**: ChannelCapabilities extended with `streaming_type` field (enum: None, BlockOnly, FullStreaming)
- [x] **CAP-02**: ChannelCapabilities extended with `formatting_support` field (enum: PlainText, BasicMarkdown, FullMarkdown, HTML)
- [x] **CAP-03**: ChannelCapabilities extended with `rate_limit` field (struct: messages_per_second, requests_per_minute)
- [ ] **CAP-04**: All 8 channel adapters updated to report extended capability fields accurately

### Documentation

- [ ] **DOC-01**: ADR documenting ORT chosen over Candle — rationale, trade-offs, RC pin justification, upgrade plan
- [ ] **DOC-02**: ADR documenting plugin architecture — Phase 1 compiled-in vs future dynamic loading plan

## Future Requirements

Deferred to future release. Tracked but not in current roadmap.

### Advanced Resilience

- **ARES-01**: Cascading circuit breaker coordination across dependent services
- **ARES-02**: Automated provider failover chain (Anthropic → OpenAI → Ollama)
- **ARES-03**: Degradation level L4 cached response store

### Advanced Formatting

- **AFMT-01**: Interactive element support (inline buttons, quick replies) in FormatPipeline
- **AFMT-02**: Media type granularity in ChannelCapabilities (formats, max sizes, audio codecs)
- **AFMT-03**: Streaming-aware content formatting (progressive rendering)

### Plugin Architecture

- **PLUG-01**: Dynamic plugin loading via libloading
- **PLUG-02**: Plugin manifest with semver compatibility
- **PLUG-03**: Hot-reload with state preservation
- **PLUG-04**: `blufio plugin install` from registry

## Out of Scope

| Feature | Reason |
|---------|--------|
| ORT upgrade to stable | No stable 2.0.0 exists yet — ADR documents rationale, upgrade when available |
| Native plugin loading | Phase 3+ feature — current compiled-in approach is Phase 1 MVP |
| Anthropic official tokenizer | Proprietary — using community Xenova/claude-tokenizer (~80-95% accuracy) |
| Gemini local tokenizer | Google publishes no local tokenizer — calibrated heuristic only option |
| Response caching at L4 degradation | Requires new storage table — defer to Advanced Resilience |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| ERR-01 | Phase 46 | Complete |
| ERR-02 | Phase 46 | Complete |
| ERR-03 | Phase 46 | Complete |
| ERR-04 | Phase 46 | Complete |
| ERR-05 | Phase 46 | Complete |
| CAP-01 | Phase 46 | Complete |
| CAP-02 | Phase 46 | Complete |
| CAP-03 | Phase 46 | Complete |
| FMT-01 | Phase 46 | Complete |
| FMT-02 | Phase 46 | Complete |
| FMT-03 | Phase 46 | Complete |
| TOK-01 | Phase 47 | Complete |
| TOK-02 | Phase 47 | Complete |
| TOK-03 | Phase 47 | Complete |
| TOK-04 | Phase 47 | Complete |
| TOK-05 | Phase 47 | Complete |
| TOK-06 | Phase 47 | Complete |
| TOK-07 | Phase 47 | Complete |
| TOK-08 | Phase 47 | Complete |
| TOK-09 | Phase 47 | Complete |
| CB-01 | Phase 48 | Complete |
| CB-02 | Phase 48 | Complete |
| CB-03 | Phase 48 | Complete |
| CB-04 | Phase 48 | Complete |
| CB-05 | Phase 48 | Complete |
| CB-06 | Phase 48 | Complete |
| CB-07 | Phase 48 | Complete |
| DEG-01 | Phase 48 | Complete |
| DEG-02 | Phase 48 | Complete |
| DEG-03 | Phase 48 | Complete |
| DEG-04 | Phase 48 | Complete |
| DEG-05 | Phase 48 | Pending |
| DEG-06 | Phase 48 | Pending |
| FMT-04 | Phase 49 | Pending |
| FMT-05 | Phase 49 | Pending |
| FMT-06 | Phase 49 | Pending |
| CAP-04 | Phase 49 | Pending |
| DOC-01 | Phase 50 | Pending |
| DOC-02 | Phase 50 | Pending |

**Coverage:**
- v1.4 requirements: 39 total
- Mapped to phases: 39
- Unmapped: 0

---
*Requirements defined: 2026-03-08*
*Last updated: 2026-03-08 after roadmap creation -- all 39 requirements mapped to phases*
