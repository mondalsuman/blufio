# Phase 66: Injection Defense Hardening - Research

**Researched:** 2026-03-13
**Domain:** Prompt injection defense -- Unicode normalization, encoding detection, multi-language patterns, canary tokens, severity weighting, false positive validation
**Confidence:** HIGH

## Summary

Phase 66 hardens the existing L1 injection classifier in `blufio-injection` by adding a normalization pre-pass (NFKC + zero-width stripping + confusable mapping), base64 payload detection and decoding, 5 new pattern categories (PromptLeaking, Jailbreak, DelimiterManipulation, IndirectInjection, EncodingEvasion), multi-language attack patterns (FR/DE/ES/ZH/JA), configurable per-category severity weights via TOML, canary token leak detection on LLM output, and a validated benign/attack corpus with 0% false positive tolerance as a hard CI gate.

The existing codebase provides a well-structured foundation: `patterns.rs` has a single-source-of-truth `PATTERNS` array with `LazyLock<RegexSet>` compilation, `classifier.rs` has a clean two-phase detect-then-extract flow, and the pipeline coordinator in `pipeline.rs` orchestrates L1/L4/L5 with correlation IDs. The phase adds a `normalize.rs` module in the pre-pass, extends `InjectionCategory` from 3 to 8 variants, adds a `language` field to `InjectionPattern`, introduces severity weight multiplication in score calculation, integrates canary detection into the output screening path, and validates everything against paired corpora.

**Primary recommendation:** Implement as a layered expansion of existing patterns -- new `normalize.rs` module for the pre-pass pipeline, extend `InjectionPattern` and `InjectionCategory` structs, add `severity_weights: HashMap<String, f64>` to `InputDetectionConfig`, create `canary.rs` for token generation/detection, and use JSON fixture files for corpus validation tests.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Two canary tokens: one global (generated at server startup, changes on restart) and one per-session (random UUID4 per session)
- Both placed at end of system prompt as last line: "CONFIDENTIAL_TOKEN: {uuid}"
- Always auto-generated -- not configurable by operator
- Enabled by default (consistent with L1/L3/L4 defaults from Phase 57)
- Detection: exact substring match of full UUID in LLM output. No partial/fuzzy/obfuscated matching
- Scans complete LLM response (not streamed chunks) -- buffers full response before checking
- Applies to all LLM output: text responses AND tool call arguments
- Action on detection: block response entirely + log SecurityEvent + emit EventBus alert
- Blocked response replaced with generic refusal ("I can't process this response.") -- never reveal detection reason
- Separate Prometheus metric: injection_canary_detections_total
- CLI: `blufio injection test-canary` subcommand for canary self-test
- Doctor check: canary self-test (generate, simulate echo, verify detection)
- Category-level weights (not per-pattern) -- operators set a multiplier per category
- TOML format: `[injection_defense.input_detection.severity_weights]` with one key per category
- All 8 categories get their own weight key
- Default weight: 1.0 (neutral), max cap: 3.0, weight=0.0 disables category
- Invalid weights (negative, NaN): warn + use default 1.0
- Restart required for weight changes (no hot reload)
- 0% false positive tolerance -- no legitimate message should trigger detection (score > 0)
- Paired corpora: benign corpus (must NOT trigger) + attack corpus (MUST trigger)
- Both are hard CI gates
- Corpus format: JSON arrays in crates/blufio-injection/tests/fixtures/
- Benign corpus (100+ messages): broad mix including multi-language text (10-20 per supported language)
- Attack corpus (50+ messages): ~2 variants per pattern, plus Unicode-evaded and base64-encoded variants
- CLI: `blufio injection validate-corpus <path>` for operator custom benign validation
- Key attack phrases only (3-5 patterns per language) -- not full translation of all English patterns
- Languages: French, German, Spanish, Chinese, Japanese
- InjectionPattern struct gets new `language: &'static str` field
- Chinese/Japanese patterns use literal character strings (not Unicode property classes)
- Pipeline order: Normalize -> Decode -> Extract -> Scan (sequential)
- Always normalize every message (NFKC + zero-width strip + confusable mapping)
- Base64 scanning on every message -- heuristic detection of 20+ char segments
- Classifier scans BOTH original AND normalized text, merges all matches (union)
- Zero-width character presence adds 0.1 evasion signal bonus
- Confusable character detection adds same 0.1 evasion signal bonus
- Confusable mapping: common Latin/Cyrillic/Greek lookalikes only (~50-100 character mappings)
- Confusable mapping table: compile-time constant
- NormalizationReport produced with zero_width_count, confusables_mapped, base64_segments_decoded
- Separate module: normalize.rs in blufio-injection
- HTML/markdown extraction: simple regex (not full HTML parser)
- 5 new InjectionCategory variants (total 8): PromptLeaking, Jailbreak, DelimiterManipulation, IndirectInjection, EncodingEvasion
- Exhaustive enum -- no Other variant
- Full recursive JSON scanning for indirect injection
- Size limit: scan first 50KB of tool output (truncate with warning if exceeded)
- HTML comment scanning: tool outputs only (not user messages)
- Reuse existing MCP server `trusted = true` flag to skip indirect injection scanning
- Extracted content scanned per-segment (not concatenated)
- 20ms timeout for full L1 pipeline
- Timeout action: allow message + log warning
- Keep LazyLock pattern for RegexSet compilation

