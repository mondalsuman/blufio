# Domain Pitfalls: v1.5 PRD Gap Closure

**Domain:** Adding multi-level compaction, prompt injection defense, cron/scheduler, memory temporal decay, hash-chained audit trail, data classification, retention policies, hook system, hot reload, iMessage/Email/SMS channels, PII redaction, GDPR tooling, OpenTelemetry, OpenAPI spec, Litestream replication, data export, and Clippy unwrap enforcement to an existing 80K LOC Rust AI agent platform (35 crates, 1,444 unwrap() calls)
**Researched:** 2026-03-10
**Confidence:** HIGH for Rust/SQLite/tokio specifics (verified against codebase); HIGH for Litestream+SQLCipher incompatibility (confirmed via GitHub issue #177 "wontfix"); HIGH for OpenTelemetry-Prometheus coexistence (verified deprecation of opentelemetry-prometheus crate); MEDIUM for prompt injection defense rates (research papers verified but production data varies); MEDIUM for PII regex patterns (well-documented failure modes); LOW for BlueBubbles stability (limited production deployment data)

---

## Critical Pitfalls

Mistakes that cause rewrites, data loss, security vulnerabilities, or make the system fundamentally worse.

---

### Pitfall 1: Litestream Cannot Replicate SQLCipher-Encrypted Databases

**What goes wrong:**
Litestream replicates by reading SQLite WAL frames directly. SQLCipher encrypts the WAL data with the database key, so Litestream sees binary garbage and returns "file is not a database." This is a fundamental incompatibility, not a configuration issue. The Litestream maintainer closed the SQLCipher support issue as "wontfix" in June 2021, stating it would require compiling a different version of SQLite with SQLCipher changes -- something they deemed too complex.

Blufio uses SQLCipher when `BLUFIO_DB_KEY` is set (see `crates/blufio-storage/src/database.rs`). The entire connection factory (`open_connection`, `open_connection_sync`) applies `PRAGMA key` as the first statement. Litestream cannot intercept after decryption because it operates at the filesystem level, not the SQLite API level.

**Why it happens:**
The PRD specifies both SQLCipher encryption at rest (shipped in v1.2) and Litestream WAL replication (v1.5 target). These two features are architecturally incompatible. The assumption is that "SQLite + WAL + Litestream" is a well-trodden path (true for plaintext databases), but encryption changes the game entirely.

**Consequences:**
- Litestream integration cannot work for encrypted deployments -- the most security-conscious users
- If implemented without testing against encrypted databases, it silently fails or produces corrupt backups
- Users who enable both features get a false sense of replication safety

**How to avoid:**
Choose one of three strategies:
1. **Application-level replication:** Instead of WAL-level replication, implement periodic backup + upload. Use the existing `blufio backup` command (which does WAL checkpoint + file copy) on a cron schedule and upload to object storage. This works with SQLCipher because the backup API operates through SQLite's connection, after decryption.
2. **Encrypt-after-replicate:** Run Litestream on unencrypted databases and encrypt the replicated data at the object storage level (e.g., S3 SSE, GCS CMEK). This trades at-rest encryption on the local disk for replication capability. Less secure but functional.
3. **Conditional feature:** Enable Litestream only when `BLUFIO_DB_KEY` is NOT set. Document the incompatibility clearly. Add a startup check that errors if both features are configured.

Recommendation: Strategy 1 (application-level backup + upload) because it works universally and does not weaken the encryption posture.

**Warning signs:**
- Litestream process starts but never uploads any WAL frames
- Restore from Litestream backup produces "file is not a database" errors
- WAL file size grows unboundedly because Litestream cannot checkpoint

**Phase to address:**
Litestream/Infrastructure phase. Must be addressed at design time, not as a bug fix.

---

### Pitfall 2: Multi-Level Compaction Loses Critical Information Silently

**What goes wrong:**
The current single-pass compaction (`crates/blufio-context/src/compaction.rs`) uses a Haiku LLM call to summarize conversation history. The prompt explicitly preserves "user preferences, names, commitments, key decisions, action items, emotional tone, facts about the user." Moving to multi-level compaction (L0 raw -> L1 turn summaries -> L2 session summaries -> L3 persona distillation) introduces a lossy compression chain. Each level discards information. The critical failure: information that seems unimportant at L1 (when context is fresh) becomes critical at L3 (when it is the only remaining record). For example, a user mentions "I'm allergic to shellfish" once in passing -- L1 might preserve it, but L2 condenses it away among more "important" business decisions, and by L3 it is gone. The agent then recommends a seafood restaurant.

**Why it happens:**
LLM-based summarization is non-deterministic. Quality scoring (proposed: evaluate if compacted output preserves key facts) requires knowing which facts are key -- which is itself an AI-hard problem. Developers test with short conversations where information loss is invisible. Long-running sessions (weeks, months of daily use) where compaction chains accumulate errors are never tested.

**Consequences:**
- Loss of user preferences, personal facts, or safety-critical information
- Agent personality drift over long sessions (compaction rewrites tone)
- User trust erosion ("I already told you this")
- Quality gate passes because it measures aggregate quality, not specific fact retention

**How to avoid:**
1. **Never compact explicitly stated preferences.** Before compaction, extract and persist critical facts to the memory system (blufio-memory) as separate Memory entries. These bypass the compaction chain entirely.
2. **Quality gates must test specific facts, not just summary quality.** Before accepting a compaction, verify that named entities, numbers, and user-stated facts from the input appear in the output. Use a checklist approach: extract key facts from input, verify each appears in output.
3. **Keep L0 raw messages in cold storage.** Compaction should summarize for context window purposes but never delete original messages. Add an `archived` status and a `cold_storage` table. This is the escape hatch when compaction quality fails.
4. **Cap compaction depth.** L3 (persona distillation) should only be generated from L0 raw messages, not from L2 summaries. Re-derive from source when possible, even if more expensive.
5. **Test with 1000+ message conversations.** Generate synthetic long conversations with planted facts and verify fact retention after full compaction chain.

**Warning signs:**
- Users report "I already told you" more than once per 50 turns
- Memory system and compaction summaries contain contradictory facts
- Quality scores are high but user satisfaction is low

**Phase to address:**
Compaction & Context phase. Quality scoring and cold storage archival must be designed together, not bolted on.

---

### Pitfall 3: GDPR Erasure Conflicts with Hash-Chained Audit Trail

**What goes wrong:**
Hash-chained audit logs create an append-only, tamper-evident record where each entry includes the hash of the previous entry. Deleting any entry breaks the chain -- subsequent entries' "previous_hash" fields no longer verify, making the entire chain appear tampered. GDPR Article 17 (right to erasure) requires deleting personal data upon request. These two requirements directly conflict: you cannot delete personal data from an audit trail without destroying tamper evidence.

This is not a hypothetical -- the EU AI Act and GDPR create explicit tension. GDPR mandates erasure; the AI Act demands lengthy archival of system documentation. The deletion process (GDPR) must happen first, leaving only the audit trail behind -- but the audit trail cannot contain the deleted personal data.

**Why it happens:**
Audit trails and GDPR erasure are designed in separate phases by different mental models. The audit team thinks "nothing can be deleted." The compliance team thinks "everything must be deletable." Neither considers the other's constraint until implementation.

**Consequences:**
- Compliance deadlock: cannot satisfy both GDPR and tamper evidence simultaneously
- Expensive rearchitecture if discovered after audit trail is deployed
- Legal exposure if GDPR erasure request cannot be fulfilled

**How to avoid:**
Design for GDPR from day one of the audit trail:
1. **Separate PII from audit events.** Store the audit event (who did what, when, hash chain) in the audit table. Store PII (user name, email, message content) in a separate linked table with a foreign key. On GDPR erasure, null out the PII columns but keep the audit event and its hash. The hash covers the event metadata (action type, timestamp, actor_id), not the PII content.
2. **Hash only non-PII fields.** The hash chain should cover: event_id, timestamp, action_type, actor_id (opaque UUID, not email), resource_type, resource_id, previous_hash. It should NOT cover: user_name, email, message_content, IP address. This way, erasing PII does not break the hash chain.
3. **Use pseudonymized actor IDs.** The audit trail references `actor_id = "usr_abc123"`, not `actor_id = "john@example.com"`. The mapping from UUID to real identity is stored separately and deleted on erasure.
4. **Document the approach in the GDPR transparency disclosure.** Explain that audit logs retain anonymized event records but personal data is erasable.

**Warning signs:**
- Audit table schema includes columns like `user_email`, `user_name`, or `message_content`
- Hash computation includes any field that might need to be erased
- No separation between audit metadata and PII

**Phase to address:**
Must be co-designed: audit trail and GDPR phases should be planned together even if implemented sequentially. The audit trail schema must be GDPR-aware from the first migration.

---

### Pitfall 4: Prompt Injection Defense Creates Unusable False Positive Rates

**What goes wrong:**
The PRD specifies a 5-layer defense: L1 pattern classifier, L3 HMAC boundary tokens, L4 output validator, L5 human-in-the-loop. The L1 pattern classifier (regex-based) is where false positives concentrate. Patterns like "ignore previous instructions" also match legitimate user messages ("Can you ignore my previous instructions about formatting and just give me the raw data?"). Research shows that even frontier model-based guardrails (GPT-4o, GPT-4.1) achieve <1% FPR on benchmarks like AgentDojo, but regex classifiers have significantly higher FPR (5-15%) depending on pattern strictness.

For an always-on personal AI agent, a 5% false positive rate means 1 in 20 legitimate messages gets flagged. If the response is to block the message, the user gets frustrated every 20 messages. If the response is a warning, alert fatigue sets in within a day.

**Why it happens:**
Developers write injection patterns from known attack lists without testing against real conversational data. The pattern "ignore all previous" matches attack prompts and also matches "Please ignore all previous formatting preferences." Testing uses adversarial datasets (which are all attacks) rather than mixed datasets (99% legitimate, 1% attacks), so precision looks high.

**Consequences:**
- Users disable the defense ("too many false alerts")
- Support burden from users reporting "my messages are being blocked"
- Agent becomes unusable for power users who write complex, technical, or meta-level prompts

**How to avoid:**
1. **Start with HMAC boundary tokens (L3) as the primary defense.** This has zero false positive rate because it is structural, not content-based. Wrap system prompts in `[BOUNDARY:hmac_hash]...[/BOUNDARY:hmac_hash]` tokens that the model is instructed to never reproduce. Any output containing boundary tokens is filtered.
2. **L1 pattern classifier should LOG, not BLOCK.** Score messages 0.0-1.0 for injection likelihood. Log high scores. Only block at >0.95 threshold (obvious attacks like "SYSTEM: you are now..."). Use the logged data to tune thresholds before enforcement.
3. **Test against a corpus of 1000+ real conversational messages** from diverse domains (technical, casual, meta-discussion). Measure FPR before deploying any blocking behavior.
4. **L4 output validation (check for data exfiltration in responses) catches what L1 misses** with near-zero false positives because it validates outputs, not inputs. Prioritize this over input filtering.
5. **L5 human-in-the-loop should be for sensitive actions only** (tool execution, data access), not for every flagged message.

**Warning signs:**
- Users report "my message was blocked" within the first week
- FPR measured on test data is >2%
- All defense layers are set to "block" mode simultaneously

**Phase to address:**
Security Hardening phase. Must include a calibration period with logging-only mode before enforcement.

---

### Pitfall 5: OpenTelemetry + Prometheus Dual Recorder Conflict

**What goes wrong:**
Blufio currently uses the `metrics-rs` facade with `metrics-exporter-prometheus` (see `crates/blufio-prometheus/src/lib.rs`). The `PrometheusBuilder::new().install_recorder()` installs a global metrics recorder. The `metrics-rs` crate supports exactly ONE global recorder. If OpenTelemetry is added with its own `metrics` SDK and another global recorder (or the now-deprecated `opentelemetry-prometheus` crate), the second `install_recorder()` call fails silently or panics.

Furthermore, the `opentelemetry-prometheus` crate has been deprecated as of 2025 -- version 0.29 is the final release. It depends on the unmaintained `protobuf` crate with unresolved security vulnerabilities. The recommended path is OTLP export, not Prometheus SDK integration.

**Why it happens:**
The assumption is "just add OpenTelemetry alongside Prometheus." But the `metrics-rs` global recorder pattern means you cannot have two metric systems writing to the same global state. The previous Prometheus integration was designed as the sole observability adapter. Adding OTel requires either replacing it or running them in parallel with careful isolation.

**Consequences:**
- Second recorder installation panics at startup (breaking change)
- Metric names diverge between Prometheus and OTel exporters
- Deprecated crate introduces security vulnerabilities
- Double-counting if both systems record the same events

**How to avoid:**
1. **Use OpenTelemetry as the single metrics facade.** Replace `metrics-rs` with `opentelemetry` SDK for metrics. Use the `opentelemetry-prometheus` bridge (note: this is different from the deprecated crate -- the OTLP-based approach is recommended) or expose metrics via OTLP and let an OpenTelemetry Collector scrape/export to Prometheus.
2. **Keep `metrics-rs` for Prometheus, use OTel only for tracing.** Since the PRD says "OpenTelemetry distributed tracing (optional, disabled by default)", the simplest approach is: Prometheus stays as-is for metrics, OpenTelemetry adds tracing only (spans, not metrics). The `tracing-opentelemetry` crate bridges `tracing` spans to OTel without touching the metrics recorder.
3. **If both metric systems are needed**, use the `metrics-tracing-context` layer to bridge tracing context into metrics labels, but keep separate export paths.

Recommendation: Option 2. Keep existing Prometheus for metrics (it works, it is tested). Add OTel for distributed tracing only. This avoids the recorder conflict entirely.

**Warning signs:**
- `install_recorder()` returns an error at startup
- Metric counts differ between `/metrics` endpoint and OTel export
- Dependency audit flags `protobuf` crate vulnerabilities

**Phase to address:**
Observability phase. Architecture decision must be made before any OTel code is written.

---

### Pitfall 6: Hot Reload with ArcSwap Causes Partial Configuration States

**What goes wrong:**
The proposed hot reload uses ArcSwap to atomically swap configuration. ArcSwap itself is sound -- the swap is atomic. The pitfall is what happens AFTER the swap. If the configuration change requires multiple downstream actions (e.g., new TLS cert requires re-binding the listener, new provider config requires updating circuit breaker thresholds, new channel config requires reconnecting the adapter), these actions are NOT atomic. Between the config swap and the completion of all downstream updates, the system is in an inconsistent state: new config loaded, but old TLS cert still serving, old circuit breaker thresholds active, old channel connections live.

The current config model (`crates/blufio-config/src/model.rs`) uses `#[serde(deny_unknown_fields)]` on ALL structs (BlufioConfig, AgentConfig, TelegramConfig, etc.). Adding new config fields for hot-reload features requires adding them to these structs. If a hot-reload config file has a field that the current binary does not recognize, `deny_unknown_fields` rejects the entire config, causing a reload failure.

**Why it happens:**
ArcSwap documentation emphasizes the atomic swap, which developers interpret as "the whole reload is atomic." But the swap only makes the new config visible -- it does not ensure all consumers have reacted to it. With 35 crates potentially reading config, the propagation delay can be significant.

**Consequences:**
- TLS cert rotation leaves old cert serving for seconds/minutes
- Provider config change creates mismatch between routing logic and circuit breaker state
- `deny_unknown_fields` causes config reload failures during rolling upgrades

**How to avoid:**
1. **Single ArcSwap, not per-component.** Store the entire `BlufioConfig` in one `ArcSwap<Arc<BlufioConfig>>`. Components use the `arc_swap::access::Access` trait to project into their section. This ensures all components see the same config version.
2. **Reload is config swap + ordered propagation.** After swapping, notify all affected components via EventBus events (e.g., `ConfigReloaded { changed_sections: Vec<String> }`). Each component that receives the event re-reads its config section and applies changes. Order matters: TLS reload before accepting new connections, circuit breaker update before routing changes.
3. **Validate before swap.** Parse and validate the new config fully before calling `arc_swap.store()`. If validation fails, log the error and keep the old config. Never swap to a partially valid config.
4. **Version the reloadable config.** Not all config is safe to reload. Database path, encryption key, and bind address cannot change at runtime. Partition config into `static` (requires restart) and `reloadable` sections. Reject hot reload of static fields.
5. **For `deny_unknown_fields` during upgrades:** Add a `#[serde(default)]` fallback for new optional sections, and consider a `strict_reload: bool` flag that relaxes `deny_unknown_fields` during hot reload only.

**Warning signs:**
- Logs show "config reloaded" but behavior does not change
- TLS cert rotation test shows old cert served after reload
- Config reload in CI fails due to `deny_unknown_fields` on new fields

**Phase to address:**
Hot Reload phase. Must design the propagation protocol before implementing any hot-reload feature.

---

## Moderate Pitfalls

Mistakes that cause significant bugs, performance problems, or wasted effort, but are recoverable without full rewrites.

---

### Pitfall 7: Cron Scheduler Timer Drift and Missed Jobs After Sleep/Suspend

**What goes wrong:**
Tokio's `tokio::time::sleep` and `tokio::time::interval` use monotonic time (Instant), not wall clock time. On a VPS that never suspends, this works fine. But: (a) On development macOS (where Blufio explicitly supports development), laptop sleep causes Instant to jump forward by the sleep duration, potentially missing multiple cron triggers. (b) Under heavy load, the tokio scheduler may delay timer execution beyond the 1-second check interval, causing missed cron ticks. (c) Over long uptimes (months, per Blufio's design goal), floating-point drift in interval calculations accumulates.

