# Phase 10: Multi-Agent & Final Integration - Research

**Researched:** 2026-03-01
**Domain:** Multi-agent delegation with Ed25519 signing + End-to-end integration testing
**Confidence:** HIGH

## Summary

Phase 10 has two pillars: (1) multi-agent delegation where a primary agent spawns ephemeral specialist SessionActors for sub-tasks via tool-based triggering with Ed25519-signed inter-agent messages, and (2) comprehensive end-to-end smoke tests validating the complete Blufio pipeline. Both pillars build directly on existing infrastructure -- DeviceKeypair already uses `ed25519_dalek` 2.1 (needs `sign()`/`verify()` methods), SessionActor already has a tool execution loop (delegation fits as a new tool type), and the project has established patterns for `#[tokio::test]` with mock implementations.

The key architectural insight is that multi-agent delegation is an *internal* mechanism -- specialist agents run as ephemeral SessionActors in the same process, communicating via in-memory signed messages. No new network protocols, no new adapter types. The delegation tool (`delegate_to_specialist`) is registered in the ToolRegistry alongside built-in tools, and the LLM decides when to delegate via its existing tool_use/tool_result loop.

**Primary recommendation:** Implement delegation as a DelegationTool in the ToolRegistry that creates ephemeral specialist SessionActors, sign all inter-agent messages with per-agent Ed25519 keypairs via extended DeviceKeypair, and build E2E tests using mock adapters with a TestHarness builder pattern in a dedicated `blufio-test-utils` crate.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

#### Delegation Model
- In-process delegation: primary agent spawns new SessionActors in the same process for specialists
- Synchronous execution: primary blocks (with timeout) until specialist responds
- Tool-based triggering: delegation exposed as tool calls (e.g., `delegate_to_researcher`), LLM decides when to delegate
- Single-level depth only: primary can delegate to specialists, specialists cannot delegate further (no chains)
- Hidden from user: specialist responses are tool_result content incorporated by the primary -- user sees one coherent response
- Ephemeral sessions: specialist SessionActors created per delegation and discarded after -- no persistent state

#### Message Signing Scheme
- Full payload signing: entire serialized message (sender, recipient, task, content, timestamp) is signed with Ed25519
- Per-agent keypairs: each agent (primary + specialists) generates its own Ed25519 keypair at startup; primary registers specialist public keys
- No replay protection for v1: in-process agents share a trust boundary, replay protection deferred
- Extend DeviceKeypair: add `sign()` and `verify()` methods to existing `DeviceKeypair` in `blufio-auth-keypair` crate

#### Agent Specialization
- TOML-configured: agents defined in `blufio.toml` with name, system_prompt, allowed skills, and model preference
- Fresh context per delegation: specialists get their own system prompt + delegated task description, no access to primary's conversation history
- Primary provides relevant context in the task payload

#### E2E Validation
- Mocked services: use mock implementations of ProviderAdapter and ChannelAdapter -- fast, free, deterministic, CI-runnable
- Integration test binary: dedicated `tests/e2e.rs` using standard `#[tokio::test]` with TestHarness builder pattern
- Core pipeline scenarios (must-have): (1) message-to-response pipeline, (2) conversation persistence across restart, (3) tool execution loop, (4) multi-agent delegation, (5) cost tracking, (6) budget enforcement, (7) Ed25519 signing/verification
- Nice-to-have scenarios: memory recall, model routing, plugin loading, Prometheus export, gateway API
- Reusable test utility crate: `blufio-test-utils` with MockProvider, MockChannel, TestHarness -- available to all workspace crates

