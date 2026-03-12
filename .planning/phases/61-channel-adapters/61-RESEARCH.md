# Phase 61: Channel Adapters - Research

**Researched:** 2026-03-13
**Domain:** Rust channel adapters (Email/IMAP/SMTP, iMessage/BlueBubbles, SMS/Twilio) implementing ChannelAdapter + PluginAdapter traits
**Confidence:** HIGH

## Summary

Phase 61 adds three new channel adapters -- Email (IMAP/SMTP), iMessage (BlueBubbles), and SMS (Twilio) -- each in a dedicated crate following the established adapter pattern. The project has a mature, well-documented adapter architecture: seven channel adapters already exist (Telegram, Discord, Slack, WhatsApp, Signal, IRC, Matrix), and the Discord/WhatsApp adapters serve as structural templates. The key patterns are: `mpsc::channel(100)` for inbound messages, `tokio::sync::Mutex<Receiver>` for async receive, `FormatPipeline::detect_and_format()` + `split_at_paragraphs()` for outbound formatting, and webhook routes merged into the gateway via `set_extra_public_routes()`.

The Email adapter is the most complex of the three due to IMAP polling, MIME parsing, quoted-text stripping, and multipart/alternative HTML+plaintext outbound. The iMessage adapter is webhook-driven (BlueBubbles POSTs to the gateway) with a simple REST outbound. The SMS adapter mirrors the iMessage webhook pattern for inbound but adds Twilio-specific HMAC-SHA1 signature validation and E.164 phone number handling. All three adapters have well-defined decisions from the CONTEXT.md, leaving only internal module structure and exact library choices (MIME parser) to Claude's discretion.

**Primary recommendation:** Follow the WhatsApp adapter as the closest structural template (webhook inbound, REST API outbound, gateway route merging) for iMessage and SMS. Follow the Discord adapter for the general ChannelAdapter trait implementation pattern. For Email, combine the polling-based pattern from Signal (background loop) with WhatsApp's webhook state pattern for shared state.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Email Adapter (blufio-email): async-imap for IMAP, lettre for SMTP, html2text for HTML stripping, comrak or pulldown-cmark for Markdown-to-HTML, configurable polling interval (default 30s), In-Reply-To/References header thread mapping, quoted-text stripping (Gmail/Outlook/Apple Mail patterns), \Seen flag on processed emails, TLS required by default, LOGIN+PLAIN+STARTTLS auth, multipart/alternative outbound, allowed_senders list, shared IMAP/SMTP credentials by default
- iMessage Adapter (blufio-imessage): BlueBubbles REST API, webhook at /webhooks/imessage, API password in TOML, webhook_callback_url auto-registration, shared secret validation, health check via /api/v1/server/info, exponential backoff (1s-60s), per chat GUID session mapping, group chat trigger prefix, typing indicators, read receipts, ignore tapbacks, plaintext outbound only, experimental status with macOS warning
- SMS Adapter (blufio-sms): Twilio Programmable Messaging API, webhook at /webhooks/sms, raw reqwest calls (no Twilio SDK), return empty 200 OK then send async, X-Twilio-Signature HMAC validation, per phone number sessions, multi-segment via Twilio auto-split, E.164 format validation, STOP/UNSUBSCRIBE keyword handling, 1 msg/second rate limit, account_sid/auth_token in [sms] config
- Three new crates: blufio-email, blufio-imessage, blufio-sms
- All adapters implement ChannelAdapter + PluginAdapter traits
- All adapters integrate with FormatPipeline
- Webhook routes added to gateway at /webhooks/imessage and /webhooks/sms
- Config structs in blufio-config/model.rs
- serve.rs conditional wiring
- Unit tests + mock HTTP (wiremock/httpmock) for API interactions
- ChannelCapabilities: Email=FullMarkdown/None streaming, iMessage=PlainText/None streaming, SMS=PlainText/None streaming/1600 max

### Claude's Discretion
- Exact MIME parsing library choice (mailparse vs mail-parser)
- HTML email template styling
- Exact exponential backoff parameters
- Internal module structure within each crate
- Error message wording

