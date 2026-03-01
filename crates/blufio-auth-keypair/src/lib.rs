// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ed25519 device keypair authentication adapter.
//!
//! Implements `AuthAdapter` with bearer token validation using Ed25519 keypairs.
//! The keypair is generated on first run and can be stored in the vault for persistence.

pub mod keypair;
pub mod message;

pub use ed25519_dalek::Signature;
pub use keypair::DeviceKeypair;
pub use message::{AgentMessage, AgentMessageType, SignedAgentMessage};

use async_trait::async_trait;

use blufio_core::traits::adapter::PluginAdapter;
use blufio_core::traits::auth::AuthAdapter;
use blufio_core::types::{AdapterType, AuthIdentity, AuthToken, HealthStatus};
use blufio_core::BlufioError;

/// Keypair-based authentication adapter.
///
/// Validates bearer tokens against the device's Ed25519 public key.
pub struct KeypairAuthAdapter {
    keypair: DeviceKeypair,
}

impl KeypairAuthAdapter {
    /// Create a new adapter with the given keypair.
    pub fn new(keypair: DeviceKeypair) -> Self {
        Self { keypair }
    }

    /// Get a reference to the underlying keypair.
    pub fn keypair(&self) -> &DeviceKeypair {
        &self.keypair
    }
}

#[async_trait]
impl PluginAdapter for KeypairAuthAdapter {
    fn name(&self) -> &str {
        "keypair-auth"
    }

    fn version(&self) -> semver::Version {
        semver::Version::new(0, 1, 0)
    }

    fn adapter_type(&self) -> AdapterType {
        AdapterType::Auth
    }

    async fn health_check(&self) -> Result<HealthStatus, BlufioError> {
        Ok(HealthStatus::Healthy)
    }

    async fn shutdown(&self) -> Result<(), BlufioError> {
        Ok(())
    }
}

#[async_trait]
impl AuthAdapter for KeypairAuthAdapter {
    async fn authenticate(&self, token: AuthToken) -> Result<AuthIdentity, BlufioError> {
        if self.keypair.verify_token(&token.token) {
            Ok(AuthIdentity {
                id: self.keypair.public_hex(),
                label: Some("device-keypair".to_string()),
            })
        } else {
            Err(BlufioError::Security(
                "invalid bearer token".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn authenticate_valid_token() {
        let kp = DeviceKeypair::generate();
        let token = kp.public_hex();
        let adapter = KeypairAuthAdapter::new(kp);

        let identity = adapter
            .authenticate(AuthToken {
                token,
            })
            .await
            .unwrap();

        assert!(!identity.id.is_empty());
        assert_eq!(identity.label.as_deref(), Some("device-keypair"));
    }

    #[tokio::test]
    async fn authenticate_invalid_token() {
        let kp = DeviceKeypair::generate();
        let adapter = KeypairAuthAdapter::new(kp);

        let result = adapter
            .authenticate(AuthToken {
                token: "bad-token".to_string(),
            })
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn adapter_name_and_type() {
        let kp = DeviceKeypair::generate();
        let adapter = KeypairAuthAdapter::new(kp);
        assert_eq!(adapter.name(), "keypair-auth");
        assert_eq!(adapter.adapter_type(), AdapterType::Auth);
    }

    #[tokio::test]
    async fn health_check_healthy() {
        let kp = DeviceKeypair::generate();
        let adapter = KeypairAuthAdapter::new(kp);
        let health = adapter.health_check().await.unwrap();
        assert_eq!(health, HealthStatus::Healthy);
    }
}
