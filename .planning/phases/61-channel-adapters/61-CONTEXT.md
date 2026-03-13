# Phase 61: Channel Adapters - Context

**Gathered:** 2026-03-13
**Status:** Ready for planning

<domain>
## Phase Boundary

Add Email (IMAP/SMTP), iMessage (BlueBubbles), and SMS (Twilio) channel adapters implementing ChannelAdapter + PluginAdapter traits with FormatPipeline integration. Three new crates: blufio-email, blufio-imessage, blufio-sms. Webhook routes for iMessage and SMS on the existing gateway. Full bridge support.

</domain>

<decisions>
## Implementation Decisions

### Email Adapter (blufio-email)

**Inbound (IMAP):**
- Configurable polling interval (default 30 seconds) via TOML
- Monitor INBOX only by default, configurable folder list for advanced users
- Thread-to-session mapping via In-Reply-To/References headers (new thread = new session)
- Trim quoted text from incoming emails before processing (strip > prefix, "On ... wrote:" blocks, Outlook "From:" blocks, signature blocks)
- Strip HTML to plaintext for HTML-only inbound emails using html2text crate
- Prepend subject line to body as context ("Subject: Re: Help with X\n\n{body}")
- Mark processed emails as \Seen in IMAP (no folder moves)
- Ignore attachments for now (media provider traits deferred per PROJECT.md)
- Use async-imap crate for async IMAP client
- Require TLS by default (IMAPS port 993 or STARTTLS), configurable allow_insecure = false for testing

