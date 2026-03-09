# Phase 49: FormatPipeline Integration - Context

**Gathered:** 2026-03-09
**Status:** Ready for planning

<domain>
## Phase Boundary

Wire the existing FormatPipeline into all 8 channel adapters' `send()` methods with auto-detection of markdown structures, message splitting at paragraph boundaries, and adapter-specific formatting applied after degradation. Update all adapters to report accurate extended capability fields (CAP-04). Scope is non-streaming `send()` path only — streaming buffer integration is a separate enhancement.

</domain>

<decisions>
## Implementation Decisions

### Message Splitting
- Silent splits at paragraph boundaries — no continuation markers (no "1/3")
- Split priority: double newline > single newline > sentence boundary (. followed by space)
- Code blocks and lists are atomic units — never split inside them
- If a single atomic block exceeds max_message_length, send it anyway (platform truncates gracefully) — log a warning
- 90% threshold of max_message_length to leave room for adapter-specific formatting expansion
- Tables split with header rows repeated in each chunk (consistent with Phase 46 decision)
- Shared `split_at_paragraphs()` utility in blufio-core — IRC keeps its own splitter.rs for PRIVMSG prefix calculations
- Streaming buffer stays independent — different use case (incremental vs batch)
- Returns `Vec<String>` — no metadata per chunk
- `send()` returns MessageId of first chunk only — no API signature change
- No delay between chunks — rate limiting handled by existing adapter logic
- No splitting for channels with max_message_length=None — send as one message
- If chunk N fails to send, abort remaining chunks and return the error
- Comprehensive test matrix: {short, at-limit, over-limit} x {plain, code block, list, table, mixed} x {channel limits}

### Rich Content Detection
- Auto-detect markdown tables, lists, and code blocks in LLM text output
- New `FormatPipeline::detect_and_format(text, caps) -> String` method — single entry point for adapters
- Keep existing `FormatPipeline::format(RichContent, caps)` method for explicit programmatic use
- Skip detection for FullMarkdown channels (Discord, Slack) — markdown renders natively
- Regex-based detection — no pulldown-cmark dependency; consistent with minimal-dependency philosophy
- Conservative detection: tables require header + separator row (|---|), lists require consistent indentation at line start
- Mixed content (text + table + text + code) returns single formatted string, not Vec
- HTML channels (Matrix, Gateway) get HTML table rendering (`<table><tr><td>`) — Tier 0 above existing 3-tier degradation
- Text structures only — no embed detection (embeds come from explicit RichContent::Embed)
- Slack uses mrkdwn text only — no Block Kit in this phase

### Formatting Pipeline Order
- Standardized 4-step pipeline in every adapter's send(): `detect_and_format(text, caps)` → `adapter_escape(formatted)` → `split_at_paragraphs(escaped, limit*0.9)` → send each chunk
- Try rich formatting first, fallback to plain text on API rejection — one-time fallback (rich → plain), not multi-level
- Warn log + `blufio_format_fallback_total{channel}` Prometheus counter on fallback
- Per-adapter formatting functions (not a shared trait) — each platform's escaping rules are unique
- PlainText channels (Signal, WhatsApp, IRC) pass FormatPipeline output as-is — no additional escaping
- Matrix uses semantic HTML (`<table>`, `<ul>`, `<code>`) not `<pre>` wrapping
- Gateway passes through raw text — API clients handle their own rendering
- WhatsApp and Signal stay plain text formatting — no new escaping added this phase
- ChannelMux lets each child adapter handle its own formatting — thin dispatcher, not pre-formatter
- edit_message() uses the same pipeline (detect_and_format → escape → split)

### Capability Verification (CAP-04)
- Trust Phase 46 capability mappings — no re-research against platform docs
- Update any values that were missed or left at defaults during Phase 46

### Claude's Discretion
- Exact regex patterns for table/list/code block detection
- Internal organization of the split_at_paragraphs utility
- Exact Prometheus metric labels and help text for format_fallback counter
- Whether HTML table output includes alignment attributes or basic `<table>` only
- Test file organization and naming

</decisions>

<specifics>
## Specific Ideas

- FormatPipeline::detect_and_format() should be the "one call" that adapters make — they shouldn't need to understand RichContent internals
- Tables should look professional on every channel — HTML tables on Matrix, unicode box on Discord, GFM markdown on Slack, key:value on Signal
- "Conservative detection" — false negatives (missed table) are fine, false positives (broken formatting from misdetection) are not
- Pipeline order is a hard contract: detect_and_format → escape → split → send. Document this in the module doc comment.

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `FormatPipeline` (blufio-core/src/format.rs): existing `format()` method with 3-tier table degradation — extend with `detect_and_format()`
- `ChannelCapabilities` (blufio-core/src/types.rs:144-173): fully extended in Phase 46 with streaming_type, formatting_support, rate_limit, supports_code_blocks
- `markdown::format_for_telegram()` (blufio-telegram): existing MarkdownV2 escaping — keep as adapter-specific function
- IRC `splitter.rs` (blufio-irc/src/splitter.rs): existing PRIVMSG-aware splitter — keep separate from shared utility
- `StreamingEditorOps` (blufio-core/src/streaming.rs): has `max_message_length()` — stays independent

### Established Patterns
- All 8 adapters implement `async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError>` — uniform integration point
- Telegram already does try-MarkdownV2-then-plain-text fallback — standardize this pattern
- All adapters return static `ChannelCapabilities` from `capabilities()` — Phase 46 set all extended fields
- Prometheus metrics use `metrics::counter!()` macro with label pairs — follow for format_fallback counter

### Integration Points
- blufio-core/src/format.rs: add detect_and_format() + split_at_paragraphs()
- Each of 8 adapter crates: update send() and edit_message() to use pipeline
- blufio-prometheus: add blufio_format_fallback_total counter
- blufio-agent/src/channel_mux.rs: ChannelMux delegates to children (no pipeline change needed)

</code_context>

<deferred>
## Deferred Ideas

- Streaming buffer integration with FormatPipeline — needs incremental detection, separate enhancement
- Slack Block Kit structured messages — richer than mrkdwn text but Slack-specific complexity
- WhatsApp bold/italic formatting — WhatsApp supports basic markup but escaping is fragile
- Multi-level formatting degradation (rich → basic markdown → plain) — one-time fallback is sufficient for now

</deferred>

---

*Phase: 49-formatpipeline-integration*
*Context gathered: 2026-03-09*
