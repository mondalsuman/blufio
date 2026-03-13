// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! IMAP client for incoming email polling.
//!
//! Spawns a tokio task that connects to an IMAP server via TLS, searches
//! for UNSEEN messages, parses them, maps threads to sessions, and sends
//! parsed messages through an mpsc channel.

use std::collections::HashMap;
use std::sync::Arc;

use blufio_config::model::EmailConfig;
use blufio_core::error::BlufioError;
use blufio_core::types::{InboundMessage, MessageContent};
use futures::TryStreamExt;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::parsing;

/// Build a rustls TLS connector with system/webpki roots.
fn build_tls_connector() -> Result<tokio_rustls::TlsConnector, BlufioError> {
    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let tls_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();

    Ok(tokio_rustls::TlsConnector::from(Arc::new(tls_config)))
}

/// Start the IMAP polling loop in a background tokio task.
///
/// Connects to the configured IMAP server, searches for UNSEEN messages,
/// parses them via [`parsing::parse_email_body`], maps email threads to
/// sessions via In-Reply-To/References headers, and sends them through
/// the provided `inbound_tx` channel. Messages are marked as `\Seen`
/// after processing.
///
/// On connection errors, retries with exponential backoff (5s up to 300s).
pub async fn start_imap_polling(
    config: EmailConfig,
    inbound_tx: mpsc::Sender<InboundMessage>,
) -> Result<tokio::task::JoinHandle<()>, BlufioError> {
    // Validate required fields.
    let imap_host = config
        .imap_host
        .clone()
        .ok_or_else(|| BlufioError::Config("email: imap_host is required".into()))?;
    let username = config
        .username
        .clone()
        .ok_or_else(|| BlufioError::Config("email: username is required".into()))?;
    let password = config
        .password
        .clone()
        .ok_or_else(|| BlufioError::Config("email: password is required".into()))?;

    let handle = tokio::spawn(async move {
        let mut backoff_secs: u64 = 5;
        let max_backoff: u64 = 300;

        // Thread tracking: message_id -> thread_id (session_id).
        let mut thread_map: HashMap<String, String> = HashMap::new();

        loop {
            match run_poll_cycle(
                &config,
                &imap_host,
                &username,
                &password,
                &inbound_tx,
                &mut thread_map,
            )
            .await
            {
                Ok(()) => {
                    // Successful cycle -- reset backoff.
                    backoff_secs = 5;
                }
                Err(e) => {
                    error!(
                        error = %e,
                        backoff_secs,
                        "IMAP polling cycle failed, retrying"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(max_backoff);
                    continue;
                }
            }

            // Sleep for the configured poll interval between cycles.
            tokio::time::sleep(std::time::Duration::from_secs(config.poll_interval_secs)).await;
        }
    });

    Ok(handle)
}

/// Run a single IMAP poll cycle: connect, search UNSEEN, fetch, parse, send.
async fn run_poll_cycle(
    config: &EmailConfig,
    imap_host: &str,
    username: &str,
    password: &str,
    inbound_tx: &mpsc::Sender<InboundMessage>,
    thread_map: &mut HashMap<String, String>,
) -> Result<(), BlufioError> {
    let port = config.imap_port.unwrap_or(993);
    let folders = if config.folders.is_empty() {
        vec!["INBOX".to_string()]
    } else {
        config.folders.clone()
    };

    // Connect via TCP.
    let tcp_stream = tokio::net::TcpStream::connect((imap_host, port))
        .await
        .map_err(|e| BlufioError::Config(format!("email: IMAP TCP connect failed: {e}")))?;

    // Wrap with TLS.
    let tls_connector = build_tls_connector()?;
    let server_name = rustls_pki_types::ServerName::try_from(imap_host.to_string())
        .map_err(|e| BlufioError::Config(format!("email: invalid IMAP server name: {e}")))?;
    let tls_stream = tls_connector
        .connect(server_name, tcp_stream)
        .await
        .map_err(|e| BlufioError::Config(format!("email: IMAP TLS handshake failed: {e}")))?;

    // Create IMAP client over TLS stream.
    let mut client = async_imap::Client::new(tls_stream);
    let _greeting = client
        .read_response()
        .await
        .map_err(|e| BlufioError::Config(format!("email: IMAP greeting failed: {e}")))?;

    // Login.
    let mut session = client
        .login(username, password)
        .await
        .map_err(|(e, _)| BlufioError::Config(format!("email: IMAP login failed: {e}")))?;

    for folder in &folders {
        debug!(folder, "selecting IMAP folder");
        session.select(folder).await.map_err(|e| {
            BlufioError::Config(format!("email: IMAP select '{folder}' failed: {e}"))
        })?;

        // Search for UNSEEN messages.
        let unseen = session
            .search("UNSEEN")
            .await
            .map_err(|e| BlufioError::Config(format!("email: IMAP search UNSEEN failed: {e}")))?;

        if unseen.is_empty() {
            debug!(folder, "no unseen messages");
            continue;
        }

        info!(folder, count = unseen.len(), "found unseen messages");

        // Build sequence set from unseen message numbers.
        let seq_set: String = unseen
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join(",");

        // Fetch full RFC822 body for each.
        let messages_stream = session
            .fetch(&seq_set, "RFC822")
            .await
            .map_err(|e| BlufioError::Config(format!("email: IMAP fetch failed: {e}")))?;

        let messages: Vec<_> = messages_stream
            .try_collect()
            .await
            .map_err(|e| BlufioError::Config(format!("email: IMAP fetch collect failed: {e}")))?;

        for msg in &messages {
            let Some(body) = msg.body() else {
                warn!("fetched message has no body, skipping");
                continue;
            };

            let Some(parsed) = parsing::parse_email_body(body) else {
                warn!("failed to parse email body, skipping");
                continue;
            };

            // Check allowed_senders filter.
            if !config.allowed_senders.is_empty()
                && !config
                    .allowed_senders
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(&parsed.from))
            {
                debug!(from = %parsed.from, "sender not in allowed_senders, skipping");
                continue;
            }

            // Thread-to-session mapping.
            let thread_id = determine_thread_id(&parsed, thread_map);

            // Register this message's ID in the thread map.
            if let Some(ref mid) = parsed.message_id {
                thread_map.insert(mid.clone(), thread_id.clone());
            }

            // Build metadata JSON.
            let metadata = serde_json::json!({
                "subject": parsed.subject,
                "message_id": parsed.message_id,
                "in_reply_to": parsed.in_reply_to,
                "thread_id": thread_id,
                "from": parsed.from,
                "date": parsed.date,
            });

            let inbound = InboundMessage {
                id: parsed
                    .message_id
                    .clone()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                session_id: Some(thread_id),
                channel: "email".to_string(),
                sender_id: parsed.from.clone(),
                content: MessageContent::Text(parsed.body),
                timestamp: parsed
                    .date
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                metadata: Some(metadata.to_string()),
            };

            if let Err(e) = inbound_tx.send(inbound).await {
                warn!(error = %e, "failed to send inbound email message");
            }
        }

        // Mark messages as \Seen.
        let store_stream = session
            .store(&seq_set, "+FLAGS (\\Seen)")
            .await
            .map_err(|e| BlufioError::Config(format!("email: IMAP store \\Seen failed: {e}")))?;
        // Consume the store response stream.
        let _: Vec<_> = store_stream
            .try_collect()
            .await
            .map_err(|e| BlufioError::Config(format!("email: IMAP store collect failed: {e}")))?;

        debug!(folder, count = unseen.len(), "marked messages as seen");
    }

    // Logout gracefully.
    if let Err(e) = session.logout().await {
        warn!(error = %e, "IMAP logout failed");
    }

    Ok(())
}

/// Determine the thread/session ID for a parsed email.
///
/// Uses In-Reply-To and References headers to look up an existing thread.
/// If no existing thread is found, creates a new thread ID.
fn determine_thread_id(
    parsed: &parsing::ParsedEmail,
    thread_map: &HashMap<String, String>,
) -> String {
    // Check In-Reply-To first.
    if let Some(ref irt) = parsed.in_reply_to
        && let Some(tid) = thread_map.get(irt)
    {
        return tid.clone();
    }

    // Check References (last reference is most recent parent).
    for reference in parsed.references.iter().rev() {
        if let Some(tid) = thread_map.get(reference) {
            return tid.clone();
        }
    }

    // New thread -- generate a new session ID.
    format!("email:{}", uuid::Uuid::new_v4())
}
