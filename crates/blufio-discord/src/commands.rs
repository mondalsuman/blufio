// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Discord slash command registration and handling.
//!
//! Registers the `/blufio` command with subcommands: status, help, chat.

use blufio_core::types::{InboundMessage, MessageContent};
use serenity::all::{
    CommandDataOptionValue, CommandInteraction, CommandOptionType, Context, CreateCommand,
    CreateCommandOption, CreateEmbed, CreateInteractionResponse, CreateInteractionResponseMessage,
    Interaction,
};
use tokio::sync::mpsc;
use tracing::{debug, error};

/// Discord blurple color.
const BLURPLE: u32 = 0x5865F2;

/// Register the /blufio slash command globally.
pub async fn register_commands(ctx: &Context) {
    let command = CreateCommand::new("blufio")
        .description("Interact with Blufio AI assistant")
        .add_option(
            CreateCommandOption::new(CommandOptionType::SubCommand, "status", "Show bot status"),
        )
        .add_option(
            CreateCommandOption::new(CommandOptionType::SubCommand, "help", "Show help information"),
        )
        .add_option(
            CreateCommandOption::new(CommandOptionType::SubCommand, "chat", "Chat with Blufio")
                .add_sub_option(
                    CreateCommandOption::new(
                        CommandOptionType::String,
                        "message",
                        "Your message to Blufio",
                    )
                    .required(true),
                ),
        );

    if let Err(e) = serenity::all::Command::create_global_command(&ctx.http, command).await {
        error!(error = %e, "failed to register /blufio slash command");
    } else {
        debug!("registered /blufio slash command globally");
    }
}

/// Handle incoming slash command interactions.
pub async fn handle_interaction(
    ctx: &Context,
    interaction: &Interaction,
    inbound_tx: &mpsc::Sender<InboundMessage>,
) {
    let Interaction::Command(cmd) = interaction else {
        return;
    };

    if cmd.data.name != "blufio" {
        return;
    }

    // Find the subcommand.
    let subcommand = cmd.data.options.first().map(|o| o.name.as_str());

    match subcommand {
        Some("status") => handle_status(ctx, cmd).await,
        Some("help") => handle_help(ctx, cmd).await,
        Some("chat") => handle_chat(ctx, cmd, inbound_tx).await,
        _ => handle_help(ctx, cmd).await,
    }
}

async fn handle_status(ctx: &Context, cmd: &CommandInteraction) {
    let embed = CreateEmbed::new()
        .title("Blufio Status")
        .description("AI assistant is online and ready.")
        .field("Status", "Online", true)
        .field("Version", env!("CARGO_PKG_VERSION"), true)
        .color(BLURPLE);

    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .embed(embed)
            .ephemeral(true),
    );

    if let Err(e) = cmd.create_response(&ctx.http, response).await {
        error!(error = %e, "failed to respond to status command");
    }
}

async fn handle_help(ctx: &Context, cmd: &CommandInteraction) {
    let embed = CreateEmbed::new()
        .title("Blufio Help")
        .description("Your AI assistant on Discord.")
        .field(
            "/blufio status",
            "Check if Blufio is online and ready",
            false,
        )
        .field(
            "/blufio help",
            "Show this help message",
            false,
        )
        .field(
            "/blufio chat <message>",
            "Send a message to Blufio",
            false,
        )
        .field(
            "@Blufio <message>",
            "Mention Blufio in a channel to chat",
            false,
        )
        .field("DM", "Send a direct message to chat privately", false)
        .color(BLURPLE);

    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .embed(embed)
            .ephemeral(true),
    );

    if let Err(e) = cmd.create_response(&ctx.http, response).await {
        error!(error = %e, "failed to respond to help command");
    }
}

async fn handle_chat(
    ctx: &Context,
    cmd: &CommandInteraction,
    inbound_tx: &mpsc::Sender<InboundMessage>,
) {
    // Extract the message option from the "chat" subcommand.
    // In serenity 0.12, subcommand options are accessed via
    // CommandDataOptionValue::SubCommand(Vec<CommandDataOption>).
    let message_text = cmd
        .data
        .options
        .first()
        .and_then(|sub| {
            if let CommandDataOptionValue::SubCommand(ref opts) = sub.value {
                opts.iter()
                    .find(|o| o.name == "message")
                    .and_then(|o| o.value.as_str())
            } else {
                sub.value.as_str()
            }
        })
        .unwrap_or("hello");

    let inbound = InboundMessage {
        id: cmd.id.to_string(),
        session_id: None,
        channel: "discord".to_string(),
        sender_id: cmd.user.id.to_string(),
        content: MessageContent::Text(message_text.to_string()),
        timestamp: cmd.id.created_at().to_string(),
        metadata: Some(
            serde_json::json!({
                "channel_id": cmd.channel_id.to_string(),
                "guild_id": cmd.guild_id.map(|g| g.to_string()),
                "chat_id": cmd.channel_id.to_string(),
                "from_slash_command": true,
            })
            .to_string(),
        ),
    };

    if inbound_tx.send(inbound).await.is_err() {
        error!("inbound channel closed, cannot forward slash command message");
    }

    // Acknowledge the command.
    let response = CreateInteractionResponse::Message(
        CreateInteractionResponseMessage::new()
            .content("Processing your message...")
            .ephemeral(true),
    );

    if let Err(e) = cmd.create_response(&ctx.http, response).await {
        error!(error = %e, "failed to acknowledge chat command");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blurple_color_is_correct() {
        assert_eq!(BLURPLE, 0x5865F2);
    }
}
