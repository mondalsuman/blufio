# Phase 56: Multi-Level Compaction & Context Budget - Research

**Researched:** 2026-03-11
**Domain:** Context management, LLM summarization, token budget enforcement, SQLite storage
**Confidence:** HIGH

## Summary

Phase 56 transforms Blufio's single-level context compaction into a 4-level progressive summarization system (L0-L3) with quality gates, entity extraction, archive storage, and per-zone token budget enforcement. The existing codebase provides a solid foundation: `blufio-context` already has a working single-level compaction in `compaction.rs` and three-zone assembly in `lib.rs`, `blufio-memory` has `MemorySource::Extracted` ready for entity storage, `blufio-bus` has a mature event publishing system, and `blufio-core::token_counter::TokenizerCache` provides provider-specific token counting. The main engineering challenges are: (1) refactoring the flat `compaction.rs` into a `compaction/` module directory without breaking re-exports, (2) handling backward compatibility for the deprecated `compaction_threshold` config key given `#[serde(deny_unknown_fields)]`, (3) correctly cascading compaction levels within a single assembly call, and (4) wiring the archive ConditionalProvider with lowest priority ordering.

**Primary recommendation:** Start with config schema extension and migration, then build the compaction level engine bottom-up (L1 first, then L2/L3), add quality scoring as a separate concern, wire budget enforcement into the existing assembly flow, and finish with CLI commands and Prometheus metrics.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Soft trigger (50% of dynamic zone budget) fires L0->L1 compaction (turn-pair summaries)
- Hard trigger (85% of dynamic zone budget) escalates: L1->L2 (session summary), then L2->L3 (cross-session archive)
- Cascade within same assembly call: if L1 isn't enough to get below hard trigger, L2 fires immediately
- Session-scoped through L2; L3 is cross-session (combines multiple L2 summaries per user)
- L3 archive generated automatically on session close
- Original messages deleted after compaction (audit trail records the event)
- Compaction runs inline during context assembly (blocking, current behavior)
- On compaction failure (LLM error, quality gate rejection): truncate oldest messages and continue, never block agent loop
- Replace existing single compaction_threshold (0.70) with soft_trigger (0.50) and hard_trigger (0.85) -- old config key generates deprecation warning
- L1 bullet-point format: each user-assistant turn pair gets a 1-2 sentence bullet summary
- Level-dependent max_tokens defaults: L1=256 per turn-pair, L2=1024 for session summary, L3=2048 for cross-session archive -- all configurable via TOML
- Summaries stored with metadata: {"compaction_level": "L1", "original_count": N, "quality_score": 0.82, ...}
- Single configurable compaction_model string in [context] TOML section -- works with any provider
- Uses existing ProviderAdapter via model routing (no dedicated compaction provider)
- compaction_enabled toggle: [context] compaction_enabled = true (default) -- set to false to disable all compaction
- Entity extraction runs before L1 compaction only (not before higher levels)
- Extracted entities stored as Memory entries via existing blufio-memory with MemorySource::Extracted
- LLM-based quality evaluation via separate call (not combined with compaction call)
- Scoring call receives full original messages + generated summary for accurate evaluation
- Structured JSON output: {"entity": 0.9, "decision": 0.8, "action": 0.75, "numerical": 0.85}
- Weighted score: entity_retention (35%), decision_retention (25%), action_retention (25%), numerical_retention (15%) -- weights configurable
- Quality gates: >=0.6 proceed, 0.4-0.6 retry, <0.4 abort -- thresholds configurable
- 1 retry on 0.4-0.6: retry prompt emphasizes the weakest dimension specifically
- If JSON parsing fails: treat as 0.5 score (retry range), log warning
- quality_scoring toggle: [context] quality_scoring = true (default)
- Hardcoded evaluation prompt template; only weights and thresholds configurable
- Same compaction_model used for scoring (no separate model config)
- Quality scores persisted in compaction summary metadata
- Prometheus: blufio_compaction_quality_score histogram, blufio_compaction_gate_total{result=proceed|retry|abort} counter
- CLI: blufio context compact --dry-run --session <id>
- Dedicated compaction_archives SQLite table in main DB
- Archives in main DB = automatic inclusion in existing backup/restore flow
- Rolling window: keep last N archive summaries (default 10, configurable) -- oldest merged into single "deep archive"
- Archive retrieval: automatic injection via ConditionalProvider at lowest priority
- Archive injection bounded by zone 2 (conditional) token budget
- Session IDs tracked as JSON array for GDPR erasure traceability
- Restricted messages filtered before compaction/archiving (consistent with Phase 53)
- Archives inherit highest classification of their source messages
- archive_enabled toggle: [context] archive_enabled = true (default)
- CLI: blufio context archive list|view|prune subcommands
- Static zone: configurable budget (default 3,000 tokens) -- advisory only, warn at startup if exceeded, never truncate
- Conditional zone: configurable budget (default 8,000 tokens) with 10% safety margin -- truncate by provider priority
- Dynamic zone: adaptive budget = context_budget - actual_static_tokens - actual_conditional_tokens
- Soft/hard compaction thresholds apply to dynamic zone's adaptive budget (not total context budget)
- 10% safety margin hardcoded (not configurable)
- Provider-specific token counting via TokenizerCache.get_counter(model) from Phase 47
- Per-zone configurable: static_zone_budget, conditional_zone_budget in [context] TOML section
- AssembledContext gains dropped_providers: Vec<String> field for debugging
- Prometheus: blufio_context_zone_tokens{zone=static|conditional|dynamic} gauge per assembly
- CLI: blufio context status --session <id>
- CompactionStarted/CompactionCompleted events via EventBus
- Extend existing blufio-context crate with new modules: compaction/levels.rs, compaction/quality.rs, compaction/archive.rs, compaction/extract.rs, budget.rs
- blufio-context gains blufio-memory and blufio-bus dependencies
- Extended [context] TOML section with ~17 new fields
- #[serde(deny_unknown_fields)] consistent with other config sections

