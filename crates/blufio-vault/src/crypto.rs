// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Low-level AES-256-GCM seal/open operations.
//!
//! Every call to [`seal`] generates a fresh random 96-bit nonce via the system
//! CSPRNG. Nonce reuse would be catastrophic for GCM security.

use blufio_core::BlufioError;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use ring::rand::{SecureRandom, SystemRandom};

/// Encrypt plaintext with AES-256-GCM using a random 96-bit nonce.
///
/// Returns `(ciphertext_with_tag, nonce_bytes)`. The caller must store both
/// the ciphertext and the nonce to be able to decrypt later.
pub fn seal(key: &[u8; 32], plaintext: &[u8]) -> Result<(Vec<u8>, [u8; 12]), BlufioError> {
    let unbound = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| BlufioError::Vault("failed to create AES-256-GCM key".to_string()))?;
    let less_safe = LessSafeKey::new(unbound);

    // Generate random 96-bit nonce.
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes)
        .map_err(|_| BlufioError::Vault("failed to generate random nonce".to_string()))?;

    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    // Seal in place: plaintext buffer is extended with the authentication tag.
    let mut in_out = plaintext.to_vec();
    less_safe
        .seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| BlufioError::Vault("AES-256-GCM encryption failed".to_string()))?;

    Ok((in_out, nonce_bytes))
}

/// Decrypt ciphertext with AES-256-GCM.
///
/// `ciphertext` must include the 16-byte authentication tag appended by [`seal`].
/// Returns the decrypted plaintext, or an error if the key is wrong or data is
/// tampered.
pub fn open(key: &[u8; 32], nonce_bytes: &[u8; 12], ciphertext: &[u8]) -> Result<Vec<u8>, BlufioError> {
    let unbound = UnboundKey::new(&AES_256_GCM, key)
        .map_err(|_| BlufioError::Vault("failed to create AES-256-GCM key".to_string()))?;
    let less_safe = LessSafeKey::new(unbound);

    let nonce = Nonce::assume_unique_for_key(*nonce_bytes);

    let mut in_out = ciphertext.to_vec();
    let plaintext = less_safe
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| BlufioError::Vault("AES-256-GCM decryption failed -- wrong key or corrupted data".to_string()))?;

    Ok(plaintext.to_vec())
}

/// Generate a random 32-byte key suitable for AES-256-GCM.
pub fn generate_random_key() -> Result<[u8; 32], BlufioError> {
    let rng = SystemRandom::new();
    let mut key = [0u8; 32];
    rng.fill(&mut key)
        .map_err(|_| BlufioError::Vault("failed to generate random key".to_string()))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let key = generate_random_key().unwrap();
        let plaintext = b"secret api key value";

        let (ciphertext, nonce) = seal(&key, plaintext).unwrap();
        let decrypted = open(&key, &nonce, &ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn seal_produces_different_ciphertext_for_same_plaintext() {
        let key = generate_random_key().unwrap();
        let plaintext = b"same input twice";

        let (ct1, nonce1) = seal(&key, plaintext).unwrap();
        let (ct2, nonce2) = seal(&key, plaintext).unwrap();

        // Random nonces should differ.
        assert_ne!(nonce1, nonce2);
        // Ciphertext should differ due to different nonces.
        assert_ne!(ct1, ct2);
    }

    #[test]
    fn open_with_wrong_key_fails() {
        let key1 = generate_random_key().unwrap();
        let key2 = generate_random_key().unwrap();
        let plaintext = b"secret data";

        let (ciphertext, nonce) = seal(&key1, plaintext).unwrap();
        let result = open(&key2, &nonce, &ciphertext);

        assert!(result.is_err());
    }

    #[test]
    fn ciphertext_is_longer_than_plaintext() {
        let key = generate_random_key().unwrap();
        let plaintext = b"hello";

        let (ciphertext, _) = seal(&key, plaintext).unwrap();

        // Ciphertext includes 16-byte GCM tag.
        assert_eq!(ciphertext.len(), plaintext.len() + 16);
    }

    #[test]
    fn tampered_ciphertext_fails_decryption() {
        let key = generate_random_key().unwrap();
        let plaintext = b"do not tamper";

        let (mut ciphertext, nonce) = seal(&key, plaintext).unwrap();
        // Flip a bit.
        ciphertext[0] ^= 0x01;

        let result = open(&key, &nonce, &ciphertext);
        assert!(result.is_err());
    }
}
