# Plan 11-03 Summary: P2 Keypair Auth Gateway Wiring

**Phase:** 11-fix-integration-bugs
**Plan:** 03
**Status:** Complete
**Duration:** ~8 min

## What Was Done

### Task 1: Extended auth middleware with keypair signature verification
- Added `keypair_public_key: Option<VerifyingKey>` field to `AuthConfig` in `crates/blufio-gateway/src/auth.rs`
- Implemented custom `Debug` for `AuthConfig` (redacts bearer token, shows keypair presence)
- Rewrote `auth_middleware` with dual-auth priority chain:
  1. Fail-closed: rejects all requests when no auth method configured
  2. Bearer token check (fast path — string comparison)
  3. Keypair signature check (slow path — Ed25519 verification with replay prevention)
- Keypair auth uses `X-Signature` (hex-encoded Ed25519 signature) and `X-Timestamp` (RFC3339) headers
- Replay prevention: rejects timestamps older than 60 seconds
- Added `ed25519-dalek` and `hex` dependencies to `crates/blufio-gateway/Cargo.toml`
- Updated `GatewayChannelConfig` in `crates/blufio-gateway/src/lib.rs` with `keypair_public_key` field
- Updated `GatewayChannel::connect()` to pass keypair through to `AuthConfig`
- Added `verifying_key()` accessor method to `DeviceKeypair` in `crates/blufio-auth-keypair/src/keypair.rs`

### Task 2: Wired keypair public key into gateway configuration during serve startup
- Updated gateway init in `crates/blufio/src/serve.rs` to load device keypair and pass public key to gateway
- Added fail-closed check: gateway refuses to start when enabled but no auth method configured
- `#[cfg(feature = "keypair")]` guard for conditional keypair loading
- Updated test in `crates/blufio-gateway/src/server.rs` with new `keypair_public_key: None` field

## Files Modified

- `crates/blufio-gateway/src/auth.rs` — dual-auth middleware with keypair signature verification
- `crates/blufio-gateway/src/lib.rs` — keypair_public_key field on GatewayChannelConfig
- `crates/blufio-gateway/src/server.rs` — test updated with new AuthConfig field
- `crates/blufio-gateway/Cargo.toml` — ed25519-dalek and hex dependencies
- `crates/blufio-auth-keypair/src/keypair.rs` — verifying_key() accessor
- `crates/blufio/src/serve.rs` — keypair loading and fail-closed gateway start check

## Verification

- `cargo check --workspace` passes clean
- `cargo test --workspace` — 586 tests pass, 0 failures
- AuthConfig has keypair_public_key field
- auth_middleware checks bearer first, then keypair signature, then rejects
- Gateway refuses to start without any auth method configured
- Replay prevention rejects timestamps > 60 seconds old

## Commit

`b0243b6` — fix(P2): wire keypair auth into gateway middleware
