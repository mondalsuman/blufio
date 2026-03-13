// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! SMTP client for outgoing email delivery via lettre.
//!
//! Builds an async SMTP transport and sends multipart/alternative
//! (HTML + plaintext) email replies with proper In-Reply-To and
//! References threading headers.

use blufio_config::model::EmailConfig;
use blufio_core::error::BlufioError;
use lettre::message::{Mailbox, Message, MultiPart, SinglePart, header::ContentType};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

/// Build an async SMTP transport from the email configuration.
///
/// Uses `smtp_host` (falls back to `imap_host`), `smtp_port` (default 587),
/// and separate SMTP credentials if configured.
pub async fn build_smtp_transport(
    config: &EmailConfig,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, BlufioError> {
    let host = config
        .smtp_host
        .as_deref()
        .or(config.imap_host.as_deref())
        .ok_or_else(|| BlufioError::Config("email: smtp_host or imap_host is required".into()))?;

    let username = config
        .smtp_username
        .as_deref()
        .or(config.username.as_deref())
        .ok_or_else(|| {
            BlufioError::Config("email: smtp_username or username is required".into())
        })?;

    let password = config
        .smtp_password
        .as_deref()
        .or(config.password.as_deref())
        .ok_or_else(|| {
            BlufioError::Config("email: smtp_password or password is required".into())
        })?;

    let credentials = Credentials::new(username.to_string(), password.to_string());

    let transport = if config.allow_insecure {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
            .port(config.smtp_port.unwrap_or(587))
            .credentials(credentials)
            .build()
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
            .map_err(|e| BlufioError::Config(format!("email SMTP TLS setup failed: {e}")))?
            .port(config.smtp_port.unwrap_or(587))
            .credentials(credentials)
            .build()
    };

    Ok(transport)
}

/// Ensure a subject line has the "Re: " prefix without doubling it.
fn ensure_re_prefix(subject: &str) -> String {
    if subject.starts_with("Re: ") {
        subject.to_string()
    } else {
        format!("Re: {subject}")
    }
}

/// Send an email reply via SMTP with multipart/alternative body.
///
/// Builds a proper threaded reply with In-Reply-To and References headers.
/// Appends the configured footer to both text and HTML bodies if set.
#[allow(clippy::too_many_arguments)]
pub async fn send_email_reply(
    transport: &AsyncSmtpTransport<Tokio1Executor>,
    config: &EmailConfig,
    to: &str,
    subject: &str,
    text_body: &str,
    html_body: &str,
    in_reply_to: Option<&str>,
    references: Option<&str>,
) -> Result<String, BlufioError> {
    let from_name = config.from_name.as_deref().unwrap_or("Blufio");
    let from_address = config
        .from_address
        .as_deref()
        .ok_or_else(|| BlufioError::Config("email: from_address is required".into()))?;

    let from_mailbox: Mailbox = format!("{from_name} <{from_address}>")
        .parse()
        .map_err(|e| BlufioError::Config(format!("email: invalid from address: {e}")))?;

    let to_mailbox: Mailbox = to
        .parse()
        .map_err(|e| BlufioError::Config(format!("email: invalid to address '{to}': {e}")))?;

    let reply_subject = ensure_re_prefix(subject);

    let mut builder = Message::builder()
        .from(from_mailbox)
        .to(to_mailbox)
        .subject(&reply_subject);

    if let Some(irt) = in_reply_to {
        builder = builder.in_reply_to(irt.to_string());
    }

    if let Some(refs) = references {
        builder = builder.references(refs.to_string());
    }

    // Append footer if configured.
    let final_text = if let Some(ref footer) = config.email_footer {
        format!("{text_body}\n\n{footer}")
    } else {
        text_body.to_string()
    };

    let final_html = if let Some(ref footer) = config.email_footer {
        format!("{html_body}<br><br><hr><p>{footer}</p>")
    } else {
        html_body.to_string()
    };

    let email = builder
        .multipart(
            MultiPart::alternative()
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_PLAIN)
                        .body(final_text),
                )
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_HTML)
                        .body(final_html),
                ),
        )
        .map_err(|e| BlufioError::Config(format!("email: failed to build message: {e}")))?;

    let response = transport
        .send(email)
        .await
        .map_err(|e| BlufioError::channel_delivery_failed("email", e))?;

    Ok(response.message().collect::<Vec<_>>().join(" ").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subject_no_double_re() {
        assert_eq!(ensure_re_prefix("Re: Help"), "Re: Help");
    }

    #[test]
    fn test_subject_adds_re() {
        assert_eq!(ensure_re_prefix("Help with X"), "Re: Help with X");
    }
}
