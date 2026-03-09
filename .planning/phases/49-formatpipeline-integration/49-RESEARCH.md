# Phase 49: FormatPipeline Integration - Research

**Researched:** 2026-03-09
**Domain:** Content formatting pipeline integration across 8 channel adapters (Rust)
**Confidence:** HIGH

## Summary

Phase 49 wires the existing `FormatPipeline` (blufio-core/src/format.rs) into all 8 channel adapters' `send()` and `edit_message()` methods. The current state is: FormatPipeline exists with `format(RichContent, ChannelCapabilities) -> FormattedOutput` and 3-tier table degradation. Each adapter already has its own formatting function (Telegram MarkdownV2, Discord pass-through, Slack mrkdwn conversion). The streaming module already has `split_at_paragraph_boundary()` for streaming use. The task is to add (1) a `detect_and_format()` method for auto-detecting markdown structures in LLM text, (2) a `split_at_paragraphs()` utility that returns `Vec<String>` for batch splitting, and (3) integrate the 4-step pipeline (detect_and_format -> adapter_escape -> split -> send) into every adapter.

This phase touches 10 crates (blufio-core for new methods, 8 adapter crates for integration, blufio-prometheus for the fallback counter) with a total of 16 files modified (8 lib.rs + 8 tests or existing markdown/handler files). The architecture is well-constrained by CONTEXT.md decisions -- the main technical challenges are (a) conservative regex-based detection of tables/lists/code blocks, (b) correct splitting that respects atomic blocks, and (c) proper pipeline ordering in each adapter.

**Primary recommendation:** Implement in 3 waves: Wave 1 adds detect_and_format + split_at_paragraphs to blufio-core with comprehensive tests; Wave 2 integrates into all 8 adapters; Wave 3 adds the Prometheus fallback counter and verifies CAP-04 accuracy.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Silent splits at paragraph boundaries -- no continuation markers (no "1/3")
- Split priority: double newline > single newline > sentence boundary (. followed by space)
- Code blocks and lists are atomic units -- never split inside them
- If a single atomic block exceeds max_message_length, send it anyway (platform truncates gracefully) -- log a warning
- 90% threshold of max_message_length to leave room for adapter-specific formatting expansion
- Tables split with header rows repeated in each chunk (consistent with Phase 46 decision)
- Shared `split_at_paragraphs()` utility in blufio-core -- IRC keeps its own splitter.rs for PRIVMSG prefix calculations
- Streaming buffer stays independent -- different use case (incremental vs batch)
- Returns `Vec<String>` -- no metadata per chunk
- `send()` returns MessageId of first chunk only -- no API signature change
- No delay between chunks -- rate limiting handled by existing adapter logic
- No splitting for channels with max_message_length=None -- send as one message
- If chunk N fails to send, abort remaining chunks and return the error
- Comprehensive test matrix: {short, at-limit, over-limit} x {plain, code block, list, table, mixed} x {channel limits}
- Auto-detect markdown tables, lists, and code blocks in LLM text output
- New `FormatPipeline::detect_and_format(text, caps) -> String` method -- single entry point for adapters
- Keep existing `FormatPipeline::format(RichContent, caps)` method for explicit programmatic use
- Skip detection for FullMarkdown channels (Discord, Slack) -- markdown renders natively
- Regex-based detection -- no pulldown-cmark dependency; consistent with minimal-dependency philosophy
- Conservative detection: tables require header + separator row (|---|), lists require consistent indentation at line start
- Mixed content (text + table + text + code) returns single formatted string, not Vec
- HTML channels (Matrix, Gateway) get HTML table rendering (`<table><tr><td>`) -- Tier 0 above existing 3-tier degradation
- Text structures only -- no embed detection (embeds come from explicit RichContent::Embed)
- Slack uses mrkdwn text only -- no Block Kit in this phase
- Standardized 4-step pipeline in every adapter's send(): `detect_and_format(text, caps)` -> `adapter_escape(formatted)` -> `split_at_paragraphs(escaped, limit*0.9)` -> send each chunk
- Try rich formatting first, fallback to plain text on API rejection -- one-time fallback (rich -> plain), not multi-level
- Warn log + `blufio_format_fallback_total{channel}` Prometheus counter on fallback
- Per-adapter formatting functions (not a shared trait) -- each platform's escaping rules are unique
- PlainText channels (Signal, WhatsApp, IRC) pass FormatPipeline output as-is -- no additional escaping
- Matrix uses semantic HTML (`<table>`, `<ul>`, `<code>`) not `<pre>` wrapping
- Gateway passes through raw text -- API clients handle their own rendering
- WhatsApp and Signal stay plain text formatting -- no new escaping added this phase
- ChannelMux lets each child adapter handle its own formatting -- thin dispatcher, not pre-formatter
- edit_message() uses the same pipeline (detect_and_format -> escape -> split)
- Trust Phase 46 capability mappings -- no re-research against platform docs
- Update any values that were missed or left at defaults during Phase 46