### Deferred Ideas (OUT OF SCOPE)
- Email attachment processing (images, PDFs)
- iMessage rich text formatting
- MMS support for SMS
- OAuth2 for SMTP (Gmail/Outlook app password deprecation)
- IMAP IDLE push notification support
- Email digest/batch mode for bridges
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CHAN-01 | Email adapter with IMAP polling for incoming messages and SMTP (lettre) for outgoing | async-imap 0.11.2 for IMAP, lettre 0.11.19 for SMTP. Polling loop pattern from Signal adapter. FormatPipeline integration from Discord template. |
| CHAN-02 | Email thread-to-session mapping via In-Reply-To/References headers | mail-parser 0.11.2 provides RFC-compliant header extraction. Thread detection via Message-ID/In-Reply-To/References per RFC 5322. |
| CHAN-03 | iMessage adapter via BlueBubbles REST API with webhook for incoming messages | BlueBubbles REST API at /api/v1/message/text (outbound), webhook registration at /api/v1/webhook, auth via ?password= query param. Webhook pattern mirrors WhatsApp adapter. |
| CHAN-04 | iMessage adapter documented as experimental (BlueBubbles requires macOS host) | Config-level warning at startup (not hard error), doc comments noting macOS requirement. |
| CHAN-05 | SMS adapter via Twilio Programmable Messaging API (webhook inbound + REST outbound) | Twilio REST at https://api.twilio.com/2010-04-01/Accounts/{sid}/Messages.json, webhook receives application/x-www-form-urlencoded with From/To/Body/MessageSid params. HMAC-SHA1 signature validation. |
| CHAN-06 | All three adapters implement existing ChannelAdapter + PluginAdapter traits | Follow Discord adapter pattern: Handler struct, mpsc channel, Mutex receiver, capabilities(), connect(), send(), receive(). |
| CHAN-07 | All three adapters support FormatPipeline integration | FormatPipeline::detect_and_format() + split_at_paragraphs() in send(). PlainText channels get markdown degradation automatically. Email uses FullMarkdown passthrough. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| async-imap | 0.11.2 | Async IMAP client for email polling | De facto async IMAP crate for Rust, maintained by chatmail project, supports TLS/STARTTLS |
| lettre | 0.11.19 | SMTP email sending | Dominant Rust email sending crate (1.4M+ downloads), full RFC 5321 compliance, multipart/alternative support |
| mail-parser | 0.11.2 | MIME email parsing (headers, body extraction) | Higher RFC compliance than mailparse, zero-copy with Cow<str>, 41 charset support, user-friendly body extraction (auto text/html separation) |
| html2text | 0.16.7 | HTML-to-plaintext conversion for inbound emails | Standard Rust HTML-to-text crate, uses html5ever (Servo parser), handles real-world HTML email well |
| comrak | 0.51.0 | Markdown-to-HTML conversion for outbound emails | 100% CommonMark + GFM compliant, used by crates.io/docs.rs/GitLab, Rust 1.85+ compatible (matches workspace) |
| reqwest | 0.13 (workspace) | HTTP client for BlueBubbles and Twilio REST APIs | Already in workspace, used by all adapters that make HTTP calls |
| hmac + sha1 | 0.12 + 0.10 | Twilio webhook HMAC-SHA1 signature validation | hmac already in workspace (0.12), need sha1 for Twilio-specific HMAC-SHA1 (NOT SHA256) |
| axum | 0.8 (workspace) | Webhook route handlers for iMessage and SMS | Already in workspace, used by gateway |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| base64 | 0.22 | Base64 encoding for Twilio HMAC-SHA1 signature comparison | Twilio signature is Base64-encoded HMAC-SHA1, not hex |
| tokio-rustls or async-native-tls | latest | TLS backend for async-imap connections | async-imap accepts any TLS stream; use rustls (already in workspace via reqwest) |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| mail-parser | mailparse 0.16.1 | mailparse is simpler and more downloads (2.2M vs 107K) but lower RFC compliance, less charset support. mail-parser provides auto text/html body separation which is exactly what we need for email processing. |
| comrak | pulldown-cmark | pulldown-cmark is faster (pull parser) but comrak has 100% GFM compliance and is used by crates.io. Comrak matches workspace edition (Rust 1.85+). |
| raw reqwest for Twilio | twilio-rs SDK | No mature official Twilio Rust SDK exists. Raw reqwest is the decision from CONTEXT.md and avoids adding an unmaintained dependency. |

