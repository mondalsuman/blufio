# Phase 39: Integration Verification - Research

**Researched:** 2026-03-07
**Domain:** Verification / Integration Testing / Traceability Audit (Rust, cargo test, wiremock, Docker)
**Confidence:** HIGH

## Summary

Phase 39 is a verification-only phase that validates all v1.3 requirements (phases 29-38), writes cross-feature integration tests, audits Docker deployment, and completes traceability documentation. No new features are implemented -- incomplete/stub implementations are flagged as UNVERIFIED, and missing test coverage is filled as part of the verification process.

The project has 71 v1.3 requirements across 8 categories (API: 18, PROV: 14, CHAN: 12, INFRA: 7, SKILL: 5, NODE: 5, MIGR: 5, CLI: 5). Currently 29 are marked Complete in the traceability table and 42 are Pending. Three phases have existing VERIFICATION.md files (30, 37, 38); seven phases need new verification reports (29, 31, 32, 33, 34, 35, 36). The entire workspace compiles and all 1,410 tests pass across 34 crates (69,818 LOC).

**Primary recommendation:** Structure the work as (1) per-phase VERIFICATION.md generation with test gap filling, (2) cross-feature integration flow tests in blufio-test-utils, (3) Docker verification in 36-VERIFICATION.md, (4) traceability audit and documentation updates, (5) final readiness summary. Each verification report follows Phase 30's format (YAML frontmatter, Observable Truths table, Required Artifacts, score line).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Generate per-phase VERIFICATION.md for all phases missing them (29, 31-36) in the same format as Phase 30's existing report
- Full re-verification of ALL phases including existing reports (30, 37, 38) -- nothing trusted as-is
- Evidence level: code + test evidence per requirement (source file + line implementing it, test(s) covering it)
- Run `cargo test` AND read source/test code -- both required for verification evidence
- Audit AND fill test gaps: if a requirement lacks test coverage, write the missing test as part of verification
- Incomplete/stub implementations flagged as UNVERIFIED with detail -- do not attempt to implement missing features
- Score per phase: each VERIFICATION.md gets a score like "9/9 verified" matching Phase 30's format
- 100% pass threshold required -- all requirements must be VERIFIED for milestone to pass
- 4 specific E2E integration flows (all mocked, no real external services):
  1. OpenAI SDK -> chat completions -> OpenRouter provider -> Discord channel -> webhook delivery
  2. Ollama local -> chat completions -> Telegram -> event bus
  3. Scoped API key -> rate limit -> chat completions -> Gemini -> batch processing
  4. Skill install -> verify signature -> execute -> cost tracking
