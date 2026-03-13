// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Channel adapter initialization for `blufio serve`.
//!
//! Each channel adapter is feature-gated and initialized based on user
//! configuration. Adapters are added to the ChannelMultiplexer for
//! multi-channel support.

use std::sync::Arc;

use blufio_agent::ChannelMultiplexer;
use blufio_config::model::BlufioConfig;
use blufio_core::error::BlufioError;
use tracing::info;

#[cfg(feature = "telegram")]
use blufio_telegram::TelegramChannel;

#[cfg(feature = "discord")]
use blufio_discord::DiscordChannel;

#[cfg(feature = "slack")]
use blufio_slack::SlackChannel;

#[cfg(feature = "whatsapp")]
use blufio_whatsapp::{WhatsAppCloudChannel, webhook::WhatsAppWebhookState};

#[cfg(feature = "signal")]
use blufio_signal::SignalChannel;

#[cfg(feature = "irc")]
use blufio_irc::IrcChannel;

#[cfg(feature = "matrix")]
use blufio_matrix::MatrixChannel;

#[cfg(feature = "email")]
use blufio_email::EmailChannel;

#[cfg(feature = "imessage")]
use blufio_imessage::{IMessageChannel, webhook::IMessageWebhookState};

#[cfg(feature = "sms")]
use blufio_sms::{SmsChannel, webhook::SmsWebhookState};

/// Result of channel initialization, carrying webhook states needed by
/// the gateway for mounting webhook routes.
pub(crate) struct ChannelInitResult {
    pub mux: ChannelMultiplexer,
    #[cfg(feature = "whatsapp")]
    pub whatsapp_webhook_state: Option<WhatsAppWebhookState>,
    #[cfg(feature = "imessage")]
    pub imessage_webhook_state: Option<IMessageWebhookState>,
    #[cfg(not(feature = "imessage"))]
    pub imessage_webhook_state: Option<()>,
    #[cfg(feature = "sms")]
    pub sms_webhook_state: Option<SmsWebhookState>,
    #[cfg(not(feature = "sms"))]
    pub sms_webhook_state: Option<()>,
}