### Claude's Discretion
- Exact regex patterns for each new category
- NFKC implementation choice (unicode-normalization crate or std)
- Base64 heuristic regex for segment detection
- HTML comment / markdown fence extraction regex
- Confusable mapping table contents (specific character pairs)
- NormalizationReport struct field names
- Canary token system prompt format string
- Benign/attack corpus message selection
- Test structure and organization

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| INJ-01 | Sanitization pre-pass normalizes Unicode (NFKC), strips zero-width characters, and maps homoglyphs before pattern matching | `normalize.rs` module using `unicode-normalization` crate for NFKC, manual zero-width strip and confusable mapping table |
| INJ-02 | Injection classifier detects base64-encoded payloads, decodes them, and re-scans decoded content | Base64 heuristic regex in normalize.rs, `base64` crate 0.22 (already in workspace) for decoding, re-scan triggers EncodingEvasion category |
| INJ-03 | Pattern set expanded from 11 to ~25 covering prompt leaking, jailbreak keywords, delimiter manipulation, and encoding obfuscation | 5 new InjectionCategory variants, ~14 new English patterns across PromptLeaking, Jailbreak, DelimiterManipulation, IndirectInjection categories |
| INJ-04 | Indirect injection patterns detect instructions hidden in HTML comments, markdown, and JSON content from tool outputs | HTML comment regex, markdown fence regex, recursive JSON string extraction in normalize.rs Extract phase; per-segment scanning |
| INJ-05 | Multi-language injection patterns cover French, German, Spanish, Chinese, and Japanese attack vectors | 3-5 patterns per language using literal strings; new `language` field on InjectionPattern struct |
| INJ-06 | Configurable severity weights via TOML config allow operators to tune per-category detection thresholds | `severity_weights: HashMap<String, f64>` in InputDetectionConfig; weight * base_severity in score calculation; 0.0 disables, 3.0 cap |
| INJ-07 | Canary token planted in system prompt detected if echoed in LLM output, indicating prompt leaking attack | `canary.rs` module with global + per-session UUID tokens; exact substring match in output screening path; SecurityEvent::CanaryDetection |
| INJ-08 | Benign message corpus (100+ messages) validates all patterns have acceptable false positive rate | JSON fixture files in tests/fixtures/; benign_corpus.json (100+), attack_corpus.json (50+); hard CI gate tests |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| unicode-normalization | 0.1.25 | NFKC normalization | 333M downloads, standard Unicode normalization in Rust. Provides `nfkc()` iterator via `UnicodeNormalization` trait. No-std compatible. |
| base64 | 0.22 | Base64 decode | Already in workspace. Provides `Engine::decode()` with proper error handling for invalid input. |
| regex | workspace | Pattern matching | Already used throughout blufio-injection. LazyLock + RegexSet pattern stays. |
| uuid | workspace | Canary token generation | Already in workspace. `Uuid::new_v4()` for random canary tokens. |
| serde_json | 1 | JSON corpus files + recursive JSON scanning | Already a dependency. Used for test fixtures and JSON string extraction. |
| HashMap (std) | - | Severity weights, confusable mapping | Standard library HashMap for severity_weights config field. |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| metrics (facade) | workspace | Prometheus metrics | Already used. Add category label, canary counter, scan duration histogram. |
| tracing | workspace | Structured logging | Already used. Log normalization reports, canary detections, timeout warnings. |
| tokio (sync) | workspace | Timeout enforcement | Already a dependency. Use `tokio::time::timeout` for 20ms pipeline timeout. |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| unicode-normalization | icu_normalizer (ICU4X) | More complete but 10x heavier; NFKC is all we need |
| unicode-normalization | std (no external crate) | Rust std has no NFKC support; crate is required |
| Manual confusable table | unicode-security crate | Full Unicode TR39 skeleton matching is overkill for ~50-100 common confusables; static table is simpler and faster |
| Simple base64 heuristic regex | data-encoding | base64 crate already in workspace; no benefit to switching |

**Installation:**
```bash
# Add to crates/blufio-injection/Cargo.toml
# [dependencies]
# unicode-normalization = "0.1"
```
Note: `base64`, `regex`, `uuid`, `serde_json`, `metrics`, `tracing`, `tokio` are already dependencies.

## Architecture Patterns

