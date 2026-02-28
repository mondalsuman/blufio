// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Passphrase acquisition via TTY prompt or BLUFIO_VAULT_KEY environment variable.

use blufio_core::BlufioError;
use secrecy::SecretString;

/// The environment variable name for providing the vault passphrase.
pub const VAULT_KEY_ENV_VAR: &str = "BLUFIO_VAULT_KEY";

/// Get vault passphrase from environment variable or interactive TTY prompt.
///
/// Priority:
/// 1. `BLUFIO_VAULT_KEY` environment variable (for headless/Docker/systemd)
/// 2. Interactive TTY prompt via `rpassword` (for human operators)
///
/// Returns an error if neither source is available.
pub fn get_vault_passphrase() -> Result<SecretString, BlufioError> {
    // Check env var first.
    if let Ok(key) = std::env::var(VAULT_KEY_ENV_VAR)
        && !key.is_empty()
    {
        return Ok(SecretString::from(key));
    }

    // Try interactive prompt.
    if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        eprint!("Vault passphrase: ");
        let passphrase = rpassword::read_password()
            .map_err(|e| BlufioError::Vault(format!("failed to read passphrase: {e}")))?;
        if passphrase.is_empty() {
            return Err(BlufioError::Vault("empty passphrase not allowed".to_string()));
        }
        return Ok(SecretString::from(passphrase));
    }

    Err(BlufioError::Vault(
        "No passphrase provided. Set BLUFIO_VAULT_KEY environment variable or run interactively."
            .to_string(),
    ))
}

/// Get vault passphrase with confirmation prompt (for vault creation).
///
/// Prompts twice and verifies the passphrases match. Only works in interactive
/// TTY mode; falls back to env var if not a terminal.
pub fn get_vault_passphrase_with_confirm() -> Result<SecretString, BlufioError> {
    // Env var does not need confirmation.
    if let Ok(key) = std::env::var(VAULT_KEY_ENV_VAR)
        && !key.is_empty()
    {
        return Ok(SecretString::from(key));
    }

    if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        eprint!("New vault passphrase: ");
        let pass1 = rpassword::read_password()
            .map_err(|e| BlufioError::Vault(format!("failed to read passphrase: {e}")))?;
        eprint!("Confirm vault passphrase: ");
        let pass2 = rpassword::read_password()
            .map_err(|e| BlufioError::Vault(format!("failed to read passphrase: {e}")))?;

        if pass1 != pass2 {
            return Err(BlufioError::Vault("passphrases do not match".to_string()));
        }
        if pass1.is_empty() {
            return Err(BlufioError::Vault("empty passphrase not allowed".to_string()));
        }
        return Ok(SecretString::from(pass1));
    }

    Err(BlufioError::Vault(
        "No passphrase provided. Set BLUFIO_VAULT_KEY environment variable or run interactively."
            .to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_passphrase_from_env_var() {
        // SAFETY: test-only env mutation. Tests using env vars must not run in parallel.
        unsafe { std::env::set_var(VAULT_KEY_ENV_VAR, "test-passphrase") };
        let result = get_vault_passphrase();
        unsafe { std::env::remove_var(VAULT_KEY_ENV_VAR) };

        assert!(result.is_ok());
    }

    #[test]
    fn get_passphrase_with_confirm_from_env_var() {
        unsafe { std::env::set_var(VAULT_KEY_ENV_VAR, "test-passphrase") };
        let result = get_vault_passphrase_with_confirm();
        unsafe { std::env::remove_var(VAULT_KEY_ENV_VAR) };

        assert!(result.is_ok());
    }

    #[test]
    fn empty_env_var_is_rejected() {
        unsafe { std::env::set_var(VAULT_KEY_ENV_VAR, "") };
        // In CI/test, stdin is not a terminal, so this will fail.
        let result = get_vault_passphrase();
        unsafe { std::env::remove_var(VAULT_KEY_ENV_VAR) };

        assert!(result.is_err());
    }
}
