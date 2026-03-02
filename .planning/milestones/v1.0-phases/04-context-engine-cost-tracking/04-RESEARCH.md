# Phase 4: Context Engine & Cost Tracking - Research

**Researched:** 2026-03-01
**Domain:** LLM context assembly, Anthropic prompt caching, token cost accounting, budget enforcement
**Confidence:** HIGH

## Summary

Phase 4 introduces two new crates (`blufio-context` and `blufio-cost`) that replace the current flat context assembly in `blufio-agent/context.rs` with a three-zone prompt engine aligned to Anthropic's prompt caching API, and add a full cost ledger with budget enforcement. The current codebase provides clean integration points: `assemble_context()` returns a `ProviderRequest` that the `SessionActor` uses, and `TokenUsage` is already propagated through the streaming response chain. The Anthropic API types (`MessageRequest`, `ApiUsage`) need extension to support structured system prompt blocks with `cache_control` markers and cache-related usage fields.

The Anthropic prompt caching API is now GA (no beta header required) and supports two modes: automatic caching (top-level `cache_control` field) and explicit breakpoints (per-block `cache_control`). For Blufio's use case -- a personal agent with stable system prompt and growing conversation history -- the optimal strategy is a hybrid approach: explicit `cache_control` on the system prompt block (to guarantee it stays cached independently) combined with automatic caching at the request level for the conversation history. This achieves the target 50-65% cache hit rate by ensuring the system prompt is always a cache hit after the first call, while conversation history accumulates cache reads on the growing prefix.

**Primary recommendation:** Use two new crates (`blufio-context` for three-zone assembly, `blufio-cost` for ledger/budget), extend Anthropic API types to support structured system blocks with `cache_control`, use post-response `ApiUsage` data (not estimation) for cost tracking, and enforce budgets via a gate check before each LLM call in the session actor.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- New `blufio-context` crate for the three-zone context engine, separate from blufio-agent
- Conditional zone stubbed with proper trait interface but kept empty -- Phase 5 (Memory) and Phase 7 (Skills) will plug into it later
- Dynamic zone uses sliding window + compaction summary: recent messages verbatim, older messages summarized
- Compaction triggers on token threshold (percentage of context window budget), checked before each LLM call
- LLM-generated summary using Haiku model for cost efficiency
- Compaction summaries persisted in SQLite (special message rows or metadata-tagged entries)
- Cost ledger stored as a new table in the existing blufio.db SQLite database
- Per-LLM-call granularity: each API call gets a ledger row with session_id, model, feature type (message/compaction/tool), input_tokens, output_tokens, cache_read_tokens, calculated cost_usd, timestamp
- In-memory running daily/monthly totals for fast budget checks, plus structured tracing logs
- New `blufio-cost` crate for cost tracking, budget enforcement, and ledger logic
- Warning notification at 80% of daily/monthly budget cap
- Hard stop at 100% -- agent stops making LLM calls and returns clear message
- Global caps only (daily + monthly) -- no per-session limits
- Daily budget resets at midnight UTC