### Recommended Module Structure
```
crates/blufio-injection/src/
  lib.rs              # Add pub mod normalize, pub mod canary
  normalize.rs        # NEW: NormalizationPipeline (NFKC, zero-width, confusable, base64, extract)
  canary.rs           # NEW: CanaryTokenManager (generate, detect, self-test)
  patterns.rs         # EXTEND: 5 new InjectionCategory variants, language field, ~14 new patterns
  classifier.rs       # EXTEND: normalize pre-pass, severity weight multiplication, dual scan
  pipeline.rs         # EXTEND: timeout enforcement, canary check delegation
  output_screen.rs    # EXTEND: canary token detection on LLM output
  config.rs           # EXTEND: re-export new config fields
  metrics.rs          # EXTEND: category label, canary counter, scan duration histogram
  events.rs           # EXTEND: CanaryDetection event constructor, normalization_report field
  boundary.rs         # UNCHANGED
  hitl.rs             # UNCHANGED

crates/blufio-injection/tests/
  fixtures/
    benign_corpus.json   # NEW: 100+ benign messages (JSON array of strings)
    attack_corpus.json   # NEW: 50+ attack messages (JSON array of strings)
  corpus_validation.rs   # NEW: integration tests loading JSON fixtures

crates/blufio-config/src/model.rs
  InputDetectionConfig   # EXTEND: severity_weights field

crates/blufio-bus/src/events.rs
  SecurityEvent          # EXTEND: CanaryDetection variant

crates/blufio/src/main.rs
  InjectionCommands      # EXTEND: TestCanary, ValidateCorpus subcommands

crates/blufio/src/cli/injection_cmd.rs
  # EXTEND: handlers for test-canary, validate-corpus

crates/blufio/src/doctor.rs
  # EXTEND: canary self-test in check_injection_defense()

contrib/blufio.example.toml
  # EXTEND: severity_weights section with commented-out defaults
```

### Pattern 1: Normalization Pipeline (normalize.rs)
**What:** A sequential pipeline: Normalize -> Decode -> Extract -> report
**When to use:** Called by classifier before pattern matching on every message

```rust
// normalize.rs -- core pipeline structure

use unicode_normalization::UnicodeNormalization;

/// Report of normalization actions taken on input.
#[derive(Debug, Clone, Default)]
pub struct NormalizationReport {
    pub zero_width_count: usize,
    pub confusables_mapped: usize,
    pub base64_segments_decoded: usize,
}

/// Result of the normalization pipeline.
#[derive(Debug, Clone)]
pub struct NormalizedInput {
    /// The normalized text (NFKC + zero-width stripped + confusables mapped).
    pub text: String,
    /// Decoded base64 segments found in the input.
    pub decoded_segments: Vec<String>,
    /// Extracted content from HTML comments, markdown fences, JSON values.
    pub extracted_segments: Vec<ExtractedSegment>,
    /// Report of what was normalized.
    pub report: NormalizationReport,
}

#[derive(Debug, Clone)]
pub struct ExtractedSegment {
    pub content: String,
    pub source: SegmentSource,
}

#[derive(Debug, Clone)]
pub enum SegmentSource {
    HtmlComment,
    MarkdownFence,
    JsonValue,
}

/// Run the full normalization pipeline on input text.
pub fn normalize(input: &str) -> NormalizedInput {
    let mut report = NormalizationReport::default();

    // Step 1: Strip zero-width characters
    let (stripped, zw_count) = strip_zero_width(input);
    report.zero_width_count = zw_count;

    // Step 2: NFKC normalize
    let nfkc: String = stripped.nfkc().collect();

    // Step 3: Map confusables (Latin/Cyrillic/Greek lookalikes)
    let (mapped, conf_count) = map_confusables(&nfkc);
    report.confusables_mapped = conf_count;

    // Step 4: Detect and decode base64 segments
    let (decoded_segments, b64_count) = decode_base64_segments(&mapped);
    report.base64_segments_decoded = b64_count;

    NormalizedInput {
        text: mapped,
        decoded_segments,
        extracted_segments: vec![], // Populated by extract_content() for tool outputs
        report,
    }
}

/// Extract content from HTML comments, markdown fences, JSON values.
/// Called separately for tool output scanning (INJ-04).
pub fn extract_content(input: &str) -> Vec<ExtractedSegment> {
    // HTML comments: <!-- ... -->
    // Markdown fences: ```...```
    // JSON string values: recursive extraction
    todo!()
}
```

### Pattern 2: Extended InjectionPattern with Language Field
**What:** Adding `language` field to `InjectionPattern` and 5 new `InjectionCategory` variants
**When to use:** All patterns in the PATTERNS array