### Claude's Discretion
- Exact delegation timeout value
- AgentMessage struct field design
- TestHarness builder API design
- Mock response generation strategy
- Integration test organization within e2e.rs

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SEC-07 | Ed25519 signed inter-agent messages -- prevents impersonation in multi-agent setups | DeviceKeypair already has `ed25519_dalek` 2.1 with SigningKey/VerifyingKey; add `sign()` and `verify()` methods. AgentMessage struct holds serialized payload + Ed25519 signature. Verification on receipt. |
| INFRA-06 | Multi-agent routing with session-based delegation between specialized agents | DelegationTool registered in ToolRegistry; spawns ephemeral specialist SessionActors; `[[agents]]` TOML config for specialist definitions; DelegationRouter manages agent lookup and keypair registry. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ed25519-dalek | 2.1 | Ed25519 signing and verification | Already in workspace deps; SigningKey::sign() and VerifyingKey::verify() for message authentication |
| serde + serde_json | 1.x | Message serialization for signing | Already used everywhere; canonical JSON serialization of AgentMessage before signing |
| tokio | 1.x | Async runtime, timeout, channels | Already in workspace; `tokio::time::timeout()` for delegation timeout |
| uuid | 1.x (v4) | Unique message IDs | Already in workspace; each AgentMessage gets a UUID |
| chrono | 0.4 | Timestamps in signed messages | Already in workspace; ISO 8601 timestamps in AgentMessage |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| async-trait | 0.1 | Async trait implementations | Already in workspace; for DelegationTool implementing Tool trait |
| tracing | 0.1 | Structured logging for delegation flow | Already in workspace; trace delegation lifecycle |
| tempfile | 3.x | Temporary DB files in E2E tests | Standard for test-scoped SQLite databases |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| In-process channels | gRPC/HTTP between agents | Massive overkill for same-process delegation; adds network complexity |
| Ed25519 | HMAC-SHA256 | HMAC needs shared secret; Ed25519 provides asymmetric verification (each agent proves identity) |
| Custom serialization | protobuf for AgentMessage | Overkill; serde_json canonical form is sufficient for in-process signing |

## Architecture Patterns

### Recommended Project Structure
```
crates/
├── blufio-auth-keypair/  # Extended with sign()/verify() methods
├── blufio-agent/         # New: delegation.rs module with DelegationTool + DelegationRouter
├── blufio-config/        # Extended: AgentSpecConfig + [[agents]] TOML section
├── blufio-test-utils/    # NEW CRATE: MockProvider, MockChannel, TestHarness
└── blufio/
    └── tests/            # NEW: e2e.rs integration tests
```

### Pattern 1: Delegation as Tool Call
**What:** The primary agent's LLM decides to delegate by calling a `delegate_to_specialist` tool, which creates an ephemeral specialist SessionActor, runs it to completion, and returns the specialist's response as the tool_result.
**When to use:** Any time the LLM determines a sub-task would benefit from specialist handling.
**Example:**
```rust
// DelegationTool implements Tool trait
pub struct DelegationTool {
    delegation_router: Arc<DelegationRouter>,
}

#[async_trait]
impl Tool for DelegationTool {
    fn name(&self) -> &str { "delegate_to_specialist" }

    async fn invoke(&self, input: serde_json::Value) -> Result<ToolOutput, BlufioError> {
        let agent_name = input["agent"].as_str().ok_or(/* ... */)?;
        let task = input["task"].as_str().ok_or(/* ... */)?;
        let context = input["context"].as_str().unwrap_or("");

        // Creates ephemeral specialist, runs task, returns result
        let result = self.delegation_router
            .delegate(agent_name, task, context)
            .await?;

        Ok(ToolOutput { content: result.content, is_error: false })
    }
}
```

### Pattern 2: Signed AgentMessage
**What:** Every inter-agent message is serialized, signed with the sender's Ed25519 private key, and verified by the recipient using the sender's public key.
**When to use:** All delegation requests and responses.
**Example:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: String,
    pub sender: String,
    pub recipient: String,
    pub message_type: AgentMessageType, // Request | Response
    pub task: String,
    pub content: String,
    pub timestamp: String,
}

impl AgentMessage {
    /// Serialize to canonical JSON for signing.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("AgentMessage is always serializable")
    }
}

#[derive(Debug, Clone)]
pub struct SignedAgentMessage {
    pub message: AgentMessage,
    pub signature: ed25519_dalek::Signature,
}
```

### Pattern 3: DelegationRouter with Agent Registry
**What:** Centralized router that maps agent names to their configurations and keypairs, creating ephemeral specialist sessions on demand.
**When to use:** At serve startup, create once and share via Arc.
**Example:**
```rust
pub struct DelegationRouter {
    agents: HashMap<String, AgentSpec>,
    primary_keypair: DeviceKeypair,
    agent_keypairs: HashMap<String, DeviceKeypair>,
    // Shared infrastructure for creating specialist sessions
    provider: Arc<dyn ProviderAdapter + Send + Sync>,
    storage: Arc<dyn StorageAdapter + Send + Sync>,
    timeout: Duration,
}
```

### Pattern 4: TestHarness Builder
**What:** A builder that assembles a complete test environment with mock adapters, returning a handle for sending messages and asserting responses.
**When to use:** All E2E integration tests.
**Example:**
```rust
let harness = TestHarness::builder()
    .with_mock_provider(responses)
    .with_mock_channel()
    .with_storage()        // tempfile SQLite
    .with_budget(10.0)     // $10 daily budget
    .build()
    .await?;