**Installation (new workspace dependencies):**
```bash
# In workspace Cargo.toml:
# async-imap = "0.11"
# lettre = { version = "0.11", default-features = false, features = ["smtp-transport", "tokio1-rustls-tls", "hostname", "builder"] }
# mail-parser = "0.11"
# html2text = "0.16"
# comrak = "0.51"
# sha1 = "0.10"
# base64 = "0.22"
```

## Architecture Patterns

### Recommended Project Structure
```
crates/
  blufio-email/
    src/
      lib.rs           # EmailChannel struct, ChannelAdapter + PluginAdapter impl
      imap.rs          # IMAP polling loop, message fetch, session mapping
      smtp.rs          # SMTP sending via lettre, multipart/alternative
      parsing.rs       # MIME parsing, quoted-text stripping, HTML-to-text
      config.rs        # Re-exports from blufio-config (optional helpers)
    Cargo.toml
  blufio-imessage/
    src/
      lib.rs           # IMessageChannel struct, ChannelAdapter + PluginAdapter impl
      api.rs           # BlueBubbles REST API client (send message, server info, webhook reg)
      webhook.rs       # Axum route handlers, webhook payload parsing, secret validation
      types.rs         # BlueBubbles JSON payload types (webhook events, API responses)
    Cargo.toml
  blufio-sms/
    src/
      lib.rs           # SmsChannel struct, ChannelAdapter + PluginAdapter impl
      api.rs           # Twilio REST API client (send message, account status)
      webhook.rs       # Axum route handlers, form parsing, signature validation
      types.rs         # Twilio request/response types
    Cargo.toml
```

### Pattern 1: Webhook-Based Adapter (iMessage, SMS)
**What:** Webhook handlers push InboundMessages via mpsc channel, adapter's receive() reads from the Mutex-wrapped receiver.
**When to use:** External service POSTs to our gateway (BlueBubbles, Twilio).
**Example:**
```rust
// Source: blufio-whatsapp/src/cloud.rs (existing pattern)
pub struct IMessageChannel {
    config: IMessageConfig,
    inbound_rx: Mutex<mpsc::Receiver<InboundMessage>>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    http_client: Option<reqwest::Client>,
}

impl IMessageChannel {
    pub fn new(config: IMessageConfig) -> Result<Self, BlufioError> {
        // Validate required config fields
        let (inbound_tx, inbound_rx) = mpsc::channel(100);
        Ok(Self {
            config,
            inbound_rx: Mutex::new(inbound_rx),
            inbound_tx,
            http_client: None,
        })
    }

    pub fn inbound_tx(&self) -> mpsc::Sender<InboundMessage> {
        self.inbound_tx.clone()
    }
}
```

### Pattern 2: Polling-Based Adapter (Email)
**What:** Background tokio task polls IMAP server at intervals, pushes InboundMessages through mpsc channel.
**When to use:** External service requires polling (IMAP has no push notification in this implementation).
**Example:**
```rust
// Email adapter uses a spawned background task for IMAP polling
pub struct EmailChannel {
    config: EmailConfig,
    inbound_rx: Mutex<mpsc::Receiver<InboundMessage>>,
    inbound_tx: mpsc::Sender<InboundMessage>,
    smtp_transport: Option<lettre::AsyncSmtpTransport<lettre::Tokio1Executor>>,
    poll_handle: Option<tokio::task::JoinHandle<()>>,
}

// In connect():
// 1. Establish IMAP connection (TLS) and verify login
// 2. Build SMTP transport and verify connection
// 3. Spawn background polling task that:
//    a. Selects INBOX
//    b. Searches for UNSEEN messages
//    c. Fetches each, parses with mail-parser
//    d. Strips quoted text, extracts body
//    e. Maps thread via In-Reply-To/References -> session_id metadata
//    f. Sends InboundMessage via inbound_tx
//    g. Marks as \Seen
//    h. Sleeps for polling_interval
//    i. On connection failure: exponential backoff retry
```