The PRD specifies the system should run "for months without restart" on a $4/month VPS. A cron scheduler that drifts or misses jobs undermines this goal.

**Why it happens:**
Developers test with short intervals (every 5 seconds) and see correct behavior. Drift only manifests over hours/days. The tokio-cron-scheduler crate documents that "time drift is possible over long uptime without correction logic" and jobs do not persist across restarts.

**How to avoid:**
1. **Use wall clock time for cron evaluation.** On each tick, compute `Utc::now()` and evaluate the cron expression against it. Do not rely on interval duration accuracy.
2. **Persist last execution time per job in SQLite.** On startup and after every tick, read `last_run_at` from the database. If `now - last_run_at > interval`, the job was missed and should execute immediately (catch-up mode) or skip (no-catch-up mode, configurable per job).
3. **Tick at 1-second granularity using `tokio::time::interval(Duration::from_secs(1))`** but compare against wall clock time, not accumulated ticks. If the interval fires late (e.g., 3 seconds instead of 1), evaluate all 3 seconds' worth of cron expressions.
4. **Implement jitter for jobs scheduled at the same time** to prevent thundering herd when multiple cron jobs fire simultaneously (e.g., all at midnight).

**Warning signs:**
- Cron job fires at :00:01 instead of :00:00 after a few hours
- Jobs missed after macOS laptop wake
- Multiple cron jobs fire simultaneously causing load spikes

