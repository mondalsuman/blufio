// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Ed25519 device keypair generation and token validation.

use blufio_core::BlufioError;
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;

/// An Ed25519 device keypair for authentication.
///
/// Used for bearer token validation. The public key (hex-encoded) serves
/// as the bearer token. More sophisticated challenge-response auth is
/// deferred to v2.
pub struct DeviceKeypair {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl DeviceKeypair {
    /// Generate a new random Ed25519 keypair.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = VerifyingKey::from(&signing_key);
        Self {
            signing_key,
            verifying_key,
        }
    }

    /// Reconstruct a keypair from private key bytes.
    pub fn from_bytes(private_bytes: &[u8; 32]) -> Result<Self, BlufioError> {
        let signing_key = SigningKey::from_bytes(private_bytes);
        let verifying_key = VerifyingKey::from(&signing_key);
        Ok(Self {
            signing_key,
            verifying_key,
        })
    }

    /// Get the private key bytes (for vault storage).
    pub fn private_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Get the public key bytes.
    pub fn public_bytes(&self) -> [u8; 32] {
        self.verifying_key.to_bytes()
    }

    /// Get the hex-encoded public key (used as bearer token).
    pub fn public_hex(&self) -> String {
        hex::encode(self.public_bytes())
    }

    /// Verify a bearer token against this keypair.
    ///
    /// Simple validation: token must equal hex-encoded public key.
    /// More sophisticated challenge-response is deferred to v2.
    pub fn verify_token(&self, token: &str) -> bool {
        let expected = self.public_hex();
        token == expected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_creates_valid_keypair() {
        let kp = DeviceKeypair::generate();
        assert_eq!(kp.private_bytes().len(), 32);
        assert_eq!(kp.public_bytes().len(), 32);
        assert_eq!(kp.public_hex().len(), 64); // 32 bytes = 64 hex chars
    }

    #[test]
    fn from_bytes_roundtrip() {
        let kp1 = DeviceKeypair::generate();
        let private = kp1.private_bytes();

        let kp2 = DeviceKeypair::from_bytes(&private).unwrap();
        assert_eq!(kp1.public_bytes(), kp2.public_bytes());
        assert_eq!(kp1.private_bytes(), kp2.private_bytes());
    }

    #[test]
    fn verify_token_correct() {
        let kp = DeviceKeypair::generate();
        let token = kp.public_hex();
        assert!(kp.verify_token(&token));
    }

    #[test]
    fn verify_token_wrong() {
        let kp = DeviceKeypair::generate();
        assert!(!kp.verify_token("wrong-token"));
    }

    #[test]
    fn verify_token_empty() {
        let kp = DeviceKeypair::generate();
        assert!(!kp.verify_token(""));
    }

    #[test]
    fn different_keypairs_have_different_tokens() {
        let kp1 = DeviceKeypair::generate();
        let kp2 = DeviceKeypair::generate();
        assert_ne!(kp1.public_hex(), kp2.public_hex());
    }
}