### Claude's Discretion
- Exact regex patterns for table/list/code block detection
- Internal organization of the split_at_paragraphs utility
- Exact Prometheus metric labels and help text for format_fallback counter
- Whether HTML table output includes alignment attributes or basic `<table>` only
- Test file organization and naming

### Deferred Ideas (OUT OF SCOPE)
- Streaming buffer integration with FormatPipeline -- needs incremental detection, separate enhancement
- Slack Block Kit structured messages -- richer than mrkdwn text but Slack-specific complexity
- WhatsApp bold/italic formatting -- WhatsApp supports basic markup but escaping is fragile
- Multi-level formatting degradation (rich -> basic markdown -> plain) -- one-time fallback is sufficient for now
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| FMT-04 | FormatPipeline called inside each channel adapter's `send()` method | New `detect_and_format()` method on FormatPipeline, integrated into 8 adapters' send() and edit_message() paths |
| FMT-05 | Message length splitting integrated -- content split at paragraph boundaries respecting `max_message_length` | New `split_at_paragraphs()` function in blufio-core, using 90% threshold, atomic block awareness, Vec<String> return |
| FMT-06 | Adapter-specific formatting (MarkdownV2, mrkdwn, etc.) applied after FormatPipeline degradation | 4-step pipeline ordering: detect_and_format -> escape -> split -> send; existing escape functions preserved |
| CAP-04 | All 8 channel adapters updated to report extended capability fields accurately | All 8 adapters already have extended fields from Phase 46; verification pass to confirm accuracy |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| blufio-core | workspace | FormatPipeline, ChannelCapabilities, split_at_paragraphs | Central crate for shared formatting logic |
| regex | 1.x | Markdown structure detection (tables, lists, code blocks) | Already in Cargo.lock via blufio-slack; no new dependency |
| metrics | workspace | Prometheus counter for format fallback tracking | Already used throughout project for all metrics |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tracing | workspace | Warn logging on fallback and oversized atomic blocks | Already imported in every adapter |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| regex detection | pulldown-cmark | Full parser but adds ~50KB, user explicitly rejected; regex sufficient for conservative detection |
| shared trait for escaping | per-adapter functions | User explicitly chose per-adapter functions; platform escaping rules are too different to unify |

**Installation:**
```bash
# regex only needed in blufio-core Cargo.toml -- already in workspace lockfile
# No new external dependencies required
```

## Architecture Patterns

