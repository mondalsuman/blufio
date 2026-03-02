# Phase 6: Model Routing & Smart Heartbeats - Research

**Researched:** 2026-03-01
**Domain:** LLM model routing, query classification, background heartbeat scheduling
**Confidence:** HIGH

## Summary

Phase 6 adds two capabilities: (1) a zero-latency heuristic classifier that routes user-facing messages to the appropriate Claude model tier (Haiku/Sonnet/Opus), and (2) a background heartbeat system that runs on Haiku to provide proactive check-ins. Both features integrate into the existing agent loop and cost tracking infrastructure.

The routing system intercepts messages before context assembly in `SessionActor.handle_message()`, replaces the static `self.model` field with a dynamically classified model, and records both `intended_model` and `actual_model` in the cost ledger. Budget-aware routing downgrades model tiers when daily budget thresholds are approached. The heartbeat system runs as a separate background task spawned alongside Telegram polling in `serve.rs`, using its own dedicated budget tracker to enforce the $10/month cap.

**Primary recommendation:** Build a `blufio-router` crate containing the `QueryClassifier` (heuristic rules) and `ModelRouter` (orchestrates classification + budget-aware downgrade + per-message overrides). Heartbeat logic goes into `blufio-agent` since it needs access to session state and the provider.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Heuristic rules only -- zero latency, zero cost classification (no LLM pre-call)
- Fixed algorithm, Claude tunes internally -- not user-configurable
- Classifier considers current message + recent context (last 2-3 messages) to track conversation momentum
- When classification is uncertain, default UP to Sonnet (prioritize quality over cost)
- Three tiers from day one: Haiku (simple), Sonnet (standard), Opus (complex)
- Routing applies to user-facing messages only -- internal calls (compaction, extraction) stay on configured models
- Global config override: `routing.force_model = "sonnet"` bypasses classification entirely
- Per-message prefix override: user types `/opus analyze this...` or `/haiku what time is it` to force a model
- Both mechanisms coexist -- global config for default behavior, per-message for power users
- Proactive check-ins: review pending items, reminders, follow-ups
- Personal assistant behavior, not infrastructure monitoring
- User-configurable: `heartbeat.delivery = "immediate" | "on_next_message"`
- Immediate: heartbeat sends a Telegram message directly
- On next message: stores the proactive insight, weaves it into next user interaction
- Distinct visual format -- prefix or header so user knows it's a check-in, not a response
- When daily budget >80% consumed: downgrade Opus to Sonnet, Sonnet to Haiku
- When daily budget >95% consumed: everything routes to Haiku
- Transparent notification to user when downgrades happen
- Separate dedicated budget: `heartbeat.monthly_budget_usd = 10.0`
- Heartbeat budget cannot eat into conversation budget
- Cost ledger tracks both `intended_model` and `actual_model` per call

### Claude's Discretion
- Crate organization (standalone `blufio-router` vs module in `blufio-agent`)
- Heartbeat scheduling strategy (fixed interval vs event-driven vs hybrid)
- Skip-when-unchanged detection logic
- Heuristic classification algorithm details (keyword lists, length thresholds, complexity indicators)
- Max tokens per tier adjustments
- Heartbeat content generation prompt design
- Distinct heartbeat message format specifics

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| LLM-05 | Model router classifies query complexity and routes to Haiku (simple), Sonnet (standard), or Opus (complex) | QueryClassifier heuristic with complexity scoring, ModelRouter with budget-aware downgrades, per-message overrides, and cost ledger integration |
| LLM-06 | Smart heartbeats run on Haiku with skip-when-unchanged logic, costing <=$10/month | HeartbeatRunner background task with dedicated budget, skip-when-unchanged via session state hashing, Haiku-only calls with proactive check-in prompts |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| (no new crates) | - | All functionality uses existing dependencies | Phase 6 is pure application logic -- no new external dependencies needed |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| regex | (existing) | Pattern matching for per-message prefix overrides (`/opus`, `/haiku`, `/sonnet`) | Parsing model override prefixes from message text |
| chrono | (existing) | Scheduling heartbeat intervals, timestamp comparisons | Heartbeat timing and skip-when-unchanged checks |
| tokio | (existing) | Background task spawning for heartbeat runner | `tokio::spawn` for heartbeat background loop |
| serde/serde_json | (existing) | Config deserialization for new `routing` and `heartbeat` sections | BlufioConfig extension |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Heuristic classifier | LLM-based pre-classification | Adds latency + cost per message; locked decision says heuristics only |
| Fixed interval heartbeats | Cron-style scheduling | Overkill for single-user agent; tokio::time::interval is simpler |
| Per-message regex parsing | Command framework | Only 3 commands (/opus, /haiku, /sonnet); framework is overhead |

## Architecture Patterns

