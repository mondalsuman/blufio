---
phase: 49-formatpipeline-integration
plan: 01
subsystem: format-pipeline
tags: [format, detection, splitting, html]
dependency_graph:
  requires: []
  provides: [FormatPipeline::detect_and_format, split_at_paragraphs, HTML-Tier-0]
  affects: [all-adapters-send, matrix-html, gateway-html]
tech_stack:
  added: []
  patterns: [segment-detection, 4-tier-degradation, atomic-block-splitting]
key_files:
  created: []
  modified:
    - crates/blufio-core/src/format.rs
decisions:
  - Conservative regex detection over pulldown-cmark dependency
  - HTML Tier 0 placed before Tier 1 in format_table dispatch
  - split_at_paragraphs as free function not on FormatPipeline
  - Sentence boundary split uses greedy packing algorithm
metrics:
  duration: 8min
  completed: "2026-03-09T16:13:00Z"
---

# Phase 49 Plan 01: FormatPipeline Core Utilities Summary

Extended FormatPipeline with detect_and_format() auto-detection entry point, split_at_paragraphs() paragraph-boundary splitter, and HTML Tier 0 semantic rendering for tables/lists/code blocks.

## What Was Done

### Task 1: detect_and_format() + HTML Tier 0

Added `FormatPipeline::detect_and_format(text, caps) -> String` as the single entry point for adapters. Conservative regex-based detection identifies markdown tables (requires `|---|` separator), lists (`- ` or `N. ` prefix), and code blocks (triple backtick fences). FullMarkdown channels skip detection entirely since markdown renders natively. Mixed content (text + table + code + list) processes segments sequentially and concatenates into a single String.

Added HTML Tier 0 to the table degradation pipeline: when `formatting_support == HTML`, tables render as `<table><thead><tr><th>` / `<tbody><tr><td>` with semantic elements. Lists render as `<ul><li>` / `<ol><li>`. Code blocks render as `<pre><code class="language-X">`. All HTML output escapes `&`, `<`, `>`.

### Task 2: split_at_paragraphs()

Added `pub fn split_at_paragraphs(text, max_length) -> Vec<String>` as a free public function. Uses 90% of max_length as threshold. Split priority: double newline > single newline > sentence boundary (`. ` followed by uppercase). Code blocks, lists, and tables are atomic units that are never split internally. Tables exceeding the limit get split with header rows repeated in each chunk. Oversized atomic blocks emit a tracing::warn and are sent as their own chunk.

## Commits

| Task | Commit | Description |
|------|--------|-------------|
| 1+2 | 3b0cb82 | detect_and_format(), split_at_paragraphs(), HTML Tier 0 |

## Test Results

54 tests pass covering:
- Plain text passthrough, FullMarkdown skip, mixed content detection
- Table detection (with/without separator, pipe-in-text false positive avoidance)
- List detection (bullet/ordered, numbered-text false positive avoidance)
- Code block detection and preservation
- HTML Tier 0: tables, lists, code blocks, HTML escaping, empty tables
- split_at_paragraphs: short/none/empty, double-newline/single-newline/sentence splits
- Atomic blocks: code block, list, table atomicity
- Table header repetition on split
- 90% threshold verification

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Sentence boundary split logic**
- **Found during:** Task 2
- **Issue:** Initial sentence split only triggered when accumulated text exceeded limit at a boundary, not when total remaining text would exceed limit
- **Fix:** Added post-loop check for remaining text exceeding limit with available boundary
- **Files modified:** crates/blufio-core/src/format.rs

### Notes

- Tasks 1 and 2 were committed together since they share the same file and the tests are interdependent
- format_test.rs mentioned in plan frontmatter was not needed -- tests are inline in format.rs per existing convention

## Self-Check: PASSED
