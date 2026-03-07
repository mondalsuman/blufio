-- Node system tables: pairings, groups, and approval routing.

CREATE TABLE IF NOT EXISTS node_pairings (
    node_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    public_key_hex TEXT NOT NULL,
    capabilities TEXT NOT NULL DEFAULT '[]',
    paired_at TEXT NOT NULL,
    last_seen TEXT,
    endpoint TEXT
);

CREATE TABLE IF NOT EXISTS node_groups (
    group_name TEXT NOT NULL,
    node_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (group_name, node_id),
    FOREIGN KEY (node_id) REFERENCES node_pairings(node_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS pending_approvals (
    request_id TEXT PRIMARY KEY,
    action_type TEXT NOT NULL,
    description TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    handled_by TEXT,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    resolved_at TEXT
);
