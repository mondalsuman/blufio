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
                let lang = language.as_deref().unwrap_or("");
                FormattedOutput::Text(format!("```{}\n{}\n```", lang, code))
            }
            RichContent::Table(table) => format_table(table, caps),
            RichContent::List(list) => format_list(list),
        }
    }
}

// ---------------------------------------------------------------------------
// Table formatting (3-tier degradation)
// ---------------------------------------------------------------------------

/// Format a table with 3-tier degradation based on channel capabilities.
fn format_table(table: &Table, caps: &ChannelCapabilities) -> FormattedOutput {
    if table.headers.is_empty() {
        return FormattedOutput::Text(String::new());
    }

    let text = if caps.supports_code_blocks {
        // Tier 1: Unicode box-drawing table wrapped in code fence
        format_table_unicode(table)
    } else if caps.formatting_support == FormattingSupport::FullMarkdown
        || caps.formatting_support == FormattingSupport::HTML
    {
        // Tier 2: GFM markdown table
        format_table_gfm(table)
    } else {
        // Tier 3: Key:value per row
        format_table_keyvalue(table)
    };

    FormattedOutput::Text(text)
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

/// Format a list (universal format, no degradation needed).
fn format_list(list: &List) -> FormattedOutput {
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
    fn table_html_no_code_blocks_uses_tier2() {
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
                // HTML with no code blocks uses Tier 2 (GFM) since it supports full formatting
                assert!(t.contains("| Name | Age |"));
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
}