### Claude's Discretion
- Cache block structure: how to structure system prompt blocks with cache_control breakpoints for optimal Anthropic cache hit rates
- Compaction timing: inline (blocking before LLM call) vs background (after response)
- Budget enforcement point: session actor gate vs context engine gate
- Token counting strategy for cost calculation (tiktoken estimation vs post-response usage data)
- Exact token threshold percentage for compaction trigger
- Error handling and recovery patterns throughout

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| LLM-03 | Three-zone context engine assembles prompts from static (system prompt, cached), conditional (skills/memory per-relevance), and dynamic (current turn) zones | Three-zone architecture with `ContextEngine` trait, `StaticZone`, `ConditionalZone` (stubbed), `DynamicZone` structs; system prompt loaded once, conditional zone trait-based for Phase 5/7 plug-in |
| LLM-04 | Context engine aligns prompt structure to exploit Anthropic prompt caching (target 50-65% cache hit rate) | Hybrid caching strategy: explicit `cache_control` on system prompt block + automatic top-level `cache_control` for conversation; structured system as array of content blocks; `ApiUsage` extended with cache fields |
| LLM-07 | Token overhead per turn stays <=3,000 for simple queries and <=5,000 weighted average | Three-zone design minimizes overhead by keeping static zone constant, conditional zone empty for now, and dynamic zone using sliding window; overhead tracked via cache_read vs input_tokens ratio |
| MEM-04 | Conversation history compacts automatically when approaching context window limits | Compaction engine triggers when history token count exceeds threshold (recommend 70% of context budget); uses Haiku model for LLM-generated summary; summary persisted as metadata-tagged message row in SQLite |
| COST-01 | Unified cost ledger tracks every token across all features (messages, heartbeats, tools, compaction) | `cost_ledger` SQLite table with per-call rows; `CostLedger` struct records after every provider response; feature_type enum for attribution |
| COST-02 | Per-session and per-model cost attribution in real-time | Ledger rows include session_id and model columns; in-memory aggregation via `BudgetTracker` provides real-time running totals |
| COST-03 | Configurable daily and monthly budget caps with hard kill switch when exhausted | `BudgetTracker` checks caps before each LLM call; 80% warning via tracing; 100% hard stop returns `BlufioError::BudgetExhausted`; reset at midnight UTC |
| COST-05 | Structured error handling with Result<T,E> everywhere -- zero empty catch blocks | New error variants (`BlufioError::BudgetExhausted`, `BlufioError::ContextAssembly`) follow existing pattern; all functions return `Result<T, BlufioError>` |
| COST-06 | All errors logged with context using tracing crate -- structured, filterable | Cost events logged at info level with structured fields (session_id, model, input_tokens, cost_usd); budget warnings at warn level; errors at error level |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio-rusqlite | 0.7 | Async SQLite access for cost ledger | Already in workspace, single-writer pattern established |
| rusqlite | 0.37 | SQLite with bundled build | Already in workspace, migration pattern established |
| refinery | 0.9 | Schema migrations | Already in workspace, V1 migration exists |
| serde/serde_json | 1.x | Serialization for API types | Already in workspace |
| chrono | 0.4 | Timestamps for ledger entries and UTC reset | Already in workspace |
| tracing | 0.1 | Structured logging for cost events | Already in workspace |
| uuid | 1.x | Unique IDs for ledger entries | Already in workspace |
| reqwest | 0.12 | HTTP for compaction calls to Haiku | Already in workspace via blufio-anthropic |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tokio (time) | 1.x | Midnight UTC reset timer for budget tracker | Budget enforcement daily reset |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Post-response usage data for cost | tiktoken pre-estimation | Post-response is exact (Anthropic returns cache_read, cache_creation, input, output tokens); tiktoken adds a dependency and is only an estimate -- use actual API response data |
| Inline (blocking) compaction | Background (async) compaction | Inline is simpler and guarantees compacted context before next LLM call; background risks stale context and race conditions -- use inline |
| Budget gate in context engine | Budget gate in session actor | Session actor is the single call site; placing the gate there is cleaner and avoids context engine needing cost awareness -- use session actor gate |

**Installation:**
No new dependencies needed -- all libraries are already in the workspace `Cargo.toml`. New crates (`blufio-context`, `blufio-cost`) will reference workspace dependencies.

## Architecture Patterns

### Recommended Project Structure
```
crates/
  blufio-context/
    src/
      lib.rs           # ContextEngine trait + three-zone orchestrator
      static_zone.rs   # System prompt loading + cache-aligned formatting
      conditional.rs   # ConditionalProvider trait (stubbed for Phase 5/7)
      dynamic.rs       # Sliding window + compaction logic
      compaction.rs    # LLM-based summary generation using Haiku
    Cargo.toml
  blufio-cost/
    src/
      lib.rs           # Public API: CostLedger, BudgetTracker, CostRecord
      ledger.rs        # SQLite persistence for cost records
      budget.rs        # In-memory budget tracking with daily/monthly caps
      pricing.rs       # Model pricing table (cost per token by model)
    migrations/
      V2__cost_ledger.sql
    Cargo.toml
```

