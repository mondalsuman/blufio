-- Migration log for tracking imported items from external platforms.
-- Enables idempotent re-runs: UNIQUE(source, item_type, source_id) prevents duplicates.

CREATE TABLE IF NOT EXISTS migration_log (
    id INTEGER PRIMARY KEY,
    source TEXT NOT NULL,
    item_type TEXT NOT NULL,
    source_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'imported',
    metadata TEXT,
    imported_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(source, item_type, source_id)
);

CREATE INDEX idx_migration_log_source ON migration_log(source, item_type);