### Recommended Project Structure
```
crates/blufio-core/src/
  format.rs           # Add detect_and_format(), html_table(), split_at_paragraphs()
  streaming.rs        # UNCHANGED -- streaming buffer stays independent

crates/blufio-telegram/src/
  lib.rs              # Update send() and edit_message() with 4-step pipeline
  markdown.rs         # UNCHANGED -- format_for_telegram() already correct

crates/blufio-discord/src/
  lib.rs              # Update send() and edit_message()
  markdown.rs         # UNCHANGED -- format_for_discord() pass-through

crates/blufio-slack/src/
  lib.rs              # Update send() and edit_message()
  markdown.rs         # UNCHANGED -- markdown_to_mrkdwn() already correct

crates/blufio-matrix/src/
  lib.rs              # Update send() and edit_message() -- use HTML formatting

crates/blufio-irc/src/
  lib.rs              # Update send() -- IRC keeps its own splitter.rs
  splitter.rs         # UNCHANGED -- PRIVMSG prefix calculations are IRC-specific

crates/blufio-signal/src/
  lib.rs              # Update send() -- plain text pass-through

crates/blufio-whatsapp/src/
  cloud.rs            # Update send() -- plain text pass-through

crates/blufio-gateway/src/
  lib.rs              # Update send() -- raw text pass-through (no formatting)

crates/blufio-prometheus/src/
  recording.rs        # Add record_format_fallback() + describe counter
```

### Pattern 1: FormatPipeline::detect_and_format()
**What:** New static method that auto-detects markdown structures in plain text and formats them based on channel capabilities.
**When to use:** Every adapter's send() and edit_message() call this as step 1.
**Example:**
```rust
// Source: blufio-core/src/format.rs (new code)
impl FormatPipeline {
    /// Auto-detect markdown structures in text and format for the given channel.
    ///
    /// For FullMarkdown channels (Discord, Slack): returns text as-is (markdown renders natively).
    /// For HTML channels (Matrix, Gateway): converts detected tables to <table> HTML.
    /// For PlainText/BasicMarkdown channels: degrades tables to key:value, code blocks to plain.
    ///
    /// This is the primary entry point for adapters -- they call this instead of
    /// needing to understand RichContent internals.
    pub fn detect_and_format(text: &str, caps: &ChannelCapabilities) -> String {
        // FullMarkdown channels render markdown natively -- skip detection
        if caps.formatting_support == FormattingSupport::FullMarkdown {
            return text.to_string();
        }
        // ... detect tables, lists, code blocks via regex, format each segment
    }
}
```

### Pattern 2: split_at_paragraphs() Batch Splitter
**What:** A function that splits formatted text into `Vec<String>` chunks respecting paragraph boundaries and atomic blocks.
**When to use:** Step 3 of the pipeline, after detect_and_format and adapter escaping.
**Example:**
```rust
// Source: blufio-core/src/format.rs (new code)
/// Split text into chunks that fit within `max_len`, respecting paragraph boundaries.
///
/// Split priority: double newline > single newline > sentence boundary (. + space) > hard split.
/// Code blocks, lists, and tables are atomic -- never split inside them.
/// If an atomic block exceeds max_len, include it as a single oversized chunk and log a warning.
/// Returns Vec<String> -- empty vec if text is empty.
pub fn split_at_paragraphs(text: &str, max_len: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }
    if text.len() <= max_len {
        return vec![text.to_string()];
    }
    // ... split logic respecting atomic blocks
}
```

### Pattern 3: 4-Step Pipeline in Adapter send()
**What:** Standardized pipeline ordering in every adapter's send() method.
**When to use:** Every outbound message delivery.
**Example:**
```rust
// Source: adapter lib.rs (e.g., blufio-telegram)
async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
    let caps = self.capabilities();
    let chat_id = extract_chat_id(&msg)?;

    // Step 1: Detect structures and format for channel capabilities
    let formatted = FormatPipeline::detect_and_format(&msg.content, &caps);

    // Step 2: Adapter-specific escaping (AFTER FormatPipeline degradation)
    let escaped = markdown::format_for_telegram(&formatted);

    // Step 3: Split at paragraph boundaries (90% of max_message_length)
    let limit = caps.max_message_length.map(|l| (l as f64 * 0.9) as usize);
    let chunks = if let Some(limit) = limit {
        split_at_paragraphs(&escaped, limit)
    } else {
        vec![escaped]
    };

    // Step 4: Send each chunk, abort on failure
    let mut first_msg_id = None;
    for chunk in &chunks {
        let result = self.try_send_formatted(chat_id, chunk).await;
        match result {
            Ok(msg_id) => {
                if first_msg_id.is_none() {
                    first_msg_id = Some(msg_id);
                }
            }
            Err(e) => {
                // One-time fallback: try plain text
                if first_msg_id.is_none() {
                    let plain_chunks = if let Some(limit) = limit {
                        split_at_paragraphs(&msg.content, limit)
                    } else {
                        vec![msg.content.clone()]
                    };
                    // ... send plain, increment fallback counter
                }
                return Err(e);
            }
        }
    }

    Ok(first_msg_id.unwrap_or_else(|| MessageId("empty".to_string())))
}
```