### Claude's Discretion
- Exact compaction and quality scoring prompt templates
- Internal module structure details within compaction/
- Exact Prometheus metric label values beyond what's specified
- Migration version numbering for compaction_archives table
- Test fixture organization and edge case selection
- Entity extraction prompt design
- Archive ConditionalProvider implementation details
- Deep archive merge prompt design
- Exact SQL queries for archive CRUD
- Budget enforcement algorithm details within zones

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| COMP-01 | 4 compaction levels (L0->L1->L2->L3) | Existing `compaction.rs` refactored into `compaction/` module directory; `generate_compaction_summary()` becomes L0->L1 path; new level progression engine in `levels.rs` |
| COMP-02 | Quality scoring with weighted dimensions (entity 35%, decision 25%, action 25%, numerical 15%) | Separate LLM call using same `compaction_model` via `ProviderAdapter::complete()`; structured JSON parsing with fallback |
| COMP-03 | Quality gates (>=0.6 proceed, 0.4-0.6 retry, <0.4 abort) | Gate logic in `quality.rs`; retry prompt targets weakest dimension; JSON parse failure treated as 0.5 |
| COMP-04 | Soft trigger at 50%, hard trigger at 85% | Replace existing `compaction_threshold` (0.70) in `DynamicZone`; thresholds apply to adaptive dynamic zone budget |
| COMP-05 | Archive system with cold storage retrieval | New `compaction_archives` SQLite table (V13 migration); `ArchiveConditionalProvider` implements existing `ConditionalProvider` trait |
| COMP-06 | Entity/fact extraction before compaction as Memory entries | LLM extraction call before L1; results stored via `MemoryStore::save()` with `MemorySource::Extracted` |
| CTXE-01 | Static zone enforces configurable token budget (3,000 tokens) | `StaticZone` gains token counting via `TokenizerCache`; advisory warning at startup, never truncate |
| CTXE-02 | Conditional zone enforces configurable token budget (8,000 tokens) with 10% safety margin | New `budget.rs` module; truncation by provider priority (memory > skills > archive); `dropped_providers` tracking |
| CTXE-03 | Provider-specific token counting (tiktoken-rs/HuggingFace) | Already implemented in `blufio-core::token_counter::TokenizerCache::get_counter(model)` and `count_with_fallback()` |
</phase_requirements>

## Standard Stack

