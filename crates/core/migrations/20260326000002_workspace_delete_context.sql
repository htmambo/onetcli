ALTER TABLE pending_cloud_deletions
ADD COLUMN base_last_synced_at INTEGER;

ALTER TABLE pending_cloud_deletions
ADD COLUMN metadata TEXT;
