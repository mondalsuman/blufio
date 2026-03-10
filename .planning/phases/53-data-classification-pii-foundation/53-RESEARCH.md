# Phase 53: Data Classification & PII Foundation - Research

**Researched:** 2026-03-10
**Domain:** Data classification (enum/trait/enforcement), PII detection (regex), database migration, CLI/API integration
**Confidence:** HIGH

## Summary

Phase 53 introduces a data classification system (Public/Internal/Confidential/Restricted) and PII detection engine into the existing Blufio codebase. The implementation extends well-understood patterns already established in the project: the `REDACTION_PATTERNS` LazyLock in `redact.rs`, the `MemorySource`/`MemoryStatus` enum serialization pattern, the `BusEvent` domain-based event hierarchy, and the `BlufioError` classified error system.

The technical surface is broad (touching 10+ crates) but each individual change is well-scoped. The classification enum, trait, and guard are pure Rust logic with no external dependencies. PII detection uses the `regex` crate (already a workspace dependency at v1) with RegexSet for the fast path and individual Regex objects for detail extraction. The database migration adds a TEXT column with DEFAULT to three tables. The CLI/API layer follows established patterns (clap subcommands in the binary crate, axum handlers with scope-based auth in blufio-gateway).

**Primary recommendation:** Implement in layers: (1) core types in blufio-core, (2) PII detection in blufio-security, (3) database migration, (4) enforcement guard, (5) agent/context integration, (6) CLI/API, (7) privacy report enhancements, (8) event bus + metrics.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
- Column per table: `classification TEXT NOT NULL DEFAULT 'internal'` on memories, messages, sessions tables
- DataClassification enum in blufio-core with ascending sensitivity ordering (Public < Internal < Confidential < Restricted) -- derive PartialOrd/Ord
- Classifiable trait in blufio-core: simple getter/setter (`fn classification(&self)` + `fn set_classification(&mut self, level)`)
- Default classification for existing and new data: Internal
- Allow reclassification in both directions (with audit trail in Phase 54)
- No propagation across relationships -- each entity independently classified
- Classification is queryable and filterable via SQL WHERE
- Batch update API supports filter by session ID, date range, current level, and content pattern
- SQL index on classification column for memories and messages tables
- Serialized as lowercase strings: 'public', 'internal', 'confidential', 'restricted' (matches MemorySource/MemoryStatus pattern)
- Rust-side enum validation only (no SQL CHECK constraint -- SQLite ALTER TABLE limitation)
- Runtime data only -- memories, messages, sessions, exports. Config values protected by vault separately.
- Both blufio-storage (messages, sessions) and blufio-memory (memories) get the classification column
- Single migration file adds column to all three tables plus creates indexes
- Extend existing REDACTION_PATTERNS in blufio-security/redact.rs -- same pipeline for secrets and PII
- PII detection returns Vec<PiiMatch> with { pii_type: PiiType, span: Range<usize>, matched_value: String }
- PiiType enum is #[non_exhaustive]: Email, Phone, Ssn, CreditCard (allows future additions)
- Type-specific redaction placeholders: [EMAIL], [PHONE], [SSN], [CREDIT_CARD] -- existing secrets stay [REDACTED]
- Phone numbers: common US/UK/EU formats with ~3-4 regex patterns
- SSN: US format only (XXX-XX-XXXX with area number validation)
- Credit card: Luhn algorithm validation after regex match to reduce false positives
- Context-aware skipping: pre-strip fenced code blocks, inline code, and URLs before detection
- RegexSet for fast two-phase detection: check if any pattern matches, then run individual regexes for details
- PII detection at write time (synchronous, before INSERT) -- auto-classify as Confidential when PII found
- All text content scanned: user messages, assistant responses, tool arguments, tool results, memory content
- Built-in patterns only (no TOML-configurable custom patterns for now)
- No caching -- regex is fast enough (microseconds)
- No content length cutoff -- always scan regardless of size
- Log PII detection at info level: "PII detected: {count} match(es) [{types}] -- auto-classified as Confidential"
- CLI command: `blufio pii scan <text>` / `--file <path>` / stdin pipe support
- Central ClassificationGuard in blufio-security: stateless with static rules, global singleton (LazyLock)
- Methods: can_export(level), can_include_in_context(level), must_redact_in_logs(level)
- Restricted: silent skip from LLM context (no placeholder, no error), excluded from memory retrieval at SQL level, excluded from exports with warning count, never in any LLM context including tool results
- Confidential: PII redacted within content (not entire field) in logs, SQLCipher encryption satisfies "encrypted at rest" requirement
- Internal: audit-logged only (Phase 54), no export/context restrictions
- Public: no restrictions
- Enforcement active immediately (not deferred)
- Filter tool results from WASM skills and MCP tools before LLM sees them
- Warn but allow when non-SQLCipher (plaintext) database stores Confidential data
- Exports exclude Restricted data with warning: "N items excluded due to classification restrictions"
- Prometheus metrics: `blufio_classification_blocked_total{level,action}` for enforcement actions
- Context engine: dynamic zone filtering only (static zone is system prompt, conditional zone already filtered at SQL)
- CLI: `blufio classify set|get|list|bulk` subcommands
- API: PUT/GET /v1/classify/{type}/{id}, POST /v1/classify/bulk endpoints in blufio-gateway classify.rs module
- Confirm downgrades: require --force flag for Confidential->Public etc.
- New 'classify' scope for scoped API keys
- Auto-inference from PII: opt-out (enabled by default, disable via `[classification] auto_classify_pii = false`)
- Bulk operations: --dry-run mode, filter by session_id/date range/current level/content pattern
- Partial success for bulk: return { total, succeeded, failed, errors }
- --json flag on classify list (matches existing CLI output patterns)
- ALTER TABLE + DEFAULT migration: no backfill PII scan on existing data
- Single migration file for all three tables
- New event variants in blufio-bus: ClassificationChanged, PiiDetected, ClassificationEnforced, BulkClassificationChanged
- Events fire only when PII actually found (not on every scan)
- Events carry metadata only (never actual PII values)
- Single bulk event (not per-item) for bulk operations
- blufio-security gains blufio-bus dependency for event emission
- New top-level `[classification]` TOML section with fields: enabled, auto_classify_pii, default_level, warn_unencrypted
- New BlufioError::Classification(ClassificationError) variant with sub-variants
- PII detection failures: log and continue, never block agent loop
- Comprehensive PII test vectors: 10+ per type, boundary cases, false positives, international formats (~50+ tests)
- Full pipeline integration tests: store -> classify -> retrieve -> verify exclusion
- Property-based tests (proptest) for Luhn validation
- Criterion benchmark for PII detection throughput
- Target: <1ms per message for PII detection

