-- vec0 virtual table for KNN vector search with metadata filtering.
-- Note: vec0 requires sqlite-vec extension to be registered on the connection.
-- The extension is registered via sqlite3_auto_extension at process startup.
--
-- Column types:
--   Metadata columns (status, classification): filterable during KNN query (VEC-03)
--   Partition key (session_id): for Phase 67 session-scoped search
--   Vector column (embedding): 384-dim float32 with cosine distance metric
--   Auxiliary columns (+prefixed): stored separately, returned at SELECT time (VEC-08)
--
-- Note: vec0 auxiliary columns use "float" not "real" (sqlite-vec parser limitation).
CREATE VIRTUAL TABLE IF NOT EXISTS memories_vec0 USING vec0(
    status text,
    classification text,
    session_id text partition key,
    embedding float[384] distance_metric=cosine,
    +memory_id text,
    +content text,
    +source text,
    +confidence float,
    +created_at text
);