### Core (existing -- no new external dependencies)
| Library | Purpose | Already In Project |
|---------|---------|-------------------|
| blufio-context | Three-zone context engine, compaction | Yes -- primary modification target |
| blufio-config | ContextConfig struct extension | Yes -- add ~17 new fields |
| blufio-core | ProviderAdapter, StorageAdapter, TokenizerCache, Message types | Yes -- read-only consumer |
| blufio-memory | MemoryStore for entity extraction storage | Yes -- new dependency for blufio-context |
| blufio-bus | EventBus for CompactionStarted/Completed events | Yes -- new dependency for blufio-context |
| blufio-storage | Database, migrations, message queries | Yes -- new migration + new queries |
| blufio-prometheus | Metric registration and recording | Yes -- new compaction/zone metrics |
| blufio-security | ClassificationGuard for restricted message filtering | Yes -- already a dependency |

### Supporting
| Library | Version | Purpose | Status |
|---------|---------|---------|--------|
| serde_json | 1.x | JSON metadata parsing, quality score parsing | Already workspace dep |
| chrono | workspace | Timestamps for archive entries | Already workspace dep |
| uuid | workspace | Archive IDs | Already workspace dep |
| tokio-rusqlite | workspace | Async SQLite for archive table | Already in blufio-storage |
| rusqlite | workspace | Sync queries for CLI commands | Already in blufio-storage |
| metrics | workspace | Prometheus metric facade | Already workspace dep |

### No New Crate Dependencies Required
This phase extends existing infrastructure. No new external crates needed. The only new dependency edges are:
- `blufio-context` -> `blufio-memory` (for entity extraction storage)
- `blufio-context` -> `blufio-bus` (for compaction events)

## Architecture Patterns

### Module Refactoring: compaction.rs -> compaction/ Directory
```
crates/blufio-context/src/
  compaction/
    mod.rs          # Re-exports (maintains backward compat with `crate::compaction::*`)
    levels.rs       # L0-L3 level progression engine, cascade logic
    quality.rs      # Quality scoring, gates, retry logic
    archive.rs      # Archive storage, retrieval, rolling window, deep merge
    extract.rs      # Entity/fact extraction before compaction
  budget.rs         # Per-zone token budget enforcement
  conditional.rs    # (existing) + ArchiveConditionalProvider
  dynamic.rs        # (existing) + soft/hard trigger, cascade, adaptive budget
  static_zone.rs    # (existing) + token counting for budget warning
  lib.rs            # (existing) + AssembledContext.dropped_providers, budget orchestration
```

**Critical: Re-export Compatibility.** The current `lib.rs` does:
```rust
pub use compaction::{generate_compaction_summary, persist_compaction_summary};
```
After refactoring `compaction.rs` -> `compaction/mod.rs`, these re-exports must still work. The `mod.rs` must re-export everything the old `compaction.rs` exported.

### Pattern 1: Compaction Level Progression
**What:** A state machine that progresses through L0->L1->L2->L3 based on trigger thresholds and cascade logic.
**When to use:** During `DynamicZone::assemble_messages()` when token budget is exceeded.
**Key insight:** The cascade happens within a single assembly call. If L1 compaction doesn't reduce tokens below the hard trigger, L2 fires immediately in the same call.

```rust
// Source: Phase 56 CONTEXT.md decisions
pub enum CompactionLevel {
    L0,  // Raw messages
    L1,  // Turn-pair summaries (bullet points)
    L2,  // Session summary (narrative)
    L3,  // Cross-session archive
}

pub struct CompactionResult {
    pub summary: String,
    pub level: CompactionLevel,
    pub quality_score: Option<f64>,
    pub tokens_saved: usize,
    pub original_count: usize,
    pub metadata: serde_json::Value,
}

/// Core compaction engine. Runs L0->L1 compaction on turn-pairs.
/// If still over hard_trigger after L1, cascades to L2.
pub async fn compact_messages(
    provider: &dyn ProviderAdapter,
    messages: &[Message],
    level: CompactionLevel,
    config: &CompactionConfig,
) -> Result<CompactionResult, BlufioError> { ... }
```

### Pattern 2: Quality Gate with Retry
**What:** Separate LLM call evaluates compaction quality, with configurable thresholds and single retry.
**When to use:** After every compaction summary is generated (unless quality_scoring is disabled).

```rust
pub struct QualityScores {
    pub entity: f64,
    pub decision: f64,
    pub action: f64,
    pub numerical: f64,
}

pub enum GateResult {
    Proceed(f64),       // score >= 0.6
    Retry(f64, String), // 0.4 <= score < 0.6, weakest dimension name
    Abort(f64),         // score < 0.4
}

impl QualityScores {
    pub fn weighted_score(&self, weights: &QualityWeights) -> f64 {
        self.entity * weights.entity
            + self.decision * weights.decision
            + self.action * weights.action
            + self.numerical * weights.numerical
    }

    pub fn weakest_dimension(&self) -> &str {
        // Return name of lowest-scoring dimension for retry prompt
    }
}
```