```rust
// patterns.rs extension

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InjectionCategory {
    RoleHijacking,
    InstructionOverride,
    DataExfiltration,
    PromptLeaking,           // NEW
    Jailbreak,               // NEW
    DelimiterManipulation,   // NEW
    IndirectInjection,       // NEW
    EncodingEvasion,         // NEW
}

pub struct InjectionPattern {
    pub category: InjectionCategory,
    pub pattern: &'static str,
    pub severity: f64,
    pub language: &'static str,  // NEW: "en", "fr", "de", "es", "zh", "ja"
}

// Example new patterns (Claude's discretion on exact regex):
InjectionPattern {
    category: InjectionCategory::PromptLeaking,
    pattern: r"(?i)(repeat|show|display|output|print)\s+(your|the)\s+(system\s+)?(prompt|instructions)",
    severity: 0.4,
    language: "en",
},
InjectionPattern {
    category: InjectionCategory::Jailbreak,
    pattern: r"(?i)(DAN|developer|unrestricted|jailbreak|bypass\s+safety)\s+mode",
    severity: 0.5,
    language: "en",
},
// French example:
InjectionPattern {
    category: InjectionCategory::RoleHijacking,
    pattern: r"(?i)ignore[rz]?\s+(toutes?\s+)?(les?\s+)?instructions?\s+pr[eé]c[eé]dentes?",
    severity: 0.5,
    language: "fr",
},
```

### Pattern 3: Severity Weight Multiplication
**What:** Category-level weights applied to base severity during score calculation
**When to use:** In `calculate_score()` after pattern matching

```rust
// classifier.rs -- modified score calculation

fn calculate_score(
    matches: &[InjectionMatch],
    input_length: usize,
    severity_weights: &HashMap<String, f64>,
    evasion_bonus: f64,  // 0.0, 0.1, or 0.2 based on normalization report
) -> f64 {
    if matches.is_empty() {
        return 0.0;
    }

    let mut score = 0.0;

    for m in matches {
        // Look up category weight (default 1.0 if not configured)
        let weight = severity_weights
            .get(&m.category.to_string())
            .copied()
            .unwrap_or(1.0)
            .clamp(0.0, 3.0);

        // Weight=0.0 disables this category entirely
        if weight == 0.0 {
            continue;
        }

        // Weighted severity
        score += m.severity * weight;

        // Positional bonus (unchanged)
        let position_ratio = 1.0 - (m.span.start as f64 / input_length.max(1) as f64);
        score += position_ratio * 0.1;
    }

    // Match count bonus (unchanged)
    if matches.len() > 1 {
        score += (matches.len() - 1) as f64 * 0.1;
    }

    // Evasion signal bonus (independent of category weights)
    score += evasion_bonus;

    score.clamp(0.0, 1.0)
}
```

### Pattern 4: Canary Token Manager
**What:** Global + per-session canary token generation and detection
**When to use:** Tokens planted at system prompt assembly time, detection in output screening

```rust
// canary.rs

use uuid::Uuid;

pub struct CanaryTokenManager {
    /// Global canary token (generated once at server startup).
    global_token: String,
    /// Per-session canary token (new UUID per session).
    session_token: Option<String>,
}

impl CanaryTokenManager {
    pub fn new() -> Self {
        Self {
            global_token: Uuid::new_v4().to_string(),
            session_token: None,
        }
    }

    pub fn new_session(&mut self) -> String {
        let token = Uuid::new_v4().to_string();
        self.session_token = Some(token.clone());
        token
    }

    /// Returns the canary line to append to the system prompt.
    pub fn canary_line(&self) -> String {
        let session = self.session_token.as_deref().unwrap_or("");
        // Both tokens on the same line, separated
        format!(
            "CONFIDENTIAL_TOKEN: {} {}",
            self.global_token, session
        )
    }

    /// Check if either canary token appears in LLM output.
    /// Returns true if a canary leak is detected.
    pub fn detect_leak(&self, output: &str) -> bool {
        if output.contains(&self.global_token) {
            return true;
        }
        if let Some(ref session) = self.session_token {
            if output.contains(session) {
                return true;
            }
        }
        false
    }
}
```

### Anti-Patterns to Avoid
- **Concatenating extracted segments before scanning:** Per the decision, scan each extracted segment independently to prevent cross-segment false positive matching (e.g., "ignore" in one JSON field + "instructions" in another).
- **Using a full HTML parser for extraction:** The decision explicitly says simple regex. Do NOT add `scraper`, `html5ever`, or similar. A regex for `<!--[\s\S]*?-->` is sufficient.
- **Hot-reloading severity weights:** The decision says restart required. Do NOT add watch/notify for weight changes.
- **Fuzzy canary matching:** The decision says exact substring match of full UUID only. Do NOT implement partial matching, edit-distance, or obfuscation-aware matching.
- **Disabling normalization conditionally:** Always normalize every message. No feature flag or config toggle for the normalization pre-pass.
- **Creating a separate L6 layer for canary detection:** Canary detection integrates into the existing output screening path (L4), not as a new layer.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| NFKC normalization | Custom character decomposition | `unicode-normalization` crate `nfkc()` | Unicode normalization has thousands of edge cases; the crate tracks Unicode standard updates |
| Base64 decoding | Manual base64 decoder | `base64` crate 0.22 `Engine::decode()` | Handles padding variants, URL-safe alphabet, proper error propagation |
| UUID generation | Random string generation | `uuid` crate `Uuid::new_v4()` | Cryptographically random, proper formatting, already in workspace |
| Regex compilation | Manual string matching | `regex` crate `RegexSet` + individual `Regex` | Existing two-phase pattern (fast set check then detail extraction) is optimal |
| JSON recursive extraction | Manual JSON string parsing | `serde_json::Value` recursive walk | Handles nested objects, arrays, escaped strings correctly |
| Prometheus metrics | Manual counter tracking | `metrics` facade crate | Already used throughout; consistent with existing metric patterns |