### Pattern 1: Three-Zone Context Assembly
**What:** Context is assembled in three zones that map to Anthropic's cache hierarchy: static (system prompt, always cached), conditional (relevant context injected per-turn, stubbed for now), and dynamic (recent messages + compaction summary).

**When to use:** Every LLM call goes through this pipeline.

**Example:**
```rust
// Source: Anthropic API docs + project patterns
pub struct ContextEngine {
    static_zone: StaticZone,
    conditional_providers: Vec<Box<dyn ConditionalProvider + Send + Sync>>,
    dynamic_zone: DynamicZone,
}

impl ContextEngine {
    /// Assembles a ProviderRequest from the three zones.
    pub async fn assemble(
        &self,
        storage: &dyn StorageAdapter,
        session_id: &str,
        inbound: &InboundMessage,
        model: &str,
        max_tokens: u32,
    ) -> Result<ProviderRequest, BlufioError> {
        // Zone 1: Static -- system prompt (cache-aligned)
        let system_blocks = self.static_zone.system_blocks();

        // Zone 2: Conditional -- empty for now, Phase 5/7 will inject
        let mut conditional_blocks = Vec::new();
        for provider in &self.conditional_providers {
            conditional_blocks.extend(provider.provide_context(session_id).await?);
        }

        // Zone 3: Dynamic -- sliding window with compaction
        let messages = self.dynamic_zone
            .assemble_messages(storage, session_id, inbound)
            .await?;

        Ok(ProviderRequest {
            model: model.to_string(),
            system_prompt: None,  // replaced by structured system_blocks
            system_blocks: Some(system_blocks),
            messages,
            max_tokens,
            stream: true,
            cache_control: Some(CacheControl::ephemeral()),
        })
    }
}
```

### Pattern 2: Anthropic Structured System Prompt with Cache Control
**What:** The system prompt is sent as an array of content blocks (not a plain string) with `cache_control` on the last block, enabling Anthropic's prompt cache to hit on the static prefix every turn.

**When to use:** Every API request to Anthropic.

**Example:**
```rust
// Source: Anthropic prompt caching docs
// MessageRequest.system changes from Option<String> to Option<SystemContent>

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SystemContent {
    /// Simple string system prompt (backward compatible).
    Text(String),
    /// Array of content blocks with optional cache_control.
    Blocks(Vec<SystemBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemBlock {
    #[serde(rename = "type")]
    pub block_type: String,  // always "text"
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlMarker>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControlMarker {
    #[serde(rename = "type")]
    pub control_type: String,  // "ephemeral"
}

// Top-level cache_control on the request body
#[derive(Debug, Clone, Serialize)]
pub struct MessageRequest {
    pub model: String,
    pub messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemContent>,
    pub max_tokens: u32,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControlMarker>,
}
```

### Pattern 3: Cost Ledger with In-Memory Budget Tracking
**What:** Every LLM API response is recorded in SQLite with full token breakdown. An in-memory `BudgetTracker` maintains running daily/monthly totals for O(1) budget checks.

**When to use:** After every provider response; before every provider call (budget check).

**Example:**
```rust
// Source: project patterns (async_trait, tokio-rusqlite, tracing)
pub struct CostRecord {
    pub id: String,
    pub session_id: String,
    pub model: String,
    pub feature_type: FeatureType,  // Message, Compaction, Tool, Heartbeat
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
    pub cost_usd: f64,
    pub created_at: String,
}

pub enum FeatureType {
    Message,
    Compaction,
    Tool,
    Heartbeat,
}

pub struct BudgetTracker {
    daily_total_usd: f64,
    monthly_total_usd: f64,
    daily_cap: Option<f64>,
    monthly_cap: Option<f64>,
    current_day: u32,     // day of year for reset detection
    current_month: u32,   // month for reset detection
}

impl BudgetTracker {
    /// Check if a call is allowed under current budget.
    pub fn check_budget(&mut self) -> Result<(), BlufioError> {
        self.maybe_reset_daily();
        self.maybe_reset_monthly();

        if let Some(cap) = self.daily_cap {
            if self.daily_total_usd >= cap {
                return Err(BlufioError::BudgetExhausted {
                    message: format!(
                        "Daily budget of ${:.2} reached. Resumes at midnight UTC.",
                        cap
                    ),
                });
            }
            if self.daily_total_usd >= cap * 0.8 {
                tracing::warn!(
                    daily_spent = self.daily_total_usd,
                    daily_cap = cap,
                    "approaching daily budget limit (80%)"
                );
            }
        }
        // Similar for monthly_cap...
        Ok(())
    }

    /// Record cost after a successful API call.
    pub fn record_cost(&mut self, cost_usd: f64) {
        self.daily_total_usd += cost_usd;
        self.monthly_total_usd += cost_usd;
    }
}
```

