-- vec0 virtual table for KNN vector search with metadata filtering.
-- Note: vec0 requires sqlite-vec extension to be registered on the connection.
-- The extension is registered via sqlite3_auto_extension at process startup.
CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec0 USING vec0(
    -- Metadata columns (filterable during KNN query)
    status text,
    classification text,
    -- Partition key (for Phase 67 session-scoped search)
    session_id text partition key,
    -- Vector column: 384-dim float32 with cosine distance
    embedding float[384] distance_metric=cosine,
    -- Auxiliary columns (stored separately, returned at SELECT time)
    +memory_id text,
    +content text,
    +source text,
    +confidence real,
    +created_at text
);