### Pattern 3: Zone Budget Enforcement
**What:** Each zone has a token budget. Dynamic zone budget is adaptive (total - static - conditional).
**When to use:** During context assembly in `ContextEngine::assemble()`.

```rust
pub struct ZoneBudget {
    pub static_budget: u32,     // Default 3000, advisory
    pub conditional_budget: u32, // Default 8000, enforced with 10% margin
    pub safety_margin: f64,      // Hardcoded 0.10
}

impl ZoneBudget {
    pub fn dynamic_budget(&self, total_budget: u32, actual_static: u32, actual_conditional: u32) -> u32 {
        total_budget.saturating_sub(actual_static).saturating_sub(actual_conditional)
    }

    pub fn conditional_effective(&self) -> u32 {
        // Budget minus 10% safety margin
        (self.conditional_budget as f64 * (1.0 - self.safety_margin)) as u32
    }
}
```

### Pattern 4: Archive ConditionalProvider
**What:** Implements `ConditionalProvider` trait to inject archive summaries at lowest priority.
**When to use:** Registered last in the provider chain (after memory, skills, MCP trust zone).

```rust
pub struct ArchiveConditionalProvider {
    db: Arc<Database>,
    token_cache: Arc<TokenizerCache>,
}

#[async_trait]
impl ConditionalProvider for ArchiveConditionalProvider {
    async fn provide_context(&self, session_id: &str) -> Result<Vec<ProviderMessage>, BlufioError> {
        // 1. Lookup user_id from session
        // 2. Fetch most recent archive summaries for user
        // 3. Format as system message: "Historical context: ..."
        // 4. Return as ProviderMessage vec
    }
}
```

### Pattern 5: EventBus Integration (existing pattern)
**What:** Fire-and-forget events emitted before/after compaction for audit trail, metrics, hooks.
**When to use:** In compaction engine before starting and after completion.

```rust
// Follows existing pattern from Phase 54: String fields, no cross-crate type deps
pub enum CompactionEvent {
    Started {
        event_id: String,
        timestamp: String,
        session_id: String,
        level: String,         // "L1", "L2", "L3"
        message_count: u32,
    },
    Completed {
        event_id: String,
        timestamp: String,
        session_id: String,
        level: String,
        quality_score: f64,
        tokens_saved: u32,
        duration_ms: u64,
    },
}
```

### Anti-Patterns to Avoid
- **Grading own work in same call:** Quality scoring MUST be a separate LLM call. Having the model evaluate its own summary in the same request produces artificially inflated scores.
- **Blocking the agent loop on compaction failure:** On LLM error or quality gate abort, truncate oldest messages and continue. Never panic or propagate the error to block the conversation.
- **Computing dynamic zone budget from total budget minus zone defaults:** Use ACTUAL static and conditional token counts, not configured budgets, for the dynamic zone calculation.
- **Registering archive provider before memory/skills:** Archive must be registered LAST to get lowest priority. The order of `add_conditional_provider()` calls determines the ordering in the output.
- **Using `#[serde(rename)]` for the deprecated `compaction_threshold`:** Since `deny_unknown_fields` is active, the old key must remain as a valid field. Use a custom deserializer or keep the field with a deprecation warning at runtime.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Token counting | Custom char/word heuristics for budget checks | `TokenizerCache::get_counter(model)` + `count_with_fallback()` | Already provides provider-specific tokenizers with automatic fallback |
| Message deletion | Raw SQL DELETE in compaction code | Add `delete_messages_by_ids()` to StorageAdapter | Consistent with existing trait pattern; SQL in storage crate only |
| Event emission | Direct tokio channel writes | `EventBus::publish()` | Handles broadcast + reliable delivery, existing pattern |
| Memory storage | Custom entity DB table | `MemoryStore::save()` with `MemorySource::Extracted` | Memory infrastructure already handles embedding, search, eviction |
| JSON structured output | Manual string parsing of LLM quality scores | `serde_json::from_str::<QualityScores>()` with fallback | Handles malformed JSON gracefully; fallback to 0.5 score |
| Archive SQLite ops | Inline SQL in context crate | Functions in `blufio-storage/src/queries/` | Follows existing query organization pattern |
| Config defaults | Hardcoded constants scattered in code | `default_*()` functions in `blufio-config/src/model.rs` | Consistent with all other config sections |