### Pattern 4: Compaction via Haiku Summarization
**What:** When conversation history token count exceeds a threshold percentage of the context window budget, older messages are summarized using a Haiku API call. The summary replaces the older messages as a special "compaction" message row in SQLite.

**When to use:** Checked before each LLM call in the dynamic zone assembly.

**Example:**
```rust
// Source: project patterns
const COMPACTION_THRESHOLD_RATIO: f64 = 0.70;  // Trigger at 70% of context budget
const CONTEXT_BUDGET_TOKENS: u32 = 180_000;     // Leave room for output in 200K window

impl DynamicZone {
    async fn assemble_messages(
        &self,
        storage: &dyn StorageAdapter,
        session_id: &str,
        inbound: &InboundMessage,
    ) -> Result<Vec<ProviderMessage>, BlufioError> {
        let history = storage.get_messages(session_id, None).await?;
        let estimated_tokens = self.estimate_history_tokens(&history);

        if estimated_tokens > (CONTEXT_BUDGET_TOKENS as f64 * COMPACTION_THRESHOLD_RATIO) as u32 {
            // Trigger compaction: summarize older half, keep recent half verbatim
            let summary = self.compact_history(storage, session_id, &history).await?;
            // summary is persisted as a metadata-tagged message
            // Return: [compaction_summary] + [recent_messages] + [inbound]
        }
        // Normal path: return history + inbound
    }
}
```

### Anti-Patterns to Avoid
- **Sending system prompt as plain string when caching:** Always use the structured `system` array format with `cache_control` on the last block. A plain string system prompt misses the explicit cache breakpoint and relies entirely on automatic caching, which may not independently cache the system prompt.
- **Estimating tokens client-side for cost tracking:** Anthropic returns exact token counts (including cache reads/writes) in the response. Using client-side estimation (e.g., tiktoken) introduces inaccuracy. Always use post-response `usage` data.
- **Background compaction with stale context:** If compaction runs asynchronously after the response, the next LLM call might still assemble the uncompacted context. Use inline (blocking) compaction before the LLM call.
- **Per-call budget DB queries:** Querying SQLite on every LLM call for budget totals adds latency. Use in-memory running totals, initialized from DB at startup, updated in-memory after each call.
- **Modifying tool definitions between calls:** Anthropic's cache hierarchy is `tools > system > messages`. Changing tools invalidates the entire cache. Keep tool definitions stable.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Token counting for cost | Custom tokenizer | Anthropic API response `usage` fields | API returns exact `input_tokens`, `output_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens` -- more accurate than any client-side estimate |
| Cost calculation | Custom per-model math | Pricing lookup table + usage data | Pricing changes; a simple table `model -> (input_rate, output_rate, cache_read_rate, cache_write_rate)` is easy to update |
| SQLite migrations | Manual DDL | refinery `embed_migrations!` | Already established in the project for V1; add V2 migration following the same pattern |
| Prompt caching | Custom prefix-matching | Anthropic API `cache_control` | The API handles all cache management server-side; just structure the request correctly |
| UTC midnight timer | Custom time tracking | `chrono::Utc::now()` day-of-year comparison | Simple integer comparison on each budget check, no background timer needed |

**Key insight:** Anthropic does the heavy lifting for caching and token counting. The client's job is to structure requests correctly and record the response data.