- Rust integration tests (#[tokio::test]) in blufio-test-utils crate
- Full performance profiling per flow step (latency/throughput metrics)
- Each flow independent -- failure in one doesn't block others, all failures collected in report
- Results in separate 39-INTEGRATION.md (not in per-phase VERIFICATION.md files)
- Docker verification goes in Phase 36's 36-VERIFICATION.md (not Phase 39)
- Actually build the Docker image (requires Docker daemon)
- Verify: multi-stage build correctness, docker-compose.yml validity, image size, healthcheck endpoint
- 200MB soft target for image size -- flag if exceeded, don't fail verification
- Build failure flagged as UNVERIFIED, doesn't block rest of verification
- Update REQUIREMENTS.md: check off verified requirements AND update Traceability table status from "Pending" to "Verified"
- Full requirement text in traceability table (not just IDs)
- Status table + coverage stats per category + overall percentage
- Update PROJECT.md: move verified v1.3 requirements from Active to Validated
- Update STATE.md: mark milestone as verified
- Update PROJECT.md stats: total LOC, test count, crate count, requirement count
- Produce 39-SUMMARY.md: high-level milestone readiness document
- Produce 39-INTEGRATION.md: cross-feature flow results

### Claude's Discretion
- Exact wiremock setup for integration flow tests
- Performance profiling instrumentation details
- How to organize per-phase verification agents (parallel vs sequential)
- Exact format of 39-SUMMARY.md readiness document

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

This is a verification phase that validates all requirements from phases 29-38. There are no unique requirement IDs for Phase 39 itself. Instead, it validates:

| Phase | Requirements | Count | Has VERIFICATION.md |
|-------|-------------|-------|---------------------|
| 29 | INFRA-01, INFRA-02, INFRA-03, PROV-10, PROV-11, PROV-12, PROV-13, PROV-14 | 8 | No -- needs creation |
| 30 | PROV-01 through PROV-09 | 9 | Yes -- needs re-verification |
| 31 | API-01 through API-10 | 10 | No -- needs creation |
| 32 | API-11 through API-18 | 8 | No -- needs creation |
| 33 | CHAN-01 through CHAN-05, CHAN-11, CHAN-12 | 7 | No -- needs creation |
| 34 | CHAN-06 through CHAN-10, INFRA-06 | 6 | No -- needs creation |
| 35 | SKILL-01 through SKILL-05 | 5 | No -- needs creation |
| 36 | INFRA-04, INFRA-05, INFRA-07 | 3 | No -- needs creation (includes Docker verification) |
| 37 | NODE-01 through NODE-05 | 5 | Yes -- needs re-verification (score was 17/19, gaps found) |
| 38 | MIGR-01 through MIGR-05, CLI-01 through CLI-05 | 10 | Yes -- needs re-verification |
| **Total** | | **71** | |

Note: CONTEXT.md says 69 requirements, but actual REQUIREMENTS.md count is **71**. The discrepancy is 2 requirements. All 71 must be verified.
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| cargo test | Rust 1.85 | Test runner | Project's existing test framework |
| tokio | workspace | Async runtime for integration tests | Already used everywhere in project |
| wiremock | 0.6 | HTTP mock server for provider tests | Already dev-dependency in 5 provider crates |
| tempfile | 3 | Temp directories for integration test DBs | Already in blufio-test-utils |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| std::time::Instant | stdlib | Performance profiling per flow step | Measure latency for each integration step |
| tokio::time::Instant | workspace | Async timing | Async flow step measurement |
| serde_json | 1 | JSON assertion in integration tests | Verify wire format correctness |

### No New Dependencies Required
All integration testing can be accomplished with existing dependencies. The project already has:
- `wiremock` for HTTP mocking (provider crates)
- `tempfile` for temp SQLite databases (test-utils)
- `tokio` for async test runtime
- `MockProvider` and `MockChannel` in blufio-test-utils
- `TestHarness` for full agent pipeline testing

## Architecture Patterns

### Verification Report Format (Gold Standard: Phase 30)

Each per-phase VERIFICATION.md follows this structure:

```yaml
---
phase: {phase-number}-{phase-name}
verified: {ISO-8601}
status: passed | gaps_found
score: X/Y must-haves verified
re_verification: true  # for phases 30, 37, 38
gaps:  # only if status == gaps_found
  - truth: "..."
    status: failed | partial
    reason: "..."
---
```

Followed by sections:
1. **Goal Achievement** -- Observable Truths table (from PLAN must_haves)
2. **Required Artifacts** -- per-plan artifact verification
3. **Key Link Verification** -- cross-file wiring checks
4. **Requirements Coverage** -- requirement ID -> evidence mapping
5. **Anti-Patterns Found** -- TODO/FIXME/HACK/placeholder scan
6. **Human Verification Required** -- behaviors needing live environment
7. **Gaps Summary** -- what passed, what failed
8. **Test Summary** -- per-crate test counts

### Integration Test Structure in blufio-test-utils

```
crates/blufio-test-utils/
  src/
    lib.rs              # exports: TestHarness, MockProvider, MockChannel
    harness.rs          # full agent pipeline test environment
    mock_channel.rs     # ChannelAdapter mock with inject/capture
    mock_provider.rs    # ProviderAdapter mock with response queue
  tests/
    integration_flows.rs  # NEW: 4 E2E integration flow tests
```

Each integration flow test follows the pattern:
```rust
#[tokio::test]
async fn flow_openai_sdk_to_webhook_delivery() {
    let start = std::time::Instant::now();
    // 1. Set up wiremock for OpenRouter provider
    // 2. Set up gateway with OpenAI compat endpoint
    // 3. Create scoped API key
    // 4. Register webhook
    // 5. Send chat completion request through OpenAI compat
    // 6. Assert: provider received request (wiremock verified)
    // 7. Assert: Discord channel received message
    // 8. Assert: webhook delivery fired with HMAC signature
    // 9. Record per-step timing metrics
    let elapsed = start.elapsed();
}
```

### Per-Phase Verification Methodology

For each phase (29-38):
1. Read all PLAN.md files to extract must_haves (truths, artifacts, key_links)
2. Read SUMMARY.md files to understand what was implemented
3. Run `cargo test -p {crate}` for each relevant crate
4. Read source code to verify Observable Truths with file:line evidence
5. Identify test gaps and write missing tests
6. Write VERIFICATION.md with score
7. Flag any UNVERIFIED items with detailed explanation

### Requirement-to-Phase Mapping (From REQUIREMENTS.md Traceability)

| Category | Requirements | Phase(s) | Current Checkbox Status |
|----------|-------------|----------|------------------------|
| API | API-01 to API-10 | Phase 31 | Unchecked |
| API | API-11 to API-18 | Phase 32 | Unchecked |
| Providers | PROV-01 to PROV-09 | Phase 30 | Checked |
| Providers | PROV-10 to PROV-14 | Phase 29 | Unchecked |
| Channels | CHAN-01 to CHAN-05, CHAN-11, CHAN-12 | Phase 33 | CHAN-11, CHAN-12 checked; rest unchecked |
| Channels | CHAN-06 to CHAN-10 | Phase 34 | Unchecked |
| Infrastructure | INFRA-01 to INFRA-03 | Phase 29 | Unchecked |
| Infrastructure | INFRA-04, INFRA-05, INFRA-07 | Phase 36 | Checked |
| Infrastructure | INFRA-06 | Phase 34 | Unchecked |
| Skills | SKILL-01 to SKILL-05 | Phase 35 | Unchecked |
| Nodes | NODE-01 to NODE-05 | Phase 37 | Checked |
| Migration | MIGR-01 to MIGR-05 | Phase 38 | Checked |
| CLI | CLI-01 to CLI-05 | Phase 38 | Checked |

### Documentation Update Pattern

Three files must be updated atomically at the end:

1. **REQUIREMENTS.md**: Change `- [ ]` to `- [x]` for each verified requirement. Update traceability table status from "Pending" to "Verified" (or "Complete" which is already used). Add full requirement text to traceability table if not present.

2. **PROJECT.md**: Move items from `### Active` to `### Validated`. Update stats (LOC: 69,818+, tests: 1,410+, crates: 34).

3. **STATE.md**: Update `status: completed` -> `verified`, update `stopped_at`, update `progress.percent: 100`.

### Output Files

| File | Location | Purpose |
|------|----------|---------|
| 29-VERIFICATION.md | .planning/phases/29-*/ | Event bus + core traits verification |
| 30-VERIFICATION.md | .planning/phases/30-*/ | Multi-provider verification (re-verify) |
| 31-VERIFICATION.md | .planning/phases/31-*/ | Gateway API verification |
| 32-VERIFICATION.md | .planning/phases/32-*/ | API keys/webhooks/batch verification |
| 33-VERIFICATION.md | .planning/phases/33-*/ | Discord/Slack verification |
| 34-VERIFICATION.md | .planning/phases/34-*/ | WhatsApp/Signal/IRC/Matrix verification |
| 35-VERIFICATION.md | .planning/phases/35-*/ | Skill registry verification |
| 36-VERIFICATION.md | .planning/phases/36-*/ | Docker verification (includes Docker build) |
| 37-VERIFICATION.md | .planning/phases/37-*/ | Node system verification (re-verify, fix 17/19 score) |
| 38-VERIFICATION.md | .planning/phases/38-*/ | Migration/CLI verification (re-verify) |
| 39-INTEGRATION.md | .planning/phases/39-*/ | Cross-feature integration flow results |
| 39-SUMMARY.md | .planning/phases/39-*/ | Milestone readiness summary |

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| HTTP mocking | Custom mock HTTP server | wiremock 0.6 (already in project) | Handles concurrent tests, request matching, response templating |
| Test database setup | Manual SQLite init | TestHarness::builder() in blufio-test-utils | Already handles temp DB, migrations, all subsystems |
| Mock LLM responses | Manual HTTP response crafting | MockProvider in blufio-test-utils | Already implements ProviderAdapter with response queue |
| Mock channel messages | Manual channel simulation | MockChannel in blufio-test-utils | Already implements ChannelAdapter with inject/capture |
| Performance timing | Custom profiling framework | std::time::Instant + simple recording | No need for flamegraphs or tracing -- just step latencies |
| HMAC verification in tests | Manual hex computation | hmac + sha2 crates (already in blufio-gateway) | Already used for webhook signing |

## Common Pitfalls

### Pitfall 1: Requirement Count Mismatch
**What goes wrong:** CONTEXT.md says "69 requirements" but REQUIREMENTS.md has 71.
**Why it happens:** Possible counting error in discussion or late additions.
**How to avoid:** Use the authoritative REQUIREMENTS.md count of 71. Verify all 71.
**Warning signs:** Score percentages don't add up to 100%.

### Pitfall 2: Phase 37 Has Known Gaps (17/19)
**What goes wrong:** Phase 37 VERIFICATION.md already documented 2 gaps -- ApprovalRouter event bus subscription and ConnectionManager approval forwarding.
**Why it happens:** Implementation was incomplete in these areas.
**How to avoid:** Re-verify Phase 37 honestly. If gaps still exist, flag as UNVERIFIED. The 100% threshold means either these must be fixed as test gap filling or acknowledged as verification failures.
**Warning signs:** Score stays at 17/19 instead of 19/19 after re-verification.

### Pitfall 3: Docker Not Available on Build Machine
**What goes wrong:** The CONTEXT requires actually building the Docker image, but Docker may not be installed.
**Why it happens:** macOS development machine may not have Docker Desktop installed.
**How to avoid:** The CONTEXT explicitly says "Build failure flagged as UNVERIFIED, doesn't block rest of verification." If Docker daemon is unavailable, perform static verification (Dockerfile syntax, docker-compose.yml validity via parsing) and flag the build as UNVERIFIED with reason "Docker daemon not available."
**Warning signs:** `docker --version` returns error. On this machine, Docker is NOT available.

### Pitfall 4: wiremock Port Conflicts in Parallel Tests
**What goes wrong:** Multiple wiremock MockServers bind to random ports but tests may conflict.
**Why it happens:** cargo test runs tests in parallel by default.
**How to avoid:** wiremock 0.6 allocates random ports per MockServer::start(). Each integration flow test should create its own MockServer instance. No shared state between flows.
**Warning signs:** Intermittent "address already in use" errors.

### Pitfall 5: Confusing Re-verification with Amendment
**What goes wrong:** Re-verifying phases 30, 37, 38 might accidentally overwrite good data or miss regressions.
**Why it happens:** VERIFICATION.md files already exist with detailed evidence.
**How to avoid:** Re-run all cargo tests. Re-read source to confirm evidence still holds. Update the YAML frontmatter to set `re_verification: true` and update the timestamp. Keep the same format.
**Warning signs:** Changing scores without re-running tests.

### Pitfall 6: Phase 32 Roadmap Shows "Not started" But Code Exists
**What goes wrong:** ROADMAP.md still shows Phase 32 as "Not started" but the code exists in blufio-gateway (api_keys, webhooks, batch modules).
**Why it happens:** ROADMAP.md was not updated after implementation.
**How to avoid:** Verify based on actual code presence, not roadmap status. Read the source files.
**Warning signs:** Assuming a phase is missing when the code is actually there.

### Pitfall 7: Integration Tests Need Crate Cross-Dependencies
**What goes wrong:** Integration tests in blufio-test-utils may need dependencies not currently in its Cargo.toml (e.g., blufio-gateway, blufio-bus, blufio-openrouter, blufio-discord, wiremock).
**Why it happens:** Current blufio-test-utils only depends on core, agent, config, context, cost, router, skill, storage.
**How to avoid:** Add needed dev-dependencies or runtime dependencies for the integration flow tests. Alternatively, place integration tests in a separate test binary or in the main blufio crate's tests/ directory.
**Warning signs:** Compilation errors when trying to import gateway/provider types in blufio-test-utils.

## Code Examples

### Verification Report YAML Frontmatter
```yaml
# Source: .planning/phases/30-multi-provider-llm-support/30-VERIFICATION.md
---
phase: 29-event-bus-core-trait-extensions
verified: 2026-03-07T18:00:00Z
status: passed
score: 8/8 must-haves verified
re_verification: false
---
```

### Observable Truths Table Row
```markdown
# Source: Phase 30 VERIFICATION.md format
| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Any component can publish typed events via Arc<EventBus> and broadcast subscribers receive them | VERIFIED | `crates/blufio-bus/src/lib.rs` lines 58-85; EventBus::publish() fans out to broadcast + mpsc; 12 tests passing |
```

### Integration Flow Test Pattern
```rust
// Source: Project pattern based on existing TestHarness in blufio-test-utils/src/harness.rs
#[tokio::test]
async fn flow_openai_sdk_openrouter_discord_webhook() {
    // Step 1: Start wiremock for OpenRouter
    let mock_server = wiremock::MockServer::start().await;
    let step1_time = std::time::Instant::now();

    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/api/v1/chat/completions"))
        .respond_with(wiremock::ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({
                "id": "chatcmpl-test",
                "choices": [{"message": {"content": "Hello from mock"}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 10, "completion_tokens": 5}
            })))
        .mount(&mock_server)
        .await;

    let step1_elapsed = step1_time.elapsed();
    // ... continue with gateway setup, Discord mock, webhook verification
}
```

### Performance Profiling Pattern
```rust
// Source: Pattern recommendation for integration flow step timing
struct FlowMetrics {
    flow_name: String,
    steps: Vec<(String, std::time::Duration)>,
    total: std::time::Duration,
}

impl FlowMetrics {
    fn record_step(&mut self, name: &str, duration: std::time::Duration) {
        self.steps.push((name.to_string(), duration));
    }

    fn to_markdown_table(&self) -> String {
        let mut table = format!("| Step | Latency |\n|------|--------|\n");
        for (name, dur) in &self.steps {
            table.push_str(&format!("| {} | {:.2}ms |\n", name, dur.as_secs_f64() * 1000.0));
        }
        table.push_str(&format!("| **Total** | **{:.2}ms** |\n", self.total.as_secs_f64() * 1000.0));
        table
    }
}
```

### REQUIREMENTS.md Checkbox Update Pattern
```markdown
# Before:
- [ ] **API-01**: User can send OpenAI-compatible chat completions via POST /v1/chat/completions

# After:
- [x] **API-01**: User can send OpenAI-compatible chat completions via POST /v1/chat/completions
```

### Traceability Table Update Pattern
```markdown
# Before:
| API-01 | Phase 31 | Pending |

# After:
| API-01 | Phase 31 | Verified |
```

## State of the Art

### Current Project State (Snapshot 2026-03-07)

| Metric | Value |
|--------|-------|
| Total Rust LOC | 69,818 |
| Total crates | 34 |
| Total tests passing | 1,410 |
| v1.3 requirements | 71 |
| Requirements with [x] | 29 |
| Requirements with [ ] | 42 |
| Traceability: Complete | 29 |
| Traceability: Pending | 42 |
| Phases with VERIFICATION.md | 3 (30, 37, 38) |
| Phases needing VERIFICATION.md | 7 (29, 31, 32, 33, 34, 35, 36) |

### Existing Verification Report Scores
| Phase | Score | Status | Notes |
|-------|-------|--------|-------|
| 30 | 9/9 | passed | Gold standard format |
| 37 | 17/19 | gaps_found | 2 gaps: ApprovalRouter bus subscription, ConnectionManager approval forwarding |
| 38 | 13/13 | passed | Clean pass |

### Key Implementation Artifacts by Phase

**Phase 29 (Event Bus + Core Traits):**
- `blufio-bus` crate: EventBus, BusEvent, 6 event domains
- `blufio-core` types: ToolDefinition, TtsAdapter, TranscriptionAdapter, ImageAdapter traits
- `blufio-config` model: CustomProviderConfig

**Phase 31 (Gateway API):**
- `blufio-gateway/src/openai_compat/`: handlers, types, stream, tools, responses
- OpenAI wire types separate from ProviderResponse

**Phase 32 (API Keys/Webhooks/Batch):**
- `blufio-gateway/src/api_keys/`: store, handlers (CRUD, scoped auth)
- `blufio-gateway/src/webhooks/`: delivery (HMAC-SHA256), store, handlers
- `blufio-gateway/src/batch/`: processor, store, handlers
- `blufio-gateway/src/rate_limit.rs`: sliding window rate limiter

**Phase 33 (Discord/Slack):**
- `blufio-discord` crate: serenity-based adapter
- `blufio-slack` crate: slack-morphism-based adapter

**Phase 34 (WhatsApp/Signal/IRC/Matrix + Bridging):**
- `blufio-whatsapp`, `blufio-signal`, `blufio-irc`, `blufio-matrix` crates
- `blufio-bridge` crate: cross-channel bridging

**Phase 35 (Skill Registry):**
- `blufio-skill/src/signing.rs`: Ed25519 code signing
- `blufio-skill/src/store.rs`: manifest store with SHA-256 hashes
- `blufio-skill/src/sandbox.rs`: pre-execution verification gate

**Phase 36 (Docker):**
- `Dockerfile`: multi-stage build, distroless cc-debian12:nonroot
- `docker-compose.yml`: volumes, env, healthcheck
- `deploy/blufio@.service`: multi-instance systemd template

### Docker Availability
Docker is NOT installed on this machine. Docker verification must use static analysis (Dockerfile syntax check, docker-compose.yml structure validation) and flag the actual build as UNVERIFIED. This is acceptable per CONTEXT decision: "Build failure flagged as UNVERIFIED, doesn't block rest of verification."

## Open Questions

1. **Requirement count: 69 vs 71**
   - What we know: REQUIREMENTS.md explicitly lists 71 requirements and its footer says "71 total"
   - What's unclear: Why CONTEXT.md says 69
   - Recommendation: Use 71 as the authoritative count. Verify all 71.

2. **Phase 37 gaps -- fix or flag?**
   - What we know: 37-VERIFICATION.md documents 2 gaps (ApprovalRouter subscription, ConnectionManager forwarding)
   - What's unclear: Whether "audit AND fill test gaps" extends to fixing implementation gaps (vs just test gaps)
   - Recommendation: The CONTEXT says "write the missing test as part of verification" and "Incomplete/stub implementations flagged as UNVERIFIED -- do not attempt to implement missing features." These are implementation gaps, not test gaps, so they should be flagged as UNVERIFIED. This means Phase 37 may not hit 100% on its own, which could block the milestone. Surface this clearly in 39-SUMMARY.md.

3. **Integration test crate dependencies**
   - What we know: blufio-test-utils currently depends on core, agent, config, context, cost, router, skill, storage
   - What's unclear: Whether to add gateway, provider crates, wiremock as dependencies, or use a separate test crate
   - Recommendation: Add needed dependencies (blufio-gateway, blufio-bus, blufio-openrouter, blufio-discord, wiremock) to blufio-test-utils for the integration flow tests. Keep the tests in `crates/blufio-test-utils/tests/integration_flows.rs` as integration tests (not unit tests).

4. **Performance profiling depth**
   - What we know: CONTEXT says "full performance profiling per flow step (latency/throughput metrics)"
   - What's unclear: Whether this means simple Instant::now() timing or full tracing/flamegraph analysis
   - Recommendation: Use simple `std::time::Instant` timing for each step in each integration flow. Report as a table in 39-INTEGRATION.md. No need for tracing or external profiling tools.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (Rust built-in) + tokio::test for async |
| Config file | Cargo.toml workspace test settings |
| Quick run command | `cargo test -p blufio-test-utils` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map

Since this is a verification phase, the "tests" are the VERIFICATION.md reports themselves plus the integration flow tests:

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| INFRA-01 | Event bus broadcast delivery | unit | `cargo test -p blufio-bus -x` | Yes |
| INFRA-02 | Event bus typed events | unit | `cargo test -p blufio-bus -x` | Yes |
| INFRA-03 | Event bus mpsc reliable | unit | `cargo test -p blufio-bus -x` | Yes |
| PROV-01..09 | Provider adapters | unit | `cargo test -p blufio-openai -p blufio-ollama -p blufio-openrouter -p blufio-gemini -x` | Yes |
| PROV-10..14 | Core traits + custom config | unit | `cargo test -p blufio-core -p blufio-config -x` | Yes |
| API-01..10 | Gateway API endpoints | unit | `cargo test -p blufio-gateway -x` | Yes |
| API-11..18 | API keys, webhooks, batch | unit | `cargo test -p blufio-gateway -x` | Yes |
| CHAN-01..05 | Discord/Slack adapters | unit | `cargo test -p blufio-discord -p blufio-slack -x` | Yes |
| CHAN-06..10 | WA/Signal/IRC/Matrix | unit | `cargo test -p blufio-whatsapp -p blufio-signal -p blufio-irc -p blufio-matrix -x` | Yes |
| INFRA-06 | Cross-channel bridging | unit | `cargo test -p blufio-bridge -x` | Yes |
| SKILL-01..05 | Skill registry + signing | unit | `cargo test -p blufio-skill -x` | Yes |
| INFRA-04..05,07 | Docker + systemd | manual | `docker build .` (Docker not available) | Dockerfile exists |
| NODE-01..05 | Node system | unit | `cargo test -p blufio-node -x` | Yes |
| MIGR-01..05 | Migration | unit | `cargo test -p blufio -x` | Yes |
| CLI-01..05 | CLI utilities | unit | `cargo test -p blufio -x` | Yes |
| Flow 1 | OpenAI->OpenRouter->Discord->webhook | integration | `cargo test -p blufio-test-utils --test integration_flows flow_1 -x` | No -- Wave 0 |
| Flow 2 | Ollama->Telegram->event bus | integration | `cargo test -p blufio-test-utils --test integration_flows flow_2 -x` | No -- Wave 0 |
| Flow 3 | API key->rate limit->Gemini->batch | integration | `cargo test -p blufio-test-utils --test integration_flows flow_3 -x` | No -- Wave 0 |
| Flow 4 | Skill install->verify->execute->cost | integration | `cargo test -p blufio-test-utils --test integration_flows flow_4 -x` | No -- Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p {relevant-crate} -x`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green + all VERIFICATION.md scores = passed

### Wave 0 Gaps
- [ ] `crates/blufio-test-utils/tests/integration_flows.rs` -- 4 E2E flow tests
- [ ] blufio-test-utils Cargo.toml -- add dev-deps: wiremock, blufio-gateway, blufio-bus, blufio-openrouter, blufio-discord
- [ ] Any missing per-crate tests identified during verification audit

## Sources

### Primary (HIGH confidence)
- Project source code: all 34 crates compiled and 1,410 tests passing (verified via `cargo test --workspace`)
- Phase 30 VERIFICATION.md: gold standard format template (read directly)
- Phase 37 VERIFICATION.md: gap documentation with 17/19 score (read directly)
- Phase 38 VERIFICATION.md: 13/13 clean pass (read directly)
- REQUIREMENTS.md: 71 requirements, traceability table (read directly)
- ROADMAP.md: phase descriptions and dependency chain (read directly)
- blufio-test-utils source: MockProvider, MockChannel, TestHarness (read directly)
- Dockerfile and docker-compose.yml: Docker deployment artifacts (read directly)

### Secondary (MEDIUM confidence)
- Docker availability: Docker not installed on build machine (verified via `docker --version`)
- Phase 32 code existence: api_keys, webhooks, batch modules present in blufio-gateway (verified via `ls`)

### Tertiary (LOW confidence)
- None -- all findings verified from source code

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all tools already in project, no new dependencies needed
- Architecture: HIGH -- Phase 30 VERIFICATION.md provides exact template; integration test patterns from existing harness
- Pitfalls: HIGH -- identified from actual project state (Docker unavailability, Phase 37 gaps, requirement count mismatch)

**Research date:** 2026-03-07
**Valid until:** 2026-03-14 (7 days -- fast-moving project)
