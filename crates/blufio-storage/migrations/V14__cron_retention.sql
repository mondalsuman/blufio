-- V14: Cron scheduler and retention policy tables.

-- Cron job definitions (persisted for last-run tracking and CLI management).
CREATE TABLE IF NOT EXISTS cron_jobs (
    name TEXT PRIMARY KEY NOT NULL,
    schedule TEXT NOT NULL,
    task TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    running INTEGER NOT NULL DEFAULT 0,
    last_run_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Job execution history.
CREATE TABLE IF NOT EXISTS cron_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_name TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    status TEXT NOT NULL DEFAULT 'running',
    duration_ms INTEGER,
    output TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_cron_history_job ON cron_history(job_name);
CREATE INDEX IF NOT EXISTS idx_cron_history_started ON cron_history(started_at);

-- Soft-delete columns for retention policies.
ALTER TABLE messages ADD COLUMN deleted_at TEXT;
ALTER TABLE sessions ADD COLUMN deleted_at TEXT;
ALTER TABLE cost_ledger ADD COLUMN deleted_at TEXT;
ALTER TABLE memories ADD COLUMN deleted_at TEXT;

CREATE INDEX IF NOT EXISTS idx_messages_deleted ON messages(deleted_at);
CREATE INDEX IF NOT EXISTS idx_sessions_deleted ON sessions(deleted_at);
CREATE INDEX IF NOT EXISTS idx_cost_ledger_deleted ON cost_ledger(deleted_at);
CREATE INDEX IF NOT EXISTS idx_memories_deleted ON memories(deleted_at);
