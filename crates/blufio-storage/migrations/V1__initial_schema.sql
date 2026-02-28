-- Initial database schema for Blufio agent framework.
-- Includes: sessions, messages, queue, vault_entries, vault_meta.

-- Sessions track conversation contexts across channels.
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    channel TEXT NOT NULL,
    user_id TEXT,
    state TEXT NOT NULL DEFAULT 'active',
    metadata TEXT, -- JSON blob
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_sessions_state ON sessions(state);
CREATE INDEX IF NOT EXISTS idx_sessions_channel ON sessions(channel);
CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);

-- Messages within sessions (conversation history).
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL, -- 'user', 'assistant', 'system', 'tool'
    content TEXT NOT NULL,
    token_count INTEGER,
    metadata TEXT, -- JSON blob
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_messages_session_id ON messages(session_id);
CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at);

-- Crash-safe message queue for reliable message processing.
CREATE TABLE IF NOT EXISTS queue (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    queue_name TEXT NOT NULL,
    payload TEXT NOT NULL, -- JSON
    status TEXT NOT NULL DEFAULT 'pending', -- 'pending', 'processing', 'completed', 'failed'
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    locked_until TEXT
);

CREATE INDEX IF NOT EXISTS idx_queue_status ON queue(status);
CREATE INDEX IF NOT EXISTS idx_queue_name_status ON queue(queue_name, status);

-- Encrypted credential vault entries.
CREATE TABLE IF NOT EXISTS vault_entries (
    name TEXT PRIMARY KEY NOT NULL,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Vault metadata (wrapped master key, salt, KDF params).
CREATE TABLE IF NOT EXISTS vault_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value BLOB NOT NULL
);
