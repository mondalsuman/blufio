// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Prompt template definitions for the MCP server.
//!
//! Provides three built-in prompt templates:
//! - **summarize-conversation**: Summarize a conversation session (requires `session_id`)
//! - **search-memory**: Search long-term memory (requires `query`)
//! - **explain-skill**: Explain a Blufio skill (requires `skill_name`)
//!
//! These templates are returned via `prompts/list` and instantiated via `prompts/get`.

use std::collections::HashMap;

/// Definition of a prompt template.
#[derive(Debug)]
pub struct PromptDef {
    pub name: String,
    pub description: String,
    pub arguments: Vec<PromptArgDef>,
}

/// Definition of a prompt argument.
#[derive(Debug)]
pub struct PromptArgDef {
    pub name: String,
    pub description: String,
    pub required: bool,
}

/// A message in a prompt conversation.
#[derive(Debug)]
pub struct PromptMessageDef {
    /// "assistant" (for system-level instructions) or "user".
    pub role: String,
    pub content: String,
}

/// Returns the list of available prompt definitions.
pub fn list_prompt_definitions() -> Vec<PromptDef> {
    vec![
        PromptDef {
            name: "summarize-conversation".to_string(),
            description: "Summarize a conversation session".to_string(),
            arguments: vec![PromptArgDef {
                name: "session_id".to_string(),
                description: "Session ID to summarize".to_string(),
                required: true,
            }],
        },
        PromptDef {
            name: "search-memory".to_string(),
            description: "Search long-term memory".to_string(),
            arguments: vec![PromptArgDef {
                name: "query".to_string(),
                description: "Search query".to_string(),
                required: true,
            }],
        },
        PromptDef {
            name: "explain-skill".to_string(),
            description: "Explain a Blufio skill".to_string(),
            arguments: vec![PromptArgDef {
                name: "skill_name".to_string(),
                description: "Name of the skill to explain".to_string(),
                required: true,
            }],
        },
    ]
}

/// Generates prompt messages for the given prompt name and arguments.
///
/// Returns an error if the prompt name is unknown or required arguments are missing.
pub fn get_prompt_messages(
    name: &str,
    arguments: &HashMap<String, String>,
) -> Result<Vec<PromptMessageDef>, String> {
    match name {
        "summarize-conversation" => {
            let session_id = require_arg(arguments, "session_id")?;
            Ok(vec![
                PromptMessageDef {
                    role: "assistant".to_string(),
                    content: "You are a helpful assistant that summarizes conversations concisely."
                        .to_string(),
                },
                PromptMessageDef {
                    role: "user".to_string(),
                    content: format!(
                        "Summarize the conversation from session {session_id}. \
                         Focus on key decisions, questions answered, and action items."
                    ),
                },
            ])
        }
        "search-memory" => {
            let query = require_arg(arguments, "query")?;
            Ok(vec![
                PromptMessageDef {
                    role: "assistant".to_string(),
                    content: "You are a memory search assistant. Help the user find relevant \
                              information from their long-term memory."
                        .to_string(),
                },
                PromptMessageDef {
                    role: "user".to_string(),
                    content: format!("Search my memory for information about: {query}"),
                },
            ])
        }
        "explain-skill" => {
            let skill_name = require_arg(arguments, "skill_name")?;
            Ok(vec![
                PromptMessageDef {
                    role: "assistant".to_string(),
                    content: "You are a Blufio skill documentation assistant.".to_string(),
                },
                PromptMessageDef {
                    role: "user".to_string(),
                    content: format!(
                        "Explain what the '{skill_name}' skill does, what parameters it accepts, \
                         and give an example of how to use it."
                    ),
                },
            ])
        }
        _ => Err(format!("unknown prompt: {name}")),
    }
}

/// Extracts a required argument, returning an error if missing.
fn require_arg<'a>(arguments: &'a HashMap<String, String>, name: &str) -> Result<&'a str, String> {
    arguments
        .get(name)
        .map(|s| s.as_str())
        .ok_or_else(|| format!("Missing required argument: {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── list_prompt_definitions tests ──────────────────────────────

    #[test]
    fn list_returns_three_prompts() {
        let defs = list_prompt_definitions();
        assert_eq!(defs.len(), 3);
    }

    #[test]
    fn list_contains_summarize_conversation() {
        let defs = list_prompt_definitions();
        let found = defs.iter().find(|d| d.name == "summarize-conversation");
        assert!(found.is_some(), "summarize-conversation prompt not found");
        let def = found.unwrap();
        assert!(!def.description.is_empty());
        assert_eq!(def.arguments.len(), 1);
        assert_eq!(def.arguments[0].name, "session_id");
        assert!(def.arguments[0].required);
    }

    #[test]
    fn list_contains_search_memory() {
        let defs = list_prompt_definitions();
        let found = defs.iter().find(|d| d.name == "search-memory");
        assert!(found.is_some(), "search-memory prompt not found");
        let def = found.unwrap();
        assert!(!def.description.is_empty());
        assert_eq!(def.arguments.len(), 1);
        assert_eq!(def.arguments[0].name, "query");
        assert!(def.arguments[0].required);
    }

    #[test]
    fn list_contains_explain_skill() {
        let defs = list_prompt_definitions();
        let found = defs.iter().find(|d| d.name == "explain-skill");
        assert!(found.is_some(), "explain-skill prompt not found");
        let def = found.unwrap();
        assert!(!def.description.is_empty());
        assert_eq!(def.arguments.len(), 1);
        assert_eq!(def.arguments[0].name, "skill_name");
        assert!(def.arguments[0].required);
    }

    // ── get_prompt_messages tests ──────────────────────────────────

    #[test]
    fn summarize_conversation_returns_messages_with_session_id() {
        let mut args = HashMap::new();
        args.insert("session_id".to_string(), "sess-1".to_string());
        let messages = get_prompt_messages("summarize-conversation", &args).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "assistant");
        assert!(messages[0].content.contains("summarize"));
        assert_eq!(messages[1].role, "user");
        assert!(messages[1].content.contains("sess-1"));
    }

    #[test]
    fn search_memory_returns_messages_with_query() {
        let mut args = HashMap::new();
        args.insert("query".to_string(), "test".to_string());
        let messages = get_prompt_messages("search-memory", &args).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "assistant");
        assert_eq!(messages[1].role, "user");
        assert!(messages[1].content.contains("test"));
    }

    #[test]
    fn explain_skill_returns_messages_with_skill_name() {
        let mut args = HashMap::new();
        args.insert("skill_name".to_string(), "http".to_string());
        let messages = get_prompt_messages("explain-skill", &args).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "assistant");
        assert_eq!(messages[1].role, "user");
        assert!(messages[1].content.contains("http"));
    }

    #[test]
    fn unknown_prompt_returns_error() {
        let args = HashMap::new();
        let result = get_prompt_messages("nonexistent", &args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("nonexistent"));
    }

    #[test]
    fn missing_required_argument_returns_error() {
        let args = HashMap::new();
        let result = get_prompt_messages("summarize-conversation", &args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("session_id"));
    }

    #[test]
    fn search_memory_missing_query_returns_error() {
        let args = HashMap::new();
        let result = get_prompt_messages("search-memory", &args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("query"));
    }

    #[test]
    fn explain_skill_missing_skill_name_returns_error() {
        let args = HashMap::new();
        let result = get_prompt_messages("explain-skill", &args);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("skill_name"));
    }
}
