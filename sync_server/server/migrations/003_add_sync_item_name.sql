ALTER TABLE sync_data ADD COLUMN name TEXT NOT NULL DEFAULT '';

UPDATE sync_data
SET name = id
WHERE name IS NULL OR name = '';