### Claude's Discretion
- Exact regex patterns for email, phone, SSN, credit card
- Internal module structure within pii.rs (sub-modules vs flat)
- Exact Prometheus metric names and label values
- Test fixture organization
- Migration version numbering

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| DCLS-01 | DataClassification enum with 4 levels: Public, Internal, Confidential, Restricted | Core types in blufio-core; PartialOrd/Ord derive; as_str()/from_str_value() following MemorySource pattern |
| DCLS-02 | Classifiable trait allows tagging memories, messages, exports, and config values | Trait in blufio-core; field added to Memory, Message, Session structs; config values deferred to vault |
| DCLS-03 | Per-level controls matrix: Restricted = never exported + never in LLM context; Confidential = encrypted at rest + redacted in logs; Internal = audit-logged; Public = no restrictions | ClassificationGuard singleton in blufio-security with static rules; SQL WHERE filter for restricted; DynamicZone filtering for context |
| DCLS-04 | Classification can be set explicitly via API/CLI or inferred from PII detection | CLI subcommands in binary crate; API endpoints in blufio-gateway/classify.rs; auto-inference via PII detection at write time |
| DCLS-05 | Classification changes logged in audit trail | EventBus events (ClassificationChanged, BulkClassificationChanged); actual audit trail deferred to Phase 54 |
| PII-01 | Regex-based PII detection covers email, phone (international), SSN, credit card (Luhn-validated) | New pii.rs module in blufio-security; RegexSet fast path; Luhn validation post-match for credit cards |
| PII-02 | PII detection integrates with existing RedactingWriter for log output | Extend REDACTION_PATTERNS with PII patterns; type-specific placeholders [EMAIL], [PHONE], [SSN], [CREDIT_CARD] |
| PII-03 | PII detection applies to data exports with configurable redaction | ClassificationGuard.can_export() + PII redaction when exporting; Restricted excluded entirely |
| PII-04 | PII-containing content auto-classifies as Confidential when data classification is active | PII detection at write time triggers auto-classification; configurable via auto_classify_pii setting |
| PII-05 | Context-aware redaction skips PII patterns inside code blocks and URLs | Pre-strip fenced code blocks, inline code, and URLs before running PII regex; new string allocation with stripped zones |