**Key insight:** The normalization pipeline combines a well-tested external crate (`unicode-normalization`) with a small, custom confusable mapping table. The confusable table is intentionally limited to ~50-100 common Latin/Cyrillic/Greek lookalikes rather than the full Unicode TR39 confusables dataset, which would add significant complexity for minimal security gain in this context.

## Common Pitfalls

### Pitfall 1: RegexSet Index Misalignment After Pattern Expansion
**What goes wrong:** Adding new patterns to `PATTERNS` array but failing to keep `INJECTION_REGEX_SET` and `INJECTION_REGEXES` indices aligned.
**Why it happens:** The existing design builds both from the same array, but adding the `language` field or changing array ordering could introduce bugs.
**How to avoid:** Keep the existing single-source-of-truth pattern. The `PATTERNS` array drives everything. The existing test `injection_regexes_array_indices_align_with_patterns` will catch misalignment.
**Warning signs:** Test failures in `patterns::tests`, or wrong category reported for a match.

### Pitfall 2: False Positives from Aggressive Normalization
**What goes wrong:** NFKC normalization converts characters that appear in legitimate text, causing benign messages to match patterns.
**Why it happens:** NFKC is aggressive -- it normalizes superscripts, ligatures, fullwidth characters to ASCII equivalents. "fi" ligature becomes "fi", which could complete a word that matches a pattern.
**How to avoid:** The dual-scan approach (scan BOTH original and normalized, merge results) plus the 0% FP corpus validation catches this. The corpus MUST include legitimate text with fullwidth characters, ligatures, and mathematical symbols.
**Warning signs:** Benign corpus test failures, especially in multi-language messages.

### Pitfall 3: Base64 Heuristic Over-Matching
**What goes wrong:** The base64 detection regex matches legitimate base64 content (e.g., image data URIs, JWT tokens, encoded file content).
**Why it happens:** Base64 encoding is used everywhere legitimately. A 20+ character segment of `[A-Za-z0-9+/=]` will match many non-malicious strings.
**How to avoid:** Only flag as `EncodingEvasion` when the DECODED content matches another injection category. The base64 detection itself is not an evasion signal -- only the decoded-content-matches result is.
**Warning signs:** High false positive rate on tool outputs containing base64-encoded files, JWT tokens, or API responses.

