// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Centralized content formatting and degradation pipeline.
//!
//! The [`FormatPipeline`] takes [`RichContent`] and degrades it based on
//! [`ChannelCapabilities`], producing [`FormattedOutput`] suitable for
//! channel-specific rendering.
//!
//! ## Table Degradation Tiers
//!
//! Tables degrade through three tiers based on channel capabilities:
//!
//! - **Tier 1** (`supports_code_blocks=true`): Unicode box-drawing table
//!   wrapped in a triple-backtick code fence.
//! - **Tier 2** (`formatting_support=FullMarkdown`, `supports_code_blocks=false`):
//!   GitHub-Flavored Markdown table with alignment markers.
//! - **Tier 3** (`PlainText` or `BasicMarkdown`): Key:value pairs per row.
//!
//! Empty tables render headers followed by "(no data)".

use serde::{Deserialize, Serialize};

use crate::types::{ChannelCapabilities, FormattingSupport};

// ---------------------------------------------------------------------------
// Regex-based markdown detection helpers (conservative)
// ---------------------------------------------------------------------------

/// Detect whether a line is a markdown table separator (e.g. `|---|---|`).
fn is_table_separator(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
        return false;
    }
    // Interior must be only dashes, colons, pipes, and spaces
    let inner = &trimmed[1..trimmed.len() - 1];
    if inner.is_empty() {
        return false;
    }
    // Each cell between pipes must match :?-+:?
    inner.split('|').all(|cell| {
        let c = cell.trim();
        if c.is_empty() {
            return false;
        }
        let bytes = c.as_bytes();
        let start = if bytes[0] == b':' { 1 } else { 0 };
        let end = if bytes[bytes.len() - 1] == b':' {
            bytes.len() - 1
        } else {
            bytes.len()
        };
        if start >= end {
            return false;
        }
        bytes[start..end].iter().all(|&b| b == b'-')
    })
}

/// Check if a line looks like a table row (starts and ends with |, has interior |).
fn is_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() > 2
}

/// Parse a markdown table from lines. Returns (headers, rows, alignments) or None.
fn parse_markdown_table(lines: &[&str]) -> Option<Table> {
    if lines.len() < 2 {
        return None;
    }
    // First line: header row, second line: separator
    if !is_table_row(lines[0]) || !is_table_separator(lines[1]) {
        return None;
    }
    let headers: Vec<String> = lines[0]
        .trim()
        .trim_matches('|')
        .split('|')
        .map(|s| s.trim().to_string())
        .collect();
    if headers.is_empty() || headers.iter().all(|h| h.is_empty()) {
        return None;
    }

    // Parse alignment from separator
    let alignment: Vec<ColumnAlign> = lines[1]
        .trim()
        .trim_matches('|')
        .split('|')
        .map(|cell| {
            let c = cell.trim();
            let left = c.starts_with(':');
            let right = c.ends_with(':');
            match (left, right) {
                (true, true) => ColumnAlign::Center,
                (false, true) => ColumnAlign::Right,
                _ => ColumnAlign::Left,
            }
        })
        .collect();

    let mut rows = Vec::new();
    for &line in &lines[2..] {
        if !is_table_row(line) {
            break;
        }
        let cells: Vec<String> = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(|s| s.trim().to_string())
            .collect();
        rows.push(cells);
    }

    Some(Table {
        headers,
        rows,
        alignment,
    })
}

/// Segment types found during markdown detection.
#[derive(Debug)]
enum Segment {
    Text(String),
    DetectedTable(Table),
    DetectedList(List),
    DetectedCodeBlock { language: Option<String>, code: String },
}

// ---------------------------------------------------------------------------
// Table & List types
// ---------------------------------------------------------------------------

/// Column alignment for table columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColumnAlign {
    Left,
    Center,
    Right,
}

/// List rendering style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ListStyle {
    Bullet,
    Ordered,
}

/// A table with headers, rows, and column alignment.
#[derive(Debug, Clone)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub alignment: Vec<ColumnAlign>,
}

/// A list of items with a rendering style.
#[derive(Debug, Clone)]
pub struct List {
    pub items: Vec<String>,
    pub style: ListStyle,
}

// ---------------------------------------------------------------------------
// RichContent & FormattedOutput
// ---------------------------------------------------------------------------

/// Rich content that can be degraded based on channel capabilities.
#[derive(Debug, Clone)]
pub enum RichContent {
    /// Plain text (no degradation needed).
    Text(String),
    /// Rich embed with title, description, fields, and optional color.
    Embed {
        title: String,
        description: String,
        fields: Vec<(String, String, bool)>, // (name, value, inline)
        color: Option<u32>,
    },
    /// Image reference with optional caption.
    Image {
        url: String,
        caption: Option<String>,
    },
    /// Code block with optional language tag.
    CodeBlock {
        language: Option<String>,
        code: String,
    },
    /// Tabular data with headers, rows, and alignment.
    Table(Table),
    /// A list of items (bullet or ordered).
    List(List),
}

/// Formatted output ready for channel-specific rendering.
#[derive(Debug, Clone)]
pub enum FormattedOutput {
    /// Pass-through text.
    Text(String),
    /// Structured embed data (for embed-capable channels).
    Embed {
        title: String,
        description: String,
        fields: Vec<(String, String, bool)>,
        color: Option<u32>,
    },
    /// Image reference.
    Image {
        url: String,
        caption: Option<String>,
    },
}

/// Centralized content formatter that degrades rich content based on channel capabilities.
pub struct FormatPipeline;

impl FormatPipeline {
    /// Single entry point for adapters: auto-detect markdown structures in LLM
    /// text output and format them via the degradation pipeline.
    ///
    /// For `FullMarkdown` channels (Discord, Slack), returns text as-is since
    /// those platforms render markdown natively. For other channels, detects
    /// tables, lists, and code blocks and degrades them appropriately.
    pub fn detect_and_format(text: &str, caps: &ChannelCapabilities) -> String {
        // FullMarkdown channels render markdown natively -- skip detection
        if caps.formatting_support == FormattingSupport::FullMarkdown {
            return text.to_string();
        }

        let segments = detect_segments(text);

        // If only one Text segment, return as-is (no structures detected)
        if segments.len() == 1 {
            if let Segment::Text(ref t) = segments[0] {
                return t.clone();
            }
        }

        let mut out = String::new();
        for segment in &segments {
            match segment {
                Segment::Text(t) => out.push_str(t),
                Segment::DetectedTable(table) => {
                    let formatted = Self::format(&RichContent::Table(table.clone()), caps);
                    if let FormattedOutput::Text(t) = formatted {
                        out.push_str(&t);
                    }
                }
                Segment::DetectedList(list) => {
                    let formatted = Self::format(&RichContent::List(list.clone()), caps);
                    if let FormattedOutput::Text(t) = formatted {
                        out.push_str(&t);
                    }
                }
                Segment::DetectedCodeBlock { language, code } => {
                    let formatted = Self::format(
                        &RichContent::CodeBlock {
                            language: language.clone(),
                            code: code.clone(),
                        },
                        caps,
                    );
                    if let FormattedOutput::Text(t) = formatted {
                        out.push_str(&t);
                    }
                }
            }
        }
        out
    }