</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| regex | 1.x | PII pattern matching, RegexSet fast path | Already workspace dependency; O(m*n) guarantees; single-pass multi-pattern matching via RegexSet |
| serde | 1.x | Serialization of classification types | Already workspace dependency; derive macros for Serialize/Deserialize |
| strum | 0.26 | Display/EnumString derives for DataClassification | Already workspace dependency; used by existing enums like AdapterType |
| thiserror | 2.x | Error derive for ClassificationError | Already workspace dependency; used by BlufioError |
| clap | 4.5 | CLI subcommand definitions | Already workspace dependency; derive-based CLI framework |
| axum | latest | API endpoint handlers | Already used by blufio-gateway for REST API |
| metrics | latest | Prometheus counter recording | Already used via metrics-rs facade by blufio-prometheus |
| proptest | 1.x | Property-based testing for Luhn validation | Already workspace dev-dependency |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| rusqlite | 0.37 | Direct SQL for migration ALTER TABLE | Already workspace dependency; used for all database operations |
| tokio-rusqlite | 0.7 | Async wrapper for classification queries | Already workspace dependency; single-writer pattern for DB access |
| refinery | 0.9 | Migration framework for schema versioning | Already workspace dependency; used for all migrations via embed_migrations! |
| uuid | 1.x | Event IDs for BusEvent variants | Already workspace dependency; used by new_event_id() in blufio-bus |
| chrono | latest | Timestamps for events and filtering | Already workspace dependency; used by now_timestamp() in blufio-bus |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled Luhn | luhn crate | Extra dependency for ~15 lines of code; hand-roll since proptest verifies correctness |
| PII regex patterns | presidio/comprehend | ML-based NER is out of scope (v1.6+); regex covers 95%+ of structured PII |
| field-level encryption | Custom AES per field | SQLCipher already encrypts at rest; additional field-level adds complexity with no compliance benefit |

**Installation:**
No new dependencies needed. All libraries are already in the workspace.

## Architecture Patterns

### Recommended Project Structure
```
crates/blufio-core/src/
  types.rs                    # Add classification field to Message, Session
  error.rs                    # Add Classification(ClassificationError) variant
  classification.rs           # NEW: DataClassification enum + Classifiable trait

crates/blufio-security/src/
  lib.rs                      # Add pub mod pii; pub mod classification_guard;
  redact.rs                   # Extend REDACTION_PATTERNS with PII patterns
  pii.rs                      # NEW: PII detection engine (PiiType, PiiMatch, detect_pii())
  classification_guard.rs     # NEW: ClassificationGuard singleton

crates/blufio-memory/src/
  types.rs                    # Add classification field to Memory struct
  store.rs                    # Add classification to INSERT/SELECT, WHERE filter for restricted

crates/blufio-storage/migrations/
  V12__data_classification.sql # NEW: ALTER TABLE + indexes for all three tables

crates/blufio-config/src/
  model.rs                    # Add ClassificationConfig struct + classification field to BlufioConfig

crates/blufio-bus/src/
  events.rs                   # Add Classification(ClassificationEvent) variant to BusEvent

crates/blufio-gateway/src/
  classify.rs                 # NEW: REST API endpoints for classification

crates/blufio-prometheus/src/
  recording.rs                # Add classification metric registration + recording

crates/blufio-agent/src/
  session.rs                  # PII detection before insert_message calls

crates/blufio-context/src/
  dynamic.rs                  # Filter restricted content from assembled messages

crates/blufio/src/
  main.rs                     # Add Classify and Pii subcommands
  classify.rs                 # NEW: CLI handler for blufio classify
  pii_cmd.rs                  # NEW: CLI handler for blufio pii scan
  privacy.rs                  # Add classification_distribution to PrivacyReport
```