### Recommended Crate Structure
```
crates/
├── blufio-router/         # New crate: QueryClassifier + ModelRouter
│   ├── src/
│   │   ├── lib.rs         # Public API
│   │   ├── classifier.rs  # Heuristic complexity classification
│   │   └── router.rs      # Model routing with budget awareness
│   └── Cargo.toml
├── blufio-agent/          # Extended: HeartbeatRunner, routing integration
│   └── src/
│       ├── heartbeat.rs   # NEW: HeartbeatRunner background task
│       ├── session.rs     # MODIFIED: Use router instead of static model
│       └── lib.rs         # MODIFIED: Accept router, spawn heartbeat
```

### Pattern 1: Heuristic Query Classification
**What:** Score-based classifier using multiple signals (message length, keyword presence, question type, conversation momentum)
**When to use:** Every user-facing message before model selection

```rust
pub enum ComplexityTier {
    Simple,   // Haiku: greetings, time queries, single-fact lookups
    Standard, // Sonnet: general conversation, moderate analysis
    Complex,  // Opus: multi-step reasoning, code generation, nuanced analysis
}

pub struct ClassificationResult {
    pub tier: ComplexityTier,
    pub confidence: f32, // 0.0-1.0
    pub reason: &'static str,
}

impl QueryClassifier {
    pub fn classify(&self, message: &str, recent_context: &[&str]) -> ClassificationResult {
        let mut score: i32 = 0;
        // Signal 1: Message length
        // Signal 2: Complexity keywords
        // Signal 3: Question structure
        // Signal 4: Conversation momentum from recent context
        // Map score to tier with confidence
    }
}
```

### Pattern 2: Budget-Aware Downgrade
**What:** Check current budget utilization and downgrade tiers when approaching caps
**When to use:** After classification, before final model selection

```rust
pub struct RoutingDecision {
    pub intended_model: String,  // What classifier chose
    pub actual_model: String,    // What budget allows
    pub downgraded: bool,        // Whether budget forced a downgrade
    pub max_tokens: u32,         // Tier-appropriate max tokens
}
```

### Pattern 3: Heartbeat Background Task
**What:** Periodic background task that checks for proactive insights and sends them
**When to use:** Spawned once at startup, runs alongside the main agent loop

```rust
// Spawned in serve.rs alongside Telegram polling
tokio::spawn(async move {
    let mut interval = tokio::time::interval(heartbeat_interval);
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if heartbeat_runner.should_run().await {
                    heartbeat_runner.execute().await;
                }
            }
            _ = cancel.cancelled() => break,
        }
    }
});
```

### Anti-Patterns to Avoid
- **LLM-based classification:** Adds latency and cost to every message; defeats the purpose of cheap routing
- **Global model field on SessionActor:** Must be per-message, not per-session, since routing changes per query
- **Heartbeat eating conversation budget:** Must use separate budget tracker to enforce $10/month cap
- **Blocking heartbeat in agent loop:** Must be a separate spawned task, not inline in the message handler

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Budget percentage calculation | Custom percentage math | Extend BudgetTracker with `utilization_percent()` | Already tracks daily/monthly totals; just add ratio method |
| Cost ledger schema extension | Raw SQL migration | SQLite migration in blufio-storage | Existing V2 migration pattern established |
| Per-message model override parsing | Custom string parsing | Simple `str::strip_prefix` or regex | Only 3 prefixes to match; stdlib is sufficient |

## Common Pitfalls

### Pitfall 1: Classification Too Aggressive on Haiku
**What goes wrong:** Too many messages routed to Haiku, quality drops noticeably
**Why it happens:** Overly broad "simple" category catches moderate queries
**How to avoid:** Default uncertain cases UP to Sonnet (locked decision); keep Haiku tier narrow (greetings, timestamps, yes/no, single-word responses)
**Warning signs:** Users consistently unhappy with response quality

### Pitfall 2: Heartbeat Budget Leak
**What goes wrong:** Heartbeat costs slowly exceed $10/month, or heartbeat budget affects conversation budget
**Why it happens:** Shared BudgetTracker instance, or missing budget isolation
**How to avoid:** Separate HeartbeatBudgetTracker with its own monthly cap; record heartbeat costs in ledger with FeatureType::Heartbeat but don't add to conversation budget tracker
**Warning signs:** Monthly heartbeat costs exceeding $10; conversation budget depleting faster than expected

### Pitfall 3: Context Assembly with Wrong Model
**What goes wrong:** Context engine assembles for Sonnet but routing switches to Haiku after assembly
**Why it happens:** Routing happens after context assembly instead of before
**How to avoid:** Route BEFORE context assembly; pass the routed model to `context_engine.assemble()`
**Warning signs:** Token counts mismatched between assembled context and model limits