## Common Pitfalls

### Pitfall 1: System Prompt Below Minimum Cache Threshold
**What goes wrong:** Short system prompts (under 1024 tokens for Sonnet 4, 2048 for Sonnet 4.6) silently fail to cache. No error, just no cache hits, and you pay full input token price every call.
**Why it happens:** Anthropic has model-specific minimum token thresholds for caching.
**How to avoid:** Ensure the system prompt (potentially combined with conditional zone content) exceeds the minimum threshold. For Claude Sonnet 4 the minimum is 1024 tokens. Pad with useful context if needed (agent personality, behavioral guidelines). Log `cache_creation_input_tokens` and `cache_read_input_tokens` at info level to detect when caching is not happening.
**Warning signs:** `cache_read_input_tokens` is always 0 in API responses.

### Pitfall 2: Cache Invalidation from System Prompt Changes
**What goes wrong:** The system prompt text changes between turns (e.g., dynamic timestamp injection), causing the entire cache chain to invalidate.
**Why it happens:** Cache keys are cumulative -- tools > system > messages. Any change to system invalidates system and all messages.
**How to avoid:** Keep the static zone truly static. Never inject timestamps, session IDs, or other per-turn data into the system prompt. Put per-turn context in the dynamic zone (messages).
**Warning signs:** `cache_creation_input_tokens` is large every turn (instead of near-zero on subsequent turns).

### Pitfall 3: Compaction Summary Losing Critical Context
**What goes wrong:** LLM-generated summary misses key facts from earlier conversation (names, commitments, preferences), causing the agent to "forget" things it was told.
**Why it happens:** Summarization is inherently lossy. Important details scattered across many messages may not survive compression.
**How to avoid:** Include explicit instructions in the compaction prompt: "Preserve all user preferences, names, commitments, and decisions." Keep the most recent N messages verbatim (never summarize the last 5-10 messages). Use a generous compaction threshold so compaction only triggers when truly needed.
**Warning signs:** Users complain the agent "forgot" something they told it earlier in the same session.

### Pitfall 4: Budget Tracker Drift After Restart
**What goes wrong:** In-memory budget totals are lost on restart. After crash/restart, the agent allows spending beyond the configured cap because in-memory totals are zero.
**Why it happens:** Budget totals are only in memory; they must be re-initialized from the ledger on startup.
**How to avoid:** On startup, query the cost ledger for today's and this month's totals: `SELECT SUM(cost_usd) FROM cost_ledger WHERE created_at >= ?`. Initialize `BudgetTracker` from these values.
**Warning signs:** Cost exceeds configured caps after a restart.

### Pitfall 5: f64 Floating-Point Drift in Cost Accumulation
**What goes wrong:** Accumulating many small f64 costs (e.g., $0.000003 per call) over thousands of calls introduces floating-point errors, causing budget checks to behave unpredictably.
**Why it happens:** IEEE 754 floating-point arithmetic is not exact for decimal values.
**How to avoid:** Store costs in the database as REAL (SQLite maps this to f64, which is acceptable for the scale of individual cost values). For in-memory accumulation, the drift at the scale of LLM costs (thousands of calls at sub-cent values) is negligible -- total error stays well under $0.01. If paranoid, re-sync from DB periodically.
**Warning signs:** Budget enforcement triggers at unexpected thresholds.

### Pitfall 6: Compaction Cost Not Tracked
**What goes wrong:** Compaction calls to Haiku consume tokens but are not recorded in the cost ledger, causing reported costs to understate actual spend.
**Why it happens:** The compaction call goes through the provider but bypasses the normal session flow that records costs.
**How to avoid:** Record compaction calls in the cost ledger with `feature_type = FeatureType::Compaction`. The compaction function must return usage data and pass it to the cost ledger.
**Warning signs:** Anthropic invoice exceeds tracked costs in the ledger.

## Code Examples

### Extended ApiUsage with Cache Fields
```rust
// Source: Anthropic API docs - response usage object
// Modify: crates/blufio-anthropic/src/types.rs

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ApiUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_read_input_tokens: u32,
    #[serde(default)]
    pub cache_creation_input_tokens: u32,
}
```