### Pattern 4: Fallback with Prometheus Counter
**What:** One-time rich -> plain fallback when API rejects formatted text.
**When to use:** Telegram (MarkdownV2 -> plain), any adapter where formatting could fail.
**Example:**
```rust
// Existing pattern in Telegram's send() -- standardize across adapters
match try_send_markdown(chat_id, &escaped).await {
    Ok(sent) => Ok(sent),
    Err(_e) => {
        warn!(error = %_e, "formatted send failed, falling back to plain text");
        metrics::counter!(
            "blufio_format_fallback_total",
            "channel" => "telegram"
        ).increment(1);
        try_send_plain(chat_id, &msg.content).await
    }
}
```

### Anti-Patterns to Avoid
- **Pre-formatting in ChannelMux:** ChannelMux MUST NOT apply any formatting. It delegates to child adapters whose send() handles the pipeline. The mux is a thin dispatcher only.
- **Splitting before escaping:** Escaping can expand text (e.g., Telegram escapes `.` to `\.`). The pipeline order MUST be detect -> escape -> split to account for expansion.
- **Splitting inside code blocks:** Code fences (```) are atomic. The splitter must track open/close fences and never split between them.
- **Using streaming's `split_at_paragraph_boundary` for batch:** The streaming function splits at a single point (returns 2 slices). The batch splitter needs to produce N chunks. Reuse the boundary-finding logic but wrap in a loop.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Table detection | Custom char-by-char parser | Regex `\|[^\n]+\|\n\|[\s:]*-+` | Conservative detection matches header+separator pattern; false negatives acceptable |
| Code block detection | Paired backtick tracking | Regex `(?s)` `` ```[^\n]*\n.*?``` `` | Slack adapter already uses this exact pattern; proven correct |
| Markdown-to-mrkdwn | New converter | Existing `markdown::markdown_to_mrkdwn()` | Already works correctly in Slack adapter |
| MarkdownV2 escaping | New escaper | Existing `markdown::escape_markdown_v2()` | Already handles code block preservation |
| IRC message splitting | Shared splitter | Existing `splitter::split_message()` | Has PRIVMSG prefix overhead calculation |
| HTML generation | Template engine | `format!("<table>...</table>")` inline | Only tables/lists need HTML; no library needed |

**Key insight:** All adapter-specific formatting functions already exist. The new work is (a) the detection layer, (b) the batch splitter, and (c) wiring them together in the correct order.

## Common Pitfalls

### Pitfall 1: Splitting After Escape Causes Incorrect Boundaries
**What goes wrong:** If you split BEFORE Telegram escaping, the split point is calculated on unescaped text. Escaping then expands characters (`\.` is 2 chars, not 1), pushing chunks over the limit.
**Why it happens:** The 4-step pipeline order seems counterintuitive -- you'd expect to "process" text (detect -> split) then "render" (escape -> send).
**How to avoid:** Pipeline order is: detect_and_format -> escape -> split -> send. This is a hard contract documented in the module doc comment.
**Warning signs:** Telegram API rejecting messages as "too long" despite splitting at 90% threshold.

### Pitfall 2: Regex Greediness in Table Detection
**What goes wrong:** A greedy table regex matches too much text, turning non-table content into tables.
**Why it happens:** GFM tables with multiple rows can span many lines; greedy `.*` consumes too far.
**How to avoid:** Use conservative detection: require `|` at start/end of line + separator row with `---`. Match row-by-row, not the entire table as one regex group.
**Warning signs:** Normal text with `|` characters being interpreted as table rows.

### Pitfall 3: Atomic Block Exceeding max_message_length
**What goes wrong:** A single code block (e.g., LLM outputs a 5000-char code sample) exceeds the 90% threshold but cannot be split.
**Why it happens:** Code blocks are atomic -- splitting them would break the fence markers.
**How to avoid:** User decision: send it anyway as a single oversized chunk, log a warning. Platform will truncate gracefully. This is a pragmatic choice -- the alternative (splitting mid-code) is worse.
**Warning signs:** `warn!` log entries about oversized atomic blocks.

### Pitfall 4: FullMarkdown Skip Logic
**What goes wrong:** Discord/Slack messages get double-processed -- detect_and_format converts tables to key:value, then mrkdwn converts bold markers.
**Why it happens:** Forgetting that FullMarkdown channels should skip detection entirely.
**How to avoid:** First check in detect_and_format: if `formatting_support == FullMarkdown`, return text as-is. Discord and Slack render GFM tables natively.
**Warning signs:** Discord showing key:value pairs instead of tables; Slack showing mangled formatting.

### Pitfall 5: Gateway Send Doesn't Need Pipeline
**What goes wrong:** Gateway responses get formatted/escaped when API clients expect raw text.
**Why it happens:** Applying the pipeline uniformly to all 8 adapters without considering Gateway's pass-through nature.
**How to avoid:** Gateway's send() passes `msg.content` directly -- no detect_and_format, no escaping, no splitting (max_message_length is None). The Gateway context from CONTEXT.md explicitly says "Gateway passes through raw text."
**Warning signs:** API clients receiving escaped markdown or truncated responses.

### Pitfall 6: edit_message() vs send() Asymmetry
**What goes wrong:** edit_message() doesn't go through the pipeline, so edited messages have different formatting than original sends.
**Why it happens:** edit_message() is often forgotten when adding new send() logic.
**How to avoid:** CONTEXT.md decision: "edit_message() uses the same pipeline." Apply detect_and_format -> escape -> split in edit_message() too. For edit_message(), only the first chunk is used (edits replace a single message).
**Warning signs:** Inconsistent formatting between initial send and subsequent edits.

## Code Examples

### Detect and Format Implementation
```rust
// Source: blufio-core/src/format.rs (new code, verified against existing patterns)

impl FormatPipeline {
    /// Auto-detect markdown structures in text and format for channel capabilities.
    pub fn detect_and_format(text: &str, caps: &ChannelCapabilities) -> String {
        if text.is_empty() {
            return String::new();
        }

        // FullMarkdown channels render GFM natively -- skip detection
        if caps.formatting_support == FormattingSupport::FullMarkdown {
            return text.to_string();
        }

        // Split text into segments: plain text, code blocks, tables, lists
        let segments = detect_segments(text);

        let mut output = String::new();
        for segment in segments {
            match segment {
                Segment::Text(t) => output.push_str(&t),
                Segment::CodeBlock(code) => {
                    // Format based on capabilities
                    if caps.supports_code_blocks {
                        output.push_str(&code); // Already fenced
                    } else {
                        // Strip fences for plain text channels
                        let inner = strip_code_fences(&code);
                        output.push_str(&inner);
                    }
                }
                Segment::Table(table_text) => {
                    if caps.formatting_support == FormattingSupport::HTML {
                        output.push_str(&text_table_to_html(&table_text));
                    } else if let Some(table) = parse_gfm_table(&table_text) {
                        let formatted = Self::format(&RichContent::Table(table), caps);
                        if let FormattedOutput::Text(t) = formatted {
                            output.push_str(&t);
                        }
                    } else {
                        output.push_str(&table_text); // Couldn't parse, keep as-is
                    }
                }
                Segment::List(list_text) => {
                    output.push_str(&list_text); // Lists render fine as-is on all channels
                }
            }
        }

        output
    }
}
```

### Split at Paragraphs Implementation
```rust
// Source: blufio-core/src/format.rs (new code)

/// Split text into chunks that fit within `max_len`, respecting atomic blocks.
pub fn split_at_paragraphs(text: &str, max_len: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }
    if max_len == 0 {
        return vec![text.to_string()];
    }
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max_len {
            chunks.push(remaining.to_string());
            break;
        }

        // Find the best split point within max_len
        let search_region = &remaining[..max_len.min(remaining.len())];

        // Check for atomic blocks (code fences, tables) that shouldn't be split
        // If we're inside one, extend the chunk to include the whole block
        let split_pos = find_split_point(search_region, remaining, max_len);

        let (chunk, rest) = remaining.split_at(split_pos);
        let chunk = chunk.trim_end().to_string();
        let rest = rest.trim_start();

        if !chunk.is_empty() {
            chunks.push(chunk);
        }
        remaining = rest;
    }

    chunks
}
```

### Adapter Integration (Telegram Example)
```rust
// Source: blufio-telegram/src/lib.rs send() method (modified)
async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
    let chat_id = extract_chat_id(&msg)?;
    let caps = self.capabilities();

    // Step 1: Detect and format
    let formatted = FormatPipeline::detect_and_format(&msg.content, &caps);

    // Step 2: Adapter-specific escaping
    let escaped = markdown::format_for_telegram(&formatted);

    // Step 3: Split at paragraph boundaries (90% threshold)
    let chunks = if let Some(max_len) = caps.max_message_length {
        split_at_paragraphs(&escaped, (max_len as f64 * 0.9) as usize)
    } else {
        vec![escaped.clone()]
    };

    // Step 4: Send each chunk
    let mut first_id = None;
    for chunk in &chunks {
        match self.bot
            .send_message(Recipient::Id(chat_id), chunk)
            .parse_mode(ParseMode::MarkdownV2)
            .await
        {
            Ok(sent) => {
                if first_id.is_none() {
                    first_id = Some(MessageId(sent.id.0.to_string()));
                }
            }
            Err(e) if first_id.is_none() => {
                // Fallback to plain text for first chunk
                warn!(error = %e, "MarkdownV2 failed, falling back to plain text");
                metrics::counter!("blufio_format_fallback_total", "channel" => "telegram")
                    .increment(1);
                // Resplit using original (unescaped) content
                let plain_chunks = if let Some(max_len) = caps.max_message_length {
                    split_at_paragraphs(&msg.content, (max_len as f64 * 0.9) as usize)
                } else {
                    vec![msg.content.clone()]
                };
                for plain_chunk in &plain_chunks {
                    let sent = self.bot
                        .send_message(Recipient::Id(chat_id), plain_chunk)
                        .await
                        .map_err(|e| BlufioError::channel_delivery_failed("telegram", e))?;
                    if first_id.is_none() {
                        first_id = Some(MessageId(sent.id.0.to_string()));
                    }
                }
                return Ok(first_id.unwrap());
            }
            Err(e) => {
                // Later chunk failed -- abort remaining
                return Err(BlufioError::channel_delivery_failed("telegram", e));
            }
        }
    }

    Ok(first_id.unwrap_or_else(|| MessageId("empty".to_string())))
}
```

### HTML Table Generation (for Matrix/Gateway)
```rust
// Source: blufio-core/src/format.rs (new code)