### Pattern 1: DataClassification Enum (following MemorySource/MemoryStatus pattern)
**What:** Enum with as_str()/from_str_value() for SQLite TEXT serialization, plus PartialOrd/Ord for sensitivity comparison
**When to use:** Everywhere classification levels are compared or stored
**Example:**
```rust
// Source: project patterns from blufio-memory/src/types.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DataClassification {
    Public,
    Internal,
    Confidential,
    Restricted,
}

impl DataClassification {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Confidential => "confidential",
            Self::Restricted => "restricted",
        }
    }

    pub fn from_str_value(s: &str) -> Option<Self> {
        match s {
            "public" => Some(Self::Public),
            "internal" => Some(Self::Internal),
            "confidential" => Some(Self::Confidential),
            "restricted" => Some(Self::Restricted),
            _ => None,
        }
    }

    /// Returns true if `self` is a downgrade from `current`.
    pub fn is_downgrade_from(&self, current: &Self) -> bool {
        *self < *current
    }
}
```

### Pattern 2: RegexSet Two-Phase PII Detection
**What:** Use RegexSet.is_match() for fast negative path, then individual Regex objects for detail extraction when a match is found
**When to use:** Every content scan -- the common case (no PII) is extremely fast
**Example:**
```rust
// Source: docs.rs/regex/latest/regex/struct.RegexSet.html + project redact.rs patterns
use std::ops::Range;
use std::sync::LazyLock;
use regex::{Regex, RegexSet};

static PII_REGEX_SET: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new(&[
        r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}",  // email
        r"\b\d{3}-\d{2}-\d{4}\b",                              // SSN
        r"\b(?:\d[ -]*?){13,19}\b",                             // credit card candidate
        // ... phone patterns
    ]).expect("PII regex patterns must compile")
});

static PII_INDIVIDUAL_REGEXES: LazyLock<Vec<(PiiType, Regex)>> = LazyLock::new(|| {
    // Individual patterns for detail extraction
    vec![
        (PiiType::Email, Regex::new(r"...").unwrap()),
        (PiiType::Ssn, Regex::new(r"...").unwrap()),
        // ...
    ]
});

pub fn detect_pii(text: &str) -> Vec<PiiMatch> {
    // Phase 1: fast check -- is there any PII at all?
    let stripped = strip_code_and_urls(text);
    if !PII_REGEX_SET.is_match(&stripped) {
        return vec![];
    }

    // Phase 2: extract details from individual patterns
    let mut matches = Vec::new();
    for (pii_type, regex) in PII_INDIVIDUAL_REGEXES.iter() {
        for m in regex.find_iter(&stripped) {
            // For credit cards: validate with Luhn before accepting
            if *pii_type == PiiType::CreditCard && !luhn_validate(m.as_str()) {
                continue;
            }
            matches.push(PiiMatch {
                pii_type: *pii_type,
                span: m.start()..m.end(),
                matched_value: m.as_str().to_string(),
            });
        }
    }
    matches
}
```

### Pattern 3: ClassificationGuard Static Singleton
**What:** LazyLock singleton with pure functions for enforcement decisions -- no config, no state, deterministic
**When to use:** Every enforcement check point (export, context assembly, log redaction)
**Example:**
```rust
// Source: project patterns -- similar to REDACTION_PATTERNS LazyLock
use std::sync::LazyLock;

pub struct ClassificationGuard;

static GUARD: LazyLock<ClassificationGuard> = LazyLock::new(|| ClassificationGuard);

impl ClassificationGuard {
    pub fn instance() -> &'static Self {
        &GUARD
    }

    pub fn can_export(&self, level: DataClassification) -> bool {
        level < DataClassification::Restricted
    }

    pub fn can_include_in_context(&self, level: DataClassification) -> bool {
        level < DataClassification::Restricted
    }

    pub fn must_redact_in_logs(&self, level: DataClassification) -> bool {
        level >= DataClassification::Confidential
    }
}
```