### Pattern 3: Webhook Route Merging (Gateway Integration)
**What:** Multiple webhook routers composed via Router::merge() and set as extra_public_routes on gateway.
**When to use:** Adding webhook endpoints for adapters that receive HTTP callbacks.
**Example:**
```rust
// In serve.rs -- compose all webhook routes into one Router
let mut webhook_routes = Router::new();

#[cfg(feature = "imessage")]
if let Some(ref state) = imessage_webhook_state {
    webhook_routes = webhook_routes.merge(
        blufio_imessage::webhook::imessage_webhook_routes(state.clone())
    );
}

#[cfg(feature = "sms")]
if let Some(ref state) = sms_webhook_state {
    webhook_routes = webhook_routes.merge(
        blufio_sms::webhook::sms_webhook_routes(state.clone())
    );
}

// Combine with existing WhatsApp routes
#[cfg(feature = "whatsapp")]
if let Some(ref state) = _whatsapp_webhook_state {
    webhook_routes = webhook_routes.merge(
        blufio_whatsapp::webhook::whatsapp_webhook_routes(state.clone())
    );
}

if !webhook_routes_empty {
    gateway.set_extra_public_routes(webhook_routes).await;
}
```

### Pattern 4: FormatPipeline Integration in send()
**What:** All adapters use the same 4-step pipeline: detect -> format -> escape -> split.
**When to use:** Every adapter's send() method.
**Example:**
```rust
// Source: blufio-discord/src/lib.rs (existing pattern)
async fn send(&self, msg: OutboundMessage) -> Result<MessageId, BlufioError> {
    let caps = self.capabilities();
    let formatted = FormatPipeline::detect_and_format(&msg.content, &caps);
    // For PlainText adapters (iMessage, SMS): formatted is already degraded
    // For FullMarkdown (Email): formatted passes through unchanged
    let chunks = split_at_paragraphs(&formatted, caps.max_message_length);
    // Send each chunk
}
```