**Phase to address:**
Cron/Scheduler phase.

---

### Pitfall 8: Memory Temporal Decay Causes Cold Start Amnesia

**What goes wrong:**
Temporal decay (proposed: `0.95^days`) reduces memory relevance scores over time. The intent is good: recent memories are more relevant. The problem: after extended inactivity (user goes on vacation for 2 weeks), ALL memories have decayed significantly. `0.95^14 = 0.488` -- every memory is at half strength. The agent "forgets" everything equally, including critical facts like the user's name, job, preferences. When the user returns, the agent behaves as if it has amnesia.

Worse: the PRD specifies "bounded index with LRU eviction (default 10,000)." If decay scores drop below the eviction threshold and LRU eviction kicks in, memories are actually deleted, not just deprioritized.

**Why it happens:**
Decay functions are tested with daily-active-user patterns. Nobody tests "what happens after 30 days of inactivity?" The `0.95^30 = 0.215` problem is invisible in active testing.

**How to avoid:**
1. **Decay must have a floor.** No memory should decay below a configurable minimum (e.g., 0.3). Formula: `max(0.3, 0.95^days)`. This ensures that even ancient memories retain some baseline relevance.
2. **Importance-weighted decay.** High-importance memories (user-stated facts, preferences, safety info) decay slower than low-importance ones (transient observations). Formula: `max(floor, base^(days / importance_factor))` where importance_factor ranges from 1.0 (normal) to 3.0 (critical).
3. **Separate decay from eviction.** Decay adjusts retrieval ranking. LRU eviction is based on access patterns (last retrieved), not decay score. A memory that has not been accessed in 90 days can be evicted. A memory that was accessed last week but has high age should NOT be evicted.
4. **Refresh on access.** When a memory is retrieved and included in context, reset its decay timer. Memories that keep being relevant stay fresh.
5. **Cold start detection.** If `days_since_last_interaction > threshold`, temporarily boost all memory scores by a "welcome back" factor. This prevents the amnesia effect.