**Key insight:** This phase is primarily an extension of existing infrastructure. Every building block (provider calls, storage, event bus, memory, token counting, metrics) already exists. The engineering is in the orchestration and the new compaction logic, not in new infrastructure.

## Common Pitfalls

### Pitfall 1: deny_unknown_fields vs. Deprecated compaction_threshold
**What goes wrong:** Adding `soft_trigger` and `hard_trigger` while removing `compaction_threshold` from the struct would break all existing configs that have `compaction_threshold` set, because `deny_unknown_fields` rejects unknown keys.
**Why it happens:** The user locked the decision to generate a deprecation warning, meaning both old and new keys must be accepted.
**How to avoid:** Keep `compaction_threshold` as an `Option<f64>` field in the struct. During initialization, if `compaction_threshold` is `Some` and `soft_trigger` is at default, emit a deprecation warning via `tracing::warn!` and use the old value as `soft_trigger`. If both are explicitly set, prefer `soft_trigger` and warn that `compaction_threshold` is ignored.
**Warning signs:** Config validation tests fail with "unknown field" errors.

### Pitfall 2: Cascade Compaction Infinite Loop
**What goes wrong:** If L1 compaction doesn't reduce enough tokens and L2 still doesn't, the system could loop forever trying to compact.
**Why it happens:** Cascade logic re-checks token budget after each level.
**How to avoid:** Cap cascade at one level up. After L1, if still over hard trigger, try L2. After L2, if still over, fall back to truncation (not L3, which is cross-session). L3 only fires on session close.
**Warning signs:** Assembly takes seconds instead of milliseconds; multiple compaction events emitted in one call.

### Pitfall 3: Re-export Breakage When Refactoring to Module Directory
**What goes wrong:** Moving `compaction.rs` to `compaction/mod.rs` can break `pub use compaction::*` imports throughout the workspace.
**Why it happens:** Rust treats `mod compaction;` the same whether it resolves to `compaction.rs` or `compaction/mod.rs`, BUT the internal module structure changes.
**How to avoid:** The `compaction/mod.rs` must re-export `generate_compaction_summary` and `persist_compaction_summary` at the module root. Also keep `lib.rs` re-exports unchanged. Run `cargo check` across entire workspace after the refactor.
**Warning signs:** Compilation errors in `blufio/src/shell.rs` or `blufio/src/serve.rs` that reference `blufio_context::compaction::*`.

### Pitfall 4: No delete_messages in StorageAdapter
**What goes wrong:** The decision says "original messages deleted after compaction" but `StorageAdapter` trait has no `delete_messages` method.
**Why it happens:** The existing single-level compaction doesn't delete messages -- it just adds a summary.
**How to avoid:** Add `delete_messages_by_ids(&self, ids: &[&str])` to `StorageAdapter` trait. Implement in `blufio-storage`. Add to all trait implementors (including mock/test implementations). Alternatively, use direct Database access within the context crate (since compaction already has storage access).
**Warning signs:** Compacted messages reappear in next assembly because they were never deleted.

### Pitfall 5: Quality Scoring LLM Response Not JSON
**What goes wrong:** The quality scoring LLM call may return prose instead of JSON, or malformed JSON.
**Why it happens:** LLMs don't always follow instructions perfectly, especially smaller/cheaper models used for compaction.
**How to avoid:** Parse with `serde_json::from_str()`, on failure treat as 0.5 score (retry range), log a warning. On retry, if JSON parse fails again, treat as abort (truncate and continue). Never let a parse failure block the agent loop.
**Warning signs:** Consistent quality score of 0.5 in metrics, high retry rate.

### Pitfall 6: Token Count Drift Between Estimation and Reality
**What goes wrong:** The budget enforcement uses token counting to decide when to compact, but the actual tokens consumed by the LLM may differ from the estimate.
**Why it happens:** Tokenizers have varying accuracy (~80-95% for community Claude tokenizer), and the LLM provider may count tokens differently.
**How to avoid:** The 10% safety margin exists precisely for this reason. Don't try to be exact -- the safety margin absorbs the drift. Use `count_with_fallback()` consistently throughout.
**Warning signs:** Context overflow errors from the LLM provider despite being "within budget."

