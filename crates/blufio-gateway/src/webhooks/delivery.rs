// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Webhook delivery engine with HMAC-SHA256 signing and exponential backoff.
//!
//! The delivery engine subscribes to the EventBus for relevant events,
//! maps them to webhook event types, and delivers payloads to registered
//! webhooks with retry logic and dead letter queue.

use std::sync::Arc;

use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::store::WebhookStore;
use super::{Webhook, WebhookPayload};

type HmacSha256 = Hmac<Sha256>;

/// Retry delays in seconds for webhook delivery (5 attempts total).
/// 1s, 5s, 25s, 2min, 10min
const RETRY_DELAYS: [u64; 5] = [1, 5, 25, 120, 600];

/// Sign a payload with HMAC-SHA256, returning the hex-encoded signature.
pub fn sign_payload(secret: &[u8], payload: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC can take key of any size");
    mac.update(payload);
    hex::encode(mac.finalize().into_bytes())
}

/// Deliver a webhook payload to a single endpoint.
///
/// Returns `Ok(status_code)` on HTTP response or `Err(error_message)` on failure.
pub async fn deliver_single(
    client: &reqwest::Client,
    webhook: &Webhook,
    payload: &WebhookPayload,
) -> Result<u16, String> {
    let body = serde_json::to_vec(payload).map_err(|e| format!("serialize error: {e}"))?;
    let signature = sign_payload(webhook.secret.as_bytes(), &body);

    let response = client
        .post(&webhook.url)
        .header("Content-Type", "application/json")
        .header("X-Webhook-Signature", &signature)
        .body(body)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("request error: {e}"))?;

    Ok(response.status().as_u16())
}

/// Deliver a webhook payload with exponential backoff retry.
///
/// Attempts delivery up to 5 times with delays of 1s, 5s, 25s, 2min, 10min.
/// On each attempt, publishes a `WebhookEvent::DeliveryAttempted` to the bus.
/// If all attempts fail, inserts the payload into the dead letter queue.
///
/// Returns `true` if delivery succeeded, `false` if all attempts failed.
pub async fn deliver_with_retry(
    client: &reqwest::Client,
    webhook: &Webhook,
    payload: &WebhookPayload,
    store: &WebhookStore,
    bus: Option<&blufio_bus::EventBus>,
) -> bool {
    let mut last_error = String::new();

    for (attempt, delay) in RETRY_DELAYS.iter().enumerate() {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(*delay)).await;
        }

        match deliver_single(client, webhook, payload).await {
            Ok(status) => {
                let success = (200..300).contains(&status);

                // Publish delivery attempt event.
                if let Some(bus) = bus {
                    bus.publish(blufio_bus::BusEvent::Webhook(
                        blufio_bus::WebhookEvent::DeliveryAttempted {
                            event_id: blufio_bus::new_event_id(),
                            timestamp: blufio_bus::now_timestamp(),
                            webhook_id: webhook.id.clone(),
                            status_code: status,
                            success,
                        },
                    ))
                    .await;
                }

                if success {
                    tracing::debug!(
                        webhook_id = %webhook.id,
                        status = status,
                        attempt = attempt + 1,
                        "webhook delivery succeeded"
                    );
                    return true;
                }

                last_error = format!("HTTP {status}");
                tracing::warn!(
                    webhook_id = %webhook.id,
                    status = status,
                    attempt = attempt + 1,
                    "webhook delivery failed, will retry"
                );
            }
            Err(e) => {
                // Publish delivery attempt event with failure.
                if let Some(bus) = bus {
                    bus.publish(blufio_bus::BusEvent::Webhook(
                        blufio_bus::WebhookEvent::DeliveryAttempted {
                            event_id: blufio_bus::new_event_id(),
                            timestamp: blufio_bus::now_timestamp(),
                            webhook_id: webhook.id.clone(),
                            status_code: 0,
                            success: false,
                        },
                    ))
                    .await;
                }

                last_error = e.clone();
                tracing::warn!(
                    webhook_id = %webhook.id,
                    error = %e,
                    attempt = attempt + 1,
                    "webhook delivery error, will retry"
                );
            }
        }
    }

    // All retries exhausted -- insert into dead letter queue.
    let payload_json = serde_json::to_string(payload).unwrap_or_default();
    if let Err(e) = store
        .insert_dead_letter(
            &webhook.id,
            &payload.event_type,
            &payload_json,
            &last_error,
            RETRY_DELAYS.len() as i64,
        )
        .await
    {
        tracing::error!(
            webhook_id = %webhook.id,
            error = %e,
            "failed to insert dead letter entry"
        );
    }

    tracing::error!(
        webhook_id = %webhook.id,
        last_error = %last_error,
        "webhook delivery exhausted all retries, stored in dead letter queue"
    );

    false
}