**Warning signs:**
- Agent fails to recall user's name after a week of inactivity
- Memory retrieval returns zero results for queries that worked a week ago
- LRU eviction removes high-importance but stale memories

**Phase to address:**
Memory Enhancements phase.

---

### Pitfall 9: PII Regex Matches Code Blocks, URLs, and Technical Content

**What goes wrong:**
The existing redaction system (`crates/blufio-security/src/redact.rs`) uses regex patterns for API keys and bearer tokens -- these are high-precision patterns with low false positive rates because their formats are distinctive (e.g., `sk-ant-[a-zA-Z0-9_\-]{20,}`). PII patterns (email, phone, SSN, credit card) have fundamentally different characteristics: they match common string formats that appear in non-PII contexts.

Examples of false positives:
- Phone regex `\d{3}-\d{3}-\d{4}` matches semantic version ranges like "123-456-7890" in code, date ranges, and order numbers
- Email regex matches `user@host` in code comments, configuration examples, and documentation
- SSN regex `\d{3}-\d{2}-\d{4}` matches dates formatted as `123-45-6789` in timestamps
- Credit card regex `\d{4}[\s-]?\d{4}[\s-]?\d{4}[\s-]?\d{4}` matches 16-digit numeric strings in technical data

For a coding assistant (which Blufio supports), false positives on code blocks are especially damaging -- the agent redacts parts of code it is supposed to help with.

**Why it happens:**
PII patterns are designed for maximum recall (catch all PII). In a general-purpose AI agent handling diverse content types (code, configuration, natural language, logs), the false positive rate skyrockets because the content domain is unbounded.

**How to avoid:**
1. **Context-aware redaction.** Detect content type before applying patterns. If the message contains code blocks (markdown fenced blocks, or content from a coding skill), apply only the high-precision patterns (API keys, bearer tokens). Skip phone/SSN patterns inside code blocks.
2. **Luhn checksum for credit cards.** Do not rely on digit patterns alone. Validate the Luhn checksum before redacting. This eliminates false positives from random 16-digit sequences.
3. **Email validation beyond regex.** Check that the domain part has a valid TLD. `user@localhost` and `var@type` are not emails.
4. **Phone number normalization.** Use a pattern that requires country code or known area code prefix, not bare `\d{3}-\d{3}-\d{4}`. International patterns vary wildly -- do not try to cover all locales with one regex.
5. **Allowlist patterns.** Maintain a set of "looks like PII but is not" patterns: semantic versions, date formats, UUIDs, hex strings. Check against allowlist before redacting.
6. **Keep the existing two-tier approach.** High-precision (API keys, bearer tokens) always on. PII patterns opt-in per deployment, with configurable sensitivity levels.

**Warning signs:**
- Agent's code output contains `[REDACTED]` in places where no PII exists
- Users report "the agent is censoring my code"
- Redaction logs show >5% of messages triggering PII redaction

**Phase to address:**
PII Redaction phase. Must include a false-positive test suite with code samples, URLs, and technical content.

---

### Pitfall 10: Hook System Infinite Loops via Cascading Event Triggers

**What goes wrong:**
The proposed hook system fires on 11 lifecycle events, with hooks integrated into the existing EventBus. A hook that fires on `channel.message_sent` and sends a message (e.g., a logging hook that posts to a channel) triggers another `channel.message_sent` event, which triggers the hook again. Infinite loop.

The existing bridge system already solved this for cross-channel bridging with the `is_bridged: bool` flag on `ChannelEvent::MessageReceived` (see `crates/blufio-bus/src/events.rs:131`). But hooks introduce a more general version of the problem: any hook that modifies state visible to other hooks creates potential for cascading loops.

**Why it happens:**
Each hook is designed and tested in isolation. Hook A works correctly alone. Hook B works correctly alone. Together, A triggers B which triggers A. The combinatorial explosion of hook interactions is not tested.

**How to avoid:**
1. **Recursion depth counter.** Add a `hook_depth: u8` field to event metadata. Each hook invocation increments depth. If depth exceeds a configurable maximum (default: 3), the event is dropped with a warning log. This is the same pattern the bridge uses with `is_bridged`.
2. **Hook isolation.** Events generated by hooks are marked with `source: "hook"`. Hooks can filter: `trigger_on_hook_events: false` (default) means hooks do not fire on events generated by other hooks.
3. **Priority ordering matters for determinism.** Hooks fire in BTreeMap priority order (proposed). Document that equal-priority hooks have undefined execution order. Require unique priorities or use priority + name as the sorting key.
4. **Timeout per hook execution.** Shell-based hooks (proposed) must have a configurable timeout (default: 30 seconds). A hung hook blocks the event pipeline. Use `tokio::process::Command` with `timeout`.
5. **Error isolation.** A failed hook must not prevent subsequent hooks from firing. Log the error, continue to next hook. Never propagate hook errors to the caller that generated the event.

**Warning signs:**
- CPU spikes to 100% after installing two or more hooks
- EventBus channel fills up (1024 capacity) and reliable subscribers start dropping events
- Hook execution logs show the same event ID appearing multiple times

**Phase to address:**
Hook System phase. Loop prevention must be in the first iteration, not added later.

---

### Pitfall 11: Hash-Chained Audit Trail Becomes a Write Bottleneck

**What goes wrong:**
A hash-chained audit log requires sequential writes: each entry's hash depends on the previous entry's hash. This means audit writes cannot be parallelized or batched without breaking the chain. In Blufio's single-writer SQLite architecture (all writes go through one `tokio_rusqlite::Connection` background thread), every audit write adds to the single-writer queue. If every API call, message, session operation, and tool invocation generates an audit entry, the write queue grows unboundedly during bursts.

Current write patterns: message insert, session update, cost ledger update. Adding audit entries potentially doubles the write count per operation.

**Why it happens:**
Developers implement the hash chain correctly but do not measure write latency under load. In testing (1-2 concurrent sessions), the overhead is invisible. Under 10+ concurrent sessions with active tool use, the single-writer thread becomes the bottleneck.

