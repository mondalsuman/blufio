# Phase 64: Close Integration Wiring Gaps - Context

**Gathered:** 2026-03-13
**Status:** Ready for planning

<domain>
## Phase Boundary

Close 3 low-severity cross-phase integration gaps identified by the v1.5 milestone audit. All three are wiring-only changes that connect existing subsystems — no new features, no new crates, no new user-facing behavior.

</domain>

<decisions>
## Implementation Decisions

### Gap 1: Wire channel_interactive to HITL adapters
- `InjectionPipeline::check_hitl()` accepts `channel_interactive: bool` but callers currently pass a hardcoded value
- The actual interactivity should come from `ChannelCapabilities` on the channel adapter serving the session
- Add `supports_interactive: bool` field to `ChannelCapabilities` (default true for messaging channels like Telegram/Discord/Slack, false for non-interactive channels like email/SMS webhooks)
- Thread the capability through `SessionActor` so `check_hitl` receives the real value

### Gap 2: Share PII patterns with OutputScreener
- `OutputScreener` in `output_screen.rs` maintains its own `CREDENTIAL_PATTERNS` static (6 regex patterns for API keys, DB URIs, bearer tokens)
- `blufio-security/src/pii.rs` has a comprehensive PII detection engine (`detect_pii()`, `PiiMatch`)
- OutputScreener should reuse blufio-security patterns instead of maintaining duplicates
- The OutputScreener's credential-specific patterns (sk-ant-, AKIA, postgres://, Bearer) overlap with but are narrower than full PII — consider exposing a credential-focused subset from blufio-security or having OutputScreener call into blufio-security

### Gap 3: Emit audit event from GDPR erasure CLI
- `gdpr_cmd.rs` performs erasure but doesn't log the erasure action itself as an audit trail entry
- CLI operates outside `serve.rs` lifecycle — no EventBus available
- Open audit.db directly and write audit entry using `AuditWriter` or direct SQL insert
- Audit entry should record: event_type=gdpr_erasure, actor=cli, user_id, timestamp, records affected

### Claude's Discretion
- Whether to add `supports_interactive` as a new field on `ChannelCapabilities` or derive it from existing fields
- Whether OutputScreener should depend on blufio-security directly or use a shared patterns module
- How to handle audit.db path resolution in CLI context (follow existing `open_audit_db` pattern in gdpr_cmd.rs)
- Whether audit write in CLI should be sync (direct rusqlite) or async (tokio-rusqlite)

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ChannelCapabilities` struct (`crates/blufio-core/src/types.rs:148`): Extensible with new bool fields, derives Default
- `blufio-security/src/pii.rs`: `detect_pii()` returns `Vec<PiiMatch>` with pattern names and positions
- `blufio-audit` crate: `AuditWriter` for async writes, `AuditEntry` model, hash-chain logic
- `open_audit_db()` helper in `gdpr_cmd.rs:576`: Already resolves audit.db path from config
- `HitlManager::check_tool()` (`hitl.rs:123`): Already uses `channel_interactive` param for auto-deny on non-interactive channels

### Established Patterns
- Channel adapters implement `capabilities()` returning `ChannelCapabilities` — adding a field propagates naturally
- CLI audit access uses `open_connection_sync` for direct SQL queries (Phase 54 pattern)
- Bus events use String fields to avoid cross-crate deps (all prior phases follow this)
- `GdprEvent` already exists on `BusEvent` with SHA-256 hashed user_id

### Integration Points
- `SessionActor` in `blufio-agent/src/session.rs`: Where channel_interactive feeds into pipeline
- `serve/subsystems.rs`: Where InjectionPipeline is constructed and wired
- `output_screen.rs`: Where CREDENTIAL_PATTERNS needs to be replaced
- `gdpr_cmd.rs::cmd_erase()`: Where audit event emission needs to be added (after erasure, before success summary)

</code_context>

<specifics>
## Specific Ideas

No specific requirements — all three gaps have clear success criteria defined in the milestone audit and ROADMAP.md. Standard implementation following existing patterns.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 64-integration-wiring-fixes*
*Context gathered: 2026-03-13*
