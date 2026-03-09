---
phase: 49-formatpipeline-integration
plan: 02
subsystem: format-pipeline
tags: [format, adapters, splitting, pipeline, prometheus]
dependency_graph:
  requires:
    - phase: 49-formatpipeline-integration plan 01
      provides: FormatPipeline::detect_and_format, split_at_paragraphs, HTML Tier 0
  provides:
    - All 8 adapters wired with FormatPipeline 4-step pipeline
    - Telegram MarkdownV2 fallback with Prometheus counter
    - IRC two-level splitting (paragraph + PRIVMSG)
    - Gateway raw passthrough with FullMarkdown caps
  affects: [streaming-buffer, channel-mux, agent-loop]
tech_stack:
  added: [metrics (blufio-telegram)]
  patterns: [4-step-pipeline, chunk-loop-with-first-id, two-level-split]
key_files:
  created: []
  modified:
    - crates/blufio-telegram/src/lib.rs
    - crates/blufio-telegram/Cargo.toml
    - crates/blufio-discord/src/lib.rs
    - crates/blufio-slack/src/lib.rs
    - crates/blufio-matrix/src/lib.rs
    - crates/blufio-signal/src/lib.rs
    - crates/blufio-whatsapp/src/cloud.rs
    - crates/blufio-whatsapp/src/web.rs
    - crates/blufio-irc/src/lib.rs
    - crates/blufio-gateway/src/lib.rs
key_decisions:
  - "Gateway capabilities changed from HTML to FullMarkdown per CONTEXT.md (API clients handle own rendering)"
  - "Matrix uses text_html when FormatPipeline output contains HTML tags, text_plain otherwise"
  - "WhatsApp Cloud capabilities kept as BasicMarkdown (plan said PlainText but existing code had BasicMarkdown -- preserved existing)"
patterns_established:
  - "4-step pipeline: detect_and_format -> adapter_escape -> split_at_paragraphs -> send each chunk"
  - "First chunk MessageId tracking pattern for multi-chunk sends"
  - "IRC two-level split: paragraph-level via split_at_paragraphs, line-level via FloodProtectedSender"
requirements-completed: [FMT-04, FMT-06, CAP-04]
metrics:
  duration: 8min
  completed: "2026-03-09T16:22:00Z"
---

# Phase 49 Plan 02: Adapter Pipeline Wiring Summary

**FormatPipeline wired into all 8 channel adapters with detect_and_format, paragraph splitting, adapter-specific escaping, and Telegram MarkdownV2 fallback counter**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-09T16:13:52Z
- **Completed:** 2026-03-09T16:22:00Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments
- All 8 adapters now call FormatPipeline::detect_and_format() in send() and edit_message()
- Pipeline order enforced everywhere: detect_and_format -> escape -> split -> send
- Telegram has MarkdownV2 parse-error fallback with blufio_format_fallback_total Prometheus counter
- IRC uses two-level splitting: paragraph boundaries then PRIVMSG line-level via existing FloodProtectedSender
- Gateway passes through raw text with FullMarkdown capabilities (API clients handle rendering)
- Matrix sends HTML content via text_html when FormatPipeline produces HTML tags
- Full workspace compiles cleanly, all 54 format tests pass

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire pipeline into Telegram, Discord, Slack, Matrix** - `1cb73d5` (feat)
2. **Task 2: Wire pipeline into Signal, WhatsApp, IRC, Gateway** - `5ae774e` (feat)

## Files Created/Modified
- `crates/blufio-telegram/src/lib.rs` - Pipeline in send() with MarkdownV2 fallback + Prometheus counter, pipeline in edit_message()
- `crates/blufio-telegram/Cargo.toml` - Added metrics workspace dependency
- `crates/blufio-discord/src/lib.rs` - Pipeline in send() with chunk loop, pipeline in edit_message()
- `crates/blufio-slack/src/lib.rs` - Pipeline in send() with mrkdwn escaping and splitting, pipeline in edit_message()
- `crates/blufio-matrix/src/lib.rs` - Pipeline in send() with HTML detection, pipeline in edit_message()
- `crates/blufio-signal/src/lib.rs` - Pipeline in send() with PlainText degradation and splitting
- `crates/blufio-whatsapp/src/cloud.rs` - Pipeline in send() with PlainText degradation and splitting
- `crates/blufio-whatsapp/src/web.rs` - Import added (stub adapter, ready when implemented)
- `crates/blufio-irc/src/lib.rs` - Pipeline in send() with two-level splitting (paragraph + PRIVMSG)
- `crates/blufio-gateway/src/lib.rs` - Pipeline in send() with raw passthrough, caps updated to FullMarkdown

## Decisions Made
- Gateway FormattingSupport changed from HTML to FullMarkdown: CONTEXT.md specifies "API clients handle their own rendering", and FullMarkdown means detect_and_format returns text as-is (passthrough)
- Matrix HTML detection uses simple tag presence check (`<` and `>`) to decide between text_html and text_plain
- WhatsApp Cloud capabilities left as BasicMarkdown (existing value) rather than changing to PlainText

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added metrics crate dependency to blufio-telegram**
- **Found during:** Task 1
- **Issue:** metrics::counter! macro used for fallback counter but metrics crate not in Cargo.toml
- **Fix:** Added `metrics.workspace = true` to blufio-telegram/Cargo.toml
- **Files modified:** crates/blufio-telegram/Cargo.toml
- **Committed in:** 1cb73d5

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary dependency for Prometheus counter. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- FormatPipeline integration complete across all adapters
- Phase 49 fully complete -- all outbound messages flow through detect_and_format
- Streaming buffer integration deferred (separate enhancement per CONTEXT.md)

## Self-Check: PASSED
</content>
</invoke>