**How to avoid:**
1. **Batch audit writes.** Buffer audit entries in memory (bounded channel) and flush to SQLite in batches every 100ms or 100 entries, whichever comes first. Compute hash chains in the batch. The chain is still sequential within each batch, but the SQLite write is a single transaction.
2. **Async hash computation.** The SHA-256 hash for each entry is cheap (~1 microsecond) but the SQLite write is not. Pre-compute the hash chain in memory and write the batch to SQLite in a single INSERT with VALUES list.
3. **Separate audit database.** Use a second SQLite database file for audit logs. This gives audit its own single-writer thread, preventing audit writes from blocking operational writes. Litestream (or the backup alternative) can replicate them independently.
4. **Selective auditing.** Not every event needs audit logging. Define audit levels: `critical` (auth events, data access, deletions), `standard` (session lifecycle, tool execution), `verbose` (every message). Default to `standard`. Let operators configure.
5. **Periodic chain verification.** Do not verify the entire chain on every read. Verify on startup, on backup, and on demand via a CLI command.

**Warning signs:**
- Write latency for messages increases after audit trail is enabled
- Queue depth metrics show sustained growth during active sessions
- SQLite WAL file grows faster than expected

**Phase to address:**
Audit Trail phase.

---

### Pitfall 12: Retention Policy Cascading Deletes Break Referential Integrity

**What goes wrong:**
The current database has `PRAGMA foreign_keys = ON` (see `database.rs:211`). Retention policies that delete old messages, sessions, or memories must respect foreign key constraints. The `messages` table references `sessions`. The `memories` table references `sessions` (via `session_id`). Queue entries reference sessions. Deleting a session without first deleting or nullifying dependent rows fails with a foreign key violation. Conversely, using `ON DELETE CASCADE` without understanding the full dependency graph can silently delete audit entries, cost records, and memories that should be retained.

SQLite foreign keys have a specific pitfall: they are disabled by default and must be enabled per-connection. If the retention job opens a new connection (e.g., a separate thread for cleanup), it may not have foreign keys enabled, causing silent referential integrity violations.

**Why it happens:**
Retention policies are implemented as "delete old rows" without analyzing the full schema dependency graph. Developers test with fresh databases (no dependent rows). Production databases have complex cross-table references that evolve over migrations.

**How to avoid:**
1. **Map the full dependency graph before implementing retention.** Document which tables reference which others. For Blufio: sessions -> messages, sessions -> memories (session_id), sessions -> queue, sessions -> cost entries.
2. **Soft-delete first, hard-delete later.** Retention policy marks rows as `status = 'expired'`. A separate garbage collection job hard-deletes expired rows in correct dependency order: leaf tables first (messages, memories), then parent tables (sessions).
3. **Never use ON DELETE CASCADE for tables with audit significance.** Use ON DELETE RESTRICT and handle deletion order in application code. This prevents accidental cascade deletions.
4. **Always use the centralized connection factory** (`open_connection`) for retention jobs. Never open raw `rusqlite::Connection` directly. This ensures `PRAGMA foreign_keys = ON` is always set.
5. **Retention must exclude audit entries.** Audit trail retention has its own policy (longer, or indefinite). Create a separate retention category for audit data.

**Warning signs:**
- `FOREIGN KEY constraint failed` errors in retention job logs
- Audit entries disappear after retention run
- Memory count drops unexpectedly after session cleanup

**Phase to address:**
Retention Policy phase. Must come after audit trail design so retention can respect audit constraints.

---

### Pitfall 13: Data Classification Over-Classification Paralyzes Operations

**What goes wrong:**
The PRD specifies 4 classification levels: Public/Internal/Confidential/Restricted. The pitfall: when in doubt, operators classify everything as Restricted. This is the "better safe than sorry" instinct. Result: all data requires maximum access controls, all exports are blocked, all API responses are filtered. The system becomes operationally useless because every action requires elevated permissions.

In an AI agent context, this is especially problematic: if conversation messages are classified as Restricted, the context engine cannot assemble prompts without Restricted-level access. Every LLM call becomes a Restricted operation. Memory retrieval is Restricted. The classification system adds overhead to every request path without differentiation.

**Why it happens:**
Classification is designed by security-minded developers who default to maximum restriction. Real data sensitivity is nuanced: a user's name is Internal, their SSN is Restricted, a weather query is Public. Without clear guidelines and defaults, operators either over-classify (paranoid) or under-classify (ignore it entirely).

**How to avoid:**
1. **Default classification per data type.** Messages: Internal. Memories: Internal. Audit logs: Confidential. Credentials: Restricted. Embeddings: Internal. Session metadata: Internal. Provide sensible defaults that operators can override.
2. **Classification determines handling rules, not access gates.** Public: no restrictions. Internal: redact in exports, include in API responses. Confidential: encrypt at rest, redact in logs. Restricted: encrypt at rest, redact in logs and API responses, require explicit access grants.
3. **Automatic classification based on content.** If PII patterns are detected, auto-elevate to Confidential. If credential patterns are detected, auto-elevate to Restricted. This reduces the classification burden on operators.
4. **Classification is metadata, not a permission gate.** Store classification level as a column on each row. Use it for filtering exports and redaction, not for blocking reads. The agent must always be able to read its own data to function.
5. **Provide a `classify` CLI command** that audits current data and reports classification distribution. If >80% of data is Restricted, the operator has over-classified and should adjust defaults.

**Warning signs:**
- All data is classified at the same level
- Operators report "everything is locked down, nothing works"
- Export and backup commands refuse to run due to classification restrictions

**Phase to address:**
Data Classification phase.

---

## Minor Pitfalls

Mistakes that cause developer friction, minor bugs, or suboptimal behavior.

---

### Pitfall 14: Clippy unwrap Enforcement Breaks 1,444 Call Sites at Once

**What goes wrong:**
Adding `#![deny(clippy::unwrap_used)]` to library crates causes all 1,444 `unwrap()` calls to become compile errors simultaneously. This is an all-or-nothing change that makes the codebase unbuildable until every single call is fixed. With 35 crates, this is a multi-day effort that blocks all other development.

**How to avoid:**
1. **Phased rollout per crate.** Start with leaf crates (blufio-core, blufio-bus, blufio-config) that have fewer unwrap() calls. Add `#![warn(clippy::unwrap_used)]` first (warnings, not errors). Fix warnings. Then promote to `#![deny(clippy::unwrap_used)]`.
2. **Categorize unwrap() calls.** Not all unwrap() calls are equal:
   - **Test code:** `#[cfg(test)]` modules can keep `unwrap()`. Add `#[allow(clippy::unwrap_used)]` to test modules.
   - **Proven safe:** `"constant".parse::<Uri>().unwrap()` where the input is a compile-time constant. Add `#[allow(clippy::unwrap_used)] // SAFETY: constant input` with a comment.
   - **Actually fallible:** These need conversion to `?`, `.unwrap_or_default()`, `.expect("reason")`, or proper error handling. These are the real bugs to fix.
