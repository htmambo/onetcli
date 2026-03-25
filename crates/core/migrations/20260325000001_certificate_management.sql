-- 证书统一管理

CREATE TABLE IF NOT EXISTS certificates (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    username TEXT NOT NULL,
    password TEXT,
    key_path TEXT,
    passphrase TEXT,
    remark TEXT,
    sync_enabled INTEGER NOT NULL DEFAULT 1,
    cloud_id TEXT,
    last_synced_at INTEGER,
    team_id TEXT,
    owner_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_certificates_name ON certificates(name);
CREATE INDEX IF NOT EXISTS idx_certificates_kind ON certificates(kind);
CREATE INDEX IF NOT EXISTS idx_certificates_cloud_id ON certificates(cloud_id);
CREATE INDEX IF NOT EXISTS idx_certificates_team_id ON certificates(team_id);
