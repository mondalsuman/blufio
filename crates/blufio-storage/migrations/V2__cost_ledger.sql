-- Cost ledger table for tracking LLM API call costs.
-- Each row records one provider request with full token breakdown.

CREATE TABLE IF NOT EXISTS cost_ledger (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    model TEXT NOT NULL,
    feature_type TEXT NOT NULL,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    cache_creation_tokens INTEGER NOT NULL DEFAULT 0,
    cost_usd REAL NOT NULL DEFAULT 0.0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_cost_ledger_session ON cost_ledger(session_id);
CREATE INDEX IF NOT EXISTS idx_cost_ledger_created ON cost_ledger(created_at);
CREATE INDEX IF NOT EXISTS idx_cost_ledger_model ON cost_ledger(model);