3. **Use `expect()` instead of `unwrap()` as an intermediate step.** `expect("descriptive message")` is allowed by `clippy::unwrap_used` but denied by `clippy::expect_used`. Migrate to `expect()` first (quick, mechanical), then to proper error handling (slower, requires thought).
4. **CI gate:** Add `cargo clippy -- -W clippy::unwrap_used` to CI as a warning. Track the count over time. Set a target: "reduce by 100 per phase."

**Warning signs:**
- PR that adds `deny(clippy::unwrap_used)` to all crates simultaneously
- Build fails on CI after lint change with 1000+ errors
- Developers add `#[allow(clippy::unwrap_used)]` everywhere to make it compile

**Phase to address:**
Code Quality phase. Must be a gradual process, not a single PR.

---

### Pitfall 15: BlueBubbles iMessage Adapter Relies on macOS-Only Sidecar

**What goes wrong:**
BlueBubbles requires a macOS host running the BlueBubbles Server application, which interfaces with iMessage through AppleScript (basic) or the Private API (advanced). The Private API is more reliable but exceptions crash the entire iMessage process, requiring restart. For a Blufio deployment on a Linux VPS ($4/month), the BlueBubbles server must run on a separate macOS machine (real or virtual), introducing a network dependency and an additional point of failure.

**How to avoid:**
1. **Design the adapter as a remote client.** The BlueBubbles REST API + WebSocket is the integration point. The adapter connects to a remote BlueBubbles server URL, not a local process. Handle connection failures gracefully with the existing circuit breaker system.
2. **Private API is recommended.** AppleScript-based sending is unreliable. Document that operators should enable the Private API on their BlueBubbles server for reliable message delivery.
3. **Reconnection logic.** BlueBubbles server restarts (macOS updates, Private API crashes) cause WebSocket disconnections. Implement exponential backoff reconnection, not just one retry.
4. **Rate limiting.** Apple does not publish iMessage rate limits, but anecdotal evidence suggests aggressive sending can trigger throttling or temporary blocks. Implement a conservative send rate limit (e.g., 1 message per second, 30 per minute).

**Warning signs:**
- Adapter connects but messages never arrive
- WebSocket disconnections every few hours
- "Transaction timeout" errors on send

**Phase to address:**
Additional Channels phase.

---

### Pitfall 16: Email Adapter Deliverability and Spam Classification

**What goes wrong:**
Sending emails from a VPS IP address almost always triggers spam classification. Without SPF, DKIM, DMARC records, the agent's emails go to spam folders. For an AI agent that responds to emails, this means responses silently vanish from the user's perspective.

**How to avoid:**
1. **Use a transactional email service** (SendGrid, Amazon SES, Postmark) as the SMTP relay, not direct VPS-to-MX delivery. These services handle deliverability, reputation, and authentication.
2. **IMAP/POP for receiving, SMTP relay for sending.** The adapter reads incoming mail via IMAP and sends via an authenticated SMTP relay.
3. **Email threading.** Reply to the original message with correct `In-Reply-To` and `References` headers. Without threading, each response appears as a new conversation.
4. **Rate limiting.** Transactional email services have hourly/daily limits. Implement rate limiting that matches the provider's quotas.

**Warning signs:**
- Agent sends emails but users report "I never received it"
- Email service reports high bounce rates or spam complaints
- Response emails appear as new threads instead of replies

**Phase to address:**
Additional Channels phase.

---

### Pitfall 17: OpenAPI Spec Drift from Actual Route Behavior

**What goes wrong:**
Auto-generating OpenAPI specs from route definitions captures the request/response types but misses runtime behavior: authentication requirements, rate limiting, error response formats, streaming behavior. The spec says `200 OK` but the actual response is `200 OK` with an SSE stream. The spec documents query parameters but not the interaction between them (e.g., `stream=true` changes the response content type).

**How to avoid:**
1. **Generate from types, validate with tests.** Use `utoipa` or `aide` to generate the spec from Rust types. Then write integration tests that fetch the spec and validate that every documented endpoint returns the documented response format.
2. **Include error responses.** Document `401`, `403`, `429`, `500` responses with their actual error type format (which uses the v1.4 typed error hierarchy).
3. **Document streaming endpoints separately.** SSE endpoints need `text/event-stream` content type in the spec. Mark them clearly.
4. **Version the spec.** The spec version should track the Blufio version. Breaking changes in the API must bump the spec version.

**Warning signs:**
- API clients generated from the spec fail on actual requests
- Spec shows `application/json` but endpoint returns `text/event-stream`
- Error responses do not match documented schemas

**Phase to address:**
OpenAPI Spec phase.

---

## Technical Debt Patterns

Shortcuts that seem reasonable but create long-term problems.

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| `#[allow(clippy::unwrap_used)]` on every module | Build compiles immediately | Defeats the purpose of the lint; unwrap calls never get fixed | Test modules only, or with `// SAFETY:` comment for proven-safe cases |
| Single-level compaction (keep current L0->summary) | No new code needed | Information loss accumulates; no cold storage; no quality verification | As temporary state while building multi-level system |
| PII regex without context awareness | Simple implementation | False positives on code, URLs, technical content; users disable it | If agent is text-chat only (no code assistance) |
| Audit trail without PII separation | Simpler schema | GDPR erasure breaks hash chain or becomes impossible | Never -- design PII separation from day one |
| In-memory cron state (no SQLite persistence) | Simpler implementation | Missed jobs on restart; no catch-up; operator cannot see schedule | Development/testing only; production must persist |
| Blocking prompt injection check on every message | Maximum security | Adds latency to every message; false positives block legitimate use | Never for blocking; acceptable for logging-only mode |

## Integration Gotchas

Common mistakes when connecting new features to the existing Blufio architecture.

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Hook system + EventBus | Subscribing to broadcast channel (fire-and-forget) for hooks that must execute | Use `subscribe_reliable()` for hooks. Broadcast is for logging/metrics. Hooks require guaranteed delivery. |
| Retention + SQLCipher | Opening new connection for cleanup without encryption key | Always use `open_connection()` factory which reads `BLUFIO_DB_KEY` automatically |
| Hot reload + deny_unknown_fields | New config fields cause reload failures on older binaries | Add `#[serde(default)]` on all new optional sections; consider relaxing deny_unknown_fields for reload path |
| OpenTelemetry + tracing | Adding a new subscriber that conflicts with the existing `tracing` subscriber | Use `tracing_subscriber::registry().with(existing_layer).with(otel_layer)` -- compose layers, do not replace |
| Data export + SQLCipher | Opening read-only connection for export without encryption | Export must use same connection factory with `BLUFIO_DB_KEY` |
| PII redaction + FormatPipeline | Redacting content after formatting (breaks markdown/HTML) | Redact before FormatPipeline processes the content. Redact on raw text, then format. |
| Audit trail + EventBus | Publishing audit events on EventBus (creates audit events for audit events) | Audit writes bypass EventBus entirely. Direct SQLite insert in audit module. |
| GDPR export + memory embeddings | Exporting raw embeddings in data export (meaningless to user) | Export memory content and metadata, not embedding vectors. Embeddings are model artifacts, not user data. |