let response = harness.send_message("Hello").await?;
assert!(response.contains("expected text"));
```

### Anti-Patterns to Avoid
- **Persistent specialist sessions:** Specialists must be ephemeral -- create, run, discard. No state leakage between delegations.
- **Nested delegation:** Specialists must NOT be able to delegate further (single-level depth). Check and enforce this in DelegationTool.
- **Unsigned messages:** Never process an AgentMessage without verifying its signature first.
- **Blocking the agent loop:** Delegation must have a timeout; a stuck specialist cannot block the primary indefinitely.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Ed25519 signing | Custom crypto | `ed25519_dalek::SigningKey::sign()` | Crypto is hard; dalek is audited and already a dependency |
| Message serialization | Custom binary format | `serde_json::to_vec()` | Canonical JSON is deterministic, debuggable, and serde is everywhere |
| Async timeout | Manual timer loops | `tokio::time::timeout()` | Correct cancellation semantics built-in |
| Temp test databases | Manual /tmp management | `tempfile::NamedTempFile` | Auto-cleanup, unique paths, no test pollution |

**Key insight:** This phase is primarily wiring together existing components (SessionActor, ToolRegistry, DeviceKeypair) with a thin delegation coordination layer. The only genuinely new code is the DelegationRouter, DelegationTool, AgentMessage types, and the test harness.

## Common Pitfalls

### Pitfall 1: Deadlock on Shared Resources
**What goes wrong:** Specialist SessionActor shares the same Arc<dyn StorageAdapter> and Arc<CostLedger> as the primary. If the primary holds a lock while waiting for the specialist, and the specialist needs the same lock, deadlock.
**Why it happens:** Single-writer SQLite via tokio-rusqlite uses a dedicated writer thread. Both primary and specialist may submit writes.
**How to avoid:** tokio-rusqlite already serializes writes via its internal channel -- no mutex deadlock possible. The budget_tracker uses `tokio::sync::Mutex` which is fine for async (doesn't block the tokio worker). Just ensure delegation is awaited, not blocking.
**Warning signs:** Tests that hang indefinitely during delegation.

### Pitfall 2: Specialist Session Leakage
**What goes wrong:** Specialist SessionActors accumulate in memory if not properly dropped after delegation completes.
**Why it happens:** Forgetting to drop the specialist SessionActor after receiving the response.
**How to avoid:** Create specialist within the DelegationRouter::delegate method scope; it drops automatically when the method returns. Don't store specialists in any HashMap.
**Warning signs:** Memory growth proportional to delegation count.

### Pitfall 3: Non-Deterministic Test Ordering
**What goes wrong:** E2E tests that depend on shared state (same DB file) interfere with each other.
**Why it happens:** Using a shared SQLite database across multiple #[tokio::test] functions.
**How to avoid:** Each test gets its own tempfile-backed SQLite via TestHarness. Never share state between tests.
**Warning signs:** Tests pass individually but fail when run together.

### Pitfall 4: Signing Payload Mismatch
**What goes wrong:** Sender serializes AgentMessage one way, receiver re-serializes differently, signature verification fails.
**Why it happens:** serde_json field ordering can vary with HashMap fields or feature flags.
**How to avoid:** Use `serde_json::to_vec(&message)` for canonical form. AgentMessage fields should be fixed-order (struct, not HashMap). Store the exact bytes that were signed alongside the signature.
**Warning signs:** "signature verification failed" errors that only appear sometimes.

### Pitfall 5: Ed25519 VerifyingKey::verify() Allowing Weak Keys
**What goes wrong:** `verify()` permits weak public keys that can be exploited for forgeries.
**Why it happens:** Default `verify()` does not check for weak keys.
**How to avoid:** Use `verify_strict()` instead of `verify()`, or pre-check with `VerifyingKey::is_weak()` when registering agent public keys.
**Warning signs:** Signature verification passing for unexpected keys.

## Code Examples

### Ed25519 Signing with DeviceKeypair Extension
```rust
use ed25519_dalek::{Signature, Signer, Verifier};