    /// Format rich content for a channel with the given capabilities.
    ///
    /// When the channel supports the content type, passes through.
    /// When it doesn't, degrades to a text representation.
    pub fn format(content: &RichContent, caps: &ChannelCapabilities) -> FormattedOutput {
        match content {
            RichContent::Text(text) => FormattedOutput::Text(text.clone()),
            RichContent::Embed {
                title,
                description,
                fields,
                color,
            } => {
                if caps.supports_embeds {
                    FormattedOutput::Embed {
                        title: title.clone(),
                        description: description.clone(),
                        fields: fields.clone(),
                        color: *color,
                    }
                } else {
                    // Degrade: convert embed to formatted text block
                    let mut text = format!("**{}**\n{}", title, description);
                    for (name, value, _inline) in fields {
                        text.push_str(&format!("\n**{}:** {}", name, value));
                    }
                    FormattedOutput::Text(text)
                }
            }
            RichContent::Image { url, caption } => {
                if caps.supports_images {
                    FormattedOutput::Image {
                        url: url.clone(),
                        caption: caption.clone(),
                    }
                } else {
                    // Degrade: convert image to text reference
                    let text = match caption {
                        Some(cap) => format!("[image: {}] {}", cap, url),
                        None => format!("[image] {}", url),
                    };
                    FormattedOutput::Text(text)
                }
            }
            RichContent::CodeBlock { language, code } => {
                if caps.formatting_support == FormattingSupport::HTML {
                    let class = match language {
                        Some(lang) => format!(" class=\"language-{}\"", lang),
                        None => String::new(),
                    };
                    FormattedOutput::Text(format!(
                        "<pre><code{}>{}</code></pre>",
                        class,
                        html_escape(code)
                    ))
                } else {
                    let lang = language.as_deref().unwrap_or("");
                    FormattedOutput::Text(format!("```{}\n{}\n```", lang, code))
                }
            }
            RichContent::Table(table) => format_table(table, caps),
            RichContent::List(list) => format_list(list, caps),
        }
    }
}

// ---------------------------------------------------------------------------
// Markdown structure detection
// ---------------------------------------------------------------------------

/// Scan text and split into segments of plain text and detected structures.
fn detect_segments(text: &str) -> Vec<Segment> {
    let lines: Vec<&str> = text.lines().collect();
    let mut segments: Vec<Segment> = Vec::new();
    let mut text_buf = String::new();
    let mut i = 0;

    while i < lines.len() {
        // Try code block detection: ```
        if lines[i].trim_start().starts_with("```") {
            let lang_part = lines[i].trim_start().strip_prefix("```").unwrap_or("");
            let language = if lang_part.trim().is_empty() {
                None
            } else {
                Some(lang_part.trim().to_string())
            };
            // Find closing fence
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim_start().starts_with("```") {
                j += 1;
            }
            if j < lines.len() {
                // Found closing fence
                if !text_buf.is_empty() {
                    segments.push(Segment::Text(text_buf.clone()));
                    text_buf.clear();
                }
                let code = lines[i + 1..j].join("\n");
                segments.push(Segment::DetectedCodeBlock { language, code });
                i = j + 1;
                continue;
            }
            // No closing fence found -- treat as text
        }

        // Try table detection: line with | and next line is separator
        if is_table_row(lines[i]) && i + 1 < lines.len() && is_table_separator(lines[i + 1]) {
            if let Some(table) = parse_markdown_table(&lines[i..]) {
                if !text_buf.is_empty() {
                    segments.push(Segment::Text(text_buf.clone()));
                    text_buf.clear();
                }
                let consumed = 2 + table.rows.len(); // header + separator + data rows
                segments.push(Segment::DetectedTable(table));
                i += consumed;
                continue;
            }
        }

        // Try list detection: consecutive lines starting with `- ` or `N. `
        if is_list_line(lines[i]) {
            let mut j = i;
            while j < lines.len() && is_list_line(lines[j]) {
                j += 1;
            }
            if j > i {
                if !text_buf.is_empty() {
                    segments.push(Segment::Text(text_buf.clone()));
                    text_buf.clear();
                }
                let style = if lines[i].trim_start().starts_with("- ") {
                    ListStyle::Bullet
                } else {
                    ListStyle::Ordered
                };
                let items: Vec<String> = lines[i..j]
                    .iter()
                    .map(|l| {
                        let trimmed = l.trim_start();
                        if trimmed.starts_with("- ") {
                            trimmed[2..].to_string()
                        } else {
                            // Strip "N. " prefix
                            if let Some(pos) = trimmed.find(". ") {
                                trimmed[pos + 2..].to_string()
                            } else {
                                trimmed.to_string()
                            }
                        }
                    })
                    .collect();
                segments.push(Segment::DetectedList(List { items, style }));
                i = j;
                continue;
            }
        }

        // Plain text line
        if !text_buf.is_empty() {
            text_buf.push('\n');
        }
        text_buf.push_str(lines[i]);
        i += 1;
    }

    // Handle trailing newline from original text
    if text.ends_with('\n') && !text_buf.is_empty() {
        text_buf.push('\n');
    }

    if !text_buf.is_empty() {
        segments.push(Segment::Text(text_buf));
    }

    if segments.is_empty() {
        segments.push(Segment::Text(text.to_string()));
    }

    segments
}

/// Check if a line looks like a list item (starts with `- ` or `N. `).
fn is_list_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("- ") {
        return true;
    }
    // Check for ordered list: digit(s) followed by `. `
    let bytes = trimmed.as_bytes();
    let mut di = 0;
    while di < bytes.len() && bytes[di].is_ascii_digit() {
        di += 1;
    }
    di > 0 && di < bytes.len() - 1 && bytes[di] == b'.' && bytes[di + 1] == b' '
}

// ---------------------------------------------------------------------------
// Table formatting (4-tier degradation: HTML Tier 0 + existing 3 tiers)
// ---------------------------------------------------------------------------