## Performance Traps

Patterns that work at small scale but fail as usage grows.

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Loading all active memory embeddings for vector search | `get_active_embeddings()` fetches all embeddings on every query | Implement ANN index (HNSW) or cache embeddings in memory with LRU. Current linear scan is O(n) per query. | >5,000 active memories (384-dim * 5000 * 4 bytes = 7.5MB per query) |
| Hash chain verification on every audit read | Full chain walk from genesis to latest entry | Verify only on startup, backup, and on-demand CLI. Cache latest verified hash. | >100,000 audit entries (~1 second per 100K SHA-256 ops) |
| Regex PII scan on every message field | Scan all 4 PII patterns against every string | Scan only user-facing content (messages, memories). Skip internal fields (session_id, metadata JSON). | >50 messages/second (regex compilation is cached but matching scales with content length) |
| Single-writer SQLite with audit + retention + normal ops | Write queue depth grows, latency increases for all operations | Separate audit database file, batch writes, or dedicated connection for cleanup | >20 concurrent sessions with active tool use |
| Cron job evaluation every second with wall clock comparison | CPU wake-ups when idle, unnecessary for hourly/daily jobs | Compute next execution time and sleep until then, rather than polling every second | Negligible CPU cost, but wastes power on battery-powered dev machines |

## Security Mistakes

Domain-specific security issues beyond general web security.

| Mistake | Risk | Prevention |
|---------|------|------------|
| Storing PII in audit trail hash computation | GDPR erasure breaks tamper evidence | Hash only non-PII fields: event_id, timestamp, action_type, actor_id (UUID) |
| Prompt injection defense that blocks the model's own output | Agent cannot respond to meta-questions about its instructions | L1 classifier runs on user INPUT only, not on model output. L4 validates OUTPUT for data exfiltration. |
| HMAC boundary tokens using static secret | Leaked HMAC key lets attacker craft boundary tokens | Rotate HMAC secret per session. Derive from session_id + master secret. |
| PII redaction in logs but not in LLM prompts | LLM provider receives unredacted PII | Redact before assembling the ProviderRequest, not just in log output |
| Data export without authentication | Anyone with API access can export all user data | Data export requires Restricted-level API key scope. Rate limit to 1 export per hour. |
| Hook system executes arbitrary shell commands | Compromised config file = RCE | Hooks run in sandboxed subprocesses with no network access, configurable UID, and resource limits (CPU, memory, time) |
| Audit trail stored in same database as operational data | Compromised application can modify audit entries | Consider write-only audit database or separate file with restricted permissions |

## UX Pitfalls

Common user experience mistakes when adding these features.

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Prompt injection warning on every flagged message | Alert fatigue; user ignores real warnings | Silent logging for LOW-confidence flags. Alert only for HIGH-confidence + sensitive action |
| PII redaction replaces content with [REDACTED] in agent responses | Agent appears to be censoring itself | Redact in logs and exports. In agent responses, use the original content (the user sent it, they know it) |
| GDPR erasure confirmation is "are you sure?" without showing what will be deleted | User does not know what they are erasing | Show a preview: "This will delete: 47 messages, 12 memories, 3 sessions. Audit records will be anonymized." |
| Cron job failures are silent | Operator does not know jobs are failing | Send cron failure notifications via the agent's channels (e.g., Telegram message to operator) |
| Hot reload success is invisible | Operator reloads config but cannot tell if it worked | Log the diff: "Config reloaded: tls.cert changed, agent.model unchanged (7 fields unchanged)" |
| Data classification levels shown in user-facing responses | Users see "CONFIDENTIAL" markers on their own messages | Classification is internal metadata. Never expose to end users. |

## "Looks Done But Isn't" Checklist

Things that appear complete but are missing critical pieces.

- [ ] **Multi-level compaction:** Quality scoring passes on test data but has never been tested on 1000+ message conversations with planted facts -- verify fact retention, not just summary quality
- [ ] **Prompt injection L1:** Pattern classifier deployed but FPR never measured on real conversational corpus -- verify <2% FPR before enabling blocking mode
- [ ] **Audit trail:** Hash chain verifies correctly but PII is embedded in hashed fields -- verify GDPR erasure does not break chain integrity
- [ ] **Retention policy:** Deletes old messages but does not respect foreign key order -- verify no `FOREIGN KEY constraint failed` errors in production logs
- [ ] **Hook system:** Hooks fire correctly in isolation but two hooks have never been tested together -- verify no infinite loops with 3+ hooks installed
- [ ] **Hot reload:** Config file reload works but TLS cert rotation was not tested with active connections -- verify active WebSocket connections survive cert rotation
- [ ] **PII redaction:** Email/phone patterns match correctly on PII test data but were never tested against code blocks -- verify no false positives on 100+ code samples
- [ ] **GDPR export:** Export generates JSON but does not include memories -- verify export includes all user data types (messages, memories, sessions, preferences)
- [ ] **Cron scheduler:** Jobs fire on time in testing but persistence was not tested across restart -- verify jobs resume correctly after `blufio serve` restart
- [ ] **Litestream:** Replication works on test database but encrypted database was not tested -- verify behavior with `BLUFIO_DB_KEY` set (expect failure, document alternative)
- [ ] **OpenTelemetry:** Traces export to collector but span context is not propagated to provider HTTP calls -- verify trace continuity across LLM provider requests
- [ ] **Clippy unwrap enforcement:** Lint added to 5 crates but test modules still panic on `unwrap()` -- verify `#[allow]` annotations are limited to test code and proven-safe constants
- [ ] **Data classification:** System classifies correctly but default level for new data types is Restricted -- verify defaults are sensible (Public/Internal for most data)
- [ ] **BlueBubbles adapter:** Connects and sends but Private API crash recovery was not tested -- verify adapter reconnects after BlueBubbles server restart

## Recovery Strategies

When pitfalls occur despite prevention, how to recover.

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Compaction loses critical facts | MEDIUM | Re-derive summaries from cold storage (L0 raw messages). Reindex memories from raw messages. Requires cold storage to exist. |
| Audit trail has PII in hashed fields | HIGH | Must rebuild entire hash chain with PII-free hashing. Requires migration script that re-hashes every entry. Downtime required. |
| Litestream configured with SQLCipher | LOW | Remove Litestream config. Switch to application-level backup + upload. No data loss (original database is intact). |
| False positive storm from PII regex | LOW | Disable PII patterns via config change (hot reload). Tune thresholds. Re-enable with allowlist. |
| Hook infinite loop | LOW | Kill the process (loop is CPU-bound, not a data corruption issue). Add recursion depth limit. Restart. |
| Retention deletes cascade to audit | HIGH | Restore from backup. Audit entries cannot be regenerated. Requires backup to exist and be recent. |
| Over-classification blocks all operations | LOW | Reset all classifications to default via CLI command. Re-classify selectively. |
| Dual recorder panic at startup | LOW | Remove OpenTelemetry config or switch to tracing-only mode. No data loss. |
| Cron drift causes missed jobs | LOW | Restart with persistence enabled. Missed jobs execute in catch-up mode on next tick. |
| Cold start amnesia from temporal decay | MEDIUM | Temporarily set decay floor to 1.0 (no decay). Gradually lower as new interactions refresh memory scores. |