/// Run the webhook delivery background loop.
///
/// Subscribes to the EventBus for relevant events, maps them to webhook
/// event types, and spawns delivery tasks for each matching webhook.
pub async fn run_webhook_delivery(
    bus: Arc<blufio_bus::EventBus>,
    store: Arc<WebhookStore>,
    client: reqwest::Client,
) {
    let mut rx = bus.subscribe_reliable(256).await;

    tracing::info!("webhook delivery engine started");

    while let Some(event) = rx.recv().await {
        let (event_type, data) = match &event {
            blufio_bus::BusEvent::Channel(blufio_bus::ChannelEvent::MessageSent {
                event_id,
                timestamp,
                channel,
            }) => (
                super::event_types::CHAT_COMPLETED,
                serde_json::json!({
                    "event_id": event_id,
                    "timestamp": timestamp,
                    "channel": channel,
                }),
            ),
            blufio_bus::BusEvent::Skill(blufio_bus::SkillEvent::Completed {
                event_id,
                timestamp,
                skill_name,
                is_error,
            }) => (
                super::event_types::TOOL_INVOKED,
                serde_json::json!({
                    "event_id": event_id,
                    "timestamp": timestamp,
                    "skill_name": skill_name,
                    "is_error": is_error,
                }),
            ),
            blufio_bus::BusEvent::Batch(blufio_bus::BatchEvent::Completed {
                event_id,
                timestamp,
                batch_id,
                success_count,
                error_count,
            }) => (
                super::event_types::BATCH_COMPLETED,
                serde_json::json!({
                    "event_id": event_id,
                    "timestamp": timestamp,
                    "batch_id": batch_id,
                    "success_count": success_count,
                    "error_count": error_count,
                }),
            ),
            // Skip all other events.
            _ => continue,
        };

        // Find webhooks subscribed to this event type.
        let webhooks = match store.list_for_event(event_type).await {
            Ok(whs) => whs,
            Err(e) => {
                tracing::error!(error = %e, "failed to list webhooks for event");
                continue;
            }
        };

        if webhooks.is_empty() {
            continue;
        }

        let payload = WebhookPayload {
            event_type: event_type.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            data,
        };

        // Spawn a delivery task for each matching webhook.
        for webhook in webhooks {
            let client = client.clone();
            let payload = payload.clone();
            let store = Arc::clone(&store);
            let bus = Arc::clone(&bus);

            tokio::spawn(async move {
                deliver_with_retry(&client, &webhook, &payload, &store, Some(&bus)).await;
            });
        }
    }

    tracing::warn!("webhook delivery engine stopped -- event bus closed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_payload_deterministic() {
        let secret = b"test-secret";
        let payload = b"hello world";
        let sig1 = sign_payload(secret, payload);
        let sig2 = sign_payload(secret, payload);
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn sign_payload_different_secrets() {
        let payload = b"hello world";
        let sig1 = sign_payload(b"secret-1", payload);
        let sig2 = sign_payload(b"secret-2", payload);
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn sign_payload_different_payloads() {
        let secret = b"test-secret";
        let sig1 = sign_payload(secret, b"hello");
        let sig2 = sign_payload(secret, b"world");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn sign_payload_hex_format() {
        let sig = sign_payload(b"secret", b"data");
        assert_eq!(sig.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
        assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn sign_payload_verifiable() {
        let secret = b"webhook-secret-key";
        let payload = b"{\"event_type\":\"chat.completed\"}";

        let signature = sign_payload(secret, payload);

        // Verify by computing the same HMAC.
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(payload);
        let expected = hex::encode(mac.finalize().into_bytes());

        assert_eq!(signature, expected);
    }

    #[test]
    fn retry_delays_correct() {
        assert_eq!(RETRY_DELAYS.len(), 5);
        assert_eq!(RETRY_DELAYS, [1, 5, 25, 120, 600]);
    }
}
