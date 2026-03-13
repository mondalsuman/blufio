// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! OpenAPI 3.1 specification generation via utoipa.
//!
//! Aggregates all handler path annotations and component schemas into a single
//! `ApiDoc` struct. The spec is served at `/openapi.json` and optionally
//! rendered via Swagger UI at `/docs`.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        // Core handlers
        crate::handlers::post_messages,
        crate::handlers::get_health,
        crate::handlers::get_sessions,
        crate::handlers::get_public_health,
        crate::handlers::get_public_metrics,
        // OpenAI-compatible endpoints
        crate::openai_compat::handlers::post_chat_completions,
        crate::openai_compat::handlers::get_models,
        crate::openai_compat::responses::post_responses,
        crate::openai_compat::tools::get_tools,
        crate::openai_compat::tools::post_tool_invoke,
        // API key management
        crate::api_keys::handlers::post_create_api_key,
        crate::api_keys::handlers::get_list_api_keys,
        crate::api_keys::handlers::delete_api_key,
        // Webhook management
        crate::webhooks::handlers::post_create_webhook,
        crate::webhooks::handlers::get_list_webhooks,
        crate::webhooks::handlers::delete_webhook,
        // Batch processing
        crate::batch::handlers::post_create_batch,
        crate::batch::handlers::get_batch_status,
    ),
    components(schemas(
        // Core handler types
        crate::handlers::MessageRequest,
        crate::handlers::MessageResponse,
        crate::handlers::HealthResponse,
        crate::handlers::SessionListResponse,
        crate::handlers::SessionInfo,
        crate::handlers::ErrorResponse,
        crate::handlers::PublicHealthResponse,
        // OpenAI compat types
        crate::openai_compat::types::GatewayCompletionRequest,
        crate::openai_compat::types::GatewayCompletionResponse,
        crate::openai_compat::types::GatewayMessage,
        crate::openai_compat::types::GatewayContent,
        crate::openai_compat::types::GatewayContentPart,
        crate::openai_compat::types::GatewayImageUrl,
        crate::openai_compat::types::GatewayTool,
        crate::openai_compat::types::GatewayFunctionDef,
        crate::openai_compat::types::GatewayToolCall,
        crate::openai_compat::types::GatewayFunctionCall,
        crate::openai_compat::types::GatewayStreamOptions,
        crate::openai_compat::types::GatewayChoice,
        crate::openai_compat::types::GatewayResponseMessage,
        crate::openai_compat::types::GatewayUsage,
        crate::openai_compat::types::GatewayErrorResponse,
        crate::openai_compat::types::GatewayErrorDetail,
        crate::openai_compat::types::ModelsListResponse,
        // Responses API types
        crate::openai_compat::responses_types::ResponsesRequest,
        crate::openai_compat::responses_types::ResponsesInput,
        crate::openai_compat::responses_types::ResponsesInputMessage,
        crate::openai_compat::responses_types::ResponsesTool,
        crate::openai_compat::responses_types::ResponsesFunction,
        // Tools API types
        crate::openai_compat::tools_types::ToolListResponse,
        crate::openai_compat::tools_types::ToolInfo,
        crate::openai_compat::tools_types::ToolFunctionInfo,
        crate::openai_compat::tools_types::ToolInvokeRequest,
        crate::openai_compat::tools_types::ToolInvokeResponse,
        // API key types
        crate::api_keys::ApiKey,
        crate::api_keys::CreateKeyRequest,
        crate::api_keys::CreateKeyResponse,
        // Webhook types
        crate::webhooks::WebhookListItem,
        crate::webhooks::CreateWebhookRequest,
        crate::webhooks::CreateWebhookResponse,
        // Batch types
        crate::batch::BatchRequest,
        crate::batch::BatchSubmitResponse,
        crate::batch::BatchResponse,
        crate::batch::BatchItemResult,
    )),
    tags(
        (name = "Messages", description = "Message exchange API"),
        (name = "Sessions", description = "Session management"),
        (name = "OpenAI Compatible", description = "OpenAI-compatible endpoints"),
        (name = "API Keys", description = "API key management"),
        (name = "Webhooks", description = "Webhook management"),
        (name = "Batch", description = "Batch processing"),
        (name = "Health", description = "Health and monitoring"),
    ),
    modifiers(&SecurityAddon),
    info(
        title = "Blufio API",
        version = env!("CARGO_PKG_VERSION"),
        description = "Always-on personal AI agent REST API",
        license(name = "MIT OR Apache-2.0"),
    )
)]
pub struct ApiDoc;

struct SecurityAddon;
impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                utoipa::openapi::security::SecurityScheme::Http(
                    utoipa::openapi::security::Http::new(
                        utoipa::openapi::security::HttpAuthScheme::Bearer,
                    ),
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use utoipa::OpenApi;

    #[test]
    fn openapi_spec_snapshot() {
        let spec = ApiDoc::openapi();
        let json: serde_json::Value =
            serde_json::from_str(&spec.to_pretty_json().unwrap()).unwrap();
        insta::assert_json_snapshot!("openapi_spec", json);
    }

    #[test]
    fn openapi_spec_is_valid_json() {
        let spec = ApiDoc::openapi();
        let json = spec.to_pretty_json().unwrap();
        let _: serde_json::Value = serde_json::from_str(&json).unwrap();
    }
}
