// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core library for the Blufio agent framework.
//!
//! This crate provides the foundational trait definitions, error types, and
//! common types used throughout the Blufio workspace. All adapter plugins
//! implement traits defined here.

pub mod error;
pub mod format;
pub mod streaming;
pub mod token_counter;
pub mod traits;
pub mod types;

// Re-export key items at crate root for ergonomic imports.
pub use error::{
    BlufioError, ChannelErrorKind, ErrorCategory, ErrorContext, FailureMode, McpErrorKind,
    MigrationErrorKind, ProviderErrorKind, Severity, SkillErrorKind, StorageErrorKind,
    http_status_to_provider_error,
};
pub use format::{ColumnAlign, FormatPipeline, FormattedOutput, List, ListStyle, RichContent, Table};
pub use streaming::{StreamingBuffer, StreamingEditorOps, split_at_paragraph_boundary};
pub use types::{
    AdapterType, ChannelCapabilities, ContentBlock, FormattingSupport, HealthStatus, ImageRequest,
    ImageResponse, InboundMessage, Message, MessageContent, MessageId, OutboundMessage,
    ProviderMessage, ProviderRequest, ProviderResponse, ProviderStreamChunk, QueueEntry, RateLimit,
    Session, SessionId, StreamEventType, StreamingType, TokenUsage, ToolDefinition,
    TranscriptionRequest, TranscriptionResponse, TtsRequest, TtsResponse,
};

// Re-export token counting abstractions.
pub use token_counter::{HeuristicCounter, TokenCounter, TokenizerCache, TokenizerMode};

// Re-export all adapter traits at crate root.
pub use traits::{
    AuthAdapter, ChannelAdapter, EmbeddingAdapter, ImageAdapter, ModelInfo, ObservabilityAdapter,
    PluginAdapter, ProviderAdapter, ProviderRegistry, SkillRuntimeAdapter, StorageAdapter,
    TranscriptionAdapter, TtsAdapter,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blufio_error_has_all_variants() {
        use error::{
            ChannelErrorKind, ErrorContext, McpErrorKind, MigrationErrorKind, ProviderErrorKind,
            SkillErrorKind, StorageErrorKind,
        };

        // Verify all 16 error variants exist and can be constructed.
        let _config = BlufioError::Config("test".into());
        let _storage = BlufioError::Storage {
            kind: StorageErrorKind::Busy,
            context: ErrorContext::default(),
            source: Box::new(std::io::Error::other("test")),
        };
        let _channel = BlufioError::Channel {
            kind: ChannelErrorKind::DeliveryFailed,
            context: ErrorContext::default(),
            source: None,
        };
        let _provider = BlufioError::Provider {
            kind: ProviderErrorKind::ServerError,
            context: ErrorContext::default(),
            source: None,
        };
        let _not_found = BlufioError::AdapterNotFound {
            adapter_type: "Channel".into(),
            name: "test".into(),
        };
        let _health = BlufioError::HealthCheckFailed {
            name: "test".into(),
            source: Box::new(std::io::Error::other("test")),
        };
        let _timeout = BlufioError::Timeout {
            duration: std::time::Duration::from_secs(30),
        };
        let _vault = BlufioError::Vault("test".into());
        let _security = BlufioError::Security("test".into());
        let _signature = BlufioError::Signature("verification failed".into());
        let _budget = BlufioError::BudgetExhausted {
            message: "daily limit reached".into(),
        };
        let _internal = BlufioError::Internal("test".into());
        let _skill = BlufioError::Skill {
            kind: SkillErrorKind::ExecutionFailed,
            context: ErrorContext::default(),
            source: None,
        };
        let _mcp = BlufioError::Mcp {
            kind: McpErrorKind::ConnectionFailed,
            context: ErrorContext::default(),
            source: None,
        };
        let _migration = BlufioError::Migration {
            kind: MigrationErrorKind::SchemaFailed,
            context: ErrorContext::default(),
        };
        let _update = BlufioError::Update("test".into());
    }

    #[test]
    fn adapter_type_has_ten_variants() {
        use std::str::FromStr;

        let variants = [
            AdapterType::Channel,
            AdapterType::Provider,
            AdapterType::Storage,
            AdapterType::Embedding,
            AdapterType::Observability,
            AdapterType::Auth,
            AdapterType::SkillRuntime,
            AdapterType::Tts,
            AdapterType::Transcription,
            AdapterType::ImageGen,
        ];

        assert_eq!(
            variants.len(),
            10,
            "AdapterType must have exactly 10 variants"
        );

        // Verify Display and FromStr round-trip for all variants.
        for variant in &variants {
            let s = variant.to_string();
            let parsed = AdapterType::from_str(&s).expect("should parse back");
            assert_eq!(*variant, parsed);
        }
    }

    #[test]
    fn adapter_type_serialization() {
        let channel = AdapterType::Channel;
        let json = serde_json::to_string(&channel).expect("should serialize");
        let parsed: AdapterType = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(channel, parsed);
    }

    #[test]
    fn health_status_variants() {
        let healthy = HealthStatus::Healthy;
        let degraded = HealthStatus::Degraded("slow".into());
        let unhealthy = HealthStatus::Unhealthy("down".into());

        assert_eq!(healthy, HealthStatus::Healthy);
        assert_ne!(degraded, healthy);
        assert_ne!(unhealthy, healthy);
    }

    #[test]
    fn session_and_message_ids() {
        let sid = SessionId("session-1".into());
        let mid = MessageId("msg-1".into());

        // Verify Clone works.
        let sid2 = sid.clone();
        assert_eq!(sid, sid2);

        let mid2 = mid.clone();
        assert_eq!(mid, mid2);
    }

    #[test]
    fn all_trait_modules_are_exported() {
        // This test verifies that all 10 adapter trait modules compile
        // and are accessible through the public API. If any module is
        // missing or has a compile error, this test won't compile.
        fn _assert_plugin_adapter<T: PluginAdapter>() {}
        fn _assert_channel_adapter<T: ChannelAdapter>() {}
        fn _assert_provider_adapter<T: ProviderAdapter>() {}
        fn _assert_storage_adapter<T: StorageAdapter>() {}
        fn _assert_embedding_adapter<T: EmbeddingAdapter>() {}
        fn _assert_observability_adapter<T: ObservabilityAdapter>() {}
        fn _assert_auth_adapter<T: AuthAdapter>() {}
        fn _assert_skill_runtime_adapter<T: SkillRuntimeAdapter>() {}
        fn _assert_tts_adapter<T: TtsAdapter>() {}
        fn _assert_transcription_adapter<T: TranscriptionAdapter>() {}
        fn _assert_image_adapter<T: ImageAdapter>() {}
    }

    #[test]
    fn tool_definition_roundtrip() {
        let td = ToolDefinition {
            name: "bash".into(),
            description: "Execute a bash command".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"]
            }),
        };

        let json = serde_json::to_string(&td).unwrap();
        let deserialized: ToolDefinition = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "bash");
        assert_eq!(deserialized.description, "Execute a bash command");
        assert_eq!(deserialized.input_schema["type"], "object");
    }

    #[test]
    fn tool_definition_to_json_value() {
        let td = ToolDefinition {
            name: "http".into(),
            description: "Make HTTP requests".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string" }
                }
            }),
        };

        let value = td.to_json_value();
        assert_eq!(value["name"], "http");
        assert_eq!(value["description"], "Make HTTP requests");
        assert_eq!(value["input_schema"]["type"], "object");
    }
}
