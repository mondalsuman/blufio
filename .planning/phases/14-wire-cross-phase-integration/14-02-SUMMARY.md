# Plan 14-02 Summary: Wire RedactingWriter into Tracing Subscriber

**Phase:** 14-wire-cross-phase-integration
**Plan:** 02
**Status:** Complete
**Duration:** ~10 min

## What Was Done

### Task 1: Added blufio-security dependency to blufio binary crate
- Added `blufio-security = { path = "../blufio-security" }` to `crates/blufio/Cargo.toml`

### Task 2: Created RedactingMakeWriter struct
- Implemented `RedactingMakeWriter` in `serve.rs` that holds `Arc<RwLock<Vec<String>>>` for vault values
- Implements `tracing_subscriber::fmt::MakeWriter<'a>` trait
- `make_writer()` creates a new `RedactingWriter<Stderr>` wrapping `std::io::stderr()` with the shared vault values

### Task 3: Modified init_tracing() to use RedactingMakeWriter
- Changed `init_tracing()` return type to `Arc<RwLock<Vec<String>>>`
- Creates shared vault values handle (starts empty, populated after vault unlock)
- Passes `RedactingMakeWriter` to tracing-subscriber via `.with_writer()`
- Returns vault values handle for later secret registration

### Task 4: Registered config secrets for log redaction
- After vault startup check, registers known config secrets:
  - `config.anthropic.api_key`
  - `config.telegram.bot_token`
  - `config.gateway.bearer_token`
- Uses `RedactingWriter::add_vault_value()` to populate the shared vault values list
- Logs count of registered secrets

## Files Modified

- `crates/blufio/Cargo.toml` -- added blufio-security dependency
- `crates/blufio/src/serve.rs` -- RedactingMakeWriter, init_tracing() rewrite, vault value registration

## Verification

- `cargo build -p blufio` compiles successfully
- RedactingWriter wraps stderr -- all log output passes through secret redaction
- Regex patterns in RedactingWriter catch `sk-ant-*`, generic `sk-*`, Bearer tokens, and Telegram bot tokens automatically
- Vault values handle catches non-pattern-matching secrets loaded from credential vault
- Zero-initialization (empty Vec) avoids ordering problem (init_tracing before vault unlock)

## Commit

`e4fc5df` -- feat(14-02): wire RedactingWriter into tracing subscriber for secret redaction
