// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Argon2id key derivation from a passphrase.
//!
//! Derives a 32-byte key using Argon2id (Algorithm::Argon2id, Version::V0x13)
//! with parameters from VaultConfig (OWASP-recommended defaults).

use blufio_core::BlufioError;
use ring::rand::{SecureRandom, SystemRandom};
use zeroize::Zeroizing;

/// Derive a 32-byte key from passphrase using Argon2id.
///
/// The returned key is wrapped in [`Zeroizing`] for automatic memory zeroing
/// on drop.
pub fn derive_key(
    passphrase: &[u8],
    salt: &[u8; 16],
    memory_cost: u32,
    iterations: u32,
    parallelism: u32,
) -> Result<Zeroizing<[u8; 32]>, BlufioError> {
    let params = argon2::Params::new(memory_cost, iterations, parallelism, Some(32))
        .map_err(|e| BlufioError::Vault(format!("invalid Argon2id parameters: {e}")))?;

    let argon2 = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

    let mut output = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(passphrase, salt, output.as_mut())
        .map_err(|e| BlufioError::Vault(format!("Argon2id key derivation failed: {e}")))?;

    Ok(output)
}

/// Generate a random 16-byte salt for Argon2id.
pub fn generate_salt() -> Result<[u8; 16], BlufioError> {
    let rng = SystemRandom::new();
    let mut salt = [0u8; 16];
    rng.fill(&mut salt)
        .map_err(|_| BlufioError::Vault("failed to generate random salt".to_string()))?;
    Ok(salt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_key_produces_consistent_output() {
        let salt = [1u8; 16];
        let passphrase = b"test passphrase";

        // Use low cost for fast tests.
        let key1 = derive_key(passphrase, &salt, 32768, 2, 1).unwrap();
        let key2 = derive_key(passphrase, &salt, 32768, 2, 1).unwrap();

        assert_eq!(*key1, *key2);
    }

    #[test]
    fn derive_key_different_passphrase_produces_different_output() {
        let salt = [2u8; 16];

        let key1 = derive_key(b"passphrase one", &salt, 32768, 2, 1).unwrap();
        let key2 = derive_key(b"passphrase two", &salt, 32768, 2, 1).unwrap();

        assert_ne!(*key1, *key2);
    }

    #[test]
    fn derive_key_different_salt_produces_different_output() {
        let passphrase = b"same passphrase";

        let key1 = derive_key(passphrase, &[1u8; 16], 32768, 2, 1).unwrap();
        let key2 = derive_key(passphrase, &[2u8; 16], 32768, 2, 1).unwrap();

        assert_ne!(*key1, *key2);
    }

    #[test]
    fn generate_salt_produces_random_values() {
        let salt1 = generate_salt().unwrap();
        let salt2 = generate_salt().unwrap();

        assert_ne!(salt1, salt2);
    }

    #[test]
    fn derive_key_output_is_32_bytes() {
        let key = derive_key(b"test", &[0u8; 16], 32768, 2, 1).unwrap();
        assert_eq!(key.len(), 32);
    }
}
