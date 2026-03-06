// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Centralized content formatting and degradation pipeline.
//!
//! The [`FormatPipeline`] takes [`RichContent`] and degrades it based on
//! [`ChannelCapabilities`], producing [`FormattedOutput`] suitable for
//! channel-specific rendering.

use crate::types::ChannelCapabilities;

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
    Image { url: String, caption: Option<String> },
    /// Code block with optional language tag.
    CodeBlock {
        language: Option<String>,
        code: String,
    },
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
    Image { url: String, caption: Option<String> },
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        }
    }

    fn caps_minimal() -> ChannelCapabilities {
        ChannelCapabilities {
            supports_edit: false,
            supports_typing: false,
            supports_images: false,
            supports_documents: false,
            supports_voice: false,
            max_message_length: Some(4096),
            supports_embeds: false,
            supports_reactions: false,
            supports_threads: false,
        }
    }

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
}
