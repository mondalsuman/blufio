# Phase 57: Prompt Injection Defense - Context

**Gathered:** 2026-03-12
**Status:** Ready for planning

<domain>
## Phase Boundary

5-layer prompt injection defense protecting the agent loop against injection attacks without blocking legitimate user input. Layers: L1 pattern classifier, L3 HMAC boundary tokens, L4 output validator, L5 human-in-the-loop. Integrates with MCP client tool output and WASM skill results.

</domain>

<decisions>
## Implementation Decisions

### Human-in-the-Loop (L5)
- Default scope: external tool execution (MCP tools, WASM skills) requires confirmation. Internal tools (memory search, cost lookup) always auto-approved
- Delivery: inline message + reply in the same conversation ("Approve [tool_name] with args [...]? Reply YES/NO")
- Timeout: auto-deny after configurable 60s default. User is informed of denial
- Granularity: per-operation allowlist via TOML config. Each operation listed explicitly
- Denied operations: inform user + continue conversation without tool result ("Tool [X] was blocked. I'll answer without it.")
- API/gateway bypass: API requests are trusted (programmatic trust). HITL only for interactive channels
- Confirmation detail: show tool name + summary args (not full JSON, not just name)
- Per-tool trust session: user approves once per tool type per session. Subsequent calls auto-approved
- Multi-agent delegation: Ed25519-signed inter-agent messages bypass HITL (cryptographic verification sufficient)
- Risk labels: tag each confirmation LOW/MEDIUM/HIGH based on tool category
- Max pending: 3 confirmations queued without response = pause and notify
- Language: always English for security message consistency
- Audit: denied operations logged to audit trail with full request context
- No dry-run mode for HITL (confirm/deny is sufficient)
- Batch operations: confirm each tool call individually (not batch-level)
- Non-interactive channels: auto-deny with log if channel can't support reply-based confirmation
- Safe tools default list: memory_search, session_history, cost_lookup, skill_list (configurable)
- Prometheus metrics: hitl_confirmations_total, hitl_denials_total, hitl_timeouts_total
- EventBus: new SecurityEvent type for HITL events + all injection defense events

### Detection Strictness (L1)
- Pattern categories: core patterns only — role hijacking ("ignore previous", "you are now"), instruction override ("system:", "[INST]"), data exfiltration ("send to", "forward to")
- Case: case-insensitive matching
- False positives: log + allow in default mode (log-not-block per INJC-02). Operators tune via audit log review
- Extensibility: hardcoded defaults + TOML custom patterns. Operators add patterns via [injection_defense.input_detection.custom_patterns]
- Scan scope: all user + external input (user messages, MCP output, WASM results, webhook payloads). Skip system prompts and internal data
- Scoring: pattern match count + severity. Score = weighted sum of pattern severity (0.1-0.5) + match count + position in message. Produces 0.0-1.0
- Reporting: operator-only by default. Users see nothing unless blocked. Avoids teaching attackers what triggers detection
- No learning mode — deterministic regex, operators tune via audit logs
- Blocked messages: generic refusal ("I can't process this message.") — never reveal detection reason
- Per-source thresholds: same patterns everywhere, but MCP/WASM uses >0.98 blocking threshold (vs >0.95 for user input)
- Pipeline position: synchronous, pre-LLM. L1 runs before message reaches the model. Regex is <1ms
- CLI testing: blufio injection test <text> command with full scoring breakdown

### HMAC Boundary Tokens (L3)
- Failure action: strip the failed zone from context + log SecurityEvent. LLM never sees corrupted content
- Key scope: per-session key derived from session ID + server secret (HKDF from vault master key with "hmac-boundary" context)
- Visibility: transparent markers — verified and stripped before sending to LLM. Model never sees boundary tokens
- Forensics: full corrupted content logged in SecurityEvent on validation failure
- Zone coverage: all three zones (static, conditional, dynamic) get HMAC boundaries
- User notification on strip: none (transparent). Response may be less informed but no security info leaked
- Export: strip boundaries on export. HMAC tokens are ephemeral security markers
- Dev mode: injection_defense.hmac_boundaries.enabled = false in TOML for development
- Compaction interaction: re-sign after compaction. Compacted content gets fresh HMAC boundaries
- Truncation interaction: re-sign after truncation. Remaining content gets fresh boundaries
- Provenance: each bounded zone includes source metadata (user, mcp:tool_name, skill:name, system)
- Multi-failure: respond with remaining valid zones. Strip all failed zones, proceed with whatever is valid
- All-or-nothing validation: when HMAC is enabled, all zones validated. No per-zone skip
- Logging: failures only (successful validations not logged)
- Doctor check: blufio doctor includes HMAC self-test (generate, validate, verify strip)
- Version prefix: v1 byte in HMAC tokens for future format upgrades
- Cache layer: boundaries apply to final assembled context only, not prompt cache layer
- Prometheus: per-zone counters — hmac_validations_total{zone, result}
- Key rotation: restart required (consistent with vault key lifecycle)

### Output Screening (L4)
- Screen for: credentials (known provider API key formats) + injection relay (heuristic pattern matching on LLM output)
- When: before tool execution only. Stream text to user in real-time, buffer tool call arguments for screening
- Credential leak action: redact + continue. Replace detected credentials with [REDACTED]
- Injection relay action: block tool execution entirely. Don't let LLM execute tools if relay detected
- Reuse: extend existing RedactingWriter/redact infrastructure with credential patterns. Shared pattern registry for log redaction and output screening
- Tool result scanning: yes — MCP/WASM tool output scanned with L1 patterns before fed back to LLM (INJC-06)
- No credential allowlist — all credential-like patterns redacted
- Separate config: injection_defense.output_screening as independent TOML section from input_detection
- Relay detection: heuristic pattern matching (instruction-like patterns in output)
- Escalation: 3 screening failures in a session = escalate to HITL for all subsequent tool calls
- Provider-specific patterns: Anthropic (sk-ant-*), OpenAI (sk-*), AWS (AKIA*), database connection strings
- Prometheus: separate metrics — injection_output_screenings_total (not combined with L1)