## Pitfall-to-Phase Mapping

How roadmap phases should address these pitfalls.

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Litestream + SQLCipher incompatibility | Infrastructure / Litestream | Startup check rejects dual config; integration test with BLUFIO_DB_KEY |
| Compaction information loss | Compaction & Context | Planted-fact test: 1000 messages, 20 planted facts, verify all 20 survive full compaction chain |
| GDPR vs audit trail conflict | Audit Trail (design) + GDPR (implementation) | GDPR erasure test: erase user, verify audit chain still validates, verify no PII in audit table |
| Prompt injection false positives | Security Hardening | FPR test: 500 legitimate messages, verify <2% flag rate before enabling blocking |
| OTel + Prometheus recorder conflict | Observability | Startup test: both Prometheus and OTel tracing enabled simultaneously without panic |
| ArcSwap partial config state | Hot Reload | Integration test: reload TLS cert, verify new cert served within 5 seconds |
| Cron timer drift | Cron/Scheduler | Long-running test: 24-hour cron job accuracy within 1 second |
| Memory cold start amnesia | Memory Enhancements | Decay test: simulate 30 days inactivity, verify core memories still retrievable |
| PII regex false positives | PII Redaction | False-positive suite: 200 code samples, 50 URLs, 50 technical strings, verify <1% false positive rate |
| Hook infinite loops | Hook System | Loop test: install 2 hooks that trigger each other, verify depth limit prevents loop |
| Audit write bottleneck | Audit Trail | Load test: 20 concurrent sessions, verify audit does not increase p95 message latency by >50ms |
| Retention cascading deletes | Retention Policy | Foreign key test: delete session with 100 messages, verify correct deletion order, no FK violations |
| Over-classification | Data Classification | Default test: new deployment, verify >80% of data classified as Internal (not Restricted) |
| Clippy unwrap enforcement | Code Quality | CI gate: warn-mode first, track count decreasing across phases, deny-mode last |
| BlueBubbles reliability | Additional Channels | Reconnection test: kill BlueBubbles server, verify adapter reconnects within 60 seconds |
| Email deliverability | Additional Channels | Deliverability test: send to Gmail, verify inbox (not spam) with SPF/DKIM/DMARC configured |
| OpenAPI spec drift | OpenAPI Spec | Spec validation test: auto-generated spec matches actual endpoint behavior for all routes |

## Sources

- [Litestream SQLCipher Issue #177 (wontfix)](https://github.com/benbjohnson/litestream/issues/177) -- confirmed incompatibility
- [Litestream Tips & Caveats](https://litestream.io/tips/) -- WAL management, checkpoint control, data loss window
- [ArcSwap documentation - patterns](https://docs.rs/arc-swap/latest/arc_swap/docs/patterns/index.html) -- Access trait for config projection
- [ArcSwap limitations and pitfalls](https://docs.rs/arc-swap/latest/arc_swap/docs/index.html) -- performance characteristics, read operation selection
- [OpenTelemetry Rust SDK](https://github.com/open-telemetry/opentelemetry-rust) -- opentelemetry-prometheus deprecation notice
- [opentelemetry-prometheus crate deprecation](https://crates.io/crates/opentelemetry-prometheus) -- v0.29 final release, unmaintained protobuf dependency
- [tokio-cron-scheduler](https://github.com/mvniekerk/tokio-cron-scheduler) -- timer drift documentation, no persistence across restarts
- [PromptArmor: Prompt Injection Defenses](https://arxiv.org/html/2507.15219v1) -- <1% FPR/FNR with GPT-4o guardrail
- [OWASP LLM Top 10 2025 - Prompt Injection](https://genai.owasp.org/llmrisk/llm01-prompt-injection/) -- defense-in-depth requirement
- [OWASP Prompt Injection Prevention Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/LLM_Prompt_Injection_Prevention_Cheat_Sheet.html)
- [GDPR Right to Erasure and Backups](https://hallboothsmith.com/we-all-know-about-gdprs-right-to-erasure-does-this-mean-you-have-to-delete-data-from-backups-as-well/) -- backup erasure obligations
- [Right to Be Forgotten vs Audit Trail Mandates](https://axiom.co/blog/the-right-to-be-forgotten-vs-audit-trail-mandates) -- legal tension, practical solutions
- [PII Detection: Why Regex Fails](https://www.protecto.ai/blog/why-regex-fails-pii-detection-in-unstructured-text/) -- false positive analysis
- [The Hidden PII Detection Crisis](https://www.private-ai.com/en/blog/hidden-pii-detection) -- context-aware detection rationale
- [SQLite Foreign Key Support](https://sqlite.org/foreignkeys.html) -- cascade behavior, performance, default-off enforcement
- [Clippy unwrap_used lint discussion](https://github.com/rust-lang/rust-clippy/issues/6636) -- migration strategies for large codebases
- [AuditableLLM: Hash-Chain-Backed Audit Framework](https://www.mdpi.com/2079-9292/15/1/56) -- performance characteristics of hash-chained audit
- [Audit Trail Scaling Strategies](https://www.sachith.co.uk/audit-trails-and-tamper-evidence-scaling-strategies-practical-guide-feb-22-2026/) -- batch writes, Merkle trees
- [BlueBubbles FAQ](https://bluebubbles.app/faq/) -- Private API requirements, macOS dependency
- [BlueBubbles Troubleshooting](https://docs.bluebubbles.app/server/troubleshooting-guides/cant-send-messages-from-bluebubbles) -- AppleScript vs Private API reliability
- [Persistent Memory Design for AI Agents](https://www.marktechpost.com/2025/11/02/how-to-design-a-persistent-memory-and-personalized-agentic-ai-system-with-decay-and-self-evaluation/) -- decay mechanisms, cold start patterns
- [Data Classification Challenges](https://www.sentra.io/learn/5-data-classification-challenges-that-security-teams-face) -- over-classification, context dependency
- Blufio codebase analysis: `crates/blufio-context/src/compaction.rs`, `crates/blufio-security/src/redact.rs`, `crates/blufio-bus/src/events.rs`, `crates/blufio-config/src/model.rs`, `crates/blufio-memory/src/store.rs`, `crates/blufio-storage/src/database.rs`, `crates/blufio-prometheus/src/lib.rs`

---
*Pitfalls research for: Blufio v1.5 PRD Gap Closure*
*Researched: 2026-03-10*
