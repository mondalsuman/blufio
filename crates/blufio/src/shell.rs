// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! `blufio shell` command implementation.
//!
//! Launches an interactive REPL with colored prompt, streaming output,
//! and readline history. Creates a new session per invocation.

use std::sync::Arc;

use blufio_agent::context;
use blufio_anthropic::AnthropicProvider;
use blufio_config::model::BlufioConfig;
use blufio_core::error::BlufioError;
use blufio_core::types::{
    InboundMessage, Message, MessageContent, Session, StreamEventType, TokenUsage,
};
use blufio_core::{ProviderAdapter, StorageAdapter};
use blufio_storage::SqliteStorage;
use colored::Colorize;
use futures::StreamExt;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use tracing::debug;

/// Runs the `blufio shell` interactive REPL.
///
/// Creates a CLI session, prompts for user input, and streams LLM responses
/// directly to stdout.
pub async fn run_shell(config: BlufioConfig) -> Result<(), BlufioError> {
    // Initialize storage.
    let storage = SqliteStorage::new(config.storage.clone());
    storage.initialize().await?;
    let storage: Arc<dyn StorageAdapter + Send + Sync> = Arc::new(storage);

    // Initialize Anthropic provider.
    let provider: Arc<dyn ProviderAdapter + Send + Sync> =
        Arc::new(AnthropicProvider::new(&config).await.inspect_err(|_| {
            eprintln!(
                "error: Anthropic API key required. Set via: config, ANTHROPIC_API_KEY env var, or `blufio config set-secret anthropic.api_key`"
            );
        })?);

    // Load system prompt.
    let system_prompt = context::load_system_prompt(&config.agent).await?;

    // Create a new CLI session.
    let session_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let session = Session {
        id: session_id.clone(),
        channel: "cli".to_string(),
        user_id: Some("local".to_string()),
        state: "active".to_string(),
        metadata: None,
        created_at: now.clone(),
        updated_at: now,
    };
    storage.create_session(&session).await?;

    // Set up readline editor.
    let mut rl = DefaultEditor::new().map_err(|e| {
        BlufioError::Internal(format!("failed to initialize readline: {e}"))
    })?;

    // Print welcome message.
    println!("{}", "blufio shell".bold().green());
    println!("Type {} to exit.\n", "/quit".yellow());

    // REPL loop.
    let prompt = format!("{}> ", "blufio".green());
    loop {
        match rl.readline(&prompt) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed == "/quit" || trimmed == "/exit" {
                    break;
                }
                if trimmed.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(&line);

                // Process the message.
                if let Err(e) = handle_shell_message(
                    &config,
                    storage.as_ref(),
                    provider.as_ref(),
                    &session_id,
                    &system_prompt,
                    trimmed,
                )
                .await
                {
                    eprintln!("{}: {e}", "error".red());
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C
                break;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D
                break;
            }
            Err(e) => {
                eprintln!("{}: {e}", "error".red());
                break;
            }
        }
    }

    // Clean up: close session.
    storage
        .update_session_state(&session_id, "closed")
        .await?;
    storage.close().await?;

    println!("{}", "goodbye".dimmed());
    Ok(())
}

/// Handles a single shell message: persists, calls LLM, streams output.
async fn handle_shell_message(
    config: &BlufioConfig,
    storage: &dyn StorageAdapter,
    provider: &dyn ProviderAdapter,
    session_id: &str,
    system_prompt: &str,
    input: &str,
) -> Result<(), BlufioError> {
    // Persist user message.
    let now = chrono::Utc::now().to_rfc3339();
    let user_msg = Message {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        role: "user".to_string(),
        content: input.to_string(),
        token_count: None,
        metadata: None,
        created_at: now,
    };
    storage.insert_message(&user_msg).await?;

    // Create InboundMessage for context assembly.
    let inbound = InboundMessage {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: Some(session_id.to_string()),
        channel: "cli".to_string(),
        sender_id: "local".to_string(),
        content: MessageContent::Text(input.to_string()),
        timestamp: chrono::Utc::now().to_rfc3339(),
        metadata: None,
    };

    // Assemble context.
    let request = context::assemble_context(
        storage,
        session_id,
        system_prompt,
        &inbound,
        &config.anthropic.default_model,
        config.anthropic.max_tokens,
    )
    .await?;

    // Stream the response.
    let mut stream = provider.stream(request).await?;
    let mut full_response = String::new();
    let mut usage: Option<TokenUsage> = None;

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => match chunk.event_type {
                StreamEventType::ContentBlockDelta => {
                    if let Some(text) = &chunk.text {
                        print!("{text}");
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                        full_response.push_str(text);
                    }
                }
                StreamEventType::MessageStart | StreamEventType::MessageDelta => {
                    if let Some(u) = chunk.usage {
                        usage = Some(u);
                    }
                }
                StreamEventType::MessageStop => {
                    break;
                }
                StreamEventType::Error => {
                    if let Some(err) = &chunk.error {
                        eprintln!("\n{}: {err}", "error".red());
                    }
                    break;
                }
                _ => {}
            },
            Err(e) => {
                eprintln!("\n{}: {e}", "error".red());
                break;
            }
        }
    }

    // Print newline after response.
    println!();

    // Persist assistant response.
    let now = chrono::Utc::now().to_rfc3339();
    let assistant_msg = Message {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        role: "assistant".to_string(),
        content: full_response,
        token_count: usage.as_ref().map(|u| i64::from(u.output_tokens)),
        metadata: None,
        created_at: now,
    };
    storage.insert_message(&assistant_msg).await?;

    // Log token usage.
    if let Some(u) = &usage {
        debug!(
            input_tokens = u.input_tokens,
            output_tokens = u.output_tokens,
            "shell response complete"
        );
    }

    Ok(())
}
