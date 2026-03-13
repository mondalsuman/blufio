# Phase 66: Injection Defense Hardening - Context

**Gathered:** 2026-03-13
**Status:** Ready for planning

<domain>
## Phase Boundary

Harden the existing L1 injection classifier with Unicode normalization pre-pass, encoding detection, expanded pattern set (~25 patterns from 11), multi-language detection (FR/DE/ES/ZH/JA), configurable severity weights, canary tokens, and false positive validation. All 5 existing defense layers (L1-L5) remain; this phase enhances L1 and adds canary detection to output screening.

</domain>

<decisions>
## Implementation Decisions

### Canary Token Design
- Two canary tokens: one global (generated at server startup, changes on restart) and one per-session (random UUID4 per session)
- Both placed at end of system prompt as last line: "CONFIDENTIAL_TOKEN: {uuid}"
- Always auto-generated — not configurable by operator
- Enabled by default (consistent with L1/L3/L4 defaults from Phase 57)
- Detection: exact substring match of full UUID in LLM output. No partial/fuzzy/obfuscated matching
- Scans complete LLM response (not streamed chunks) — buffers full response before checking
- Applies to all LLM output: text responses AND tool call arguments
- Action on detection: block response entirely + log SecurityEvent + emit EventBus alert
- Blocked response replaced with generic refusal ("I can't process this response.") — never reveal detection reason (consistent with Phase 57 L1 blocking)
- Separate Prometheus metric: injection_canary_detections_total (not combined with L4)
- CLI: `blufio injection test-canary` subcommand for canary self-test
- Doctor check: canary self-test (generate, simulate echo, verify detection) — follows HMAC self-test pattern

### Severity Weight Configuration
- Category-level weights (not per-pattern) — operators set a multiplier per category
- TOML format: `[injection_defense.input_detection.severity_weights]` with one key per category
- All 8 categories get their own weight key: role_hijacking, instruction_override, data_exfiltration, prompt_leaking, jailbreak, delimiter_manipulation, indirect_injection, encoding_evasion
- Default weight: 1.0 (neutral) for all categories — operators adjust up/down from baseline
- Weight = 0.0 disables a category entirely
- Weight cap: maximum 3.0 — prevents absurd multipliers
- Invalid weights (negative, NaN): warn + use default 1.0 (server still starts — Phase 57 pattern)
- Restart required for weight changes (no hot reload — Phase 57 pattern)
- `blufio injection config` shows effective weights (defaults + overrides)
- `blufio injection test <text>` shows both base severity and weighted severity per match
- Prometheus metrics include category label: injection_input_detections_total{category="...", action="..."}
- Weight section included in blufio.example.toml with commented-out defaults
- Custom patterns (operator TOML) assigned to InstructionOverride category (current behavior preserved)

### False Positive Validation
- 0% false positive tolerance — no legitimate message should trigger detection (score > 0)
- Paired corpora: benign corpus (must NOT trigger) + attack corpus (MUST trigger)
- Both are hard CI gates — tests fail if any benign message scores > 0, or any attack message scores 0
- Corpus format: JSON arrays — benign_corpus.json and attack_corpus.json in crates/blufio-injection/tests/fixtures/
- Benign corpus (100+ messages): broad mix — casual chat, technical discussion, code snippets, multi-language text, security topic discussion (educational), programming patterns with trigger-like keywords (send, system, output)
- Multi-language benign messages: 10-20 per supported language (FR/DE/ES/ZH/JA) — validates multi-language patterns don't FP
- Attack corpus (50+ messages): ~2 variants per pattern, plus Unicode-evaded and base64-encoded variants
- Corpus is test fixtures only — not shipped in binary, not in `blufio doctor`
- CLI: `blufio injection validate-corpus <path>` for operators to test their own custom benign messages against current patterns

### Multi-Language Coverage
- Key attack phrases only (3-5 patterns per language) — not full translation of all English patterns
- Languages: French, German, Spanish, Chinese, Japanese (per requirements)
- Patterns use existing categories (RoleHijacking, InstructionOverride, etc.) — language is metadata, not a separate attack type
- Same severity as English equivalents — language doesn't change threat level
- InjectionPattern struct gets new `language: &'static str` field ("en", "fr", "de", "es", "zh", "ja")
- Chinese/Japanese patterns use literal character strings (not Unicode property classes)
- Additional languages via existing custom_patterns TOML config (document this use case)
- `blufio injection status` shows per-language detection counts
- snake_case display names for categories in CLI/metrics (consistent with existing Display impl)

