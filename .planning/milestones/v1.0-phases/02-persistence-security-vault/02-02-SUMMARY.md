---
phase: 02-persistence-security-vault
plan: 02
subsystem: security
tags: [vault, aes-256-gcm, argon2id, key-wrapping, tls, ssrf, redaction, cli]

requires: [02-01]
provides:
  - blufio-vault crate with AES-256-GCM encrypted credential storage
  - Argon2id key derivation with key-wrapping pattern
  - Vault CRUD (create, unlock, store, retrieve, list, delete, change-passphrase)
  - Plaintext config secret auto-migration to vault
  - blufio-security crate with TLS enforcement, SSRF prevention, secret redaction
  - CLI commands: set-secret, list-secrets
affects: [agent-loop, context-engine, skill-sandbox, logging]

tech-stack:
  added: [ring-0.17, argon2-0.5, secrecy-0.10, zeroize-1, rpassword-7, regex-1]
  patterns: [key-wrapping, master-key-in-memory-only, ssrf-safe-resolver, redacting-writer, lazy-vault-creation]

key-files:
  created:
    - crates/blufio-vault/Cargo.toml
    - crates/blufio-vault/src/lib.rs
    - crates/blufio-vault/src/crypto.rs
    - crates/blufio-vault/src/kdf.rs
    - crates/blufio-vault/src/prompt.rs
    - crates/blufio-vault/src/vault.rs
    - crates/blufio-vault/src/migration.rs
    - crates/blufio-security/Cargo.toml
    - crates/blufio-security/src/lib.rs
    - crates/blufio-security/src/tls.rs
    - crates/blufio-security/src/ssrf.rs
    - crates/blufio-security/src/redact.rs
  modified:
    - crates/blufio/Cargo.toml
    - crates/blufio/src/main.rs
    - crates/blufio-config/src/loader.rs

key-decisions:
  - "Used Zeroizing<[u8; 32]> for master key instead of SecretBox since SecretBox requires Box allocation and Zeroizing provides equivalent memory safety"
  - "Vault creates tables on-demand via conn.call() SQL rather than through refinery migrations (tables already exist from Plan 02-01)"
  - "SSRF resolver converts reqwest::dns::Name to String early because Name does not implement Display"
  - "Used std::sync::LazyLock (stable since Rust 1.80) instead of once_cell for compiled regex patterns"
  - "BLUFIO_VAULT_KEY env var excluded from config loader via Figment Env::ignore() to prevent config parse errors"
  - "Vault is created lazily on first set-secret call (not eagerly on startup)"
  - "Config rewrite failure after vault migration is a warning, not an error (secret is safely in vault)"

patterns-established:
  - "AES-256-GCM seal() generates random 96-bit nonce per encryption operation -- nonce stored alongside ciphertext"
  - "Key wrapping: random master key encrypts secrets, passphrase-derived key wraps master key"
  - "change_passphrase re-wraps master key without re-encrypting all secrets"
  - "mask_secret shows first 4 + last 4 chars with '...' for values >= 10 chars"
  - "SsrfSafeResolver resolves DNS once, filters IPs, preventing DNS rebinding attacks"
  - "RedactingWriter uses Arc<RwLock<Vec<String>>> for dynamic vault value registration"
  - "collapsible_if pattern: use 'if let ... && condition { }' on edition 2024"
  - "type aliases for complex return types to satisfy clippy::type_complexity"

requirements-completed: [SEC-01, SEC-03, SEC-04, SEC-08, SEC-09, SEC-10]

duration: 45min
completed: 2026-02-28
---

# Plan 02-02: Credential Vault & Network Security Summary

**AES-256-GCM encrypted vault with Argon2id KDF, TLS enforcement, SSRF prevention, secret redaction, and CLI integration**

## Performance

- **Duration:** ~45 min
- **Completed:** 2026-02-28
- **Tasks:** 4
- **Tests:** 132 total (71 new in this plan)
- **Clippy:** Clean (zero warnings)

## Accomplishments

- **blufio-vault crate** with complete credential lifecycle:
  - AES-256-GCM seal/open with random 96-bit nonces per operation (ring)
  - Argon2id key derivation with OWASP-recommended parameters (argon2)
  - Key-wrapping pattern: passphrase-derived key wraps random master key
  - Vault create/unlock/store/retrieve/list/delete/change-passphrase
  - Passphrase from BLUFIO_VAULT_KEY env var or interactive TTY prompt
  - Masked secret previews (first 4 + last 4 chars)
  - All secret material uses Zeroizing/SecretString for memory safety