### Pattern 4: BusEvent Domain Sub-Enum (following existing pattern)
**What:** New Classification domain variant on BusEvent with sub-enum ClassificationEvent
**When to use:** Events for classification changes, PII detection, enforcement actions
**Example:**
```rust
// Source: blufio-bus/src/events.rs existing pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClassificationEvent {
    Changed {
        event_id: String,
        timestamp: String,
        entity_type: String,      // "memory", "message", "session"
        entity_id: String,
        old_level: String,
        new_level: String,
        changed_by: String,       // "user", "auto_pii", "bulk"
    },
    PiiDetected {
        event_id: String,
        timestamp: String,
        entity_type: String,
        entity_id: String,
        pii_types: Vec<String>,   // ["email", "phone"]
        count: usize,
    },
    Enforced {
        event_id: String,
        timestamp: String,
        entity_type: String,
        entity_id: String,
        level: String,
        action_blocked: String,   // "export", "context_include"
    },
    BulkChanged {
        event_id: String,
        timestamp: String,
        entity_type: String,
        count: usize,
        old_level: String,
        new_level: String,
        changed_by: String,
    },
}

// Add to BusEvent:
// Classification(ClassificationEvent),
```

### Pattern 5: Error Hierarchy (following existing BlufioError pattern)
**What:** New Classification variant with sub-enum for specific error kinds
**When to use:** All classification and PII operations that can fail
**Example:**
```rust
// Source: blufio-core/src/error.rs existing patterns
#[derive(Debug, Clone, Display)]
#[non_exhaustive]
pub enum ClassificationError {
    InvalidLevel(String),
    DowngradeRejected { current: String, requested: String },
    EntityNotFound { entity_type: String, entity_id: String },
    BulkOperationFailed { total: usize, failed: usize },
}

// Add to BlufioError:
// #[error("classification: {0}")]
// Classification(ClassificationError),
```

### Anti-Patterns to Avoid
- **Scanning PII after storage:** PII must be detected BEFORE insert to auto-classify correctly. Scanning after means the classification field is wrong until a follow-up update.
- **Blocking on PII detection errors:** PII detection must never block the agent loop. Log the error and continue -- false negatives are acceptable, message drops are not.
- **Field-level encryption for Confidential:** SQLCipher already encrypts the entire database. Adding AES per field doubles complexity with no compliance benefit.
- **SQL CHECK constraints:** SQLite does not support ADD COLUMN with CHECK on ALTER TABLE. Validate in Rust only.
- **Propagating classification across relationships:** Each entity is independently classified. A message in a session does not inherit the session's classification.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Regex compilation | Custom parser | regex crate LazyLock | Linear-time guarantees, SIMD optimization, battle-tested |
| Multi-pattern matching | Loop over individual patterns for every scan | RegexSet | Single-pass matching; massive perf win when most scans find nothing |
| Migration versioning | Custom schema tracker | refinery embed_migrations! | Already used; tracks applied migrations in refinery_schema_history |
| CLI argument parsing | Manual argparse | clap derive macros | Already used; consistent with existing subcommand patterns |
| API auth scoping | Custom middleware | require_scope() helper | Already exists in blufio-gateway/api_keys; scope-based is established |
| Event ID generation | Custom UUID | new_event_id() from blufio-bus | Already exists; UUID v4 generation helper |

**Key insight:** Every infrastructure component this phase needs already exists in the codebase. The implementation is about extending existing patterns, not introducing new paradigms.

## Common Pitfalls

### Pitfall 1: RegexSet Pattern Index Mismatch
**What goes wrong:** RegexSet.matches() returns indices into the pattern list. If the order of patterns in RegexSet differs from the individual Regex vector, you match the wrong PiiType.
**Why it happens:** Maintaining two parallel lists of patterns (RegexSet and Vec<Regex>) that must stay synchronized.
**How to avoid:** Define patterns in a single source-of-truth struct array. Build both RegexSet and Vec<(PiiType, Regex)> from the same array.
**Warning signs:** SSN detected where email exists, wrong placeholder used.

### Pitfall 2: Context-Aware Stripping Changes Span Offsets
**What goes wrong:** When you strip code blocks and URLs to prevent false positives, the spans from regex matches on the stripped text don't correspond to positions in the original text.
**Why it happens:** Removing characters shifts all subsequent offsets.
**How to avoid:** For the PiiMatch return type, spans reference the stripped text. When redacting in the original text, either: (a) replace code blocks/URLs with same-length placeholders to preserve offsets, or (b) re-run individual patterns on the original text with exclusion zone logic. The CONTEXT.md specifies "new string allocation with stripped zones" -- approach (a) is simpler and correct.
**Warning signs:** Garbled output when redacting PII from original text.