### Pre-processing Pipeline
- Pipeline order: Normalize -> Decode -> Extract -> Scan (sequential, each step feeds the next)
- Always normalize every message (NFKC + zero-width strip + confusable mapping) — no conditional path
- Base64 scanning on every message (not just tool outputs) — heuristic detection of 20+ char segments
- Classifier scans BOTH original AND normalized text, merges all matches (union)
- Zero-width character presence adds 0.1 evasion signal bonus to score
- Confusable character detection (mixed Latin/Cyrillic/Greek) adds same 0.1 evasion signal bonus
- Confusable mapping: common Latin/Cyrillic/Greek lookalikes only (~50-100 character mappings)
- Confusable mapping table: compile-time constant (static HashMap or match expression)
- NormalizationReport produced: zero_width_count, confusables_mapped, base64_segments_decoded — included in SecurityEvents
- `blufio injection test <text>` shows normalization step output (original vs normalized, what was stripped/mapped)
- Separate module: `normalize.rs` in blufio-injection (not inline in classifier)
- HTML/markdown extraction: simple regex (not full HTML parser) — extract HTML comment content, markdown code fence content, JSON string values
- Normalization code lives in blufio-injection (not shared blufio-security)

### New Pattern Categories
- 5 new InjectionCategory variants (total 8): PromptLeaking, Jailbreak, DelimiterManipulation, IndirectInjection, EncodingEvasion
- Exhaustive enum — no Other variant, no catch-all
- PromptLeaking: direct extraction attempts only ("repeat your system prompt", "output your instructions verbatim") — not indirect probing
- Jailbreak: known jailbreak keywords ("DAN mode", "developer mode", "unrestricted mode", "jailbreak", "bypass safety")
- DelimiterManipulation: template/markup delimiters — XML tags (<system>, </user>), JSON structure ({"role":"system"}), markdown headings (## System:)
- IndirectInjection: instructions hidden in structured content — HTML comments, hidden JSON fields, markdown with instruction URLs
- EncodingEvasion: triggers when decoded content (base64/encoded) matches another category — not on technique presence alone
- EncodingEvasion gets its own severity weight key (separate from underlying category weight)
- CLI test output shows category + language + severity + weighted score per match

### Indirect Injection Depth
- Full recursive JSON scanning — extract ALL string values at any depth
- Size limit: scan first 50KB of tool output (truncate with warning if exceeded)
- Truncation is NOT an evasion signal — just a performance guard
- HTML comment scanning: tool outputs only (not user messages)
- Markdown code fence content: scanned (not skipped)
- Part of existing L1 scan (not a separate layer)
- SecurityEvent includes full attribution: tool_name, server_name (MCP), skill_name (WASM) — consistent with Phase 57
- Reuse existing MCP server `trusted = true` flag to skip indirect injection scanning for trusted servers
- Extracted content scanned per-segment (not concatenated) — prevents cross-segment FP matching

### Performance Budget
- 20ms timeout for full L1 pipeline (normalize -> decode -> extract -> scan original -> scan normalized)
- Timeout action: allow message + log warning (don't block on timeout)
- Prometheus histogram: injection_scan_duration_seconds for p50/p95/p99 latency tracking
- Keep LazyLock pattern for RegexSet compilation (zero startup cost if disabled)
- Criterion benchmark (Phase 68): full pipeline benchmark, not just pattern matching
- Confusable mapping: compile-time constant for zero-cost lookups

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

</decisions>

<specifics>
## Specific Ideas

- Normalization pipeline (normalize.rs) follows same architecture as patterns.rs — single source of truth, compile-time constants
- Canary detection integrates into the existing output screening path (L4) since it scans LLM output
- Severity weights multiply the base pattern severity before scoring (score = base_severity * category_weight + position_bonus)
- The evasion signal bonus (0.1 for zero-width, 0.1 for confusables) is independent of category weights — it's an input-level signal
- Per-segment scanning for indirect injection prevents "ignore" in one JSON field + "instructions" in another from matching as a single pattern

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `patterns.rs`: PATTERNS array + INJECTION_REGEX_SET + INJECTION_REGEXES — extend with new categories and language field
- `classifier.rs`: InjectionClassifier with classify() — add normalization pre-pass and severity weight multiplication
- `config.rs`: Re-exports from blufio-config — extend InputDetectionConfig with severity_weights HashMap
- `pipeline.rs`: InjectionPipeline with scan_input() — add canary detection to output path
- `output_screen.rs`: OutputScreener — extend with canary token checking
- `metrics.rs`: Prometheus metrics — add category label, canary counter, scan duration histogram

### Established Patterns
- LazyLock for compiled RegexSet (zero-cost until first use)
- InjectionPattern struct with category, pattern, severity — extend with language field
- InjectionCategory enum with Display impl (snake_case) — add 5 new variants
- Two-phase detection: RegexSet fast path -> individual Regex detail extraction
- Custom pattern support with validation at construction time
- Source-type thresholds (user: 0.95, MCP/WASM: 0.98)

### Integration Points
- `classifier.rs:classify()` — normalization pre-pass before pattern matching
- `pipeline.rs:scan_input()` — timeout enforcement around full pipeline
- `output_screen.rs` or new `canary.rs` — canary detection on LLM output
- `blufio-config/src/model.rs` — new severity_weights field in InputDetectionConfig
- CLI `injection` subcommand — new test-canary and validate-corpus commands
- `blufio doctor` — canary self-test addition

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 66-injection-defense-hardening*
*Context gathered: 2026-03-13*