- **blufio-security crate** with network security enforcement:
  - TLS 1.2+ enforcement for all remote connections (localhost exempt)
  - SSRF-safe DNS resolver blocking RFC 1918/4193/link-local/AWS metadata IPs
  - Configurable private IP allowlist
  - Secret redaction via regex patterns (sk-ant-*, sk-*, Bearer, Telegram tokens)
  - RedactingWriter for log output with dynamic vault value registration

- **Plaintext config migration:**
  - Auto-detects telegram.bot_token and anthropic.api_key in TOML config
  - Stores in vault, rewrites config without secrets
  - Idempotent (skips already-migrated secrets)
  - vault_startup_check for agent boot sequence

- **CLI integration:**
  - `blufio config set-secret <key>` -- creates vault lazily, hidden input
  - `blufio config list-secrets` -- masked previews, values never shown
  - Piped stdin support for scripting

## Task Commits

1. **Tasks 1-3: Implement blufio-vault and blufio-security crates** - `86d5455`
2. **Task 4: Wire vault CLI commands into blufio binary** - `a81fed0`

## Files Created/Modified

### Created
- `crates/blufio-vault/` - 6 source files (crypto, kdf, prompt, vault, migration, lib)
- `crates/blufio-security/` - 4 source files (tls, ssrf, redact, lib)

### Modified
- `crates/blufio/Cargo.toml` - Added vault, storage, rpassword, secrecy dependencies
- `crates/blufio/src/main.rs` - Config subcommands (set-secret, list-secrets)
- `crates/blufio-config/src/loader.rs` - BLUFIO_VAULT_KEY env var exclusion

## Deviations from Plan

### Auto-fixed Issues

**1. reqwest::dns::Name does not implement Display**
- **Found during:** Task 2 compilation
- **Issue:** SSRF resolver used `format!("{name}:0")` but Name has no Display impl
- **Fix:** Convert to String early: `let hostname = name.as_str().to_string()`
- **Verification:** All SSRF tests pass

**2. vault.rs move-after-borrow with name parameter**
- **Found during:** Task 1 compilation
- **Issue:** `name` moved into closure then used in debug! macro after
- **Fix:** Used `name_owned` for closure, kept `name` (&str) for debug macro
- **Verification:** All vault tests pass

**3. BLUFIO_VAULT_KEY leaks into config loader**
- **Found during:** Task 4 test failure
- **Issue:** Figment env provider with BLUFIO_ prefix picks up vault passphrase env var
- **Fix:** Added `.ignore(&["vault_key"])` to env_provider in loader.rs
- **Verification:** All config tests pass

**4. Multiple clippy collapsible_if warnings**
- **Found during:** Tasks 1-3 clippy run
- **Issue:** Nested if-let blocks in prompt.rs, migration.rs, redact.rs, ssrf.rs
- **Fix:** Collapsed using `if let ... && ...` syntax (edition 2024 let chains)
- **Verification:** Zero clippy warnings

**5. clippy type_complexity in vault.rs**
- **Found during:** Tasks 1-3 clippy run
- **Issue:** Return type `Result<Option<(Vec<u8>, Vec<u8>)>, rusqlite::Error>` too complex
- **Fix:** Added `type CipherNonce = (Vec<u8>, Vec<u8>)` type alias
- **Verification:** Zero clippy warnings

---

**Total deviations:** 5 auto-fixed (API differences, borrow checker, env var conflict, clippy)
**Impact on plan:** No scope change. All deviations were necessary adaptations.

## Security Invariants Established

1. **No plaintext secrets on disk** -- all vault entries encrypted with AES-256-GCM
2. **Passphrase never stored** -- Argon2id derives key in memory only, wrapped in Zeroizing
3. **Key rotation without re-encryption** -- change_passphrase re-wraps master key only
4. **TLS enforced** -- remote connections require TLS 1.2+, localhost exempt
5. **SSRF blocked** -- private/reserved IPs filtered at DNS resolution layer
6. **Secrets redacted** -- regex patterns + exact vault values replaced with [REDACTED]
7. **Security violations hard-fail** -- BlufioError::Security returned, logged at ERROR level

## Next Phase Readiness
- Vault and security crates ready for agent loop integration (Phase 3)
- build_secure_client() provides TLS-enforced HTTP client for API calls
- RedactingWriter ready to wrap tracing subscriber output
- All 132 workspace tests pass, zero clippy warnings

---
*Plan: 02-02-credential-vault-network-security*
*Completed: 2026-02-28*
