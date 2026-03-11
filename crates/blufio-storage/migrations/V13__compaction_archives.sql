-- Compaction archive storage for multi-level compaction summaries.
-- Archives persist compaction results for recall, GDPR erasure, and quality analysis.

CREATE TABLE IF NOT EXISTS compaction_archives (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    summary TEXT NOT NULL,
    quality_score REAL,
    session_ids TEXT NOT NULL DEFAULT '[]',
    classification TEXT NOT NULL DEFAULT 'internal',
    created_at TEXT NOT NULL,
    token_count INTEGER
);

CREATE INDEX IF NOT EXISTS idx_compaction_archives_user_id ON compaction_archives (user_id);
CREATE INDEX IF NOT EXISTS idx_compaction_archives_created_at ON compaction_archives (created_at);