/// Initialize all channel adapters and add them to the multiplexer.
///
/// Returns the populated multiplexer and any webhook states needed by gateway.
pub(crate) fn init_channels(
    config: &BlufioConfig,
    event_bus: &Arc<blufio_bus::EventBus>,
    vault_values: &std::sync::Arc<std::sync::RwLock<Vec<String>>>,
) -> Result<ChannelInitResult, BlufioError> {
    let mut mux = ChannelMultiplexer::new();
    mux.set_event_bus(event_bus.clone());

    // --- Telegram ---
    #[cfg(feature = "telegram")]
    {
        if config.telegram.bot_token.is_some() {
            let telegram = TelegramChannel::new(config.telegram.clone()).map_err(|e| {
                tracing::error!(error = %e, "failed to initialize Telegram channel");
                eprintln!(
                    "error: Telegram bot token required. Set via: config or `blufio config set-secret telegram.bot_token`"
                );
                e
            })?;
            mux.add_channel("telegram".to_string(), Box::new(telegram));
            info!("telegram channel added to multiplexer");
        } else {
            info!("telegram channel skipped (no bot_token configured)");
        }
    }

    // --- Discord ---
    #[cfg(feature = "discord")]
    {
        if config.discord.bot_token.is_some() {
            let discord = DiscordChannel::new(config.discord.clone()).map_err(|e| {
                tracing::error!(error = %e, "failed to initialize Discord channel");
                eprintln!(
                    "error: Discord bot token required. Set via: config or `blufio config set-secret discord.bot_token`"
                );
                e
            })?;
            mux.add_channel("discord".to_string(), Box::new(discord));
            info!("discord channel added to multiplexer");

            // Redact Discord token in logs.
            if let Some(ref token) = config.discord.bot_token {
                blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                    vault_values,
                    token.clone(),
                );
            }
        } else {
            info!("discord channel skipped (no bot_token configured)");
        }
    }

    // --- Slack ---
    #[cfg(feature = "slack")]
    {
        if config.slack.bot_token.is_some() && config.slack.app_token.is_some() {
            let slack = SlackChannel::new(config.slack.clone()).map_err(|e| {
                tracing::error!(error = %e, "failed to initialize Slack channel");
                eprintln!(
                    "error: Slack bot_token and app_token required for Socket Mode. \
                     Set via: config or `blufio config set-secret slack.bot_token`"
                );
                e
            })?;
            mux.add_channel("slack".to_string(), Box::new(slack));
            info!("slack channel added to multiplexer");

            // Redact Slack tokens in logs.
            if let Some(ref token) = config.slack.bot_token {
                blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                    vault_values,
                    token.clone(),
                );
            }
            if let Some(ref token) = config.slack.app_token {
                blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                    vault_values,
                    token.clone(),
                );
            }
        } else {
            info!("slack channel skipped (bot_token and/or app_token not configured)");
        }
    }

    // --- WhatsApp ---
    #[cfg(feature = "whatsapp")]
    let whatsapp_webhook_state: Option<WhatsAppWebhookState> = {
        if config.whatsapp.phone_number_id.is_some() {
            let whatsapp = WhatsAppCloudChannel::new(config.whatsapp.clone()).map_err(|e| {
                tracing::error!(error = %e, "failed to initialize WhatsApp channel");
                e
            })?;

            // Capture inbound_tx and webhook state before moving the adapter.
            let inbound_tx = whatsapp.inbound_tx();
            let webhook_state = WhatsAppWebhookState {
                inbound_tx,
                verify_token: config.whatsapp.verify_token.clone().unwrap_or_default(),
                app_secret: config.whatsapp.app_secret.clone().unwrap_or_default(),
            };

            mux.add_channel("whatsapp".to_string(), Box::new(whatsapp));
            info!("whatsapp channel added to multiplexer");

            // Redact access token in logs.
            if let Some(ref token) = config.whatsapp.access_token {
                blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                    vault_values,
                    token.clone(),
                );
            }

            Some(webhook_state)
        } else {
            info!("whatsapp channel skipped (no phone_number_id configured)");
            None
        }
    };

    // --- Signal ---
    #[cfg(feature = "signal")]
    {
        if config.signal.socket_path.is_some() || config.signal.host.is_some() {
            let signal = SignalChannel::new(config.signal.clone()).map_err(|e| {
                tracing::error!(error = %e, "failed to initialize Signal channel");
                eprintln!(
                    "error: Signal requires a running signal-cli daemon. \
                     Configure socket_path or host:port in [signal] config section"
                );
                e
            })?;
            mux.add_channel("signal".to_string(), Box::new(signal));
            info!("signal channel added to multiplexer");
        } else {
            info!("signal channel skipped (no socket_path or host configured)");
        }
    }

    // --- IRC ---
    #[cfg(feature = "irc")]
    {
        if config.irc.server.is_some() {
            let irc = IrcChannel::new(config.irc.clone()).map_err(|e| {
                tracing::error!(error = %e, "failed to initialize IRC channel");
                eprintln!(
                    "error: IRC server configuration required. \
                     Set server and nickname in [irc] config section"
                );
                e
            })?;
            mux.add_channel("irc".to_string(), Box::new(irc));
            info!("irc channel added to multiplexer");

            // Redact IRC password in logs.
            if let Some(ref password) = config.irc.password {
                blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                    vault_values,
                    password.clone(),
                );
            }
        } else {
            info!("irc channel skipped (no server configured)");
        }
    }

    // --- Matrix ---
    #[cfg(feature = "matrix")]
    {
        if config.matrix.homeserver_url.is_some() {
            let matrix = MatrixChannel::new(config.matrix.clone()).map_err(|e| {
                tracing::error!(error = %e, "failed to initialize Matrix channel");
                eprintln!(
                    "error: Matrix homeserver URL, username, and password required. \
                     Set in [matrix] config section"
                );
                e
            })?;
            mux.add_channel("matrix".to_string(), Box::new(matrix));
            info!("matrix channel added to multiplexer");

            // Redact Matrix password in logs.
            if let Some(ref password) = config.matrix.password {
                blufio_security::RedactingWriter::<std::io::Stderr>::add_vault_value(
                    vault_values,
                    password.clone(),
                );
            }
        } else {
            info!("matrix channel skipped (no homeserver_url configured)");
        }
    }

    // --- Email ---
    #[cfg(feature = "email")]
    {
        if config.email.imap_host.is_some() {
            let email = EmailChannel::new(config.email.clone()).map_err(|e| {
                tracing::error!(error = %e, "failed to initialize Email channel");
                e
            })?;
            mux.add_channel("email".to_string(), Box::new(email));
            info!("email channel added to multiplexer");
        } else {
            info!("email channel skipped (no imap_host configured)");
        }
    }

    // --- iMessage ---
    #[cfg(feature = "imessage")]
    let imessage_webhook_state: Option<IMessageWebhookState> = {
        if config.imessage.bluebubbles_url.is_some() {
            let imessage = IMessageChannel::new(config.imessage.clone()).map_err(|e| {
                tracing::error!(error = %e, "failed to initialize iMessage channel");
                e
            })?;
            let inbound_tx = imessage.inbound_tx();
            let webhook_state = IMessageWebhookState {
                inbound_tx,
                webhook_secret: config.imessage.webhook_secret.clone(),
                allowed_contacts: config.imessage.allowed_contacts.clone(),
                group_trigger: config
                    .imessage
                    .group_trigger
                    .clone()
                    .unwrap_or_else(|| "Blufio".to_string()),
            };
            mux.add_channel("imessage".to_string(), Box::new(imessage));
            info!("imessage channel added to multiplexer");
            Some(webhook_state)
        } else {
            info!("imessage channel skipped (no bluebubbles_url configured)");
            None
        }
    };
    #[cfg(not(feature = "imessage"))]
    let imessage_webhook_state: Option<()> = None;

    // --- SMS ---
    #[cfg(feature = "sms")]
    let sms_webhook_state: Option<SmsWebhookState> = {
        if config.sms.account_sid.is_some() {
            let sms = SmsChannel::new(config.sms.clone()).map_err(|e| {
                tracing::error!(error = %e, "failed to initialize SMS channel");
                e
            })?;
            let inbound_tx = sms.inbound_tx();
            let webhook_state = SmsWebhookState {
                inbound_tx,
                auth_token: config.sms.auth_token.clone().unwrap_or_default(),
                webhook_url: config.sms.webhook_url.clone().unwrap_or_default(),
                allowed_numbers: config.sms.allowed_numbers.clone(),
            };
            mux.add_channel("sms".to_string(), Box::new(sms));
            info!("sms channel added to multiplexer");
            Some(webhook_state)
        } else {
            info!("sms channel skipped (no account_sid configured)");
            None
        }
    };
    #[cfg(not(feature = "sms"))]
    let sms_webhook_state: Option<()> = None;

    Ok(ChannelInitResult {
        mux,
        #[cfg(feature = "whatsapp")]
        whatsapp_webhook_state,
        imessage_webhook_state,
        sms_webhook_state,
    })
}