### Pitfall 7: Memory Struct Missing user_id Field
**What goes wrong:** Entity extraction stores Memory entries but Memory has `session_id: Option<String>` and no `user_id` field. Archives need user_id for cross-session lookup.
**Why it happens:** Memory was designed for session-scoped knowledge.
**How to avoid:** For entity extraction, Memory entries use session_id (they are session-scoped). For archive retrieval, look up user_id from the sessions table. The `compaction_archives` table has its own `user_id` column.
**Warning signs:** Archives cannot be retrieved for a user because there's no user->archive mapping.

## Code Examples

### Example 1: Compaction Level Metadata (extending existing pattern)
```rust
// Source: existing compaction.rs metadata pattern, extended per CONTEXT.md
fn build_compaction_metadata(
    level: &CompactionLevel,
    original_count: usize,
    quality_score: Option<f64>,
    dimension_scores: Option<&QualityScores>,
) -> serde_json::Value {
    let mut meta = serde_json::json!({
        "type": "compaction_summary",
        "compaction_level": level.as_str(),
        "original_count": original_count,
        "compacted_at": chrono::Utc::now().to_rfc3339(),
    });
    if let Some(score) = quality_score {
        meta["quality_score"] = serde_json::json!(score);
    }
    if let Some(scores) = dimension_scores {
        meta["entity"] = serde_json::json!(scores.entity);
        meta["decision"] = serde_json::json!(scores.decision);
        meta["action"] = serde_json::json!(scores.action);
        meta["numerical"] = serde_json::json!(scores.numerical);
    }
    meta
}
```

### Example 2: Soft/Hard Trigger Logic in DynamicZone
```rust
// Source: existing dynamic.rs pattern, modified for dual triggers
let counter = self.token_cache.get_counter(model);
let mut estimated_tokens: usize = 0;
for m in &history {
    estimated_tokens += count_with_fallback(counter.as_ref(), &m.content).await;
}

let dynamic_budget = self.compute_dynamic_budget(actual_static, actual_conditional);
let soft_threshold = (dynamic_budget as f64 * self.soft_trigger) as usize;
let hard_threshold = (dynamic_budget as f64 * self.hard_trigger) as usize;

if estimated_tokens > soft_threshold && history.len() > 2 {
    // Fire L0->L1 compaction
    let result = compact_to_l1(provider, &older_messages, config).await;

    // Check if still over hard trigger after L1
    let new_estimate = recount_tokens(&result, &recent_messages, counter.as_ref()).await;
    if new_estimate > hard_threshold {
        // Cascade to L2
        let l2_result = compact_to_l2(provider, &l1_summaries, config).await;
    }
}
```

### Example 3: Quality Scoring Prompt (separate call)
```rust
// Source: Phase 56 CONTEXT.md decisions
const QUALITY_SCORING_PROMPT: &str = r#"You are evaluating a conversation summary for quality.

Compare the ORIGINAL conversation with the SUMMARY and score retention across four dimensions.

Return ONLY a JSON object (no other text):
{"entity": <0.0-1.0>, "decision": <0.0-1.0>, "action": <0.0-1.0>, "numerical": <0.0-1.0>}

Dimensions:
- entity: Are all people, places, products, and identifiers preserved?
- decision: Are all decisions and their rationale preserved?
- action: Are all action items and commitments preserved?
- numerical: Are all numbers, dates, amounts, and measurements preserved?

ORIGINAL:
{original}

SUMMARY:
{summary}"#;
```

### Example 4: Archive Table Migration (V13)
```sql
-- V13__compaction_archives.sql
CREATE TABLE IF NOT EXISTS compaction_archives (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    summary TEXT NOT NULL,
    quality_score REAL,
    session_ids TEXT NOT NULL DEFAULT '[]',  -- JSON array of session IDs
    classification TEXT NOT NULL DEFAULT 'internal',
    created_at TEXT NOT NULL,
    token_count INTEGER
);

CREATE INDEX IF NOT EXISTS idx_compaction_archives_user_id
    ON compaction_archives (user_id);
CREATE INDEX IF NOT EXISTS idx_compaction_archives_created_at
    ON compaction_archives (created_at);
```