### Pitfall 4: Heartbeat Skip Logic Too Aggressive
**What goes wrong:** Heartbeat never fires because "nothing changed" is too broadly defined
**Why it happens:** Comparing too much state; any session activity looks like "change"
**How to avoid:** Skip-when-unchanged should check a specific state hash (pending reminders, scheduled items, last heartbeat content); not overall session activity
**Warning signs:** Heartbeat never triggers despite being enabled

### Pitfall 5: Per-Message Override Not Stripped Before LLM
**What goes wrong:** User types `/opus analyze this code` and the LLM sees the `/opus` prefix
**Why it happens:** Override prefix parsed but not removed from message content
**How to avoid:** Strip the override prefix from the message content before persisting and sending to the LLM
**Warning signs:** LLM responses referencing "/opus" or "/haiku" commands

## Code Examples

### QueryClassifier Heuristic Signals

```rust
// Signal: Simple greetings and short queries
const SIMPLE_PATTERNS: &[&str] = &[
    "hi", "hello", "hey", "thanks", "thank you", "bye", "ok", "okay",
    "yes", "no", "sure", "good", "great",
];

const SIMPLE_QUESTIONS: &[&str] = &[
    "what time", "what day", "what date", "how are you",
    "what's up", "who are you",
];

// Signal: Complex indicators
const COMPLEX_INDICATORS: &[&str] = &[
    "analyze", "compare", "evaluate", "implement", "design",
    "architecture", "trade-off", "tradeoff", "pros and cons",
    "step by step", "explain in detail", "debug", "refactor",
    "code review", "write a function", "write code",
];

fn message_length_score(text: &str) -> i32 {
    let word_count = text.split_whitespace().count();
    match word_count {
        0..=3 => -2,    // Very short -> likely simple
        4..=15 => 0,    // Medium -> neutral
        16..=50 => 1,   // Longer -> leaning complex
        _ => 2,         // Very long -> likely complex
    }
}

fn keyword_score(text: &str) -> i32 {
    let lower = text.to_lowercase();
    if SIMPLE_PATTERNS.iter().any(|p| lower == *p) {
        return -3; // Strong simple signal
    }
    if SIMPLE_QUESTIONS.iter().any(|q| lower.contains(q)) {
        return -2;
    }
    if COMPLEX_INDICATORS.iter().any(|c| lower.contains(c)) {
        return 2; // Strong complex signal
    }
    0
}

fn has_code_block(text: &str) -> bool {
    text.contains("```") || text.contains("    ") // indented code
}

fn conversation_momentum(recent: &[&str]) -> i32 {
    // If recent messages show sustained complexity (code, analysis),
    // bias toward maintaining the same tier
    let complex_count = recent.iter()
        .filter(|m| COMPLEX_INDICATORS.iter().any(|c| m.to_lowercase().contains(c)))
        .count();
    if complex_count >= 2 { 1 } else { 0 }
}
```

### Per-Message Model Override

```rust
/// Check for per-message model override prefix.
/// Returns (override_model, cleaned_message) if prefix found.
pub fn parse_model_override(text: &str) -> (Option<&'static str>, &str) {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("/opus ") {
        (Some("claude-opus-4-20250514"), rest)
    } else if let Some(rest) = trimmed.strip_prefix("/haiku ") {
        (Some("claude-haiku-4-5-20250901"), rest)
    } else if let Some(rest) = trimmed.strip_prefix("/sonnet ") {
        (Some("claude-sonnet-4-20250514"), rest)
    } else {
        (None, text)
    }
}
```

### Budget-Aware Downgrade

```rust
impl ModelRouter {
    pub fn route(&self, message: &str, recent: &[&str], budget_pct: f64) -> RoutingDecision {
        // 1. Check for per-message override
        let (override_model, clean_msg) = parse_model_override(message);
        if let Some(model) = override_model {
            return RoutingDecision {
                intended_model: model.to_string(),
                actual_model: model.to_string(),
                downgraded: false,
                max_tokens: self.max_tokens_for_model(model),
            };
        }

        // 2. Check for global force_model config
        if let Some(ref forced) = self.force_model {
            return RoutingDecision { /* forced model */ };
        }

        // 3. Classify complexity
        let classification = self.classifier.classify(clean_msg, recent);

        // 4. Map tier to model
        let intended = match classification.tier {
            ComplexityTier::Simple => "claude-haiku-4-5-20250901",
            ComplexityTier::Standard => "claude-sonnet-4-20250514",
            ComplexityTier::Complex => "claude-opus-4-20250514",
        };

        // 5. Apply budget downgrade
        let actual = if budget_pct >= 0.95 {
            "claude-haiku-4-5-20250901" // Everything to Haiku
        } else if budget_pct >= 0.80 {
            match classification.tier {
                ComplexityTier::Complex => "claude-sonnet-4-20250514", // Opus -> Sonnet
                ComplexityTier::Standard => "claude-haiku-4-5-20250901", // Sonnet -> Haiku
                ComplexityTier::Simple => "claude-haiku-4-5-20250901",
            }
        } else {
            intended
        };

        RoutingDecision {
            intended_model: intended.to_string(),
            actual_model: actual.to_string(),
            downgraded: intended != actual,
            max_tokens: self.max_tokens_for_model(actual),
        }
    }
}
```

### Config Extensions

```rust
// New config sections in BlufioConfig
pub struct RoutingConfig {
    /// Enable model routing. When false, uses anthropic.default_model for all messages.
    pub enabled: bool,             // default: true
    /// Force all messages to a specific model, bypassing classification.
    pub force_model: Option<String>, // default: None
    /// Model for simple queries.
    pub simple_model: String,      // default: "claude-haiku-4-5-20250901"
    /// Model for standard queries.
    pub standard_model: String,    // default: "claude-sonnet-4-20250514"
    /// Model for complex queries.
    pub complex_model: String,     // default: "claude-opus-4-20250514"
    /// Max tokens for simple tier.
    pub simple_max_tokens: u32,    // default: 1024
    /// Max tokens for standard tier.
    pub standard_max_tokens: u32,  // default: 4096
    /// Max tokens for complex tier.
    pub complex_max_tokens: u32,   // default: 8192
}