### Extended TokenUsage in blufio-core
```rust
// Source: project types.rs pattern
// Modify: crates/blufio-core/src/types.rs

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default)]
    pub cache_read_tokens: u32,
    #[serde(default)]
    pub cache_creation_tokens: u32,
}
```

### Cost Ledger SQL Migration
```sql
-- V2__cost_ledger.sql
-- Cost tracking ledger for token usage and budget enforcement.

CREATE TABLE IF NOT EXISTS cost_ledger (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    model TEXT NOT NULL,
    feature_type TEXT NOT NULL,  -- 'message', 'compaction', 'tool', 'heartbeat'
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
    cost_usd REAL NOT NULL DEFAULT 0.0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_cost_ledger_session ON cost_ledger(session_id);
CREATE INDEX IF NOT EXISTS idx_cost_ledger_created ON cost_ledger(created_at);
CREATE INDEX IF NOT EXISTS idx_cost_ledger_model ON cost_ledger(model);
```

### Pricing Table
```rust
// Source: Anthropic pricing page (March 2026)
// crates/blufio-cost/src/pricing.rs

/// Cost per million tokens for a given model.
pub struct ModelPricing {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_read_per_mtok: f64,   // 0.1x input price
    pub cache_write_per_mtok: f64,  // 1.25x input price (5-min TTL)
}

pub fn get_pricing(model: &str) -> ModelPricing {
    // Match on model ID prefix for flexibility
    if model.contains("opus-4") {
        // Claude Opus 4.5/4.6 pricing
        ModelPricing {
            input_per_mtok: 5.0,
            output_per_mtok: 25.0,
            cache_read_per_mtok: 0.50,
            cache_write_per_mtok: 6.25,
        }
    } else if model.contains("sonnet-4") {
        // Claude Sonnet 4/4.5/4.6 pricing
        ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: 0.30,
            cache_write_per_mtok: 3.75,
        }
    } else if model.contains("haiku") {
        // Claude Haiku 3.5/4.5 pricing
        ModelPricing {
            input_per_mtok: 1.0,
            output_per_mtok: 5.0,
            cache_read_per_mtok: 0.10,
            cache_write_per_mtok: 1.25,
        }
    } else {
        // Fallback to Sonnet pricing
        ModelPricing {
            input_per_mtok: 3.0,
            output_per_mtok: 15.0,
            cache_read_per_mtok: 0.30,
            cache_write_per_mtok: 3.75,
        }
    }
}

/// Calculate cost from token usage and model pricing.
pub fn calculate_cost(usage: &TokenUsage, pricing: &ModelPricing) -> f64 {
    let input_cost = (usage.input_tokens as f64 / 1_000_000.0) * pricing.input_per_mtok;
    let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * pricing.output_per_mtok;
    let cache_read_cost = (usage.cache_read_tokens as f64 / 1_000_000.0) * pricing.cache_read_per_mtok;
    let cache_write_cost = (usage.cache_creation_tokens as f64 / 1_000_000.0) * pricing.cache_write_per_mtok;
    input_cost + output_cost + cache_read_cost + cache_write_cost
}
```

### Compaction Summary Prompt
```rust
// Source: project patterns
// crates/blufio-context/src/compaction.rs

const COMPACTION_PROMPT: &str = r#"Summarize the following conversation history concisely.
Preserve ALL of the following:
- User preferences and personal details (names, locations, habits)
- Commitments and decisions made
- Key facts and context established
- Action items and pending tasks
- Emotional tone and relationship context

Be concise but complete. This summary replaces the original messages.

Conversation:
"#;

/// Generate a compaction summary using Haiku for cost efficiency.
pub async fn generate_compaction_summary(
    provider: &dyn ProviderAdapter,
    messages_to_compact: &[Message],
) -> Result<(String, TokenUsage), BlufioError> {
    let conversation_text = messages_to_compact
        .iter()
        .map(|m| format!("{}: {}", m.role, m.content))
        .collect::<Vec<_>>()
        .join("\n");

    let request = ProviderRequest {
        model: "claude-haiku-4-5-20250901".to_string(),  // Haiku for cost
        system_prompt: None,
        messages: vec![ProviderMessage {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: format!("{COMPACTION_PROMPT}{conversation_text}"),
            }],
        }],
        max_tokens: 2048,
        stream: false,
    };

    let response = provider.complete(request).await?;
    Ok((response.content, response.usage))
}
```