### Pitfall 3: ALTER TABLE DEFAULT Value Not Retroactive in SQLite
**What goes wrong:** `ALTER TABLE t ADD COLUMN c TEXT NOT NULL DEFAULT 'internal'` in SQLite does set the default for existing rows, but only at read time for storage format 4. Older SQLite versions may not populate existing rows.
**Why it happens:** SQLite optimization -- the DEFAULT is stored in schema, not written to every row.
**How to avoid:** The bundled SQLite (via rusqlite bundled-sqlcipher) uses a modern format that handles this correctly. Verify by querying existing rows after migration. The default is applied at read time, so no backfill is needed.
**Warning signs:** NULL classification values on existing rows (would require explicit UPDATE).

### Pitfall 4: Luhn False Positives on Numeric Strings
**What goes wrong:** Sequences of 13-19 digits that pass Luhn but aren't credit card numbers (timestamps, IDs, large integers).
**Why it happens:** Luhn is a checksum, not a format validator. Many numeric sequences happen to pass.
**How to avoid:** Combine with prefix validation: credit card numbers start with specific digits (4 for Visa, 5 for Mastercard, 3 for Amex, 6 for Discover). Reject numbers that don't match known BIN ranges. Also, the context-aware stripping removes many numeric sequences in code/URLs.
**Warning signs:** High false positive rate in benchmarks.

### Pitfall 5: Circular Dependency Between blufio-security and blufio-bus
**What goes wrong:** blufio-security needs blufio-bus to emit events, but blufio-bus might depend on blufio-core types that blufio-security also uses.
**Why it happens:** Adding event emission to a low-level crate.
**How to avoid:** Check the dependency graph. blufio-bus currently depends only on serde, uuid, chrono -- no dependency on blufio-security. Adding blufio-bus as a dependency of blufio-security creates no cycle. Verify: blufio-core <- blufio-security, blufio-bus is independent.
**Warning signs:** Cargo circular dependency error during compilation.

### Pitfall 6: Bulk Classification SQL Injection via Content Pattern Filter
**What goes wrong:** The bulk update supports filtering by content pattern (LIKE). If the pattern is user-supplied without sanitization, it could be used for SQL injection.
**Why it happens:** Building dynamic SQL with user input.
**How to avoid:** Use parameterized queries for all user-supplied filter values. The LIKE pattern should be passed as a parameter (`WHERE content LIKE ?`), never interpolated into the SQL string. rusqlite's params! macro enforces this.
**Warning signs:** Arbitrary SQL execution via bulk classify.

### Pitfall 7: PII Detection Blocking Agent Loop
**What goes wrong:** If PII detection panics or takes unexpectedly long on adversarial input, the agent loop stalls.
**Why it happens:** Regex backtracking (though Rust regex crate guarantees linear time), or unexpected error propagation.
**How to avoid:** The Rust regex crate guarantees O(m*n) with no backtracking, so catastrophic regex is not possible. For error handling: wrap detect_pii calls in a catch-all that logs and continues. Never propagate PII detection errors as blocking errors.
**Warning signs:** Session timeouts correlated with PII-heavy messages.

## Code Examples

### Migration SQL (V12__data_classification.sql)
```sql
-- Source: project pattern from V7__api_keys_webhooks_batch.sql and CONTEXT.md decisions
-- Add classification column to all three content tables.
-- Default 'internal' for all existing and new rows.

ALTER TABLE memories ADD COLUMN classification TEXT NOT NULL DEFAULT 'internal';
ALTER TABLE messages ADD COLUMN classification TEXT NOT NULL DEFAULT 'internal';
ALTER TABLE sessions ADD COLUMN classification TEXT NOT NULL DEFAULT 'internal';

-- Indexes for classification-based queries.
CREATE INDEX IF NOT EXISTS idx_memories_classification ON memories(classification);
CREATE INDEX IF NOT EXISTS idx_messages_classification ON messages(classification);
```