### Example 5: ContextConfig Extension Pattern
```rust
// Source: existing model.rs ContextConfig pattern
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextConfig {
    // --- Existing fields ---
    #[serde(default = "default_compaction_model")]
    pub compaction_model: String,
    #[serde(default = "default_context_budget")]
    pub context_budget: u32,

    // --- Deprecated (backward compat) ---
    /// Deprecated: use soft_trigger instead. Emits warning if set.
    #[serde(default)]
    pub compaction_threshold: Option<f64>,

    // --- New compaction fields ---
    #[serde(default = "default_true")]
    pub compaction_enabled: bool,
    #[serde(default = "default_soft_trigger")]
    pub soft_trigger: f64,
    #[serde(default = "default_hard_trigger")]
    pub hard_trigger: f64,
    #[serde(default = "default_true")]
    pub quality_scoring: bool,
    #[serde(default = "default_quality_gate_proceed")]
    pub quality_gate_proceed: f64,
    #[serde(default = "default_quality_gate_retry")]
    pub quality_gate_retry: f64,
    #[serde(default = "default_quality_weight_entity")]
    pub quality_weight_entity: f64,
    #[serde(default = "default_quality_weight_decision")]
    pub quality_weight_decision: f64,
    #[serde(default = "default_quality_weight_action")]
    pub quality_weight_action: f64,
    #[serde(default = "default_quality_weight_numerical")]
    pub quality_weight_numerical: f64,
    #[serde(default = "default_max_tokens_l1")]
    pub max_tokens_l1: u32,
    #[serde(default = "default_max_tokens_l2")]
    pub max_tokens_l2: u32,
    #[serde(default = "default_max_tokens_l3")]
    pub max_tokens_l3: u32,
    #[serde(default = "default_static_zone_budget")]
    pub static_zone_budget: u32,
    #[serde(default = "default_conditional_zone_budget")]
    pub conditional_zone_budget: u32,
    #[serde(default = "default_true")]
    pub archive_enabled: bool,
    #[serde(default = "default_max_archives")]
    pub max_archives: u32,
}
```