pub struct HeartbeatConfig {
    /// Enable smart heartbeats.
    pub enabled: bool,             // default: false (opt-in)
    /// Heartbeat interval in seconds.
    pub interval_secs: u64,        // default: 3600 (1 hour)
    /// Delivery mode: "immediate" or "on_next_message".
    pub delivery: String,          // default: "on_next_message"
    /// Monthly budget for heartbeats in USD.
    pub monthly_budget_usd: f64,   // default: 10.0
    /// Model to use for heartbeat calls (always Haiku).
    pub model: String,             // default: "claude-haiku-4-5-20250901"
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single model per agent | Per-request model routing | Common in 2025+ with multi-model providers | Cost reduction 40-60% on mixed workloads |
| Timer-based heartbeats | Skip-when-unchanged heartbeats | Emerging pattern | Prevents unnecessary API spend |
| Fixed budget caps | Graduated degradation (80%/95%) | Cost-aware routing pattern | Graceful quality reduction vs hard cutoff |

## Open Questions

1. **Max tokens per tier calibration**
   - What we know: Haiku works well with shorter outputs, Opus benefits from longer context
   - What's unclear: Optimal max_tokens for each tier in this specific use case
   - Recommendation: Start with 1024/4096/8192, tune based on actual usage patterns

2. **Heartbeat content prompt design**
   - What we know: Heartbeat should review pending items, reminders, follow-ups
   - What's unclear: Exactly what system prompt produces useful proactive insights without hallucinating tasks
   - Recommendation: Use a focused prompt that reviews recent conversation summaries and only generates heartbeats when there's something actionable

3. **Skip-when-unchanged state hash scope**
   - What we know: Hash should capture "meaningful state" that would produce different heartbeat content
   - What's unclear: Which state dimensions to hash (recent messages? pending items? time of day?)
   - Recommendation: Hash (last_heartbeat_content, message_count_since_last_heartbeat, date); if hash unchanged, skip

## Sources

### Primary (HIGH confidence)
- Codebase analysis: `SessionActor.handle_message()` in `blufio-agent/src/session.rs` -- current single-model flow
- Codebase analysis: `ProviderRequest.model` field in `blufio-core/src/types.rs` -- already supports per-request model
- Codebase analysis: `BudgetTracker` in `blufio-cost/src/budget.rs` -- existing budget infrastructure
- Codebase analysis: `CostLedger` + `CostRecord` in `blufio-cost/src/ledger.rs` -- existing cost recording
- Codebase analysis: `FeatureType::Heartbeat` already defined in `blufio-cost/src/ledger.rs`
- Codebase analysis: `pricing::get_pricing()` in `blufio-cost/src/pricing.rs` -- already handles Haiku/Sonnet/Opus
- Codebase analysis: `serve.rs` -- startup initialization pattern for spawning background tasks
- Codebase analysis: `BlufioConfig` in `blufio-config/src/model.rs` -- config extension pattern

### Secondary (MEDIUM confidence)
- Anthropic pricing docs (verified 2026-03-01): Haiku $0.80/$4.00, Sonnet $3.00/$15.00, Opus $15.00/$75.00 per MTok

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - No new external dependencies; pure application logic on existing infrastructure
- Architecture: HIGH - Clear integration points identified; existing patterns (cost recording, config, background tasks) directly reusable
- Pitfalls: HIGH - All pitfalls derive from known code paths and architectural constraints

**Research date:** 2026-03-01
**Valid until:** 2026-04-01 (stable -- no external dependencies to change)