impl DeviceKeypair {
    /// Sign arbitrary bytes with this keypair's private key.
    pub fn sign(&self, message: &[u8]) -> Signature {
        self.signing_key.sign(message)
    }

    /// Verify a signature against this keypair's public key.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), BlufioError> {
        self.verifying_key.verify(message, signature).map_err(|e| {
            BlufioError::Security(format!("signature verification failed: {e}"))
        })
    }

    /// Verify using strict mode (rejects weak public keys).
    pub fn verify_strict(&self, message: &[u8], signature: &Signature) -> Result<(), BlufioError> {
        self.verifying_key.verify_strict(message, signature).map_err(|e| {
            BlufioError::Security(format!("strict signature verification failed: {e}"))
        })
    }
}
```

### TOML Config for Agent Specialization
```toml
[[agents]]
name = "researcher"
system_prompt = "You are a research specialist. Find and summarize information."
model = "claude-haiku-4-5-20250901"
allowed_skills = ["http_request"]

[[agents]]
name = "coder"
system_prompt = "You are a coding specialist. Write and review code."
model = "claude-sonnet-4-20250514"
allowed_skills = ["bash", "file_io"]
```

### TestHarness Mock Pattern
```rust
pub struct MockProvider {
    responses: VecDeque<String>,
}

#[async_trait]
impl ProviderAdapter for MockProvider {
    async fn stream(&self, _request: ProviderRequest)
        -> Result<Pin<Box<dyn Stream<Item = Result<ProviderStreamChunk, BlufioError>> + Send>>, BlufioError>
    {
        let text = self.responses.lock().await.pop_front()
            .unwrap_or_else(|| "mock response".to_string());
        // Return a stream that yields a single text delta + message_stop
        Ok(mock_stream(text))
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| ed25519-dalek 1.x (bytes-only API) | ed25519-dalek 2.x (SigningKey/VerifyingKey types) | 2023 | Type-safe key handling, Signer/Verifier traits |
| Custom test harnesses per crate | Workspace-level test-utils crate | Rust ecosystem convention | Reusable mock implementations across integration tests |
| Ring for Ed25519 | ed25519-dalek | Project uses dalek already | Stick with existing dependency; ring would add a second Ed25519 implementation |

## Open Questions

1. **Delegation timeout value**
   - What we know: Must be finite to prevent blocking. Specialist runs a full LLM call (2-30 seconds typical).
   - What's unclear: Exact value depends on model and task complexity.
   - Recommendation: Default 60 seconds, configurable per-agent in TOML. Generous enough for Opus responses.

2. **Cost attribution for specialist sessions**
   - What we know: CostLedger already tracks per-session costs. Specialist gets its own session_id.
   - What's unclear: Should specialist costs be attributed to the primary session or tracked separately?
   - Recommendation: Record costs under the specialist's ephemeral session_id with a `delegated_from` field linking to the primary session. This preserves accurate per-model attribution while maintaining the delegation trail.

3. **Mock stream implementation for tests**
   - What we know: ProviderAdapter::stream() returns `Pin<Box<dyn Stream<...>>>`. Tests need predictable responses.
   - What's unclear: Exact chunking strategy for mock streams.
   - Recommendation: Simple approach -- single ContentBlockDelta with full text, then MessageStop. No need to simulate real SSE chunking in E2E tests.

## Sources

### Primary (HIGH confidence)
- ed25519-dalek 2.x API: SigningKey::sign(), VerifyingKey::verify(), VerifyingKey::verify_strict() -- verified via Context7
- Existing codebase: DeviceKeypair (blufio-auth-keypair), SessionActor (blufio-agent), ToolRegistry (blufio-skill), BlufioConfig (blufio-config) -- direct code reading
- Cargo.toml workspace dependencies: ed25519-dalek 2.1 with rand_core feature confirmed

### Secondary (MEDIUM confidence)
- tokio::time::timeout pattern for async operations with cancellation
- tempfile crate for test-scoped temporary databases

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all libraries already in workspace, APIs verified
- Architecture: HIGH - delegation maps directly to existing SessionActor + ToolRegistry patterns
- Pitfalls: HIGH - identified from codebase analysis (shared resources, session lifecycle, test isolation)

**Research date:** 2026-03-01
**Valid until:** 2026-04-01 (stable -- all dependencies are mature)
