-- 为 llm_providers 表添加云同步相关字段

ALTER TABLE llm_providers ADD COLUMN cloud_id TEXT;
ALTER TABLE llm_providers ADD COLUMN last_synced_at INTEGER;
ALTER TABLE llm_providers ADD COLUMN sync_enabled INTEGER NOT NULL DEFAULT 1;
