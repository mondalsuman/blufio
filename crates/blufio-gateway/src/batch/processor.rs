// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Parallel batch processor with Semaphore-controlled concurrency.
//!
//! Executes batch items in parallel, checking scopes and routing to the
//! correct provider. Publishes BatchEvent::Submitted and BatchEvent::Completed
//! to the EventBus.

use std::sync::Arc;

use blufio_core::ProviderRegistry;
use tokio::sync::Semaphore;

use super::store::BatchStore;
use crate::api_keys::AuthContext;
use crate::openai_compat::types::{
    GatewayCompletionRequest, gateway_request_to_provider_request, parse_model_string,
};

/// Default concurrency limit for batch processing.
pub const DEFAULT_CONCURRENCY: usize = 3;

/// Process a batch of chat completion requests in parallel.
///
/// This function runs as a background task (spawned by the handler).
/// It executes items with Semaphore-controlled concurrency, updates
/// each item's status in the store, and finalizes the batch.
pub async fn process_batch(
    batch_id: String,
    items: Vec<serde_json::Value>,
    providers: Arc<dyn ProviderRegistry + Send + Sync>,
    store: Arc<BatchStore>,
    bus: Option<Arc<blufio_bus::EventBus>>,
    auth_ctx: AuthContext,
    concurrency: usize,
) {
    let item_count = items.len();

    // Publish BatchEvent::Submitted.
    if let Some(ref bus) = bus {
        bus.publish(blufio_bus::BusEvent::Batch(
            blufio_bus::BatchEvent::Submitted {
                event_id: blufio_bus::new_event_id(),
                timestamp: blufio_bus::now_timestamp(),
                batch_id: batch_id.clone(),
                item_count,
            },
        ))
        .await;
    }

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut handles = Vec::with_capacity(item_count);

    for (index, request_value) in items.into_iter().enumerate() {
        let sem = semaphore.clone();
        let store = Arc::clone(&store);
        let providers = providers.clone();
        let batch_id = batch_id.clone();
        let auth_ctx = auth_ctx.clone();

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");

            // Check scope: batch items require chat.completions scope.
            if !auth_ctx.has_scope("chat.completions") {
                store
                    .update_item(
                        &batch_id,
                        index,
                        "failed",
                        None,
                        Some("scope denied: chat.completions required"),
                    )
                    .await
                    .ok();
                return;
            }

            // Deserialize gateway request from the batch item JSON.
            let gateway_req: GatewayCompletionRequest = match serde_json::from_value(request_value)
            {
                Ok(req) => req,
                Err(e) => {
                    store
                        .update_item(
                            &batch_id,
                            index,
                            "failed",
                            None,
                            Some(&format!("invalid request format: {e}")),
                        )
                        .await
                        .ok();
                    return;
                }
            };

            // Parse model string to resolve provider.
            let (provider_name, model_name) =
                parse_model_string(&gateway_req.model, providers.default_provider());

            // Get provider adapter.
            let provider = match providers.get_provider(&provider_name) {
                Some(p) => p,
                None => {
                    store
                        .update_item(
                            &batch_id,
                            index,
                            "failed",
                            None,
                            Some(&format!("provider not found: {provider_name}")),
                        )
                        .await
                        .ok();
                    return;
                }
            };

            // Convert gateway request to provider request.
            let mut provider_request = match gateway_request_to_provider_request(&gateway_req) {
                Ok(req) => req,
                Err(e) => {
                    store
                        .update_item(
                            &batch_id,
                            index,
                            "failed",
                            None,
                            Some(&format!("invalid request: {e}")),
                        )
                        .await
                        .ok();
                    return;
                }
            };

            // Override model to the resolved model name (without provider prefix).
            provider_request.model = model_name;

            // Execute the chat completion (non-streaming).
            match provider.complete(provider_request).await {
                Ok(response) => {
                    // ProviderResponse doesn't derive Serialize, so build JSON manually.
                    let response_value = serde_json::json!({
                        "id": response.id,
                        "content": response.content,
                        "model": response.model,
                        "stop_reason": response.stop_reason,
                        "usage": {
                            "input_tokens": response.usage.input_tokens,
                            "output_tokens": response.usage.output_tokens,
                        }
                    });
                    let response_json = response_value.to_string();
                    store
                        .update_item(&batch_id, index, "completed", Some(&response_json), None)
                        .await
                        .ok();
                }
                Err(e) => {
                    store
                        .update_item(
                            &batch_id,
                            index,
                            "failed",
                            None,
                            Some(&format!("provider error: {e}")),
                        )
                        .await
                        .ok();
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all tasks to complete.
    for handle in handles {
        let _ = handle.await;
    }

    // Finalize the batch.
    let (success_count, error_count) = store.finalize_batch(&batch_id).await.unwrap_or((0, 0));

    // Publish BatchEvent::Completed.
    if let Some(ref bus) = bus {
        bus.publish(blufio_bus::BusEvent::Batch(
            blufio_bus::BatchEvent::Completed {
                event_id: blufio_bus::new_event_id(),
                timestamp: blufio_bus::now_timestamp(),
                batch_id,
                success_count,
                error_count,
            },
        ))
        .await;
    }
}