/// Convert a GFM table text block to semantic HTML.
fn text_table_to_html(table_text: &str) -> String {
    let lines: Vec<&str> = table_text.lines().collect();
    if lines.len() < 2 {
        return table_text.to_string();
    }

    let mut html = String::from("<table>\n<thead><tr>");
    // Parse header row
    let headers: Vec<&str> = lines[0].split('|')
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim())
        .collect();
    for h in &headers {
        html.push_str(&format!("<th>{}</th>", h));
    }
    html.push_str("</tr></thead>\n<tbody>\n");

    // Skip separator row (line 1), parse data rows
    for line in &lines[2..] {
        let cells: Vec<&str> = line.split('|')
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim())
            .collect();
        html.push_str("<tr>");
        for cell in &cells {
            html.push_str(&format!("<td>{}</td>", cell));
        }
        html.push_str("</tr>\n");
    }

    html.push_str("</tbody>\n</table>");
    html
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Adapters send raw `msg.content` | FormatPipeline detects structures and formats | Phase 49 | Tables render professionally on every channel |
| No message splitting in adapters | split_at_paragraphs() in every adapter | Phase 49 | Long LLM responses never truncated |
| Telegram escaping applied to raw text | Escaping applied after FormatPipeline degradation | Phase 49 | Pipeline ordering prevents double-formatting |
| FormatPipeline only programmatic API | detect_and_format() auto-detection API added | Phase 49 | Adapters don't need to understand RichContent |

