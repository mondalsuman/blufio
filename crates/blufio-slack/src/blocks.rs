// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Block Kit message builders for structured Slack content.
//!
//! Uses serde_json::json! macros for Block Kit structure rather than
//! typed slack-morphism Block Kit types for cleaner, simpler code.

use serde_json::json;

/// Build Block Kit blocks for bot status display.
pub fn build_status_blocks() -> serde_json::Value {
    json!([
        {
            "type": "header",
            "text": {
                "type": "plain_text",
                "text": "Blufio Status",
                "emoji": true
            }
        },
        {
            "type": "section",
            "fields": [
                {
                    "type": "mrkdwn",
                    "text": "*Status:*\nOnline"
                },
                {
                    "type": "mrkdwn",
                    "text": format!("*Version:*\n{}", env!("CARGO_PKG_VERSION"))
                }
            ]
        },
        {
            "type": "divider"
        },
        {
            "type": "context",
            "elements": [
                {
                    "type": "mrkdwn",
                    "text": "AI assistant is online and ready."
                }
            ]
        }
    ])
}

/// Build Block Kit blocks for help display.
pub fn build_help_blocks() -> serde_json::Value {
    json!([
        {
            "type": "header",
            "text": {
                "type": "plain_text",
                "text": "Blufio Help",
                "emoji": true
            }
        },
        {
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": "Your AI assistant on Slack."
            }
        },
        {
            "type": "divider"
        },
        {
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": "*`/blufio status`*\nCheck if Blufio is online and ready"
            }
        },
        {
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": "*`/blufio help`*\nShow this help message"
            }
        },
        {
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": "*`/blufio <message>`*\nSend a message to Blufio"
            }
        },
        {
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": "*@Blufio <message>*\nMention Blufio in a channel to chat"
            }
        },
        {
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": "*Direct Message*\nSend a DM to chat privately"
            }
        }
    ])
}

/// Build Block Kit blocks for error display.
pub fn build_error_blocks(error: &str) -> serde_json::Value {
    json!([
        {
            "type": "section",
            "text": {
                "type": "mrkdwn",
                "text": format!(":warning: *Error:* {}", error)
            }
        }
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_blocks_has_header() {
        let blocks = build_status_blocks();
        let arr = blocks.as_array().unwrap();
        assert_eq!(arr[0]["type"], "header");
        assert_eq!(arr[0]["text"]["text"], "Blufio Status");
    }

    #[test]
    fn status_blocks_has_fields() {
        let blocks = build_status_blocks();
        let arr = blocks.as_array().unwrap();
        let fields = arr[1]["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 2);
        assert!(fields[0]["text"].as_str().unwrap().contains("Status"));
        assert!(fields[1]["text"].as_str().unwrap().contains("Version"));
    }

    #[test]
    fn help_blocks_has_header() {
        let blocks = build_help_blocks();
        let arr = blocks.as_array().unwrap();
        assert_eq!(arr[0]["type"], "header");
        assert_eq!(arr[0]["text"]["text"], "Blufio Help");
    }

    #[test]
    fn help_blocks_has_commands() {
        let blocks = build_help_blocks();
        let arr = blocks.as_array().unwrap();
        // Should have header + description + divider + 5 command sections
        assert!(arr.len() >= 5);
    }

    #[test]
    fn error_blocks_contains_message() {
        let blocks = build_error_blocks("something went wrong");
        let arr = blocks.as_array().unwrap();
        let text = arr[0]["text"]["text"].as_str().unwrap();
        assert!(text.contains("something went wrong"));
        assert!(text.contains("Error"));
    }
}
