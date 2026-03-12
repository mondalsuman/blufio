# Phase 56: Multi-Level Compaction & Context Budget - Context

**Gathered:** 2026-03-11
**Status:** Ready for planning

<domain>
## Phase Boundary

Long-running sessions maintain context quality through progressive summarization (L0-L3) with quality guarantees, and each context zone enforces its token budget. Replaces the current single-level compaction with a 4-level system, adds quality scoring with gates, entity extraction before compaction, cross-session archives, and per-zone token budget enforcement.

</domain>

<decisions>
## Implementation Decisions

### Compaction Level Progression
- Soft trigger (50% of dynamic zone budget) fires L0->L1 compaction (turn-pair summaries)
- Hard trigger (85% of dynamic zone budget) escalates: L1->L2 (session summary), then L2->L3 (cross-session archive)
- Cascade within same assembly call: if L1 isn't enough to get below hard trigger, L2 fires immediately
- Session-scoped through L2; L3 is cross-session (combines multiple L2 summaries per user)
- L3 archive generated automatically on session close
- Original messages deleted after compaction (audit trail records the event)
- Compaction runs inline during context assembly (blocking, current behavior)
- On compaction failure (LLM error, quality gate rejection): truncate oldest messages and continue, never block agent loop
- Replace existing single compaction_threshold (0.70) with soft_trigger (0.50) and hard_trigger (0.85) — old config key generates deprecation warning

### L1 Turn-Pair Summary Format
- Bullet-point format: each user-assistant turn pair gets a 1-2 sentence bullet summary
- Preserves granularity for re-compaction into L2
- Level-dependent max_tokens defaults: L1=256 per turn-pair, L2=1024 for session summary, L3=2048 for cross-session archive — all configurable via TOML

### Compaction Metadata
- Summaries stored with metadata: `{"compaction_level": "L1", "original_count": N, "quality_score": 0.82, "entity": 0.9, "decision": 0.8, "action": 0.75, "numerical": 0.85}`
- Enables targeted re-compaction and quality observability

### Compaction Model
- Single configurable `compaction_model` string in `[context]` TOML section — works with any provider (Anthropic, OpenAI, Ollama, OpenRouter, Gemini)
- Uses existing ProviderAdapter via model routing (no dedicated compaction provider)
- compaction_enabled toggle: `[context] compaction_enabled = true` (default) — set to false to disable all compaction

### Entity Extraction
- Runs before L1 compaction only (not before higher levels — summaries already preserve entities)
- Extracted entities stored as Memory entries via existing blufio-memory with MemorySource::Extracted
- Reuses existing memory infrastructure: searchable, subject to temporal decay/eviction

### Quality Scoring
- LLM-based evaluation via separate call (not combined with compaction call)
- Scoring call receives full original messages + generated summary for accurate evaluation
- Structured JSON output: `{"entity": 0.9, "decision": 0.8, "action": 0.75, "numerical": 0.85}`
- Weighted score: entity_retention (35%), decision_retention (25%), action_retention (25%), numerical_retention (15%) — weights configurable via TOML with these defaults
- Quality gates: >=0.6 proceed, 0.4-0.6 retry, <0.4 abort — thresholds configurable via TOML
- 1 retry on 0.4-0.6: retry prompt emphasizes the weakest dimension specifically
- If JSON parsing fails: treat as 0.5 score (retry range), log warning
- quality_scoring toggle: `[context] quality_scoring = true` (default) — set to false to skip scoring entirely
- Hardcoded evaluation prompt template; only weights and thresholds configurable
- Same compaction_model used for scoring (no separate model config)
- Quality scores persisted in compaction summary metadata
- Prometheus: blufio_compaction_quality_score histogram, blufio_compaction_gate_total{result=proceed|retry|abort} counter
- CLI: `blufio context compact --dry-run --session <id>` runs compaction without persisting, shows quality scores

### Archive & Cold Storage
- Dedicated `compaction_archives` SQLite table in main DB: id, user_id, summary, quality_score, session_ids (JSON array), classification, created_at, token_count
- Archives in main DB = automatic inclusion in existing backup/restore flow
- Rolling window: keep last N archive summaries (default 10, configurable) — oldest merged into single "deep archive" via LLM summarization with quality scoring
- Archive retrieval: automatic injection via ConditionalProvider at lowest priority (after memory, skills)
- Archive injection bounded by zone 2 (conditional) token budget
- Session IDs tracked as JSON array for GDPR erasure traceability
- Restricted messages filtered before compaction/archiving (consistent with Phase 53)
- Archives inherit highest classification of their source messages
- archive_enabled toggle: `[context] archive_enabled = true` (default) — set to false to skip L3 generation
- CLI: `blufio context archive list|view|prune` subcommands

### Zone Budget Enforcement
- Static zone (system prompt): configurable budget (default 3,000 tokens) — advisory only, warn at startup if exceeded, never truncate system prompt
- Conditional zone: configurable budget (default 8,000 tokens) with 10% safety margin — truncate by provider priority (memory > skills > archive), log which providers dropped
- Dynamic zone: adaptive budget = context_budget - actual_static_tokens - actual_conditional_tokens
- Soft/hard compaction thresholds apply to dynamic zone's adaptive budget (not total context budget)
- 10% safety margin hardcoded (not configurable) — operators adjust budgets directly
- Provider-specific token counting via TokenizerCache.get_counter(model) from Phase 47 (CTXE-03)
- Per-zone configurable: static_zone_budget, conditional_zone_budget in [context] TOML section
- AssembledContext gains `dropped_providers: Vec<String>` field for debugging
- Prometheus: blufio_context_zone_tokens{zone=static|conditional|dynamic} gauge per assembly
- CLI: `blufio context status --session <id>` shows zone usage breakdown