/// Format a table with 3-tier degradation based on channel capabilities.
fn format_table(table: &Table, caps: &ChannelCapabilities) -> FormattedOutput {
    if table.headers.is_empty() {
        return FormattedOutput::Text(String::new());
    }

    let text = if caps.formatting_support == FormattingSupport::HTML {
        // Tier 0: Semantic HTML table (Matrix, Gateway)
        format_table_html(table)
    } else if caps.supports_code_blocks {
        // Tier 1: Unicode box-drawing table wrapped in code fence
        format_table_unicode(table)
    } else if caps.formatting_support == FormattingSupport::FullMarkdown {
        // Tier 2: GFM markdown table
        format_table_gfm(table)
    } else {
        // Tier 3: Key:value per row
        format_table_keyvalue(table)
    };

    FormattedOutput::Text(text)
}

/// Tier 0: Semantic HTML table for HTML-capable channels.
fn format_table_html(table: &Table) -> String {
    let mut out = String::from("<table>\n<thead>\n<tr>");
    for header in &table.headers {
        out.push_str("<th>");
        out.push_str(&html_escape(header));
        out.push_str("</th>");
    }
    out.push_str("</tr>\n</thead>\n<tbody>\n");
    if table.rows.is_empty() {
        out.push_str("<tr><td colspan=\"");
        out.push_str(&table.headers.len().to_string());
        out.push_str("\">(no data)</td></tr>\n");
    } else {
        for row in &table.rows {
            out.push_str("<tr>");
            for (i, _) in table.headers.iter().enumerate() {
                let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
                out.push_str("<td>");
                out.push_str(&html_escape(cell));
                out.push_str("</td>");
            }
            out.push_str("</tr>\n");
        }
    }
    out.push_str("</tbody>\n</table>");
    out
}

/// Minimal HTML escaping for table cell content.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Tier 1: Unicode box-drawing table wrapped in triple-backtick code fence.
fn format_table_unicode(table: &Table) -> String {
    let col_count = table.headers.len();

    // Compute column widths using char count (not byte len).
    let mut widths: Vec<usize> = table.headers.iter().map(|h| h.chars().count()).collect();

    for row in &table.rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_count {
                widths[i] = widths[i].max(cell.chars().count());
            }
        }
    }

    // Ensure minimum width of 1.
    for w in &mut widths {
        if *w == 0 {
            *w = 1;
        }
    }

    let mut out = String::from("```\n");

    // Top border: +---+---+
    out.push_str(&box_horizontal_line(&widths, '+'));

    // Header row: | H1  | H2  |
    out.push_str(&box_data_row(
        &table.headers,
        &widths,
        &table.alignment,
        col_count,
    ));

    // Separator: +---+---+
    out.push_str(&box_horizontal_line(&widths, '+'));

    if table.rows.is_empty() {
        // Empty table: show (no data)
        let total_inner: usize = widths.iter().sum::<usize>() + (col_count - 1) * 3;
        let no_data = "(no data)";
        let padded = format!("| {:<width$} |", no_data, width = total_inner);
        out.push_str(&padded);
        out.push('\n');
        out.push_str(&box_horizontal_line(&widths, '+'));
    } else {
        // Data rows
        for row in &table.rows {
            out.push_str(&box_data_row(row, &widths, &table.alignment, col_count));
        }
        // Bottom border
        out.push_str(&box_horizontal_line(&widths, '+'));
    }

    out.push_str("```");
    out
}

/// Build a horizontal line like `+------+------+`
fn box_horizontal_line(widths: &[usize], corner: char) -> String {
    let mut line = String::new();
    for (i, w) in widths.iter().enumerate() {
        if i == 0 {
            line.push(corner);
        }
        for _ in 0..(*w + 2) {
            line.push('-');
        }
        line.push(corner);
    }
    line.push('\n');
    line
}

/// Build a data row like `| val1 | val2 |` with alignment.
fn box_data_row(
    cells: &[String],
    widths: &[usize],
    alignment: &[ColumnAlign],
    col_count: usize,
) -> String {
    let mut line = String::new();
    for (i, &w) in widths.iter().enumerate().take(col_count) {
        let cell = cells.get(i).map(|s| s.as_str()).unwrap_or("");
        let align = alignment.get(i).copied().unwrap_or(ColumnAlign::Left);
        line.push_str("| ");
        let cell_len = cell.chars().count();
        match align {
            ColumnAlign::Left => {
                line.push_str(cell);
                for _ in cell_len..w {
                    line.push(' ');
                }
            }
            ColumnAlign::Right => {
                for _ in cell_len..w {
                    line.push(' ');
                }
                line.push_str(cell);
            }
            ColumnAlign::Center => {
                let pad = w.saturating_sub(cell_len);
                let left_pad = pad / 2;
                let right_pad = pad - left_pad;
                for _ in 0..left_pad {
                    line.push(' ');
                }
                line.push_str(cell);
                for _ in 0..right_pad {
                    line.push(' ');
                }
            }
        }
        line.push(' ');
    }
    line.push('|');
    line.push('\n');
    line
}

/// Tier 2: GFM markdown table with alignment markers.
fn format_table_gfm(table: &Table) -> String {
    let col_count = table.headers.len();
    let mut out = String::new();

    // Header row: | H1 | H2 |
    out.push('|');
    for header in &table.headers {
        out.push(' ');
        out.push_str(header);
        out.push_str(" |");
    }
    out.push('\n');

    // Alignment row: | :--- | ---: | :---: |
    out.push('|');
    for i in 0..col_count {
        let align = table.alignment.get(i).copied().unwrap_or(ColumnAlign::Left);
        match align {
            ColumnAlign::Left => out.push_str(" :--- |"),
            ColumnAlign::Right => out.push_str(" ---: |"),
            ColumnAlign::Center => out.push_str(" :---: |"),
        }
    }
    out.push('\n');

    if table.rows.is_empty() {
        // Empty table: show (no data) spanning all columns
        out.push('|');
        out.push_str(" (no data)");
        for _ in 1..col_count {
            out.push_str(" |");
        }
        out.push_str(" |\n");
    } else {
        // Data rows
        for row in &table.rows {
            out.push('|');
            for i in 0..col_count {
                let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
                out.push(' ');
                out.push_str(cell);
                out.push_str(" |");
            }
            out.push('\n');
        }
    }

    out
}

/// Tier 3: Key:value per row (PlainText/BasicMarkdown fallback).
fn format_table_keyvalue(table: &Table) -> String {
    if table.rows.is_empty() {
        // Empty table with headers only
        let header_line = table.headers.join(" | ");
        return format!("{}\n(no data)", header_line);
    }

    let col_count = table.headers.len();
    let mut out = String::new();

    for (ri, row) in table.rows.iter().enumerate() {
        if ri > 0 {
            out.push('\n');
        }
        let mut parts = Vec::new();
        for i in 0..col_count {
            let header = &table.headers[i];
            let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
            parts.push(format!("{}: {}", header, cell));
        }
        out.push_str(&parts.join(" | "));
    }

    out
}

