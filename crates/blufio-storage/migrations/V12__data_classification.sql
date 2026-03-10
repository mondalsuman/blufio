-- V12: Add data classification columns to memories, messages, and sessions tables.
-- Classification is stored as TEXT with a default of 'internal' (the safest default).
-- Indexes are created on memories and messages for classification-filtered queries.
-- Sessions are less frequently queried by classification, so no index is needed.

ALTER TABLE memories ADD COLUMN classification TEXT NOT NULL DEFAULT 'internal';
ALTER TABLE messages ADD COLUMN classification TEXT NOT NULL DEFAULT 'internal';
ALTER TABLE sessions ADD COLUMN classification TEXT NOT NULL DEFAULT 'internal';

CREATE INDEX IF NOT EXISTS idx_memories_classification ON memories(classification);
CREATE INDEX IF NOT EXISTS idx_messages_classification ON messages(classification);
