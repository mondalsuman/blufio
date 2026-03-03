-- MCP tool hash pins for rug-pull detection (CLNT-07).
CREATE TABLE IF NOT EXISTS mcp_tool_pins (
    server_name TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    pin_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (server_name, tool_name)
);

-- Per-server cost attribution (CLNT-12).
ALTER TABLE cost_ledger ADD COLUMN server_name TEXT;
CREATE INDEX idx_cost_ledger_server ON cost_ledger(server_name);