### Pitfall 4: `deny_unknown_fields` Breaking Severity Weights Config
**What goes wrong:** The `InputDetectionConfig` uses `#[serde(deny_unknown_fields)]`. Adding `severity_weights` as a new field means existing TOML files without this field will still work (it's `#[serde(default)]`), but if the operator adds a TYPO in the section name, it will be silently ignored.
**Why it happens:** The weights are under `[injection_defense.input_detection.severity_weights]` which is a nested table. A typo like `severety_weights` would be an unknown field.
**How to avoid:** The `deny_unknown_fields` attribute on `InputDetectionConfig` will catch typos. The severity_weights field needs `#[serde(default)]` to be optional, and the HashMap keys need validation at construction time (warn on unrecognized category names).
**Warning signs:** Operator reports "my weights aren't being applied" -- check for typos in category names.

### Pitfall 5: Performance Regression from Normalization Pipeline
**What goes wrong:** Adding NFKC normalization + confusable mapping + base64 scanning + dual-scan exceeds the 20ms budget.
**Why it happens:** NFKC normalization is O(n) but with a non-trivial constant factor. Base64 regex scanning adds another pass. Scanning both original and normalized text doubles regex work.
**How to avoid:** Keep the LazyLock pattern so RegexSet is compiled once. Use `tokio::time::timeout` for the 20ms hard limit. On timeout, allow the message + log warning (don't block). Criterion benchmarks in Phase 68 will measure actual latency.
**Warning signs:** `injection_scan_duration_seconds` p99 approaching or exceeding 20ms in production.

### Pitfall 6: Multi-Language Pattern False Positives
**What goes wrong:** French/German/Spanish patterns match legitimate text in those languages because common words overlap with attack vocabulary.
**Why it happens:** Words like "instructions" (French) or "System" (German) appear in normal conversation.
**How to avoid:** Multi-language patterns must be phrase-level, not single-word. "Ignore les instructions" is a pattern; "instructions" alone is not. The benign corpus must include 10-20 per-language messages with trigger-like words in benign contexts.
**Warning signs:** Multi-language benign corpus tests failing.

### Pitfall 7: Canary Token Timing -- Checking Before Full Response
**What goes wrong:** Canary detection runs on partial/streamed response instead of buffered full response, missing split tokens.
**Why it happens:** The LLM response might be streamed token-by-token. A UUID could be split across chunks.
**How to avoid:** The decision explicitly says "Scans complete LLM response (not streamed chunks) -- buffers full response before checking". Canary detection MUST run after the full response is assembled, before returning to user.
**Warning signs:** Canary detection misses in streaming mode.

## Code Examples

### Zero-Width Character Stripping
```rust
// Source: Unicode standard, zero-width characters list
const ZERO_WIDTH_CHARS: &[char] = &[
    '\u{200B}', // ZERO WIDTH SPACE
    '\u{200C}', // ZERO WIDTH NON-JOINER
    '\u{200D}', // ZERO WIDTH JOINER
    '\u{FEFF}', // ZERO WIDTH NO-BREAK SPACE (BOM)
    '\u{2060}', // WORD JOINER
    '\u{180E}', // MONGOLIAN VOWEL SEPARATOR
    '\u{00AD}', // SOFT HYPHEN
];

// Unicode tag characters (U+E0001-U+E007F)
fn is_unicode_tag(c: char) -> bool {
    ('\u{E0001}'..='\u{E007F}').contains(&c)
}

fn strip_zero_width(input: &str) -> (String, usize) {
    let mut count = 0;
    let result: String = input
        .chars()
        .filter(|c| {
            if ZERO_WIDTH_CHARS.contains(c) || is_unicode_tag(*c) {
                count += 1;
                false
            } else {
                true
            }
        })
        .collect();
    (result, count)
}
```

### Confusable Mapping Table
```rust
// Source: Unicode TR39 confusables subset -- common Latin/Cyrillic/Greek lookalikes
// This is a static compile-time table, NOT the full TR39 dataset
fn map_confusables(input: &str) -> (String, usize) {
    let mut count = 0;
    let result: String = input
        .chars()
        .map(|c| {
            match confusable_to_latin(c) {
                Some(mapped) => {
                    count += 1;
                    mapped
                }
                None => c,
            }
        })
        .collect();
    (result, count)
}

/// Map a confusable character to its Latin equivalent.
/// Returns None if the character is not a known confusable.
fn confusable_to_latin(c: char) -> Option<char> {
    // Cyrillic -> Latin lookalikes
    match c {
        '\u{0410}' => Some('A'), // Cyrillic A
        '\u{0412}' => Some('B'), // Cyrillic VE -> B
        '\u{0421}' => Some('C'), // Cyrillic ES -> C
        '\u{0415}' => Some('E'), // Cyrillic IE -> E
        '\u{041D}' => Some('H'), // Cyrillic EN -> H
        '\u{041A}' => Some('K'), // Cyrillic KA -> K
        '\u{041C}' => Some('M'), // Cyrillic EM -> M
        '\u{041E}' => Some('O'), // Cyrillic O
        '\u{0420}' => Some('P'), // Cyrillic ER -> P
        '\u{0422}' => Some('T'), // Cyrillic TE -> T
        '\u{0425}' => Some('X'), // Cyrillic HA -> X
        // Lowercase Cyrillic
        '\u{0430}' => Some('a'), // Cyrillic a
        '\u{0435}' => Some('e'), // Cyrillic ie -> e
        '\u{043E}' => Some('o'), // Cyrillic o
        '\u{0440}' => Some('p'), // Cyrillic er -> p
        '\u{0441}' => Some('c'), // Cyrillic es -> c
        '\u{0443}' => Some('y'), // Cyrillic u -> y
        '\u{0445}' => Some('x'), // Cyrillic ha -> x
        // Greek -> Latin lookalikes
        '\u{0391}' => Some('A'), // Greek Alpha
        '\u{0392}' => Some('B'), // Greek Beta
        '\u{0395}' => Some('E'), // Greek Epsilon
        '\u{0397}' => Some('H'), // Greek Eta
        '\u{0399}' => Some('I'), // Greek Iota
        '\u{039A}' => Some('K'), // Greek Kappa
        '\u{039C}' => Some('M'), // Greek Mu
        '\u{039D}' => Some('N'), // Greek Nu
        '\u{039F}' => Some('O'), // Greek Omicron
        '\u{03A1}' => Some('P'), // Greek Rho
        '\u{03A4}' => Some('T'), // Greek Tau
        '\u{03A7}' => Some('X'), // Greek Chi
        '\u{03B1}' => Some('a'), // Greek alpha
        '\u{03BF}' => Some('o'), // Greek omicron
        // ... extend to ~50-100 total mappings
        _ => None,
    }
}
```

### Base64 Heuristic Detection
```rust
use std::sync::LazyLock;
use regex::Regex;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// Regex for detecting potential base64-encoded segments (20+ chars).
/// Matches sequences of base64 characters with optional padding.
static BASE64_SEGMENT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z0-9+/]{20,}={0,2}").expect("base64 heuristic regex must compile")
});

fn decode_base64_segments(input: &str) -> (Vec<String>, usize) {
    let mut decoded = Vec::new();
    for m in BASE64_SEGMENT_RE.find_iter(input) {
        if let Ok(bytes) = STANDARD.decode(m.as_str()) {
            if let Ok(text) = String::from_utf8(bytes) {
                // Only keep decoded text that is valid UTF-8 and non-empty
                if !text.trim().is_empty() {
                    decoded.push(text);
                }
            }
        }
    }
    let count = decoded.len();
    (decoded, count)
}
```

### Recursive JSON String Extraction
```rust
/// Extract all string values from a JSON value, recursively.
/// Used for indirect injection scanning of tool outputs (INJ-04).
/// Stops after scanning first 50KB of serialized content.
fn extract_json_strings(value: &serde_json::Value, out: &mut Vec<String>, budget: &mut usize) {
    if *budget == 0 {
        return;
    }
    match value {
        serde_json::Value::String(s) => {
            let len = s.len().min(*budget);
            out.push(s[..len].to_string());
            *budget = budget.saturating_sub(s.len());
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                extract_json_strings(item, out, budget);
                if *budget == 0 { break; }
            }
        }
        serde_json::Value::Object(map) => {
            for (_, val) in map {
                extract_json_strings(val, out, budget);
                if *budget == 0 { break; }
            }
        }
        _ => {}
    }
}
```

### SecurityEvent Extension for Canary Detection
```rust
// In blufio-bus/src/events.rs -- add new variant to SecurityEvent enum
SecurityEvent::CanaryDetection {
    event_id: String,
    timestamp: String,
    correlation_id: String,
    token_type: String,  // "global" or "session"
    action: String,      // "blocked"
    content: String,     // truncated output for forensics
},
```

### Config Extension for Severity Weights
```rust
// In blufio-config/src/model.rs -- extend InputDetectionConfig
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InputDetectionConfig {
    #[serde(default = "default_detection_mode")]
    pub mode: String,
    #[serde(default = "default_blocking_threshold")]
    pub blocking_threshold: f64,
    #[serde(default = "default_mcp_blocking_threshold")]
    pub mcp_blocking_threshold: f64,
    #[serde(default)]
    pub custom_patterns: Vec<String>,
    /// Per-category severity weight multipliers (1.0 = neutral, 0.0 = disabled, max 3.0).
    #[serde(default)]
    pub severity_weights: std::collections::HashMap<String, f64>,
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Regex-only injection detection | Normalize-then-scan pipeline | 2024-2025 (OWASP LLM Top 10 v2) | Raw regex can be evaded by Unicode tricks; normalization pre-pass is now standard |
| English-only patterns | Multi-language pattern sets | 2025 (multi-language LLM adoption) | Non-English injection payloads bypass English-only filters |
| Static pattern severity | Configurable category weights | 2025 (operator-tunable security) | Different deployments have different risk profiles |
| No canary tokens | Canary token leak detection | 2024 (Vigil-LLM, Rebuff) | Detects system prompt exfiltration attacks |
| Post-hoc FP analysis | Paired corpus CI gates | 2025 (security-as-code) | Catches FP regressions before deployment |

**Deprecated/outdated:**
- Single-pass regex without normalization: Easily evaded by Unicode tricks (OWASP acknowledges this as a top technique)
- Full Unicode TR39 confusables: Overkill for LLM injection defense; ~50-100 common confusables is sufficient

## Open Questions

1. **Exact confusable character count**
   - What we know: The decision says ~50-100 Latin/Cyrillic/Greek lookalikes
   - What's unclear: The exact boundary between "common enough to matter" and "too rare to bother"
   - Recommendation: Start with the ~30 most common Cyrillic/Greek lookalikes (uppercase + lowercase), expand to ~60-80 if test coverage reveals gaps. The confusable table is a static constant, so it can be extended in future phases without breaking changes.

2. **Base64 heuristic tuning**
   - What we know: 20+ character segments of base64 alphabet
   - What's unclear: Will this match too many legitimate strings in tool output?
   - Recommendation: Only flag as EncodingEvasion when decoded content matches another category. The heuristic regex is permissive by design; the downstream re-scan is the actual filter.

3. **Canary token placement format**
   - What we know: "CONFIDENTIAL_TOKEN: {uuid}" at end of system prompt
   - What's unclear: Whether both tokens go on one line or two
   - Recommendation: One line with both UUIDs separated by space: `CONFIDENTIAL_TOKEN: {global_uuid} {session_uuid}`. Simple, detectable, easy to parse.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test framework + cargo test |
| Config file | Workspace Cargo.toml + per-crate Cargo.toml |
| Quick run command | `cargo test -p blufio-injection --lib` |
| Full suite command | `cargo test -p blufio-injection` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| INJ-01 | NFKC normalization strips zero-width, maps confusables | unit | `cargo test -p blufio-injection normalize -- -x` | Wave 0 |
| INJ-02 | Base64 payload decoded and re-scanned | unit | `cargo test -p blufio-injection base64 -- -x` | Wave 0 |
| INJ-03 | 25+ patterns across 8 categories | unit | `cargo test -p blufio-injection patterns -- -x` | Partial (11 patterns exist) |
| INJ-04 | Indirect injection in HTML/markdown/JSON | unit | `cargo test -p blufio-injection extract -- -x` | Wave 0 |
| INJ-05 | Multi-language patterns (FR/DE/ES/ZH/JA) | unit | `cargo test -p blufio-injection language -- -x` | Wave 0 |
| INJ-06 | Severity weights multiply score correctly | unit | `cargo test -p blufio-injection weight -- -x` | Wave 0 |
| INJ-07 | Canary token detection blocks leaked output | unit | `cargo test -p blufio-injection canary -- -x` | Wave 0 |
| INJ-08 | Benign corpus 0% FP, attack corpus all detected | integration | `cargo test -p blufio-injection --test corpus_validation -x` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-injection --lib`
- **Per wave merge:** `cargo test -p blufio-injection && cargo test -p blufio-config --lib`
- **Phase gate:** Full suite green + `cargo clippy -p blufio-injection -- -D warnings` + `cargo fmt -- --check`

### Wave 0 Gaps
- [ ] `crates/blufio-injection/src/normalize.rs` -- new module (INJ-01, INJ-02, INJ-04)
- [ ] `crates/blufio-injection/src/canary.rs` -- new module (INJ-07)
- [ ] `crates/blufio-injection/tests/` -- create tests directory
- [ ] `crates/blufio-injection/tests/fixtures/benign_corpus.json` -- benign corpus (INJ-08)
- [ ] `crates/blufio-injection/tests/fixtures/attack_corpus.json` -- attack corpus (INJ-08)
- [ ] `crates/blufio-injection/tests/corpus_validation.rs` -- integration test (INJ-08)
- [ ] Add `unicode-normalization = "0.1"` to `crates/blufio-injection/Cargo.toml`

## Sources

### Primary (HIGH confidence)
- Existing codebase: `crates/blufio-injection/src/*.rs` -- all 10 source files read and analyzed
- `crates/blufio-config/src/model.rs` lines 2440-2600 -- existing config model
- `crates/blufio-bus/src/events.rs` lines 748-810 -- SecurityEvent enum
- `crates/blufio/src/doctor.rs` -- existing doctor check patterns
- `crates/blufio/src/cli/injection_cmd.rs` -- existing CLI subcommand pattern
- `crates/blufio/src/main.rs` lines 672-699 -- InjectionCommands enum

### Secondary (MEDIUM confidence)
- [unicode-normalization crate](https://crates.io/crates/unicode-normalization) -- v0.1.25, 333M downloads, NFKC via `nfkc()` method
- [base64 crate](https://crates.io/crates/base64) -- v0.22, already in workspace
- [OWASP LLM01:2025 Prompt Injection](https://genai.owasp.org/llmrisk/llm01-prompt-injection/) -- canary tokens, Unicode evasion as top technique
- [OWASP LLM Prompt Injection Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/LLM_Prompt_Injection_Prevention_Cheat_Sheet.html)
- [Vigil-LLM canary token implementation](https://github.com/deadbits/vigil-llm)
- [Rebuff prompt injection detector](https://github.com/protectai/rebuff)
- [Unicode confusables.txt vs NFKC](https://paultendo.github.io/posts/unicode-confusables-nfkc-conflict/) -- explains why confusable mapping is separate from NFKC
- [Bypassing Prompt Injection and Jailbreak Detection in LLM Guardrails](https://arxiv.org/html/2504.11168v2) -- zero-width, homoglyph, diacritics evasion rates 44-76%

### Tertiary (LOW confidence)
- [Multilingual prompt injection research](https://nwosunneoma.medium.com/multilingual-prompt-injection-your-llms-safety-net-has-a-language-problem-440d9aaa8bac) -- confirms phrase-level patterns needed, not word-level
- [Multilingual Hidden Prompt Injection Attacks](https://arxiv.org/abs/2512.23684) -- Japanese and Chinese injection patterns effective

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in workspace or well-established crates
- Architecture: HIGH -- extending existing well-structured crate; patterns are clear from CONTEXT.md
- Pitfalls: HIGH -- identified from domain knowledge of Unicode normalization + regex matching + multi-language text + base64 encoding
- Code examples: HIGH -- derived from existing codebase patterns and verified crate APIs

**Research date:** 2026-03-13
**Valid until:** 2026-04-13 (stable domain; Unicode normalization is well-established)