// ---------------------------------------------------------------------------
// List formatting
// ---------------------------------------------------------------------------

/// Format a list with HTML support for HTML-capable channels.
fn format_list(list: &List, caps: &ChannelCapabilities) -> FormattedOutput {
    if caps.formatting_support == FormattingSupport::HTML {
        return format_list_html(list);
    }
    let mut out = String::new();
    for (i, item) in list.items.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        match list.style {
            ListStyle::Bullet => {
                out.push_str("- ");
                out.push_str(item);
            }
            ListStyle::Ordered => {
                out.push_str(&format!("{}. ", i + 1));
                out.push_str(item);
            }
        }
    }
    FormattedOutput::Text(out)
}

/// HTML list rendering.
fn format_list_html(list: &List) -> FormattedOutput {
    let (tag, _) = match list.style {
        ListStyle::Bullet => ("ul", ""),
        ListStyle::Ordered => ("ol", ""),
    };
    let mut out = format!("<{}>", tag);
    for item in &list.items {
        out.push_str("<li>");
        out.push_str(&html_escape(item));
        out.push_str("</li>");
    }
    out.push_str(&format!("</{}>", tag));
    FormattedOutput::Text(out)
}

// ---------------------------------------------------------------------------
// Message splitting utility
// ---------------------------------------------------------------------------