### Anti-Patterns to Avoid
- **Blocking IMAP calls on the async runtime:** async-imap is fully async; never use the sync imap crate or block_on() in an async context.
- **Hardcoding webhook paths outside the adapter:** Webhook path constants should live in the adapter crate (e.g., `pub const WEBHOOK_PATH: &str = "/webhooks/sms"`) and be referenced from serve.rs.
- **Implementing custom Twilio HMAC without testing:** Twilio uses HMAC-SHA1 with Base64 encoding (not hex), which is different from the WhatsApp HMAC-SHA256 hex pattern already in the project. Requires dedicated test vectors.
- **Merging extra_public_routes multiple times:** The gateway's set_extra_public_routes replaces the value, not appends. Compose all webhook routers into one Router before setting.
- **Missing E.164 validation:** Twilio requires strict +{digits} format. Validate at config time, not message time.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| MIME email parsing | Custom RFC 822 parser | mail-parser crate | MIME is deceptively complex: multipart boundaries, charset encoding, quoted-printable, base64, nested parts. mail-parser handles 41 charsets and all RFC 2045-2049 edge cases. |
| HTML-to-text conversion | Custom HTML stripper with regex | html2text crate | HTML email is malformed, deeply nested, CSS-styled. html2text uses html5ever (Servo's parser) for robust handling. Regex HTML stripping fails on real-world email. |
| Markdown-to-HTML conversion | Custom markdown renderer | comrak crate | GFM tables, code blocks, nested lists all need correct rendering for email HTML bodies. |
| SMTP transport | Raw TCP socket SMTP | lettre crate | SMTP requires EHLO handshaking, TLS negotiation, MIME multipart construction, proper header encoding. lettre handles RFC 5321 fully. |
| Twilio signature validation | Custom HMAC implementation | hmac + sha1 crates (already established pattern in project) | Timing-safe comparison, correct Base64 encoding, URL construction rules. One-off implementation is error-prone. |
| Email quoted-text stripping | Simple regex for "> " lines | Dedicated strip_quoted_text function with multiple patterns | Gmail, Outlook, Apple Mail all use different quoting conventions. Need pattern matching for "On ... wrote:", "From: ...", "> " prefixes, and signature blocks ("-- \n"). |

**Key insight:** Email is the most treacherous domain here. Every email client formats replies differently, MIME has decades of edge cases, and charset encoding is a minefield. Use established parsers and test with real-world email fixtures.

## Common Pitfalls

### Pitfall 1: IMAP Connection Drops
**What goes wrong:** IMAP connections drop silently after idle timeout (typically 30 minutes). The polling loop continues but receives no messages.
**Why it happens:** IMAP servers close idle connections per RFC 3501 Section 5.4. Polling at 30s intervals keeps the connection alive, but network issues or server restarts cause drops.
**How to avoid:** Wrap the IMAP polling loop in a reconnection loop with exponential backoff. Detect connection loss via fetch/select errors and re-establish the session. The CONTEXT.md specifies 5s->10s->20s...->300s backoff.
**Warning signs:** Empty SEARCH results over extended periods, connection errors in debug logs.

### Pitfall 2: Twilio HMAC-SHA1 vs Project's HMAC-SHA256
**What goes wrong:** Using the WhatsApp webhook's HMAC-SHA256 hex verification pattern for Twilio validation.
**Why it happens:** Copy-pasting from the existing WhatsApp webhook handler. Twilio uses HMAC-SHA1 with Base64 encoding (not SHA256, not hex).
**How to avoid:** Implement Twilio validation as a separate function. Key differences: (1) HMAC-SHA1 not SHA256, (2) data string = URL + sorted POST params concatenated, (3) signature is Base64-encoded not hex-encoded, (4) form-urlencoded body not JSON.
**Warning signs:** All webhook validations fail in testing.

### Pitfall 3: Email Thread Detection Gaps
**What goes wrong:** New email threads get mapped to existing sessions or vice versa.
**Why it happens:** Not all email clients set In-Reply-To/References headers correctly. Some clients strip or mangle Message-ID headers.
**How to avoid:** Use both In-Reply-To AND References headers for matching. Fall back to Subject line matching ("Re: " prefix with matching subject) as secondary heuristic. Generate stable Message-IDs for outbound emails. Store Message-ID -> session_id mapping.
**Warning signs:** Conversations crossing session boundaries, orphaned sessions.

### Pitfall 4: Gateway Extra Public Routes Replacement
**What goes wrong:** Setting extra_public_routes for SMS overwrites the WhatsApp/iMessage routes.
**Why it happens:** `set_extra_public_routes()` replaces the entire Router, not appends.
**How to avoid:** Compose ALL webhook routes into a single Router using `Router::merge()` before calling `set_extra_public_routes()` once. This is the pattern shown in Architecture Pattern 3.
**Warning signs:** WhatsApp webhooks stop working after adding SMS/iMessage adapters.

### Pitfall 5: Twilio Webhook Response Expectations
**What goes wrong:** Returning JSON or non-empty body causes Twilio to interpret it as TwiML, sending unexpected messages to users.
**Why it happens:** Twilio expects TwiML in webhook responses. An empty 200 OK (or empty TwiML `<Response></Response>`) means "no reply". Any text body gets treated as a reply.
**How to avoid:** Return `StatusCode::OK` with empty body (or minimal empty TwiML). Process the message asynchronously and send the reply via the Twilio REST API in a separate request. The CONTEXT.md correctly specifies this pattern.
**Warning signs:** Users receive duplicate or garbled responses.

### Pitfall 6: BlueBubbles Authentication Method
**What goes wrong:** Sending API key in Authorization header or request body.
**Why it happens:** Assuming standard REST API auth patterns.
**How to avoid:** BlueBubbles uses query parameter authentication: `?password={api_password}` on every request. This is different from most modern APIs.
**Warning signs:** 401 responses from BlueBubbles server.

## Code Examples

Verified patterns from existing codebase and official documentation:

### Email: MIME Parsing with mail-parser
```rust
// Source: mail-parser docs (https://docs.rs/mail-parser/)
use mail_parser::MessageParser;

fn parse_email(raw: &[u8]) -> Option<(String, String, Option<String>)> {
    let message = MessageParser::default().parse(raw)?;

    // Extract text body (auto-selects text/plain from multipart/alternative)
    let body = message.body_text(0)?.to_string();

    // Extract subject
    let subject = message.subject()?.to_string();

    // Extract In-Reply-To for thread mapping
    let in_reply_to = message.in_reply_to()
        .as_text_list()
        .and_then(|list| list.first().map(|s| s.to_string()));

    Some((subject, body, in_reply_to))
}
```

### Email: Sending via lettre with multipart/alternative
```rust
// Source: lettre docs (https://docs.rs/lettre/)
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
    Message as LettreMessage,
    message::{header::ContentType, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
};

async fn send_email(
    transport: &AsyncSmtpTransport<Tokio1Executor>,
    from: &str,
    to: &str,
    subject: &str,
    text_body: &str,
    html_body: &str,
) -> Result<(), BlufioError> {
    let email = LettreMessage::builder()
        .from(from.parse().map_err(|e| BlufioError::Config(format!("invalid from: {e}")))?)
        .to(to.parse().map_err(|e| BlufioError::Config(format!("invalid to: {e}")))?)
        .subject(subject)
        .multipart(
            MultiPart::alternative()
                .singlepart(SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(text_body.to_string()))
                .singlepart(SinglePart::builder()
                    .header(ContentType::TEXT_HTML)
                    .body(html_body.to_string()))
        )
        .map_err(|e| BlufioError::channel_delivery_failed("email", e))?;

    transport.send(email).await
        .map_err(|e| BlufioError::channel_delivery_failed("email", e))?;
    Ok(())
}
```

### Twilio: HMAC-SHA1 Signature Validation
```rust
// Source: Twilio docs (https://www.twilio.com/docs/usage/webhooks/webhooks-security)
use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

/// Validate Twilio X-Twilio-Signature header.
///
/// 1. Start with the full URL (including scheme and query params)
/// 2. Sort POST parameters alphabetically by key
/// 3. Append each key+value to the URL string
/// 4. HMAC-SHA1 with auth_token as key
/// 5. Base64-encode the result
/// 6. Compare with X-Twilio-Signature header
fn validate_twilio_signature(
    auth_token: &str,
    url: &str,
    params: &[(String, String)],
    signature: &str,
) -> bool {
    let mut data = url.to_string();

    // Sort parameters alphabetically by key
    let mut sorted_params = params.to_vec();
    sorted_params.sort_by(|a, b| a.0.cmp(&b.0));

    // Append key+value pairs
    for (key, value) in &sorted_params {
        data.push_str(key);
        data.push_str(value);
    }

    let mut mac = HmacSha1::new_from_slice(auth_token.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(data.as_bytes());
    let result = mac.finalize();

    use base64::Engine;
    let computed = base64::engine::general_purpose::STANDARD.encode(result.into_bytes());

    computed == signature
}
```

### WhatsApp Webhook Pattern (Template for iMessage/SMS)
```rust
// Source: blufio-whatsapp/src/webhook.rs (existing codebase)
// This is the exact pattern to follow for iMessage and SMS webhook handlers

pub struct SmsWebhookState {
    pub inbound_tx: mpsc::Sender<InboundMessage>,
    pub auth_token: String,    // For HMAC validation
    pub webhook_url: String,   // Full URL for signature computation
}

pub fn sms_webhook_routes(state: SmsWebhookState) -> Router {
    Router::new()
        .route("/webhooks/sms", axum::routing::post(sms_webhook))
        .with_state(state)
}

async fn sms_webhook(
    State(state): State<SmsWebhookState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // 1. Validate X-Twilio-Signature
    // 2. Parse form-urlencoded body
    // 3. Check STOP/UNSUBSCRIBE keywords
    // 4. Create InboundMessage
    // 5. Send via inbound_tx
    // 6. Return empty 200 OK (critical: no body to prevent TwiML interpretation)
    StatusCode::OK
}
```

### Email Quoted-Text Stripping Patterns
```rust
/// Strip quoted text from email replies.
///
/// Handles common patterns:
/// - Gmail: "On Mon, Jan 1, 2026 at 12:00 PM User <user@example.com> wrote:"
/// - Outlook: "From: User\nSent: ...\nTo: ...\nSubject: ..."
/// - Apple Mail: "On Jan 1, 2026, at 12:00 PM, User <user@example.com> wrote:"
/// - Generic: lines starting with ">"
/// - Signature blocks: "-- \n" (note trailing space per RFC 3676)
fn strip_quoted_text(text: &str) -> String {
    let mut result = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    for (i, line) in lines.iter().enumerate() {
        // Stop at signature delimiter
        if *line == "-- " || *line == "--" {
            break;
        }

        // Stop at "On ... wrote:" pattern (Gmail/Apple Mail)
        if line.starts_with("On ") && line.ends_with(" wrote:") {
            break;
        }

        // Stop at Outlook "From:" block
        if line.starts_with("From: ") && i + 1 < lines.len()
            && lines[i + 1].starts_with("Sent: ")
        {
            break;
        }

        // Skip quoted lines
        if line.starts_with("> ") || line.starts_with(">") {
            continue;
        }

        result.push(*line);
    }

    // Trim trailing whitespace
    let text = result.join("\n");
    text.trim_end().to_string()
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Sync IMAP (rust-imap) | async-imap (tokio-native) | 2024 | No blocking on async runtime, better integration with tokio ecosystem |
| lettre 0.10 | lettre 0.11 | 2024 | Async transport support (Tokio1Executor), builder pattern for messages |
| Manual MIME parsing | mail-parser with auto body extraction | 2023 | body_text()/body_html() auto-select from multipart, handle charset conversion |
| Twilio SDK (unofficial) | Raw reqwest | N/A | No mature Rust Twilio SDK exists; raw HTTP is the standard approach |

**Deprecated/outdated:**
- `imap` crate (sync): Use `async-imap` instead for async contexts
- lettre 0.9/0.10: Use 0.11 for async transport support
- `native-tls` backend: Project uses rustls throughout; use `tokio-rustls` for IMAP TLS

## Open Questions

1. **async-imap TLS backend selection**
   - What we know: async-imap accepts any async TLS stream. The project uses rustls everywhere (reqwest, serenity).
   - What's unclear: Whether to use `tokio-rustls` directly or `async-native-tls` with rustls backend for the IMAP connection.
   - Recommendation: Use `tokio-rustls` directly for consistency with the rest of the project. This requires constructing a `TlsConnector` from rustls and wrapping the TCP stream manually, which is a few lines of boilerplate.

2. **ChannelEvent::ConnectionLost / DeliveryFailed variants**
   - What we know: CONTEXT.md specifies emitting these events. Current ChannelEvent enum only has MessageReceived and MessageSent.
   - What's unclear: Whether to add these variants to the ChannelEvent enum in blufio-bus or handle via a different event type.
   - Recommendation: Add ConnectionLost and DeliveryFailed variants to ChannelEvent in blufio-bus/src/events.rs. This is a simple extension of the existing enum and follows the pattern of other event sub-enums being extended as needed.

3. **Multiple webhook Router composition**
   - What we know: Gateway currently supports one Router via set_extra_public_routes(). Now we need up to 3 webhook routers (WhatsApp, iMessage, SMS).
   - What's unclear: Whether to change the gateway API or compose in serve.rs.
   - Recommendation: Compose all webhook routers into a single Router via merge() in serve.rs before calling set_extra_public_routes(). No gateway API changes needed.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test + tokio::test |
| Config file | None (Cargo-standard test discovery) |
| Quick run command | `cargo test -p blufio-email -p blufio-imessage -p blufio-sms` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CHAN-01 | Email IMAP polling + SMTP sending | unit | `cargo test -p blufio-email` | Wave 0 |
| CHAN-02 | Thread-to-session via In-Reply-To/References | unit | `cargo test -p blufio-email::imap::tests` | Wave 0 |
| CHAN-03 | iMessage BlueBubbles webhook + REST | unit | `cargo test -p blufio-imessage` | Wave 0 |
| CHAN-04 | iMessage experimental docs | manual-only | N/A (doc review) | N/A |
| CHAN-05 | SMS Twilio webhook + REST | unit | `cargo test -p blufio-sms` | Wave 0 |
| CHAN-06 | ChannelAdapter + PluginAdapter impl | unit | `cargo test -p blufio-email -p blufio-imessage -p blufio-sms -- adapter` | Wave 0 |
| CHAN-07 | FormatPipeline integration | unit | `cargo test -p blufio-email -p blufio-imessage -p blufio-sms -- format` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p blufio-email -p blufio-imessage -p blufio-sms`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/blufio-email/` -- new crate, all tests needed
- [ ] `crates/blufio-imessage/` -- new crate, all tests needed
- [ ] `crates/blufio-sms/` -- new crate, all tests needed
- [ ] Email MIME parsing tests with real-world email fixtures (Gmail, Outlook, Apple Mail)
- [ ] Quoted-text stripping tests covering all three client patterns
- [ ] Twilio HMAC-SHA1 validation tests with known test vectors
- [ ] BlueBubbles webhook payload parsing with sample JSON fixtures
- [ ] Config validation tests for each adapter (missing fields, invalid formats)
- [ ] E.164 phone number format validation tests

## Sources

### Primary (HIGH confidence)
- Existing codebase: blufio-discord/src/lib.rs, blufio-whatsapp/src/webhook.rs, blufio-whatsapp/src/cloud.rs -- structural templates
- Existing codebase: blufio-core/src/traits/channel.rs, blufio-core/src/traits/adapter.rs -- trait definitions
- Existing codebase: blufio-core/src/types.rs -- InboundMessage, OutboundMessage, ChannelCapabilities
- Existing codebase: blufio-core/src/format.rs -- FormatPipeline::detect_and_format(), split_at_paragraphs()
- Existing codebase: blufio-config/src/model.rs -- Config struct patterns
- Existing codebase: serve.rs -- Adapter wiring and feature-gate patterns
- crates.io: async-imap 0.11.2, lettre 0.11.19, mail-parser 0.11.2, comrak 0.51.0, html2text 0.16.7

### Secondary (MEDIUM confidence)
- [BlueBubbles REST API docs](https://docs.bluebubbles.app/server/developer-guides/rest-api-and-webhooks) -- API endpoints and webhook events
- [Twilio webhook security docs](https://www.twilio.com/docs/usage/webhooks/webhooks-security) -- HMAC-SHA1 validation process
- [Twilio Messages resource](https://www.twilio.com/docs/messaging/api/message-resource) -- REST API for sending SMS
- [Twilio webhook request guide](https://www.twilio.com/docs/messaging/guides/webhook-request) -- Inbound webhook parameters
- [lettre docs](https://docs.rs/lettre/) -- AsyncSmtpTransport, Message builder, multipart
- [async-imap docs](https://docs.rs/async-imap/) -- Client, Session, TLS connection
- [mail-parser docs](https://docs.rs/mail-parser/) -- MessageParser, body_text(), in_reply_to()

### Tertiary (LOW confidence)
- BlueBubbles Postman collection -- API endpoint paths (may differ by BB server version)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All crates verified on crates.io with current versions, workspace compatibility confirmed (Rust 1.85+ / edition 2024)
- Architecture: HIGH - Seven existing channel adapters provide clear, battle-tested patterns. Discord and WhatsApp are direct templates.
- Pitfalls: HIGH - Identified from codebase analysis (gateway single-router, HMAC algorithm difference) and domain knowledge (email complexity, Twilio TwiML response behavior)
- Email domain specifics: MEDIUM - mail-parser recommendation based on API comparison; real-world email parsing edge cases will emerge during implementation
- BlueBubbles API: MEDIUM - Documented but version-dependent; exact payload structures may vary

**Research date:** 2026-03-13
**Valid until:** 2026-04-13 (30 days -- stable domain, all crates are mature)