**Outbound (SMTP):**
- Use lettre crate for SMTP (already in roadmap requirements)
- HTML with plaintext fallback (multipart/alternative) for outbound emails
- Markdown-to-HTML conversion using pulldown-cmark or comrak crate
- Reply subject: "Re: {original_subject}" (don't double Re: prefix)
- Configurable display name (from_name = "Blufio AI", default "Blufio")
- Reply-To matches From address (no separate Reply-To config)
- Optional configurable footer (email_footer TOML field, disabled by default)
- LOGIN + PLAIN + STARTTLS authentication
- Shared credentials for IMAP and SMTP by default, optional separate SMTP credentials
- SMTP send failures: fail fast, log error, no retry (email is best-effort)

**Authorization:**
- Allowed senders list in TOML (allowed_senders), empty = accept all
- Credentials stored in blufio.toml (consistent with other adapters)

### iMessage Adapter (blufio-imessage)

**BlueBubbles Integration:**
- Webhook-based incoming messages (BlueBubbles POSTs to /webhooks/imessage on gateway)
- Configurable BlueBubbles URL (bluebubbles_url TOML field, default localhost with standard BB port)
- API password stored in TOML config
- Configurable webhook_callback_url for auto-registration with BlueBubbles on connect()
- Shared secret validation on webhook endpoint (prevent unauthorized calls)
- Health check pings BlueBubbles /api/v1/server/info endpoint
- Auto-reconnect with exponential backoff (1s, 2s, 4s... up to 60s) on connection drop
- Retry 5xx errors from BlueBubbles API, fail fast on 4xx

**Session Mapping:**
- Per chat/conversation mapping (BlueBubbles chat GUID -> Blufio session)
- Support both 1:1 and group chats
- Group chat trigger: configurable prefix (group_trigger = "Blufio", default "Blufio")

**Capabilities:**
- Typing indicators via BlueBubbles API (if supported, fallback to no-op)
- Send read receipts after processing
- Log delivery status at debug level (don't expose to agent loop)
- Ignore tapback reactions
- Plaintext only for outbound (no rich text via BlueBubbles)
- Allowed contacts list (allowed_contacts TOML field, empty = accept all)

**Experimental Status:**
- Config-level warning + docs noting macOS requirement (not a hard error on Linux, just warns at startup)

### SMS Adapter (blufio-sms)

**Twilio Integration:**
- Webhook inbound at /webhooks/sms on gateway (Twilio POSTs)
- REST API outbound via raw reqwest calls (no Twilio SDK dependency)
- Return empty 200 OK to webhook, then send reply asynchronously via Twilio REST API
- Validate X-Twilio-Signature HMAC on inbound webhooks using Twilio Auth Token
- Health check calls Twilio Account API to verify credentials/status
- Respect Retry-After header on HTTP 429, single retry, then fail
- Log delivery status via Twilio status callbacks at debug level

**Session & Message Handling:**
- Per phone number session mapping (one session per sender number)
- Multi-segment via Twilio (full response sent, Twilio auto-splits/concatenates)
- Configurable max response length (default 1600 chars / ~10 segments)
- E.164 phone number format required (+1234567890), validated at startup
- Always set From number (configured twilio_phone_number)
- Direct phone number sending (not Messaging Service SID)

**Authorization & Compliance:**
- Allowed numbers list (allowed_numbers TOML field, empty = accept all)
- Respect STOP/UNSUBSCRIBE keywords at application level
- Ignore MMS (text SMS only, media deferred)
- Configurable outbound rate limit (default 1 msg/second per number)

**Config:**
- account_sid and auth_token as sibling fields in [sms] config section
- Fixed webhook path /webhooks/sms
- Log delivery failures, emit ChannelEvent::DeliveryFailed on EventBus

### Adapter Capabilities (ChannelCapabilities)

| Capability | Email | iMessage | SMS |
|---|---|---|---|
| streaming_type | None | None | None |
| formatting_support | FullMarkdown | PlainText | PlainText |
| max_message_length | None | 20000 | 1600 |
| supports_code_blocks | true | false | false |
| supports_typing | false | true (if BB supports) | false |
| supports_edit | false (no-op) | false | false |
| supports_images | false | false | false |
| supports_documents | false | false | false |
| supports_embeds | false | false | false |
| supports_reactions | false | false | false |
| supports_threads | false | false | false |

### Cross-Channel Bridging
- All three adapters bridge-compatible, registered with BridgeGroupConfig
- Adapter names: "email", "imessage", "sms" (matching name() return values)
- FormatPipeline handles format degradation automatically (no custom bridge logic)
- Email bridge: full bidirectional, individual emails per bridged message
- Email bridge subject: "[SourceChannel] message text" tag
- SMS bridge: sender prefix "[Channel/Sender]: message text"
- SMS bridge: per-bridge-group rate limiting (default 1 msg/min to SMS targets)
- iMessage bridge: dedicated bridge chat GUID (separate from direct conversations)

### Error Handling
- Email IMAP polling: exponential backoff retry (5s, 10s, 20s... up to 300s) on connection failure
- All adapters integrate with per-dependency circuit breakers (IMAP server, BlueBubbles, Twilio)
- Webhook errors: return 200 OK + log error internally (prevent retry storms)
- Adapter failures emit ChannelEvent::ConnectionLost / ChannelEvent::DeliveryFailed on EventBus
- Invalid webhook payloads logged at debug level
- SMTP: fail fast, no retry
- Twilio: respect Retry-After on 429, single retry
- BlueBubbles: retry 5xx, fail fast on 4xx

### Config Validation
- Required fields validated at new() time (fail fast on missing credentials)
- connect() does connectivity pre-flight: IMAP login, BlueBubbles ping, Twilio Account API
- Email: basic @ format check on username
- SMS: E.164 format check on phone number (starts with +, digits only)
- iMessage: basic URL format check on bluebubbles_url (http:// or https://)
- Gateway must be enabled for webhook routes (warn at startup if webhooks configured but gateway off)

### Crate & Wiring
- Three new crates: blufio-email, blufio-imessage, blufio-sms
- serve.rs conditional wiring: check config fields (email.imap_host.is_some(), etc.), same pattern as Discord/Slack
- Webhook routes added to gateway: /webhooks/imessage, /webhooks/sms

### Testing
- Unit tests + mock HTTP (wiremock/httpmock) for API interactions
- Config validation tests (missing fields, invalid values)
- Message parsing tests: MIME parsing with sample email fixtures, thread detection edge cases
- Quoted-text stripping tests: Gmail, Outlook, Apple Mail patterns
- Twilio signature validation: dedicated tests with known test vectors
- BlueBubbles webhook payload parsing: sample JSON fixtures
- Full integration tests deferred to Phase 63 (QUAL-06)

### Claude's Discretion
- Exact MIME parsing library choice (mailparse vs mail-parser)
- HTML email template styling
- Exact exponential backoff parameters
- Internal module structure within each crate
- Error message wording

</decisions>

<specifics>
## Specific Ideas

- Follow the Discord adapter (blufio-discord/src/lib.rs) as the structural template: Handler struct, mpsc channel for inbound, Mutex on receiver, capabilities method, FormatPipeline integration in send()
- Email quoted-text stripping should handle Gmail, Outlook, and Apple Mail quoting patterns
- Twilio webhook signature validation is security-critical and needs dedicated test coverage
- MIME email parsing should handle multipart/alternative, quoted-printable, base64 encoding with real-world test fixtures

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ChannelAdapter` trait (crates/blufio-core/src/traits/channel.rs): 6 methods including default no-ops for edit_message and send_typing
- `PluginAdapter` trait (crates/blufio-core/src/traits/adapter.rs): name, version, adapter_type, health_check, shutdown
- `FormatPipeline` (blufio-core::format): detect_and_format() + split_at_paragraphs() used by all existing adapters
- `ChannelCapabilities` struct with StreamingType, FormattingSupport, RateLimit fields
- `BlufioError::channel_delivery_failed()` and `channel_connection_lost()` error constructors
- `InboundMessage` / `OutboundMessage` / `MessageId` types in blufio-core::types
- `mpsc::channel(100)` + `tokio::sync::Mutex<Receiver>` pattern used by all channel adapters
- `BridgeGroupConfig` in blufio-config for cross-channel bridge rules
- Circuit breaker infrastructure in blufio-core for per-dependency breakers
- EventBus with ChannelEvent variants for adapter status events

### Established Patterns
- Config structs in blufio-config/model.rs with `#[serde(deny_unknown_fields)]` and `#[serde(default)]`
- Optional credential fields (Option<String>) to enable/disable adapters
- Handler struct holds Arc<HandlerState> with inbound_tx and allowed_users
- FormatPipeline 4-step: detect -> format -> split -> escape (adapter-specific escape function)
- Adapter name() returns short lowercase string matching TOML section name
- health_check() calls external API to verify connectivity

### Integration Points
- serve.rs: conditional adapter instantiation based on config field presence
- Gateway axum router: add /webhooks/imessage and /webhooks/sms routes
- blufio-config/model.rs: add EmailConfig, IMessageConfig, SmsConfig structs
- Workspace Cargo.toml: add three new crate members
- blufio (binary) Cargo.toml: add three new dependencies

</code_context>

<deferred>
## Deferred Ideas

- Email attachment processing (images, PDFs) -- depends on media provider trait implementations
- iMessage rich text formatting -- BlueBubbles API limitations
- MMS support for SMS -- depends on media provider traits
- OAuth2 for SMTP (Gmail/Outlook app password deprecation) -- future enhancement
- IMAP IDLE push notification support -- future enhancement for lower latency
- Email digest/batch mode for bridges -- future enhancement

</deferred>

---

*Phase: 61-channel-adapters*
*Context gathered: 2026-03-13*