/// Split text at paragraph boundaries respecting `max_length`.
///
/// Uses 90% of `max_length` as threshold to leave room for adapter formatting
/// expansion. Split priority: double newline > single newline > sentence
/// boundary (`. ` followed by uppercase). Code blocks, lists, and tables are
/// treated as atomic units that are never split internally.
///
/// If `max_length` is `None`, returns the input as a single chunk.
pub fn split_at_paragraphs(text: &str, max_length: Option<usize>) -> Vec<String> {
    let max_length = match max_length {
        Some(m) => m,
        None => return vec![text.to_string()],
    };

    let effective_limit = (max_length * 9) / 10; // 90% threshold
    if text.len() <= effective_limit {
        return vec![text.to_string()];
    }

    // Identify atomic blocks and split at paragraph boundaries
    let blocks = identify_blocks(text);

    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();

    for block in &blocks {
        let block_text = &block.text;

        if current.is_empty() {
            if block_text.len() <= effective_limit {
                current = block_text.clone();
            } else if block.atomic {
                // Atomic block exceeding limit -- warn and emit as own chunk
                tracing::warn!(
                    "Atomic block ({} bytes) exceeds message limit ({} bytes), sending as single chunk",
                    block_text.len(),
                    effective_limit
                );
                chunks.push(block_text.clone());
            } else {
                // Large non-atomic text block -- split at sub-boundaries
                let sub = split_text_block(block_text, effective_limit);
                for (i, s) in sub.into_iter().enumerate() {
                    if i == 0 {
                        current = s;
                    } else {
                        if !current.is_empty() {
                            chunks.push(current);
                        }
                        current = s;
                    }
                }
            }
            continue;
        }

        // Check if adding this block fits
        let separator = if block.needs_leading_separator {
            "\n\n"
        } else {
            ""
        };
        let combined_len = current.len() + separator.len() + block_text.len();

        if combined_len <= effective_limit {
            current.push_str(separator);
            current.push_str(block_text);
        } else {
            // Doesn't fit -- flush current and start new chunk
            let flushed = std::mem::take(&mut current);
            chunks.push(flushed);

            // Handle table header repetition
            if block.is_table && block_text.len() > effective_limit {
                let table_chunks = split_table_with_headers(block_text, effective_limit);
                let tc_len = table_chunks.len();
                for (i, tc) in table_chunks.into_iter().enumerate() {
                    if i == tc_len.saturating_sub(1) {
                        current = tc;
                    } else {
                        chunks.push(tc);
                    }
                }
                continue;
            }

            if block_text.len() <= effective_limit {
                current = block_text.clone();
            } else if block.atomic {
                tracing::warn!(
                    "Atomic block ({} bytes) exceeds message limit ({} bytes), sending as single chunk",
                    block_text.len(),
                    effective_limit
                );
                chunks.push(block_text.clone());
            } else {
                let sub = split_text_block(block_text, effective_limit);
                for s in sub {
                    if current.is_empty() {
                        current = s;
                    } else {
                        let prev = std::mem::take(&mut current);
                        chunks.push(prev);
                        current = s;
                    }
                }
            }
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    if chunks.is_empty() {
        chunks.push(text.to_string());
    }

    chunks
}

struct Block {
    text: String,
    atomic: bool,
    is_table: bool,
    needs_leading_separator: bool,
}

/// Identify atomic blocks (code fences, lists, tables) and text paragraphs.
fn identify_blocks(text: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let parts: Vec<&str> = text.split("\n\n").collect();

    for (pi, part) in parts.iter().enumerate() {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }

        let is_code_fence = trimmed.starts_with("```");
        let is_list = trimmed.lines().all(|l| is_list_line(l));
        let is_table = {
            let lines: Vec<&str> = trimmed.lines().collect();
            lines.len() >= 2 && is_table_row(lines[0]) && is_table_separator(lines[1])
        };

        blocks.push(Block {
            text: part.to_string(),
            atomic: is_code_fence || is_list || is_table,
            is_table,
            needs_leading_separator: pi > 0,
        });
    }

    blocks
}

/// Split a non-atomic text block at single newlines or sentence boundaries.
fn split_text_block(text: &str, limit: usize) -> Vec<String> {
    // Try single newline split first
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.len() > 1 {
        let mut chunks = Vec::new();
        let mut current = String::new();
        for line in &lines {
            let combined = if current.is_empty() {
                line.len()
            } else {
                current.len() + 1 + line.len()
            };
            if combined <= limit {
                if !current.is_empty() {
                    current.push('\n');
                }
                current.push_str(line);
            } else {
                if !current.is_empty() {
                    chunks.push(current);
                }
                current = line.to_string();
            }
        }
        if !current.is_empty() {
            chunks.push(current);
        }
        return chunks;
    }

    // Try sentence boundary: `. ` followed by uppercase
    // Find all sentence boundaries, then greedily pack into chunks
    let bytes = text.as_bytes();
    let mut boundaries: Vec<usize> = Vec::new();
    for i in 0..bytes.len().saturating_sub(2) {
        if bytes[i] == b'.'
            && bytes[i + 1] == b' '
            && (i + 2 < bytes.len() && bytes[i + 2].is_ascii_uppercase())
        {
            boundaries.push(i + 2); // Position after ". "
        }
    }

    if boundaries.is_empty() {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    let mut last_good = start;

    for &boundary in &boundaries {
        if boundary - start > limit && last_good > start {
            chunks.push(text[start..last_good].to_string());
            start = last_good;
        }
        last_good = boundary;
    }
    // If remaining text exceeds limit and we have a boundary to split at
    if text.len() - start > limit && last_good > start {
        chunks.push(text[start..last_good].to_string());
        start = last_good;
    }
    if start < text.len() {
        chunks.push(text[start..].to_string());
    }

    if chunks.is_empty() {
        chunks.push(text.to_string());
    }
    chunks
}

/// Split a table block with header repetition in each chunk.
fn split_table_with_headers(text: &str, limit: usize) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 3 {
        return vec![text.to_string()];
    }

    // Lines 0 and 1 are header and separator
    let header = format!("{}\n{}", lines[0], lines[1]);
    let header_len = header.len() + 1; // +1 for newline before first data row

    let mut chunks = Vec::new();
    let mut current = header.clone();

    for line in &lines[2..] {
        let combined = current.len() + 1 + line.len();
        if combined <= limit {
            current.push('\n');
            current.push_str(line);
        } else {
            chunks.push(current);
            current = format!("{}\n{}", header, line);
        }
    }
    if !current.is_empty() && current != header {
        chunks.push(current);
    }

    if chunks.is_empty() {
        chunks.push(text.to_string());
    }
    let _ = header_len; // suppress unused warning
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FormattingSupport;

    fn caps_all() -> ChannelCapabilities {
        ChannelCapabilities {
            supports_edit: true,
            supports_typing: true,
            supports_images: true,
            supports_documents: true,
            supports_voice: true,
            max_message_length: Some(4096),
            supports_embeds: true,
            supports_reactions: true,
            supports_threads: true,
            ..Default::default()
        }
    }

    fn caps_minimal() -> ChannelCapabilities {
        ChannelCapabilities {
            max_message_length: Some(4096),
            ..Default::default()
        }
    }

    fn caps_code_blocks() -> ChannelCapabilities {
        ChannelCapabilities {
            supports_code_blocks: true,
            max_message_length: Some(4096),
            formatting_support: FormattingSupport::FullMarkdown,
            ..Default::default()
        }
    }

    fn caps_full_markdown_no_code() -> ChannelCapabilities {
        ChannelCapabilities {
            supports_code_blocks: false,
            formatting_support: FormattingSupport::FullMarkdown,
            max_message_length: Some(4096),
            ..Default::default()
        }
    }

    fn caps_plain_text() -> ChannelCapabilities {
        ChannelCapabilities {
            supports_code_blocks: false,
            formatting_support: FormattingSupport::PlainText,
            max_message_length: Some(4096),
            ..Default::default()
        }
    }

    fn caps_basic_markdown() -> ChannelCapabilities {
        ChannelCapabilities {
            supports_code_blocks: false,
            formatting_support: FormattingSupport::BasicMarkdown,
            max_message_length: Some(4096),
            ..Default::default()
        }
    }

    // -- Original format tests --

    #[test]
    fn text_passes_through() {
        let content = RichContent::Text("Hello, world!".into());
        let output = FormatPipeline::format(&content, &caps_all());
        match output {
            FormattedOutput::Text(t) => assert_eq!(t, "Hello, world!"),
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn embed_passes_through_when_supported() {
        let content = RichContent::Embed {
            title: "Status".into(),
            description: "All systems operational".into(),
            fields: vec![("Uptime".into(), "99.9%".into(), true)],
            color: Some(0x00FF00),
        };
        let output = FormatPipeline::format(&content, &caps_all());
        match output {
            FormattedOutput::Embed {
                title,
                description,
                fields,
                color,
            } => {
                assert_eq!(title, "Status");
                assert_eq!(description, "All systems operational");
                assert_eq!(fields.len(), 1);
                assert_eq!(color, Some(0x00FF00));
            }
            _ => panic!("expected Embed output"),
        }
    }

    #[test]
    fn embed_degrades_to_text_when_unsupported() {
        let content = RichContent::Embed {
            title: "Status".into(),
            description: "All systems operational".into(),
            fields: vec![("Uptime".into(), "99.9%".into(), true)],
            color: Some(0x00FF00),
        };
        let output = FormatPipeline::format(&content, &caps_minimal());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("**Status**"));
                assert!(t.contains("All systems operational"));
                assert!(t.contains("**Uptime:** 99.9%"));
            }
            _ => panic!("expected Text output for degraded embed"),
        }
    }

    #[test]
    fn image_passes_through_when_supported() {
        let content = RichContent::Image {
            url: "https://example.com/cat.png".into(),
            caption: Some("A cat".into()),
        };
        let output = FormatPipeline::format(&content, &caps_all());
        match output {
            FormattedOutput::Image { url, caption } => {
                assert_eq!(url, "https://example.com/cat.png");
                assert_eq!(caption, Some("A cat".into()));
            }
            _ => panic!("expected Image output"),
        }
    }

    #[test]
    fn image_degrades_to_text_when_unsupported() {
        let content = RichContent::Image {
            url: "https://example.com/cat.png".into(),
            caption: Some("A cat".into()),
        };
        let output = FormatPipeline::format(&content, &caps_minimal());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("[image: A cat]"));
                assert!(t.contains("https://example.com/cat.png"));
            }
            _ => panic!("expected Text output for degraded image"),
        }
    }

    #[test]
    fn image_degrades_without_caption() {
        let content = RichContent::Image {
            url: "https://example.com/cat.png".into(),
            caption: None,
        };
        let output = FormatPipeline::format(&content, &caps_minimal());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("[image]"));
                assert!(t.contains("https://example.com/cat.png"));
            }
            _ => panic!("expected Text output for degraded image"),
        }
    }

    #[test]
    fn code_block_with_language() {
        let content = RichContent::CodeBlock {
            language: Some("rust".into()),
            code: "fn main() {}".into(),
        };
        let output = FormatPipeline::format(&content, &caps_all());
        match output {
            FormattedOutput::Text(t) => {
                assert_eq!(t, "```rust\nfn main() {}\n```");
            }
            _ => panic!("expected Text output for code block"),
        }
    }

    #[test]
    fn code_block_without_language() {
        let content = RichContent::CodeBlock {
            language: None,
            code: "hello".into(),
        };
        let output = FormatPipeline::format(&content, &caps_all());
        match output {
            FormattedOutput::Text(t) => {
                assert_eq!(t, "```\nhello\n```");
            }
            _ => panic!("expected Text output for code block"),
        }
    }

    // -- Table degradation tests --

    fn small_table() -> Table {
        Table {
            headers: vec!["Name".into(), "Age".into()],
            rows: vec![
                vec!["Alice".into(), "30".into()],
                vec!["Bob".into(), "25".into()],
            ],
            alignment: vec![ColumnAlign::Left, ColumnAlign::Right],
        }
    }

    fn wide_table() -> Table {
        Table {
            headers: vec![
                "ID".into(),
                "Name".into(),
                "Email".into(),
                "Role".into(),
                "Status".into(),
            ],
            rows: vec![
                vec![
                    "1".into(),
                    "Alice".into(),
                    "alice@example.com".into(),
                    "Admin".into(),
                    "Active".into(),
                ],
                vec![
                    "2".into(),
                    "Bob".into(),
                    "bob@example.com".into(),
                    "User".into(),
                    "Inactive".into(),
                ],
            ],
            alignment: vec![
                ColumnAlign::Right,
                ColumnAlign::Left,
                ColumnAlign::Left,
                ColumnAlign::Center,
                ColumnAlign::Left,
            ],
        }
    }

    fn empty_table() -> Table {
        Table {
            headers: vec!["Name".into(), "Value".into()],
            rows: vec![],
            alignment: vec![ColumnAlign::Left, ColumnAlign::Left],
        }
    }

    // -- Tier 1: Unicode box-drawing (code blocks) --

    #[test]
    fn table_small_tier1_unicode() {
        let content = RichContent::Table(small_table());
        let output = FormatPipeline::format(&content, &caps_code_blocks());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.starts_with("```\n"), "should start with code fence");
                assert!(t.ends_with("```"), "should end with code fence");
                assert!(t.contains("| Alice"), "should contain cell data");
                assert!(t.contains("| Name"), "should contain header");
                assert!(t.contains("+-------+-----+"), "should have box borders");
            }
            _ => panic!("expected Text output for table"),
        }
    }

    #[test]
    fn table_wide_tier1_unicode() {
        let content = RichContent::Table(wide_table());
        let output = FormatPipeline::format(&content, &caps_code_blocks());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.starts_with("```\n"));
                assert!(t.contains("alice@example.com"));
                assert!(t.contains("| ID"));
                assert!(t.contains("| Status"));
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn table_empty_tier1_unicode() {
        let content = RichContent::Table(empty_table());
        let output = FormatPipeline::format(&content, &caps_code_blocks());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("Name"));
                assert!(t.contains("Value"));
                assert!(t.contains("(no data)"));
            }
            _ => panic!("expected Text output"),
        }
    }

    // -- Tier 2: GFM markdown --

    #[test]
    fn table_small_tier2_gfm() {
        let content = RichContent::Table(small_table());
        let output = FormatPipeline::format(&content, &caps_full_markdown_no_code());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("| Name | Age |"), "should have header row");
                assert!(t.contains(":---"), "should have alignment marker");
                assert!(t.contains("---:"), "should have right alignment marker");
                assert!(t.contains("| Alice | 30 |"), "should have data row");
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn table_wide_tier2_gfm() {
        let content = RichContent::Table(wide_table());
        let output = FormatPipeline::format(&content, &caps_full_markdown_no_code());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("| ID |"));
                assert!(t.contains("| Status |"));
                assert!(t.contains(":---:"), "center align marker");
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn table_empty_tier2_gfm() {
        let content = RichContent::Table(empty_table());
        let output = FormatPipeline::format(&content, &caps_full_markdown_no_code());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("| Name |"));
                assert!(t.contains("(no data)"));
            }
            _ => panic!("expected Text output"),
        }
    }

    // -- Tier 3: Key:value pairs --

    #[test]
    fn table_small_tier3_keyvalue() {
        let content = RichContent::Table(small_table());
        let output = FormatPipeline::format(&content, &caps_plain_text());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("Name: Alice | Age: 30"));
                assert!(t.contains("Name: Bob | Age: 25"));
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn table_wide_tier3_keyvalue() {
        let content = RichContent::Table(wide_table());
        let output = FormatPipeline::format(&content, &caps_plain_text());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("ID: 1"));
                assert!(t.contains("Email: alice@example.com"));
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn table_empty_tier3_keyvalue() {
        let content = RichContent::Table(empty_table());
        let output = FormatPipeline::format(&content, &caps_plain_text());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("Name | Value"));
                assert!(t.contains("(no data)"));
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn table_basic_markdown_uses_tier3() {
        let content = RichContent::Table(small_table());
        let output = FormatPipeline::format(&content, &caps_basic_markdown());
        match output {
            FormattedOutput::Text(t) => {
                // BasicMarkdown uses Tier 3 key:value
                assert!(t.contains("Name: Alice"));
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn table_html_renders_semantic_html() {
        let caps = ChannelCapabilities {
            supports_code_blocks: false,
            formatting_support: FormattingSupport::HTML,
            max_message_length: Some(4096),
            ..Default::default()
        };
        let content = RichContent::Table(small_table());
        let output = FormatPipeline::format(&content, &caps);
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("<table>"), "should have table tag");
                assert!(t.contains("<th>Name</th>"), "should have th header");
                assert!(t.contains("<td>Alice</td>"), "should have td cell");
                assert!(t.contains("</thead>"));
                assert!(t.contains("</tbody>"));
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn table_html_empty_shows_no_data() {
        let caps = ChannelCapabilities {
            formatting_support: FormattingSupport::HTML,
            ..Default::default()
        };
        let content = RichContent::Table(empty_table());
        let output = FormatPipeline::format(&content, &caps);
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("(no data)"));
                assert!(t.contains("colspan"));
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn table_html_escapes_special_chars() {
        let table = Table {
            headers: vec!["Key".into(), "Value".into()],
            rows: vec![vec!["<script>".into(), "a & b".into()]],
            alignment: vec![ColumnAlign::Left, ColumnAlign::Left],
        };
        let caps = ChannelCapabilities {
            formatting_support: FormattingSupport::HTML,
            ..Default::default()
        };
        let output = FormatPipeline::format(&RichContent::Table(table), &caps);
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("&lt;script&gt;"));
                assert!(t.contains("a &amp; b"));
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn table_single_column() {
        let table = Table {
            headers: vec!["Item".into()],
            rows: vec![vec!["Apple".into()], vec!["Banana".into()]],
            alignment: vec![ColumnAlign::Left],
        };
        let content = RichContent::Table(table);
        let output = FormatPipeline::format(&content, &caps_code_blocks());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("| Item"));
                assert!(t.contains("| Apple"));
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn table_alignment_center() {
        let table = Table {
            headers: vec!["Col1".into(), "Col2".into()],
            rows: vec![vec!["AB".into(), "C".into()]],
            alignment: vec![ColumnAlign::Center, ColumnAlign::Center],
        };
        let content = RichContent::Table(table);
        let output = FormatPipeline::format(&content, &caps_code_blocks());
        match output {
            FormattedOutput::Text(t) => {
                // Center alignment: "AB" in width 4 -> " AB " (1 left, 1 right)
                assert!(t.contains("```\n"), "should have code fence");
                assert!(t.contains("Col1"));
                assert!(t.contains("AB"));
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn table_empty_headers_returns_empty() {
        let table = Table {
            headers: vec![],
            rows: vec![],
            alignment: vec![],
        };
        let content = RichContent::Table(table);
        let output = FormatPipeline::format(&content, &caps_code_blocks());
        match output {
            FormattedOutput::Text(t) => assert!(t.is_empty()),
            _ => panic!("expected Text output"),
        }
    }

    // -- List rendering tests --

    #[test]
    fn list_bullet_three_items() {
        let list = List {
            items: vec!["Apple".into(), "Banana".into(), "Cherry".into()],
            style: ListStyle::Bullet,
        };
        let content = RichContent::List(list);
        let output = FormatPipeline::format(&content, &caps_all());
        match output {
            FormattedOutput::Text(t) => {
                assert_eq!(t, "- Apple\n- Banana\n- Cherry");
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn list_ordered_five_items() {
        let list = List {
            items: vec![
                "First".into(),
                "Second".into(),
                "Third".into(),
                "Fourth".into(),
                "Fifth".into(),
            ],
            style: ListStyle::Ordered,
        };
        let content = RichContent::List(list);
        let output = FormatPipeline::format(&content, &caps_all());
        match output {
            FormattedOutput::Text(t) => {
                assert_eq!(t, "1. First\n2. Second\n3. Third\n4. Fourth\n5. Fifth");
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn list_empty() {
        let list = List {
            items: vec![],
            style: ListStyle::Bullet,
        };
        let content = RichContent::List(list);
        let output = FormatPipeline::format(&content, &caps_all());
        match output {
            FormattedOutput::Text(t) => assert!(t.is_empty()),
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn list_single_item() {
        let list = List {
            items: vec!["Only item".into()],
            style: ListStyle::Bullet,
        };
        let content = RichContent::List(list);
        let output = FormatPipeline::format(&content, &caps_all());
        match output {
            FormattedOutput::Text(t) => {
                assert_eq!(t, "- Only item");
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn list_single_item_ordered() {
        let list = List {
            items: vec!["Only item".into()],
            style: ListStyle::Ordered,
        };
        let content = RichContent::List(list);
        let output = FormatPipeline::format(&content, &caps_all());
        match output {
            FormattedOutput::Text(t) => {
                assert_eq!(t, "1. Only item");
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn list_degrades_cleanly_to_plain_text() {
        let list = List {
            items: vec!["Apple".into(), "Banana".into()],
            style: ListStyle::Bullet,
        };
        let content = RichContent::List(list);
        let output = FormatPipeline::format(&content, &caps_plain_text());
        match output {
            FormattedOutput::Text(t) => {
                assert_eq!(t, "- Apple\n- Banana");
            }
            _ => panic!("expected Text output"),
        }
    }

    // -- HTML list and code block tests --

    fn caps_html() -> ChannelCapabilities {
        ChannelCapabilities {
            formatting_support: FormattingSupport::HTML,
            max_message_length: Some(4096),
            ..Default::default()
        }
    }

    #[test]
    fn list_html_bullet() {
        let list = List {
            items: vec!["Apple".into(), "Banana".into()],
            style: ListStyle::Bullet,
        };
        let output = FormatPipeline::format(&RichContent::List(list), &caps_html());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("<ul>"));
                assert!(t.contains("<li>Apple</li>"));
                assert!(t.contains("</ul>"));
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn list_html_ordered() {
        let list = List {
            items: vec!["First".into(), "Second".into()],
            style: ListStyle::Ordered,
        };
        let output = FormatPipeline::format(&RichContent::List(list), &caps_html());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("<ol>"));
                assert!(t.contains("<li>First</li>"));
                assert!(t.contains("</ol>"));
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn code_block_html_with_language() {
        let content = RichContent::CodeBlock {
            language: Some("rust".into()),
            code: "fn main() {}".into(),
        };
        let output = FormatPipeline::format(&content, &caps_html());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("<pre><code class=\"language-rust\">"));
                assert!(t.contains("fn main() {}"));
                assert!(t.contains("</code></pre>"));
            }
            _ => panic!("expected Text output"),
        }
    }

    #[test]
    fn code_block_html_escapes_content() {
        let content = RichContent::CodeBlock {
            language: None,
            code: "if x < 10 && y > 5".into(),
        };
        let output = FormatPipeline::format(&content, &caps_html());
        match output {
            FormattedOutput::Text(t) => {
                assert!(t.contains("&lt;"));
                assert!(t.contains("&amp;&amp;"));
                assert!(t.contains("&gt;"));
            }
            _ => panic!("expected Text output"),
        }
    }

    // -- detect_and_format tests --

    #[test]
    fn test_detect_and_format_plain_text() {
        let text = "Hello, this is plain text without any markdown.";
        let result = FormatPipeline::detect_and_format(text, &caps_plain_text());
        assert_eq!(result, text);
    }

    #[test]
    fn test_detect_and_format_table_code_blocks() {
        let text = "Here is a table:\n| Name | Age |\n|---|---|\n| Alice | 30 |\n| Bob | 25 |";
        let caps = ChannelCapabilities {
            supports_code_blocks: true,
            formatting_support: FormattingSupport::BasicMarkdown,
            max_message_length: Some(4096),
            ..Default::default()
        };
        let result = FormatPipeline::detect_and_format(text, &caps);
        assert!(result.contains("```"), "should have code fence for table");
        assert!(result.contains("Alice"), "should contain cell data");
        assert!(result.contains("Here is a table:"), "should preserve surrounding text");
    }

    #[test]
    fn test_detect_and_format_table_html() {
        let text = "| Name | Age |\n|---|---|\n| Alice | 30 |";
        let result = FormatPipeline::detect_and_format(text, &caps_html());
        assert!(result.contains("<table>"), "should render HTML table");
        assert!(result.contains("<th>Name</th>"));
        assert!(result.contains("<td>Alice</td>"));
    }

    #[test]
    fn test_detect_and_format_bullet_list() {
        let text = "Items:\n- Apple\n- Banana\n- Cherry";
        let result = FormatPipeline::detect_and_format(text, &caps_plain_text());
        assert!(result.contains("- Apple"));
        assert!(result.contains("- Banana"));
        assert!(result.contains("Items:"));
    }

    #[test]
    fn test_detect_and_format_code_block() {
        let text = "Here is code:\n```rust\nfn main() {}\n```\nDone.";
        let result = FormatPipeline::detect_and_format(text, &caps_plain_text());
        assert!(result.contains("fn main() {}"));
        assert!(result.contains("Here is code:"));
        assert!(result.contains("Done."));
    }

    #[test]
    fn test_detect_and_format_full_markdown_skips_detection() {
        let text = "| Name | Age |\n|---|---|\n| Alice | 30 |";
        let caps = ChannelCapabilities {
            formatting_support: FormattingSupport::FullMarkdown,
            max_message_length: Some(4096),
            ..Default::default()
        };
        let result = FormatPipeline::detect_and_format(text, &caps);
        assert_eq!(result, text, "FullMarkdown should skip detection");
    }

    #[test]
    fn test_detect_and_format_mixed_content() {
        let text = "Intro text\n| A | B |\n|---|---|\n| 1 | 2 |\nMiddle\n- item1\n- item2\nEnd";
        let result = FormatPipeline::detect_and_format(text, &caps_plain_text());
        assert!(result.contains("Intro text"), "should have intro");
        assert!(result.contains("A: 1"), "table degrades to key:value in plain text");
        assert!(result.contains("Middle"), "should have middle text");
        assert!(result.contains("- item1"), "should have list");
        assert!(result.contains("End"), "should have ending");
    }

    #[test]
    fn test_detect_and_format_pipe_not_table() {
        // Pipes that aren't tables (no separator row)
        let text = "This | is | not a table\nJust some text with pipes";
        let result = FormatPipeline::detect_and_format(text, &caps_plain_text());
        assert_eq!(result, text, "should not detect non-tables");
    }

    #[test]
    fn test_detect_and_format_numbered_not_list() {
        // Text that starts with numbers but isn't really a list
        let text = "2026 is a good year.\nLet me explain why.";
        let result = FormatPipeline::detect_and_format(text, &caps_plain_text());
        assert_eq!(result, text, "should not detect non-lists");
    }

    #[test]
    fn test_detect_conservative_table_requires_separator() {
        // Table without separator row should not be detected
        let text = "| A | B |\n| 1 | 2 |";
        let result = FormatPipeline::detect_and_format(text, &caps_plain_text());
        assert_eq!(result, text, "no separator = no table detection");
    }

    // -- split_at_paragraphs tests (Task 2) --

    #[test]
    fn test_split_at_paragraphs_short_text() {
        let result = split_at_paragraphs("short text", Some(4096));
        assert_eq!(result, vec!["short text"]);
    }

    #[test]
    fn test_split_at_paragraphs_none_max_length() {
        let long = "a".repeat(10000);
        let result = split_at_paragraphs(&long, None);
        assert_eq!(result, vec![long]);
    }

    #[test]
    fn test_split_at_paragraphs_empty_input() {
        let result = split_at_paragraphs("", Some(4096));
        assert_eq!(result, vec![""]);
    }

    #[test]
    fn test_split_at_paragraphs_double_newline() {
        let text = format!("{}\n\n{}", "a".repeat(100), "b".repeat(100));
        let result = split_at_paragraphs(&text, Some(150));
        assert!(result.len() >= 2, "should split at double newline");
        assert!(result[0].contains(&"a".repeat(100)));
        assert!(result[1].contains(&"b".repeat(100)));
    }

    #[test]
    fn test_split_at_paragraphs_single_newline_fallback() {
        // No double newlines, should fall back to single newline
        let text = format!("{}\n{}", "a".repeat(100), "b".repeat(100));
        let result = split_at_paragraphs(&text, Some(150));
        assert!(result.len() >= 2, "should split at single newline");
    }

    #[test]
    fn test_split_at_paragraphs_sentence_fallback() {
        // No newlines at all, fall back to sentence boundary
        let text = format!("{}. {}", "a".repeat(100), "B".repeat(100));
        let result = split_at_paragraphs(&text, Some(150));
        assert!(result.len() >= 2, "should split at sentence boundary");
    }

    #[test]
    fn test_split_at_paragraphs_code_block_atomic() {
        let code = format!("```\n{}\n```", "x".repeat(200));
        let text = format!("Before\n\n{}\n\nAfter", code);
        let result = split_at_paragraphs(&text, Some(100));
        // Code block should never be split
        let code_chunk = result.iter().find(|c| c.contains("```")).unwrap();
        assert!(code_chunk.contains(&"x".repeat(200)), "code block must be atomic");
    }

    #[test]
    fn test_split_at_paragraphs_list_atomic() {
        let list_text = (0..20).map(|i| format!("- item {}", i)).collect::<Vec<_>>().join("\n");
        let text = format!("Before\n\n{}\n\nAfter", list_text);
        let result = split_at_paragraphs(&text, Some(100));
        // Find chunk containing list
        let list_chunk = result.iter().find(|c| c.contains("- item 0")).unwrap();
        assert!(list_chunk.contains("- item 19"), "list must be atomic");
    }

    #[test]
    fn test_split_at_paragraphs_table_header_repeat() {
        let mut rows = String::new();
        for i in 0..50 {
            rows.push_str(&format!("| row{} | data{} |\n", i, i));
        }
        let text = format!("| Name | Value |\n|---|---|\n{}", rows);
        let result = split_at_paragraphs(&text, Some(200));
        // If table is split, each chunk should have headers
        if result.len() > 1 {
            for chunk in &result {
                if chunk.contains("| row") {
                    assert!(chunk.contains("| Name | Value |"), "split table chunk must repeat headers");
                }
            }
        }
    }

    #[test]
    fn test_split_at_paragraphs_90_percent_threshold() {
        // 90% of 100 = 90. Text of exactly 90 should NOT split.
        let text = "a".repeat(90);
        let result = split_at_paragraphs(&text, Some(100));
        assert_eq!(result.len(), 1, "90 chars should fit in 90% of 100");

        // Text of 91 should trigger split attempt (but since no boundary, stays as one)
        let text2 = "a".repeat(91);
        let result2 = split_at_paragraphs(&text2, Some(100));
        // No split boundary available, so it stays as one chunk
        assert_eq!(result2.len(), 1);
    }
}
