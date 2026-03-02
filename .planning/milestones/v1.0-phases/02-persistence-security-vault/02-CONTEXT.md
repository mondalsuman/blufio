# Phase 2: Persistence & Security Vault - Context

**Gathered:** 2026-02-28
**Status:** Ready for planning

<domain>
## Phase Boundary

All application state persists in a single SQLite database with WAL mode and ACID guarantees. Credentials are encrypted at rest with AES-256-GCM. Security defaults (localhost binding, TLS, secret redaction, SSRF prevention) are enforced from this point forward. No agent loop, no channels, no LLM calls -- pure persistence and security foundation.

</domain>

<decisions>
## Implementation Decisions

### Vault Unlock Experience
- Both passphrase prompt (default) and environment variable (`BLUFIO_VAULT_KEY`) for headless deployments
- Passphrase prompt on interactive TTY, env var detected automatically for unattended startup (systemd, Docker)
- Vault created lazily on first `blufio config set-secret` call -- no upfront vault initialization step
- Agent fails to start with clear error if vault exists but is not unlocked -- no degraded/partial operation
- Key wrapping pattern: master key encrypted by passphrase-derived key (Argon2id). Changing passphrase re-wraps master key, does not re-encrypt all secrets

### Secret Management Workflow
- `blufio config set-secret <key>` CLI command for adding/updating secrets
- Hidden prompt (no echo) for interactive use, stdin pipe support for scripting -- TTY detection selects mode automatically
- `blufio config list-secrets` shows names + masked preview (e.g., `sk-...4f2b`) -- values never fully displayed
- Auto-migrate plaintext secrets found in TOML config into vault on startup, remove from config file, warn user about migration

### Database Lifecycle
- Default location: XDG data directory (`~/.local/share/blufio/blufio.db` on Linux), configurable via `storage.database_path`
- Embedded SQL migrations compiled into binary, auto-applied on startup -- zero manual database operations
- Fully automatic: create parent directories + database file on first startup if they don't exist
- Schema version tracked in `_migrations` table

### Security Strictness
- Localhost connections (127.0.0.1/::1) exempt from TLS requirement automatically; all remote connections require TLS -- no config toggle needed
- Secret redaction: known patterns (API key prefixes, Bearer tokens, common formats) + all vault-stored values redacted from logs
- SSRF prevention: private IP ranges blocked by default, allowlist via `security.allowed_private_ips` config for explicit local service access, all allowlisted connections logged
- Security violations (TLS failure, SSRF blocked, invalid credential) hard-fail the operation immediately and log at ERROR level -- no silent swallowing, no fallback

### Claude's Discretion
- Single-writer concurrency pattern implementation (dedicated writer thread vs. connection pool) -- goal is zero SQLITE_BUSY errors per PERS-05
- Argon2id parameter tuning (memory cost, iterations, parallelism)
- Exact secret pattern regex for redaction
- Migration file organization and naming convention
- Database table schema design for sessions, messages, and queue

</decisions>

<specifics>
## Specific Ideas

- Vault UX should feel like `ssh-keygen` or GPG -- familiar to developers
- Masked secret preview like `sk-...4f2b` helps identify which key is stored without exposure
- Auto-migration of plaintext config secrets provides smooth transition -- user doesn't need to know about vault to have secrets protected
- Zero-config database experience: `blufio serve` just works, DB appears in XDG data dir

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `StorageAdapter` trait (`blufio-core/src/traits/storage.rs`): Has `initialize()` and `close()` -- extend with query methods for persistence layer
- `StorageConfig` (`blufio-config/src/model.rs`): Already has `database_path` and `wal_mode` fields -- update default to XDG path
- `SecurityConfig` (`blufio-config/src/model.rs`): Already has `bind_address` (127.0.0.1) and `require_tls` (true) -- extend with SSRF allowlist and vault config

### Established Patterns
- All config structs use `#[serde(deny_unknown_fields)]` -- new config sections must follow this
- Config uses figment with XDG hierarchy and env var overrides -- vault config should integrate the same way
- Error handling via `BlufioError` in blufio-core -- security errors should be variants of this

### Integration Points
- `blufio-config/src/model.rs`: Add `VaultConfig` section and extend `SecurityConfig` with SSRF allowlist
- `blufio-core/src/traits/storage.rs`: Extend `StorageAdapter` with session/message/queue operations
- New crate needed: `blufio-storage` (SQLite implementation) and potentially `blufio-vault` (encryption)
- 7 adapter trait stubs exist (`auth`, `channel`, `embedding`, `observability`, `provider`, `skill`, `storage`) -- storage adapter gets first real implementation

</code_context>

<deferred>
## Deferred Ideas

None -- discussion stayed within phase scope

</deferred>

---

*Phase: 02-persistence-security-vault*
*Context gathered: 2026-02-28*