**Existing code preserved (not deprecated):**
- `FormatPipeline::format(RichContent, caps)` -- still available for explicit programmatic use
- `split_at_paragraph_boundary()` in streaming.rs -- still used by StreamingBuffer for incremental splits
- IRC `splitter.rs` -- still used by FloodProtectedSender for PRIVMSG-aware splitting
- All adapter markdown modules -- still called as step 2 of the pipeline

## Open Questions

1. **Table detection: header alignment row strictness**
   - What we know: GFM tables require `|---|` separator row. LLMs sometimes produce sloppy formatting.
   - What's unclear: How strictly to match alignment markers (`:---:` vs `---` vs `----`).
   - Recommendation: Accept any combination of `-`, `:`, `|`, and spaces in the separator row. The regex `^\|[\s:]*-+[\s:|-]*\|` should cover all valid GFM alignment rows.

2. **HTML table alignment attributes**
   - What we know: CONTEXT.md says HTML channels get `<table>` rendering. Claude has discretion on alignment attributes.
   - What's unclear: Whether to include `style="text-align:right"` on `<td>` elements.
   - Recommendation: Basic `<table>` only (no alignment attributes). Keep it simple; alignment is a nice-to-have that adds complexity.

3. **edit_message() splitting behavior**
   - What we know: edit_message replaces a single message. Splitting returns multiple chunks.
   - What's unclear: If edit produces multiple chunks, should we only use the first chunk?
   - Recommendation: For edit_message(), run the full pipeline but only use the first chunk. If the content expanded beyond one message, log a warning. Edits are typically for streaming updates where content grows incrementally.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in Rust test framework) |
