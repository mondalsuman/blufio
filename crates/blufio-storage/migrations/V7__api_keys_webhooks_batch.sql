-- Phase 32: Scoped API Keys, Webhooks & Batch
-- Tables for API key management, webhook delivery, and batch processing.

-- API Keys: scoped access tokens with per-key rate limits.
CREATE TABLE api_keys (
    id TEXT PRIMARY KEY,
    key_hash TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    scopes TEXT NOT NULL DEFAULT '[]',
    rate_limit INTEGER NOT NULL DEFAULT 60,
    created_at TEXT NOT NULL,
    expires_at TEXT,
    revoked_at TEXT
);
CREATE INDEX idx_api_keys_hash ON api_keys(key_hash);

-- Webhooks: registered endpoints for async event delivery.
CREATE TABLE webhooks (
    id TEXT PRIMARY KEY,
    url TEXT NOT NULL,
    secret TEXT NOT NULL,
    events TEXT NOT NULL DEFAULT '[]',
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Webhook Dead Letter Queue: failed deliveries stored for replay.
CREATE TABLE webhook_dead_letter (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    webhook_id TEXT NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    last_attempt_at TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_dead_letter_webhook ON webhook_dead_letter(webhook_id);

-- Batches: top-level batch processing records.
CREATE TABLE batches (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL DEFAULT 'processing',
    total_items INTEGER NOT NULL,
    completed_items INTEGER NOT NULL DEFAULT 0,
    failed_items INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    completed_at TEXT,
    api_key_id TEXT
);

-- Batch Items: individual requests within a batch.
CREATE TABLE batch_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    batch_id TEXT NOT NULL REFERENCES batches(id) ON DELETE CASCADE,
    item_index INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    request TEXT NOT NULL,
    response TEXT,
    created_at TEXT NOT NULL,
    completed_at TEXT
);
CREATE INDEX idx_batch_items_batch ON batch_items(batch_id);

-- Rate Limit Counters: sliding window per-key request counts.
CREATE TABLE rate_limit_counters (
    key_id TEXT NOT NULL,
    window_start TEXT NOT NULL,
    count INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (key_id, window_start)
);
