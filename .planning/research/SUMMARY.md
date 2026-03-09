# Research Summary: Blufio v1.4 Quality & Resilience

**Domain:** Quality improvements and resilience hardening for Rust AI agent platform
**Researched:** 2026-03-08
**Overall confidence:** HIGH

## Executive Summary

Blufio v1.4 is a quality and resilience milestone for a production Rust AI agent platform (71,808 LOC, 35 crates, 219 verified requirements across v1.0-v1.3). The goal is to address QA audit deviations: replace the inaccurate `len()/4` token estimation heuristic with real tokenizer-backed counting, add circuit breakers for all external dependencies, implement a 6-level graceful degradation ladder, create a typed error hierarchy enabling automated retry decisions, wire the existing FormatPipeline into all 8 channel adapters, extend ChannelCapabilities with streaming/formatting/rate-limit metadata, and document the ORT RC pinning decision via an ADR.

The technology research reveals that v1.4 requires only **one new workspace dependency** (`tiktoken-rs` 0.9.1 for OpenAI token counting). Everything else either reuses existing workspace crates (`tokenizers` 0.21 for Claude/Ollama counting, `thiserror` 2 for error hierarchy, `metrics` 0.24 for observability) or requires no external crate at all (circuit breaker is ~200 LOC of custom Rust using atomics, degradation ladder is a state machine over existing primitives). This is a depth milestone -- improving what exists rather than adding new subsystems.

The most critical technical decision is the multi-provider token counting strategy. No single crate handles all providers: `tiktoken-rs` covers OpenAI models exclusively (o200k_base, cl100k_base encodings), while the HuggingFace `tokenizers` crate (already in the workspace for ONNX embedding) can load Claude's tokenizer.json from the Xenova/claude-tokenizer HuggingFace repository. For Gemini, Google publishes no tokenizer -- a calibrated heuristic is the only offline option. The dual-crate approach (tiktoken-rs + tokenizers) is more work than a single crate but achieves accurate counting across all 5 supported providers.

For circuit breakers, all Rust crates evaluated (`failsafe` 1.3.0, `circuitbreaker-rs` 0.1.1, `tower-circuitbreaker` 0.2.0) were rejected because Blufio's adapter calls are async trait methods through `dyn` dispatch -- not tower `Service` implementations. A custom ~200 LOC implementation using `std::sync::atomic` and the existing `DashMap` + `metrics` crates is the cleanest fit. The ORT crate remains pinned at `=2.0.0-rc.11` because no stable 2.0.0 exists yet and rc.12's breaking API renames offer zero functional benefit.

## Key Findings

**Stack:** Only 1 new crate (`tiktoken-rs` 0.9.1); everything else is existing workspace deps or custom code
**Architecture:** Modify existing crates only -- no new crate creation; `blufio-core` for errors/types, `blufio-context` for token counting, `blufio-agent` for circuit breaker + degradation
**Critical pitfall:** Token counting on the async hot path blocks tokio worker threads -- `tokenizer.encode()` is synchronous and must use `tokio::task::spawn_blocking` or batch outside async context

## Implications for Roadmap

Based on research, suggested phase structure:

1. **Core Types & Error Hierarchy** - Foundation phase
   - Addresses: Typed error hierarchy (`is_retryable()`, `severity()`, `category()`), ChannelCapabilities extension (streaming_type, formatting_support, rate_limits), FormatPipeline Table/List content types
   - Avoids: Breaking existing error match arms (additive methods only); changing OutboundMessage shape (which would require 8 adapter changes simultaneously)
   - Rationale: All downstream phases depend on enriched error types for circuit breaker decisions and extended capabilities for format pipeline wiring

2. **Accurate Token Counting** - Replace len()/4 heuristic
   - Addresses: Multi-provider token counting (Claude via tokenizers + Xenova, OpenAI via tiktoken-rs, Ollama via per-model tokenizer.json, Gemini via calibrated heuristic)
   - Avoids: Blocking tokio worker threads with synchronous tokenizer.encode() calls (Pitfall 1); inflating binary with include_bytes!() for all tokenizer files
   - Rationale: Independent of circuit breaker/degradation work; can be developed and tested in isolation