### EventBus Integration
- CompactionStarted{session_id, level, message_count} event emitted before compaction
- CompactionCompleted{session_id, level, quality_score, tokens_saved, duration_ms} event emitted after
- Events feed audit trail (Phase 54), lifecycle hooks (Phase 59 pre_compaction/post_compaction), and Prometheus

### Crate Organization
- Extend existing blufio-context crate (no new crate) with new modules:
  - compaction/levels.rs — L0-L3 level progression engine
  - compaction/quality.rs — quality scoring, gates, retry logic
  - compaction/archive.rs — archive storage, retrieval, rolling window, deep merge
  - compaction/extract.rs — entity/fact extraction before compaction
  - budget.rs — per-zone token budget enforcement
- blufio-context gains blufio-memory dependency (for entity extraction storage)
- blufio-context gains blufio-bus dependency (for compaction events)

### Config Schema
- Extended [context] TOML section with new fields:
  - compaction_enabled (bool, default true)
  - soft_trigger (f64, default 0.50) — replaces compaction_threshold
  - hard_trigger (f64, default 0.85)
  - compaction_model (String, existing — now provider-agnostic)
  - quality_scoring (bool, default true)
  - quality_gate_proceed (f64, default 0.6)
  - quality_gate_retry (f64, default 0.4)
  - quality_weight_entity (f64, default 0.35)
  - quality_weight_decision (f64, default 0.25)
  - quality_weight_action (f64, default 0.25)
  - quality_weight_numerical (f64, default 0.15)
  - max_tokens_l1 (u32, default 256)
  - max_tokens_l2 (u32, default 1024)
  - max_tokens_l3 (u32, default 2048)
  - static_zone_budget (u32, default 3000)
  - conditional_zone_budget (u32, default 8000)
  - archive_enabled (bool, default true)
  - max_archives (u32, default 10)
- #[serde(deny_unknown_fields)] consistent with other config sections
- Deprecation warning if old compaction_threshold key is found

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

</decisions>

<specifics>
## Specific Ideas

- Compaction model is provider-agnostic: operators can use any model from any provider (Haiku, GPT-4o-mini, local Ollama, etc.) — not limited to Anthropic models
- L1 bullet-point format enables clean re-compaction into L2 narrative
- Quality scoring is a separate LLM call for accuracy (model shouldn't grade its own work in the same call)
- Retry prompt specifically targets weakest dimension: "Pay special attention to preserving [dimension]. Previous attempt scored [X]."
- Archive conditional provider has lowest priority — current context (memories, skills) always takes precedence over historical summaries
- Cascade compaction within single assembly call prevents context overflow between turns
- Existing compaction.rs refactored into compaction/ module with levels, quality, archive, extract submodules

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `blufio-context::compaction::generate_compaction_summary()`: Refactored into multi-level engine (current single-level becomes L0->L1 path)
- `blufio-context::compaction::persist_compaction_summary()`: Extended with level metadata and quality scores
- `blufio-context::dynamic::DynamicZone`: Gains soft/hard trigger logic and cascade compaction
- `blufio-context::conditional::ConditionalProvider` trait: Archive provider implements this for context injection
- `blufio-context::static_zone::StaticZone`: Gains token counting for budget enforcement
- `blufio-core::token_counter::TokenizerCache`: Used for all budget enforcement token counting (CTXE-03)
- `blufio-core::token_counter::count_with_fallback()`: Already used in dynamic.rs for token estimation
- `blufio-memory::store::MemoryStore`: Used for entity extraction storage (MemorySource::Extracted entries)
- `blufio-bus::events::BusEvent`: Extended with Compaction(CompactionEvent) variants
- `blufio-config::model::ContextConfig`: Extended with all new configuration fields

### Established Patterns
- `#[serde(deny_unknown_fields)]` on config structs
- LazyLock for compiled regex patterns
- EventBus fire-and-forget for async event emission
- AssembledContext struct carries side-effect information back to caller
- CLI subcommands in main binary crate, library logic in crate libraries
- Prometheus metrics via EventBus subscription in blufio-prometheus
- Optional<Arc<EventBus>> for components that emit events (None in tests/CLI)

### Integration Points
- blufio-context/src/compaction.rs: Refactored into compaction/ module directory
- blufio-context/src/dynamic.rs: Soft/hard trigger logic, cascade, adaptive budget
- blufio-context/src/lib.rs: AssembledContext gains dropped_providers field, ContextEngine gains budget enforcement
- blufio-context/src/conditional.rs: Archive ConditionalProvider registered with lowest priority
- blufio-config/src/model.rs: ContextConfig extended with ~17 new fields
- blufio-storage: New compaction_archives table migration
- blufio-bus/src/events.rs: New Compaction(CompactionEvent) variant
- blufio (binary): CLI subcommands for context compact/archive/status
- blufio-prometheus: New compaction and zone budget metric subscribers
- serve.rs: Archive ConditionalProvider registration during context engine init

</code_context>

<deferred>
## Deferred Ideas

None -- discussion stayed within phase scope

</deferred>

---

*Phase: 56-multi-level-compaction-context-budget*
*Context gathered: 2026-03-11*
