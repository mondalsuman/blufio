-- Add cryptographic verification columns to installed_skills.
-- Nullable so existing rows are unaffected (NULL = unsigned/unhashed legacy install).
ALTER TABLE installed_skills ADD COLUMN content_hash TEXT;
ALTER TABLE installed_skills ADD COLUMN signature TEXT;
ALTER TABLE installed_skills ADD COLUMN publisher_id TEXT;

-- Publisher key store for TOFU (trust on first use) model.
-- Stores public keys encountered during skill installation.
CREATE TABLE IF NOT EXISTS publisher_keys (
    publisher_id TEXT PRIMARY KEY,
    public_key_hex TEXT NOT NULL,
    pinned INTEGER NOT NULL DEFAULT 0,
    first_seen TEXT NOT NULL,
    last_used TEXT NOT NULL
);