### Context-Aware Stripping for PII Detection
```rust
/// Replace code blocks, inline code, and URLs with equal-length whitespace
/// to preserve span offsets while preventing false positive PII matches.
fn strip_code_and_urls(text: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    static FENCED_CODE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?s)```[^`]*```").unwrap()
    });
    static INLINE_CODE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"`[^`]+`").unwrap()
    });
    static URL: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"https?://[^\s)>\]]+").unwrap()
    });

    let mut result = text.to_string();
    for re in [&*FENCED_CODE, &*INLINE_CODE, &*URL] {
        result = re.replace_all(&result, |caps: &regex::Captures| {
            " ".repeat(caps[0].len())
        }).to_string();
    }
    result
}
```

### Luhn Validation
```rust
/// Validate a credit card number candidate using the Luhn algorithm.
/// Input should be digit-only (strip spaces/dashes first).
fn luhn_validate(number: &str) -> bool {
    let digits: Vec<u32> = number
        .chars()
        .filter(|c| c.is_ascii_digit())
        .map(|c| c.to_digit(10).unwrap())
        .collect();

    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }

    let checksum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let doubled = d * 2;
                if doubled > 9 { doubled - 9 } else { doubled }
            } else {
                d
            }
        })
        .sum();

    checksum % 10 == 0
}
```

### ClassificationConfig for TOML
```rust
// Source: project pattern from blufio-config/src/model.rs
use blufio_core::classification::DataClassification;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClassificationConfig {
    /// Enable data classification system.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Auto-classify PII-containing content as Confidential.
    #[serde(default = "default_true")]
    pub auto_classify_pii: bool,

    /// Default classification level for new data.
    #[serde(default)]
    pub default_level: DataClassification, // defaults to Internal via Default impl

    /// Warn when non-SQLCipher database stores Confidential data.
    #[serde(default = "default_true")]
    pub warn_unencrypted: bool,
}

fn default_true() -> bool { true }

impl Default for ClassificationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_classify_pii: true,
            default_level: DataClassification::Internal,
            warn_unencrypted: true,
        }
    }
}
```

### API Endpoint Handler Pattern
```rust
// Source: project pattern from blufio-gateway/src/webhooks/handlers.rs
use axum::{extract::{Path, State, Json}, http::StatusCode};
use crate::api_keys::{AuthContext, require_scope};
use crate::server::GatewayState;