### Layer Interaction & Pipeline
- Order: L1 (input detection) → L3 (HMAC boundary validation) → LLM → L4 (output screening) → L5 (HITL)
- Each layer independently enableable/disableable via config
- Cross-layer escalation: L1 flagged context (even below blocking threshold) causes L4/L5 to apply stricter rules
- Each layer acts independently with its own action. No unified verdict score
- Correlation ID: message-level ID flows through L1→L3→L4→L5 for forensic tracing

### Config Structure
- Enabled by default in production. L1 log-not-block, L3 active, L4 active, L5 disabled
- Individual settings only (no preset profiles)
- Startup validation: warn and use defaults for invalid values. Server still starts
- TOML structure: nested sections — [injection_defense.input_detection], [injection_defense.output_screening], [injection_defense.hmac_boundaries], [injection_defense.hitl]
- No hot reload — restart required for config changes
- Global dry_run mode: injection_defense.dry_run = true simulates all layers without action
- Custom regex validated at startup (compile check, reject invalid with warning)
- Global config only — no per-session overrides
- Auto-apply defaults for existing deployments missing [injection_defense] section
- HITL safe_tools list under [injection_defense.hitl]
- Env var overrides for key toggles: BLUFIO_INJECTION_ENABLED, BLUFIO_INJECTION_DRY_RUN
- Documented example config (blufio.example.toml section)

### MCP/WASM Integration
- MCP tool output: higher blocking threshold (>0.98 vs >0.95 for user input)
- WASM skill output: same rules as MCP (sandboxed + signed, semi-trusted)
- No detection results leaked to MCP servers (internal only)
- Separate pipeline stage from existing MCP sanitize module
- MCP tool descriptions scanned at discovery time for injection payloads
- Per-server trust flag: [mcp.servers.my_server] trusted = true skips injection scanning
- No skill quarantine — per-output decision, operators manually manage trust
- Injection defense separate from TrustZoneProvider (different concerns)
- SecurityEvent includes full attribution: source_type (mcp/wasm/user), source_name, server_name

### EventBus & Audit
- SecurityEvent enum with per-layer variants: InputDetection, BoundaryFailure, OutputScreening, HitlPrompt
- Events go to both EventBus (real-time) AND audit trail (persistent)
- Message-level correlation ID across all layers
- Always include full content in security events (not just metadata)

### CLI & Operator Tooling
- Commands: blufio injection test, blufio injection status, blufio injection config
- Test output: full scoring breakdown (patterns, scores, action, layers)
- Status: config + last 10 detection events
- Namespace: blufio injection <subcommand>
- Colorized output with --no-color flag
- --json flag for programmatic output
- blufio doctor includes injection defense summary (active layers, pattern count, HMAC status)
- blufio injection config shows full effective config including defaults

### Crate & Architecture
- New blufio-injection crate (separate from blufio-security)
- Testing: unit tests with attack corpus for L1 + integration tests through agent loop for L4/L5
- Performance: pre-compiled RegexSet for O(1) multi-pattern matching + 10ms timeout per scan

### Claude's Discretion
- Exact regex patterns for each injection category
- HMAC token format details (byte layout, encoding)
- SecurityEvent struct field names and types
- Test attack corpus selection
- Prometheus metric label values
- HKDF derivation parameters

</decisions>

<specifics>
## Specific Ideas

- L1 should follow the same RegexSet pattern as PII detection in blufio-security (proven approach, consistent architecture)
- HMAC key derivation uses HKDF from vault master key — same key management pattern as SQLCipher
- The existing execute_tools() in SessionActor is the natural interception point for L4/L5
- EventBus already has 14 event variants — SecurityEvent becomes the 15th following the same pattern

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `blufio-security::pii` — RegexSet-based pattern matching with scoring. Model for L1 pattern classifier
- `blufio-security::redact` — RedactingWriter with pattern registry. Extend for L4 credential patterns (shared registry)
- `blufio-mcp-client::trust_zone` — TrustZoneProvider for zone classification. Separate from but parallel to injection defense
- `blufio-mcp-client::sanitize` — MCP description sanitization. Separate pipeline stage from injection scanning
- `blufio-bus::events` — BusEvent enum with 14 variants. Add SecurityEvent as 15th
- `blufio-vault` — AES-256-GCM vault with Argon2id KDF. HKDF derivation source for HMAC keys

### Established Patterns
- EventBus: all event enums use String fields to avoid cross-crate dependencies
- Optional<Arc<EventBus>> pattern for test/CLI contexts (None = no events)
- Prometheus metrics: facade pattern (describe_histogram!, describe_counter!)
- Config: figment + TOML with deny_unknown_fields, Option<T> for optional sections
- CLI: clap subcommands with --json flag pattern (used in compaction, audit, etc.)

### Integration Points
- `SessionActor::execute_tools()` (blufio-agent/src/session.rs:822) — L4/L5 interception before tool execution
- `SessionActor::handle_message()` — L1 scanning point for incoming user messages
- Context engine assembly (blufio-context) — L3 HMAC boundary application point
- MCP client tool invocation (blufio-mcp-client) — scan tool results before context injection
- WASM skill invoke (blufio-skill) — scan skill output before context injection
- `blufio/src/serve.rs` — injection defense initialization alongside other subsystems

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 57-prompt-injection-defense*
*Context gathered: 2026-03-12*