3. **Circuit Breaker + Degradation Ladder** - Resilience infrastructure
   - Addresses: Per-dependency circuit breakers, 6-level degradation ladder, EventBus resilience events, Prometheus metrics
   - Avoids: Modifying ProviderAdapter trait (Anti-Pattern 1); global circuit breaker treating all deps as one (Anti-Pattern 4); degradation as Tower middleware (Anti-Pattern 5)
   - Rationale: Depends on typed errors (Phase 1) for `is_retryable()` decisions; publishes events to bus (existing)

4. **FormatPipeline Integration** - Wire into all channel adapters
   - Addresses: FormatPipeline called in each adapter's send(), Table/List rendering per channel capabilities, consistent content degradation
   - Avoids: Changing OutboundMessage type (Anti-Pattern 3); modifying agent loop for formatting (formatting belongs in adapters)
   - Rationale: Leaf-crate changes; 8 adapters can be updated in parallel; depends on Phase 1 for extended ChannelCapabilities

5. **ORT ADR + Documentation** - Decision record
   - Addresses: ORT pinning rationale, upgrade migration plan when stable 2.0.0 lands, plugin architecture ADR
   - Avoids: Unnecessary rc.11->rc.12 upgrade (zero functional benefit, stable expected soon)
   - Rationale: Documentation phase; no code changes for ORT; can run in parallel with any other phase

**Phase ordering rationale:**
- Types/errors first because circuit breaker needs `is_retryable()` and format pipeline needs extended ChannelCapabilities
- Token counting is independent and can run in parallel with any phase after Phase 1
- Circuit breaker depends on typed errors but not on token counting
- FormatPipeline integration is leaf-crate work with no downstream dependents -- safest to do last
- ORT ADR is documentation only and can be written at any time

**Research flags for phases:**
- Phase 2 (Token Counting): The Xenova/claude-tokenizer is a community artifact, not official Anthropic. Accuracy for Claude 3/4 models is ~80-95%, not 100%. This should be documented in the implementation and calibrated against the Anthropic count_tokens API.
- Phase 3 (Circuit Breaker): Standard pattern, unlikely to need research. Custom implementation is straightforward.
- Phase 5 (ORT ADR): Monitor pykeio/ort GitHub for stable 2.0.0 announcement. If it lands during v1.4 development, upgrade instead of writing an ADR for the RC pin.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | tiktoken-rs 0.9.1 verified on crates.io; tokenizers 0.21 already in workspace; no new deps for circuit breaker/degradation |
| Features | HIGH | All features well-defined in PROJECT.md Active requirements; clear scope with no ambiguity |
| Architecture | HIGH | Based on direct source code analysis; all integration points verified against existing crate boundaries |
| Pitfalls | HIGH | Verified against codebase (tokio blocking, thiserror derive, ort API changes); circuit breaker patterns are well-established |

## Gaps to Address

- **Claude tokenizer accuracy on Claude 3/4 models:** The Xenova/claude-tokenizer matches Claude 2.x exactly but Claude 3+ may use a different vocabulary. The ctoc project (grohan.co) achieved ~96% accuracy by reverse-engineering Claude 4.x's count_tokens API with 36K verified tokens. Consider using ctoc's vocabulary if Xenova's accuracy proves insufficient in testing. Calibrate against the free Anthropic count_tokens API endpoint.

- **tiktoken-rs binary size impact:** tiktoken-rs 0.9.1 embeds BPE vocabulary data. Measure the actual binary size impact before committing -- the project has a <50MB binary constraint. If the impact exceeds 2MB, consider lazy-loading the vocabulary from a downloaded file instead.

- **ORT stable 2.0.0 timing:** The ort maintainer stated stable was imminent in the rc.11 release notes (January 2025). As of March 2026, it still has not shipped. If stable 2.0.0 lands during v1.4 development, pivot from "write ADR for RC pin" to "upgrade to stable + migration notes."

- **Degradation ladder level definitions:** The exact behavior at each degradation level (what gets disabled, what models are available, what responses are served) needs to be specified during phase planning. Research provides the 6-level framework; concrete feature toggles per level depend on operator preferences and should be configurable.