pub async fn put_classification(
    State(state): State<GatewayState>,
    axum::Extension(auth_ctx): axum::Extension<AuthContext>,
    Path((entity_type, entity_id)): Path<(String, String)>,
    Json(req): Json<SetClassificationRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    require_scope(&auth_ctx, "classify")?;

    // Validate level
    let level = DataClassification::from_str_value(&req.level)
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Check for downgrade
    let current = get_current_classification(&state, &entity_type, &entity_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    if level.is_downgrade_from(&current) && !req.force {
        return Err(StatusCode::CONFLICT); // 409
    }

    // Apply classification...
    Ok(Json(serde_json::json!({ "status": "ok" })))
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| No data classification | Enum-based classification per entity | Phase 53 | Every piece of data has a sensitivity level |
| Secrets-only redaction | PII + secrets redaction in single pipeline | Phase 53 | Broader protection without separate systems |
| No export controls | Classification-gated exports | Phase 53 | Restricted data never leaves the system |
| No context filtering | SQL-level + dynamic zone classification filtering | Phase 53 | Restricted data never reaches LLM |

**Deprecated/outdated:**
- None -- this is a new capability being added

## Open Questions

1. **Migration version number**
   - What we know: Last migration is V11__bench_results.sql
   - What's unclear: Whether other phases between now and execution will add V12
   - Recommendation: Use V12 for now; adjust at implementation time if a collision occurs

2. **Export mechanism location**
   - What we know: No explicit export module exists yet in the codebase
   - What's unclear: Where exactly "exports" happen -- there's no blufio-export crate
   - Recommendation: The export restriction enforcement should be placed in ClassificationGuard as a gatekeeping function. When GDPR export (Phase 60) is built, it calls can_export() before including data. For now, enforce at the CLI level for any data dump operations.

3. **Session classification semantics**
   - What we know: Sessions get a classification column
   - What's unclear: Whether session classification affects messages within it (answer: no, per CONTEXT.md -- no propagation)
   - Recommendation: Session classification is independent. A Restricted session simply means the session metadata itself is restricted, not that all messages in it are.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) + proptest 1.x |
| Config file | Cargo.toml [dev-dependencies] per crate |
| Quick run command | `cargo test -p blufio-security --lib` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DCLS-01 | DataClassification enum with 4 levels, ordering, serialization | unit | `cargo test -p blufio-core -- classification` | Wave 0 |
| DCLS-02 | Classifiable trait on Memory, Message, Session | unit | `cargo test -p blufio-core -- classifiable` | Wave 0 |
| DCLS-03 | Per-level controls matrix enforcement | unit | `cargo test -p blufio-security -- classification_guard` | Wave 0 |
| DCLS-04 | Set classification via API/CLI + PII inference | integration | `cargo test -p blufio-gateway -- classify` | Wave 0 |
| DCLS-05 | Classification changes emit events | unit | `cargo test -p blufio-security -- classification_event` | Wave 0 |
| PII-01 | PII detection covers email, phone, SSN, credit card | unit | `cargo test -p blufio-security -- pii` | Wave 0 |
| PII-02 | PII integrates with RedactingWriter | unit | `cargo test -p blufio-security -- redact::pii` | Wave 0 |
| PII-03 | PII redaction in exports | integration | `cargo test -p blufio-security -- pii::export` | Wave 0 |
| PII-04 | PII auto-classifies as Confidential | integration | `cargo test -p blufio-security -- pii::auto_classify` | Wave 0 |
| PII-05 | Context-aware skipping (code blocks, URLs) | unit | `cargo test -p blufio-security -- pii::context_aware` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-security -p blufio-core --lib`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before /gsd:verify-work

### Wave 0 Gaps
- [ ] `crates/blufio-core/src/classification.rs` -- new module with DataClassification enum + Classifiable trait + tests
- [ ] `crates/blufio-security/src/pii.rs` -- new module with PII detection engine + comprehensive tests
- [ ] `crates/blufio-security/src/classification_guard.rs` -- new module with ClassificationGuard + tests
- [ ] proptest dependency in blufio-security dev-dependencies for Luhn property tests
- [ ] Test fixtures for PII patterns (email, phone, SSN, credit card -- true/false positive vectors)

## Sources

### Primary (HIGH confidence)
- Project codebase -- blufio-security/src/redact.rs (existing REDACTION_PATTERNS and RedactingWriter)
- Project codebase -- blufio-memory/src/types.rs (MemorySource/MemoryStatus serialization pattern)
- Project codebase -- blufio-bus/src/events.rs (BusEvent domain sub-enum pattern)
- Project codebase -- blufio-core/src/error.rs (BlufioError classified error hierarchy)
- Project codebase -- blufio-config/src/model.rs (BlufioConfig with deny_unknown_fields)
- Project codebase -- blufio-gateway/src/api_keys/mod.rs (require_scope auth pattern)
- [RegexSet docs](https://docs.rs/regex/latest/regex/struct.RegexSet.html) -- single-pass multi-pattern matching, cannot extract captures
- [regex crate](https://docs.rs/regex/latest/regex/) -- O(m*n) linear time guarantee, no backtracking

### Secondary (MEDIUM confidence)
- [Luhn algorithm implementations in Rust](https://google.github.io/comprehensive-rust/testing/exercise.html) -- standard Luhn validation logic
- [Rust regex performance discussion](https://github.com/rust-lang/regex/discussions/960) -- RegexSet performance characteristics

### Tertiary (LOW confidence)
- None -- all findings verified against project code and official docs

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in workspace, patterns well-established
- Architecture: HIGH -- extends existing patterns (MemorySource enum, REDACTION_PATTERNS, BusEvent, BlufioError), no new paradigms
- Pitfalls: HIGH -- based on direct code inspection of existing patterns and Rust regex guarantees
- PII regex accuracy: MEDIUM -- exact patterns are Claude's discretion per CONTEXT.md; the architecture is sound but false positive rates need empirical tuning during implementation

**Research date:** 2026-03-10
**Valid until:** 2026-04-10 (stable -- no fast-moving dependencies)
