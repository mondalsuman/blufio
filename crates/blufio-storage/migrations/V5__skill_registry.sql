-- Installed skills registry for the WASM skill sandbox system.
CREATE TABLE IF NOT EXISTS installed_skills (
    name TEXT PRIMARY KEY,
    version TEXT NOT NULL,
    description TEXT NOT NULL,
    author TEXT,
    wasm_path TEXT NOT NULL,
    manifest_toml TEXT NOT NULL,
    capabilities_json TEXT NOT NULL,
    verification_status TEXT NOT NULL DEFAULT 'unverified',
    installed_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
