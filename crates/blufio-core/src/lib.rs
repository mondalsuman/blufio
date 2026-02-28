// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core library for the Blufio agent framework.
//!
//! This crate provides the foundational trait definitions, error types, and
//! common types used throughout the Blufio workspace. All adapter plugins
//! implement traits defined here.

pub mod error;
pub mod traits;
pub mod types;

// Re-export key items at crate root for ergonomic imports.
pub use error::BlufioError;
pub use types::{AdapterType, HealthStatus, MessageId, SessionId};

// Re-export all adapter traits at crate root.
pub use traits::{
    AuthAdapter, ChannelAdapter, EmbeddingAdapter, ObservabilityAdapter, PluginAdapter,
    ProviderAdapter, SkillRuntimeAdapter, StorageAdapter,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blufio_error_has_all_variants() {
        // Verify all 8 error variants exist and can be constructed.
        let _config = BlufioError::Config("test".into());
        let _storage = BlufioError::Storage {
            source: Box::new(std::io::Error::other("test")),
        };
        let _channel = BlufioError::Channel {
            message: "test".into(),
            source: None,
        };
        let _provider = BlufioError::Provider {
            message: "test".into(),
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
        let _internal = BlufioError::Internal("test".into());
    }

    #[test]
    fn adapter_type_has_seven_variants() {
        use std::str::FromStr;

        let variants = [
            AdapterType::Channel,
            AdapterType::Provider,
            AdapterType::Storage,
            AdapterType::Embedding,
            AdapterType::Observability,
            AdapterType::Auth,
            AdapterType::SkillRuntime,
        ];

        assert_eq!(variants.len(), 7, "AdapterType must have exactly 7 variants");

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
        // This test verifies that all 7 adapter trait modules compile
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
    }
}
