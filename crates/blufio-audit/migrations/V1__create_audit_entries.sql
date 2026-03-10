-- Audit trail entries with tamper-evident hash chain.
--
-- Hash chain: each entry's entry_hash = SHA-256(prev_hash|timestamp|event_type|action|resource_type|resource_id).
-- PII fields (actor, session_id, details_json) are excluded from the hash to support GDPR erasure.

CREATE TABLE audit_entries (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_hash   TEXT    NOT NULL,
    prev_hash    TEXT    NOT NULL,
    timestamp    TEXT    NOT NULL,
    event_type   TEXT    NOT NULL,
    action       TEXT    NOT NULL,
    resource_type TEXT   NOT NULL DEFAULT '',
    resource_id  TEXT    NOT NULL DEFAULT '',
    actor        TEXT    NOT NULL DEFAULT '',
    session_id   TEXT    NOT NULL DEFAULT '',
    details_json TEXT    NOT NULL DEFAULT '{}',
    pii_marker   INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_audit_entries_timestamp  ON audit_entries (timestamp);
CREATE INDEX idx_audit_entries_event_type ON audit_entries (event_type);
CREATE INDEX idx_audit_entries_actor      ON audit_entries (actor);
CREATE INDEX idx_audit_entries_pii_marker ON audit_entries (pii_marker);