| Config file | Cargo.toml `[dev-dependencies]` in each crate |
| Quick run command | `cargo test -p blufio-core --lib format` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| FMT-04 | detect_and_format returns correct output per channel type | unit | `cargo test -p blufio-core --lib format::tests::detect` | Wave 0 |
| FMT-04 | Each adapter calls detect_and_format in send() | integration | `cargo test -p blufio-telegram --lib tests` | Existing (modify) |
| FMT-05 | split_at_paragraphs splits correctly | unit | `cargo test -p blufio-core --lib format::tests::split` | Wave 0 |
| FMT-05 | Atomic blocks (code, tables) not split | unit | `cargo test -p blufio-core --lib format::tests::split_atomic` | Wave 0 |
| FMT-05 | Oversized atomic block sent as single chunk with warning | unit | `cargo test -p blufio-core --lib format::tests::split_oversized` | Wave 0 |
| FMT-06 | Pipeline ordering: detect -> escape -> split | unit | `cargo test -p blufio-core --lib format::tests::pipeline_order` | Wave 0 |
| FMT-06 | Fallback from rich to plain increments counter | unit | Manual inspection (requires live API) | Manual |
| CAP-04 | All 8 adapters report accurate extended fields | unit | `cargo test --workspace -- capabilities` | Existing (verify) |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-core --lib format`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `blufio-core/src/format.rs` tests for `detect_and_format()` -- new test functions needed
- [ ] `blufio-core/src/format.rs` tests for `split_at_paragraphs()` -- new test functions needed
- [ ] `blufio-core/src/format.rs` tests for HTML table generation -- new test functions needed
- [ ] Comprehensive test matrix: {short, at-limit, over-limit} x {plain, code block, list, table, mixed} x {channel limits} -- as specified in CONTEXT.md

## Adapter Integration Summary

This section provides a quick reference for each adapter's integration specifics:

| Adapter | formatting_support | escape fn | detect_and_format? | split? | fallback? | max_len |
|---------|-------------------|-----------|--------------------|---------|-----------|---------|
| Telegram | BasicMarkdown | `format_for_telegram()` | Yes | Yes (90% of 4096) | MarkdownV2 -> plain | 4096 |
| Discord | FullMarkdown | `format_for_discord()` (pass-through) | **Skip** (native MD) | Yes (90% of 2000) | No (native MD) | 2000 |
| Slack | FullMarkdown | `markdown_to_mrkdwn()` | **Skip** (native MD) | Yes (90% of 40000) | No (native mrkdwn) | 40000 |
| Matrix | HTML | None (use HTML output) | Yes (HTML tier) | Yes (90% of 65536) | HTML -> plain | 65536 |
| IRC | PlainText | None (pass-through) | Yes | **Own splitter** | No (already plain) | 450 |
| Signal | PlainText | None (pass-through) | Yes | Yes (90% of 4096) | No (already plain) | 4096 |
| WhatsApp | BasicMarkdown | None (plain text this phase) | Yes | Yes (90% of 4096) | No (already plain) | 4096 |
| Gateway | HTML | None (raw pass-through) | **Skip** (raw for API) | **No** (max_len=None) | No (raw text) | None |

Key notes:
- **Discord/Slack skip detect_and_format** because they have FullMarkdown support -- GFM tables render natively.
- **Gateway skips everything** -- max_message_length is None and it passes raw text to API clients.
- **IRC uses its own splitter** for PRIVMSG prefix calculations but still calls detect_and_format for table degradation.
- **Matrix gets HTML table output** via the new Tier 0 HTML rendering in detect_and_format.

## Sources

### Primary (HIGH confidence)
- blufio-core/src/format.rs -- Full FormatPipeline implementation, 3-tier table degradation, RichContent types
- blufio-core/src/types.rs -- ChannelCapabilities with all extended fields (lines 144-173)
- blufio-core/src/streaming.rs -- Existing split_at_paragraph_boundary() function and StreamingBuffer
- All 8 adapter lib.rs files -- Current send(), edit_message(), capabilities() implementations
- blufio-telegram/src/markdown.rs -- MarkdownV2 escaping (escape_markdown_v2)
- blufio-slack/src/markdown.rs -- mrkdwn conversion (markdown_to_mrkdwn)
- blufio-discord/src/markdown.rs -- Pass-through formatting (format_for_discord)
- blufio-irc/src/splitter.rs -- IRC PRIVMSG-aware splitting (split_message)
- blufio-agent/src/channel_mux.rs -- ChannelMultiplexer send() routing (delegates to child)
- blufio-prometheus/src/recording.rs -- Metric recording patterns (counter!, gauge!, describe_counter!)
- .planning/phases/49-formatpipeline-integration/49-CONTEXT.md -- All user decisions

### Secondary (MEDIUM confidence)
- Telegram Bot API documentation -- MarkdownV2 parse mode, 4096 char limit
- Discord API documentation -- 2000 char message limit, native markdown support
- Slack API documentation -- 40000 char limit, mrkdwn syntax

### Tertiary (LOW confidence)
- None -- all findings verified from source code

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already exist in workspace, no new dependencies
- Architecture: HIGH -- all integration points examined, existing code patterns well understood
- Pitfalls: HIGH -- based on direct analysis of existing code and user decisions in CONTEXT.md
- Validation: HIGH -- existing test framework and patterns established in prior phases

**Research date:** 2026-03-09
**Valid until:** 2026-04-09 (stable -- all decisions locked in CONTEXT.md, codebase is internal)