### New BlufioError Variant
```rust
// Source: project error pattern
// Modify: crates/blufio-core/src/error.rs

/// Budget cap exceeded -- agent cannot make LLM calls until budget resets.
#[error("budget exhausted: {message}")]
BudgetExhausted { message: String },
```

### MessageRequest with Automatic Cache Control
```rust
// The simplest and most effective approach for multi-turn conversations:
// Add top-level cache_control to the request body, combined with
// explicit cache_control on system prompt for independent caching.

// Source: Anthropic prompt caching docs - hybrid approach
let request_json = serde_json::json!({
    "model": "claude-sonnet-4-20250514",
    "max_tokens": 4096,
    "cache_control": {"type": "ephemeral"},  // auto-cache conversation
    "system": [
        {
            "type": "text",
            "text": "You are blufio, a concise personal assistant...",
            "cache_control": {"type": "ephemeral"}  // explicit system cache
        }
    ],
    "messages": [
        {"role": "user", "content": "Hello"},
        {"role": "assistant", "content": "Hi there!"},
        {"role": "user", "content": "What's new?"}
    ],
    "stream": true
});
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Beta header for prompt caching | GA -- no special header needed | 2025 | Simplifies implementation; just add `cache_control` to request body |
| Only explicit breakpoints | Automatic caching + explicit breakpoints | 2025 | Automatic caching handles multi-turn conversations; explicit breakpoints for system prompt |
| Plain string system prompt | Structured system array with cache_control | 2024 | Enables per-block caching on system prompt for independent cache hits |
| No cache cost breakdown | `cache_read_input_tokens` + `cache_creation_input_tokens` in response | 2024 | Enables accurate cost calculation distinguishing cache hits from misses |

**Deprecated/outdated:**
- `anthropic-beta: prompt-caching-2024-07-31` header: No longer needed. Prompt caching is GA.
- Single `input_tokens` field: Now supplemented with `cache_read_input_tokens` and `cache_creation_input_tokens` for full breakdown.

## Discretion Recommendations

Based on research, here are recommendations for the areas left to Claude's discretion:

### Cache Block Structure
**Recommendation:** Use hybrid approach -- explicit `cache_control` on the last system block + top-level `cache_control` on the request body.
**Rationale:** The explicit system breakpoint ensures the system prompt is cached independently (even if conversation changes). The top-level automatic cache handles the growing conversation prefix. This achieves the target 50-65% cache hit rate because the system prompt is always a cache read after the first call, and conversation history accumulates cache reads on the growing prefix. Maximum of 4 breakpoints total; this uses 2 (1 explicit system + 1 automatic).

### Compaction Timing
**Recommendation:** Inline (blocking) before the LLM call.
**Rationale:** Inline guarantees the LLM sees the compacted context, avoiding a window where an uncompacted context exceeds the window. The latency cost of one Haiku call (~500ms) is acceptable since compaction is infrequent (only when history exceeds 70% of context budget). Background compaction introduces race conditions and complexity without meaningful benefit for a personal agent with low concurrency.

### Budget Enforcement Point
**Recommendation:** Session actor gate -- check budget in `SessionActor::handle_message()` before calling the context engine.
**Rationale:** The session actor is the single call site for LLM requests. Placing the gate there is the cleanest integration: `budget_tracker.check_budget()` -> `context_engine.assemble()` -> `provider.stream()` -> `budget_tracker.record_cost()`. The context engine should remain budget-unaware; its job is assembling context, not enforcing policy.

### Token Counting Strategy
**Recommendation:** Use post-response `ApiUsage` data exclusively for cost tracking.
**Rationale:** Anthropic returns exact token counts including cache breakdowns. Client-side estimation (tiktoken) would add a Rust dependency (or Python FFI), only approximate, and cannot account for cache reads/writes. For the compaction threshold check, use a simple heuristic: `message.content.len() / 4` as a rough token estimate (English text averages ~4 chars per token). This is sufficient for triggering compaction; exact counting is not needed for the threshold.

### Compaction Trigger Threshold
**Recommendation:** 70% of context window budget.
**Rationale:** Claude Sonnet 4 has a 200K token context window. A 70% threshold (~126K tokens for history) leaves 30% (~54K tokens) for the current turn, system prompt, and output. This is generous enough to avoid premature compaction while ensuring there is always room for the response. The threshold should be configurable via `CostConfig` in TOML.

### Error Handling Patterns
**Recommendation:** Follow the existing `BlufioError` pattern with new variants:
- `BudgetExhausted { message: String }` -- for budget cap enforcement
- Extend existing `Provider` variant for compaction failures (it is still a provider call)
- All errors logged via `tracing` with structured fields (session_id, cost_usd, etc.)

## Open Questions

1. **Compaction message storage format**
   - What we know: Compaction summaries should be persisted in SQLite as special message rows
   - What's unclear: Should they use a special role (e.g., "system" or "compaction") or a metadata tag on a "system" role message? Using role="system" would be simplest but might conflict with other system messages.
   - Recommendation: Use role="system" with a metadata JSON tag `{"type":"compaction_summary","original_count":N,"compacted_at":"..."}`. The context engine recognizes this tag and includes the summary text in the dynamic zone prefix.

2. **Migration ownership -- blufio-cost or blufio-storage?**
   - What we know: The V1 migration lives in `blufio-storage/migrations/`. The cost_ledger table logically belongs to `blufio-cost`.
   - What's unclear: Should the V2 migration live in blufio-cost or blufio-storage?
   - Recommendation: Keep all migrations in `blufio-storage/migrations/` since the `Database::open()` method runs all migrations from that directory. Add V2 there. blufio-cost depends on blufio-storage for database access.

3. **Haiku model ID for compaction**
   - What we know: Haiku 4.5 exists with model ID format like `claude-haiku-4-5-YYYYMMDD`.
   - What's unclear: Exact model ID string and whether it should be configurable.
   - Recommendation: Default to the latest Haiku model ID, make it configurable in TOML config under a new `[context]` section (e.g., `compaction_model = "claude-haiku-4-5-20250901"`).

## Sources

### Primary (HIGH confidence)
- [Anthropic Prompt Caching Docs](https://platform.claude.com/docs/en/build-with-claude/prompt-caching) -- cache_control format, structured system blocks, pricing multipliers, minimum token thresholds, cache hierarchy, usage response fields, automatic vs explicit caching
- [Anthropic Pricing](https://platform.claude.com/docs/en/about-claude/pricing) -- per-model token costs, cache read/write multipliers

### Secondary (MEDIUM confidence)
- Existing codebase analysis: `blufio-agent/context.rs`, `blufio-anthropic/types.rs`, `blufio-core/types.rs`, `blufio-config/model.rs`, `blufio-storage/migrations/V1__initial_schema.sql` -- current implementation patterns, type structures, integration points

### Tertiary (LOW confidence)
- Token estimation heuristic (4 chars per token for English) -- commonly cited approximation, sufficient for compaction threshold but not for cost tracking

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in workspace, no new dependencies needed
- Architecture: HIGH -- three-zone pattern is well-understood, Anthropic caching API is GA with clear documentation
- Pitfalls: HIGH -- cache thresholds, invalidation rules, and budget tracking edge cases are well-documented
- Pricing: MEDIUM -- Anthropic pricing may change; the pricing table should be treated as a configuration, not a constant
- Compaction quality: MEDIUM -- LLM summarization quality depends on prompt engineering; may need iteration

**Research date:** 2026-03-01
**Valid until:** 2026-04-01 (30 days -- Anthropic API is stable; pricing may update)
