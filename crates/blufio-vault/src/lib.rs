// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! AES-256-GCM encrypted credential vault for the Blufio agent framework.
//!
//! Provides encrypted storage for API keys, bot tokens, and other secrets
//! using a key-wrapping pattern: a random master key encrypts all secrets,
//! and the master key itself is protected by a passphrase-derived key via
//! Argon2id.

pub mod crypto;
pub mod kdf;
pub mod migration;
pub mod prompt;
pub mod vault;

pub use migration::{migrate_plaintext_secrets, vault_startup_check, MigrationReport};
pub use prompt::get_vault_passphrase;
pub use vault::{mask_secret, Vault};
