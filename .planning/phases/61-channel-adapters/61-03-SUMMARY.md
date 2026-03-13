---
phase: 61-channel-adapters
plan: 03
subsystem: channel
tags: [imessage, sms, bluebubbles, twilio, webhook, hmac-sha1, e164, adapter]

# Dependency graph
requires:
  - phase: 61-channel-adapters
    provides: "Crate scaffolds, config structs (IMessageConfig, SmsConfig), ChannelEvent variants"
provides:
  - "IMessageChannel with ChannelAdapter + PluginAdapter implementations"
  - "SmsChannel with ChannelAdapter + PluginAdapter implementations"
  - "BlueBubbles REST API client with query-param auth"
  - "Twilio REST API client with HTTP Basic auth"
  - "Webhook handler at /webhooks/imessage with shared secret validation"
  - "Webhook handler at /webhooks/sms with HMAC-SHA1 signature validation"
  - "E.164 phone number format validation"
  - "STOP/UNSUBSCRIBE keyword detection"
affects: [61-channel-adapters, 63-testing]

# Tech tracking
tech-stack:
  added: [serde_urlencoded 0.7]
  patterns: [HMAC-SHA1 Base64 signature validation (Twilio), query-param auth (BlueBubbles), form-urlencoded POST with manual body construction]

key-files:
  created: []
  modified:
    - "crates/blufio-imessage/src/lib.rs"
    - "crates/blufio-imessage/src/api.rs"
    - "crates/blufio-imessage/src/webhook.rs"
    - "crates/blufio-imessage/src/types.rs"
    - "crates/blufio-imessage/Cargo.toml"
    - "crates/blufio-sms/src/lib.rs"
    - "crates/blufio-sms/src/api.rs"
    - "crates/blufio-sms/src/webhook.rs"
    - "crates/blufio-sms/src/types.rs"
    - "crates/blufio-sms/Cargo.toml"

key-decisions:
  - "BlueBubblesClient uses manual URL construction with ?password= query param (not Authorization header)"
  - "TwilioClient uses manual form-urlencoded body construction (reqwest form feature not enabled in workspace)"
  - "HMAC-SHA1 with Base64 encoding for Twilio (distinct from WhatsApp HMAC-SHA256 with hex)"
  - "serde_urlencoded added to SMS crate for webhook form body parsing"

patterns-established:
  - "Webhook-based adapter: mpsc channel + webhook state pattern for iMessage and SMS"
  - "E.164 validation function reusable across SMS-related code"
  - "STOP keyword detection as application-level compliance filter"

requirements-completed: [CHAN-03, CHAN-04, CHAN-05]

# Metrics
duration: 7min
completed: 2026-03-13
---

# Phase 61 Plan 03: iMessage and SMS Channel Adapters Summary

**IMessageChannel (BlueBubbles REST API, webhook at /webhooks/imessage) and SmsChannel (Twilio REST API, HMAC-SHA1 webhook at /webhooks/sms) with full ChannelAdapter + PluginAdapter implementations and 28 unit tests**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-13T00:01:43Z
- **Completed:** 2026-03-13T00:08:50Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments
- IMessageChannel adapter with BlueBubbles REST API client, webhook handler, group chat trigger prefix, tapback filtering, and experimental warning
- SmsChannel adapter with Twilio REST API client, HMAC-SHA1 webhook validation, E.164 phone number validation, STOP keyword detection, and rate limiting
- Both adapters implement ChannelAdapter + PluginAdapter traits with FormatPipeline integration in send()
- 28 unit tests covering config validation, capabilities, plugin metadata, HMAC-SHA1 test vectors, E.164 format, and STOP keywords

## Task Commits

Each task was committed atomically:

1. **Task 1: Implement iMessage adapter (BlueBubbles webhook + REST API)** - `4ff7a8a` (feat)
2. **Task 2: Implement SMS adapter (Twilio webhook + REST API with HMAC-SHA1)** - `f1c38f3` (feat)

## Files Created/Modified
- `crates/blufio-imessage/src/types.rs` - BlueBubbles webhook payload and API types (6 structs)
- `crates/blufio-imessage/src/api.rs` - BlueBubbles REST API client with query-param auth, server info, send message, register webhook, read receipt, typing
- `crates/blufio-imessage/src/webhook.rs` - Axum POST handler at /webhooks/imessage with shared secret validation, tapback filtering, group trigger prefix
- `crates/blufio-imessage/src/lib.rs` - IMessageChannel with ChannelAdapter + PluginAdapter impls, PlainText/20k max/typing support
- `crates/blufio-imessage/Cargo.toml` - Added semver dependency
- `crates/blufio-sms/src/types.rs` - Twilio webhook inbound, send request/response, and account info types (4 structs)
- `crates/blufio-sms/src/api.rs` - Twilio REST API client with HTTP Basic auth, send message (429 retry), account status, E.164 validation
- `crates/blufio-sms/src/webhook.rs` - Axum POST handler at /webhooks/sms with HMAC-SHA1 validation, STOP keyword detection, MMS filtering
- `crates/blufio-sms/src/lib.rs` - SmsChannel with ChannelAdapter + PluginAdapter impls, PlainText/1600 max/1 msg/s rate limit
- `crates/blufio-sms/Cargo.toml` - Added semver and serde_urlencoded dependencies

## Decisions Made
- BlueBubblesClient uses `?password=` query param auth (per BlueBubbles API convention, NOT Authorization header)
- TwilioClient builds form-urlencoded body manually via `serde_urlencoded::to_string()` + Content-Type header (workspace reqwest lacks `form` feature)
- HMAC-SHA1 with Base64 encoding for Twilio webhook validation (distinct from WhatsApp's HMAC-SHA256 with hex encoding)
- `serde_urlencoded` added as direct dependency for SMS webhook form body parsing
- Both adapters use `Mutex<Option<Client>>` pattern for API client (initialized during `connect()`)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added semver dependency to iMessage Cargo.toml**
- **Found during:** Task 1
- **Issue:** PluginAdapter::version() returns semver::Version but the crate didn't declare semver as dependency
- **Fix:** Added `semver.workspace = true` to Cargo.toml
- **Files modified:** crates/blufio-imessage/Cargo.toml
- **Verification:** `cargo test -p blufio-imessage` passes
- **Committed in:** 4ff7a8a (Task 1 commit)

**2. [Rule 3 - Blocking] Used manual form body construction for Twilio API**
- **Found during:** Task 2
- **Issue:** reqwest workspace feature set doesn't include `form`, so `RequestBuilder::form()` is unavailable
- **Fix:** Used `serde_urlencoded::to_string()` to build the body and set Content-Type header manually
- **Files modified:** crates/blufio-sms/src/api.rs, crates/blufio-sms/Cargo.toml
- **Verification:** `cargo test -p blufio-sms` passes (21 tests)
- **Committed in:** f1c38f3 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 blocking issues)
**Impact on plan:** Both fixes necessary for compilation. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- IMessageChannel and SmsChannel ready for gateway wiring in serve.rs
- Webhook routes (imessage_webhook_routes, sms_webhook_routes) ready for Router::merge() composition
- Both adapters export inbound_tx() for webhook state construction
- All 28 unit tests pass; integration tests deferred to Phase 63

## Self-Check: PASSED

All 8 modified source files verified present. Both task commits (4ff7a8a, f1c38f3) verified in git log. All files exceed minimum line counts (lib.rs: 282/80, 295/80; webhook.rs: 186/40, 343/60; api.rs: 193/30, 241/30).

---
*Phase: 61-channel-adapters*
*Completed: 2026-03-13*