### Example 6: EventBus Emission (follows existing pattern)
```rust
// Source: existing blufio-bus usage pattern (e.g., audit, memory events)
if let Some(bus) = &self.event_bus {
    let event = BusEvent::Compaction(CompactionEvent::Started {
        event_id: new_event_id(),
        timestamp: now_timestamp(),
        session_id: session_id.to_string(),
        level: "L1".to_string(),
        message_count: messages_to_compact.len() as u32,
    });
    bus.publish(event).await;
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single compaction_threshold (0.70) | Dual soft (0.50) / hard (0.85) triggers | Phase 56 | More nuanced compaction -- gentle summarization early, aggressive only when needed |
| Single-level summarization | 4-level progressive (L0->L1->L2->L3) | Phase 56 | Better context quality at each level; turn-pair granularity preserved in L1 |
| No quality evaluation | LLM-based quality scoring with gates | Phase 56 | Prevents lossy summaries from silently degrading context quality |
| No entity extraction | Pre-compaction entity extraction to Memory | Phase 56 | Critical facts survive summarization as separate searchable entries |
| No zone budgets | Per-zone token budget enforcement | Phase 56 | Prevents any single zone from consuming the entire context window |
| Compacted messages kept in DB | Original messages deleted after compaction | Phase 56 | Storage efficiency; audit trail preserves the event record |

**Deprecated:**
- `compaction_threshold` config key: replaced by `soft_trigger`/`hard_trigger`. Old key accepted with deprecation warning.

## Open Questions

1. **Message Deletion Mechanism**
   - What we know: `StorageAdapter` trait has no `delete_messages` method. Existing `blufio-storage/src/queries/messages.rs` has insert and get only.
   - What's unclear: Whether to add to the trait (affects all implementors) or use direct DB access within the context crate.
   - Recommendation: Add `delete_messages_by_ids()` to `StorageAdapter` trait for consistency. Only blufio-storage's SQLiteStorage implements it. The mock provider in tests can use a no-op. This is cleaner than bypassing the trait.

2. **User ID Resolution for Archives**
   - What we know: Archives need `user_id` for cross-session lookup. Sessions have `user_id: Option<String>`. Memory entries don't have user_id.
   - What's unclear: How to reliably get user_id during compaction (the DynamicZone receives session_id, not user_id).
   - Recommendation: Look up user_id from the sessions table during L3 archive generation. If user_id is None (anonymous sessions), use session_id as a fallback grouping key.

3. **ConditionalProvider Priority Ordering**
   - What we know: Providers are called in registration order. Archive must be last.
   - What's unclear: Whether `ContextEngine::add_conditional_provider()` needs a priority parameter, or if registration order is sufficient.
   - Recommendation: Registration order is sufficient (simpler). Document that archive registration must happen after all other providers. Both `serve.rs` and `shell.rs` show clear sequential registration.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in Rust test framework) |
| Config file | Cargo.toml [dev-dependencies] per crate |
| Quick run command | `cargo test -p blufio-context` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| COMP-01 | L0-L3 level progression with cascade | unit | `cargo test -p blufio-context compaction::levels::tests -x` | Wave 0 |
| COMP-02 | Quality scoring with weighted dimensions | unit | `cargo test -p blufio-context compaction::quality::tests -x` | Wave 0 |
| COMP-03 | Quality gates (proceed/retry/abort) | unit | `cargo test -p blufio-context compaction::quality::tests::gate -x` | Wave 0 |
| COMP-04 | Soft/hard trigger thresholds | unit | `cargo test -p blufio-context dynamic::tests::trigger -x` | Wave 0 |
| COMP-05 | Archive CRUD and rolling window | unit | `cargo test -p blufio-context compaction::archive::tests -x` | Wave 0 |
| COMP-06 | Entity extraction before L1 | unit | `cargo test -p blufio-context compaction::extract::tests -x` | Wave 0 |
| CTXE-01 | Static zone budget warning | unit | `cargo test -p blufio-context static_zone::tests::budget -x` | Wave 0 |
| CTXE-02 | Conditional zone budget enforcement | unit | `cargo test -p blufio-context budget::tests -x` | Wave 0 |
| CTXE-03 | Provider-specific token counting | unit | `cargo test -p blufio-core token_counter::tests -x` | Exists (Phase 47) |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-context && cargo test -p blufio-config`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full workspace green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/blufio-context/src/compaction/levels.rs` -- L0-L3 progression tests (COMP-01, COMP-04)
- [ ] `crates/blufio-context/src/compaction/quality.rs` -- quality scoring and gate tests (COMP-02, COMP-03)
- [ ] `crates/blufio-context/src/compaction/archive.rs` -- archive CRUD tests (COMP-05)
- [ ] `crates/blufio-context/src/compaction/extract.rs` -- entity extraction tests (COMP-06)
- [ ] `crates/blufio-context/src/budget.rs` -- zone budget enforcement tests (CTXE-01, CTXE-02)
- [ ] `crates/blufio-storage/migrations/V13__compaction_archives.sql` -- archive table schema
- [ ] `crates/blufio-storage/src/queries/archives.rs` -- archive query tests
- [ ] Mock ProviderAdapter for compaction tests (returns predictable summaries and quality scores)

## Sources

### Primary (HIGH confidence)
- Existing codebase: `crates/blufio-context/src/` -- all 5 source files read and analyzed
- Existing codebase: `crates/blufio-config/src/model.rs` -- ContextConfig struct with deny_unknown_fields
- Existing codebase: `crates/blufio-core/src/token_counter.rs` -- TokenizerCache, count_with_fallback
- Existing codebase: `crates/blufio-core/src/traits/storage.rs` -- StorageAdapter trait (no delete_messages)
- Existing codebase: `crates/blufio-memory/src/store.rs` -- MemoryStore::save() with MemorySource::Extracted
- Existing codebase: `crates/blufio-bus/src/events.rs` -- BusEvent enum, event patterns
- Existing codebase: `crates/blufio-bus/src/lib.rs` -- EventBus::publish() fire-and-forget pattern
- Existing codebase: `crates/blufio-storage/migrations/` -- V1-V12 migrations (next is V13)
- Existing codebase: `crates/blufio-prometheus/src/recording.rs` -- metric registration pattern

### Secondary (MEDIUM confidence)
- Phase 56 CONTEXT.md -- all locked decisions and implementation details from user discussion

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries are existing project dependencies, no new external crates
- Architecture: HIGH -- patterns directly derived from existing codebase conventions
- Pitfalls: HIGH -- identified from concrete codebase analysis (deny_unknown_fields, missing delete_messages, etc.)
- Config extension: HIGH -- directly verified ContextConfig struct and serde attributes
- Migration numbering: HIGH -- verified existing migrations (V1-V12, next is V13)

**Research date:** 2026-03-11
**Valid until:** 2026-04-11 (stable -- this is internal architecture, not external dependencies)
